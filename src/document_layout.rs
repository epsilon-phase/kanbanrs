use crate::kanban::KanbanId;

use super::*;
///The layout type, with somes tate
pub enum KanbanDocumentLayoutType {
    ///The queue state
    Queue(kanban::queue_view::QueueState),
    ///The column state, consisting of three lists that represent
    ///* Ready
    ///* Blocked
    ///* Completed
    Columnar([Vec<i32>; 3]),
    ///The search view
    Search(kanban::search::SearchState),
    ///The focused view
    Focused(kanban::focused_layout::Focus),
    ///The Tree Outline state
    TreeOutline(kanban::tree_outline_layout::TreeOutline),
    ///The node layout
    NodeLayout(kanban::node_layout::NodeLayout),
    ///Placeholder to be loaded later
    Unloaded,
}
impl std::fmt::Debug for KanbanDocumentLayoutType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Type").unwrap();
        f.write_str(match self {
            Self::Queue(_) => "Queue",
            Self::Columnar(_) => "Columnar",
            Self::Search(_) => "Search",
            Self::Focused(_) => "Focused",
            Self::TreeOutline(_) => "Tree Outline",
            Self::NodeLayout(_) => "Node",
            Self::Unloaded => "Unloaded",
        })
    }
}
pub struct KanbanDocumentLayout {
    pub layout: KanbanDocumentLayoutType,
    pub scroll_to: Option<KanbanId>,
}
impl PartialEq for KanbanDocumentLayout {
    fn eq(&self, other: &Self) -> bool {
        match self.layout {
            KanbanDocumentLayoutType::Columnar(_) => {
                matches!(other.layout, KanbanDocumentLayoutType::Columnar(_))
            }
            KanbanDocumentLayoutType::Queue(_) => {
                matches!(other.layout, KanbanDocumentLayoutType::Queue(_))
            }
            KanbanDocumentLayoutType::Search(_) => {
                matches!(other.layout, KanbanDocumentLayoutType::Search(_))
            }
            KanbanDocumentLayoutType::Focused(_) => {
                matches!(other.layout, KanbanDocumentLayoutType::Focused(_))
            }
            KanbanDocumentLayoutType::TreeOutline(_) => {
                matches!(other.layout, KanbanDocumentLayoutType::TreeOutline(_))
            }
            KanbanDocumentLayoutType::NodeLayout(_) => {
                matches!(other.layout, KanbanDocumentLayoutType::NodeLayout(_))
            }
            KanbanDocumentLayoutType::Unloaded => {
                matches!(other.layout, KanbanDocumentLayoutType::Unloaded)
            }
        }
    }
}
impl KanbanDocumentLayout {
    fn update_columnar(
        columnar_cache: &mut [Vec<i32>; 3],
        document: &KanbanDocument,
        filter: &KanbanFilter,
    ) {
        columnar_cache.iter_mut().for_each(|x| x.clear());
        for task in document.get_tasks() {
            let index = match document.task_status(&task.id) {
                kanban::Status::Ready => 0,
                kanban::Status::Blocked => 1,
                kanban::Status::Completed => 2,
            };
            if !filter.matches(task, document) {
                continue;
            }
            columnar_cache[index].push(task.id);
        }
    }
    pub fn inform_of_new_items(&mut self) {
        if let KanbanDocumentLayoutType::Search(x) = &mut self.layout {
            x.force_update();
        }
    }
    pub fn update_cache(
        &mut self,
        document: &KanbanDocument,
        sort: &ItemSort,
        style: &egui::Style,
        filter: &KanbanFilter,
    ) {
        kanban::layout_cache::clear_layout_cache();
        match &mut self.layout {
            KanbanDocumentLayoutType::Queue(x) => {
                x.update(document);
            }
            KanbanDocumentLayoutType::Columnar(array) => {
                KanbanDocumentLayout::update_columnar(array, document, filter);
            }
            KanbanDocumentLayoutType::Search(search_state) => {
                search_state.update(document);
            }
            KanbanDocumentLayoutType::Focused(focus) => {
                focus.update(document);
            }
            KanbanDocumentLayoutType::TreeOutline(tree) => {
                tree.update(document, *sort, filter);
            }
            KanbanDocumentLayoutType::NodeLayout(nl) => {
                nl.update(document, style, filter, sort);
            }
            KanbanDocumentLayoutType::Unloaded => {
                panic!("Layout type should not be Unloaded");
            }
        }
    }

    pub fn sort_cache(&mut self, document: &KanbanDocument, sort: &ItemSort) {
        match &mut self.layout {
            KanbanDocumentLayoutType::Columnar(array) => array
                .iter_mut()
                .for_each(|item| sort.sort_by(item, document)),
            KanbanDocumentLayoutType::Focused(focus) => {
                sort.sort_by(&mut focus.children, document);
                sort.sort_by(&mut focus.ancestors, document);
            }
            _ => (),
        }
    }
}
impl Default for KanbanDocumentLayout {
    fn default() -> Self {
        KanbanDocumentLayout {
            layout: KanbanDocumentLayoutType::Columnar([Vec::new(), Vec::new(), Vec::new()]),
            scroll_to: None,
        }
    }
}
impl From<&KanbanDocumentLayout> for String {
    fn from(src: &KanbanDocumentLayout) -> String {
        match src.layout {
            KanbanDocumentLayoutType::Columnar(_) => "Columnar",
            KanbanDocumentLayoutType::Queue(_) => "Queue",
            KanbanDocumentLayoutType::Search(_) => "Search",
            KanbanDocumentLayoutType::Focused(_) => "Focus",
            KanbanDocumentLayoutType::TreeOutline(_) => "Tree outline",
            KanbanDocumentLayoutType::NodeLayout(_) => "Node outline",
            KanbanDocumentLayoutType::Unloaded => "You shouldn't see this",
        }
        .into()
    }
}

