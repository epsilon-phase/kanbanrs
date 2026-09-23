use eframe::egui::collapsing_header::CollapsingState;
use eframe::egui::{self, Align, Color32, Direction, Margin, RichText, Stroke, Vec2};
use epaint::{Pos2, Rect};
use log::debug;

use super::{
    application, layout_cache, AppShellCommand, KanbanDocument, KanbanId, KanbanItem, Status,
    TaskRelation,
};
use crate::ACCENT;

/// Status colors shared by cards, column headers, and the status bar.
pub const READY: Color32 = Color32::from_rgb(80, 200, 120);
pub const BLOCKED: Color32 = Color32::from_rgb(235, 90, 90);
pub const COMPLETED: Color32 = Color32::from_rgb(150, 155, 165);

/// The dot color for a task status.
pub fn status_color(status: &Status) -> Color32 {
    match status {
        Status::Ready => READY,
        Status::Blocked => BLOCKED,
        Status::Completed => COMPLETED,
    }
}

/// Paint a small status dot as a span-sized widget.
pub fn status_dot(ui: &mut egui::Ui, color: Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(9.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
    response
}

/// A small rounded chip with colored text on a tinted background.
fn chip(ui: &mut egui::Ui, text: &str, color: Color32) -> egui::Response {
    egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 48))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(Margin::symmetric(5, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(text).small().strong().color(color));
        })
        .response
}

pub struct TaskPresentationState<'a> {
    pub hovered_task: &'a mut Option<KanbanId>,
    pub shell_commands: &'a mut Vec<AppShellCommand>,
    pub application_commands: &'a mut Vec<application::ApplicationCommand>,
}

#[derive(Clone, Copy)]
pub struct SummaryOptions {
    pub start_open: bool,
    pub idx: usize,
}

/// Produce a vertical task list with viewport-aware height caching.
pub fn layout_id_list(
    document: &KanbanDocument,
    ui: &mut egui::Ui,
    ids: &[KanbanId],
    presentation: &mut TaskPresentationState<'_>,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    scroll_to: Option<KanbanId>,
) {
    ui.set_width(ui.available_width());
    let cache_key = egui::Id::new(&id_salt);
    let scrollarea = egui::ScrollArea::vertical().id_salt(id_salt);

    scrollarea.show_viewport(ui, |ui, rect| {
        debug!(target:"layout_id_list","{rect:?}");
        ui.set_width(ui.available_width());
        ui.vertical_centered_justified(|ui| {
            ui.set_width(ui.available_width() - 5.);
            let height = layout_cache::cached_total_height(cache_key)
                + ui.spacing().item_spacing.y * (ids.len() as f32);
            ui.set_min_height(height);
            let mut accumulated_height: f32 = 0.;
            let populating_cache = !layout_cache::has_cache(cache_key, ids.len())
                || scroll_to.is_some_and(|x| ids.contains(&x));

            for item_id in ids.iter() {
                let Some(item) = document.get_task(*item_id) else {
                    continue;
                };

                let element_height = layout_cache::get_item_height(cache_key, *item_id);
                if let Some(element_height) = element_height {
                    if accumulated_height + element_height < rect.min.y {
                        accumulated_height += element_height + ui.spacing().item_spacing.y;
                        let mut c = ui.cursor();
                        c.set_height(element_height);
                        ui.advance_cursor_after_rect(c);
                        continue;
                    }
                }
                if accumulated_height > rect.max.y && !populating_cache {
                    break;
                }

                let start = ui.cursor().min.y;
                task_summary(
                    item,
                    document,
                    ui,
                    presentation,
                    SummaryOptions {
                        start_open: true,
                        idx: 0,
                    },
                );
                let end = ui.cursor().min.y;

                layout_cache::record_height(cache_key, *item_id, start, end);
                if scroll_to.is_some_and(|x| *item_id == x) {
                    ui.scroll_to_rect(
                        Rect {
                            min: Pos2::new(0., start),
                            max: Pos2::new(0., end),
                        },
                        Some(Align::TOP),
                    );
                }
                accumulated_height += end - start;
            }
        });
    });
}

