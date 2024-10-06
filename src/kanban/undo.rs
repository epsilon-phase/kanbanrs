use super::*;
#[derive(Debug)]
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
#[derive(Debug)]
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
#[derive(Debug)]
pub struct ModificationEvent {
    pub former_item: KanbanItem,
}
impl ModificationEvent {
    pub fn undo(&self, document: &mut KanbanDocument) {
        document.replace_task(&self.former_item);
    }
}
#[derive(Debug)]
pub enum UndoItem {
    Create(CreationEvent),
    Delete(DeletionEvent),
    Modification(ModificationEvent),
}
impl UndoItem {
    pub fn undo(&self, document: &mut KanbanDocument) {
        println!("Undoing: {:?}", self);
        match self {
            UndoItem::Create(ce) => ce.undo(document),
            UndoItem::Delete(de) => de.undo(document),
            UndoItem::Modification(me) => me.undo(document),
        }
    }
    pub fn merge(&self, other: &Self) -> Option<Self> {
        match self {
            UndoItem::Create(ce) => match other {
                UndoItem::Modification(me) if ce.new_task.id == me.former_item.id => {
                    Some(UndoItem::Create(CreationEvent {
                        new_task: me.former_item.clone(),
                        parent_id: ce.parent_id,
                    }))
                }
                _ => None,
            },
            _ => None,
        }
    }
}