//---------------------------------------------------------
// KanbanRS implementation
//---------------------------------------------------------

/// Layout code
impl KanbanRS {
    pub fn layout_columnar(&mut self, ui: &mut egui::Ui) {
        if let KanbanDocumentLayoutType::Columnar(cache) = &mut self.current_layout.layout {
            let column_width = ui.available_width() / 3.0;
            ui.columns(3, |columns| {
                columns[0].label(RichText::new("Ready").heading());
                columns.iter_mut().for_each(|x| x.set_width(column_width));
                self.document.read().layout_id_list(
                    &mut columns[0],
                    &cache[0],
                    &mut self.hovered_task,
                    &mut self.summary_actions_pending,
                    "ReadyScrollArea",
                    None,
                );
                columns[1].label(RichText::new("Blocked").heading());
                self.document.read().layout_id_list(
                    &mut columns[1],
                    &cache[1],
                    &mut self.hovered_task,
                    &mut self.summary_actions_pending,
                    "BlockedScrollArea",
                    self.current_layout.scroll_to,
                );
                columns[2].label(RichText::new("Completed").heading());

                self.document.read().layout_id_list(
                    &mut columns[2],
                    &cache[2],
                    &mut self.hovered_task,
                    &mut self.summary_actions_pending,
                    "CompletedScrollArea",
                    self.current_layout.scroll_to,
                );
                self.current_layout.scroll_to = None;
            });
        }
    }

    pub fn layout_queue(&mut self, ui: &mut egui::Ui) {
        if let KanbanDocumentLayoutType::Queue(qs) = &mut self.current_layout.layout {
            // ScrollArea::vertical().id_salt("Queue").show_rows(
            //     ui,
            //     200.0,
            //     qs.cached_ready.len(),
            //     |ui, range| {
            self.document.read().layout_id_list(
                ui,
                &qs.cached_ready,
                &mut self.hovered_task,
                &mut self.summary_actions_pending,
                "Queue",
                self.current_layout.scroll_to,
            );
            self.current_layout.scroll_to = None;
            // );
        }
    }
    pub fn layout_search(&mut self, ui: &mut egui::Ui) {
        let doc = self.document.read();
        if let KanbanDocumentLayoutType::Search(search_state) = &mut self.current_layout.layout {
            ui.horizontal(|ui| {
                let label = ui.label("Search");
                ui.text_edit_singleline(&mut search_state.search_prompt)
                    .labelled_by(label.id);
                search_state.update(&doc);
            });

            doc.layout_id_list(
                ui,
                &search_state.matched_ids,
                &mut self.hovered_task,
                &mut self.summary_actions_pending,
                "SearchArea",
                self.current_layout.scroll_to,
            );
        }
    }
    pub fn layout_focused(&mut self, ui: &mut egui::Ui) {
        if let KanbanDocumentLayoutType::Focused(focus) = &mut self.current_layout.layout {
            ui.columns(3, |columns| {
                columns[0].label(RichText::new("Child tasks").heading());
                columns[2].label(RichText::new("Parent tasks").heading());
                columns[1].label(RichText::new("Focused Task").heading());
                if let Some(target) = focus.cares_about {
                    let doc = self.document.read();
                    let task = doc.get_task(target).unwrap();
                    self.summary_actions_pending.push(task.summary(
                        &doc,
                        &mut self.hovered_task,
                        &mut columns[1],
                        true,
                        0,
                    ));
                }

                self.document.read().layout_id_list(
                    &mut columns[0],
                    &focus.children,
                    &mut self.hovered_task,
                    &mut self.summary_actions_pending,
                    "ChildScroller",
                    self.current_layout.scroll_to,
                );

                self.document.read().layout_id_list(
                    &mut columns[2],
                    &focus.ancestors,
                    &mut self.hovered_task,
                    &mut self.summary_actions_pending,
                    "ParentScroller",
                    self.current_layout.scroll_to,
                );
            });
        }
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    #[test]
    fn test_columnar_layout() {
        use chrono::Utc;

        let children = vec![vec![1], Vec::new(), vec![3]];
        let mut document = kanban::tests::make_document_easy(4, &children);
        {
            let mut task = document.get_task(1).unwrap().clone();
            task.completed = Some(Utc::now());
            document.replace_task(&task);
        }
        let mut layout = KanbanDocumentLayout::default();
        layout.update_cache(
            &document,
            &ItemSort::None,
            &egui::Style::default(),
            &KanbanFilter::None,
        );
        if let KanbanDocumentLayoutType::Columnar(cache) = layout.layout {
            assert_eq!(cache[0].len(), 2);
            assert_eq!(cache[1].len(), 1);
            assert_eq!(cache[2].len(), 1);
            for column in cache {
                for id in &column {
                    assert_eq!(column.iter().filter(|x| *x == id).count(), 1);
                }
            }
        }
    }
}
