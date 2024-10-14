use std::{
    collections::{HashSet, VecDeque},
    default,
};

use filter::KanbanFilter;
use sorting::ItemSort;

use super::*;
#[derive(Default, Clone)]
pub struct TreeOutline {
    toplevel_items: Vec<KanbanId>,
    /// Vector of associations between a kanban item id and the indentation level for that
    /// particular occurance
    cache: Vec<(KanbanId, u32)>,
    // If this is set then it should only display the tree from this node onwards.
    focused_id: Option<KanbanId>,
    exclude_completed: bool,
    total_height: f64,
    layout_count: f64,
}

type Depth = u32;
impl TreeOutline {
    pub fn new() -> TreeOutline {
        TreeOutline {
            total_height: 50.0,
            layout_count: 1.0,
            ..Default::default()
        }
    }
    fn bfs(&mut self, document: &KanbanDocument, sort: ItemSort, filter: &KanbanFilter) {
        self.cache.clear();
        let mut queue: VecDeque<(KanbanId, Depth)> = VecDeque::new();
        let mut buffer: Vec<(KanbanId, Depth)> =
            self.toplevel_items.iter().map(|x| (*x, 0)).collect();
        buffer.sort_by(|(a, _), (b, _)| {
            sort.cmp_by(
                document.get_task(*a).unwrap(),
                document.get_task(*b).unwrap(),
            )
        });
        queue.extend(buffer.drain(..));
        while let Some((current_id, depth)) = queue.pop_front() {
            if self.exclude_completed && document.get_task(current_id).unwrap().completed.is_some()
            {
                continue;
            }
            let item = document.get_task(current_id).unwrap();
            if !filter.matches(item, document) {
                continue;
            }
            self.cache.push((current_id, depth));

            buffer.extend(item.child_tasks.iter().map(|x| (*x, depth + 1)));
            buffer.sort_by(|(a, _), (b, _)| {
                sort.cmp_by(
                    document.get_task(*a).unwrap(),
                    document.get_task(*b).unwrap(),
                )
            });
            queue.extend(buffer.drain(..));
        }
    }
    pub fn update(&mut self, document: &KanbanDocument, sort: ItemSort, filter: &KanbanFilter) {
        self.toplevel_items.clear();
        self.cache.clear();
        let mut children_of_something: HashSet<KanbanId> = HashSet::new();
        document
            .get_tasks()
            .filter(|x| filter.matches(x, document))
            .for_each(|task| {
                children_of_something.extend(task.child_tasks.iter());
            });
        self.toplevel_items = document
            .get_tasks()
            .filter(|x| !children_of_something.contains(&x.id))
            .map(|key| key.id)
            .collect();
        self.bfs(document, sort, filter);
        println!("Found {} toplevel items", self.toplevel_items.len());
    }
    pub fn set_focus(&mut self, id: KanbanId) {
        self.focused_id = Some(id);
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        document: &KanbanDocument,
        actions: &mut Vec<SummaryAction>,
        hovered_item: &mut Option<KanbanId>,
    ) {
        if ui
            .checkbox(&mut self.exclude_completed, "Exclude completed")
            .changed()
        {
            actions.push(SummaryAction::UpdateLayout);
        }
        ScrollArea::vertical().id_salt("Tree Outline").show_rows(
            ui,
            (self.total_height / self.layout_count) as f32,
            self.cache.len(),
            |ui, range| {
                ui.set_width(ui.available_width());
                for idx in range {
                    let (id, depth) = self.cache[idx];
                    if let Some(task) = document.get_task(id) {
                        let start = ui.cursor().min.y;
                        ui.horizontal(|ui| {
                            ui.label("");
                            ui.add_space((depth as f32) * ui.available_width() / 20.0);

                            actions.push(task.summary(document, hovered_item, ui));
                        });
                        let end = ui.cursor().min.y;
                        let difference = (end - start) as f64;
                        let divergence = (self.total_height / self.layout_count - difference).abs();
                        if divergence > 20.0 {
                            self.total_height += difference;
                            self.layout_count += 1.;
                        }
                    }
                }
            },
        );
    }
}
