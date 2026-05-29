use chrono::Utc;
use circular_buffer::CircularBuffer;

use super::{
    undo::{CategoryStyleEvent, CreationEvent, PriorityEvent, UndoItem},
    KanbanCategoryStyle, KanbanDocument, KanbanId, KanbanItem,
};

const UNDO_LIMIT: usize = 35;

/// Document-changing commands handled by the kanban application module.
#[derive(Clone, Debug)]
pub enum ApplicationCommand {
    /// Create a top-level task with the provided name.
    CreateRootTask { name: String },
    /// Create a new child task on this one.
    CreateChildOf(KanbanId),
    /// Create a new task as a child (parent, new_child) from an editor viewport.
    CreateTask(KanbanItem, KanbanItem),
    /// Replace a task in the document with an updated version.
    UpdateTask(KanbanItem),
    /// Delete the specified kanban item from the document.
    DeleteTask(KanbanItem),
    /// Mark the specified task as completed, or uncomplete it.
    MarkCompleted(KanbanId),
    /// Add an extant task as a child to another task.
    AddChildTo(KanbanId, KanbanId),
    /// Toggle time recording for a given item.
    FinishTimeRecording(KanbanId),
    /// Set or replace a category's style.
    ReplaceCategory(String, KanbanCategoryStyle),
    /// Set a priority's numeric value.
    SetPriority(String, i32),
}

/// App-shell work caused by an application command.
#[derive(Clone, Debug, PartialEq)]
pub enum ApplicationEffect {
    /// Open an editor for the task.
    OpenEditor(KanbanItem),
    /// Remove a deleted task from any open editor copies.
    RemoveDeletedTaskFromEditors(KanbanItem),
    /// Rebuild the current layout cache.
    UpdateLayout,
    /// Tell layout state that the document item set changed.
    InformOfNewItems,
}

/// State owned by application command handling.
pub struct ApplicationState {
    undo_buffer: CircularBuffer<UNDO_LIMIT, UndoItem>,
    modified_since_last_saved: bool,
}

impl Default for ApplicationState {
    fn default() -> Self {
        Self::new()
    }
}

