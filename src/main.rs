mod kanban;
use eframe::egui::{self, ComboBox, WidgetText};

use kanban::KanbanDocument;
use std::{fs, path::PathBuf};

#[derive(Default)]
struct KanbanRS {
    document: KanbanDocument,
    task_name: String,
    open_editors: Vec<kanban::editor::State>,
    save_file_name: Option<PathBuf>,
    current_layout: Layout,
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
    println!("Hello, world!");
}
impl eframe::App for KanbanRS {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested()) {
            println!("{}", serde_json::to_string(&self.document).unwrap());
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Save").clicked() {
                        // Save to already existing file, as most applications tend to do.
                        self.save_file(false);
                    }
                    if ui.button("Save As").clicked() {
                        self.save_file(true);
                    }
                    if ui.button("Open").clicked() {
                        let filename = rfd::FileDialog::new()
                            .add_filter("Kanban", &["kan"])
                            .pick_file();
                        if let Some(filename) = filename {
                            let file = fs::File::open(filename).unwrap();
                            self.document = serde_json::from_reader(file).unwrap();
                            self.open_editors.clear();
                        }
                    }
                });
            });
            ComboBox::from_label("Layout Type")
                .selected_text(self.current_layout)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.current_layout, Layout::Columnar, "Columnar");
                    ui.selectable_value(&mut self.current_layout, Layout::Queue, "Queue");
                });
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.task_name);
                if ui.button("Add Task").clicked() {
                    let thing = self.document.get_new_task();
                    thing.name = self.task_name.clone();
                }
            });
            match self.current_layout {
                Layout::Columnar => self.layout_columnar(ui),
                Layout::Queue => self.layout_queue(ui),
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
            for editor in self.open_editors.iter_mut() {
                ui.ctx().show_viewport_immediate(
                    egui::ViewportId::from_hash_of(editor.item_copy.id),
                    egui::ViewportBuilder::default(),
                    |ctx, _class| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let new_item = kanban::editor::editor(ui, &self.document, editor);
                            if new_item.is_some() {
                                kanban::editor::state_from(new_item.as_ref().unwrap());
                                self.document.replace_task(new_item.as_ref().unwrap());
                            }
                        });
                        if ctx.input(|i| i.viewport().close_requested()) {
                            editor.open = false;
                        }
                    },
                )
            }
        });
    }
}
impl KanbanRS {
    pub fn save_file(&mut self, force_choose_file: bool) {
        if self.save_file_name.is_some() && !force_choose_file {
            let filename = rfd::FileDialog::new()
                .add_filter("Kanban", &["kan"])
                .save_file();
            self.save_file_name = Some(filename.unwrap());
        }
        let file = fs::File::create(self.save_file_name.as_ref().unwrap());
        if let Err(x) = serde_json::to_writer(file.unwrap(), &self.document) {
            println!("Error on saving: {}", x);
        }
    }
    pub fn layout_columnar(&mut self, ui: &mut egui::Ui) {
        let mut add_item = |item: &kanban::KanbanItem| {
            let mut editor = kanban::editor::state_from(item);
            editor.open = true;
            self.open_editors.push(editor);
        };
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label("Ready");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for item in self
                        .document
                        .get_tasks()
                        .filter(|x| self.document.task_status(&x.id) == kanban::Status::Ready)
                    {
                        item.summary(&self.document, ui, |item| {
                            add_item(item);
                        });
                    }
                });
            });
            ui.vertical(|ui| {
                ui.label("Blocked");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for item in self
                        .document
                        .get_tasks()
                        .filter(|x| self.document.task_status(&x.id) == kanban::Status::Blocked)
                    {
                        item.summary(&self.document, ui, |item| {
                            add_item(item);
                        });
                    }
                });
            });
            ui.vertical(|ui| {
                ui.label("Completed");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for item in self
                        .document
                        .get_tasks()
                        .filter(|x| self.document.task_status(&x.id) == kanban::Status::Completed)
                    {
                        item.summary(&self.document, ui, |item| {
                            add_item(item);
                        });
                    }
                });
            });
        });
    }
    pub fn layout_queue(&mut self, ui: &mut egui::Ui) {}
}
#[derive(PartialEq, Eq, Clone, Copy)]
enum Layout {
    Queue,
    Columnar,
}
impl Default for Layout {
    fn default() -> Self {
        Layout::Columnar
    }
}
impl Into<WidgetText> for Layout {
    fn into(self) -> WidgetText {
        match self {
            Layout::Columnar => "Columnar",
            Layout::Queue => "Queue",
        }
        .into()
    }
}