pub fn task_summary(
    item: &KanbanItem,
    document: &KanbanDocument,
    ui: &mut egui::Ui,
    presentation: &mut TaskPresentationState<'_>,
    options: SummaryOptions,
) {
    let mut stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    let mut panel_fill = ui.visuals().faint_bg_color;
    let mut name_color = ui.visuals().text_color();

    if let Some(category) = &item.category {
        if let Some(category_style) = document.categories.get(category) {
            apply_category_style(
                category_style,
                &mut stroke,
                &mut panel_fill,
                &mut name_color,
            );
        }
    }
    let status = document.task_status(&item.id);
    let status_color = status_color(&status);

    if let Some(hovered_id) = *presentation.hovered_task {
        if document.task_is_extant(hovered_id) {
            match document.get_relation(item.id, hovered_id) {
                TaskRelation::ChildOf => {
                    stroke.color = Color32::from_rgb(80, 120, 250)
                }
                TaskRelation::ParentOf => {
                    stroke.color = Color32::from_rgb(250, 80, 90)
                }
                _ => (),
            };
        }
    }

    let dark = ui.visuals().dark_mode;
    let frame = eframe::egui::Frame::new()
        .fill(panel_fill)
        .inner_margin(Margin::symmetric(8, 6))
        .corner_radius(egui::CornerRadius::same(8))
        .stroke(stroke)
        .shadow(egui::epaint::Shadow {
            offset: [0, 1],
            blur: 4,
            spread: 0,
            color: Color32::from_black_alpha(if dark { 60 } else { 25 }),
        });

    let mut close = false;
    frame.show(ui, |ui| {
        let mut nearest_acceptable_layout = *ui.layout();
        nearest_acceptable_layout.main_dir = Direction::TopDown;
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        ui.with_layout(nearest_acceptable_layout, |ui| {
            let id = ui.make_persistent_id(format!("summary {}, idx {}", item.id, options.idx));
            CollapsingState::load_with_default_open(ui.ctx(), id, options.start_open)
                .show_header(ui, |ui| {
                    ui.horizontal(|ui| {
                        status_dot(ui, status_color);
                        let title = RichText::new(&item.name).heading().color(name_color);
                        let title = if item.completed.is_some() {
                            title.strikethrough()
                        } else {
                            title
                        };
                        if ui
                            .label(title)
                            .on_hover_text("Double-click to edit this task")
                            .double_clicked()
                        {
                            presentation
                                .shell_commands
                                .push(AppShellCommand::OpenEditor(item.id));
                        }
                        if status == Status::Blocked {
                            chip(ui, "Blocked", BLOCKED)
                                .on_hover_text("This task has uncompleted children");
                        }
                        if let Some(priority) = &item.priority {
                            chip(ui, priority, ACCENT).on_hover_text("Priority");
                        }
                        if let Some(category) = &item.category {
                            let color = document
                                .categories
                                .get(category)
                                .and_then(|s| s.text_color)
                                .map(|c| Color32::from_rgba_premultiplied(c[0], c[1], c[2], c[3]))
                                .unwrap_or(name_color);
                            chip(ui, category, color).on_hover_text("Category");
                        }
                        for tag in item.tags.iter().take(2) {
                            chip(ui, tag, ui.visuals().text_color()).on_hover_text("Tag");
                        }
                        if item.tags.len() > 2 {
                            ui.label(
                                RichText::new(format!("+{}", item.tags.len() - 2))
                                    .small()
                                    .weak(),
                            );
                        }
                        if item.time_records.is_recording() {
                            let pulse = ui.input(|i| i.time) * 4.0;
                            let alpha = (80.0 + 175.0 * (pulse.sin() * 0.5 + 0.5)) as u8;
                            ui.label(
                                RichText::new("REC")
                                    .small()
                                    .strong()
                                    .color(Color32::from_rgba_unmultiplied(235, 70, 70, alpha)),
                            )
                            .on_hover_text("Time recording in progress");
                            ui.ctx()
                                .request_repaint_after(std::time::Duration::from_millis(80));
                        }
                    });
                })
                .body(|ui| {
                    ui.vertical(|ui| {
                        if item.completed.is_some() {
                            ui.label(
                                RichText::new(format!(
                                    "Completed {}",
                                    item.get_completed_time_string().unwrap()
                                ))
                                .small()
                                .weak(),
                            );
                        }
                        ui.horizontal_wrapped(|ui| {
                            let edit = ui.add(
                                egui::Button::new(
                                    RichText::new("Edit").strong().color(Color32::WHITE),
                                )
                                .fill(ACCENT),
                            );
                            if edit
                                .on_hover_text("Open the task editor (or double-click the title)")
                                .clicked()
                            {
                                presentation
                                    .shell_commands
                                    .push(AppShellCommand::OpenEditor(item.id));
                                close = true;
                            }
                            if ui
                                .button("Add child")
                                .on_hover_text("Create a new task under this one")
                                .clicked()
                            {
                                presentation
                                    .application_commands
                                    .push(application::ApplicationCommand::CreateChildOf(item.id));
                                close = true;
                            }
                            if ui
                                .button(if item.completed.is_some() {
                                    "Uncomplete"
                                } else {
                                    "Complete"
                                })
                                .on_hover_text("Toggle the completed state")
                                .clicked()
                            {
                                presentation
                                    .application_commands
                                    .push(application::ApplicationCommand::MarkCompleted(item.id));
                                close = true;
                            }
                            if ui
                                .button("Focus")
                                .on_hover_text("Show this task with its parents and children")
                                .clicked()
                            {
                                presentation
                                    .shell_commands
                                    .push(AppShellCommand::FocusOn(item.id));
                                close = true;
                            }
                        });

                        if !item.description.is_empty() {
                            ui.label(RichText::new(&item.description));
                        }
                    })
                });
        });
    });
    if close {
        ui.close();
    }
}

