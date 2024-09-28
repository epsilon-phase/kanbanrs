use super::*;
pub struct PriorityEditor {
    pub name: String,
    pub current_value: i32,
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
    /// * `document` the document to operate on
    /// * `ui` The UI instance
    ///
    /// returns true if the layout needs to be updated due to user action.
    pub fn show(&mut self, document: &mut KanbanDocument, ui: &mut egui::Ui) -> bool {
        let mut needs_change = false;
        let mut items: Vec<(String, i32)> = document
            .priorities
            .iter()
            .map(|(name, priority)| (name.clone(), *priority))
            .collect();
        items.sort_by(|a, b| a.1.cmp(&b.1));
        ui.horizontal(|ui| {
            ui.label("Priority name");
            ui.text_edit_singleline(&mut self.name);
        });
        ui.horizontal(|ui| {
            let mut s = self.current_value.to_string();
            ui.label("Priority(higher is more important)");
            ui.text_edit_singleline(&mut s);
            if let Ok(x) = s.parse::<i32>() {
                self.current_value = x;
            }
            if !self.name.is_empty() && ui.button("Add").clicked() {
                document
                    .priorities
                    .insert(self.name.clone(), self.current_value);
                self.name.clear();
                self.current_value = 0;
                needs_change = true;
            }
        });
        ScrollArea::vertical().id_salt("priorities").show(ui, |ui| {
            for (name, priority) in items.iter() {
                ui.horizontal(|ui| {
                    ui.label(format!("{} - {}", name, priority));
                    if ui.button("+").clicked() {
                        *document.priorities.get_mut(name).unwrap() += 1;
                        needs_change = true;
                    }
                    if ui.button("-").clicked() {
                        *document.priorities.get_mut(name).unwrap() -= 1;
                        needs_change = true;
                    }
                });
            }
        });
        needs_change
    }
}
