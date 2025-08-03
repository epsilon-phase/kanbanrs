use super::*;
use std::{cell::RefCell, ops::Add};

// In the future, this should be a mapping of computed item heights rather than positions.
thread_local! {
    static ITEM_POSITION_CACHE: RefCell<HashMap<egui::Id,HashMap<KanbanId,f32>>> = RefCell::new(HashMap::new());
}
pub fn clear_layout_cache() {
    ITEM_POSITION_CACHE.with_borrow_mut(|cache| cache.iter_mut().for_each(|x| x.1.clear()))
}
///Record the height of a given item in the display.
///
///Necessary for efficient display of many tasks
pub fn record_height(id: egui::Id, task_idx: KanbanId, start: f32, end: f32) {
    ITEM_POSITION_CACHE.with_borrow_mut(|cache| {
        cache.entry(id).or_default();
        let id_cache = cache.get_mut(&id).unwrap();
        id_cache.insert(task_idx, end - start);
    });
}
///Determine if the cache contains a given item under a specific layout id,
///and whether or not it has the expected number of items
pub fn has_cache(id: egui::Id, expected_count: usize) -> bool {
    ITEM_POSITION_CACHE.with_borrow(|cache| {
        cache
            .get(&id)
            .is_some_and(|x| x.len() == expected_count && expected_count != 0)
    })
}
///Returns the expected height of the item in a given layout, or None
pub fn get_item_height(id: egui::Id, task_id: KanbanId) -> Option<f32> {
    ITEM_POSITION_CACHE.with_borrow(|x| {
        if let Some(id_cache) = x.get(&id) {
            id_cache.get(&task_id).copied()
        } else {
            None
        }
    })
}
///The expected total height of items in a given layout
pub fn cached_total_height(id: egui::Id) -> f32 {
    ITEM_POSITION_CACHE.with_borrow(|cache| {
        if let Some(cache) = cache.get(&id) {
            cache.values().fold(0.0, Add::add)
        } else {
            //Hopefully more pixels than the end user will have to deal with
            1e6
        }
    })
}