fn apply_category_style(
    style: &super::KanbanCategoryStyle,
    stroke: &mut Stroke,
    panel_fill: &mut Color32,
    text_color: &mut Color32,
) {
    if let Some(color) = style.panel_fill {
        *panel_fill = Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
    }
    if let Some(stroke_width) = style.panel_stroke_width {
        stroke.width = stroke_width;
    }
    if let Some(stroke_color) = style.panel_stroke_color {
        stroke.color = Color32::from_rgba_unmultiplied(
            stroke_color[0],
            stroke_color[1],
            stroke_color[2],
            stroke_color[3],
        );
    }
    if let Some(this_text_color) = style.text_color {
        *text_color = Color32::from_rgba_premultiplied(
            this_text_color[0],
            this_text_color[1],
            this_text_color[2],
            this_text_color[3],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_id_list_ignores_ids_from_previous_document() {
        let document = KanbanDocument::new();
        let ctx = egui::Context::default();
        let mut hovered_task = None;
        let mut shell_commands = Vec::new();
        let mut application_commands = Vec::new();

        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut presentation = TaskPresentationState {
                hovered_task: &mut hovered_task,
                shell_commands: &mut shell_commands,
                application_commands: &mut application_commands,
            };
            layout_id_list(&document, ui, &[1234], &mut presentation, "stale ids", None);
        });
        output.textures_delta.clear();

        assert!(shell_commands.is_empty());
        assert!(application_commands.is_empty());
    }

    #[test]
    fn task_summary_ignores_hovered_id_from_previous_document() {
        let mut document = KanbanDocument::new();
        let task_id = document.get_new_task_mut().id;
        let task = document.get_task(task_id).unwrap();
        let ctx = egui::Context::default();
        let mut hovered_task = Some(1234);
        let mut shell_commands = Vec::new();
        let mut application_commands = Vec::new();

        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut presentation = TaskPresentationState {
                hovered_task: &mut hovered_task,
                shell_commands: &mut shell_commands,
                application_commands: &mut application_commands,
            };
            task_summary(
                task,
                &document,
                ui,
                &mut presentation,
                SummaryOptions {
                    start_open: true,
                    idx: 0,
                },
            );
        });
        output.textures_delta.clear();

        assert!(shell_commands.is_empty());
        assert!(application_commands.is_empty());
    }
}
