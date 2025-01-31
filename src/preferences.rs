use eframe::egui::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;
#[derive(Serialize, Deserialize, Copy, Clone, Default)]
pub struct Preferences {
    pub store_undo_history_for_files: bool,
    pub autosave: Option<Duration>,
    #[serde(skip)]
    pub showing_preference: bool,
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
        })
        .inner
    }
}
