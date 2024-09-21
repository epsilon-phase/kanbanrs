mod kanban;
use eframe::egui::{self, ComboBox, Rect, RichText, ScrollArea, WidgetText};

use core::f32;
use kanban::{search::SearchState, KanbanDocument, KanbanItem, SummaryAction};
use std::{
    fs,
    path::{Path, PathBuf},
};

struct KanbanRS {
    document: KanbanDocument,
    task_name: String,
    open_editors: Vec<kanban::editor::State>,
    save_file_name: Option<PathBuf>,
    current_layout: KanbanLayout,
    searcher: kanban::search::SearchState,
    base_dirs: xdg::BaseDirectories,
    hovered_task: Option<i32>,
    close_application: bool,
}
impl Default for KanbanRS {
    fn default() -> Self {
        return KanbanRS {
            document: KanbanDocument::default(),
            task_name: String::new(),
            open_editors: Vec::new(),
            save_file_name: None,
            current_layout: KanbanLayout::default(),
            searcher: SearchState::new(),
            base_dirs: xdg::BaseDirectories::with_prefix("kanbanrs").unwrap(),
            hovered_task: None,
            close_application: false,
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
        ctx.input_mut(|i| {
            let save_shortcut = egui::KeyboardShortcut {
                modifiers: egui::Modifiers {
                    alt: false,
                    ctrl: true,
                    shift: false,
                    mac_cmd: true,
                    command: false,
                },
                logical_key: egui::Key::S,
            };
            let save_as_shortcut = egui::KeyboardShortcut {
                modifiers: egui::Modifiers {
                    alt: false,
                    ctrl: true,
                    shift: true,
                    mac_cmd: true,
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
                        ui.close_menu();
                    }
                    ui.menu_button("Recently Used", |ui| {
                        for i in self.read_recents() {
                            let s: String = String::from(i.to_str().unwrap());
                            if ui.button(&s).clicked() {
                                self.open_file(&i);
                                ui.close_menu();
                            }
                        }
                    });
                    if ui.button("Quit").clicked() {
                        self.close_application = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });

            ComboBox::from_label("Layout Type")
                .selected_text(self.current_layout)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.current_layout,
                        KanbanLayout::Columnar,
                        "Columnar",
                    );
                    ui.selectable_value(&mut self.current_layout, KanbanLayout::Queue, "Queue");
                    ui.selectable_value(&mut self.current_layout, KanbanLayout::Search, "Search");
                });

            ui.text_edit_singleline(&mut self.task_name);
            if ui.button("Add Task").clicked() {
                let thing = self.document.get_new_task();
                thing.name = self.task_name.clone();
            }

            ui.end_row();
            match self.current_layout {
                KanbanLayout::Columnar => self.layout_columnar(ui),
                KanbanLayout::Queue => self.layout_queue(ui),
                KanbanLayout::Search => self.layout_search(ui),
            }

            self.open_editors
                .iter()
                .filter(|editor| !editor.open)
                .for_each(|editor| {
                    if !editor.cancelled {
                        self.document.replace_task(&editor.item_copy);
                    }
                });
            self.open_editors.retain(|editor| editor.open);
            let mut new_item: Option<KanbanItem> = None;
            let mut delete_item: Option<KanbanItem> = None;
            for editor in self.open_editors.iter_mut() {
                ui.ctx().show_viewport_immediate(
                    egui::ViewportId::from_hash_of(editor.item_copy.id),
                    egui::ViewportBuilder::default(),
                    |ctx, _class| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let request = kanban::editor::editor(ui, &self.document, editor);
                            match request {
                                kanban::editor::EditorRequest::NewItem(new_task) => {
                                    self.document.replace_task(&new_task);
                                    new_item = Some(new_task);
                                }
                                // The main distinction between the two is that opening an
                                // existing task shouldn't change the state of the item in the
                                // document.
                                kanban::editor::EditorRequest::OpenItem(item_to_open) => {
                                    new_item = Some(item_to_open);
                                }
                                kanban::editor::EditorRequest::DeleteItem(to_delete) => {
                                    delete_item = Some(to_delete);
                                }
                                _ => {}
                            }
                        });
                        if ctx.input(|i| i.viewport().close_requested()) {
                            editor.open = false;
                        }
                    },
                )
            }
            if let Some(to_delete) = delete_item {
                self.document.remove_task(&to_delete);
                for editor in self.open_editors.iter_mut() {
                    editor.item_copy.remove_child(&to_delete);
                }
            }
            if let Some(item) = new_item {
                if !self
                    .open_editors
                    .iter()
                    .any(|editor| editor.item_copy.id == item.id)
                {
                    self.open_editors.push(kanban::editor::state_from(&item));
                }
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
                task_copy.child_tasks.push(*id);
                let editor = kanban::editor::state_from(self.document.get_task(new_task).unwrap());
                self.document.replace_task(&task_copy);
                self.open_editors.push(editor);
            }
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
    pub fn layout_columnar(&mut self, ui: &mut egui::Ui) {
        let mut add_item = |item: &kanban::KanbanItem| {
            let mut editor = kanban::editor::state_from(item);
            editor.open = true;
            self.open_editors.push(editor);
        };

        // ui.push_id("Ready tasks", |ui| {
        ui.columns(3, |columns| {
            let mut actions: Vec<SummaryAction> = Vec::new();
            columns[0].label(RichText::new("Ready").heading());
            egui::ScrollArea::vertical()
                .id_source("ReadyScrollarea")
                // .auto_shrink([false; 2])
                .show(&mut columns[0], |ui| {
                    ui.vertical_centered_justified(|ui| {
                        for item in self
                            .document
                            .get_tasks()
                            .filter(|x| self.document.task_status(&x.id) == kanban::Status::Ready)
                        {
                            let action =
                                item.summary(&self.document, &mut self.hovered_task, ui, |_item| {
                                    ()
                                });
                            actions.push(action);
                        }
                    });
                });

            columns[1].label(RichText::new("Blocked").heading());
            egui::ScrollArea::vertical()
                .id_source("BlockedScrollArea")
                // .auto_shrink([false, true])
                .show(&mut columns[1], |ui| {
                    ui.vertical(|ui| {
                        for item in self
                            .document
                            .get_tasks()
                            .filter(|x| self.document.task_status(&x.id) == kanban::Status::Blocked)
                        {
                            actions.push(item.summary(
                                &self.document,
                                &mut self.hovered_task,
                                ui,
                                |_item| (),
                            ));
                        }
                    });
                });
            columns[2].label(RichText::new("Completed").heading());
            egui::ScrollArea::vertical()
                .id_source("CompletedScrollArea")
                // .auto_shrink(false)
                .show(&mut columns[2], |ui| {
                    ui.vertical(|ui| {
                        for item in self.document.get_tasks().filter(|x| {
                            self.document.task_status(&x.id) == kanban::Status::Completed
                        }) {
                            actions.push(item.summary(
                                &self.document,
                                &mut self.hovered_task,
                                ui,
                                |_item| (),
                            ));
                        }
                    });
                });
            actions.iter().for_each(|x| self.handle_summary_action(x));
        });

        // });
        // ui.push_id("Blocked tasks", |ui| {

        // ui.allocate_space(egui::Vec2 {
        //     x: width_available / 3.0,
        //     y: ui.available_size().y,
        // });
        // });
        // ui.push_id("Completed task", |ui| {

        // });
    }
    pub fn layout_queue(&mut self, ui: &mut egui::Ui) {}
    pub fn layout_search(&mut self, ui: &mut egui::Ui) {
        let mut actions: Vec<SummaryAction> = Vec::new();
        let mut add_item = |item: &kanban::KanbanItem| {
            let mut editor = kanban::editor::state_from(item);
            editor.open = true;
            self.open_editors.push(editor);
        };
        ui.horizontal(|ui| {
            let label = ui.label("Search");

            ui.text_edit_singleline(&mut self.searcher.search_prompt)
                .labelled_by(label.id);
        });
        self.searcher.update(&self.document);
        ScrollArea::vertical()
            .id_source("SearchArea")
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for task_id in self.searcher.matched_ids.iter() {
                        let task = self.document.get_task(*task_id).unwrap();
                        actions.push(task.summary(
                            &self.document,
                            &mut self.hovered_task,
                            ui,
                            |_item| (),
                        ));
                    }
                });
            });
        actions.iter().for_each(|x| self.handle_summary_action(x));
    }
}
#[derive(PartialEq, Eq, Clone, Copy)]
enum KanbanLayout {
    Queue,
    Columnar,
    Search,
}
impl Default for KanbanLayout {
    fn default() -> Self {
        KanbanLayout::Columnar
    }
}
impl Into<WidgetText> for KanbanLayout {
    fn into(self) -> WidgetText {
        match self {
            KanbanLayout::Columnar => "Columnar",
            KanbanLayout::Queue => "Queue",
            KanbanLayout::Search => "Search",
        }
        .into()
    }
}
