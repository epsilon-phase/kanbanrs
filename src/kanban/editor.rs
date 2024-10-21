use std::borrow::BorrowMut;

use super::{time_tracking, KanbanDocument, KanbanId, KanbanItem};
use chrono::TimeDelta;
use eframe::egui::{self, Button, ComboBox, RichText, ScrollArea};
use std::sync::mpsc::Sender;
#[derive(Clone)]
pub struct State {
    pub open: bool,
    pub cancelled: bool,
    pub item_copy: super::KanbanItem,
    selected_child: Option<KanbanId>,
    new_tag: String,
    category: String,
    is_on_child_view: bool,
    is_on_tag_view: bool,
    new_time_entry: TimeDelta,
    new_time_descr: String,
    time_entry_under_edit: Option<usize>,
    transmitter: Sender<EditorRequest>,
}
pub fn state_from(item: &KanbanItem, tx: Sender<EditorRequest>) -> State {
    State {
        open: true,
        cancelled: false,
        item_copy: item.clone(),
        selected_child: None,
        new_tag: "".into(),
        category: item.category.as_ref().unwrap_or(&String::new()).clone(),
        is_on_child_view: true,
        is_on_tag_view: true,
        new_time_descr: String::new(),
        new_time_entry: TimeDelta::new(0, 0).unwrap(),
        time_entry_under_edit: None,
        transmitter: tx,
    }
}
#[derive(Clone, Debug)]
pub enum EditorRequest {
    NoRequest,
    NewItem(KanbanItem, KanbanItem),
    OpenItem(KanbanItem),
    DeleteItem(KanbanItem),
    UpdateItem(KanbanItem),
}
pub fn editor(ui: &mut egui::Ui, document: &KanbanDocument, state: &mut State) -> bool {
    let mut create_child = false;
    let mut open_task: Option<KanbanId> = None;
    let mut delete_task: Option<KanbanItem> = None;
    let mut update_task = false;
    let mut copy: Vec<KanbanId> = state.item_copy.child_tasks.iter().copied().collect();
    let mut needs_update = false;
    super::sorting::sort_completed_last(document, &mut copy);
    ui.vertical(|ui| {
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
            ui.horizontal(|ui| {
                ui.label("Name");
                if ui.text_edit_singleline(&mut state.item_copy.name).changed() {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Title(format!(
                            "Editing '{}'",
                            state.item_copy.name
                        )))
                }
            });
            if state.item_copy.completed.is_some() {
                if ui
                    .button(state.item_copy.get_completed_time_string().unwrap())
                    .clicked()
                {
                    state.item_copy.completed = None;
                }
            } else if ui.button("Mark completed").clicked() {
                state.item_copy.completed = Some(chrono::Utc::now());
            }
            ui.horizontal(|ui| {
                ui.label("Priority");
                ComboBox::from_id_salt("Priority")
                    .selected_text(match &state.item_copy.priority {
                        Some(x) => x,
                        None => "None",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.item_copy.priority, None, "None");
                        for (name, _) in document.get_sorted_priorities().iter() {
                            ui.selectable_value(
                                &mut state.item_copy.priority,
                                Some((*name).clone()),
                                (*name).clone(),
                            );
                        }
                    })
            });
            ui.heading("Description");
            ui.text_edit_multiline(&mut state.item_copy.description);
            ui.columns(2, |columns| {
                columns[1]
                    .text_edit_singleline(&mut state.category)
                    .on_hover_text("Enter a category Name");
                columns[0].horizontal(|ui| {
                    ui.label("Category");
                    ComboBox::from_id_salt("Category")
                        .selected_text(&state.category)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut state.category, "".to_owned(), "None");
                            for i in document.categories.keys() {
                                ui.selectable_value(&mut state.category, i.clone(), i.clone());
                            }
                        })
                });
            });

            ui.columns(2, |columns| {
                columns[0].horizontal(|ui| {
                    ui.radio_value(&mut state.is_on_child_view, true, "Children");
                    ui.radio_value(&mut state.is_on_child_view, false, "Parents");
                });
                if state.is_on_child_view {
                    columns[0].horizontal(|ui| {
                        if ui.button("Add new child").clicked {
                            create_child = true;
                        }
                        ui.label("Select Child to add");
                        ComboBox::from_id_salt("Select Child to add")
                            .selected_text(match state.selected_child {
                                None => "None",
                                Some(x) => &document.get_task(x).unwrap().name[..12],
                            })
                            .show_ui(ui, |ui| {
                                let mut task: Vec<&KanbanItem> = document
                                    .get_tasks()
                                    .filter(|x| document.can_add_as_child(&state.item_copy, x))
                                    .collect();
                                let c = super::sorting::ItemSort::Id;
                                task.sort_by(|a, b| c.cmp_by(a, b));
                                task.reverse();
                                task.sort_by(|a, b| {
                                    super::sorting::task_comparison_completed_last(a, b)
                                });
                                ui.selectable_value(&mut state.selected_child, None, "None");
                                for i in task.drain(..) {
                                    let mut style = RichText::new(&i.name);
                                    if i.completed.is_some() {
                                        style = style.strikethrough();
                                    }
                                    ui.selectable_value(
                                        &mut state.selected_child,
                                        Some(i.id),
                                        style,
                                    );
                                }
                            });
                        ui.add_enabled(state.selected_child.is_some(), Button::new("Add Child"))
                            .clicked()
                            .then(|| {
                                state
                                    .item_copy
                                    .child_tasks
                                    .insert(state.selected_child.unwrap());
                            });
                    });
                    show_children(&mut columns[0], state, document, &mut open_task, &copy);
                } else {
                    show_parents(&mut columns[0], state, document, &mut open_task);
                }
                {
                    let ui = &mut columns[1];
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut state.is_on_tag_view, true, "Tags");
                        ui.radio_value(&mut state.is_on_tag_view, false, "Time tracking");
                    });
                    if state.is_on_tag_view {
                        display_tags(ui, state);
                    } else {
                        show_time_records(ui, state, document)
                    }
                }
            });
            ui.horizontal(|ui| {
                let accept_button = ui.button("Accept changes");
                let cancel_button = ui.button("Cancel changes");
                let delete_button = ui.button("Delete and close");
                if accept_button.clicked() {
                    if !state.category.is_empty() {
                        state.item_copy.category = Some(state.category.clone());
                    } else {
                        state.item_copy.category = None;
                    }
                    state.open = false;
                }
                if cancel_button.clicked() {
                    state.open = false;
                    state.cancelled = true;
                }
                if delete_button.clicked() {
                    state.open = false;
                    state.cancelled = true;
                    // May be more efficient to avoid copying this in full and just populate a
                    // dummy task with only the id set
                    delete_task = Some(state.item_copy.clone());
                }
                if accept_button
                    .union(delete_button)
                    .union(cancel_button)
                    .clicked()
                {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button("Apply").clicked() {
                    update_task = true;
                }
            });
        });
    });
    needs_update = open_task.is_some() || create_child || update_task || delete_task.is_some();
    if update_task {
        state
            .transmitter
            .send(EditorRequest::UpdateItem(state.item_copy.clone()))
            .unwrap();
    }
    if let Some(to_delete) = delete_task {
        state
            .transmitter
            .send(EditorRequest::DeleteItem(to_delete))
            .unwrap();
    }
    if create_child {
        let new_child = KanbanItem::new(document);
        state.item_copy.add_child(&new_child);
        state
            .transmitter
            .send(EditorRequest::NewItem(state.item_copy.clone(), new_child))
            .unwrap();
    }
    if let Some(task_to_edit) = open_task {
        state
            .transmitter
            .send(EditorRequest::OpenItem(
                document.get_task(task_to_edit).cloned().unwrap(),
            ))
            .unwrap();
    }
    needs_update
}

