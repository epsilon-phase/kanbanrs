use chrono::Utc;
use circular_buffer::CircularBuffer;

use super::{
    undo::{CategoryStyleEvent, CreationEvent, PriorityEvent, RenameEvent, UndoItem},
    AppShellCommand, KanbanCategoryStyle, KanbanDocument, KanbanId, KanbanItem,
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
    /// Rename a priority and everything referencing it.
    RenamePriority { from: String, to: String },
    /// Rename a category and everything referencing it.
    RenameCategory { from: String, to: String },
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
    ) -> Vec<AppShellCommand> {
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
            ApplicationCommand::RenamePriority { from, to } => {
                self.rename_priority(document, from, to)
            }
            ApplicationCommand::RenameCategory { from, to } => {
                self.rename_category(document, from, to)
            }
        }
    }

    pub fn undo(&mut self, document: &mut KanbanDocument) -> Vec<AppShellCommand> {
        if let Some(item) = self.undo_buffer.pop_back() {
            // Rename undos change references too, so open editors must be
            // told about the swap-back just like the forward rename.
            let mut effects = vec![AppShellCommand::UpdateLayout];
            match &item {
                UndoItem::RenamePriority(re) => {
                    effects.push(AppShellCommand::RenamedPriority {
                        from: re.to.clone(),
                        to: re.from.clone(),
                    });
                }
                UndoItem::RenameCategory(re) => {
                    effects.push(AppShellCommand::RenamedCategory {
                        from: re.to.clone(),
                        to: re.from.clone(),
                    });
                }
                _ => {}
            }
            item.undo(document);
            effects
        } else {
            Vec::new()
        }
    }

    fn create_root_task(
        &mut self,
        document: &mut KanbanDocument,
        name: String,
    ) -> Vec<AppShellCommand> {
        let task = document.get_new_task_mut();
        task.name = name;
        let new_task = task.clone();
        self.record_undo(UndoItem::Create(CreationEvent {
            new_task: new_task.clone(),
            parent_id: None,
        }));
        self.modified_since_last_saved = true;
        vec![
            AppShellCommand::UpdateLayout,
            AppShellCommand::InformOfNewItems,
            AppShellCommand::OpenTask(new_task),
        ]
    }

    fn create_child_of(
        &mut self,
        document: &mut KanbanDocument,
        id: KanbanId,
    ) -> Vec<AppShellCommand> {
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
            AppShellCommand::OpenTask(new_task),
            AppShellCommand::UpdateLayout,
            AppShellCommand::InformOfNewItems,
        ]
    }

    fn create_task(
        &mut self,
        document: &mut KanbanDocument,
        parent: KanbanItem,
        mut new_task: KanbanItem,
    ) -> Vec<AppShellCommand> {
        new_task.inherit(&parent, document);
        let undo = document.replace_task(&new_task);
        self.record_undo(undo);
        self.modified_since_last_saved = true;
        vec![
            AppShellCommand::OpenTask(new_task),
            AppShellCommand::UpdateLayout,
            AppShellCommand::InformOfNewItems,
        ]
    }

    fn update_task(
        &mut self,
        document: &mut KanbanDocument,
        item: KanbanItem,
    ) -> Vec<AppShellCommand> {
        self.modified_since_last_saved = if let Some(existing) = document.get_task(item.id) {
            existing != &item
        } else {
            true
        };
        let undo = document.replace_task(&item);
        self.record_undo(undo);
        if self.modified_since_last_saved {
            vec![AppShellCommand::UpdateLayout]
        } else {
            Vec::new()
        }
    }

    fn delete_task(
        &mut self,
        document: &mut KanbanDocument,
        to_delete: KanbanItem,
    ) -> Vec<AppShellCommand> {
        let undo = document.remove_task(&to_delete);
        self.record_undo(undo);
        self.modified_since_last_saved = true;
        vec![
            AppShellCommand::RemoveDeletedTaskFromEditors(to_delete),
            AppShellCommand::UpdateLayout,
            AppShellCommand::InformOfNewItems,
        ]
    }

    fn mark_completed(
        &mut self,
        document: &mut KanbanDocument,
        id: KanbanId,
    ) -> Vec<AppShellCommand> {
        let mut task = document.get_task(id).unwrap().clone();
        task.completed = match task.completed {
            Some(_) => None,
            None => Some(Utc::now()),
        };
        let undo = document.replace_task(&task);
        self.record_undo(undo);
        self.modified_since_last_saved = true;
        vec![AppShellCommand::UpdateLayout]
    }

    fn add_child_to(
        &mut self,
        document: &mut KanbanDocument,
        parent: KanbanId,
        child: KanbanId,
    ) -> Vec<AppShellCommand> {
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
        vec![AppShellCommand::UpdateLayout]
    }

    fn finish_time_recording(
        &mut self,
        document: &mut KanbanDocument,
        id: KanbanId,
    ) -> Vec<AppShellCommand> {
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
    ) -> Vec<AppShellCommand> {
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
    ) -> Vec<AppShellCommand> {
        let former_value = document
            .get_sorted_priorities()
            .into_iter()
            .find(|(n, _)| *n == &name)
            .map(|(_, v)| *v);
        document.set_priority(name.clone(), value);
        self.record_undo(UndoItem::Priority(PriorityEvent { name, former_value }));
        self.modified_since_last_saved = true;
        vec![AppShellCommand::UpdateLayout]
    }

    fn rename_priority(
        &mut self,
        document: &mut KanbanDocument,
        from: String,
        to: String,
    ) -> Vec<AppShellCommand> {
        if !document.rename_priority(&from, &to) {
            return Vec::new();
        }
        self.record_undo(UndoItem::RenamePriority(RenameEvent {
            from: from.clone(),
            to: to.clone(),
        }));
        self.modified_since_last_saved = true;
        vec![
            AppShellCommand::UpdateLayout,
            AppShellCommand::RenamedPriority { from, to },
        ]
    }

    fn rename_category(
        &mut self,
        document: &mut KanbanDocument,
        from: String,
        to: String,
    ) -> Vec<AppShellCommand> {
        if !document.rename_category(&from, &to) {
            return Vec::new();
        }
        self.record_undo(UndoItem::RenameCategory(RenameEvent {
            from: from.clone(),
            to: to.clone(),
        }));
        self.modified_since_last_saved = true;
        vec![
            AppShellCommand::UpdateLayout,
            AppShellCommand::RenamedCategory { from, to },
        ]
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
                AppShellCommand::UpdateLayout,
                AppShellCommand::InformOfNewItems,
                AppShellCommand::OpenTask(created.clone()),
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

    #[test]
    fn rename_priority_cascades_and_undoes() {
        let mut state = ApplicationState::new();
        let mut document = KanbanDocument::new();
        let id = document.get_new_task().id;
        let mut task = document.get_task(id).unwrap().clone();
        task.priority = Some("High".into());
        document.replace_task(&task);

        state.handle_command(
            &mut document,
            ApplicationCommand::RenamePriority {
                from: "High".to_owned(),
                to: "Critical".to_owned(),
            },
        );
        assert_eq!(
            document.get_task(id).unwrap().priority,
            Some("Critical".into())
        );
        assert_eq!(document.task_priority_value(&id), 10);
        assert!(document
            .get_sorted_priorities()
            .iter()
            .any(|(name, _)| *name == "Critical"));
        assert!(state.modified_since_last_saved());

        state.undo(&mut document);
        assert_eq!(document.get_task(id).unwrap().priority, Some("High".into()));
        assert_eq!(document.task_priority_value(&id), 10);
    }

    #[test]
    fn rename_category_cascades_and_undoes() {
        let mut state = ApplicationState::new();
        let mut document = KanbanDocument::new();
        let id = document.get_new_task().id;
        let mut task = document.get_task(id).unwrap().clone();
        task.category = Some("Work".into());
        document.replace_task(&task);

        state.handle_command(
            &mut document,
            ApplicationCommand::RenameCategory {
                from: "Work".to_owned(),
                to: "Office".to_owned(),
            },
        );
        assert_eq!(
            document.get_task(id).unwrap().category,
            Some("Office".into())
        );
        assert!(document.get_category_style("Office").is_some());
        assert!(document.get_category_style("Work").is_none());

        state.undo(&mut document);
        assert_eq!(
            document.get_task(id).unwrap().category,
            Some("Work".into())
        );
        assert!(document.get_category_style("Work").is_some());
        assert!(document.get_category_style("Office").is_none());
    }

    #[test]
    fn rename_notifications_reach_open_editors() {
        let mut state = ApplicationState::new();
        let mut document = KanbanDocument::new();

        let effects = state.handle_command(
            &mut document,
            ApplicationCommand::RenamePriority {
                from: "High".to_owned(),
                to: "Urgent".to_owned(),
            },
        );
        assert!(effects.contains(&AppShellCommand::RenamedPriority {
            from: "High".to_owned(),
            to: "Urgent".to_owned(),
        }));
        assert!(effects.contains(&AppShellCommand::UpdateLayout));

        // A no-op rename (unknown source) must not notify anyone.
        let none = state.handle_command(
            &mut document,
            ApplicationCommand::RenamePriority {
                from: "Nope".to_owned(),
                to: "Whatever".to_owned(),
            },
        );
        assert!(none.is_empty());

        let cat_effects = {
            // The category must exist in the registry first (new documents
            // ship priorities but no categories).
            document.replace_category_style(
                &"Chore".to_owned(),
                KanbanCategoryStyle::default(),
            );
            state.handle_command(
                &mut document,
                ApplicationCommand::RenameCategory {
                    from: "Chore".to_owned(),
                    to: "Errand".to_owned(),
                },
            )
        };
        assert!(cat_effects.contains(&AppShellCommand::RenamedCategory {
            from: "Chore".to_owned(),
            to: "Errand".to_owned(),
        }));

        // Undo must notify too, in reverse: LIFO pops the category rename
        // first, then the priority rename.
        let undo_cat = state.undo(&mut document);
        assert!(undo_cat.contains(&AppShellCommand::RenamedCategory {
            from: "Errand".to_owned(),
            to: "Chore".to_owned(),
        }));
        let undo_pri = state.undo(&mut document);
        assert!(undo_pri.contains(&AppShellCommand::RenamedPriority {
            from: "Urgent".to_owned(),
            to: "High".to_owned(),
        }));
    }
}
