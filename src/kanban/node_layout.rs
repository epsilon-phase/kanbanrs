use std::cell::RefCell;
use std::cmp::Ordering;

use std::thread::JoinHandle;
use std::time::Instant;

use lazy_static::lazy_static;

use super::force_directed::{force_atlas2, rect_edge_point};
use super::*;

use eframe::egui::Scene;
use egui::epaint::CubicBezierShape;
use egui::{Modal, Pos2, Rect, Style};
use filter::KanbanFilter;
use layout::adt::dag::NodeHandle;
use layout::core::format::{ClipHandle, RenderBackend};
use layout::core::geometry::Point;
use layout::core::style::StyleAttr;
use layout::std_shapes::render::get_shape_size;
use layout::std_shapes::shapes::{Arrow, Element, LineEndKind, ShapeKind};
use layout::topo::layout::VisualGraph;
use sorting::ItemSort;

///Stores a path between two items in the drawing
#[derive(PartialEq, Clone, Eq)]
struct ArrowOptions {
    path: Vec<Pos2>,
    dashed: bool,
    head: (bool, bool),
    text: String,
}
///A drawing command used to store the results of the layout-rs commands.
///
///This loses a *lot* of information, partly because there's no way to
///store the information required in a way useful to egui
#[derive(Clone, PartialEq)]
enum DrawCommand {
    // There would ideally be a text color here, however I don't think layout-rs has
    // a suitable field for this in the styleattr struct.
    ///A text item to display, consisting of the position, the text
    ///and the text size.
    Text(Pos2, String, f32),
    ///A rectangle to display, consisting of the rectangle coordinates,
    ///the color of the outline, the color of the background, and the
    ///thickness of the thickness of the outline
    Rect(Rect, Color32, Option<Color32>, f32),
    ///An ellipse, bounded by these two points
    Circle(Pos2, Pos2),
    ///An arrow that follows a path
    Arrow(ArrowOptions),
    ///A line from one point to another, not used, I think?
    Line(Pos2, Pos2),
}
/// This is necessary to order the drawing commands so as not to clash with
/// one another. Commands that should be drawn on top must come last
///
/// Text > Rectangle > Circle > Arrow > Line
impl PartialOrd for DrawCommand {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        use DrawCommand::*;
        Some(match (self, &other) {
            (Text(_, _, _), Text(_, _, _)) => Ordering::Equal,
            (_, Text(_, _, _)) => Ordering::Less,
            (Text(_, _, _), _) => Ordering::Greater,
            (Rect(_, _, _, _), Rect(_, _, _, _)) => Ordering::Equal,
            (Rect(_, _, _, _), _) => Ordering::Greater,
            (Circle(_, _), Circle(_, _)) => Ordering::Equal,
            (Circle(_, _), _) => Ordering::Greater,
            (_, Circle(_, _)) => Ordering::Less,
            (_, Rect(_, _, _, _)) => Ordering::Less,
            (Arrow(_), Arrow(_)) => Ordering::Equal,
            (Arrow(_), _) => Ordering::Less,
            (_, Arrow(_)) => Ordering::Greater,
            (Line(_, _), Line(_, _)) => Ordering::Equal,
        })
    }
}

