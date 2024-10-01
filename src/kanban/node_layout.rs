use super::*;

use egui::{Pos2, Rect, Style};
use layout::adt::dag::NodeHandle;
use layout::core::format::{ClipHandle, RenderBackend};
use layout::core::geometry::Point;
use layout::core::style::StyleAttr;
use layout::std_shapes::render::get_shape_size;
use layout::std_shapes::shapes::{Arrow, Element, LineEndKind, ShapeKind};
use layout::topo::layout::VisualGraph;

#[derive(PartialEq, Clone)]
struct ArrowOptions {
    path: Vec<Pos2>,
    dashed: bool,
    head: (bool, bool),
    text: String,
}

#[derive(Clone, PartialEq)]
enum DrawCommand {
    Rect(Rect, Color32, Option<Color32>),
    Line(Pos2, Pos2),
    Text(Pos2, String, f32),
    Arrow(ArrowOptions),
    Circle(Pos2, Pos2),
}
impl DrawCommand {
    pub fn operate_on(&self, paint: &egui::Painter, style: &egui::Style, painting_rectangle: Rect) {
        let offset = Vec2::new(painting_rectangle.min.x, painting_rectangle.min.y);
        match self {
            DrawCommand::Rect(r, color, fill) => {
                if fill.is_some() {
                    println!("Fill Color: {}", fill.unwrap().to_hex());
                }
                let mut r = *r;
                r.min += offset;
                r.max += offset;
                paint.rect(
                    r,
                    0.0,
                    fill.unwrap_or(style.noninteractive().bg_fill),
                    egui::Stroke::new(style.noninteractive().bg_stroke.width, *color),
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
                for i in 1..ao.path.len() {
                    let _prev = ao.path[i - 1] + offset;
                    let _current = ao.path[i] + offset;
                    paint.line_segment(
                        [ao.path[i - 1] + offset, ao.path[i] + offset],
                        style.noninteractive().fg_stroke,
                    );
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
#[derive(Clone, Default)]
pub struct NodeLayout {
    commands: Vec<DrawCommand>,
    min: Pos2,
    max: Pos2,
    sense_regions: Vec<(KanbanId, Rect)>,
    focus: Option<KanbanId>,
    exclude_completed: bool,
}
impl NodeLayout {
    pub fn new() -> Self {
        NodeLayout {
            commands: Vec::new(),
            min: Pos2 { x: 0.0, y: 0.0 },
            max: Pos2::new(0.0, 0.0),
            sense_regions: Vec::new(),
            ..Default::default()
        }
    }
}
fn from_point(value: Point) -> Pos2 {
    Pos2 {
        x: value.x as f32,
        y: value.y as f32,
    }
}
fn offset_rect(rect: Rect, pos: Vec2) -> Rect {
    Rect {
        min: rect.min + pos,
        max: rect.max + pos,
    }
}
fn from_color32(a: Color32) -> layout::core::color::Color {
    let mut result: u32 = 0;
    for i in a.to_srgba_unmultiplied().iter() {
        result = result << 8 | (*i as u32);
    }
    layout::core::color::Color::new(result)
}
impl RenderBackend for NodeLayout {
    fn draw_rect(&mut self, xy: Point, size: Point, look: &StyleAttr, clip: Option<ClipHandle>) {
        if clip.is_some() {
            println!("Ow");
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
        ));
    }
    fn draw_line(&mut self, start: Point, end: Point, _look: &StyleAttr) {
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
            head: (false, false),
            text: "".into(),
        }));
    }
    fn create_clip(&mut self, _xy: Point, _size: Point, _rounded_px: usize) -> ClipHandle {
        0
    }
    fn draw_circle(&mut self, xy: Point, size: Point, _look: &StyleAttr) {
        self.commands
            .push(DrawCommand::Circle(from_point(xy), from_point(size)));
    }
}
impl NodeLayout {
    pub fn update(&mut self, document: &KanbanDocument, style: &egui::Style) {
        self.min = Pos2::new(f32::INFINITY, f32::INFINITY);
        self.max = Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        self.commands.clear();
        let mut vg = VisualGraph::new(layout::core::base::Orientation::LeftToRight);

        let mut handles: HashMap<KanbanId, NodeHandle> = HashMap::new();
        let mut arrow = Arrow::simple("");
        arrow.end = LineEndKind::None;
        if let Some(focused_id) = self.focus {
            for i in document.get_tasks().filter(|x| {
                let is_focused = x.id == focused_id;
                let is_related = document.get_relation(focused_id, x.id) != TaskRelation::Unrelated;
                let is_completed = x.completed.is_some();

                if is_focused {
                    true
                } else {
                    is_related && !(self.exclude_completed && is_completed)
                }
            }) {
                add_item_to_graph(i, document, style, &mut vg, &mut handles);
            }
            // document.on_tree(focused_id, 0, |document, id, _depth| {
            //     if self.exclude_completed && document.get_task(id).unwrap().completed.is_some() {
            //         return;
            //     }
            //     if handles.contains_key(&id) {
            //         return;
            //     }
            //     add_item_to_graph(
            //         document.get_task(id).unwrap(),
            //         document,
            //         style,
            //         &mut vg,
            //         &mut handles,
            //     );
            // });
        } else {
            for i in document.get_tasks() {
                if self.exclude_completed && i.completed.is_some() {
                    continue;
                }
                add_item_to_graph(i, document, style, &mut vg, &mut handles);
            }
        }
        for id in handles.keys() {
            let i = document.get_task(*id).unwrap();
            for c in i.child_tasks.iter() {
                if handles.contains_key(c) {
                    vg.add_edge(arrow.clone(), handles[id], handles[c]);
                }
            }
        }
        if handles.is_empty() {
            return;
        }
        vg.do_it(false, false, false, self);
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
    pub fn show(
        &mut self,
        _document: &KanbanDocument,
        ui: &mut egui::Ui,
        actions: &mut Vec<SummaryAction>,
    ) -> bool {
        let mut needs_update = false;
        ui.horizontal(|ui| {
            needs_update |= ui
                .checkbox(&mut self.exclude_completed, "Hide completed tasks")
                .changed();
            if self.focus.is_some() && ui.button("Clear focus").clicked() {
                self.focus = None;
                needs_update = true;
            }
        });
        ScrollArea::both().id_salt("NodeLayout").show(ui, |ui| {
            if !self.min.is_finite() || !self.max.is_finite() {
                return;
            }
            let (response, paint) = ui.allocate_painter(
                self.max.to_vec2() - self.min.to_vec2(),
                egui::Sense {
                    click: false,
                    drag: false,
                    focusable: false,
                },
            );
            let start = response.rect.min;

            self.commands
                .iter()
                .for_each(|x| x.operate_on(&paint, ui.style(), response.rect));
            for (task_id, region) in self.sense_regions.iter() {
                let task = _document.get_task(*task_id).unwrap();
                let mut thing: Option<KanbanId> = None;
                let senses = ui
                    .allocate_rect(
                        offset_rect(*region, start.to_vec2()),
                        egui::Sense {
                            click: true,
                            drag: false,
                            focusable: false,
                        },
                    )
                    .on_hover_ui(|ui| {
                        actions.push(task.summary(_document, &mut thing, ui));
                    });
                if senses.middle_clicked() {
                    self.focus = Some(*task_id);
                    actions.push(SummaryAction::FocusOn(*task_id));
                }
                if senses.clicked() {
                    actions.push(SummaryAction::OpenEditor(*task_id));
                }
            }
        });
        needs_update
    }
}

fn add_item_to_graph(
    i: &KanbanItem,
    document: &KanbanDocument,
    style: &Style,
    vg: &mut VisualGraph,
    handles: &mut HashMap<i32, NodeHandle>,
) {
    let id = i.id;
    let shape = ShapeKind::new_box(&i.name);
    let mut look0 = StyleAttr::simple();
    look0.fill_color = None;
    if let Some(category) = &i.category {
        if let Some(style) = document.categories.get(category) {
            if let Some(color) = &style.panel_stroke_color {
                look0.line_color = from_color32(Color32::from_rgba_unmultiplied(
                    color[0], color[1], color[2], color[3],
                ));
            }
            look0.fill_color = style
                .panel_fill
                .map(|x| from_color32(Color32::from_rgba_unmultiplied(x[0], x[1], x[2], x[3])));
        }
    } else if i.completed.is_some() {
        look0.line_color = layout::core::color::Color::from_name("green").unwrap();
    } else {
        look0.line_color = from_color32(style.noninteractive().fg_stroke.color);
    }
    let sz = get_shape_size(
        layout::core::base::Orientation::LeftToRight,
        &shape,
        15,
        false,
    );
    let node = Element::create(
        shape,
        look0.clone(),
        layout::core::base::Orientation::LeftToRight,
        sz,
    );
    let handle = vg.add_node(node);
    handles.insert(id, handle);
}
