use eframe::egui::collapsing_header::CollapsingState;
use eframe::egui::{self, Align, Color32, Direction, Margin, RichText, ScrollArea, Stroke, Vec2};
use epaint::{Pos2, Rect};
use log::debug;

use super::{
    application, layout_cache, AppShellCommand, KanbanDocument, KanbanId, KanbanItem, Status,
    TaskRelation,
};

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
    id_salt: impl std::hash::Hash,
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
    let style = ui.visuals_mut();
    let mut status_color = style.text_color();
    let mut panel_fill = style.panel_fill;
    let mut stroke = style.noninteractive().bg_stroke;
    let mut name_color = style.text_color();

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
    match document.task_status(&item.id) {
        Status::Blocked => {
            status_color = Color32::from_rgba_unmultiplied(200, 0, 0, 255);
            style.window_fill = Color32::from_rgba_unmultiplied(75, 0, 0, 255);
        }
        Status::Ready => {
            status_color = Color32::from_rgba_unmultiplied(0, 150, 0, 255);
            style.window_fill = Color32::from_rgba_unmultiplied(0, 150, 0, 255);
        }
        _ => (),
    }

    if let Some(hovered_id) = *presentation.hovered_task {
        if document.task_is_extant(hovered_id) {
            match document.get_relation(item.id, hovered_id) {
                TaskRelation::ChildOf => {
                    stroke.color = Color32::from_rgba_premultiplied(50, 50, 250, 255)
                }
                TaskRelation::ParentOf => {
                    stroke.color = Color32::from_rgba_unmultiplied(255, 50, 50, 255)
                }
                _ => (),
            };
        }
    }

    let frame = eframe::egui::Frame::new()
        .fill(panel_fill)
        .inner_margin(Margin::same(6))
        .outer_margin(Vec2::new(3.0, 0.0))
        .corner_radius(style.noninteractive().corner_radius)
        .stroke(stroke);

    let text = RichText::new(&item.name).heading().color(name_color);
    let mut close = false;
    frame.show(ui, |ui| {
        let mut nearest_acceptable_layout = *ui.layout();
        nearest_acceptable_layout.main_dir = Direction::TopDown;
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        ui.with_layout(nearest_acceptable_layout, |ui| {
            let id = ui.make_persistent_id(format!("summary {}, idx {}", item.id, options.idx));
            CollapsingState::load_with_default_open(ui.ctx(), id, options.start_open)
                .show_header(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        if ui.label(text).double_clicked() {
                            presentation
                                .shell_commands
                                .push(AppShellCommand::OpenEditor(item.id));
                        }
                    });
                })
                .body(|ui| {
                    ui.vertical(|ui| {
                        let completion_text = match item.completed {
                            Some(_) => {
                                format!("Completed {}", item.get_completed_time_string().unwrap())
                            }
                            None => "Not completed".into(),
                        };
                        ui.label(
                            RichText::new(completion_text)
                                .color(status_color)
                                .strong()
                                .small(),
                        );
                        ui.horizontal_wrapped(|ui| {
                            let button = ui.button("Edit");
                            if button.clicked() {
                                presentation
                                    .shell_commands
                                    .push(AppShellCommand::OpenEditor(item.id));
                                close = true;
                            }
                            if ui.button("Add Child").clicked() {
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
                                .clicked()
                            {
                                presentation
                                    .application_commands
                                    .push(application::ApplicationCommand::MarkCompleted(item.id));
                                close = true;
                            }
                            if ui.button("focus").clicked() {
                                presentation
                                    .shell_commands
                                    .push(AppShellCommand::FocusOn(item.id));
                                close = true;
                            }
                        });

                        ScrollArea::vertical()
                            .id_salt(format!("Summary for item {}", item.id))
                            .max_height(50.0)
                            .show(ui, |ui| ui.label(RichText::new(item.description.clone())));
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

        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut presentation = TaskPresentationState {
                hovered_task: &mut hovered_task,
                shell_commands: &mut shell_commands,
                application_commands: &mut application_commands,
            };
            layout_id_list(&document, ui, &[1234], &mut presentation, "stale ids", None);
        });

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

        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
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

        assert!(shell_commands.is_empty());
        assert!(application_commands.is_empty());
    }
}