impl DrawCommand {
    pub fn operate_on(&self, paint: &egui::Painter, style: &egui::Style, painting_rectangle: Rect) {
        let offset = Vec2::new(painting_rectangle.min.x, painting_rectangle.min.y);
        match self {
            DrawCommand::Rect(r, color, fill, stroke_width) => {
                let mut r = *r;
                r.min += offset;
                r.max += offset;
                paint.rect(
                    r,
                    0.0,
                    fill.unwrap_or(style.noninteractive().bg_fill),
                    egui::Stroke::new(*stroke_width, *color),
                    egui::StrokeKind::Middle,
                );
            }
            DrawCommand::Text(pos, str, size) => {
                paint.text(
                    *pos + offset,
                    egui::Align2::CENTER_CENTER,
                    str,
                    egui::FontId {
                        size: *size,
                        family: egui::FontFamily::Monospace,
                    },
                    style.noninteractive().text_color(),
                );
            }
            DrawCommand::Line(a, b) => {
                paint.line_segment([*a + offset, *b + offset], style.noninteractive().fg_stroke);
            }
            DrawCommand::Arrow(ao) => {
                let mut points: [Pos2; 4] = Default::default();
                for (index, i) in points.iter_mut().enumerate() {
                    *i = ao.path[index] + offset;
                }
                let shape = CubicBezierShape::from_points_stroke(
                    points,
                    false,
                    Color32::TRANSPARENT,
                    style.noninteractive().fg_stroke,
                );

                paint.add(shape);
                for i in (3..ao.path.len() - 2).step_by(2) {
                    let start = ao.path[i] + offset;
                    let control =
                        ao.path[i] - (ao.path[i - 1].to_vec2() - ao.path[i].to_vec2()) + offset;
                    let exit = ao.path[i + 1] + offset;
                    let end = ao.path[i + 2] + offset;
                    paint.add(CubicBezierShape::from_points_stroke(
                        [start, control, exit, end],
                        false,
                        Color32::TRANSPARENT,
                        style.noninteractive().fg_stroke,
                    ));
                }
                if ao.head.1 {
                    let tip = *ao.path.last().unwrap() + offset;
                    // layout-rs sometimes repeats the endpoint, so find the last distinct point
                    let from = (0..ao.path.len() - 1)
                        .rev()
                        .find(|&i| ao.path[i].distance(*ao.path.last().unwrap()) > 0.5)
                        .map(|i| ao.path[i] + offset);
                    if let Some(from) = from {
                        draw_arrowhead(paint, tip, from, style.noninteractive().fg_stroke);
                    }
                }
                if ao.head.0 {
                    let tip = *ao.path.first().unwrap() + offset;
                    let from = (1..ao.path.len())
                        .find(|&i| ao.path[i].distance(*ao.path.first().unwrap()) > 0.5)
                        .map(|i| ao.path[i] + offset);
                    if let Some(from) = from {
                        draw_arrowhead(paint, tip, from, style.noninteractive().fg_stroke);
                    }
                }
            }
            DrawCommand::Circle(center, size) => {
                paint.circle(
                    *center + offset,
                    size.x,
                    style.visuals.extreme_bg_color,
                    style.noninteractive().fg_stroke,
                );
            }
        }
    }
}
enum LayoutResult {
    Hierarchical(
        VisualGraph,
        BTreeMap<KanbanId, NodeHandle>,
        CommandContainer,
    ),
    ForceDirected(BTreeMap<KanbanId, Rect>, CommandContainer),
}
#[derive(Default)]
struct CommandContainer {
    commands: Vec<DrawCommand>,
}
///The state required for the node layout
pub struct NodeLayout {
    // A thought on this. This should be extracted into another container and then
    // the interface should be implemented on that instead.
    ///A list of commands required to draw the node layout.
    commands: CommandContainer,
    ///The scene rectangle which controls the Scene layout
    scene_rect: Rect,
    ///The center of the rectangle that is being scrolled to
    scroll_target: Option<(Pos2, bool)>,
    ///The minimum coordinate
    min: Pos2,
    ///The maximum coordinate
    max: Pos2,
    ///A container mapping the rectangles displayed to their associated
    ///task ids
    sense_regions: Vec<(KanbanId, Rect)>,
    ///The focused kanban task, which when set, determines that only
    ///related tasks will be displayed
    focus: Option<KanbanId>,
    ///Whether or not completed tasks should be displayed
    exclude_completed: bool,
    ///The id of the dragged item, or `None`
    dragged_item: Option<KanbanId>,
    ///A container of every kanban task which is collapsed
    collapsed: Vec<KanbanId>,
    ///The instant during which the current drag target was selected
    drag_linger: Option<std::time::Instant>,
    ///A thread handle that returns the necessary state to build the update
    ///on the main thread and displayed
    layout_handle: Option<JoinHandle<LayoutResult>>,
    ///The number of frames since the layout thread was spawned, state
    ///used to determine when the waiting modal must be displayed
    frames_in_update: u32,
    ///Whether to use Fruchterman-Reingold force-directed layout instead of hierarchical
    use_fr_layout: bool,
    ///Stable node positions from previous F-R layout iteration, used as warm start
    fr_stable_positions: BTreeMap<KanbanId, Pos2>,
    ///Set to true when switching to F-R so the viewport scrolls to the graph center on completion
    scroll_to_center_on_complete: bool,
}
impl Default for NodeLayout {
    fn default() -> Self {
        Self::new()
    }
}
impl NodeLayout {
    pub fn new() -> Self {
        NodeLayout {
            commands: CommandContainer {
                commands: Vec::new(),
            },
            min: Pos2 { x: 0.0, y: 0.0 },
            max: Pos2::new(0.0, 0.0),

            scene_rect: Rect {
                min: Pos2::new(0.0, 0.0),
                max: Pos2::new(0.0, 0.0),
            },
            scroll_target: None,
            drag_linger: Option::None,
            layout_handle: Option::None,
            focus: Option::None,
            sense_regions: Vec::new(),
            exclude_completed: false,
            dragged_item: Option::None,
            collapsed: Vec::new(),
            frames_in_update: 0,
            use_fr_layout: false,
            fr_stable_positions: BTreeMap::new(),
            scroll_to_center_on_complete: false,
        }
    }
}
fn from_point(value: Point) -> Pos2 {
    Pos2 {
        x: value.x as f32,
        y: value.y as f32,
    }
}
///Move the rectangle by the position specified
///
///* `rect`: The rectangle to move
///* `pos`: The offset to move by
fn offset_rect(rect: Rect, pos: Vec2) -> Rect {
    Rect {
        min: rect.min + pos,
        max: rect.max + pos,
    }
}
/// Convert a Color32 from egui into the representation used by
/// the layoutrs library
fn from_color32(a: Color32) -> layout::core::color::Color {
    let mut result: u32 = 0;
    for i in a.to_srgba_unmultiplied().iter() {
        result = result << 8 | (*i as u32);
    }
    layout::core::color::Color::new(result)
}
/// Returns true if the position specified is on the left side of the
/// rectangle's center
///
/// * `r` - The rectangle to check against
/// * `cursor` - The position to check
fn is_on_left_side(r: &Rect, cursor: Pos2) -> bool {
    let diff = r.max.x - r.min.x;
    cursor.x < r.min.x + diff / 2.0
}
impl RenderBackend for CommandContainer {
    fn draw_rect(
        &mut self,
        xy: Point,
        size: Point,
        look: &StyleAttr,
        _properties: Option<String>,
        clip: Option<ClipHandle>,
    ) {
        if clip.is_some() {
            // This has never triggered in the input I've created.
            warn!(target:"node_layout","Ow, I'm getting clipped and I'm not bothering to react. Layout-rs may not be behaving");
        }
        let start = Pos2 {
            x: xy.x as f32,
            y: xy.y as f32,
        };

        let end = Pos2 {
            x: start.x + size.x as f32,
            y: start.y + size.y as f32,
        };

        self.commands.push(DrawCommand::Rect(
            Rect {
                min: start,
                max: end,
            },
            Color32::from_hex(&look.line_color.to_web_color()).unwrap(),
            look.fill_color
                .map(|fill| Color32::from_hex(&fill.to_web_color()).unwrap()),
            look.line_width as f32,
        ));
    }
    fn draw_line(
        &mut self,
        start: Point,
        end: Point,
        _look: &StyleAttr,
        _properties: Option<String>,
    ) {
        self.commands
            .push(DrawCommand::Line(from_point(start), from_point(end)));
    }
    fn draw_text(&mut self, xy: Point, text: &str, _look: &StyleAttr) {
        self.commands.push(DrawCommand::Text(
            from_point(xy),
            text.to_string(),
            _look.font_size as f32,
        ))
    }
    fn draw_arrow(
        &mut self,
        path: &[(Point, Point)],
        _dashed: bool,
        _head: (bool, bool),
        _look: &StyleAttr,
        _properties: Option<String>,
        _text: &str,
    ) {
        let mut buffer: Vec<Pos2> = Vec::new();
        // I don't feel like getting the SVG curves implemented here lmao
        buffer.push(from_point(path[0].0));
        buffer.push(from_point(path[0].1));
        for i in &path[1..] {
            buffer.push(from_point(i.0));
            buffer.push(from_point(i.1));
        }
        buffer.push(from_point(path.last().unwrap().1));
        self.commands.push(DrawCommand::Arrow(ArrowOptions {
            path: buffer,
            dashed: false,
            head: _head,
            text: "".into(),
        }));
    }
    fn create_clip(&mut self, _xy: Point, _size: Point, _rounded_px: usize) -> ClipHandle {
        0
    }
    fn draw_circle(
        &mut self,
        xy: Point,
        size: Point,
        _look: &StyleAttr,
        _properties: Option<String>,
    ) {
        self.commands
            .push(DrawCommand::Circle(from_point(xy), from_point(size)));
    }
}

