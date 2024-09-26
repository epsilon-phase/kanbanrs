mod kanban;
use chrono::Utc;
use eframe::egui::{self, ComboBox, RichText, ScrollArea};

use kanban::{
    category_editor::State, editor::EditorRequest, queue_view::QueueState, search::SearchState,
    sorting::ItemSort, KanbanDocument, KanbanItem, SummaryAction,
};
use std::{fs, ops::Range, path::PathBuf};

mod document_layout;
use document_layout::*;

struct KanbanRS {
    document: KanbanDocument,
    task_name: String,
    open_editors: Vec<kanban::editor::State>,
    save_file_name: Option<PathBuf>,
    current_layout: KanbanDocumentLayout,
    base_dirs: xdg::BaseDirectories,
    hovered_task: Option<i32>,
    close_application: bool,
    layout_cache_needs_updating: bool,
    // Both of these might merit renaming at some point
    summary_actions_pending: Vec<SummaryAction>,
    editor_requests_pending: Vec<EditorRequest>,
    sorting_type: kanban::sorting::ItemSort,
    category_editor: kanban::category_editor::State,
}
impl Default for KanbanRS {
    fn default() -> Self {
        return KanbanRS {
            document: KanbanDocument::default(),
            task_name: String::new(),
            open_editors: Vec::new(),
            save_file_name: None,
            current_layout: KanbanDocumentLayout::default(),
            base_dirs: xdg::BaseDirectories::with_prefix("kanbanrs").unwrap(),
            hovered_task: None,
            close_application: false,
            layout_cache_needs_updating: true,
            summary_actions_pending: Vec::new(),
            editor_requests_pending: Vec::new(),
            sorting_type: kanban::sorting::ItemSort::None,
            category_editor: State::new(),
        };
    }
}

