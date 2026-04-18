use super::{time_tracking, AppCommand, KanbanDocument, KanbanId, KanbanItem};
use chrono::TimeDelta;
use eframe::egui::{self, Button, ComboBox, RichText, ScrollArea};
use std::{collections::BTreeSet, sync::mpsc::Sender};
///The task editor's state
#[derive(Clone)]
pub struct State {
    ///If the editor is open
    pub open: bool,
    ///Whether or not the editor's changes are cancelled, and thus discarded
    pub cancelled: bool,
    ///The copy of the item being edited
    pub item_copy: super::KanbanItem,
    ///The unique id of the editor
    pub viewport_id: egui::ViewportId,
    ///The id of the selected child
    selected_child: Option<KanbanId>,
    ///A tag's name that is being edited, but has not yet been entered
    new_tag: String,
    ///The name of the category of the task
    category: String,
    ///If the category is being edited
    editing_category: bool,
    ///If the ui is set to view the children, if not, then it will display
    ///the parents of the task
    is_on_child_view: bool,
    ///If the ui is set to display the tags, if not it will display the
    ///time records associated with the task
    is_on_tag_view: bool,
    ///The time delta for a new time entry
    new_time_entry: TimeDelta,
    ///The description for a new(but not created) time entry
    new_time_descr: String,
    ///The index of the time entry being edited
    time_entry_under_edit: Option<usize>,
    ///A pipe to the main thread to send information to
    transmitter: Sender<AppCommand>,
    ///Set to true if the editor is displaying a warning about another task
    ///having a time entry currently recording
    show_time_rec_modal: bool,
}
pub fn state_from(item: &KanbanItem, tx: Sender<AppCommand>) -> State {
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
        viewport_id: egui::ViewportId::from_hash_of(item.id),
        editing_category: false,
        show_time_rec_modal: false,
    }
}
impl State {
    pub fn editor(self: &mut State, ui: &mut egui::Ui, document: &KanbanDocument) -> bool {
        let mut create_child = false;
        let mut open_task: Option<KanbanId> = None;
        let mut delete_task: Option<KanbanItem> = None;
        let mut update_task = false;
        let mut copy: Vec<KanbanId> = self.item_copy.child_tasks.iter().copied().collect();
        super::sorting::sort_completed_last(document, &mut copy);
        ui.vertical(|ui| {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name");
                    if ui.text_edit_singleline(&mut self.item_copy.name).changed() {
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::Title(format!(
                                "Editing '{}'",
                                self.item_copy.name
                            )))
                    }
                    if ui.button("Scroll to").clicked() {
                        self.transmitter
                            .send(AppCommand::ScrollTo(self.item_copy.id))
                            .unwrap();
                    }
                });
                if self.item_copy.completed.is_some() {
                    if ui
                        .button(self.item_copy.get_completed_time_string().unwrap())
                        .clicked()
                    {
                        self.item_copy.completed = None;
                    }
                } else if ui.button("Mark completed").clicked() {
                    self.item_copy.completed = Some(chrono::Utc::now());
                }
                ui.horizontal(|ui| {
                    ui.label("Priority");
                    ComboBox::from_id_salt("Priority")
                        .selected_text(match &self.item_copy.priority {
                            Some(x) => x,
                            None => "None",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.item_copy.priority, None, "None");
                            for (name, _) in document.get_sorted_priorities().iter() {
                                ui.selectable_value(
                                    &mut self.item_copy.priority,
                                    Some((*name).clone()),
                                    (*name).clone(),
                                );
                            }
                        })
                });
                ui.heading("Description");
                ui.text_edit_multiline(&mut self.item_copy.description);
                ui.horizontal(|ui| {
                    ui.label("Category:");

                    if self.editing_category {
                        ui.text_edit_singleline(&mut self.category);
                        if ui.button("Accept").clicked() {
                            self.editing_category = false;
                        }
                    } else {
                        ComboBox::from_id_salt("Category")
                            .selected_text(&self.category)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.category, "".to_owned(), "None");
                                // let mut found_current = false;
                                for i in document.categories.keys() {
                                    ui.selectable_value(&mut self.category, i.clone(), i.clone());
                                    // found_current |= *i == self.category;
                                }
                                // This can be uncommented if I find being able to select the
                                // current value useful
                                // if !found_current {
                                //     let current = self.category.clone();
                                //     // Need to investigate ways to avoid allocation here
                                //     ui.selectable_value(
                                //         &mut self.category,
                                //         current.clone(),
                                //         &current,
                                //     );
                                // }
                            });
                        self.editing_category = ui.button("Edit").clicked();
                    }
                });

                ui.columns(2, |columns| {
                    columns[0].group(|ui| {
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut self.is_on_child_view, true, "Children");
                            ui.radio_value(&mut self.is_on_child_view, false, "Parents");
                        });
                        ui.separator();
                        if self.is_on_child_view {
                            ui.horizontal(|ui| {
                                if ui.button("Add new child").clicked() {
                                    create_child = true;
                                }
                                ui.label("Select Child to add");
                                ComboBox::from_id_salt("Select Child to add")
                                    .selected_text(match self.selected_child {
                                        None => "None",
                                        Some(x) => &document.get_task(x).unwrap().name[..12],
                                    })
                                    .show_ui(ui, |ui| {
                                        let mut task: Vec<&KanbanItem> = document
                                            .get_tasks()
                                            .filter(|x| {
                                                document.can_add_as_child(&self.item_copy, x)
                                            })
                                            .collect();
                                        let c = super::sorting::ItemSort::Id;
                                        task.sort_by(|a, b| c.cmp_by(a, b));
                                        task.reverse();
                                        task.sort_by(|a, b| {
                                            super::sorting::task_comparison_completed_last(a, b)
                                        });
                                        ui.selectable_value(&mut self.selected_child, None, "None");
                                        for i in task.drain(..) {
                                            let mut style = RichText::new(&i.name);
                                            if i.completed.is_some() {
                                                style = style.strikethrough();
                                            }
                                            ui.selectable_value(
                                                &mut self.selected_child,
                                                Some(i.id),
                                                style,
                                            );
                                        }
                                    });
                                ui.add_enabled(
                                    self.selected_child.is_some(),
                                    Button::new("Add Child"),
                                )
                                .clicked()
                                .then(|| {
                                    self.item_copy
                                        .child_tasks
                                        .insert(self.selected_child.unwrap());
                                });
                            });
                            self.show_children(ui, document, &mut open_task, &copy);
                        } else {
                            self.show_parents(ui, document, &mut open_task);
                        }
                    });
                    columns[1].group(|ui| {
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut self.is_on_tag_view, true, "Tags");
                            ui.radio_value(&mut self.is_on_tag_view, false, "Time tracking");
                        });
                        ui.separator();
                        if self.is_on_tag_view {
                            self.display_tags(ui, &document.tags);
                        } else {
                            self.show_time_records(ui, document);
                        }
                    });
                });
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        let accept_button = ui.button("Accept changes");
                        let cancel_button = ui.button("Cancel changes");
                        let delete_button = ui.button("Delete and close");
                        if accept_button.clicked() {
                            if !self.category.is_empty() {
                                self.item_copy.category = Some(self.category.clone());
                            } else {
                                self.item_copy.category = None;
                            }
                            self.open = false;
                        }
                        if cancel_button.clicked() {
                            self.open = false;
                            self.cancelled = true;
                        }
                        if delete_button.clicked() {
                            self.open = false;
                            self.cancelled = true;
                            // May be more efficient to avoid copying this in full and just populate a
                            // dummy task with only the id set
                            delete_task = Some(self.item_copy.clone());
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
        });
        let needs_update =
            open_task.is_some() || create_child || update_task || delete_task.is_some();
        if update_task {
            self.transmitter
                .send(AppCommand::UpdateTask(self.item_copy.clone()))
                .unwrap();
        }
        if let Some(to_delete) = delete_task {
            self.transmitter
                .send(AppCommand::DeleteTask(to_delete))
                .unwrap();
        }
        if create_child {
            let new_child = KanbanItem::new(document);
            self.item_copy.add_child(&new_child);
            self.transmitter
                .send(AppCommand::CreateTask(self.item_copy.clone(), new_child))
                .unwrap();
        }
        if let Some(task_to_edit) = open_task {
            self.transmitter
                .send(AppCommand::OpenTask(
                    document.get_task(task_to_edit).cloned().unwrap(),
                ))
                .unwrap();
        }
        needs_update
    }

    fn display_tags(self: &mut State, ui: &mut egui::Ui, tags: &BTreeSet<String>) {
        ui.label("Tags");
        let mut removed_tag: Option<String> = None;
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.text_edit_singleline(&mut self.new_tag);
                ComboBox::new("TagSelector", "").show_ui(ui, |ui| {
                    for i in tags.iter() {
                        ui.selectable_value(&mut self.new_tag, i.clone(), i);
                    }
                });
            });
            if !self.item_copy.tags.contains(&self.new_tag) && ui.button("Add tag").clicked() {
                self.item_copy.tags.push(self.new_tag.clone());
                self.new_tag.clear();
            }
        });
        egui::ScrollArea::vertical()
            .max_height(ui.available_height() / 2.0)
            .max_width(ui.available_width())
            .id_salt("tags")
            .show(ui, |ui| {
                for tag in self.item_copy.tags.iter() {
                    ui.horizontal(|ui| {
                        ui.label(tag);
                        if ui.button("X").clicked() {
                            removed_tag = Some(tag.clone());
                        }
                    });
                }
                if let Some(tag) = removed_tag {
                    self.item_copy.tags.retain(|x| *x != tag);
                }
            });
    }
    fn show_children(
        self: &mut State,
        ui: &mut egui::Ui,
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
            .id_salt(format!("child tasks {}", self.item_copy.id))
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
                        if button.clicked() {
                            removed_task = Some(*child);
                        }
                        if ui.button("scroll to").clicked() {
                            self.transmitter.send(AppCommand::ScrollTo(*child)).unwrap();
                        }
                    });
                }
                if let Some(id) = removed_task {
                    self.item_copy.child_tasks.retain(|x| *x != id);
                }
            });
    }
    fn show_parents(
        self: &mut State,
        ui: &mut egui::Ui,
        document: &KanbanDocument,
        open_task: &mut Option<i32>,
    ) {
        ui.set_max_width(ui.available_width());
        ui.label("Parent tasks");
        let parents: Vec<&KanbanItem> = document
            .get_tasks()
            .filter(|x| x.child_tasks.contains(&self.item_copy.id))
            .collect();
        egui::ScrollArea::vertical()
            // Without the .max_height it seems to force the button cluster at the
            // bottom half-off the screen, which I don't care for.
            .max_height(ui.available_height() / 2.0)
            .max_width(ui.available_width())
            .id_salt(format!("parent tasks {}", self.item_copy.id))
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
    fn show_time_records(self: &mut State, ui: &mut egui::Ui, document: &KanbanDocument) {
        self.time_entry_ui(ui, document);
        ScrollArea::vertical().show(ui, |ui| {
            self.produce_time_list(ui);
            ui.label("Child Tasks");
            for (child_id, duration) in
                time_tracking::collect_child_durations(document, &self.item_copy)
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
    fn time_entry_ui(self: &mut State, ui: &mut egui::Ui, document: &KanbanDocument) {
        use chrono::TimeDelta;
        use time_tracking::*;
        ui.vertical_centered_justified(|ui| {
            let hours = self.new_time_entry.num_hours();
            let minutes = self.new_time_entry.num_minutes();
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
                    self.new_time_entry = TimeDelta::new(60 * minutes + 3600 * hours, 0).unwrap();
                }
            });
            ui.text_edit_singleline(&mut self.new_time_descr);
            ui.horizontal(|ui| {
                if ui.button("Add new entry").clicked() {
                    self.item_copy.time_records.entries.push((
                        TimeEntry::InstanteousDuration(self.new_time_entry),
                        if !self.new_time_descr.is_empty() {
                            Some(self.new_time_descr.clone())
                        } else {
                            None
                        },
                    ));
                    self.new_time_entry = TimeDelta::new(0, 0).unwrap();
                    self.new_time_descr.clear();
                }
                if ui
                    .button(if self.item_copy.time_records.is_recording() {
                        "Stop recording"
                    } else {
                        "Start recording"
                    })
                    .clicked()
                {
                    // The program should inquire about the user's intention
                    // when there is 1) A task with an open time recording and
                    // 2) The task is not already recording, and thus it is being marked as finished
                    if document.get_tasks().any(|a| a.time_records.is_recording())
                        && !self.item_copy.time_records.is_recording()
                    {
                        self.show_time_rec_modal = true;
                    } else {
                        let desc = if self.new_time_descr.is_empty() {
                            None
                        } else {
                            Some(self.new_time_descr.clone())
                        };
                        self.item_copy.time_records.handle_record_request(desc);
                        self.new_time_descr.clear();
                    }
                }
            });
        });
        if self.show_time_rec_modal {
            egui::Modal::new("time_rec_modal".into()).show(ui.ctx(), |ui| {
                // This may need to be expanded to handle more than one open recording.
                // It is uncertain to me if this will be a real issue for the users.
                ui.label("You have a recording task");
                if ui.button("Start anyway").clicked() {
                    let desc = if self.new_time_descr.is_empty() {
                        None
                    } else {
                        Some(self.new_time_descr.clone())
                    };
                    self.item_copy.time_records.handle_record_request(desc);
                    self.new_time_descr.clear();
                    self.show_time_rec_modal = false;
                }
                if ui.button("Cancel").clicked() {
                    self.show_time_rec_modal = false;
                }
                if ui.button("Finish recording other tasks").clicked() {
                    if let Some(x) = document
                        .get_tasks()
                        .filter(|x| x.time_records.is_recording())
                        .map(|x| x.id)
                        .nth(0)
                    {
                        self.transmitter
                            .send(AppCommand::FinishTimeRecording(x))
                            .unwrap();
                        let desc = if self.new_time_descr.is_empty() {
                            None
                        } else {
                            Some(self.new_time_descr.clone())
                        };
                        self.item_copy.time_records.handle_record_request(desc);
                        self.new_time_descr.clear();
                        self.show_time_rec_modal = false;
                    }
                }
            });
        }
    }
    ///Create the list of time entries, as it must be displayed
    fn produce_time_list(self: &mut State, ui: &mut egui::Ui) {
        let mut current_index = 0;
        // This feels like a very bad use-case for retain
        // idiomatically
        self.item_copy.time_records.entries.retain_mut(|x| {
            let mut delete = false;
            ui.horizontal(|ui| {
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.label(x.0.to_description());
                        delete |= ui.button("Delete").clicked();
                    });
                    if let Some(index) = self.time_entry_under_edit {
                        if current_index == index {
                            if x.1.is_none() {
                                x.1 = Some(String::new());
                            }
                            ui.text_edit_multiline(x.1.as_mut().unwrap());
                            if ui.button("Done").clicked() {
                                self.time_entry_under_edit = None;
                            }
                        }
                    } else if let Some(ref desc) = x.1 {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        ui.label(desc);
                    }
                    if ui.button("Edit").clicked() {
                        self.time_entry_under_edit = Some(current_index);
                    }
                });
            });
            current_index += 1;
            !delete
        });
    }
}
