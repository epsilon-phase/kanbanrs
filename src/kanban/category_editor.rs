use eframe::egui::{self, ComboBox, Ui, Widget};

use super::{application::ApplicationCommand, *};
#[derive(Clone)]
pub struct State {
    ///The category style being edited, a copy at this point
    style: KanbanCategoryStyle,
    ///The new category's name
    new_category_name: String,
    // Necessary because if the selected style changes it needs to wait till the next
    // frame to update
    selected_category_name: String,
    ///The selected category name, in the listbox, I think?
    current_category_name: String,
    ///Whether or not the category editor is opened
    pub open: bool,
    ///Whether the selected category is being renamed
    renaming: bool,
    ///The pending replacement name while `renaming`
    rename_to: String,
    ///A test document used to display the current category's style
    dummy_document: KanbanDocument,
}
impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}
impl State {
    pub fn new() -> Self {
        let mut result = State {
            style: KanbanCategoryStyle::default(),
            new_category_name: String::new(),
            selected_category_name: String::new(),
            current_category_name: String::new(),
            open: false,
            renaming: false,
            rename_to: String::new(),
            dummy_document: KanbanDocument::new(),
        };
        let mut task = result.dummy_document.get_new_task_mut().clone();
        task.category = Some("category".into());
        task.name = "Test".into();
        result.dummy_document.replace_task(&task);
        result
    }
    pub fn show(&mut self, ui: &mut Ui, document: &KanbanDocument) -> Option<ApplicationCommand> {
        let mut action: Option<ApplicationCommand> = None;

        if !self.open {
            return None;
        }
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
            if self.current_category_name != self.selected_category_name {
                self.current_category_name = self.selected_category_name.clone();
                self.style = document.categories[&self.current_category_name];
            }
            ui.columns(2, |columns| {
                columns[0].horizontal(|ui| {
                    ui.set_max_width(ui.available_width() / 2.);
                    ui.text_edit_singleline(&mut self.new_category_name);

                    if ui.button("Add new Category").clicked() {
                        self.selected_category_name = self.new_category_name.clone();
                        action = Some(ApplicationCommand::ReplaceCategory(
                            self.selected_category_name.clone(),
                            KanbanCategoryStyle::default(),
                        ));
                    }
                });
                self.dummy_document
                    .replace_category_style(&String::from("category"), self.style);
                let mut shell_commands = Vec::new();
                let mut application_commands = Vec::new();
                for i in self.dummy_document.get_tasks() {
                    let mut hovered = None;
                    super::task_presentation::task_summary(
                        i,
                        &self.dummy_document,
                        &mut columns[0],
                        &mut super::task_presentation::TaskPresentationState {
                            hovered_task: &mut hovered,
                            shell_commands: &mut shell_commands,
                            application_commands: &mut application_commands,
                        },
                        super::task_presentation::SummaryOptions {
                            start_open: true,
                            idx: 0,
                        },
                    );
                }
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
                columns[1].horizontal(|ui| {
                    if self.renaming {
                        ui.text_edit_singleline(&mut self.rename_to);
                        if !self.rename_to.is_empty()
                            && self.rename_to != self.current_category_name
                            && document.categories.contains_key(&self.rename_to)
                        {
                            ui.label(
                                egui::RichText::new(
                                    "Name in use: tasks move to it; its style stays.",
                                )
                                .small()
                                .weak()
                                .color(ui.visuals().warn_fg_color),
                            );
                        }
                        if ui
                            .button("Rename")
                            .on_hover_text("Rename everywhere it is referenced")
                            .clicked()
                            && !self.rename_to.is_empty()
                            && self.rename_to != self.current_category_name
                        {
                            action = Some(ApplicationCommand::RenameCategory {
                                from: self.current_category_name.clone(),
                                to: self.rename_to.clone(),
                            });
                            self.selected_category_name = self.rename_to.clone();
                            self.renaming = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.renaming = false;
                        }
                    } else if ui
                        .button("Rename category")
                        .on_hover_text("Rename the selected category everywhere it is referenced")
                        .clicked()
                        && !self.current_category_name.is_empty()
                    {
                        self.renaming = true;
                        self.rename_to = self.current_category_name.clone();
                    }
                });
                columns[1].group(|ui| {

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
                        if let Some(width) = &mut self.style.panel_stroke_width {
                            egui::Slider::new(width, 0.5..=12.)
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
            });
        });
        ui.horizontal(|ui| {
            if ui.button("Close").clicked() {
                self.open = false;
            }
            if ui.button("Apply style").clicked() {
                action = Some(ApplicationCommand::ReplaceCategory(
                    self.current_category_name.clone(),
                    self.style,
                ));
            }
        });
        action
    }
}
