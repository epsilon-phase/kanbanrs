mod kanban;
use eframe::egui;

use kanban::KanbanDocument;
use std::fs;

#[derive(Default)]
struct KanbanRS {
    document: KanbanDocument,
    task_name: String,
    open_editors: Vec<kanban::editor::State>,
}

fn main() {
    env_logger::init();
    let names = [
        "Write better widget",
        "Make completion meaningful",
        "The rest of the owl",
    ];
    let mut document = KanbanDocument::new();
    for name in names {
        let item = document.get_new_task();
        item.name = name.into();
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([640.0, 240.0]),
        ..Default::default()
    };
    eframe::run_native(
        "KanbanRS",
        options,
        Box::new(|_cc| Ok(Box::<KanbanRS>::default())),
    );
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
                        let filename = rfd::FileDialog::new()
                            .add_filter("Kanban", &["kan"])
                            .save_file();
                        let file = fs::File::create(filename.unwrap());
                        if let Err(x) = serde_json::to_writer(file.unwrap(), &self.document) {
                            println!("Error on saving: {}", x);
                        }
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
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.task_name);
                if ui.button("Add Name").clicked() {
                    let thing = self.document.get_new_task();
                    thing.name = self.task_name.clone();
                }
            });

            egui::ScrollArea::vertical().show(ui, |ui| {
                for item in self.document.get_tasks() {
                    item.summary(&self.document, ui, |item| {
                        self.open_editors.push(kanban::editor::state_from(item));
                    });
                }
            });
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
                    egui::ViewportId::from_hash_of("Editor"),
                    egui::ViewportBuilder::default(),
                    |ctx, _class| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            kanban::editor::editor(ui, &self.document, editor);
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