impl NodeLayout {}
// It may be a good idea, should performance be an issue,
// to check if this actually saves any cpu time
lazy_static! {
    static ref SCROLL_X_ID: egui::Id = egui::Id::new("scroll-x");
    static ref SCROLL_Y_ID: egui::Id = egui::Id::new("scroll-y");
}
impl NodeLayout {
    fn is_collapsed(&self, document: &KanbanDocument, item: &KanbanItem) -> bool {
        self.collapsed
            .iter()
            .any(|parent_id| item.is_child_of(document.get_task(*parent_id).unwrap(), document))
    }
    /// Update the nodelayout's state to match that of the kanban document
    pub fn update(
        &mut self,
        document: &KanbanDocument,
        style: &egui::Style,
        filter: &KanbanFilter,
        sort: &ItemSort,
    ) {
        self.min = Pos2::new(f32::INFINITY, f32::INFINITY);
        self.max = Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        self.commands.commands.clear();
        let tasks: Vec<&KanbanItem> = if let Some(focused_id) = self.focus {
            document
                .get_tasks()
                .filter(|x| {
                    let is_focused = x.id == focused_id;
                    let relationship = document.get_relation(focused_id, x.id);
                    let is_related = relationship != TaskRelation::Unrelated;
                    let is_completed = x.completed.is_some();
                    let not_collapsed = !self.is_collapsed(document, x);
                    if is_focused {
                        true
                    } else {
                        is_related && !(self.exclude_completed && is_completed) && not_collapsed
                    }
                })
                .collect()
        } else {
            document
                .get_tasks()
                .filter(|x| !(self.exclude_completed && x.completed.is_some()))
                .filter(|x| filter.matches(x, document))
                .filter(|x| !self.is_collapsed(document, x))
                .collect()
        };

        if self.use_fr_layout {
            let node_data: Vec<(KanbanId, String, StyleAttr, Vec2)> = tasks
                .iter()
                .map(|item| {
                    let (wrapped, look, size) = prepare_node_display(item, document, style);
                    (item.id, wrapped, look, size)
                })
                .collect();

            let fr_task_ids: Vec<KanbanId> = node_data.iter().map(|(id, _, _, _)| *id).collect();
            let edge_list = build_edge_list(&fr_task_ids, document, sort);

            if node_data.is_empty() {
                return;
            }

            let stable = self.fr_stable_positions.clone();
            self.layout_handle = Some(std::thread::spawn(move || {
                let mut centers = force_atlas2(&node_data, &edge_list, 150, &stable);
                let min_x = centers.values().map(|p| p.x).fold(f32::INFINITY, f32::min);
                let min_y = centers.values().map(|p| p.y).fold(f32::INFINITY, f32::min);
                for p in centers.values_mut() {
                    p.x = p.x - min_x + 50.0;
                    p.y = p.y - min_y + 50.0;
                }
                let (commands, rects) = build_fr_draw_commands(&centers, &node_data, &edge_list);
                LayoutResult::ForceDirected(rects, commands)
            }));
        } else {
            let mut vg = VisualGraph::new(layout::core::base::Orientation::LeftToRight);
            let mut handles: BTreeMap<KanbanId, NodeHandle> = BTreeMap::new();
            let mut arrow = Arrow::simple("");
            arrow.end = LineEndKind::Arrow;
            tasks
                .iter()
                .for_each(|x| add_item_to_graph(x, document, style, &mut vg, &mut handles));
            for id in handles.keys() {
                let i = document.get_task(*id).unwrap();
                let mut tasks: Vec<KanbanId> = i.child_tasks.iter().copied().collect();
                sort.sort_by(&mut tasks, document);
                for c in tasks.iter() {
                    if handles.contains_key(c) {
                        vg.add_edge(arrow.clone(), handles[id], handles[c]);
                    }
                }
            }

            if handles.is_empty() {
                return;
            }
            let capacity = self.commands.commands.len();
            self.layout_handle = Some(std::thread::spawn(move || {
                let mut commands = CommandContainer {
                    commands: Vec::with_capacity(capacity),
                };
                vg.do_it(false, false, false, &mut commands);
                LayoutResult::Hierarchical(vg, handles, commands)
            }));
        }
    }
    fn incorporate_update(&mut self, result: LayoutResult) {
        match result {
            LayoutResult::Hierarchical(vg, handles, commands) => {
                self.commands = commands;
                self.commands
                    .commands
                    .sort_by(|a, b| a.partial_cmp(b).unwrap());
                self.sense_regions.clear();
                for (task_id, node_handle) in handles.iter() {
                    let element = vg.element(*node_handle);
                    let start_x = element.pos.left(false) as f32;
                    let start_y = element.pos.top(false) as f32;
                    let end_x = element.pos.right(false) as f32;
                    let end_y = element.pos.bottom(false) as f32;
                    self.max.x = self.max.x.max(end_x + 50.);
                    self.max.y = self.max.y.max(end_y + 90.);
                    self.min.x = self.min.x.min(start_x);
                    self.min.y = self.min.y.min(start_y);
                    self.sense_regions.push((
                        *task_id,
                        Rect::from_min_max(
                            Pos2 {
                                x: start_x,
                                y: start_y,
                            },
                            Pos2 { x: end_x, y: end_y },
                        ),
                    ));
                }
            }
            LayoutResult::ForceDirected(rects, commands) => {
                self.fr_stable_positions = rects
                    .iter()
                    .map(|(id, rect)| (*id, rect.center()))
                    .collect();
                self.commands = commands;
                self.commands
                    .commands
                    .sort_by(|a, b| a.partial_cmp(b).unwrap());
                self.sense_regions.clear();
                for (task_id, rect) in &rects {
                    self.max.x = self.max.x.max(rect.max.x + 50.);
                    self.max.y = self.max.y.max(rect.max.y + 90.);
                    self.min.x = self.min.x.min(rect.min.x);
                    self.min.y = self.min.y.min(rect.min.y);
                    self.sense_regions.push((*task_id, *rect));
                }
            }
        }
    }
    pub fn scroll_to(&mut self, id: KanbanId) {
        let target_rect = self.sense_regions.iter().find(|x| x.0 == id);
        if let Some(target_rect) = target_rect {
            self.scroll_target = Some((target_rect.1.center(), true));
        }
    }
    /// Draw the node layout into the ui
    pub fn show(
        &mut self,
        _document: &KanbanDocument,
        ui: &mut egui::Ui,
        actions: &mut Vec<AppCommand>,
    ) -> bool {
        if self.layout_handle.is_some() {
            if !self.layout_handle.as_ref().unwrap().is_finished() {
                if self.frames_in_update > 40 {
                    Modal::new("layout_modal".into()).show(ui.ctx(), |ui| {
                        ui.label("Performing layout!");
                    });
                    debug!("Displaying modal due to slow update!");
                }
                self.frames_in_update += 1;
                ui.ctx().request_repaint();
            } else {
                if self.frames_in_update > 0 {
                    println!("{} frames to layout graph", self.frames_in_update);
                }
                self.frames_in_update = 0;
                let handle = self.layout_handle.take().unwrap();
                let result = handle.join().unwrap();
                self.incorporate_update(result);
                let min_max_rect = Rect {
                    min: self.min,
                    max: self.max,
                };

                if !self.scene_rect.intersects(min_max_rect) || self.scroll_to_center_on_complete {
                    // Reset the scene rectangle to include the start of the
                    // layout.
                    self.scene_rect = Rect {
                        min: Pos2::new(0.0, 0.0),
                        max: (self.scene_rect.max.to_vec2() - self.scene_rect.min.to_vec2())
                            .to_pos2(),
                    };
                }

                if self.scroll_to_center_on_complete {
                    self.scroll_to_center_on_complete = false;
                    let centroid = Pos2::new(
                        (self.min.x + self.max.x) / 2.0,
                        (self.min.y + self.max.y) / 2.0,
                    );
                    if let Some((_, rect)) = self.sense_regions.iter().min_by(|a, b| {
                        a.1.center()
                            .distance(centroid)
                            .partial_cmp(&b.1.center().distance(centroid))
                            .unwrap()
                    }) {
                        self.scroll_target = Some((rect.center(), true));
                    }
                }
            }
        }

        let mut needs_update = false;
        ui.horizontal(|ui| {
            needs_update |= ui
                .checkbox(&mut self.exclude_completed, "Hide completed tasks")
                .changed();
            let old_fr = self.use_fr_layout;
            needs_update |= ui
                .selectable_value(&mut self.use_fr_layout, false, "Hierarchical")
                .changed();
            needs_update |= ui
                .selectable_value(&mut self.use_fr_layout, true, "Force-directed")
                .changed();
            if !old_fr && self.use_fr_layout {
                self.scroll_to_center_on_complete = true;
            }
            if self.focus.is_some() && ui.button("Clear focus").clicked() {
                self.focus = None;
                needs_update = true;
            }
        });
        let scene_center = self.scene_rect.center();
        if let Some((target, first_frame)) = &mut self.scroll_target {
            const SCROLL_ANIMATION_TIME: f32 = 1.5;
            let center = if *first_frame {
                Pos2::new(
                    ui.ctx().animate_value_with_time(
                        *SCROLL_X_ID,
                        scene_center.x,
                        SCROLL_ANIMATION_TIME,
                    ),
                    ui.ctx().animate_value_with_time(
                        *SCROLL_Y_ID,
                        scene_center.y,
                        SCROLL_ANIMATION_TIME,
                    ),
                )
            } else {
                Pos2::new(
                    ui.ctx()
                        .animate_value_with_time(*SCROLL_X_ID, target.x, SCROLL_ANIMATION_TIME),
                    ui.ctx()
                        .animate_value_with_time(*SCROLL_Y_ID, target.y, SCROLL_ANIMATION_TIME),
                )
            };
            *first_frame = false;
            self.scene_rect.set_center(center);
        }
        if self
            .scroll_target
            .is_some_and(|(x, _y)| x == self.scene_rect.center())
        {
            self.scroll_target = None;
        }
        Scene::new().show(ui, &mut self.scene_rect, |ui| {
            if !self.min.is_finite() || !self.max.is_finite() {
                return;
            }
            let (response, paint) = ui.allocate_painter(
                self.max.to_vec2() - self.min.to_vec2(),
                egui::Sense::empty(),
            );

            let start = response.rect.min;

            self.commands
                .commands
                .iter()
                .for_each(|x| x.operate_on(&paint, ui.style(), response.rect));
            let mut hovered = false;
            for (task_id, region) in self.sense_regions.iter() {
                let senses = ui.allocate_rect(
                    offset_rect(*region, start.to_vec2()),
                    egui::Sense::click_and_drag(),
                );
                senses.dnd_set_drag_payload(*task_id);
                let senses = senses.on_hover_ui(|ui| {
                    let task = _document.get_task(*task_id).unwrap();
                    let mut nothing: Option<KanbanId> = None;
                    if let Some(cmd) = task.summary(_document, &mut nothing, ui, true, 0) {
                        actions.push(cmd);
                    }
                });
                if senses.middle_clicked() {
                    self.focus = Some(*task_id);
                    actions.push(AppCommand::FocusOn(*task_id));
                }
                if senses.clicked() {
                    actions.push(AppCommand::OpenEditor(*task_id));
                }
                if senses.secondary_clicked() {
                    if let Some(index) = self.collapsed.iter().position(|x| *x == *task_id) {
                        self.collapsed.remove(index);
                    } else {
                        self.collapsed.push(*task_id);
                    }
                    needs_update = true;
                }
                if senses.drag_started() {
                    self.dragged_item = Some(*task_id);
                }
                if senses.drag_stopped() {
                    self.dragged_item = None;
                }
                /// The amount of time that must elapse until the dragged item can be dropped onto
                /// the hovered item
                const DRAG_AND_DROP_HYSTERISIS_SECS: f32 = 1.0;
                let current = Instant::now();
                if let Some(dropped) = senses.dnd_hover_payload::<KanbanId>() {
                    let paint = ui.painter();
                    if self.drag_linger.is_none() {
                        self.drag_linger = Some(current);
                        ui.ctx().clear_animations();
                        ui.ctx().animate_value_with_time(
                            egui::Id::new("stroke"),
                            0.0,
                            DRAG_AND_DROP_HYSTERISIS_SECS,
                        );
                        ui.ctx().animate_value_with_time(
                            egui::Id::new("roundness"),
                            0.0,
                            DRAG_AND_DROP_HYSTERISIS_SECS,
                        );
                    }
                    let drag_stroke = ui.ctx().animate_value_with_time(
                        egui::Id::new("stroke"),
                        5.,
                        DRAG_AND_DROP_HYSTERISIS_SECS,
                    );
                    let drag_roundness = ui.ctx().animate_value_with_time(
                        egui::Id::new("roundness"),
                        3.0,
                        DRAG_AND_DROP_HYSTERISIS_SECS,
                    );
                    hovered = true;
                    {
                        let pointer_position = ui.ctx().pointer_latest_pos().unwrap();
                        let rect = offset_rect(*region, start.to_vec2());

                        let sign = if is_on_left_side(&rect, pointer_position) {
                            -1.
                        } else {
                            1.
                        };
                        let clip = rect
                            .shrink2(Vec2 {
                                x: rect.width() / 4.,
                                y: 0.,
                            })
                            .translate(Vec2::new(sign * rect.width() / 4.0, 0.))
                            .expand2(Vec2::new(0.0, 5.));

                        let paint = paint.with_clip_rect(clip);
                        ui.ctx().set_cursor_icon(
                            if _document.can_add_as_child(
                                _document.get_task(*dropped).unwrap(),
                                _document.get_task(*task_id).unwrap(),
                            ) {
                                paint.rect_stroke(
                                    offset_rect(*region, start.to_vec2()),
                                    drag_roundness,
                                    Stroke::new(drag_stroke, Color32::from_rgb(0, 255, 0)),
                                    egui::StrokeKind::Middle,
                                );
                                egui::CursorIcon::PointingHand
                            } else {
                                paint.rect_stroke(
                                    offset_rect(*region, start.to_vec2()),
                                    drag_roundness,
                                    Stroke::new(drag_stroke, Color32::from_rgb(255, 0, 0)),
                                    egui::StrokeKind::Middle,
                                );
                                egui::CursorIcon::NoDrop
                            },
                        );
                    }
                }
                if let Some(x) = senses.dnd_release_payload::<i32>().clone() {
                    if _document.can_add_as_child(
                        _document.get_task(*x).unwrap(),
                        _document.get_task(*task_id).unwrap(),
                    ) && self
                        .drag_linger
                        .is_some_and(|x| x.elapsed().as_secs_f32() > 1.0)
                    {
                        if is_on_left_side(region, start) {
                            actions.push(AppCommand::AddChildTo(*x, *task_id));
                        } else {
                            actions.push(AppCommand::AddChildTo(*task_id, *x));
                        }
                    }
                }
            }
            if !hovered {
                self.drag_linger = None;
            }
        });
        needs_update
    }
    pub fn set_focus(&mut self, id: &KanbanId) {
        self.focus = Some(*id);
    }
}

