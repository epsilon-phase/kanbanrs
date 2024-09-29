use std::collections::HashSet;

use sorting::ItemSort;

use super::*;
#[derive(Default, Clone)]
pub struct TreeOutline {
    toplevel_items: Vec<KanbanId>,
    cache: Vec<(KanbanId, u32)>,
    // If this is set then it should only display the tree from this node onwards.
    focused_id: Option<KanbanId>,
}
impl TreeOutline {
    pub fn new() -> TreeOutline {
        TreeOutline {
            ..Default::default()
        }
    }
    pub fn update(&mut self, document: &KanbanDocument, sort: ItemSort) {
        self.toplevel_items.clear();
        self.cache.clear();
        let mut children_of_something: HashSet<KanbanId> = HashSet::new();
        document.get_tasks().for_each(|task| {
            children_of_something.extend(task.child_tasks.iter());
        });
        self.toplevel_items = document
            .get_tasks()
            .filter(|x| !children_of_something.contains(&x.id))
            .map(|key| key.id)
            .collect();
        sort.sort_by(&mut self.toplevel_items, document);
        if let Some(id) = self.focused_id {
            document.on_tree(id, 0, |_document, id, depth| {
                self.cache.push((id, depth));
            });
        } else {
            for id in self.toplevel_items.iter() {
                document.on_tree(*id, 0, |_document, id, depth| {
                    self.cache.push((id, depth));
                });
            }
        }
        println!("Found {} toplevel items", self.toplevel_items.len());
    }
    pub fn set_focus(&mut self, id: KanbanId) {
        self.focused_id = Some(id);
    }
    pub fn show(
        &self,
        ui: &mut egui::Ui,
        document: &KanbanDocument,
        actions: &mut Vec<SummaryAction>,
        hovered_item: &mut Option<KanbanId>,
    ) {
        ScrollArea::vertical()
            .id_salt("Tree Outline")
            .show(ui, |ui| {
                for (id, depth) in self.cache.iter() {
                    if let Some(task) = document.get_task(*id) {
                        ui.horizontal(|ui| {
                            ui.label("");
                            ui.add_space((*depth as f32) * ui.available_width() / 20.0);
                            actions.push(task.summary(document, hovered_item, ui));
                        });
                    }
                }
            });
    }
}
