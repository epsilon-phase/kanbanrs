use std::cmp::Ordering;

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
            ItemSort::Id => "Creation Order",
            ItemSort::Name => "Name",
            ItemSort::Category => "Category",
            ItemSort::Completed => "Completed",
        }
        .to_owned()
    }
}
impl ItemSort {
    pub fn cmp_by(&self, a: &KanbanItem, b: &KanbanItem) -> std::cmp::Ordering {
        match self {
            Self::None => Ordering::Equal,
            Self::Id => a.id.cmp(&b.id),
            Self::Name => a.name.cmp(&b.name),
            Self::Category => a.category.cmp(&b.category),
            Self::Completed => a.completed.cmp(&b.completed),
        }
    }
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
        ui.label("Sort by");
        ComboBox::from_id_salt("SortingScheme")
            .selected_text(String::from(*self))
            .show_ui(ui, |ui| {
                needs_sorting = [
                    ui.selectable_value(self, Self::None, "None"),
                    ui.selectable_value(self, Self::Id, "Creation order"),
                    ui.selectable_value(self, Self::Name, "Name"),
                    ui.selectable_value(self, Self::Category, "Category"),
                    ui.selectable_value(self, Self::Completed, "Completed"),
                ]
                .iter()
                .any(|x| x.clicked());
            });
        needs_sorting
    }
}
pub fn task_comparison_completed_last(a: &KanbanItem, b: &KanbanItem) -> Ordering {
    if a.completed.is_some() {
        if b.completed.is_some() {
            return a.completed.unwrap().cmp(b.completed.as_ref().unwrap());
        } else {
            Ordering::Greater
        }
    } else if b.completed.is_some() {
        Ordering::Less
    } else {
        Ordering::Equal
    }
}
pub fn sort_completed_last(document: &KanbanDocument, ids: &mut [KanbanId]) {
    ids.sort_by(|a, b| {
        let task_a = document.get_task(*a);
        let task_b = document.get_task(*b);
        if let Some(task_a) = task_a {
            if let Some(task_b) = task_b {
                task_comparison_completed_last(task_a, task_b)
            } else {
                Ordering::Less
            }
        } else if task_b.is_some() {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    })
}
#[cfg(test)]
mod test {
    use chrono::Utc;

    use super::*;

    #[test]
    fn test_sort_completed_last() {
        let mut document: KanbanDocument = KanbanDocument::new();
        let mut a = document.get_new_task();
        let b = document.get_new_task();
        a.completed = Some(Utc::now());
        document.replace_task(&a);
        let mut thing = [a.id, b.id];
        sort_completed_last(&document, &mut thing);
        assert_eq!(task_comparison_completed_last(&a, &b), Ordering::Greater);
        assert_eq!(task_comparison_completed_last(&b, &a), Ordering::Less);
        assert_eq!(a.id, thing[1]);
    }
}
