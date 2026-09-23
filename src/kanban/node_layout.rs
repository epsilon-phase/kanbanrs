use std::cell::RefCell;
use std::cmp::Ordering;

use std::thread::JoinHandle;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use lazy_static::lazy_static;
use log::debug;

use crate::preferences::PREFERENCES;

use super::application::ApplicationCommand;
use super::force_directed::{force_atlas2, rect_edge_point};
use super::*;

use eframe::egui::epaint::CubicBezierShape;
use eframe::egui::{self, Color32, DragPanButtons, Modal, Pos2, Rect, Scene, Stroke, Vec2};
use filter::KanbanFilter;
use layout::adt::dag::NodeHandle;
use layout::core::format::{ClipHandle, RenderBackend};
use layout::core::geometry::Point;
use layout::core::style::StyleAttr;
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
    /// Move the command's geometry, keeping layout-rs output in the same
    /// origin-normalized frame as the node rectangles.
    fn translate(&mut self, delta: Vec2) {
        match self {
            DrawCommand::Text(pos, _, _) => *pos += delta,
            DrawCommand::Rect(r, _, _, _) => *r = r.translate(delta),
            DrawCommand::Circle(center, _) => *center += delta,
            DrawCommand::Arrow(ao) => {
                for p in &mut ao.path {
                    *p += delta;
                }
            }
            DrawCommand::Line(a, b) => {
                *a += delta;
                *b += delta;
            }
        }
    }

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
}
#[derive(Default)]
struct CommandContainer {
    commands: Vec<DrawCommand>,
}
///The state required for the node layout.
///
/// # Coordinate frame invariant
///
/// Scene content lives in `[0, max]`: `min` is pinned to zero for
/// every layout mode and the painter's clip rect covers
/// `[start, start + max - min]`. Content is drawn at `content + start`
/// only — anything outside `[0, max]` in content space (including
/// decorations that hang off node edges) is clipped away. Both layout
/// engines must normalize before their first frame.
pub struct NodeLayout {
    // A thought on this. This should be extracted into another container and then
    // the interface should be implemented on that instead.
    ///A list of commands required to draw the node layout.
    commands: CommandContainer,
    ///The scene rectangle which controls the Scene layout
    scene_rect: Rect,
    ///The size of the area the scene renders into, recorded each frame
    last_outer: Vec2,
    ///Set until the first layout of this view has been framed on screen
    pending_initial_view: bool,
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
    ///Edge id pairs for the hierarchical layout, used for hover highlighting
    hier_edge_list: Vec<(KanbanId, KanbanId)>,
    ///The instant during which the current drag target was selected
    drag_linger: Option<Instant>,
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
    ///Channel receiver for streaming positions from the FR layout thread
    layout_rx: Option<std::sync::mpsc::Receiver<BTreeMap<KanbanId, Pos2>>>,
    ///Node data kept on the main thread for direct painting during FR layout
    fr_node_data: Vec<(KanbanId, String, StyleAttr, Vec2)>,
    ///Edge list kept on the main thread for direct painting during FR layout
    fr_edge_list: Vec<(KanbanId, KanbanId)>,
    ///Current node bounding boxes for FR layout, updated each time positions arrive
    fr_rects: BTreeMap<KanbanId, Rect>,
    ///Node display data for hierarchical layout, kept for direct rendering
    hier_node_data: Vec<(KanbanId, String, StyleAttr, Vec2)>,
    ///Node bounding boxes for hierarchical layout, populated from layout-rs positions
    hier_rects: BTreeMap<KanbanId, Rect>,
    ///Per-node (target, animated) position pairs for smooth interpolation.
    ///Target is the latest snapshot center; animated tracks egui's interpolated value.
    fr_anim: BTreeMap<KanbanId, (Pos2, Pos2)>,
    ///Graph centroid at the moment the current FR layout thread was spawned.
    fr_layout_start_centroid: Option<Pos2>,
    ///Scene center at the moment the current FR layout thread was spawned.
    fr_layout_start_scene_center: Option<Pos2>,
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
            last_outer: Vec2::ZERO,
            pending_initial_view: true,
            scroll_target: None,
            drag_linger: Option::None,
            layout_handle: Option::None,
            focus: Option::None,
            sense_regions: Vec::new(),
            exclude_completed: false,
            dragged_item: Option::None,
            collapsed: Vec::new(),
            hier_edge_list: Vec::new(),
            frames_in_update: 0,
            use_fr_layout: false,
            fr_stable_positions: BTreeMap::new(),
            scroll_to_center_on_complete: false,
            layout_rx: None,
            fr_node_data: Vec::new(),
            fr_edge_list: Vec::new(),
            fr_rects: BTreeMap::new(),
            hier_node_data: Vec::new(),
            hier_rects: BTreeMap::new(),
            fr_anim: BTreeMap::new(),
            fr_layout_start_centroid: None,
            fr_layout_start_scene_center: None,
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
/// The amount of time that must elapse until a dragged item can be
/// dropped onto the hovered item.
const DRAG_AND_DROP_HYSTERESIS_SECS: f32 = 1.0;
/// The lowest scale the initial view of a graph will use when framing
/// it: fit-to-view is clamped to this so huge graphs open as a readable
/// overview instead of an unreadable speck.
const START_VIEW_MIN_SCALE: f32 = 0.35;
impl NodeLayout {
    fn is_collapsed(&self, document: &KanbanDocument, item: &KanbanItem) -> bool {
        self.collapsed.iter().any(|parent_id| {
            document
                .get_task(*parent_id)
                .map(|parent| item.is_descendant_of(parent, document))
                .unwrap_or(false)
        })
    }
    /// Update the nodelayout's state to match that of the kanban document
    pub fn update(
        &mut self,
        document: &KanbanDocument,
        style: &egui::Style,
        filter: &KanbanFilter,
        sort: &ItemSort,
    ) {
        let max_iterations = PREFERENCES.read().force_max_iteration;
        if !self.use_fr_layout {
            self.min = Pos2::new(f32::INFINITY, f32::INFINITY);
            self.max = Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        }
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
            self.layout_handle = None; // drop any in-progress hierarchical thread
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
                self.fr_node_data.clear();
                self.fr_edge_list.clear();
                self.fr_rects.clear();
                self.fr_anim.clear();
                self.sense_regions.clear();
                return;
            }

