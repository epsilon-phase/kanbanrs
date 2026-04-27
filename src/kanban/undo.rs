use super::*;
///Represents the creation of a task
#[derive(Debug, Serialize, Deserialize)]
pub struct CreationEvent {
    ///The id of the parent task, there can only be one, and the new
    ///id must be deleted from the list of its children to undo
    pub parent_id: Option<KanbanId>,
    ///The new task added. Kept here mostly for posterity
    pub new_task: KanbanItem,
}
impl CreationEvent {
    pub fn undo(&self, document: &mut KanbanDocument) {
        document.remove_task(&KanbanItem {
            id: self.new_task.id,
            ..Default::default()
        });
    }
}
///Represents the deletion record of a task
#[derive(Debug, Serialize, Deserialize)]
pub struct DeletionEvent {
    ///The deleted task
    pub former_item: KanbanItem,
    ///The ids of the tasks which had this as a parent, necessary
    ///to restore it
    pub parent_ids: Vec<KanbanId>,
}
impl DeletionEvent {
    pub fn undo(&self, document: &mut KanbanDocument) {
        document.replace_task(&self.former_item);
        for i in self.parent_ids.iter() {
            let task = document.get_task_mut(*i).unwrap();
            task.add_child(&self.former_item);
        }
    }
}
///Represents a modification of an existing task
#[derive(Debug, Serialize, Deserialize)]
pub struct ModificationEvent {
    ///The former version of that task.
    pub former_item: KanbanItem,
}
impl ModificationEvent {
    pub fn undo(&self, document: &mut KanbanDocument) {
        document.replace_task(&self.former_item);
    }
}
/// Represents a change to a category's style
#[derive(Debug, Serialize, Deserialize)]
pub struct CategoryStyleEvent {
    pub name: String,
    /// The style before the change, or None if the category was newly created
    pub former_style: Option<KanbanCategoryStyle>,
}
impl CategoryStyleEvent {
    pub fn undo(&self, document: &mut KanbanDocument) {
        match self.former_style {
            Some(style) => document.replace_category_style(&self.name, style),
            None => document.remove_category(&self.name),
        }
    }
}
/// Represents a change to a priority's value
#[derive(Debug, Serialize, Deserialize)]
pub struct PriorityEvent {
    pub name: String,
    /// The value before the change, or None if the priority was newly created
    pub former_value: Option<i32>,
}
impl PriorityEvent {
    pub fn undo(&self, document: &mut KanbanDocument) {
        match self.former_value {
            Some(value) => document.set_priority(self.name.clone(), value),
            None => document.remove_priority(&self.name),
        }
    }
}
///The various types of events
#[derive(Debug, Serialize, Deserialize)]
pub enum UndoItem {
    Create(CreationEvent),
    Delete(DeletionEvent),
    Modification(ModificationEvent),
    CategoryStyle(CategoryStyleEvent),
    Priority(PriorityEvent),
}
impl UndoItem {
    pub fn undo(&self, document: &mut KanbanDocument) {
        info!(target:"Undo","Undoing: {self:?}");
        match self {
            UndoItem::Create(ce) => ce.undo(document),
            UndoItem::Delete(de) => de.undo(document),
            UndoItem::Modification(me) => me.undo(document),
            UndoItem::CategoryStyle(ce) => ce.undo(document),
            UndoItem::Priority(pe) => pe.undo(document),
        }
    }
    pub fn merge(&self, other: &Self) -> Option<Self> {
        // The only time this can really be done semantically is if an item is modified twice in a row
        // after creation, but I think this makes doing the undo a bit easier
        match (self, other) {
            (UndoItem::Create(ce), UndoItem::Modification(me))
                if me.former_item.id == ce.new_task.id && ce.new_task.is_unset() =>
            {
                Some(UndoItem::Create(CreationEvent {
                    new_task: me.former_item.clone(),
                    parent_id: ce.parent_id,
                }))
            }
            (_, _) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with_task() -> (KanbanDocument, KanbanItem) {
        let mut doc = KanbanDocument::new();
        let task = doc.get_new_task();
        let task = doc.get_task(task.id).unwrap().clone();
        (doc, task)
    }

    #[test]
    fn test_creation_undo_removes_task() {
        let (mut doc, task) = doc_with_task();
        assert!(doc.get_task(task.id).is_some());
        let event = CreationEvent { parent_id: None, new_task: task.clone() };
        event.undo(&mut doc);
        assert!(doc.get_task(task.id).is_none());
    }

    #[test]
    fn test_deletion_undo_restores_task_and_parent_link() {
        let mut doc = KanbanDocument::new();
        let parent = doc.get_new_task();
        let parent_id = parent.id;
        let child = doc.get_new_task();
        let child_id = child.id;
        let mut parent = doc.get_task(parent_id).unwrap().clone();
        parent.add_child(&child);
        doc.replace_task(&parent);

        let undo_item = doc.remove_task(&child);
        assert!(doc.get_task(child_id).is_none());
        assert!(!doc.get_task(parent_id).unwrap().child_tasks.contains(&child_id));

        undo_item.undo(&mut doc);
        assert!(doc.get_task(child_id).is_some());
        assert!(doc.get_task(parent_id).unwrap().child_tasks.contains(&child_id));
    }

    #[test]
    fn test_modification_undo_restores_former_state() {
        let (mut doc, original) = doc_with_task();
        let id = original.id;
        let mut updated = original.clone();
        updated.name = "Changed".to_owned();
        doc.replace_task(&updated);
        assert_eq!(doc.get_task(id).unwrap().name, "Changed");

        ModificationEvent { former_item: original }.undo(&mut doc);
        assert_eq!(doc.get_task(id).unwrap().name, "");
    }

    #[test]
    fn test_category_style_undo_removes_newly_created() {
        let mut doc = KanbanDocument::new();
        doc.replace_category_style("work", KanbanCategoryStyle { children_inherit_category: true, ..Default::default() });
        assert!(doc.get_category_style("work").is_some());

        CategoryStyleEvent { name: "work".to_owned(), former_style: None }.undo(&mut doc);
        assert!(doc.get_category_style("work").is_none());
    }

    #[test]
    fn test_category_style_undo_restores_previous() {
        let mut doc = KanbanDocument::new();
        let old = KanbanCategoryStyle { children_inherit_category: false, ..Default::default() };
        let new = KanbanCategoryStyle { children_inherit_category: true, ..Default::default() };
        doc.replace_category_style("work", old);
        doc.replace_category_style("work", new);

        CategoryStyleEvent { name: "work".to_owned(), former_style: Some(old) }.undo(&mut doc);
        assert!(!doc.get_category_style("work").unwrap().children_inherit_category);
    }

    #[test]
    fn test_priority_undo_removes_newly_added() {
        let mut doc = KanbanDocument::new();
        doc.set_priority("Urgent".to_owned(), 20);

        PriorityEvent { name: "Urgent".to_owned(), former_value: None }.undo(&mut doc);
        assert!(!doc.get_sorted_priorities().iter().any(|(n, _)| n.as_str() == "Urgent"));
    }

    #[test]
    fn test_priority_undo_restores_previous_value() {
        let mut doc = KanbanDocument::new();
        doc.set_priority("Urgent".to_owned(), 20);
        doc.set_priority("Urgent".to_owned(), 99);

        PriorityEvent { name: "Urgent".to_owned(), former_value: Some(20) }.undo(&mut doc);
        let val = doc.get_sorted_priorities().into_iter()
            .find(|(n, _)| n.as_str() == "Urgent")
            .map(|(_, v)| *v);
        assert_eq!(val, Some(20));
    }

    #[test]
    fn test_merge_create_then_modify_same_id() {
        let blank = KanbanItem { id: 7, ..Default::default() };
        assert!(blank.is_unset());
        let mut named = blank.clone();
        named.name = "Named".to_owned();

        let create = UndoItem::Create(CreationEvent { parent_id: None, new_task: blank });
        let modify = UndoItem::Modification(ModificationEvent { former_item: named });

        let merged = create.merge(&modify);
        assert!(matches!(&merged, Some(UndoItem::Create(ce)) if ce.new_task.name == "Named"));
    }

    #[test]
    fn test_merge_different_ids_returns_none() {
        let create = UndoItem::Create(CreationEvent {
            parent_id: None,
            new_task: KanbanItem { id: 1, ..Default::default() },
        });
        let modify = UndoItem::Modification(ModificationEvent {
            former_item: KanbanItem { id: 2, ..Default::default() },
        });
        assert!(create.merge(&modify).is_none());
    }

    #[test]
    fn test_merge_non_create_first_returns_none() {
        let task = KanbanItem { id: 1, ..Default::default() };
        let a = UndoItem::Modification(ModificationEvent { former_item: task.clone() });
        let b = UndoItem::Modification(ModificationEvent { former_item: task });
        assert!(a.merge(&b).is_none());
    }
}
