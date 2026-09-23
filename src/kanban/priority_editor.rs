use eframe::egui::{self, ScrollArea};

use super::{application::ApplicationCommand, *};
///The state used for the priority editor
#[derive(Clone, Default)]
pub struct PriorityEditor {
    ///The name of the priority being editor
    pub name: String,
    ///The priority number
    pub current_value: i32,
    ///Whether the editor is open
    pub open: bool,
    ///The priority currently being renamed, if any
    renaming: Option<String>,
    ///The pending replacement name for `renaming`
    rename_to: String,
}
impl PriorityEditor {
    pub fn new() -> Self {
        PriorityEditor {
            name: String::new(),
            current_value: 0,
            open: false,
            renaming: None,
            rename_to: String::new(),
        }
    }
    /// Show the PriorityEditor
    /// * `document` the document to read priorities from
    /// * `ui` The UI instance
    ///
    /// Returns a command if the user made a change, otherwise None.
    pub fn show(
        &mut self,
        document: &KanbanDocument,
        ui: &mut egui::Ui,
    ) -> Option<ApplicationCommand> {
        let mut items: Vec<(String, i32)> = document
            .get_sorted_priorities()
            .into_iter()
            .map(|(name, priority)| (name.clone(), *priority))
            .collect();
        items.sort_by_key(|a| a.1);
        ui.horizontal(|ui| {
            ui.label("Priority name");
            ui.text_edit_singleline(&mut self.name);
        });
        let mut command: Option<ApplicationCommand> = None;
        ui.horizontal(|ui| {
            let mut s = self.current_value.to_string();
            ui.label("Priority(higher is more important)");
            ui.text_edit_singleline(&mut s);
            if let Ok(x) = s.parse::<i32>() {
                self.current_value = x;
            }
            if !self.name.is_empty() && ui.button("Add").clicked() {
                command = Some(ApplicationCommand::SetPriority(
                    self.name.clone(),
                    self.current_value,
                ));
                self.name.clear();
                self.current_value = 0;
            }
        });
        if command.is_some() {
            return command;
        }
        ScrollArea::vertical().id_salt("priorities").show(ui, |ui| {
            for (name, priority) in items.iter() {
                ui.horizontal(|ui| {
                    if self.renaming.as_deref() == Some(name.as_str()) {
                        ui.label(format!("{priority}"));
                        ui.text_edit_singleline(&mut self.rename_to);
                        if !self.rename_to.is_empty()
                            && self.rename_to.as_str() != name.as_str()
                            && items.iter().any(|(n, _)| n == &self.rename_to)
                        {
                            ui.label(
                                egui::RichText::new(
                                    "Name in use: tasks move to it; its value stays.",
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
                            && self.rename_to.as_str() != name.as_str()
                        {
                            command = Some(ApplicationCommand::RenamePriority {
                                from: name.clone(),
                                to: self.rename_to.clone(),
                            });
                        }
                        if ui.button("Cancel").clicked() {
                            self.renaming = None;
                        }
                    } else {
                        ui.label(format!("{name} - {priority}"));
                        if ui.button("+").clicked() {
                            command =
                                Some(ApplicationCommand::SetPriority(name.clone(), priority + 1));
                        }
                        if ui.button("-").clicked() {
                            command =
                                Some(ApplicationCommand::SetPriority(name.clone(), priority - 1));
                        }
                        if ui
                            .button("Rename")
                            .on_hover_text("Rename this priority everywhere it is referenced")
                            .clicked()
                        {
                            self.renaming = Some(name.clone());
                            self.rename_to = name.clone();
                        }
                    }
                });
            }
        });
        if command.is_some() {
            self.renaming = None;
        }
        command
    }
}
