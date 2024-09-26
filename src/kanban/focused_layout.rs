use super::*;
#[derive(Clone)]
pub struct Focus {
    pub cares_about: Option<KanbanId>,
    pub children: Vec<KanbanId>,
    pub ancestors: Vec<KanbanId>,
}
impl Focus {
    pub fn new(id: KanbanId) -> Focus {
        return Focus {
            cares_about: Some(id),
            children: Vec::new(),
            ancestors: Vec::new(),
        };
    }
    pub fn update(&mut self, document: &KanbanDocument) {
        if self.cares_about.is_none() {
            return;
        }
        self.children.clear();
        self.ancestors.clear();
        let subject = self.cares_about.unwrap();
        for task in document.get_tasks() {
            if task.id == subject {
                continue;
            }
            match document.get_relation(subject, task.id) {
                TaskRelation::ChildOf => self.ancestors.push(task.id),
                TaskRelation::ParentOf => self.children.push(task.id),
                _ => (),
            }
        }
    }
}
