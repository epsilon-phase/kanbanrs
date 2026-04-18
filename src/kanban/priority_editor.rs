use super::*;
///The state used for the priority editor
#[derive(Clone, Default)]
pub struct PriorityEditor {
    ///The name of the priority being editor
    pub name: String,
    ///The priority number
    pub current_value: i32,
    ///Whether the editor is open
    pub open: bool,
}
impl PriorityEditor {
    pub fn new() -> Self {
        PriorityEditor {
            name: String::new(),
            current_value: 0,
            open: false,
        }
    }
    /// Show the PriorityEditor
    /// * `document` the document to read priorities from
    /// * `ui` The UI instance
    ///
    /// Returns a command if the user made a change, otherwise None.
    pub fn show(&mut self, document: &KanbanDocument, ui: &mut egui::Ui) -> Option<AppCommand> {
        let mut items: Vec<(String, i32)> = document
            .get_sorted_priorities()
            .into_iter()
            .map(|(name, priority)| (name.clone(), *priority))
            .collect();
        items.sort_by(|a, b| a.1.cmp(&b.1));
        ui.horizontal(|ui| {
            ui.label("Priority name");
            ui.text_edit_singleline(&mut self.name);
        });
        let mut command: Option<AppCommand> = None;
        ui.horizontal(|ui| {
            let mut s = self.current_value.to_string();
            ui.label("Priority(higher is more important)");
            ui.text_edit_singleline(&mut s);
            if let Ok(x) = s.parse::<i32>() {
                self.current_value = x;
            }
            if !self.name.is_empty() && ui.button("Add").clicked() {
                command = Some(AppCommand::SetPriority(
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
                    ui.label(format!("{name} - {priority}"));
                    if ui.button("+").clicked() {
                        command = Some(AppCommand::SetPriority(name.clone(), priority + 1));
                    }
                    if ui.button("-").clicked() {
                        command = Some(AppCommand::SetPriority(name.clone(), priority - 1));
                    }
                });
            }
        });
        command
    }
}