fn display_tags(ui: &mut egui::Ui, state: &mut State) {
    ui.label("Tags");
    let mut removed_tag: Option<String> = None;
    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut state.new_tag);
        if !state.item_copy.tags.contains(&state.new_tag) && ui.button("Add tag").clicked {
            state.item_copy.tags.push(state.new_tag.clone());
            state.new_tag.clear();
        }
    });
    egui::ScrollArea::vertical()
        .max_height(ui.available_height() / 2.0)
        .max_width(ui.available_width())
        .id_salt("tags")
        .show(ui, |ui| {
            for tag in state.item_copy.tags.iter() {
                ui.horizontal(|ui| {
                    ui.label(tag);
                    if ui.button("X").clicked {
                        removed_tag = Some(tag.clone());
                    }
                });
            }
            if let Some(tag) = removed_tag {
                state.item_copy.tags.retain(|x| *x != tag);
            }
        });
}

fn show_children(
    ui: &mut egui::Ui,
    state: &mut State,
    document: &KanbanDocument,
    open_task: &mut Option<i32>,
    task_vec: &[KanbanId],
) {
    ui.set_max_width(ui.available_width());

    ui.label("Child tasks");
    let mut removed_task: Option<KanbanId> = None;
    egui::ScrollArea::vertical()
        // Without the .max_height it seems to force the button cluster at the
        // bottom half-off the screen, which I don't care for.
        .max_height(ui.available_height() / 2.0)
        .max_width(ui.available_width())
        .id_salt(format!("child tasks {}", state.item_copy.id))
        .show(ui, |ui| {
            for child in task_vec.iter() {
                if !document.tasks.contains_key(child) {
                    continue;
                }
                ui.horizontal_wrapped(|ui| {
                    let mut text = RichText::new(document.tasks[child].name.clone());
                    if document.tasks[child].completed.is_some() {
                        text = text.strikethrough();
                    }
                    if ui.link(text).clicked() {
                        *open_task = Some(*child);
                    }
                    let button = ui.button("Remove");
                    if button.clicked {
                        removed_task = Some(*child);
                    }
                });
            }
            if let Some(id) = removed_task {
                state.item_copy.child_tasks.retain(|x| *x != id);
            }
        });
}
fn show_parents(
    ui: &mut egui::Ui,
    state: &mut State,
    document: &KanbanDocument,
    open_task: &mut Option<i32>,
) {
    ui.set_max_width(ui.available_width());
    ui.label("Parent tasks");
    let parents: Vec<&KanbanItem> = document
        .get_tasks()
        .filter(|x| x.child_tasks.contains(&state.item_copy.id))
        .collect();
    egui::ScrollArea::vertical()
        // Without the .max_height it seems to force the button cluster at the
        // bottom half-off the screen, which I don't care for.
        .max_height(ui.available_height() / 2.0)
        .max_width(ui.available_width())
        .id_salt(format!("parent tasks {}", state.item_copy.id))
        .show(ui, |ui| {
            for &parent in parents.iter() {
                ui.horizontal_wrapped(|ui| {
                    let mut text = RichText::new(parent.name.clone());
                    if parent.completed.is_some() {
                        text = text.strikethrough();
                    }
                    if ui.link(text).clicked() {
                        *open_task = Some(parent.id);
                    }
                });
            }
        });
}

