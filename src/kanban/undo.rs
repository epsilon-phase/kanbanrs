use super::*;
#[derive(Debug, Serialize, Deserialize)]
pub struct CreationEvent {
    pub parent_id: Option<KanbanId>,
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
#[derive(Debug, Serialize, Deserialize)]
pub struct DeletionEvent {
    pub former_item: KanbanItem,
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
#[derive(Debug, Serialize, Deserialize)]
pub struct ModificationEvent {
    pub former_item: KanbanItem,
}
impl ModificationEvent {
    pub fn undo(&self, document: &mut KanbanDocument) {
        document.replace_task(&self.former_item);
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub enum UndoItem {
    Create(CreationEvent),
    Delete(DeletionEvent),
    Modification(ModificationEvent),
}
impl UndoItem {
    pub fn undo(&self, document: &mut KanbanDocument) {
        info!(target:"Undo","Undoing: {:?}", self);
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
