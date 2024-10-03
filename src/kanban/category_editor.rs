use egui::{ComboBox, Ui, Widget};

use super::*;
#[derive(PartialEq)]
pub enum EditorAction {
    CreateCategory(String, KanbanCategoryStyle),
    ApplyStyle(String, KanbanCategoryStyle),
    Nothing,
}
pub struct State {
    style: KanbanCategoryStyle,
    new_category_name: String,
    // Necessary because if the selected style changes it needs to wait till the next
    // frame to update
    selected_category_name: String,
    current_category_name: String,
    pub open: bool,
    dummy_document: KanbanDocument,
}
impl State {
    pub fn new() -> Self {
        let mut result = State {
            style: KanbanCategoryStyle::default(),
            new_category_name: String::new(),
            selected_category_name: String::new(),
            current_category_name: String::new(),
            open: false,
            dummy_document: KanbanDocument::new(),
        };
        let mut task = result.dummy_document.get_new_task_mut().clone();
        task.category = Some("category".into());
        task.name = "Test".into();
        result.dummy_document.replace_task(&task);
        result
    }
    pub fn show(&mut self, ui: &mut Ui, document: &KanbanDocument) -> EditorAction {
        let mut action = EditorAction::Nothing;

        if !self.open {
            return EditorAction::Nothing;
        }
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
            if self.current_category_name != self.selected_category_name {
                self.current_category_name = self.selected_category_name.clone();
                self.style = document.categories[&self.current_category_name];
            }
            ui.columns(2, |columns| {
                columns[0].horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.new_category_name);

                    if ui.button("Add new Category").clicked() {
                        self.selected_category_name = self.new_category_name.clone();
                        action = EditorAction::CreateCategory(
                            self.selected_category_name.clone(),
                            KanbanCategoryStyle::default(),
                        );
                    }
                });
                ComboBox::new("Style select", "Select Category")
                    .selected_text(&self.current_category_name)
                    .show_ui(&mut columns[1], |ui| {
                        for category in document.categories.keys() {
                            ui.selectable_value(
                                &mut self.selected_category_name,
                                category.clone(),
                                category.clone(),
                            );
                        }
                    });
            });
        });
        ui.group(|ui| {

            ui.checkbox(
                &mut self.style.children_inherit_category,
                "Children inherit category",
            ).on_hover_text("When enabled, newly created children will be populated with the category of their parent.");
            // Outline color
            ui.group(|ui| {
                ui.label("Outline color");
                if let Some(color) = self.style.panel_stroke_color {
                    let mut color = egui::Color32::from_rgba_unmultiplied(
                        color[0], color[1], color[2], color[3],
                    );
                    ui.color_edit_button_srgba(&mut color);
                    self.style.panel_stroke_color = Some(color.to_array());
                    if ui.button("Clear color").clicked() {
                        self.style.panel_stroke_color = None;
                    }
                } else if ui.button("Add stroke color").clicked() {
                    self.style.panel_stroke_color = Some([0, 0, 0, 255]);
                }
            });
            //Outline width
            ui.group(|ui| {
                ui.label("Outline thickness");
                if self.style.panel_stroke_width.is_some() {
                    egui::Slider::new(self.style.panel_stroke_width.as_mut().unwrap(), 0.5..=12.)
                        .ui(ui);
                } else if ui.button("Set stroke width").clicked() {
                    self.style.panel_stroke_width =
                        Some(ui.style().noninteractive().bg_stroke.width);
                }
            });
            // Fill color
            ui.group(|ui| {
                ui.label("Fill color");
                if let Some(color) = self.style.panel_fill {
                    let mut color = egui::Color32::from_rgba_unmultiplied(
                        color[0], color[1], color[2], color[3],
                    );
                    ui.color_edit_button_srgba(&mut color);
                    self.style.panel_fill = Some(color.to_array());
                    if ui.button("Clear color").clicked() {
                        self.style.panel_fill = None;
                    }
                } else if ui.button("Add fill color").clicked() {
                    self.style.panel_fill = ui.style().visuals.panel_fill.to_array().into();
                }
            });
            ui.group(|ui| {
                ui.label("Task Name color");
                if let Some(color) = self.style.text_color {
                    let mut color = egui::Color32::from_rgba_unmultiplied(
                        color[0], color[1], color[2], color[3],
                    );
                    ui.color_edit_button_srgba(&mut color);
                    self.style.text_color = Some(color.to_array());
                    if ui.button("Clear color").clicked() {
                        self.style.text_color = None;
                    }
                } else if ui.button("Set text color").clicked() {
                    self.style.text_color = Some([255, 255, 255, 255])
                }
            });
        });
        self.dummy_document
            .replace_category_style(&String::from("category"), self.style);
        for i in self.dummy_document.get_tasks() {
            let mut hovered = None;
            i.summary(&self.dummy_document, &mut hovered, ui);
        }
        if ui.button("Apply style").clicked() {
            action = EditorAction::ApplyStyle(self.current_category_name.clone(), self.style);
        }
        action
    }
}