fn main() {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([640.0, 240.0]),
        ..Default::default()
    };
    if let Err(x) = eframe::run_native(
        "KanbanRS",
        options,
        Box::new(|_cc| Ok(Box::<KanbanRS>::default())),
    ) {
        println!("{}", x);
    }
}
impl eframe::App for KanbanRS {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.close_application {
            return;
        }
        if self.layout_cache_needs_updating {
            self.current_layout.update_cache(&self.document);
            self.current_layout
                .sort_cache(&self.document, &self.sorting_type);
            self.layout_cache_needs_updating = false;
        }
        ctx.input_mut(|i| {
            let save_shortcut = egui::KeyboardShortcut {
                modifiers: egui::Modifiers {
                    alt: false,
                    ctrl: true,
                    shift: false,
                    mac_cmd: false,
                    command: false,
                },
                logical_key: egui::Key::S,
            };
            let save_as_shortcut = egui::KeyboardShortcut {
                modifiers: egui::Modifiers {
                    alt: false,
                    ctrl: true,
                    shift: true,
                    mac_cmd: false,
                    command: false,
                },
                logical_key: egui::Key::S,
            };
            i.consume_shortcut(&save_as_shortcut).then(|| {
                self.save_file(true);
            });
            i.consume_shortcut(&save_shortcut).then(|| {
                self.save_file(false);
            });
            let find_shortcut = egui::KeyboardShortcut {
                modifiers: egui::Modifiers {
                    alt: false,
                    ctrl: true,
                    shift: false,
                    mac_cmd: false,
                    command: false,
                },
                logical_key: egui::Key::F,
            };
            i.consume_shortcut(&find_shortcut).then(|| {
                self.current_layout = KanbanDocumentLayout::Search(SearchState::new());
                self.layout_cache_needs_updating = true;
                println!("FINDING");
            })
        });
        self.hovered_task = None;
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Save").clicked() {
                        // Save to already existing file, as most applications tend to do.
                        self.save_file(false);
                        ui.close_menu();
                    }
                    if ui.button("Save As").clicked() {
                        self.save_file(true);
                        ui.close_menu();
                    }
                    if ui.button("Open").clicked() {
                        let filename = rfd::FileDialog::new()
                            .add_filter("Kanban", &["kan"])
                            .pick_file();
                        if let Some(filename) = filename {
                            self.open_file(&filename);
                        }
                        self.current_layout.update_cache(&self.document);
                        ui.close_menu();
                    }
                    ui.menu_button("Recently Used", |ui| {
                        for i in self.read_recents() {
                            let s: String = String::from(i.to_str().unwrap());
                            if ui.button(&s).clicked() {
                                self.open_file(&i);
                                ui.close_menu();
                                self.layout_cache_needs_updating = true;
                            }
                        }
                    });
                    if ui.button("Quit").clicked() {
                        self.close_application = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.button("Category style editor").clicked() {
                        self.category_editor.open = true;
                        ui.close_menu();
                    }
                });
            });
            ui.horizontal(|ui| {
                ComboBox::from_label("Layout")
                    .selected_text(String::from(&self.current_layout))
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_value(
                                &mut self.current_layout,
                                KanbanDocumentLayout::default(),
                                "Columnar",
                            )
                            .clicked()
                        {
                            self.layout_cache_needs_updating = true;
                        }
                        if ui
                            .selectable_value(
                                &mut self.current_layout,
                                KanbanDocumentLayout::Queue(QueueState::new()),
                                "Queue",
                            )
                            .clicked()
                        {
                            self.layout_cache_needs_updating = true;
                        }
                        if ui
                            .selectable_value(
                                &mut self.current_layout,
                                KanbanDocumentLayout::Search(SearchState::new()),
                                "Search",
                            )
                            .clicked()
                        {
                            self.layout_cache_needs_updating = true;
                        }
                    });
                if let KanbanDocumentLayout::Search(_) = self.current_layout {
                } else {
                    self.layout_cache_needs_updating |= self.sorting_type.combobox(ui);
                }
            });
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.task_name);
                if ui.button("Add Task").clicked() {
                    let thing = self.document.get_new_task();
                    thing.name = self.task_name.clone();
                    self.layout_cache_needs_updating = true;
                    self.current_layout.inform_of_new_items();
                }
            });

            ui.end_row();
            if let KanbanDocumentLayout::Columnar(_) = self.current_layout {
                self.layout_columnar(ui);
            } else if let KanbanDocumentLayout::Search(_) = self.current_layout {
                self.layout_search(ui);
            } else {
                self.layout_queue(ui);
            }

            self.open_editors
                .iter()
                .filter(|editor| !editor.open)
                .for_each(|editor| {
                    if !editor.cancelled {
                        self.document.replace_task(&editor.item_copy);
                        self.layout_cache_needs_updating = true;
                    }
                });
            self.open_editors.retain(|editor| editor.open);
            for editor in self.open_editors.iter_mut() {
                ui.ctx().show_viewport_immediate(
                    egui::ViewportId::from_hash_of(editor.item_copy.id),
                    egui::ViewportBuilder::default(),
                    |ctx, _class| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let request = kanban::editor::editor(ui, &self.document, editor);
                            self.editor_requests_pending.push(request);
                        });
                        if ctx.input(|i| i.viewport().close_requested()) {
                            editor.open = false;
                        }
                    },
                )
            }

            // I would prefer this in an iterator or a for loop, but, I am simply not brain enough tonight
            while !self.summary_actions_pending.is_empty() {
                let x = self.summary_actions_pending.pop().unwrap();
                self.handle_summary_action(&x);
            }
            while !self.editor_requests_pending.is_empty() {
                let x = self.editor_requests_pending.pop().unwrap();
                self.handle_editor_request(&x);
            }
            if self.category_editor.open {
                ui.ctx().show_viewport_immediate(
                    egui::ViewportId::from_hash_of("Category Editor"),
                    egui::ViewportBuilder::default(),
                    |ctx, _class| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let action = self.category_editor.show(ui, &self.document);
                            match action {
                                kanban::category_editor::EditorAction::CreateCategory(
                                    name,
                                    style,
                                ) => self.document.replace_category_style(&name, style),
                                kanban::category_editor::EditorAction::ApplyStyle(name, style) => {
                                    self.document.replace_category_style(&name, style)
                                }
                                kanban::category_editor::EditorAction::Nothing => (),
                            }
                        });
                        if ctx.input(|i| i.viewport().close_requested()) {
                            self.category_editor.open = false;
                        }
                    },
                );
            }
        });
    }
}

