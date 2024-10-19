use chrono::TimeDelta;

use super::*;
use std::{collections::HashSet, time};
#[derive(PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Clone, Copy, Debug)]
pub enum TimeEntry {
    InstanteousDuration(chrono::TimeDelta),

    Concluded(chrono::DateTime<chrono::Utc>, chrono::DateTime<Utc>),
    Started(chrono::DateTime<chrono::Utc>),
}
impl TimeEntry {
    /// Mark a time period as having been concluded.
    /// This produces a range of different results/
    ///
    /// For all Started items, it will turn it into a Concluded item
    /// with the current datetime
    pub fn conclude(self) -> Self {
        match self {
            Self::Started(started) => Self::Concluded(started, Utc::now()),
            _ => self,
        }
    }
    pub fn duration(self) -> chrono::TimeDelta {
        match self {
            Self::InstanteousDuration(x) => x,
            Self::Concluded(started, ended) => ended - started,
            Self::Started(start) => Utc::now() - start,
        }
    }
    pub fn to_description(self) -> String {
        let dur = self.duration();
        match self {
            Self::InstanteousDuration(_) => format!(
                "{} hours, {} minutes",
                dur.num_hours(),
                dur.num_minutes() % 60
            ),
            Self::Started(start) => format!(
                "{}",
                if dur.num_days() > 0 {
                    // The date
                    start.format("%B %d")
                } else {
                    //The hour started
                    start.format("%I:%M:%S")
                }
            ),
            Self::Concluded(start, end) => {
                if start.day() == end.day()
                    && start.month() == end.month()
                    && start.year() == end.year()
                {
                    format!("{} - {}", start.format("%I:%M%P"), end.format("%I:%M%P %F"))
                } else if start.year() == end.year() {
                    format!("{} - {}", start.format("%B %d"), end.format("%B %d %Y"))
                } else {
                    format!("{} - {}", start.format("%B %d %Y"), end.format("%B %d %Y"))
                }
            }
        }
    }
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TimeRecords {
    pub entries: Vec<(TimeEntry, Option<String>)>,
    #[serde(skip)]
    new_entry: chrono::TimeDelta,
    #[serde(skip)]
    new_description: String,
}
impl Default for TimeRecords {
    fn default() -> Self {
        Self::new()
    }
}
impl TimeRecords {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            new_entry: chrono::TimeDelta::new(0, 0).unwrap(),
            new_description: "".to_owned(),
        }
    }
    /// Finish recording, or add a new in-progress recording.
    ///
    /// * `description` - The description of the time being done
    pub fn handle_record_request(&mut self, description: Option<String>) {
        for item in self.entries.iter_mut().rev() {
            if matches!(item.0, TimeEntry::Started(_)) {
                item.0 = item.0.conclude();
                return;
            }
        }
        self.entries
            .push((TimeEntry::Started(Utc::now()), description));
    }
    pub fn is_recording(&self) -> bool {
        self.entries
            .iter()
            .rev()
            .any(|x| matches!(x.0, TimeEntry::Started(_)))
    }
    /// Get the total duration of all the time records in the structure
    pub fn duration(&self) -> chrono::TimeDelta {
        self.entries
            .iter()
            .map(|x| x.0.duration())
            .fold(chrono::TimeDelta::new(0, 0).unwrap(), |a, b| a + b)
    }
    pub fn entry_ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered_justified(|ui| {
            let hours = self.new_entry.num_hours();
            let minutes = self.new_entry.num_minutes();
            let mut h = hours.to_string();
            let mut m = (minutes % 60).to_string();
            let hour_input = ui
                .horizontal(|ui| {
                    let hour_label = ui.label("Hours");
                    ui.text_edit_singleline(&mut h)
                        .on_hover_text("Hours")
                        .labelled_by(hour_label.id)
                })
                .inner;
            ui.horizontal(|ui| {
                let minute_label = ui.label("Minutes");
                let minute_input = ui
                    .text_edit_singleline(&mut m)
                    .on_hover_text("Minutes")
                    .labelled_by(minute_label.id);
                if hour_input.union(minute_input).changed() {
                    let hours: i64 = str::parse(&h).unwrap_or(hours);
                    let minutes: i64 = str::parse(&m).unwrap_or(minutes);
                    self.new_entry = TimeDelta::new(60 * minutes + 3600 * hours, 0).unwrap();
                }
            });
            ui.text_edit_singleline(&mut self.new_description);
            ui.horizontal(|ui| {
                if ui.button("Add new entry").clicked() {
                    self.entries.push((
                        TimeEntry::InstanteousDuration(self.new_entry),
                        if !self.new_description.is_empty() {
                            Some(self.new_description.clone())
                        } else {
                            None
                        },
                    ));
                    self.new_entry = TimeDelta::new(0, 0).unwrap();
                    self.new_description.clear();
                }
                if ui
                    .button(if self.is_recording() {
                        "Stop recording"
                    } else {
                        "Start recording"
                    })
                    .clicked()
                {
                    let desc = if self.new_description.is_empty() {
                        None
                    } else {
                        Some(self.new_description.clone())
                    };
                    self.handle_record_request(desc);
                    self.new_description.clear();
                }
            });
        });
    }
    pub fn produce_list(&mut self, ui: &mut egui::Ui) {
        // This feels like a very bad use-case for retain
        // idiomatically
        self.entries.retain(|x| {
            let mut delete = false;
            ui.horizontal(|ui| {
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.label(x.0.to_description());
                        delete |= ui.button("Delete").clicked()
                    });
                    if let Some(ref desc) = x.1 {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        ui.label(desc);
                    }
                });
            });
            !delete
        });
    }
}
pub fn collect_child_durations(
    document: &KanbanDocument,
    item: &KanbanItem,
) -> Vec<(KanbanId, TimeDelta)> {
    let mut result = Vec::new();
    let mut seen: HashSet<KanbanId> = HashSet::new();
    seen.extend(item.child_tasks.iter());
    for i in item.child_tasks.iter() {
        // This needs to start at zero because on_tree calls the function on the root node.
        let mut current = document.get_task(*i).unwrap().time_records.duration();
        document.on_tree(*i, 0, |document, x, _| {
            if seen.contains(&x) {
                return;
            }
            seen.insert(x);
            let task = document.get_task(x);
            current += task.unwrap().time_records.duration();
        });
        result.push((*i, current));
    }
    result
}
#[cfg(test)]
mod test {

    use super::*;
    #[test]
    fn test_recording() {
        let mut t = TimeRecords::new();
        t.entries
            .push((TimeEntry::Started(DateTime::UNIX_EPOCH), None));
        assert!(matches!(t.entries[0].0, TimeEntry::Started(_)));
        t.handle_record_request(None);
        assert!(matches!(t.entries[0].0, TimeEntry::Concluded(_, _)));
        assert_eq!(t.entries[0].0.duration(), t.duration());
        t.handle_record_request(None);
        assert_eq!(t.entries.len(), 2);
    }
}