impl ApplicationState {
    pub fn new() -> Self {
        Self {
            undo_buffer: CircularBuffer::new(),
            modified_since_last_saved: false,
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_buffer.is_empty()
    }

    pub fn modified_since_last_saved(&self) -> bool {
        self.modified_since_last_saved
    }

    pub fn mark_saved(&mut self) {
        self.modified_since_last_saved = false;
    }

    pub fn handle_command(
        &mut self,
        document: &mut KanbanDocument,
        command: ApplicationCommand,
    ) -> Vec<ApplicationEffect> {
        match command {
            ApplicationCommand::CreateRootTask { name } => self.create_root_task(document, name),
            ApplicationCommand::CreateChildOf(id) => self.create_child_of(document, id),
            ApplicationCommand::CreateTask(parent, new_task) => {
                self.create_task(document, parent, new_task)
            }
            ApplicationCommand::UpdateTask(item) => self.update_task(document, item),
            ApplicationCommand::DeleteTask(to_delete) => self.delete_task(document, to_delete),
            ApplicationCommand::MarkCompleted(id) => self.mark_completed(document, id),
            ApplicationCommand::AddChildTo(parent, child) => {
                self.add_child_to(document, parent, child)
            }
            ApplicationCommand::FinishTimeRecording(id) => self.finish_time_recording(document, id),
            ApplicationCommand::ReplaceCategory(name, style) => {
                self.replace_category(document, name, style)
            }
            ApplicationCommand::SetPriority(name, value) => {
                self.set_priority(document, name, value)
            }
        }
    }

    pub fn undo(&mut self, document: &mut KanbanDocument) -> Vec<ApplicationEffect> {
        if let Some(item) = self.undo_buffer.pop_back() {
            item.undo(document);
            vec![ApplicationEffect::UpdateLayout]
        } else {
            Vec::new()
        }
    }

    fn create_root_task(
        &mut self,
        document: &mut KanbanDocument,
        name: String,
    ) -> Vec<ApplicationEffect> {
        let task = document.get_new_task_mut();
        task.name = name;
        let new_task = task.clone();
        self.record_undo(UndoItem::Create(CreationEvent {
            new_task: new_task.clone(),
            parent_id: None,
        }));
        self.modified_since_last_saved = true;
        vec![
            ApplicationEffect::UpdateLayout,
            ApplicationEffect::InformOfNewItems,
            ApplicationEffect::OpenEditor(new_task),
        ]
    }

    fn create_child_of(
        &mut self,
        document: &mut KanbanDocument,
        id: KanbanId,
    ) -> Vec<ApplicationEffect> {
        let mut new_task = document.get_new_task();
        let mut task_copy = document.get_task(id).unwrap().clone();
        new_task.inherit(&task_copy, document);
        let child_creation = document.replace_task(&new_task);
        task_copy.add_child(&new_task);
        self.undo_buffer
            .push_back(document.replace_task(&task_copy));
        self.record_undo(child_creation);
        self.modified_since_last_saved = true;
        vec![
            ApplicationEffect::OpenEditor(new_task),
            ApplicationEffect::UpdateLayout,
            ApplicationEffect::InformOfNewItems,
        ]
    }

    fn create_task(
        &mut self,
        document: &mut KanbanDocument,
        parent: KanbanItem,
        mut new_task: KanbanItem,
    ) -> Vec<ApplicationEffect> {
        new_task.inherit(&parent, document);
        let undo = document.replace_task(&new_task);
        self.record_undo(undo);
        self.modified_since_last_saved = true;
        vec![
            ApplicationEffect::OpenEditor(new_task),
            ApplicationEffect::UpdateLayout,
            ApplicationEffect::InformOfNewItems,
        ]
    }

    fn update_task(
        &mut self,
        document: &mut KanbanDocument,
        item: KanbanItem,
    ) -> Vec<ApplicationEffect> {
        self.modified_since_last_saved = if let Some(existing) = document.get_task(item.id) {
            existing != &item
        } else {
            true
        };
        let undo = document.replace_task(&item);
        self.record_undo(undo);
        if self.modified_since_last_saved {
            vec![ApplicationEffect::UpdateLayout]
        } else {
            Vec::new()
        }
    }

    fn delete_task(
        &mut self,
        document: &mut KanbanDocument,
        to_delete: KanbanItem,
    ) -> Vec<ApplicationEffect> {
        let undo = document.remove_task(&to_delete);
        self.record_undo(undo);
        self.modified_since_last_saved = true;
        vec![
            ApplicationEffect::RemoveDeletedTaskFromEditors(to_delete),
            ApplicationEffect::UpdateLayout,
            ApplicationEffect::InformOfNewItems,
        ]
    }

    fn mark_completed(
        &mut self,
        document: &mut KanbanDocument,
        id: KanbanId,
    ) -> Vec<ApplicationEffect> {
        let mut task = document.get_task(id).unwrap().clone();
        task.completed = match task.completed {
            Some(_) => None,
            None => Some(Utc::now()),
        };
        let undo = document.replace_task(&task);
        self.record_undo(undo);
        self.modified_since_last_saved = true;
        vec![ApplicationEffect::UpdateLayout]
    }

    fn add_child_to(
        &mut self,
        document: &mut KanbanDocument,
        parent: KanbanId,
        child: KanbanId,
    ) -> Vec<ApplicationEffect> {
        if document.can_add_as_child(
            document.get_task(parent).unwrap(),
            document.get_task(child).unwrap(),
        ) {
            let mut task = document.get_task(parent).unwrap().clone();
            task.child_tasks.insert(child);
            let undo = document.replace_task(&task);
            self.record_undo(undo);
        }
        self.modified_since_last_saved = true;
        vec![ApplicationEffect::UpdateLayout]
    }

    fn finish_time_recording(
        &mut self,
        document: &mut KanbanDocument,
        id: KanbanId,
    ) -> Vec<ApplicationEffect> {
        let task = document.get_task_mut(id).unwrap();
        task.time_records.handle_record_request(None);
        self.modified_since_last_saved = true;
        Vec::new()
    }

    fn replace_category(
        &mut self,
        document: &mut KanbanDocument,
        name: String,
        style: KanbanCategoryStyle,
    ) -> Vec<ApplicationEffect> {
        let former_style = document.get_category_style(&name);
        document.replace_category_style(&name, style);
        self.record_undo(UndoItem::CategoryStyle(CategoryStyleEvent {
            name,
            former_style,
        }));
        self.modified_since_last_saved = true;
        Vec::new()
    }

    fn set_priority(
        &mut self,
        document: &mut KanbanDocument,
        name: String,
        value: i32,
    ) -> Vec<ApplicationEffect> {
        let former_value = document
            .get_sorted_priorities()
            .into_iter()
            .find(|(n, _)| *n == &name)
            .map(|(_, v)| *v);
        document.set_priority(name.clone(), value);
        self.record_undo(UndoItem::Priority(PriorityEvent { name, former_value }));
        self.modified_since_last_saved = true;
        vec![ApplicationEffect::UpdateLayout]
    }

    fn record_undo(&mut self, item: UndoItem) {
        if let Some(last) = self.undo_buffer.back_mut() {
            if let Some(combined) = last.merge(&item) {
                *last = combined;
            } else {
                self.undo_buffer.push_back(item);
            }
        } else {
            self.undo_buffer.push_back(item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_root_task_records_undo_and_requests_app_shell_work() {
        let mut state = ApplicationState::new();
        let mut document = KanbanDocument::new();

        let effects = state.handle_command(
            &mut document,
            ApplicationCommand::CreateRootTask {
                name: "Task".to_owned(),
            },
        );

        let created = document.get_tasks().next().unwrap().clone();
        assert_eq!(created.name, "Task");
        assert!(state.modified_since_last_saved());
        assert!(state.can_undo());
        assert_eq!(
            effects,
            vec![
                ApplicationEffect::UpdateLayout,
                ApplicationEffect::InformOfNewItems,
                ApplicationEffect::OpenEditor(created.clone()),
            ]
        );

        state.undo(&mut document);
        assert!(document.get_task(created.id).is_none());
    }

    #[test]
    fn mark_completed_toggles_completion() {
        let mut state = ApplicationState::new();
        let mut document = KanbanDocument::new();
        let id = document.get_new_task().id;

        state.handle_command(&mut document, ApplicationCommand::MarkCompleted(id));
        assert!(document.get_task(id).unwrap().completed.is_some());

        state.handle_command(&mut document, ApplicationCommand::MarkCompleted(id));
        assert!(document.get_task(id).unwrap().completed.is_none());
    }
}
