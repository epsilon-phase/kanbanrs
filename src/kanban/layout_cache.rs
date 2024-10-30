use egui::{Pos2, Rect};

use super::*;
use std::{borrow::Borrow, cell::RefCell, collections::HashSet, ops::Add};

// In the future, this should be a mapping of computed item heights rather than positions.
thread_local! {
    static ITEM_POSITION_CACHE: RefCell<HashMap<egui::Id,HashMap<KanbanId,f32>>> = RefCell::new(HashMap::new());
}
pub fn clear_layout_cache() {
    ITEM_POSITION_CACHE.with_borrow_mut(|cache| cache.iter_mut().for_each(|x| x.1.clear()))
}

pub fn record_position(id: egui::Id, task_idx: KanbanId, start: f32, end: f32) {
    ITEM_POSITION_CACHE.with_borrow_mut(|cache| {
        if !cache.contains_key(&id) {
            cache.insert(id, HashMap::new());
        }
        let id_cache = cache.get_mut(&id).unwrap();
        id_cache.insert(task_idx, end - start);
    });
}
// pub fn items_within(id: egui::Id, viewport: Rect) -> HashSet<KanbanId> {
//     ITEM_POSITION_CACHE.with_borrow(|cache| {
//         if let Some(cache) = cache.get(&id) {
//             let midpoint = viewport.center_top().x;
//             cache
//                 .iter()
//                 .filter(|(_id, (start, end))| {
//                     let start = *start;
//                     let end = *end;
//                     let mock_rect = Rect {
//                         min: Pos2::new(midpoint, start),
//                         max: Pos2::new(midpoint, end),
//                     };
//                     mock_rect.intersects(viewport)
//                 })
//                 .map(|x| *x.0)
//                 .collect()
//         } else {
//             HashSet::new()
//         }
//     })
// }
pub fn has_cache(id: egui::Id, expected_count: usize) -> bool {
    ITEM_POSITION_CACHE.with_borrow(|cache| {
        cache
            .get(&id)
            .map_or(false, |x| x.len() == expected_count && expected_count != 0)
    })
}
pub fn get_item_height(id: egui::Id, task_id: KanbanId) -> f32 {
    return ITEM_POSITION_CACHE.with_borrow(|x| *x.get(&id).unwrap().get(&task_id).unwrap());
}
pub fn cached_total_height(id: egui::Id) -> f32 {
    let mut max = f32::NEG_INFINITY;
    let mut min = f32::INFINITY;
    ITEM_POSITION_CACHE.with_borrow(|cache| {
        cache
            .get(&id)
            .unwrap()
            .iter()
            .map(|(_id, height)| height)
            .fold(0.0, Add::add)
    })
}
