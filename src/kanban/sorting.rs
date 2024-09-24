use super::{KanbanDocument, KanbanId, KanbanItem};
use eframe::egui::{self, ComboBox};
#[derive(PartialEq, Copy, Clone)]
pub enum ItemSort {
    None,
    Id,
    Name,
    Category,
    Completed,
}
impl From<ItemSort> for String {
    fn from(value: ItemSort) -> Self {
        match value {
            ItemSort::None => "None",
            ItemSort::Id => "Id",
            ItemSort::Name => "Name",
            ItemSort::Category => "Category",
            ItemSort::Completed => "Completed",
        }
        .to_owned()
    }
}
impl ItemSort {
    pub fn sort_by(&self, ids: &mut [KanbanId], document: &KanbanDocument) {
        match self {
            Self::None => (),
            Self::Id => ids.sort_by_key(|x| document.get_task(*x).as_ref().unwrap().id),
            Self::Name => ids.sort_by_key(|x| &document.get_task(*x).as_ref().unwrap().name),
            Self::Category => {
                ids.sort_by_key(|x| &document.get_task(*x).as_ref().unwrap().category)
            }
            Self::Completed => {
                ids.sort_by_key(|x| &document.get_task(*x).as_ref().unwrap().completed)
            }
        }
    }
    pub fn combobox(&mut self, ui: &mut egui::Ui) -> bool {
        let mut needs_sorting = false;
        ComboBox::new("SortingScheme", "Sort by").show_ui(ui, |ui| {
            needs_sorting = [
                ui.selectable_value(self, Self::None, "None"),
                ui.selectable_value(self, Self::Name, "Name"),
                ui.selectable_value(self, Self::Category, "Category"),
                ui.selectable_value(self, Self::Completed, "Completed"),
            ]
            .iter()
            .any(|x| x.clicked());
        });
        return needs_sorting;
    }
}