fn show_time_records(ui: &mut egui::Ui, state: &mut State, document: &KanbanDocument) {
    time_entry_ui(state, ui);
    ScrollArea::vertical().show(ui, |ui| {
        produce_time_list(state, ui);
        ui.label("Child Tasks");
        for (child_id, duration) in
            time_tracking::collect_child_durations(document, &state.item_copy)
        {
            if duration.is_zero() {
                continue;
            }
            let task = document.get_task(child_id).unwrap();
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{}h {}m spent on {}",
                    duration.num_hours(),
                    duration.num_minutes() % 60,
                    &task.name
                ));
            });
        }
    });
}
fn time_entry_ui(state: &mut State, ui: &mut egui::Ui) {
    use chrono::TimeDelta;
    use time_tracking::*;
    ui.vertical_centered_justified(|ui| {
        let hours = state.new_time_entry.num_hours();
        let minutes = state.new_time_entry.num_minutes();
        let mut h = hours.to_string();
        let mut m = (minutes % 60).to_string();
        let hour_input = ui
            .horizontal(|ui| {
                let hour_label = ui.label("Hours");
                ui.text_edit_singleline(&mut h)
                    .on_hover_text("Hours")
                    .labelled_by(hour_label.id)
            })
            .inner;
        ui.horizontal(|ui| {
            let minute_label = ui.label("Minutes");
            let minute_input = ui
                .text_edit_singleline(&mut m)
                .on_hover_text("Minutes")
                .labelled_by(minute_label.id);
            if hour_input.union(minute_input).changed() {
                let hours: i64 = str::parse(&h).unwrap_or(hours);
                let minutes: i64 = str::parse(&m).unwrap_or(minutes);
                state.new_time_entry = TimeDelta::new(60 * minutes + 3600 * hours, 0).unwrap();
            }
        });
        ui.text_edit_singleline(&mut state.new_time_descr);
        ui.horizontal(|ui| {
            if ui.button("Add new entry").clicked() {
                state.item_copy.time_records.entries.push((
                    TimeEntry::InstanteousDuration(state.new_time_entry),
                    if !state.new_time_descr.is_empty() {
                        Some(state.new_time_descr.clone())
                    } else {
                        None
                    },
                ));
                state.new_time_entry = TimeDelta::new(0, 0).unwrap();
                state.new_time_descr.clear();
            }
            if ui
                .button(if state.item_copy.time_records.is_recording() {
                    "Stop recording"
                } else {
                    "Start recording"
                })
                .clicked()
            {
                let desc = if state.new_time_descr.is_empty() {
                    None
                } else {
                    Some(state.new_time_descr.clone())
                };
                state.item_copy.time_records.handle_record_request(desc);
                state.new_time_descr.clear();
            }
        });
    });
}
fn produce_time_list(state: &mut State, ui: &mut egui::Ui) {
    let mut current_index = 0;
    // This feels like a very bad use-case for retain
    // idiomatically
    state.item_copy.time_records.entries.retain_mut(|x| {
        let mut delete = false;
        ui.horizontal(|ui| {
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.label(x.0.to_description());
                    delete |= ui.button("Delete").clicked();
                });
                if let Some(index) = state.time_entry_under_edit {
                    if current_index == index {
                        if x.1.is_none() {
                            x.1 = Some(String::new());
                        }
                        ui.text_edit_multiline(x.1.as_mut().unwrap());
                        if ui.button("Done").clicked() {
                            state.time_entry_under_edit = None;
                        }
                    }
                }
                if let Some(ref desc) = x.1 {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    ui.label(desc);
                }
                if ui.button("Edit").clicked() {
                    state.time_entry_under_edit = Some(current_index);
                }
            });
        });
        current_index += 1;
        !delete
    });
}