impl KanbanRS {
    fn handle_summary_action(&mut self, action: &SummaryAction) {
        match action {
            SummaryAction::NoAction => (),
            SummaryAction::OpenEditor(id) => {
                let mut editor = kanban::editor::state_from(self.document.get_task(*id).unwrap());
                editor.open = true;
                self.open_editors.push(editor);
            }
            SummaryAction::CreateChildOf(id) => {
                let new_task = self.document.get_new_task().id;
                let mut task_copy = self.document.get_task(*id).unwrap().clone();
                task_copy.child_tasks.push(new_task);
                let editor = kanban::editor::state_from(self.document.get_task(new_task).unwrap());
                self.document.replace_task(&task_copy);
                self.open_editors.push(editor);
                self.layout_cache_needs_updating = true;
                self.current_layout.inform_of_new_items();
            }
            SummaryAction::MarkCompleted(id) => {
                let mut task = self.document.get_task(*id).unwrap().clone();
                let new = match task.completed {
                    Some(_) => None,
                    None => Some(Utc::now()),
                };
                task.completed = new;
                self.document.replace_task(&task);
                self.layout_cache_needs_updating = true;
            }
        }
    }
    fn handle_editor_request(&mut self, request: &EditorRequest) {
        match request {
            kanban::editor::EditorRequest::NewItem(new_task) => {
                self.document.replace_task(&new_task);
                self.open_editors
                    .push(kanban::editor::state_from(&new_task));

                self.layout_cache_needs_updating = true;
                self.current_layout.inform_of_new_items();
            }
            // The main distinction between the two is that opening an
            // existing task shouldn't change the state of the item in the
            // document.
            kanban::editor::EditorRequest::OpenItem(item_to_open) => {
                self.open_editors
                    .push(kanban::editor::state_from(&item_to_open));
            }
            kanban::editor::EditorRequest::DeleteItem(to_delete) => {
                self.document.remove_task(&to_delete);
                for editor in self.open_editors.iter_mut() {
                    editor.item_copy.remove_child(&to_delete);
                }
                self.layout_cache_needs_updating = true;
                self.current_layout.inform_of_new_items();
            }
            kanban::editor::EditorRequest::UpdateItem(item) => {
                self.document.replace_task(&item);
                self.layout_cache_needs_updating = true;
            }
            _ => {}
        }
    }
}

impl KanbanRS {
    pub fn read_recents(&self) -> Vec<PathBuf> {
        let recents_file = self.base_dirs.find_state_file("recent");
        if let None = recents_file {
            return Vec::new();
        }
        let recents_file = recents_file.unwrap();
        std::fs::read_to_string(recents_file)
            .unwrap_or("".to_string())
            .split("\n")
            .filter(|x| x.len() > 0)
            .map(|x| x.into())
            .collect()
    }
    pub fn write_recents(&self) {
        let recents_file = self
            .base_dirs
            .place_state_file("recent")
            .expect("Could not create state file");
        if !std::fs::exists(&recents_file).unwrap() {
            if let Err(x) = std::fs::File::create(&recents_file) {
                println!("Failed to open file with error '{}'", x.to_string());
            }
        }
        let mut old_recents: Vec<String> = std::fs::read_to_string(&recents_file)
            .unwrap()
            .split('\n')
            .filter(|x| x.len() > 1)
            .map(|x| String::from(x))
            .collect();
        let pb: String = String::from(self.save_file_name.as_ref().unwrap().to_str().unwrap());
        // If the file is already in recents then we should avoid adding it.
        if old_recents.contains(&pb) {
            return;
        }
        if old_recents.len() > 10 {
            old_recents.rotate_right(1);
            old_recents[0] = pb;
        } else {
            old_recents.push(pb);
            old_recents.rotate_right(1);
        }
        if let Err(x) = std::fs::write(recents_file, old_recents.join("\n")) {
            println!("{}", x);
            std::process::abort();
        }
    }
    fn open_file(&mut self, path: &PathBuf) {
        let file = fs::File::open(&path).unwrap();
        self.document = serde_json::from_reader(file).unwrap();
        self.open_editors.clear();
        self.save_file_name = Some(path.into());
    }
    pub fn save_file(&mut self, force_choose_file: bool) {
        if !self.save_file_name.is_some() || force_choose_file {
            let filename = rfd::FileDialog::new()
                .add_filter("Kanban", &["kan"])
                .save_file();
            if filename.is_none() {
                return;
            }
            self.save_file_name = filename;
        }
        let file = fs::File::create(self.save_file_name.as_ref().unwrap());
        if let Err(x) = serde_json::to_writer(file.unwrap(), &self.document) {
            println!("Error on saving: {}", x);
        }
        self.write_recents();
    }
}
