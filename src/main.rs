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

    base_dirs: xdg::BaseDirectories,
    hovered_task: Option<i32>,
    close_application: bool,
    layout_cache_needs_updating: bool,
}
impl Default for KanbanRS {
    fn default() -> Self {
        return KanbanRS {
            document: KanbanDocument::default(),
            task_name: String::new(),
            open_editors: Vec::new(),
            save_file_name: None,
            current_layout: KanbanLayout::default(),
            base_dirs: xdg::BaseDirectories::with_prefix("kanbanrs").unwrap(),
            hovered_task: None,
            close_application: false,
            layout_cache_needs_updating: true,
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
            self.layout_cache_needs_updating = false;
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
            });

            ComboBox::from_label("Layout Type")
                .selected_text(String::from(&self.current_layout))
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_value(
                            &mut self.current_layout,
                            KanbanLayout::default(),
                            "Columnar",
                        )
                        .clicked()
                    {
                        self.layout_cache_needs_updating = true;
                    }
                    if ui
                        .selectable_value(&mut self.current_layout, KanbanLayout::Queue, "Queue")
                        .clicked()
                    {
                        self.layout_cache_needs_updating = true;
                    }
                    if ui
                        .selectable_value(
                            &mut self.current_layout,
                            KanbanLayout::Search(SearchState::new()),
                            "Search",
                        )
                        .clicked()
                    {
                        self.layout_cache_needs_updating = true;
                    }
                });

            ui.text_edit_singleline(&mut self.task_name);
            if ui.button("Add Task").clicked() {
                let thing = self.document.get_new_task();
                thing.name = self.task_name.clone();
                self.layout_cache_needs_updating = true;
            }

            ui.end_row();
            if let KanbanLayout::Columnar(_) = self.current_layout {
                self.layout_columnar(ui);
            } else if let KanbanLayout::Search(_) = self.current_layout {
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
            // This should probably work more like the vectors of summary actions
            //
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
                                    self.layout_cache_needs_updating = true;
                                }
                                // The main distinction between the two is that opening an
                                // existing task shouldn't change the state of the item in the
                                // document.
                                kanban::editor::EditorRequest::OpenItem(item_to_open) => {
                                    new_item = Some(item_to_open);
                                }
                                kanban::editor::EditorRequest::DeleteItem(to_delete) => {
                                    delete_item = Some(to_delete);
                                    self.layout_cache_needs_updating = true;
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
                self.layout_cache_needs_updating = true;
            }
            if let Some(item) = new_item {
                if !self
                    .open_editors
                    .iter()
                    .any(|editor| editor.item_copy.id == item.id)
                {
                    self.open_editors.push(kanban::editor::state_from(&item));
                }
                self.layout_cache_needs_updating = true;
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
        if let KanbanLayout::Columnar(cache) = &mut self.current_layout.clone() {
            ui.columns(3, |columns| {
                let mut actions: Vec<SummaryAction> = Vec::new();
                columns[0].label(RichText::new("Ready").heading());
                egui::ScrollArea::vertical()
                    .id_source("ReadyScrollarea")
                    .show_rows(&mut columns[0], 200., cache[0].len(), |ui, range| {
                        ui.vertical_centered_justified(|ui| {
                            for row in range.clone() {
                                let item_id = cache[0][row];
                                let item = self.document.get_task(item_id).unwrap();
                                let action =
                                    item.summary(&self.document, &mut self.hovered_task, ui);
                                actions.push(action);
                            }
                        });
                    });

                columns[1].label(RichText::new("Blocked").heading());
                egui::ScrollArea::vertical()
                    .id_source("BlockedScrollArea")
                    .show_rows(&mut columns[1], 200., cache[1].len(), |ui, range| {
                        ui.vertical_centered_justified(|ui| {
                            for row in range.clone() {
                                let item_id = cache[1][row];
                                let item = self.document.get_task(item_id).unwrap();
                                let action =
                                    item.summary(&self.document, &mut self.hovered_task, ui);
                                actions.push(action);
                            }
                        });
                    });
                columns[2].label(RichText::new("Completed").heading());
                egui::ScrollArea::vertical()
                    .id_source("CompletedScrollArea")
                    .show_rows(&mut columns[2], 200., cache[2].len(), |ui, range| {
                        ui.vertical_centered_justified(|ui| {
                            for row in range.clone() {
                                let item_id = cache[2][row];
                                let item = self.document.get_task(item_id).unwrap();
                                let action =
                                    item.summary(&self.document, &mut self.hovered_task, ui);
                                actions.push(action);
                            }
                        });
                    });
                actions.iter().for_each(|x| self.handle_summary_action(x));
            });
        }
    }

    pub fn layout_queue(&mut self, ui: &mut egui::Ui) {}
    pub fn layout_search(&mut self, ui: &mut egui::Ui) {
        if let KanbanLayout::Search(search_state) = &mut self.current_layout {
            let mut actions: Vec<SummaryAction> = Vec::new();
            ui.horizontal(|ui| {
                let label = ui.label("Search");

                ui.text_edit_singleline(&mut search_state.search_prompt)
                    .labelled_by(label.id);
            });
            search_state.update(&self.document);
            ScrollArea::vertical().id_source("SearchArea").show_rows(
                ui,
                200.0,
                search_state.matched_ids.len(),
                |ui, range| {
                    for row in range {
                        let task_id = search_state.matched_ids[row];
                        let task = self.document.get_task(task_id).unwrap();
                        actions.push(task.summary(&self.document, &mut self.hovered_task, ui));
                    }
                },
            );
            actions.iter().for_each(|x| self.handle_summary_action(x));
        }
    }
}
#[derive(PartialEq, Eq, Clone)]
enum KanbanLayout {
    Queue,
    Columnar([Vec<i32>; 3]),
    Search(kanban::search::SearchState),
}
impl KanbanLayout {
    fn update_columnar(columnar_cache: &mut [Vec<i32>; 3], document: &KanbanDocument) {
        columnar_cache.iter_mut().for_each(|x| x.clear());
        for task in document.get_tasks() {
            let index = match document.task_status(&task.id) {
                kanban::Status::Ready => 0,
                kanban::Status::Blocked => 1,
                kanban::Status::Completed => 2,
            };
            columnar_cache[index].push(task.id);
        }
    }
    pub fn update_cache(&mut self, document: &KanbanDocument) {
        match self {
            KanbanLayout::Queue => {}
            KanbanLayout::Columnar(array) => KanbanLayout::update_columnar(array, document),
            KanbanLayout::Search(search_state) => {
                search_state.update(document);
            }
        }
    }
}
impl Default for KanbanLayout {
    fn default() -> Self {
        KanbanLayout::Columnar([Vec::new(), Vec::new(), Vec::new()])
    }
}
impl From<&KanbanLayout> for String {
    fn from(src: &KanbanLayout) -> String {
        match src {
            KanbanLayout::Columnar(_) => "Columnar",
            KanbanLayout::Queue => "Queue",
            KanbanLayout::Search(_) => "Search",
        }
        .into()
    }
}
