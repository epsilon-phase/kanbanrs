use chrono::{prelude::*, DurationRound, TimeDelta};
use eframe::egui::{self, Color32, Margin, Response, RichText, Rounding, ScrollArea, Stroke, Vec2};
use nucleo_matcher::pattern::Pattern;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::hash_map::{Values, ValuesMut};
use std::collections::HashMap;

pub mod category_editor;
pub mod sorting;

pub type KanbanId = i32;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Blocked,
    Completed,
    Ready,
}
#[derive(Default, Serialize, Deserialize)]
pub struct KanbanDocument {
    tasks: HashMap<KanbanId, KanbanItem>,
    priorities: HashMap<String, i32>,
    categories: HashMap<String, KanbanCategoryStyle>,
    next_id: RefCell<KanbanId>,
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
        let mut stack: Vec<KanbanId> = Vec::new();
        let mut seen: Vec<KanbanId> = Vec::new();
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
    pub fn get_next_id(&self) -> KanbanId {
        self.next_id.replace_with(|val| (*val) + 1)
    }
    /**
    Create a new task and add it to the document, returning a mutable reference
    */
    pub fn get_new_task(&mut self) -> &mut KanbanItem {
        let new_task = KanbanItem::new(&self);
        let new_task_id = new_task.id;
        self.tasks.insert(new_task_id, new_task);
        return self.tasks.get_mut(&new_task_id).unwrap();
    }
    pub fn get_tasks<'a>(&'a self) -> Values<'a, KanbanId, KanbanItem> {
        self.tasks.values()
    }
    pub fn get_tasks_mut<'a>(&'a mut self) -> ValuesMut<'a, KanbanId, KanbanItem> {
        self.tasks.values_mut()
    }
    pub fn task_status(&self, id: &KanbanId) -> Status {
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
        if item.category.is_some() {
            if !self
                .categories
                .contains_key(item.category.as_ref().unwrap())
            {
                self.categories.insert(
                    item.category.as_ref().unwrap().clone(),
                    KanbanCategoryStyle::default(),
                );
            }
        }
    }
    pub fn get_task(&self, id: KanbanId) -> Option<&KanbanItem> {
        self.tasks.get(&id)
    }
    pub fn remove_task(&mut self, item: &KanbanItem) {
        for i in self.tasks.values_mut() {
            i.remove_child(item);
        }
        self.tasks.remove(&item.id);
    }
    pub fn get_relation(&self, target: KanbanId, other: KanbanId) -> TaskRelation {
        let task_a = self.get_task(target).unwrap();
        let task_b = self.get_task(other).unwrap();
        if task_a.is_child_of(task_b, self) {
            return TaskRelation::ChildOf;
        }
        if task_b.is_child_of(task_a, self) {
            return TaskRelation::ParentOf;
        }
        TaskRelation::Unrelated
    }
    /**
    Get the numeric priority of the task. Defaults to zero when
    * The task does not have a set priority
    * The task's priority does not have a numeric value assigned to it.
    */
    pub fn task_priority_value(&self, task: &i32) -> i32 {
        if let Some(priority_name) = &self.tasks[task].priority {
            if let Some(value) = self.priorities.get(priority_name) {
                return *value;
            } else {
                return 0;
            }
        }
        return 0;
    }
}
/// Category functions
impl KanbanDocument {
    pub fn replace_category_style(&mut self, name: &String, style: KanbanCategoryStyle) {
        self.categories.insert(name.clone(), style);
    }
}

