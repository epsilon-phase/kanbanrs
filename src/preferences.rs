use crate::kanban::application::ApplicationCommand;
use crate::kanban::category_editor;
use crate::kanban::priority_editor::PriorityEditor;
use crate::{KanbanDocument, StartupLayout};
use eframe::egui::collapsing_header::CollapsingState;
use eframe::egui::{self, *};
use lazy_static::lazy_static;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
/// The minimum iteration limit for the force directed layout slider.
const MINIMUM_ITERATIONS: u32 = 10u32;
/// The maximum iteration limit selected by the slider.
const MAXIMUM_ITERATIONS: u32 = 5000u32;

/// The seed content for new documents plus the transient editors that
/// customize it. Grouped so app-wide preferences stay app-wide; the
/// document is flattened so previously persisted JSON keeps its shape.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct DocumentTemplate {
    /// Seed content for new documents.
    #[serde(flatten)]
    pub document: KanbanDocument,
    /// Editor state for the template's categories (never persisted).
    #[serde(skip)]
    pub category_editor: category_editor::State,
    /// Editor state for the template's priorities (never persisted).
    #[serde(skip)]
    pub priority_editor: PriorityEditor,
}

///Preferences to be stored between invocations of the program across
///all documents
#[derive(Serialize, Deserialize, Clone)]
pub struct Preferences {
    ///If something, then the duration between automatic saves
    pub autosave: Option<Duration>,
    ///Whether or not to display the preference UI
    #[serde(skip)]
    pub showing_preference: bool,
    ///The startup layout to open a document with if not specified
    #[serde(default)]
    pub startup_layout: StartupLayout,
    ///The length number of characters to wrap nodes at.
    #[serde(default = "Preferences::default_node_width")]
    pub node_width: usize,
    #[serde(default = "Preferences::default_force_iterations")]
    pub force_max_iteration: u32,
    pub template: DocumentTemplate,
    ///File to open automatically on startup (native only)
    #[cfg(not(target_arch = "wasm32"))]
    pub auto_open_file: Option<String>,
    ///Use dark or light egui visuals
    #[serde(default = "Preferences::default_dark_mode")]
    pub dark_mode: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            autosave: Some(Duration::from_secs(60)),
            showing_preference: false,
            startup_layout: StartupLayout::default(),
            node_width: Self::default_node_width(),
            force_max_iteration: Self::default_force_iterations(),
            template: DocumentTemplate::default(),
            #[cfg(not(target_arch = "wasm32"))]
            auto_open_file: None,
            dark_mode: Self::default_dark_mode(),
        }
    }
}

lazy_static! {
    pub static ref PREFERENCES: Arc<RwLock<Preferences>> =
        Arc::new(RwLock::new(Preferences::default()));
}

