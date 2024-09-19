use chrono::{prelude::*, DurationRound, TimeDelta};
use eframe::egui::{self, Color32, Margin, Response, RichText};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::hash_map::{Values, ValuesMut};
use std::collections::HashMap;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Blocked,
    Completed,
    Ready,
}
#[derive(Default, Serialize, Deserialize)]
pub struct KanbanDocument {
    tasks: HashMap<i32, KanbanItem>,
    priorities: HashMap<String, i32>,
    categories: HashMap<String, [u8; 4]>,
    next_id: RefCell<i32>,
}
impl KanbanDocument {
    pub fn new() -> Self {
        KanbanDocument {
            tasks: HashMap::new(),
            priorities: HashMap::new(),
            categories: HashMap::new(),
            next_id: RefCell::new(0),
        }
    }
    /** Determine if the child can be added to the parent's dependency list without
       causing a cycle
    */
    pub fn can_add_as_child(&self, parent: &KanbanItem, child: &KanbanItem) -> bool {
        let mut stack: Vec<i32> = Vec::new();
        let mut seen: Vec<i32> = Vec::new();
        stack.push(child.id);
        let mut found = false;
        while stack.len() > 0 {
            // We can be sure that it won't return a nonopt because of the loop's precondition
            let current = stack.pop().unwrap();
            if current == parent.id && seen.len() > 0 {
                found = true;
                break;
            }
            seen.push(current);
            // Either parent or child may be a hypothetical; not yet committed to the document,
            // and thus needs to be intercepted here to ensure up-to-dateness
            let task = if current == parent.id {
                &parent
            } else if current == child.id {
                &child
            } else {
                &self.tasks[&current]
            };
            task.child_tasks.iter().for_each(|child_id| {
                if seen.contains(&child_id) {
                    return;
                }
                stack.push(*child_id);
            });
        }
        return !found;
    }
    pub fn get_next_id(&self) -> i32 {
        self.next_id.replace_with(|val| (*val) + 1)
    }
    pub fn get_new_task(&mut self) -> &mut KanbanItem {
        let new_task = KanbanItem::new(&self);
        let new_task_id = new_task.id;
        self.tasks.insert(new_task_id, new_task);
        return self.tasks.get_mut(&new_task_id).unwrap();
    }
    pub fn get_tasks<'a>(&'a self) -> Values<'a, i32, KanbanItem> {
        self.tasks.values()
    }
    pub fn get_tasks_mut<'a>(&'a mut self) -> ValuesMut<'a, i32, KanbanItem> {
        self.tasks.values_mut()
    }
    pub fn task_status(&self, id: &i32) -> Status {
        match self.tasks[id].completed {
            Some(_) => return Status::Completed,
            None => {
                if self.tasks[id]
                    .child_tasks
                    .iter()
                    .all(|child_id| self.task_status(child_id) == Status::Completed)
                {
                    return Status::Ready;
                } else {
                    return Status::Blocked;
                }
            }
        }
    }
    pub fn replace_task(&mut self, item: &KanbanItem) {
        self.tasks.insert(item.id, item.clone());
    }
    pub fn get_task(&self, id: i32) -> Option<&KanbanItem> {
        self.tasks.get(&id)
    }
    pub fn remove_task(&mut self, item: &KanbanItem) {
        for i in self.tasks.values_mut() {
            i.remove_child(item);
        }
        self.tasks.remove(&item.id);
    }
}
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct KanbanItem {
    pub id: i32,
    pub name: String,
    pub description: String,
    pub completed: Option<DateTime<Utc>>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub child_tasks: Vec<i32>,
}
impl KanbanItem {
    pub fn new(document: &KanbanDocument) -> Self {
        KanbanItem {
            id: document.get_next_id(),
            name: String::new(),
            description: String::new(),
            completed: None,
            category: None,
            tags: Vec::new(),
            child_tasks: Vec::new(),
        }
    }
    pub fn remove_child(&mut self, other: &Self) {
        self.child_tasks.retain(|x| *x != other.id);
    }
    pub fn summary<G>(
        &self,
        document: &KanbanDocument,
        ui: &mut egui::Ui,
        mut on_click: G,
    ) -> egui::Id
    where
        G: FnMut(&KanbanItem),
    {
        let style = ui.visuals_mut();
        let mut status_color = style.text_color();
        let mut panel_fill = style.panel_fill;
        // Get the custom color for the category
        if self.category.is_some() {
            if let Some(color) = document.categories.get(self.category.as_ref().unwrap()) {
                panel_fill =
                    Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
            }
        }
        match document.task_status(&self.id) {
            Status::Blocked => {
                status_color = Color32::from_rgba_unmultiplied(150, 0, 0, 255);
                style.window_fill = Color32::from_rgba_unmultiplied(75, 0, 0, 255);
            }
            Status::Ready => {
                status_color = Color32::from_rgba_unmultiplied(0, 150, 0, 255);
                style.window_fill = Color32::from_rgba_unmultiplied(0, 150, 0, 255);
            }
            _ => (),
        }
        let mut id: egui::Id = egui::Id::new(0);
        /* Groups don't allow for setting the fill color.
        They might still be better, after all, the category seems like a better
        option to color the frame with */
        let frame = eframe::egui::Frame::none()
            .fill(panel_fill)
            .inner_margin(Margin::same(6.0))
            .rounding(style.noninteractive().rounding)
            .stroke(style.widgets.noninteractive.bg_stroke);
        frame.show(ui, |ui| {
            // There might be a better way to do this :p
            id = ui.id();
            ui.vertical(|ui| {
                let mut label: Option<Response> = None;
                ui.horizontal(|ui| {
                    let button = ui.button("Edit");
                    if button.clicked() {
                        on_click(self);
                    }
                    label = Some(ui.label(self.name.clone()));
                });

                ui.horizontal(|ui| {
                    let thing = match self.completed {
                        Some(x) => {
                            let local: chrono::DateTime<chrono::Local> = x.into();
                            format!(
                                "Completed on {}",
                                local
                                    .duration_round(TimeDelta::try_minutes(1).unwrap())
                                    .unwrap()
                                    .to_string()
                            )
                        }
                        None => "Not completed".into(),
                    };
                    ui.label(RichText::new(thing).color(status_color).strong());
                });
                ui.label(RichText::new(self.description.clone()));
            });
        });
        id
    }
    pub fn matches(&self, other: &str) -> bool {
        if self.name.contains(other) {
            return true;
        }
        if self.description.contains(other) {
            return true;
        }
        if self.tags.iter().any(|tag| tag == other) {
            return true;
        }
        false
    }
}
/*
*/
pub mod search {
    pub struct SearchState {
        pub matched_ids: Vec<i32>,
        pub search_prompt: String,
        /**
        The former search prompt, if search_prompt and former_search_prompt are in disagreement
        the matched_ids must be rebuilt.
        */
        former_search_prompt: String,
    }