/// Layout helper functions. Very simple. Might be better moved elsewhere at some point
impl KanbanDocument {
    //! Produce a vertical layout scrolling downwards.
    //!
    //! self the document, you silly goose
    pub fn layout_id_list<I>(
        &self,
        ui: &mut egui::Ui,
        ids: &I,
        range: std::ops::Range<usize>,
        hovered_task: &mut Option<i32>,
        event_collector: &mut Vec<SummaryAction>,
    ) where
        I: std::ops::Index<usize, Output = i32>,
    {
        ui.vertical_centered_justified(|ui| {
            for row in range.clone() {
                let item_id = ids[row];
                let item = &self.tasks[&item_id];
                let action = item.summary(&self, hovered_task, ui);
                event_collector.push(action);
            }
        });
    }
}
#[derive(PartialEq, Eq)]
pub enum TaskRelation {
    Unrelated,
    ChildOf,
    ParentOf,
}
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct KanbanItem {
    pub id: KanbanId,
    pub name: String,
    pub description: String,
    pub completed: Option<DateTime<Utc>>,
    pub category: Option<String>,
    pub priority: Option<String>,
    pub tags: Vec<String>,
    pub child_tasks: Vec<KanbanId>,
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
            priority: None,
            child_tasks: Vec::new(),
        }
    }
    pub fn remove_child(&mut self, other: &Self) {
        self.child_tasks.retain(|x| *x != other.id);
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
    /// Fill a buffer with a string for the purposes of full text search
    /// \param
    pub fn into_searchable_string(&self, output: &mut String) {
        output.extend(self.name.chars());
        output.push(' ');
        output.push_str(self.category.as_ref().map(|x| x.as_str()).unwrap_or(""));
        output.push(' ');
        output.push_str(self.priority.as_ref().map(|x| x.as_str()).unwrap_or(""));
        output.push(' ');
        output.push_str(&self.description);
        output.push(' ');
        for tag in self.tags.iter() {
            output.push_str(tag.as_str());
            output.push(' ');
        }
    }
}
#[derive(Clone, Copy)]
pub enum SummaryAction {
    NoAction,
    OpenEditor(KanbanId),
    CreateChildOf(KanbanId),
}
impl KanbanItem {
    pub fn summary(
        &self,
        document: &KanbanDocument,
        hovered_task: &mut Option<KanbanId>,
        ui: &mut egui::Ui,
    ) -> SummaryAction {
        let mut action = SummaryAction::NoAction;
        let style = ui.visuals_mut();
        let mut status_color = style.text_color();
        let mut panel_fill = style.panel_fill;
        let mut stroke = style.noninteractive().bg_stroke;
        // Get the custom color for the category
        if self.category.is_some() {
            if let Some(category_style) = document.categories.get(self.category.as_ref().unwrap()) {
                category_style.apply_to(&mut stroke, &mut panel_fill);
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

        if let Some(ht) = hovered_task {
            match document.get_relation(self.id, *ht) {
                TaskRelation::ChildOf => {
                    stroke.color = Color32::from_rgba_premultiplied(50, 50, 250, 255)
                }
                TaskRelation::ParentOf => {
                    stroke.color = Color32::from_rgba_unmultiplied(255, 50, 50, 255)
                }
                _ => (),
            };
        }
        let mut id: egui::Id = egui::Id::new(0);
        /* Groups don't allow for setting the fill color.
        They might still be better, after all, the category seems like a better
        option to color the frame with */
        let frame = eframe::egui::Frame::none()
            .fill(panel_fill)
            .inner_margin(Margin::same(6.0))
            .outer_margin(Vec2::new(3.0, 0.0))
            .rounding(style.noninteractive().rounding)
            .stroke(stroke);
        frame.show(ui, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            // There might be a better way to do this :p
            id = ui.id();
            ui.vertical(|ui| {
                let mut label: Option<Response> = None;
                ui.horizontal(|ui| {
                    if hovered_task.is_none() {
                        label = Some(ui.label(self.name.clone()));
                    } else {
                        label = Some(ui.label(
                            match document.get_relation(self.id, hovered_task.unwrap()) {
                                TaskRelation::Unrelated => self.name.clone(),
                                TaskRelation::ChildOf => format!("{}\nDependent on", self.name),
                                TaskRelation::ParentOf => format!("{}\nParent task of", self.name),
                            },
                        ));
                    }

                    if label.unwrap().hovered() {
                        *hovered_task = Some(self.id);
                    }
                });
                ui.horizontal(|ui| {
                    let button = ui.button("Edit");
                    if button.clicked() {
                        action = SummaryAction::OpenEditor(self.id);
                    }
                    if ui.button("Add Child").clicked() {
                        action = SummaryAction::CreateChildOf(self.id)
                    }
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
                ScrollArea::vertical()
                    .id_source(4000000 + self.id)
                    .max_height(100.0)
                    .show(ui, |ui| ui.label(RichText::new(self.description.clone())));
                if ui.min_size().y < 200. {
                    ui.allocate_space(Vec2::new(ui.available_width(), 200. - ui.min_size().y));
                }
            });
        });
        action
    }
}
/**
Contains some methods to make determining relation easier.
*/
impl KanbanItem {
    pub fn is_child_of(&self, parent: &Self, document: &KanbanDocument) -> bool {
        let mut stack: Vec<KanbanId> = Vec::new();
        let mut seen: Vec<KanbanId> = Vec::new();
        stack.push(parent.id);
        while !stack.is_empty() {
            let current_id = stack.pop().unwrap();
            let item = document.get_task(current_id).unwrap();
            for child_id in item.child_tasks.iter() {
                if *child_id == self.id {
                    return true;
                }
                if !seen.contains(child_id) {
                    seen.push(*child_id);
                    stack.push(*child_id);
                }
            }
        }
        false
    }
}
/*
*/
pub mod search {
    use nucleo_matcher::{chars::normalize, pattern::Pattern, Config, Utf32Str, Utf32String};

    use super::KanbanId;

    #[derive(Clone)]
    pub struct SearchState {
        pub matched_ids: Vec<i32>,
        pub search_prompt: String,
        /**
        The former search prompt, if search_prompt and former_search_prompt are in disagreement
        the matched_ids must be rebuilt.
        */
        former_search_prompt: String,
        matcher: nucleo_matcher::Matcher,
        pattern: nucleo_matcher::pattern::Pattern,
    }

    impl SearchState {
        pub fn new() -> Self {
            SearchState {
                matched_ids: Vec::new(),
                search_prompt: String::new(),
                former_search_prompt: String::new(),
                matcher: nucleo_matcher::Matcher::new(Config::DEFAULT),
                pattern: Pattern::new(
                    "",
                    nucleo_matcher::pattern::CaseMatching::Smart,
                    nucleo_matcher::pattern::Normalization::Smart,
                    nucleo_matcher::pattern::AtomKind::Fuzzy,
                ),
            }
        }
        pub fn force_update(&mut self) {
            self.matched_ids.clear();
        }
        pub fn update(&mut self, document: &super::KanbanDocument) {
            // This is *kinda* expensive, so we should avoid it if possible.
            // The two conditions I can think of off the top of my head are that
            // if the search prompt is unchanged, and the matched_ids are not empty, then
            // we don't need to update.
            if self.search_prompt == self.former_search_prompt && !self.matched_ids.is_empty() {
                println!("I don't need to update :3");
                return;
            }
            if self.search_prompt != self.former_search_prompt {
                self.pattern.reparse(
                    &self.search_prompt,
                    nucleo_matcher::pattern::CaseMatching::Smart,
                    nucleo_matcher::pattern::Normalization::Smart,
                );
                self.former_search_prompt = self.search_prompt.clone();
            }
            self.matched_ids.clear();
            let mut thing: String = "".into();
            let mut utfs_buffer: Vec<char> = Vec::new();
            let mut values: Vec<(KanbanId, i32)> = Vec::new();
            for i in document.get_tasks() {
                thing.clear();
                i.into_searchable_string(&mut thing);

                if let Some(score) = self.pattern.score(
                    Utf32Str::new(&thing.as_str(), &mut utfs_buffer),
                    &mut self.matcher,
                ) {
                    values.push((i.id, score as i32));
                    // self.matched_ids.push(i.id);
                }
            }
            values.sort_by_key(|x| x.1);
            // println!("Top score: {}", values.first().unwrap().1);
            // values.reverse();
            self.matched_ids.extend(values.drain(..).map(|x| x.0));
            self.matched_ids.reverse();
            self.former_search_prompt = self.search_prompt.clone();
        }
    }
}
/**
 module for the queue_view, in this case, the cache state.
*/
pub mod queue_view {
    use super::*;
    #[derive(PartialEq, Eq, Clone)]
    pub struct QueueState {
        pub cached_ready: Vec<KanbanId>,
    }
    impl QueueState {
        pub fn new() -> Self {
            QueueState {
                cached_ready: Vec::new(),
            }
        }
        pub fn update(&mut self, document: &KanbanDocument) {
            let thing = document.get_tasks().map(|x| x.id);
            self.cached_ready.clear();
            self.cached_ready
                .extend(thing.filter(|x| document.task_status(x) == Status::Ready));
            self.cached_ready
                .sort_by_key(|x| document.task_priority_value(x));
            self.cached_ready.reverse();
        }
    }
}
/*
 * This is for the item editor. It requires a state object to be kept alive'
 * in order to avoid applying the changes instantaneously and making it uncomfortably
 * 'twitchy'
*/
pub mod editor {
    use super::{KanbanDocument, KanbanId, KanbanItem};
    use chrono::DateTime;
    use eframe::egui::{self, ComboBox};
    #[derive(Clone)]
    pub struct State {
        pub open: bool,
        pub cancelled: bool,
        pub item_copy: super::KanbanItem,
        selected_child: Option<KanbanId>,
        new_tag: String,
        category: String,
        priority: String,
    }
    pub fn state_from(item: &KanbanItem) -> State {
        State {
            open: true,
            cancelled: false,
            item_copy: item.clone(),
            selected_child: None,
            new_tag: "".into(),
            category: item.category.as_ref().unwrap_or(&String::new()).clone(),
            priority: item.priority.as_ref().unwrap_or(&String::new()).clone(),
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
        let mut open_task: Option<KanbanId> = None;
        let mut delete_task: Option<KanbanItem> = None;
        super::sorting::sort_completed_last(&document, &mut state.item_copy.child_tasks);
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
                ui.columns(2, |columns| {
                    columns[1].text_edit_singleline(&mut state.category);
                    ComboBox::new("Category", "Category")
                        .selected_text(&state.category)
                        .show_ui(&mut columns[0], |ui| {
                            ui.text_edit_singleline(&mut state.category);
                            for i in document.categories.keys() {
                                ui.selectable_value(&mut state.category, i.clone(), i.clone());
                            }
                        });
                });

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
                ui.columns(2, |columns| {
                    {
                        let ui = &mut columns[0];
                        ui.label("Child tasks");
                        let mut removed_task: Option<KanbanId> = None;
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
                    }
                    {
                        let ui = &mut columns[1];
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
                    }
                });
                /*
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
                */
                ui.horizontal(|ui| {
                    let accept_button = ui.button("Accept changes");
                    let cancel_button = ui.button("Cancel changes");
                    let delete_button = ui.button("Delete and close");
                    if accept_button.clicked() {
                        if !state.category.is_empty() {
                            state.item_copy.category = Some(state.category.clone());
                        } else {
                            state.item_copy.category = None;
                        }
                        if !state.priority.is_empty() {
                            state.item_copy.priority = Some(state.priority.clone());
                        } else {
                            state.item_copy.priority = None;
                        }
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
        a.child_tasks.push(c.id);
        document.replace_task(&a);
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
#[derive(Serialize, Deserialize, Default, PartialEq, Copy, Clone)]
pub struct KanbanCategoryStyle {
    pub panel_stroke_width: Option<f32>,
    pub panel_stroke_color: Option<[u8; 4]>,
    pub panel_fill: Option<[u8; 4]>,
    pub text_color: Option<[u8; 4]>,
}
impl KanbanCategoryStyle {
    pub fn apply_to(&self, stroke: &mut Stroke, panel_fill: &mut Color32) {
        if let Some(color) = self.panel_fill {
            *panel_fill = Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
        }
        if let Some(stroke_width) = self.panel_stroke_width {
            stroke.width = stroke_width;
        }
        if let Some(stroke_color) = self.panel_stroke_color {
            stroke.color = Color32::from_rgba_unmultiplied(
                stroke_color[0],
                stroke_color[1],
                stroke_color[2],
                stroke_color[3],
            );
        }
    }
}