impl Preferences {
    fn default_node_width() -> usize {
        50
    }
    fn default_force_iterations() -> u32 {
        250
    }
    fn default_dark_mode() -> bool {
        true
    }
    pub fn show_ui(
        &mut self,
        ui: &mut Ui,
        #[cfg(not(target_arch = "wasm32"))] recents: &[String],
    ) -> Response {
        if self.template.category_editor.open {
            let mut open = true;
            egui::Window::new("Template Category Editor")
                .open(&mut open)
                .show(ui, |ui| {
                    match self
                        .template
                        .category_editor
                        .show(ui, &self.template.document)
                    {
                        Some(ApplicationCommand::ReplaceCategory(name, style)) => {
                            self.template.document.replace_category_style(&name, style);
                        }
                        Some(ApplicationCommand::RenameCategory { from, to }) => {
                            self.template.document.rename_category(&from, &to);
                        }
                        _ => {}
                    }
                });
            if !open {
                self.template.category_editor.open = false;
            }
        }
        if self.template.priority_editor.open {
            let mut open = true;
            egui::Window::new("Template Priority Editor")
                .open(&mut open)
                .show(ui, |ui| {
                    match self
                        .template
                        .priority_editor
                        .show(&self.template.document, ui)
                    {
                        Some(ApplicationCommand::SetPriority(name, value)) => {
                            self.template.document.set_priority(name, value);
                        }
                        Some(ApplicationCommand::RenamePriority { from, to }) => {
                            self.template.document.rename_priority(&from, &to);
                        }
                        _ => {}
                    }
                });
            if !open {
                self.template.priority_editor.open = false;
            }
        }
        ui.vertical(|ui| {
            CollapsingState::load_with_default_open(ui.ctx(),"Template".into(),false)
                .show_header(ui, |ui|ui.heading("Template"))
                .body(|ui|{
                ui.horizontal(|ui|{
                    if ui.button("Edit template categories").clicked(){
                        self.template.category_editor.open=true;
                    }
                    if ui.button("Edit template priorities").clicked(){
                        self.template.priority_editor.open=true;
                    }
                });
                ui.columns(2, |columns|{
                    columns[0].heading("Categories");
                    for i in self.template.document.get_categories(){
                        columns[0].label(i.0);
                    }
                    columns[1].heading("Priorities");
                    for i in self.template.document.get_sorted_priorities(){
                        columns[1].label(format!("{} - {}",i.0,i.1));
                    }
                })
            });
            let resp = ui.horizontal(|ui| {
                let mut enabled = self.autosave.is_some();
                let mut response = ui.checkbox(&mut enabled, "Enable autosave");

                if enabled {
                    if self.autosave.is_none() {
                        self.autosave = Some(Duration::from_secs(600));
                    }
                    let value = self.autosave.unwrap();
                    let mut minutes = value.as_secs() / 60;
                    ui.group(|ui| {
                        response = response
                            .union(Slider::new(&mut minutes, 1..=30).text("minutes").ui(ui));
                    });
                if response.changed() {
                    self.autosave = Some(Duration::from_secs(60 * minutes));
                    }
                } else {
                    self.autosave = None;
                }
                response
            })
            .inner
            .union(
                ui.horizontal(|ui|{
                    ComboBox::new("StartupLayout", "Startup layout").selected_text(format!("{}",&self.startup_layout))
                        .show_ui(ui,|ui|{
                            ui.selectable_value(&mut self.startup_layout, StartupLayout::Column, "Columnar");
                            ui.selectable_value(&mut self.startup_layout, StartupLayout::Queue, "Queue");
                            ui.selectable_value(&mut self.startup_layout, StartupLayout::Node, "Node");
                            ui.selectable_value(&mut self.startup_layout, StartupLayout::TreeOutline, "Tree Outline");
                        }).response
                }).inner
            )
            .union(
                ui.horizontal(|ui| {
                    let resp = ui.checkbox(&mut self.dark_mode, "Dark mode");
                    if resp.changed() {
                        crate::apply_theme(ui.ctx(), self.dark_mode);
                    }
                    resp
                }).inner
            )
            .union(
                {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        ui.horizontal(|ui| {
                            let selected = self.auto_open_file.clone().unwrap_or_default();
                            let text = if selected.is_empty() { "None".to_owned() } else { selected.clone() };
                            ComboBox::new("AutoOpenFile", "Auto-open file")
                                .selected_text(&text)
                                .show_ui(ui, |ui| {
                                    if ui.selectable_label(self.auto_open_file.is_none(), "None").clicked() {
                                        self.auto_open_file = None;
                                    }
                                    for recent in recents {
                                        let is_selected = self.auto_open_file.as_ref() == Some(recent);
                                        if ui.selectable_label(is_selected, recent).clicked() {
                                            self.auto_open_file = Some(recent.clone());
                                        }
                                    }
                                })
                                .response
                        }).inner
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        ui.allocate_response(egui::Vec2::ZERO, egui::Sense::hover())
                    }
                }
            )
            .union(
                ui.horizontal(|ui|{
                    ui.label("Node width");
                    let resp = ui.add(DragValue::new(&mut self.node_width).range(25..=120));
                    ui.label(format!("{}", self.node_width));
                    resp
                }).inner
            ).union(

                ui.horizontal(|ui|{
                    ui.add(Slider::new(&mut self.force_max_iteration, MINIMUM_ITERATIONS..=MAXIMUM_ITERATIONS)
                        .text("Maximum force directed layout iterations")
                        .show_value(true))
                }).inner
            );
            #[cfg(target_arch = "wasm32")]
            if ui.button("Close").clicked() {
                self.showing_preference = false;
            }
            resp
        })
        .inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_keeps_persisted_json_shape() {
        // Document fields must stay inline under "template" (flattened)
        // so previously persisted preferences keep loading.
        let prefs = Preferences::default();
        let json = serde_json::to_value(&prefs).unwrap();
        let template = json.get("template").expect("template key");
        assert!(
            template.get("version").is_some(),
            "document fields should be flattened into template, found: {template:?}"
        );

        let round: Preferences = serde_json::from_value(json).unwrap();
        assert!(round.template.document == prefs.template.document);
    }
}
