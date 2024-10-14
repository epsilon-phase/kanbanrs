use super::*;
#[derive(Clone)]
pub enum KanbanDocumentLayout {
    Queue(kanban::queue_view::QueueState),
    Columnar([Vec<i32>; 3]),
    Search(kanban::search::SearchState),
    Focused(kanban::focused_layout::Focus),
    TreeOutline(kanban::tree_outline_layout::TreeOutline),
    NodeLayout(kanban::node_layout::NodeLayout),
}
impl PartialEq for KanbanDocumentLayout {
    fn eq(&self, other: &Self) -> bool {
        match self {
            KanbanDocumentLayout::Columnar(_) => matches!(other, KanbanDocumentLayout::Columnar(_)),
            KanbanDocumentLayout::Queue(_) => matches!(other, KanbanDocumentLayout::Queue(_)),
            KanbanDocumentLayout::Search(_) => matches!(other, KanbanDocumentLayout::Search(_)),
            KanbanDocumentLayout::Focused(_) => matches!(other, KanbanDocumentLayout::Focused(_)),
            KanbanDocumentLayout::TreeOutline(_) => {
                matches!(other, KanbanDocumentLayout::TreeOutline(_))
            }
            KanbanDocumentLayout::NodeLayout(_) => {
                matches!(other, KanbanDocumentLayout::NodeLayout(_))
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
        if let KanbanDocumentLayout::Search(x) = self {
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
        match self {
            KanbanDocumentLayout::Queue(x) => {
                x.update(document);
            }
            KanbanDocumentLayout::Columnar(array) => {
                KanbanDocumentLayout::update_columnar(array, document, filter);
            }
            KanbanDocumentLayout::Search(search_state) => {
                search_state.update(document);
            }
            KanbanDocumentLayout::Focused(focus) => {
                focus.update(document);
            }
            KanbanDocumentLayout::TreeOutline(tree) => {
                tree.update(document, *sort, filter);
            }
            KanbanDocumentLayout::NodeLayout(nl) => {
                nl.update(document, style);
            }
        }
    }

    pub fn sort_cache(&mut self, document: &KanbanDocument, sort: &ItemSort) {
        match self {
            KanbanDocumentLayout::Columnar(array) => array
                .iter_mut()
                .for_each(|item| sort.sort_by(item, document)),
            KanbanDocumentLayout::Focused(focus) => {
                sort.sort_by(&mut focus.children, document);
                sort.sort_by(&mut focus.ancestors, document);
            }
            _ => (),
        }
    }
}
impl Default for KanbanDocumentLayout {
    fn default() -> Self {
        KanbanDocumentLayout::Columnar([Vec::new(), Vec::new(), Vec::new()])
    }
}
impl From<&KanbanDocumentLayout> for String {
    fn from(src: &KanbanDocumentLayout) -> String {
        match src {
            KanbanDocumentLayout::Columnar(_) => "Columnar",
            KanbanDocumentLayout::Queue(_) => "Queue",
            KanbanDocumentLayout::Search(_) => "Search",
            KanbanDocumentLayout::Focused(_) => "Focus",
            KanbanDocumentLayout::TreeOutline(_) => "Tree outline",
            KanbanDocumentLayout::NodeLayout(_) => "Node outline",
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
        if let KanbanDocumentLayout::Columnar(cache) = &mut self.current_layout.clone() {
            ui.columns(3, |columns| {
                columns[0].label(RichText::new("Ready").heading());

                self.document.read().layout_id_list(
                    &mut columns[0],
                    &cache[0],
                    &mut self.hovered_task,
                    &mut self.summary_actions_pending,
                    "ReadyScrollArea",
                );
                columns[1].label(RichText::new("Blocked").heading());
                self.document.read().layout_id_list(
                    &mut columns[1],
                    &cache[1],
                    &mut self.hovered_task,
                    &mut self.summary_actions_pending,
                    "BlockedScrollArea",
                );
                columns[2].label(RichText::new("Completed").heading());

                self.document.read().layout_id_list(
                    &mut columns[2],
                    &cache[2],
                    &mut self.hovered_task,
                    &mut self.summary_actions_pending,
                    "CompletedScrollArea",
                );
            });
        }
    }

    pub fn layout_queue(&mut self, ui: &mut egui::Ui) {
        if let KanbanDocumentLayout::Queue(qs) = &mut self.current_layout {
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
            );
            // );
        }
    }
    pub fn layout_search(&mut self, ui: &mut egui::Ui) {
        let doc = self.document.read();
        if let KanbanDocumentLayout::Search(search_state) = &mut self.current_layout {
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
            );
        }
    }
    pub fn layout_focused(&mut self, ui: &mut egui::Ui) {
        if let KanbanDocumentLayout::Focused(focus) = &mut self.current_layout {
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
                    ));
                }

                self.document.read().layout_id_list(
                    &mut columns[0],
                    &focus.children,
                    &mut self.hovered_task,
                    &mut self.summary_actions_pending,
                    "ChildScroller",
                );

                self.document.read().layout_id_list(
                    &mut columns[2],
                    &focus.ancestors,
                    &mut self.hovered_task,
                    &mut self.summary_actions_pending,
                    "ParentScroller",
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
        let mut layout = KanbanDocumentLayout::Columnar([Vec::new(), vec![], vec![]]);
        layout.update_cache(
            &document,
            &ItemSort::None,
            &egui::Style::default(),
            &KanbanFilter::None,
        );
        if let KanbanDocumentLayout::Columnar(cache) = layout {
            assert_eq!(cache[0].len(), 2);
            assert_eq!(cache[1].len(), 1);
            assert_eq!(cache[2].len(), 1);
        }
    }
}