    impl SearchState {
        pub fn new() -> Self {
            SearchState {
                matched_ids: Vec::new(),
                search_prompt: String::new(),
                former_search_prompt: String::new(),
            }
        }
        pub fn update(&mut self, document: &super::KanbanDocument) {
            if self.search_prompt == self.former_search_prompt {
                return;
            }
            self.matched_ids.clear();
            for i in document.get_tasks() {
                if i.matches(&self.search_prompt) {
                    self.matched_ids.push(i.id);
                }
            }
            self.former_search_prompt = self.search_prompt.clone();
        }
    }
}
/*
 * This is for the item editor. It requires a state object to be kept alive'
 * in order to avoid applying the changes instantaneously and making it uncomfortably
 * 'twitchy'
*/
pub mod editor {
    use super::{KanbanDocument, KanbanItem};
    use chrono::DateTime;
    use eframe::egui::{self, ComboBox};
    #[derive(Clone)]
    pub struct State {
        pub open: bool,
        pub cancelled: bool,
        pub item_copy: super::KanbanItem,
        selected_child: Option<i32>,
        new_tag: String,
    }
    pub fn state_from(item: &KanbanItem) -> State {
        State {
            open: true,
            cancelled: false,
            item_copy: item.clone(),
            selected_child: None,
            new_tag: "".into(),
        }
    }
    pub enum EditorRequest {
        NoRequest,
        NewItem(KanbanItem),
        OpenItem(KanbanItem),
        DeleteItem(KanbanItem),
    }
    pub fn editor(
        ui: &mut egui::Ui,
        document: &KanbanDocument,
        state: &mut State,
    ) -> EditorRequest {
        let mut create_child = false;
        let mut open_task: Option<i32> = None;
        let mut delete_task: Option<KanbanItem> = None;

        ui.vertical(|ui| {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut state.item_copy.name);
                });
                if state.item_copy.completed.is_some() {
                    let local: DateTime<chrono::Local> = state.item_copy.completed.unwrap().into();
                    if ui.button(format!("Completed on {}", local)).clicked() {
                        state.item_copy.completed = None;
                    }
                } else {
                    if ui.button("Mark completed").clicked() {
                        state.item_copy.completed = Some(chrono::Utc::now());
                    }
                }
                ui.heading("Description");
                ui.text_edit_multiline(&mut state.item_copy.description);