            self.fr_node_data = node_data.clone();
            self.fr_edge_list = edge_list.clone();

            let stable = self.fr_stable_positions.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            self.layout_rx = Some(rx);
            // Snapshot initial centroid and scene center so we can track the
            // graph as it moves during convergence without snapping the view.
            if !stable.is_empty() {
                let cx = stable.values().map(|p| p.x).sum::<f32>() / stable.len() as f32;
                let cy = stable.values().map(|p| p.y).sum::<f32>() / stable.len() as f32;
                self.fr_layout_start_centroid = Some(Pos2::new(cx, cy));
            } else {
                self.fr_layout_start_centroid = None;
            }
            self.fr_layout_start_scene_center = Some(self.scene_rect.center());
            #[cfg(not(target_arch = "wasm32"))]
            std::thread::spawn(move || {
                force_atlas2(&node_data, &edge_list, max_iterations, &stable, &tx);
            });
            #[cfg(target_arch = "wasm32")]
            force_atlas2(&node_data, &edge_list, max_iterations, &stable, &tx);
        } else {
            self.layout_rx = None; // drop any unread FR snapshots
            self.hier_edge_list.clear();
            self.hier_node_data = tasks
                .iter()
                .map(|item| {
                    let (wrapped, look, size) = prepare_node_display(item, document, style);
                    (item.id, wrapped, look, size)
                })
                .collect();
            let mut vg = VisualGraph::new(layout::core::base::Orientation::LeftToRight);
            let mut handles: BTreeMap<KanbanId, NodeHandle> = BTreeMap::new();
            let mut arrow = Arrow::simple("");
            arrow.end = LineEndKind::Arrow;
            for (id, wrapped, look, size) in &self.hier_node_data {
                add_item_to_graph(*id, wrapped, look.clone(), *size, &mut vg, &mut handles);
            }
            for id in handles.keys() {
                let i = document.get_task(*id).unwrap();
                let mut tasks: Vec<KanbanId> = i.child_tasks.iter().copied().collect();
                sort.sort_by(&mut tasks, document);
                for c in tasks.iter() {
                    if handles.contains_key(c) {
                        vg.add_edge(arrow.clone(), handles[id], handles[c]);
                        self.hier_edge_list.push((*id, *c));
                    }
                }
            }

            if handles.is_empty() {
                self.hier_rects.clear();
                self.sense_regions.clear();
                return;
            }
            let capacity = self.commands.commands.len();
            #[cfg(not(target_arch = "wasm32"))]
            {
                self.layout_handle = Some(std::thread::spawn(move || {
                    let mut commands = CommandContainer {
                        commands: Vec::with_capacity(capacity),
                    };
                    vg.do_it(false, false, false, &mut commands);
                    LayoutResult::Hierarchical(vg, handles, commands)
                }));
            }
            #[cfg(target_arch = "wasm32")]
            {
                let mut commands = CommandContainer {
                    commands: Vec::with_capacity(capacity),
                };
                vg.do_it(false, false, false, &mut commands);
                self.incorporate_update(LayoutResult::Hierarchical(vg, handles, commands));
                self.reset_scene_rect_for_new_layout();
            }
        }
    }
    fn reset_scene_rect_for_new_layout(&mut self) {
        let min_max_rect = Rect {
            min: self.min,
            max: self.max,
        };
        let content_valid = self.min.is_finite()
            && self.max.is_finite()
            && self.max.x > self.min.x
            && self.max.y > self.min.y;
        if self.pending_initial_view && content_valid && self.last_outer != Vec2::ZERO {
            // First layout shown: frame the whole graph at a readable
            // scale — fit-to-view clamped so nodes stay at least
            // START_VIEW_MIN_SCALE — centered on the graph instead of
            // dumping the camera at the origin of a huge canvas.
            self.pending_initial_view = false;
            self.scroll_to_center_on_complete = false;
            let content = Rect::from_min_max(self.min, self.max);
            let fit =
                (self.last_outer.x / content.width()).min(self.last_outer.y / content.height());
            let target = fit.clamp(START_VIEW_MIN_SCALE, 1.0);
            let scene_size = content.size() * (fit / target);
            self.scene_rect = Rect::from_center_size(content.center(), scene_size);
            return;
        }
        if !self.scene_rect.intersects(min_max_rect) || self.scroll_to_center_on_complete {
            self.scene_rect = Rect {
                min: Pos2::new(0.0, 0.0),
                max: (self.scene_rect.max.to_vec2() - self.scene_rect.min.to_vec2()).to_pos2(),
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

    fn incorporate_update(&mut self, result: LayoutResult) {
        let LayoutResult::Hierarchical(vg, handles, commands) = result;
        self.commands = commands;
        self.sense_regions.clear();
        self.hier_rects.clear();
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
            let rect = Rect::from_min_max(
                Pos2 {
                    x: start_x,
                    y: start_y,
                },
                Pos2 { x: end_x, y: end_y },
            );
            self.sense_regions.push((*task_id, rect));
            self.hier_rects.insert(*task_id, rect);
        }
        // Normalize to the origin: the scene painter's clip rect covers
        // [0, max - min] of the content frame, so raw layout coordinates
        // beyond (max - min) get cut off — e.g. collapsed-subtree pills
        // hanging past a node's right edge. Force-directed mode already
        // pins min to zero for the same reason.
        if self.min.is_finite() {
            let shift = -self.min.to_vec2();
            if shift != Vec2::ZERO {
                for (_, rect) in &mut self.sense_regions {
                    *rect = rect.translate(shift);
                }
                for rect in self.hier_rects.values_mut() {
                    *rect = rect.translate(shift);
                }
                for command in &mut self.commands.commands {
                    command.translate(shift);
                }
                self.min = Pos2::ZERO;
                self.max += shift;
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
        shell_commands: &mut Vec<AppShellCommand>,
        application_commands: &mut Vec<ApplicationCommand>,
    ) -> bool {
        self.collapsed.retain(|id| _document.task_is_extant(*id));
        if self.layout_handle.is_some() {
            if !self.layout_handle.as_ref().unwrap().is_finished() {
                if self.frames_in_update > 40 {
                    Modal::new("layout_modal".into()).show(ui.ctx(), |ui| {
                        ui.horizontal(|ui| {
                            ui.add(egui::Spinner::new());
                            ui.label(egui::RichText::new("Laying out the graph…").strong());
                        });
                        ui.label(
                            egui::RichText::new(
                                "Large documents can take a moment in hierarchical mode.",
                            )
                            .weak()
                            .small(),
                        );
                    });
                    debug!("Displaying modal due to slow update!");
                }
                self.frames_in_update += 1;
                ui.ctx().request_repaint();
            } else {
                if self.frames_in_update > 0 {
                    debug!(target: "node_layout", "{} frames to layout graph", self.frames_in_update);
                }
                self.frames_in_update = 0;
                let handle = self.layout_handle.take().unwrap();
                let result = handle.join().unwrap();
                self.incorporate_update(result);
                self.reset_scene_rect_for_new_layout();
            }
        }

        if self.layout_rx.is_some() {
            let mut latest = None;
            let mut disconnected = false;
            match self.layout_rx.as_ref().unwrap().try_recv() {
                Ok(positions) => latest = Some(positions),
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => disconnected = true,
            }
            if let Some(centers) = latest {
                let min_x = centers.values().map(|p| p.x).fold(f32::INFINITY, f32::min);
                let min_y = centers.values().map(|p| p.y).fold(f32::INFINITY, f32::min);
                // Build rects with center-based normalization (leftmost center → x=50)
                let mut temp: BTreeMap<KanbanId, Rect> = self
                    .fr_node_data
                    .iter()
                    .filter_map(|(id, _, _, size)| {
                        centers.get(id).map(|&c| {
                            let c = Pos2::new(c.x - min_x + 50.0, c.y - min_y + 50.0);
                            (*id, Rect::from_center_size(c, *size))
                        })
                    })
                    .collect();
                // Shift so the minimum left/top *edge* is at least 10px from scene origin,
                // preventing wide nodes from being clipped by the painter's clip rect.
                let edge_min_x = temp.values().map(|r| r.min.x).fold(f32::INFINITY, f32::min);
                let edge_min_y = temp.values().map(|r| r.min.y).fold(f32::INFINITY, f32::min);
                const EDGE_PAD: f32 = 10.0;
                let shift = Vec2::new(
                    (EDGE_PAD - edge_min_x).max(0.0),
                    (EDGE_PAD - edge_min_y).max(0.0),
                );
                if shift != Vec2::ZERO {
                    for r in temp.values_mut() {
                        *r = r.translate(shift);
                    }
                }
                // Update (target, animated) pairs; seed animated from target on first snapshot
                for (id, rect) in &temp {
                    let target = rect.center();
                    let entry = self.fr_anim.entry(*id).or_insert((target, target));
                    entry.0 = target;
                }

                self.sense_regions.clear();
                // Pin min at scene origin so the painter starts at (0,0) and covers [0, max].
                self.min = Pos2::ZERO;
                self.max = Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
                for (id, size) in self
                    .fr_node_data
                    .iter()
                    .map(|(id, _, _, size)| (*id, *size))
                {
                    if let Some(&(target, _)) = self.fr_anim.get(&id) {
                        let rect = Rect::from_center_size(target, size);
                        self.sense_regions.push((id, rect));
                        self.max.x = self.max.x.max(rect.max.x + 50.0);
                        self.max.y = self.max.y.max(rect.max.y + 90.0);
                    }
                }
                self.fr_stable_positions = self
                    .fr_anim
                    .iter()
                    .map(|(id, (target, _))| (*id, *target))
                    .collect();

                // Track the graph centroid: shift scene_rect so the centroid
                // stays at the same visual position as when layout started.
                if let (Some(start_centroid), Some(start_scene_center)) = (
                    self.fr_layout_start_centroid,
                    self.fr_layout_start_scene_center,
                ) {
                    let targets = &self.fr_stable_positions;
                    if !targets.is_empty() {
                        let cx = targets.values().map(|p| p.x).sum::<f32>() / targets.len() as f32;
                        let cy = targets.values().map(|p| p.y).sum::<f32>() / targets.len() as f32;
                        let delta = Pos2::new(cx, cy) - start_centroid;
                        self.scene_rect.set_center(start_scene_center + delta);
                    }
                }
            }
            if disconnected {
                self.layout_rx = None;
                self.fr_layout_start_centroid = None;
                self.fr_layout_start_scene_center = None;
                self.fr_anim.clear();
                self.fr_stable_positions.clear();
                self.reset_scene_rect_for_new_layout();
            } else {
                ui.ctx().request_repaint();
            }
        }

        // Animate FR positions toward targets and rebuild rects
        if !self.fr_anim.is_empty() {
            self.fr_rects = self
                .fr_node_data
                .iter()
                .filter_map(|(id, _, _, size)| {
                    self.fr_anim.get_mut(id).map(|(target, animated)| {
                        let ax = ui.ctx().animate_value_with_time(
                            egui::Id::new(("fr_x", *id)),
                            target.x,
                            0.2,
                        );
                        let ay = ui.ctx().animate_value_with_time(
                            egui::Id::new(("fr_y", *id)),
                            target.y,
                            0.2,
                        );
                        *animated = Pos2::new(ax, ay);
                        (*id, Rect::from_center_size(*animated, *size))
                    })
                })
                .collect();
        }

        let mut needs_update = false;
        ui.horizontal_wrapped(|ui| {
            needs_update |= ui
                .checkbox(&mut self.exclude_completed, "Hide completed")
                .on_hover_text("Hide tasks that are marked completed")
                .changed();
            let old_fr = self.use_fr_layout;
            needs_update |= ui
                .selectable_value(&mut self.use_fr_layout, false, "Hierarchical")
                .on_hover_text("Tree-style layout via layout-rs; slower on large documents")
                .changed();
            needs_update |= ui
                .selectable_value(&mut self.use_fr_layout, true, "Force-directed")
                .on_hover_text("Physics-based layout that streams positions as it settles")
                .changed();
            if !old_fr && self.use_fr_layout {
                self.scroll_to_center_on_complete = true;
            }
            if self.min.is_finite() && self.max.is_finite() && self.max.x > self.min.x {
                if ui
                    .button("Fit")
                    .on_hover_text("Zoom and center the view on the whole graph")
                    .clicked()
                {
                    self.scene_rect = Rect::from_min_max(self.min, self.max).expand(24.0);
                }
            }
            if let Some(focused_id) = self.focus {
                if let Some(task) = _document.get_task(focused_id) {
                    ui.label(egui::RichText::new(format!("Focused: {}", task.name)).strong());
                }
                if ui
                    .button("Clear focus")
                    .on_hover_text("Show every task again")
                    .clicked()
                {
                    self.focus = None;
                    needs_update = true;
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(
                    "Click: open  ·  Middle-click: focus  ·  Right-click: collapse  ·  \
                     Drag: add link  ·  Right-drag: pan  ·  Wheel: zoom",
                )
                .weak()
                .small(),
            );
        });
        if self.sense_regions.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.label(egui::RichText::new("No tasks to show").strong());
                ui.label(
                    egui::RichText::new(
                        "Adjust the filter, or uncheck \u{201c}Hide completed\u{201d}.",
                    )
                    .weak(),
                );
            });
        }
        // Record the viewport the scene will render into, used to frame
        // the first layout at a readable scale.
        self.last_outer = ui.available_rect_before_wrap().size();
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
        // Seed a 1:1 view centered on the graph the first time it is shown,
        // so it opens at a readable scale; the Fit button frames everything.
        if self.scene_rect.size() == Vec2::ZERO
            && self.min.is_finite()
            && self.max.is_finite()
            && self.max.x > self.min.x
            && self.max.y > self.min.y
        {
            let outer = ui.available_rect_before_wrap();
            let content_center = Pos2::new(
                (self.min.x + self.max.x) * 0.5,
                (self.min.y + self.max.y) * 0.5,
            );
            self.scene_rect = Rect::from_center_size(content_center, outer.size());
        }
        // Graph-tool convention: the wheel zooms toward the pointer.
        // Consume the scroll delta first so Scene doesn't pan with it too.
        let outer = ui.available_rect_before_wrap();
        let (wheel, pointer) = ui.input(|i| (i.smooth_scroll_delta.y, i.pointer.latest_pos()));
        if wheel.abs() > f32::EPSILON
            && self.scene_rect.size().x.min(self.scene_rect.size().y) > f32::EPSILON
            && pointer.is_some_and(|p| outer.contains(p))
        {
            let pointer = pointer.unwrap();
            let size = self.scene_rect.size();
            // Replicate Scene's fit transform to find the scene point under
            // the pointer, then rescale the scene rect around it.
            let scale = (outer.width() / size.x).min(outer.height() / size.y);
            let offset = (outer.size() - size * scale) * 0.5;
            let scene_ptr = (pointer - outer.min.to_vec2() - offset) / scale
                + self.scene_rect.min.to_vec2();
            // Zoom out for positive deltas (wheel down), toward the pointer:
            // graph-tool convention.
            let factor = (1.0 / (1.0 + wheel * 0.002)).clamp(0.5, 2.0);
            let new_size = size * factor;
            let fraction = Vec2::new(
                (scene_ptr.x - self.scene_rect.min.x) / size.x,
                (scene_ptr.y - self.scene_rect.min.y) / size.y,
            );
            let new_min = scene_ptr - Vec2::new(fraction.x * new_size.x, fraction.y * new_size.y);
            self.scene_rect = Rect::from_min_size(new_min, new_size);
            ui.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
        }
        Scene::new()
            .drag_pan_buttons(DragPanButtons::SECONDARY | DragPanButtons::MIDDLE)
            .show(ui, &mut self.scene_rect, |ui| {
                if !self.min.is_finite() || !self.max.is_finite() {
                    return;
                }
                let content_size = (self.max.to_vec2() - self.min.to_vec2()).max(Vec2::splat(1.0));
                let (response, paint) = ui.allocate_painter(content_size, egui::Sense::empty());

                let start = response.rect.min;
                let style = ui.style().clone();

                // Nodes under the pointer: the hovered node and the endpoints
                // of its edges glow in the accent color.
                let hovered_node: Option<KanbanId> =
                    ui.ctx().pointer_latest_pos().and_then(|global| {
                        let local = ui
                            .ctx()
                            .layer_transform_from_global(ui.layer_id())
                            .map(|t| t * global)
                            .unwrap_or(global);
                        self.sense_regions
                            .iter()
                            .find(|(_, region)| {
                                offset_rect(*region, start.to_vec2()).contains(local)
                            })
                            .map(|(id, _)| *id)
                    });
                let mut highlighted: BTreeSet<KanbanId> = BTreeSet::new();
                let mut hot_edges: BTreeSet<(KanbanId, KanbanId)> = BTreeSet::new();
                if let Some(target) = hovered_node {
                    highlighted.insert(target);
                    let edges: &[(KanbanId, KanbanId)] = if self.use_fr_layout {
                        &self.fr_edge_list
                    } else {
                        &self.hier_edge_list
                    };
                    for &(src, dst) in edges {
                        if src == target || dst == target {
                            hot_edges.insert((src, dst));
                            highlighted.insert(src);
                            highlighted.insert(dst);
                        }
                    }
                }

                if self.use_fr_layout {
                    // Pass 1: edges
                    for &(src, dst) in &self.fr_edge_list {
                        if let (Some(&sr), Some(&dr)) =
                            (self.fr_rects.get(&src), self.fr_rects.get(&dst))
                        {
                            let sr = offset_rect(sr, start.to_vec2());
                            let dr = offset_rect(dr, start.to_vec2());
                            let p0 = rect_edge_point(sr.center(), dr.center(), sr);
                            let p3 = rect_edge_point(dr.center(), sr.center(), dr);
                            let p1 = p0 + (p3 - p0) * 0.33;
                            let p2 = p0 + (p3 - p0) * 0.67;
                            let mut edge_stroke = style.noninteractive().fg_stroke;
                            if hot_edges.contains(&(src, dst)) {
                                edge_stroke.color = crate::ACCENT;
                                edge_stroke.width = (edge_stroke.width * 1.6).max(2.0);
                            }
                            paint.add(CubicBezierShape::from_points_stroke(
                                [p0, p1, p2, p3],
                                false,
                                Color32::TRANSPARENT,
                                edge_stroke,
                            ));
                            draw_arrowhead(&paint, p3, p2, edge_stroke);
                        }
                    }
                    // Pass 2: node backgrounds, with status dots
                    paint_node_rects(
                        &paint,
                        &style,
                        &self.fr_node_data,
                        &self.fr_rects,
                        start.to_vec2(),
                        _document,
                        &highlighted,
                    );
                    // Pass 3: labels
                    paint_node_labels(
                        &paint,
                        &style,
                        &self.fr_node_data,
                        &self.fr_rects,
                        start.to_vec2(),
                        _document,
                    );
                } else {
                    // Pass 1: edges — only Arrow/Line commands from layout-rs.
                    // Their geometry carries no task ids, so hovered neighbor
                    // nodes glow but their edges cannot be recolored.
                    self.commands
                        .commands
                        .iter()
                        .filter(|c| matches!(c, DrawCommand::Arrow(_) | DrawCommand::Line(_, _)))
                        .for_each(|x| x.operate_on(&paint, &style, response.rect));
                    // Pass 2: node backgrounds, with status dots
                    paint_node_rects(
                        &paint,
                        &style,
                        &self.hier_node_data,
                        &self.hier_rects,
                        start.to_vec2(),
                        _document,
                        &highlighted,
                    );
                    // Pass 3: labels
                    paint_node_labels(
                        &paint,
                        &style,
                        &self.hier_node_data,
                        &self.hier_rects,
                        start.to_vec2(),
                        _document,
                    );
                }
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
                        task_presentation::task_summary(
                            task,
                            _document,
                            ui,
                            &mut task_presentation::TaskPresentationState {
                                hovered_task: &mut nothing,
                                shell_commands,
                                application_commands,
                            },
                            task_presentation::SummaryOptions {
                                start_open: true,
                                idx: 0,
                            },
                        );
                    });
                    if senses.middle_clicked() {
                        self.focus = Some(*task_id);
                        shell_commands.push(AppShellCommand::FocusOn(*task_id));
                    }
                    if senses.clicked() {
                        shell_commands.push(AppShellCommand::OpenEditor(*task_id));
                    }
                    if senses.secondary_clicked() && !senses.dragged() {
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
                    let current = Instant::now();
                    if let Some(dropped) = senses.dnd_hover_payload::<KanbanId>() {
                        let paint = ui.painter();
                        if self.drag_linger.is_none() {
                            self.drag_linger = Some(current);
                            ui.ctx().clear_animations();
                            ui.ctx().animate_value_with_time(
                                egui::Id::new("stroke"),
                                0.0,
                                DRAG_AND_DROP_HYSTERESIS_SECS,
                            );
                            ui.ctx().animate_value_with_time(
                                egui::Id::new("roundness"),
                                0.0,
                                DRAG_AND_DROP_HYSTERESIS_SECS,
                            );
                        }
                        let drag_stroke = ui.ctx().animate_value_with_time(
                            egui::Id::new("stroke"),
                            5.,
                            DRAG_AND_DROP_HYSTERESIS_SECS,
                        );
                        let drag_roundness = ui.ctx().animate_value_with_time(
                            egui::Id::new("roundness"),
                            3.0,
                            DRAG_AND_DROP_HYSTERESIS_SECS,
                        );
                        hovered = true;
                        {
                            let pointer_global = ui.ctx().pointer_latest_pos().unwrap();
                            let pointer_position = ui
                                .ctx()
                                .layer_transform_from_global(ui.layer_id())
                                .map(|t| t * pointer_global)
                                .unwrap_or(pointer_global);
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
                            .is_some_and(|x| x.elapsed().as_secs_f32() > DRAG_AND_DROP_HYSTERESIS_SECS)
                        {
                            let pointer_global = ui.ctx().pointer_latest_pos().unwrap_or_default();
                            let pointer_pos = ui
                                .ctx()
                                .layer_transform_from_global(ui.layer_id())
                                .map(|t| t * pointer_global)
                                .unwrap_or(pointer_global);
                            let local_rect = offset_rect(*region, start.to_vec2());
                            if is_on_left_side(&local_rect, pointer_pos) {
                                application_commands
                                    .push(ApplicationCommand::AddChildTo(*x, *task_id));
                            } else {
                                application_commands
                                    .push(ApplicationCommand::AddChildTo(*task_id, *x));
                            }
                        }
                    }
                }
                if !hovered {
                    self.drag_linger = None;
                }

                // Collapsed-subtree pills: make right-click collapsing visible
                // and offer a click target to expand again.
                let mut expand: Option<KanbanId> = None;
                {
                    let rects = if self.use_fr_layout {
                        &self.fr_rects
                    } else {
                        &self.hier_rects
                    };
                    for collapsed_id in self.collapsed.clone() {
                        let Some(&rect) = rects.get(&collapsed_id) else {
                            continue;
                        };
                        let Some(parent) = _document.get_task(collapsed_id) else {
                            continue;
                        };
                        let hidden = _document
                            .get_tasks()
                            .filter(|x| x.id != collapsed_id && x.is_descendant_of(parent, _document))
                            .count();
                        if hidden == 0 {
                            continue;
                        }
                        let text = format!("+{hidden}");
                        let pill = Rect::from_min_size(
                            Pos2::new(rect.max.x + 4.0, rect.min.y),
                            Vec2::new(18.0 + 7.0 * text.len() as f32, 19.0),
                        );
                        let screen = offset_rect(pill, start.to_vec2());
                        // A real Button rather than raw paint, so the label
                        // and its bounds reach the accessibility tree (the
                        // inspector can then see the pill).
                        let pill_response = ui
                            .scope(|ui| {
                                {
                                    let w = &mut ui.style_mut().visuals.widgets;
                                    w.inactive.bg_fill = crate::ACCENT;
                                    w.inactive.weak_bg_fill = crate::ACCENT;
                                    w.inactive.bg_stroke = egui::Stroke::new(
                                        1.0,
                                        crate::ACCENT.gamma_multiply(0.7),
                                    );
                                    w.inactive.fg_stroke =
                                        egui::Stroke::new(1.0, egui::Color32::WHITE);
                                    w.inactive.corner_radius = egui::CornerRadius::same(8);
                                    w.inactive.expansion = 0.0;
                                    let fill = crate::ACCENT.gamma_multiply(1.2);
                                    w.hovered.bg_fill = fill;
                                    w.hovered.weak_bg_fill = fill;
                                    w.hovered.bg_stroke =
                                        egui::Stroke::new(1.0, egui::Color32::WHITE);
                                    w.hovered.fg_stroke =
                                        egui::Stroke::new(1.0, egui::Color32::WHITE);
                                    w.hovered.corner_radius = egui::CornerRadius::same(8);
                                    w.hovered.expansion = 0.0;
                                }
                                ui.put(
                                    screen,
                                    egui::Button::new(
                                        egui::RichText::new(&text).strong().size(11.0),
                                    ),
                                )
                            })
                            .inner;
                        if pill_response.on_hover_text("Expand hidden tasks").clicked() {
                            expand = Some(collapsed_id);
                        }
                    }
                }
                if let Some(id) = expand {
                    self.collapsed.retain(|x| *x != id);
                    needs_update = true;
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
    let text = item.name.clone();
    let mut look = StyleAttr::simple();
    look.fill_color = None;
    look.line_width = style.noninteractive().bg_stroke.width as usize;

    if let Some(category) = &item.category {
        if let Some(this_style) = document.categories.get(category.as_str()) {
            if let Some(color) = &this_style.panel_stroke_color {
                look.line_color = from_color32(Color32::from_rgba_unmultiplied(
                    color[0], color[1], color[2], color[3],
                ));
            }
            look.fill_color = this_style
                .panel_fill
                .map(|x| from_color32(Color32::from_rgba_unmultiplied(x[0], x[1], x[2], x[3])));
            look.line_width = this_style
                .panel_stroke_width
                .map_or(style.noninteractive().fg_stroke.width as usize, |x| {
                    x as usize
                });
        }
    } else {
        look.line_color = from_color32(style.noninteractive().fg_stroke.color);
    }

    const CHAR_W: f32 = 9.0;
    const LINE_H: f32 = 20.0;
    const PADDING: f32 = 12.0;
    const MIN_SIDE: f32 = 80.0;

    let (wrapped, size) = NAME_BUFFER.with_borrow_mut(|buffer| {
        let n = text.chars().count() as f32;
        let col = ((n * LINE_H / CHAR_W).sqrt() as usize).clamp(6, 40);
        wrap_string(buffer, &text, col);
        let n_lines = buffer.lines().count().max(1) as f32;
        let max_chars = buffer
            .lines()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(col) as f32;
        let w = (max_chars * CHAR_W + PADDING * 2.0).max(MIN_SIDE);
        let h = (n_lines * LINE_H + PADDING * 2.0).max(MIN_SIDE);
        (buffer.clone(), Vec2::new(w, h))
    });

    (wrapped, look, size)
}

/// Node sizing for the force-directed layout, in actual egui pixels.
///
/// Wraps the label text to a column width chosen so the resulting node is
/// roughly square: optimal column = sqrt(N * LINE_H / CHAR_W).
fn wrap_string<'a>(buffer: &'a mut String, s: &str, max_line_length: usize) -> &'a mut String {
    buffer.clear();
    let mut line_size = 0;
    for i in s.chars() {
        if line_size > max_line_length && i.is_whitespace() {
            buffer.push('\n');
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

/// Paint node backgrounds: rounded, category-colored, with a status dot.
/// Nodes in `highlighted` get an accent border instead of their own.
fn paint_node_rects(
    paint: &egui::Painter,
    style: &egui::Style,
    node_data: &[(KanbanId, String, StyleAttr, Vec2)],
    rects: &BTreeMap<KanbanId, Rect>,
    start: Vec2,
    document: &KanbanDocument,
    highlighted: &BTreeSet<KanbanId>,
) {
    for (id, _, look, _) in node_data {
        let Some(&rect) = rects.get(id) else {
            continue;
        };
        let rect = offset_rect(rect, start);
        let mut stroke_color =
            Color32::from_hex(&look.line_color.to_web_color()).unwrap_or(Color32::WHITE);
        let fill = look
            .fill_color
            .map(|c| Color32::from_hex(&c.to_web_color()).unwrap_or(Color32::TRANSPARENT))
            .unwrap_or(style.noninteractive().bg_fill);
        let mut stroke_width = look.line_width as f32;
        if highlighted.contains(id) {
            stroke_color = crate::ACCENT;
            stroke_width = stroke_width.max(2.0);
        }
        paint.rect(
            rect,
            8.0,
            fill,
            egui::Stroke::new(stroke_width, stroke_color),
            egui::StrokeKind::Middle,
        );
        paint.circle_filled(
            Pos2::new(rect.min.x + 9.0, rect.min.y + 9.0),
            4.0,
            task_presentation::status_color(&document.task_status(id)),
        );
    }
}

/// Paint node labels, dimmed for completed tasks.
fn paint_node_labels(
    paint: &egui::Painter,
    style: &egui::Style,
    node_data: &[(KanbanId, String, StyleAttr, Vec2)],
    rects: &BTreeMap<KanbanId, Rect>,
    start: Vec2,
    document: &KanbanDocument,
) {
    for (id, text, _, _) in node_data {
        let Some(&rect) = rects.get(id) else {
            continue;
        };
        let rect = offset_rect(rect, start);
        let color = if document
            .get_task(*id)
            .is_some_and(|task| task.completed.is_some())
        {
            style.visuals.weak_text_color()
        } else {
            style.noninteractive().text_color()
        };
        paint.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId {
                size: 15.0,
                family: egui::FontFamily::Monospace,
            },
            color,
        );
    }
}

fn add_item_to_graph<G>(
    id: KanbanId,
    wrapped: &str,
    look: StyleAttr,
    size: Vec2,
    vg: &mut VisualGraph,
    handles: &mut G,
) where
    G: Extend<(KanbanId, NodeHandle)>,
{
    NAME_BUFFER.with_borrow_mut(|buffer| {
        buffer.clear();
        buffer.push_str(wrapped);
        let shape = ShapeKind::new_box(buffer);
        let node = Element::create(
            shape,
            look,
            layout::core::base::Orientation::LeftToRight,
            layout::core::geometry::Point::new(size.x as f64, size.y as f64),
        );
        let handle = vg.add_node(node);
        handles.extend([(id, handle)].iter().cloned());
    });
}
