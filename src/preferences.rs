use eframe::egui::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::StartupLayout;
///Preferences to be stored between invocations of the program across
///all documents
#[derive(Serialize, Deserialize, Copy, Clone, Default)]
pub struct Preferences {
    ///Does nothing right now, not sure it ever will.
    pub store_undo_history_for_files: bool,
    ///If something, then the duration between automatic saves
    pub autosave: Option<Duration>,
    ///Whether or not to display the preference UI
    #[serde(skip)]
    pub showing_preference: bool,
    ///The startup layout to open a document with if not specified
    #[serde(default)]
    pub startup_layout: StartupLayout,
}

impl Preferences {
    pub fn show_ui(&mut self, ui: &mut Ui) -> Response {
        ui.vertical_centered(|ui| {
            ui.horizontal(|ui| {
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
            )
        })
        .inner
    }
}
