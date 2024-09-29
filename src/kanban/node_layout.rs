use super::*;

use egui::epaint::CubicBezierShape;
use egui::{Align2, Context, Pos2, Rect, Style};
use layout::adt::dag::NodeHandle;
use layout::core::format::{ClipHandle, RenderBackend};
use layout::core::geometry::Point;
use layout::core::style::StyleAttr;
use layout::std_shapes::render::get_shape_size;
use layout::std_shapes::shapes::{Arrow, Element, LineEndKind, ShapeKind};
use layout::topo::layout::VisualGraph;

struct NodeLayoutCache {
    rectangles: Vec<egui::Pos2>,
}
#[derive(PartialEq, Clone)]
struct ArrowOptions {
    path: Vec<Pos2>,
    dashed: bool,
    head: (bool, bool),
    text: String,
}

#[derive(Clone, PartialEq)]
enum DrawCommand {
    Rect(Rect),
    Line(Pos2, Pos2),
    Text(Pos2, String, f32),
    Arrow(ArrowOptions),
    Circle(Pos2, Pos2),
}
impl DrawCommand {
    pub fn operate_on(&self, paint: &egui::Painter, style: &egui::Style, painting_rectangle: Rect) {
        let offset = Vec2::new(painting_rectangle.min.x, painting_rectangle.min.y);
        match self {
            DrawCommand::Rect(r) => {
                let mut r = *r;
                r.min += offset;
                r.max += offset;
                paint.rect(
                    r,
                    0.0,
                    style.noninteractive().bg_fill,
                    style.noninteractive().fg_stroke,
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
                    let prev = ao.path[i - 1] + offset;
                    let current = ao.path[i] + offset;
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
    // pub fn bounding_box(&self) -> [Pos2; 2] {
    //     match self {
    //         DrawCommand::Arrow(ao) => {
    //             let mut min = ao.path[0];
    //         }
    //         DrawCommand::Line(a, b) => {}
    //         DrawCommand::Text(pos, str, size) => {}
    //         DrawCommand::Circle(a, b) => {}
    //     }
    // }
}
#[derive(Clone, Default)]
pub struct NodeLayout {
    commands: Vec<DrawCommand>,
    min: Pos2,
    max: Pos2,
}
impl NodeLayout {
    pub fn new() -> Self {
        NodeLayout {
            commands: Vec::new(),
            min: Pos2 { x: 0.0, y: 0.0 },
            max: Pos2::new(0.0, 0.0),
        }
    }
}
fn from_point(value: Point) -> Pos2 {
    Pos2 {
        x: value.x as f32,
        y: value.y as f32,
    }
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
        if self.min.x > start.x {
            self.min.x = start.x;
        }
        if self.min.y > start.y {
            self.min.y = start.y;
        }

        let end = Pos2 {
            x: start.x + size.x as f32,
            y: start.y + size.y as f32,
        };
        if self.max.y < end.y {
            self.max.y = end.y;
        }
        if self.max.x < end.x {
            self.max.x = end.x;
        }

        self.commands.push(DrawCommand::Rect(Rect {
            min: start,
            max: end,
        }));
    }
    fn draw_line(&mut self, start: Point, end: Point, look: &StyleAttr) {
        self.commands
            .push(DrawCommand::Line(from_point(start), from_point(end)));
    }
    fn draw_text(&mut self, xy: Point, text: &str, _look: &StyleAttr) {
        println!("Font size: {}", _look.font_size as f32);
        self.commands.push(DrawCommand::Text(
            from_point(xy),
            text.to_string(),
            _look.font_size as f32,
        ))
    }
    fn draw_arrow(
        &mut self,
        path: &[(Point, Point)],
        dashed: bool,
        head: (bool, bool),
        look: &StyleAttr,
        text: &str,
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
    fn create_clip(&mut self, xy: Point, size: Point, rounded_px: usize) -> ClipHandle {
        0
    }
    fn draw_circle(&mut self, xy: Point, size: Point, look: &StyleAttr) {
        self.commands
            .push(DrawCommand::Circle(from_point(xy), from_point(size)));
    }
}
impl NodeLayout {
    pub fn update(&mut self, document: &KanbanDocument) {
        self.commands.clear();
        let mut vg = VisualGraph::new(layout::core::base::Orientation::LeftToRight);
        let sp0 = ShapeKind::new_box("one");
        let look0 = StyleAttr::simple();
        let mut handles: HashMap<KanbanId, NodeHandle> = HashMap::new();
        let mut arrow = Arrow::simple("");
        arrow.end = LineEndKind::None;

        for i in document.get_tasks() {
            let id = i.id;
            let shape = ShapeKind::new_box(&i.name);
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
        for i in document.get_tasks() {
            let id = i.id;
            for c in i.child_tasks.iter() {
                vg.add_edge(arrow.clone(), handles[&id], handles[c]);
            }
        }
        vg.do_it(false, false, false, self);
    }
    pub fn show(&mut self, document: &KanbanDocument, ui: &mut egui::Ui) {
        ScrollArea::both().id_salt("NodeLayout").show(ui, |ui| {
            let (response, paint) = ui.allocate_painter(
                self.max - self.min,
                egui::Sense {
                    click: false,
                    drag: false,
                    focusable: false,
                },
            );

            self.commands
                .iter()
                .for_each(|x| x.operate_on(&paint, ui.style(), response.rect));
        });
    }
}
