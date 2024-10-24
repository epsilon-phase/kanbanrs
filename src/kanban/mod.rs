use chrono::prelude::*;
use eframe::egui::{self, Color32, Margin, Response, RichText, ScrollArea, Stroke, Vec2};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::btree_map::{Values, ValuesMut};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use time_tracking::TimeRecords;
use undo::{DeletionEvent, UndoItem};
pub mod category_editor;
pub mod filter;
pub mod focused_layout;
pub mod node_layout;
pub mod priority_editor;
pub mod sorting;
pub mod time_tracking;
pub mod tree_outline_layout;
pub mod undo;

pub type KanbanId = i32;

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Status {
    Blocked,
    Completed,
    Ready,
}
#[derive(Default, Serialize, Deserialize)]
pub struct KanbanDocument {
    tasks: BTreeMap<KanbanId, KanbanItem>,
    priorities: HashMap<String, i32>,
    categories: HashMap<String, KanbanCategoryStyle>,
    next_id: RwLock<KanbanId>,
}
impl Clone for KanbanDocument {
    fn clone(&self) -> Self {
        let mut r = Self::new();
        r.clone_from(self);
        r
    }
    fn clone_from(&mut self, source: &Self) {
        self.tasks = source.tasks.clone();
        self.categories = source.categories.clone();
        self.priorities = source.priorities.clone();
        *self.next_id.write() = *source.next_id.read();
    }
}
impl KanbanDocument {
    pub fn new() -> Self {
        KanbanDocument {
            tasks: BTreeMap::new(),
            priorities: HashMap::from([
                ("High".to_owned(), 10),
                ("Medium".to_owned(), 5),
                ("Low".to_owned(), 1),
            ]),
            categories: HashMap::new(),
            next_id: RwLock::new(0),
        }
    }
    /** Determine if the child can be added to the parent's dependency list without
       causing a cycle
    */
    pub fn can_add_as_child(&self, parent: &KanbanItem, child: &KanbanItem) -> bool {
        if parent.id == child.id {
            return false;
        }
        let mut stack: Vec<KanbanId> = Vec::new();
        let mut seen: Vec<KanbanId> = Vec::new();
        stack.push(child.id);
        let mut found = false;
        while let Some(current) = stack.pop() {
            // We can be sure that it won't return a nonopt because of the loop's precondition

            if current == parent.id && !seen.is_empty() {
                found = true;
                break;
            }
            seen.push(current);
            // Either parent or child may be a hypothetical; not yet committed to the document,
            // and thus needs to be intercepted here to ensure up-to-dateness
            let task = if current == parent.id {
                parent
            } else if current == child.id {
                child
            } else {
                &self.tasks[&current]
            };
            task.child_tasks.iter().for_each(|child_id| {
                if seen.contains(child_id) {
                    return;
                }
                stack.push(*child_id);
            });
        }
        !found
    }
    pub fn get_next_id(&self) -> KanbanId {
        let next = *self.next_id.read();
        let start = if next == KanbanId::MAX {
            println!("I'm at the highest!");
            KanbanId::MIN
        } else {
            next
        };

        if self.tasks.contains_key(&(start + 1)) {
            // If this ever trips, well, congrats on completing billions of tasks? I guess.
            // Assuming this is running at 60 fps, this should take over 1800 hours of adding
            // tasks every single frame.
            *self.next_id.write() = (start..KanbanId::MAX)
                .find(|x| !self.tasks.contains_key(x))
                .unwrap();
        } else {
            *self.next_id.write() = start + 1;
        }
        next
    }
    /**
    Create a new task and add it to the document, returning a mutable reference
    */
    pub fn get_new_task_mut(&mut self) -> &mut KanbanItem {
        let new_task = KanbanItem::new(self);
        let new_task_id = new_task.id;
        self.tasks.insert(new_task_id, new_task);
        return self.tasks.get_mut(&new_task_id).unwrap();
    }
    pub fn get_new_task(&mut self) -> KanbanItem {
        let new_task = KanbanItem::new(self);
        let new_task_id = new_task.id;
        self.tasks.insert(new_task_id, new_task);
        return self.tasks.get(&new_task_id).unwrap().clone();
    }
    pub fn get_tasks(&'_ self) -> Values<'_, KanbanId, KanbanItem> {
        self.tasks.values()
    }
    pub fn get_tasks_mut(&'_ mut self) -> ValuesMut<'_, KanbanId, KanbanItem> {
        self.tasks.values_mut()
    }
    pub fn task_status(&self, id: &KanbanId) -> Status {
        match self.tasks[id].completed {
            Some(_) => Status::Completed,
            None => {
                if self.tasks[id]
                    .child_tasks
                    .iter()
                    .all(|child_id| self.task_status(child_id) == Status::Completed)
                {
                    Status::Ready
                } else {
                    Status::Blocked
                }
            }
        }
    }
    pub fn replace_task(&mut self, item: &KanbanItem) -> UndoItem {
        let result = if let Some(old) = self.tasks.insert(item.id, item.clone()) {
            UndoItem::Modification(undo::ModificationEvent { former_item: old })
        } else {
            UndoItem::Create(undo::CreationEvent {
                parent_id: None,
                new_task: item.clone(),
            })
        };
        if item.category.is_some()
            && !self
                .categories
                .contains_key(item.category.as_ref().unwrap())
        {
            self.categories.insert(
                item.category.as_ref().unwrap().clone(),
                KanbanCategoryStyle::default(),
            );
        }
        result
    }
    pub fn get_sorted_priorities<'a>(&'a self) -> Vec<(&'a String, &'a i32)> {
        let mut i: Vec<(&'a String, &'a i32)> = self.priorities.iter().collect();
        i.sort_by(|a, b| a.1.cmp(b.1));
        i
    }
    pub fn get_task(&self, id: KanbanId) -> Option<&KanbanItem> {
        self.tasks.get(&id)
    }
    pub fn remove_task(&mut self, item: &KanbanItem) -> undo::UndoItem {
        let mut result = Vec::new();
        for i in self.tasks.values_mut() {
            if i.remove_child(item) {
                result.push(i.id)
            }
        }
        self.tasks.remove(&item.id);
        undo::UndoItem::Delete(DeletionEvent {
            parent_ids: result,
            former_item: item.clone(),
        })
    }
    pub fn get_relation(&self, target: KanbanId, other: KanbanId) -> TaskRelation {
        if target == other {
            return TaskRelation::TheItemItself;
        }
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
        0
    }
    /// Operate on the whole of the tree down from this point
    ///
    /// This will visit the same nodes multiple times; try not to worry too much.
    ///
    /// * `root_id` The starting point for this tree
    /// * `depth` The starting depth
    /// * `func` The function to call on each child.
    ///
    pub fn on_tree<F>(&self, root_id: KanbanId, depth: u32, mut func: F)
    where
        F: FnMut(&Self, KanbanId, u32),
    {
        let mut stack = Vec::new();
        stack.push((root_id, depth));
        while let Some((id, depth)) = stack.pop() {
            let task = self.get_task(id).unwrap();
            func(self, id, depth);
            for child in task.child_tasks.iter() {
                stack.push((*child, depth + 1));
            }
        }
    }
    pub fn parents_of(&'_ self, id: KanbanId) -> Vec<&'_ KanbanItem> {
        self.tasks
            .values()
            .filter(|possible_parent| possible_parent.child_tasks.contains(&id))
            .collect()
    }
    pub fn get_task_mut(&mut self, id: KanbanId) -> Option<&mut KanbanItem> {
        self.tasks.get_mut(&id)
    }
}
/// Category functions
impl KanbanDocument {
    pub fn replace_category_style(&mut self, name: &str, style: KanbanCategoryStyle) {
        self.categories.insert(name.into(), style);
    }
}
pub mod layout_cache {
    use super::*;
    use std::{borrow::Borrow, cell::RefCell};
    thread_local! {
        static AVERAGE_CACHE:RefCell<HashMap<egui::Id,(f64,f64)>>=RefCell::new(HashMap::new());

    }
    pub fn get_average_item_height(id: egui::Id) -> f64 {
        let map = AVERAGE_CACHE.with(|x| x.borrow().get(&id).copied());
        if let Some((height, count)) = map {
            height / count
        } else {
            0.0
        }
    }
    pub fn record_measurement(id: egui::Id, height: f64) {
        AVERAGE_CACHE.with_borrow_mut(|x| {
            let (cached_height, count) = x.entry(id).or_insert((50.0, 1.0));
            let avg = *cached_height / *count;

            if (height - avg).abs() / height > 0.2 {
                *cached_height += height;
                *count += 1.0;
                println!("Average height for id '{}': {:.2}", id.value(), avg);
            }
        });
    }
}
impl KanbanDocument {
    //! Produce a vertical layout scrolling downwards.
    //!
    //! * `self` - the document, you silly goose
    //! * `ui` - The ui to apply this list into
    //! * `range` - the range of indices to render
    //! * `hovered_task` - The task being hovered over by the user, may be set here
    //! * `event_collector` - The list of actions being collected.
    pub fn layout_id_list(
        &self,
        ui: &mut egui::Ui,
        ids: &[KanbanId],
        hovered_task: &mut Option<i32>,
        event_collector: &mut Vec<SummaryAction>,
        id_salt: impl std::hash::Hash,
    ) -> f64 {
        let cache_key = egui::Id::new(&id_salt);
        egui::ScrollArea::vertical().id_salt(id_salt).show_rows(
            ui,
            layout_cache::get_average_item_height(cache_key) as f32,
            ids.len(),
            |ui, range| {
                ui.vertical_centered_justified(|ui| {
                    for row in range.clone() {
                        let start = ui.cursor().min.y;
                        let item_id = ids[row];
                        let item = &self.tasks[&item_id];
                        let action = item.summary(self, hovered_task, ui);
                        event_collector.push(action);
                        let end = ui.cursor().min.y;
                        layout_cache::record_measurement(cache_key, (end - start) as f64);
                    }
                });
            },
        );
        layout_cache::get_average_item_height(cache_key)
    }
}
#[derive(PartialEq, Eq)]
pub enum TaskRelation {
    Unrelated,
    ChildOf,
    ParentOf,
    TheItemItself,
}
#[derive(Default, Clone, Serialize, Deserialize, Debug)]
pub struct KanbanItem {
    pub id: KanbanId,
    pub name: String,
    pub description: String,
    pub completed: Option<DateTime<Utc>>,
    pub category: Option<String>,
    pub priority: Option<String>,
    pub tags: Vec<String>,
    pub child_tasks: BTreeSet<KanbanId>,
    #[serde(default)]
    pub time_records: TimeRecords,
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
            child_tasks: BTreeSet::new(),
            time_records: Default::default(),
        }
    }