/// Wrap a string to a specified length, into a buffer.
///
/// * **buffer** The buffer to write to
/// * **s** The string to wrap
/// * **max_line_length** The number of characters(I think grapheme is the t)
///
/// Returns a mutable reference to the buffer
fn build_edge_list(
    task_ids: &[KanbanId],
    document: &KanbanDocument,
    sort: &ItemSort,
) -> Vec<(KanbanId, KanbanId)> {
    let mut edges = Vec::new();
    let id_set: std::collections::HashSet<_> = task_ids.iter().copied().collect();
    for &id in task_ids {
        let item = document.get_task(id).unwrap();
        let mut children: Vec<KanbanId> = item.child_tasks.iter().copied().collect();
        sort.sort_by(&mut children, document);
        for child_id in children {
            if id_set.contains(&child_id) {
                edges.push((id, child_id));
            }
        }
    }
    edges
}

fn prepare_node_display(
    item: &KanbanItem,
    document: &KanbanDocument,
    style: &egui::Style,
) -> (String, StyleAttr, Vec2) {
    let mut text = item.name.clone();
    let mut look = StyleAttr::simple();
    look.fill_color = None;
    look.line_width = style.noninteractive().bg_stroke.width as usize;

    if let Some(category) = &item.category {
        if let Some(this_style) = document.categories.get(category) {
            if let Some(color) = &this_style.panel_stroke_color {
                look.line_color = from_color32(Color32::from_rgba_unmultiplied(
                    color[0], color[1], color[2], color[3],
                ));
            }
            look.fill_color = this_style.panel_fill.map(|x| {
                from_color32(Color32::from_rgba_unmultiplied(x[0], x[1], x[2], x[3]))
            });
            look.line_width = this_style
                .panel_stroke_width
                .map_or(style.noninteractive().fg_stroke.width as usize, |x| {
                    x as usize
                });
        }
    } else {
        look.line_color = from_color32(style.noninteractive().fg_stroke.color);
    }

    if item.completed.is_some() {
        text += " (Completed)";
    }

    let (wrapped, size) = NAME_BUFFER.with_borrow_mut(|buffer| {
        wrap_string(
            buffer,
            &text,
            crate::preferences::PREFERENCES.read().node_width,
        );
        let shape = ShapeKind::new_box(buffer);
        let mut sz = get_shape_size(
            layout::core::base::Orientation::LeftToRight,
            &shape,
            15,
            false,
        );
        sz.x *= 0.7;
        (buffer.clone(), Vec2::new(sz.x as f32, sz.y as f32))
    });

    (wrapped, look, size)
}