                ui.horizontal(|ui| {
                    if ui.button("Add new child").clicked {
                        create_child = true;
                    }
                    ComboBox::from_label("Select Child to add")
                        .selected_text(match state.selected_child {
                            None => "None",
                            Some(x) => &document.get_task(x).unwrap().name,
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut state.selected_child, None, "None");
                            for i in document.get_tasks().filter(|x| {
                                document.can_add_as_child(&state.item_copy, x)
                                    && x.id != state.item_copy.id
                                    && document.tasks.contains_key(&x.id)
                            }) {
                                ui.selectable_value(&mut state.selected_child, Some(i.id), &i.name);
                            }
                        });
                    if let Some(x) = state.selected_child {
                        let button = ui.button("Add Child");
                        if button.clicked() {
                            state.item_copy.child_tasks.push(x);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label("Child tasks");
                        let mut removed_task: Option<i32> = None;
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for child in state.item_copy.child_tasks.iter() {
                                if !document.tasks.contains_key(&child) {
                                    continue;
                                }
                                ui.horizontal(|ui| {
                                    if ui.link(document.tasks[child].name.clone()).clicked() {
                                        open_task = Some(*child);
                                    }
                                    let button = ui.button("Remove dependency");
                                    if button.clicked {
                                        removed_task = Some(*child);
                                    }
                                });
                            }
                            if let Some(id) = removed_task {
                                state.item_copy.child_tasks.retain(|x| *x != id);
                            }
                        });
                    });
                    ui.vertical(|ui| {
                        ui.label("Tags");
                        let mut removed_tag: Option<String> = None;
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut state.new_tag);
                            if !state.item_copy.tags.contains(&state.new_tag) {
                                if ui.button("Add tag").clicked {
                                    state.item_copy.tags.push(state.new_tag.clone());
                                    state.new_tag.clear();
                                }
                            }
                        });
                        ui.group(|ui| {
                            for tag in state.item_copy.tags.iter() {
                                ui.horizontal(|ui| {
                                    ui.label(tag);
                                    if ui.button("X").clicked {
                                        removed_tag = Some(tag.clone());
                                    }
                                });
                            }
                            if let Some(tag) = removed_tag {
                                state.item_copy.tags.retain(|x| *x != tag);
                            }
                        });
                    });
                });

                ui.horizontal(|ui| {
                    let accept_button = ui.button("Accept changes");
                    let cancel_button = ui.button("Cancel changes");
                    let delete_button = ui.button("Delete and close");
                    if accept_button.clicked() {
                        state.open = false;
                    }
                    if cancel_button.clicked() {
                        state.open = false;
                        state.cancelled = true;
                    }
                    if delete_button.clicked() {
                        state.open = false;
                        state.cancelled = true;
                        // May be more efficient to avoid copying this in full and just populate a
                        // dummy task with only the id set
                        delete_task = Some(state.item_copy.clone());
                    }
                });
            });
        });
        if let Some(to_delete) = delete_task {
            return EditorRequest::DeleteItem(to_delete);
        }
        if create_child {
            let new_child = KanbanItem::new(document);
            state.item_copy.child_tasks.push(new_child.id);
            return EditorRequest::NewItem(new_child);
        }
        if let Some(task_to_edit) = open_task {
            return EditorRequest::OpenItem(document.get_task(task_to_edit).cloned().unwrap());
        }
        EditorRequest::NoRequest
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_cycle_detection() {
        let mut document = KanbanDocument::new();
        let a = KanbanItem::new(&document);
        let a_id = a.id;
        let b = KanbanItem::new(&document);
        let b_id = b.id;
        let c = KanbanItem::new(&document);
        let c_id = c.id;
        document.tasks.insert(a_id, a);
        document.tasks.insert(b_id, b);
        document.tasks.insert(c_id, c);
        document
            .tasks
            .get_mut(&a_id)
            .unwrap()
            .child_tasks
            .push(b_id);
        assert!(!document.can_add_as_child(&document.tasks[&b_id], &document.tasks[&a_id]));
        assert!(document.can_add_as_child(&document.tasks[&c_id], &document.tasks[&a_id]));
    }
    #[test]
    fn test_task_removal() {
        let mut document = KanbanDocument::new();

        let mut a = document.get_new_task().clone();
        let mut b = document.get_new_task().clone();
        let mut c = document.get_new_task().clone();
        document.replace_task(&a);
        a.child_tasks.push(c.id);
        {
            let copy = document.get_task(a.id);
            assert!(copy.unwrap().child_tasks.len() == 1);
        }

        document.remove_task(&c);
        {
            let copy = document.get_task(a.id).unwrap();
            assert!(copy.child_tasks.is_empty());
        }
    }
}
