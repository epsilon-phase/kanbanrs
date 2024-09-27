use super::*;
#[derive(Clone)]
pub enum KanbanDocumentLayout {
    Queue(kanban::queue_view::QueueState),
    Columnar([Vec<i32>; 3]),
    Search(kanban::search::SearchState),
    Focused(kanban::focused_layout::Focus),
}
impl PartialEq for KanbanDocumentLayout {
    fn eq(&self, other: &Self) -> bool {
        match self {
            KanbanDocumentLayout::Columnar(_) => match other {
                KanbanDocumentLayout::Columnar(_) => true,
                _ => false,
            },
            KanbanDocumentLayout::Queue(_) => match other {
                KanbanDocumentLayout::Queue(_) => true,
                _ => false,
            },
            KanbanDocumentLayout::Search(_) => match other {
                KanbanDocumentLayout::Search(_) => true,
                _ => false,
            },
            KanbanDocumentLayout::Focused(_) => match other {
                KanbanDocumentLayout::Focused(_) => true,
                _ => false,
            },
        }
    }
}
impl KanbanDocumentLayout {
    fn update_columnar(columnar_cache: &mut [Vec<i32>; 3], document: &KanbanDocument) {
        columnar_cache.iter_mut().for_each(|x| x.clear());
        for task in document.get_tasks() {
            let index = match document.task_status(&task.id) {
                kanban::Status::Ready => 0,
                kanban::Status::Blocked => 1,
                kanban::Status::Completed => 2,
            };
            columnar_cache[index].push(task.id);
        }
    }
    pub fn inform_of_new_items(&mut self) {
        match self {
            KanbanDocumentLayout::Search(x) => x.force_update(),
            _ => (),
        }
    }
    pub fn update_cache(&mut self, document: &KanbanDocument) {
        match self {
            KanbanDocumentLayout::Queue(x) => {
                x.update(document);
            }
            KanbanDocumentLayout::Columnar(array) => {
                KanbanDocumentLayout::update_columnar(array, document);
            }
            KanbanDocumentLayout::Search(search_state) => {
                search_state.update(document);
            }
            KanbanDocumentLayout::Focused(focus) => {
                focus.update(document);
            }
        }
    }

    pub fn sort_cache(&mut self, document: &KanbanDocument, sort: &ItemSort) {
        match self {
            KanbanDocumentLayout::Columnar(array) => array
                .iter_mut()
                .for_each(|item| sort.sort_by(item, document)),
            KanbanDocumentLayout::Focused(focus) => {
                sort.sort_by(&mut focus.children, &document);
                sort.sort_by(&mut focus.ancestors, &document);
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
                egui::ScrollArea::vertical()
                    .id_source("ReadyScrollarea")
                    .show_rows(&mut columns[0], 200., cache[0].len(), |ui, range| {
                        self.document.layout_id_list(
                            ui,
                            &cache[0],
                            range,
                            &mut self.hovered_task,
                            &mut self.summary_actions_pending,
                        );
                    });

                columns[1].label(RichText::new("Blocked").heading());
                egui::ScrollArea::vertical()
                    .id_source("BlockedScrollArea")
                    .show_rows(&mut columns[1], 200., cache[1].len(), |ui, range| {
                        self.document.layout_id_list(
                            ui,
                            &cache[1],
                            range,
                            &mut self.hovered_task,
                            &mut self.summary_actions_pending,
                        );
                    });
                columns[2].label(RichText::new("Completed").heading());
                egui::ScrollArea::vertical()
                    .id_source("CompletedScrollArea")
                    .show_rows(&mut columns[2], 200., cache[2].len(), |ui, range| {
                        self.document.layout_id_list(
                            ui,
                            &cache[2],
                            range,
                            &mut self.hovered_task,
                            &mut self.summary_actions_pending,
                        );
                    });
            });
        }
    }

    pub fn layout_queue(&mut self, ui: &mut egui::Ui) {
        if let KanbanDocumentLayout::Queue(qs) = &mut self.current_layout {
            ScrollArea::vertical().id_source("Queue").show_rows(
                ui,
                200.0,
                qs.cached_ready.len(),
                |ui, range| {
                    self.document.layout_id_list(
                        ui,
                        &qs.cached_ready,
                        range,
                        &mut self.hovered_task,
                        &mut self.summary_actions_pending,
                    );
                },
            );
        }
    }
    pub fn layout_search(&mut self, ui: &mut egui::Ui) {
        if let KanbanDocumentLayout::Search(search_state) = &mut self.current_layout {
            ui.horizontal(|ui| {
                let label = ui.label("Search");
                ui.text_edit_singleline(&mut search_state.search_prompt)
                    .labelled_by(label.id);
                search_state.update(&self.document);
            });
            ScrollArea::vertical().id_source("SearchArea").show_rows(
                ui,
                200.0,
                search_state.matched_ids.len(),
                |ui, range| {
                    self.document.layout_id_list(
                        ui,
                        &search_state.matched_ids,
                        range,
                        &mut self.hovered_task,
                        &mut self.summary_actions_pending,
                    );
                },
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
                    let task = self.document.get_task(target).unwrap();
                    task.summary(&self.document, &mut self.hovered_task, &mut columns[1]);
                }

                ScrollArea::vertical().id_source("ChildScroller").show_rows(
                    &mut columns[0],
                    200.0,
                    focus.children.len(),
                    |ui, range| {
                        self.document.layout_id_list(
                            ui,
                            &focus.children,
                            range,
                            &mut self.hovered_task,
                            &mut self.summary_actions_pending,
                        );
                    },
                );
                ScrollArea::vertical()
                    .id_source("ParentScroller")
                    .show_rows(
                        &mut columns[2],
                        200.0,
                        focus.ancestors.len(),
                        |ui, range| {
                            self.document.layout_id_list(
                                ui,
                                &focus.ancestors,
                                range,
                                &mut self.hovered_task,
                                &mut self.summary_actions_pending,
                            );
                        },
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
        layout.update_cache(&document);
        if let KanbanDocumentLayout::Columnar(cache) = layout {
            assert_eq!(cache[0].len(), 2);
            assert_eq!(cache[1].len(), 1);
            assert_eq!(cache[2].len(), 1);
        }
    }
}