fn wrap_string<'a>(buffer: &'a mut String, s: &str, max_line_length: usize) -> &'a mut String {
    buffer.clear();
    let mut line_size = 0;
    for i in s.chars() {
        if line_size > max_line_length && i.is_whitespace() {
            #[cfg(unix)]
            buffer.push('\n');
            // I don't know if this is necessary but I doubt it will hurt
            #[cfg(windows)]
            buffer.push_str("\r\n");
            line_size = 0;
        } else {
            buffer.push(i);
            line_size += 1;
        }
    }
    buffer
}
thread_local! {
    ///A buffer used to wrap the nodes without reallocating memory constantly.
    static  NAME_BUFFER:RefCell<String>=const{RefCell::new(String::new())};
}

fn draw_arrowhead(paint: &egui::Painter, tip: Pos2, from: Pos2, stroke: egui::Stroke) {
    let dir = (tip - from).normalized();
    let perp = Vec2::new(-dir.y, dir.x);
    let length = stroke.width * 3.0 + 8.0;
    let width = stroke.width * 1.5 + 4.0;
    let base = tip - dir * length;
    paint.add(egui::Shape::convex_polygon(
        vec![tip, base + perp * width, base - perp * width],
        stroke.color,
        egui::Stroke::NONE,
    ));
}