    /// Apply the category from the parent as specified in the category's preferences.
    ///
    /// * `self` - The item in to inherit
    /// * `parent` - The parent
    /// * `document` - The document
    pub fn inherit(&mut self, parent: &KanbanItem, document: &KanbanDocument) {
        if parent.category.is_none() {
            return;
        }
        let category = parent.category.as_ref().unwrap();
        if let Some(style) = document.categories.get(category) {
            if style.children_inherit_category {
                self.category = parent.category.clone();
            }
        }
    }
    pub fn add_child(&mut self, child: &Self) {
        if !self.child_tasks.contains(&child.id) {
            self.child_tasks.insert(child.id);
        }
    }
    pub fn get_completed_time_string(&self) -> Option<String> {
        if let Some(completion_time) = self.completed {
            let current_time = Utc::now();
            let difference = current_time - completion_time;
            if difference.num_days() > 7 {
                let local: DateTime<chrono::Local> = completion_time.into();
                Some(format!("on {}", local))
            } else {
                let diff_str;
                if difference.num_days() >= 1 {
                    diff_str = format!(
                        "{} days, {} hours ago",
                        difference.num_days(),
                        difference.num_hours() % 24
                    );
                } else if difference.num_hours() >= 1 {
                    diff_str = format!(
                        "{} hour, {}minutes ago",
                        difference.num_hours(),
                        difference.num_minutes() % 60
                    );
                } else {
                    diff_str = format!("{} minutes ago", difference.num_minutes());
                }
                Some(diff_str)
            }
        } else {
            None
        }
    }
    // Remove a child from the task, returning true if it was present
    pub fn remove_child(&mut self, other: &Self) -> bool {
        let mut found = false;
        self.child_tasks.retain(|x| {
            let t = *x == other.id;
            if t {
                found = true;
                false
            } else {
                true
            }
        });
        found
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
    /// * `output` - The output string buffer. For reuse.
    pub fn fill_searchable_buffer(&self, output: &mut String) {
        output.push_str(&self.name);
        output.push(' ');
        output.push_str(self.category.as_deref().unwrap_or(""));
        output.push(' ');
        output.push_str(self.priority.as_deref().unwrap_or(""));
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
    MarkCompleted(KanbanId),
    FocusOn(KanbanId),
    AddChildTo(KanbanId, KanbanId),
    UpdateLayout,
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
        let mut name_color = style.text_color();
        // Get the custom color for the category
        if self.category.is_some() {
            if let Some(category_style) = document.categories.get(self.category.as_ref().unwrap()) {
                category_style.apply_to(&mut stroke, &mut panel_fill, &mut name_color);
            }
        }
        match document.task_status(&self.id) {
            Status::Blocked => {
                status_color = Color32::from_rgba_unmultiplied(200, 0, 0, 255);
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
                        label = Some(ui.label(RichText::new(self.name.clone()).color(name_color)));
                    } else {
                        label = Some(ui.label(
                            match document.get_relation(self.id, hovered_task.unwrap()) {
                                TaskRelation::Unrelated | TaskRelation::TheItemItself => {
                                    self.name.clone()
                                }
                                TaskRelation::ChildOf => format!("{}\nDependent on", self.name),
                                TaskRelation::ParentOf => format!("{}\nParent task of", self.name),
                            },
                        ));
                    }

                    if label.as_ref().unwrap().hovered() {
                        *hovered_task = Some(self.id);
                    }
                    if label.as_ref().unwrap().middle_clicked() {
                        action = SummaryAction::FocusOn(self.id);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    let button = ui.button("Edit");
                    if button.clicked() {
                        action = SummaryAction::OpenEditor(self.id);
                        ui.close_menu();
                    }
                    if ui.button("Add Child").clicked() {
                        action = SummaryAction::CreateChildOf(self.id);
                        ui.close_menu();
                    }
                    if ui
                        .button(if self.completed.is_some() {
                            "Uncomplete"
                        } else {
                            "Complete"
                        })
                        .clicked()
                    {
                        action = SummaryAction::MarkCompleted(self.id);
                        ui.close_menu();
                    }
                    if ui.button("focus").clicked() {
                        action = SummaryAction::FocusOn(self.id);
                        ui.close_menu();
                    }
                });
                ui.horizontal(|ui| {
                    let thing = match self.completed {
                        Some(_) => {
                            format!("Completed {}", self.get_completed_time_string().unwrap())
                        }
                        None => "Not completed".into(),
                    };
                    ui.label(RichText::new(thing).color(status_color).strong());
                });
                ScrollArea::vertical()
                    .id_salt(format!("Summary for item {}", self.id))
                    .max_height(50.0)
                    .show(ui, |ui| ui.label(RichText::new(self.description.clone())));
                // if ui.min_size().y < 200. {
                //     ui.allocate_space(Vec2::new(ui.available_width(), 200. - ui.min_size().y));
                // }
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
        while let Some(current_id) = stack.pop() {
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
    use nucleo_matcher::{pattern::Pattern, Config, Utf32Str};

    use super::KanbanId;

    #[derive(Clone, Default)]
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
                i.fill_searchable_buffer(&mut thing);

                if let Some(score) = self.pattern.score(
                    Utf32Str::new(thing.as_str(), &mut utfs_buffer),
                    &mut self.matcher,
                ) {
                    values.push((i.id, score as i32));
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
    impl Default for QueueState {
        fn default() -> Self {
            Self::new()
        }
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
pub mod editor;
#[cfg(test)]
pub mod tests {
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
            .insert(b_id);
        assert!(!document.can_add_as_child(&document.tasks[&b_id], &document.tasks[&a_id]));
        assert!(document.can_add_as_child(&document.tasks[&c_id], &document.tasks[&a_id]));
    }
    #[test]
    fn test_task_removal() {
        let mut document = KanbanDocument::new();
        let mut a = document.get_new_task_mut().clone();
        document.get_new_task_mut();
        let c = document.get_new_task_mut().clone();
        a.add_child(&c);
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
    /**
    Make a KanbanDocument easily.

    * `number_of_tasks` - The number of tasks to populate the document with.
    * `children` - The ids of each child in order. If the vector ends prior to the last task,
      it assumes none of the following tasks have children. Assume that ids start from 0 and end at number_of_tasks
    */
    pub fn make_document_easy(
        number_of_tasks: usize,
        children: &[Vec<KanbanId>],
    ) -> KanbanDocument {
        let mut n = KanbanDocument::new();
        let mut ids = Vec::new();
        for _ in 0..number_of_tasks {
            ids.push(n.get_new_task_mut().id);
        }
        for (index, child_set) in ids.iter().zip(children.iter()) {
            let mut task = n.get_task(*index).unwrap().clone();
            for child_id in child_set.iter() {
                task.child_tasks.insert(*child_id);
            }
            n.replace_task(&task);
        }
        n
    }
    #[test]
    fn test_inheritance() {
        let mut document = KanbanDocument::new();
        document.categories.insert(
            "test_true".to_owned(),
            KanbanCategoryStyle {
                children_inherit_category: true,
                ..Default::default()
            },
        );
        document.categories.insert(
            "test_false".to_owned(),
            KanbanCategoryStyle {
                children_inherit_category: false,
                ..Default::default()
            },
        );
        {
            let a_id: KanbanId;
            {
                let task_a = document.get_new_task_mut();
                task_a.category = Some("test_true".to_owned());
                a_id = task_a.id;
            };
            {
                let mut task_b = document.get_new_task();
                task_b.inherit(document.get_task(a_id).unwrap(), &document);
                assert_eq!(task_b.category, Some("test_true".to_owned()));
            }
        }
        // It should of course, not apply on categories with the inheritance set to false
        {
            let mut task_a = document.get_new_task();
            task_a.category = Some("test_false".to_owned());
            document.replace_task(&task_a);
            let mut task_b = document.get_new_task();
            task_b.inherit(&task_a, &document);
            assert_eq!(task_b.category, None);
        }
    }
    mod queue_state_tests {
        use queue_view::QueueState;

        use super::super::*;
        use super::*;
        #[test]
        fn test_queue_state() {
            let children = vec![vec![4], vec![0], vec![1]];
            let mut document = make_document_easy(5, &children);

            // This honestly might be better to extract into a test for the document itself
            let mut task = document.get_task(4).unwrap().clone();
            task.completed = Some(chrono::Utc::now());

            document.replace_task(&task);
            assert_eq!(document.get_task(1).unwrap().child_tasks.len(), 1);
            let mut qs = QueueState::new();
            qs.update(&document);
            assert_eq!(qs.cached_ready.len(), 2);
            assert!(qs.cached_ready.contains(&0));
            assert!(qs.cached_ready.contains(&3));
            // Make sure that every task that it collects is ready.
            for task in qs
                .cached_ready
                .iter()
                .map(|x| document.get_task(*x).unwrap())
            {
                assert_eq!(document.task_status(&task.id), Status::Ready);
            }
        }
    }
}
#[derive(Serialize, Deserialize, Default, PartialEq, Copy, Clone)]
pub struct KanbanCategoryStyle {
    pub panel_stroke_width: Option<f32>,
    pub panel_stroke_color: Option<[u8; 4]>,
    pub panel_fill: Option<[u8; 4]>,
    pub text_color: Option<[u8; 4]>,
    #[serde(default)]
    pub children_inherit_category: bool,
}
impl KanbanCategoryStyle {
    pub fn apply_to(
        &self,
        stroke: &mut Stroke,
        panel_fill: &mut Color32,
        text_color: &mut Color32,
    ) {
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
        if let Some(this_text_color) = self.text_color {
            *text_color = Color32::from_rgba_premultiplied(
                this_text_color[0],
                this_text_color[1],
                this_text_color[2],
                this_text_color[3],
            );
        }
    }
}
