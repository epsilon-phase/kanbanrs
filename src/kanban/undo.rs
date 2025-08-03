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
///The various types of events
#[derive(Debug, Serialize, Deserialize)]
pub enum UndoItem {
    Create(CreationEvent),
    Delete(DeletionEvent),
    Modification(ModificationEvent),
}
impl UndoItem {
    pub fn undo(&self, document: &mut KanbanDocument) {
        info!(target:"Undo","Undoing: {self:?}");
        match self {
            UndoItem::Create(ce) => ce.undo(document),
            UndoItem::Delete(de) => de.undo(document),
            UndoItem::Modification(me) => me.undo(document),
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