fn build_fr_draw_commands(
    centers: &BTreeMap<KanbanId, Pos2>,
    node_data: &[(KanbanId, String, StyleAttr, Vec2)],
    edges: &[(KanbanId, KanbanId)],
) -> (CommandContainer, BTreeMap<KanbanId, Rect>) {
    let rects: BTreeMap<KanbanId, Rect> = node_data
        .iter()
        .filter_map(|(id, _, _, size)| {
            centers
                .get(id)
                .map(|&center| (*id, Rect::from_center_size(center, *size)))
        })
        .collect();

    let mut commands = CommandContainer {
        commands: Vec::new(),
    };

    for &(src, dst) in edges {
        if let (Some(src_rect), Some(dst_rect)) = (rects.get(&src), rects.get(&dst)) {
            let start = rect_edge_point(src_rect.center(), dst_rect.center(), *src_rect);
            let end = rect_edge_point(dst_rect.center(), src_rect.center(), *dst_rect);
            let ctrl1 = start + (end.to_vec2() - start.to_vec2()) * 0.33;
            let ctrl2 = start + (end.to_vec2() - start.to_vec2()) * 0.67;
            commands.commands.push(DrawCommand::Arrow(ArrowOptions {
                path: vec![start, ctrl1, ctrl2, end],
                dashed: false,
                head: (false, true),
                text: String::new(),
            }));
        }
    }

    for (id, text, look, _size) in node_data {
        if let Some(&rect) = rects.get(id) {
            let stroke_color =
                Color32::from_hex(&look.line_color.to_web_color()).unwrap_or(Color32::WHITE);
            let fill = look
                .fill_color
                .map(|c| Color32::from_hex(&c.to_web_color()).unwrap_or(Color32::TRANSPARENT));
            commands.commands.push(DrawCommand::Rect(
                rect,
                stroke_color,
                fill,
                look.line_width as f32,
            ));
            commands
                .commands
                .push(DrawCommand::Text(rect.center(), text.clone(), 15.0));
        }
    }

    (commands, rects)
}

/// Add an item to the layout-rs graph, mostly a convenience function
/// as it's kinda heavy to do inline
fn add_item_to_graph<G>(
    i: &KanbanItem,
    document: &KanbanDocument,
    style: &Style,
    vg: &mut VisualGraph,
    handles: &mut G, //&mut HashMap<i32, NodeHandle>,
) where
    G: Extend<(KanbanId, NodeHandle)>,
{
    let id = i.id;
    let (_wrapped, look, size) = prepare_node_display(i, document, style);
    NAME_BUFFER.with_borrow_mut(|buffer| {
        wrap_string(
            buffer,
            &i.name,
            crate::preferences::PREFERENCES.read().node_width,
        );
        let shape = ShapeKind::new_box(buffer);
        let node = Element::create(
            shape,
            look.clone(),
            layout::core::base::Orientation::LeftToRight,
            layout::core::geometry::Point::new(size.x as f64, size.y as f64),
        );
        let handle = vg.add_node(node);
        handles.extend([(id, handle)].iter().cloned());
    });
}
