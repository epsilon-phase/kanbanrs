use crate::kanban::category_editor;
use crate::kanban::priority_editor::PriorityEditor;
use crate::{AppCommand, KanbanDocument, StartupLayout};
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

///Preferences to be stored between invocations of the program across
///all documents
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Preferences {
    ///Does nothing right now, not sure it ever will.
    pub store_undo_history_for_files: bool,
    ///If something, then the duration between automatic saves
    pub autosave: Option<Duration>,
    ///Whether or not to display the preference UI
    #[serde(skip)]
    pub showing_preference: bool,
    #[serde(skip)]
    category_editor_state: category_editor::State,
    #[serde(skip)]
    priority_editor_state: PriorityEditor,
    ///The startup layout to open a document with if not specified
    #[serde(default)]
    pub startup_layout: StartupLayout,
    ///The length number of characters to wrap nodes at.
    #[serde(default = "Preferences::default_node_width")]
    pub node_width: usize,
    #[serde(default = "Preferences::default_force_iterations")]
    pub force_max_iteration: u32,
    pub template: KanbanDocument,
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
    pub fn show_ui(&mut self, ui: &mut Ui) -> Response {
        if self.category_editor_state.open {
            ui.ctx().show_viewport_immediate(
                egui::ViewportId::from_hash_of("template category editor"),
                egui::ViewportBuilder::default(),
                |ctx, _class| {
                    // This may be a good candidate for refactoring later
                    egui::CentralPanel::default().show_inside(ctx, |ui| {
                        if let Some(AppCommand::ReplaceCategory(name, style)) =
                            self.category_editor_state.show(ui, &self.template)
                        {
                            self.template.replace_category_style(&name, style);
                        }
                    });
                    if ctx.input(|i| i.viewport().close_requested()) {
                        self.category_editor_state.open = false;
                    }
                },
            )
        }
        if self.priority_editor_state.open {
            ui.ctx().show_viewport_immediate(
                egui::ViewportId::from_hash_of("template priority editor"),
                egui::ViewportBuilder::default(),
                |ctx, _class| {
                    egui::CentralPanel::default().show_inside(ctx, |ui| {
                        if let Some(AppCommand::SetPriority(name, value)) =
                            self.priority_editor_state.show(&self.template, ui)
                        {
                            self.template.set_priority(name, value);
                        }
                    });
                    if ctx.input(|i| i.viewport().close_requested()) {
                        self.priority_editor_state.open = false;
                    }
                },
            )
        }
        ui.vertical_centered(|ui| {
            CollapsingState::load_with_default_open(ui.ctx(),"Template".into(),false)
                .show_header(ui, |ui|ui.heading("Template"))
                .body(|ui|{
                ui.horizontal(|ui|{
                    if ui.button("Edit template categories").clicked(){
                        self.category_editor_state.open=true;
                    }
                    if ui.button("Edit template priorities").clicked(){
                        self.priority_editor_state.open=true;
                    }
                });
                ui.columns(2, |columns|{
                    columns[0].heading("Categories");
                    for i in self.template.get_categories(){
                        columns[0].label(i.0);
                    }
                    columns[1].heading("Priorities");
                    for i in self.template.get_sorted_priorities(){
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
                }
                response = response.union(ui.checkbox(
                    &mut self.store_undo_history_for_files,
                    "Persist undo history",
                ));
                response
            })
            .inner
            .union(
                ui.horizontal(|ui| {
                    let response= ui.checkbox(
                        &mut self.store_undo_history_for_files,
                        "Persist Undo records",
                    );
                    ui.label(r"This retains information in your home configuration directory.
It will leak information there, and may lead to retaining information saved into more secure locations.
Currently this doesn't do anything");
                    response
                })
                .inner,
            ).union(
                ui.horizontal(|ui|{
                    ComboBox::new("StartupLayout", "Startup layout").selected_text(format!("{}",&self.startup_layout))
                        .show_ui(ui,|ui|{
                            ui.selectable_value(&mut self.startup_layout, StartupLayout::Column, "Columnar");
                            ui.selectable_value(&mut self.startup_layout, StartupLayout::Queue, "Queue");
                            ui.selectable_value(&mut self.startup_layout, StartupLayout::Node, "Node");
                            ui.selectable_value(&mut self.startup_layout, StartupLayout::TreeOutline, "Tree Outline");
                        }).response
                }).inner
            ).union(
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
