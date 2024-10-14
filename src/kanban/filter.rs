use egui::{ComboBox, Ui};

use super::*;
#[derive(PartialEq, Clone)]
pub enum KanbanFilter {
    None,
    ContainsString(String),
    MatchesCategory(String),
    RelatedTo(KanbanId),
    CompletionStatus(bool),
}

impl Default for KanbanFilter {
    fn default() -> Self {
        Self::None
    }
}
impl KanbanFilter {
    fn option_name(&self) -> &'static str {
        match self {
            Self::None => "No filter",
            Self::ContainsString(_) => "Contains string",
            Self::MatchesCategory(_) => "Matches Category",
            Self::RelatedTo(_) => "Related To",
            Self::CompletionStatus(true) => "Completed",
            Self::CompletionStatus(false) => "Uncompleted",
        }
    }
    pub fn show_ui(&mut self, ui: &mut Ui, document: &KanbanDocument) -> egui::Response {
        let mut response: Option<Response> = None;
        ui.horizontal_wrapped(|ui| {
            let previous = self.clone();
            let mut box_response = ComboBox::new("Filter Select", "Select filter type")
                .selected_text(self.option_name())
                .show_ui(ui, |ui| {
                    ui.selectable_value(self, Self::None, "None");
                    ui.selectable_value(
                        self,
                        Self::ContainsString("".to_owned()),
                        "Contains String",
                    );
                    ui.selectable_value(
                        self,
                        Self::MatchesCategory("".to_owned()),
                        "Matches Category",
                    );
                    ui.selectable_value(self, Self::CompletionStatus(true), "Completed");
                    ui.selectable_value(self, Self::CompletionStatus(false), "Uncompleted");
                })
                .response;
            // I need to report this to egui as this seems as if it shouldn't be necessary
            if *self != previous {
                box_response.mark_changed();
            }
            let mut text_response: Option<Response> = None;
            match self {
                Self::ContainsString(ref mut str) => {
                    text_response = Some(ui.text_edit_singleline(str));
                }
                Self::MatchesCategory(ref mut str) => {
                    text_response = Some(ui.text_edit_singleline(str));
                }
                _ => {}
            }
            if let Some(tr) = text_response {
                response = Some(tr.union(box_response));
            } else {
                response = Some(box_response);
            }
        });
        response.unwrap()
    }
    pub fn matches(&self, item: &KanbanItem, document: &KanbanDocument) -> bool {
        match self {
            KanbanFilter::None => true,
            KanbanFilter::ContainsString(str) => {
                let mut s: String = String::new();
                item.fill_searchable_buffer(&mut s);
                s.contains(str)
            }
            KanbanFilter::MatchesCategory(category) => item
                .category
                .as_ref()
                .is_some_and(|x| x.eq(category.as_str())),
            Self::RelatedTo(id) => document.get_relation(*id, item.id) != TaskRelation::Unrelated,
            Self::CompletionStatus(completion_status) => {
                if *completion_status {
                    item.completed.is_some()
                } else {
                    item.completed.is_none()
                }
            }
        }
    }
}
#[cfg(test)]
mod test {
    use super::*;
    const TEST_TAG: &str = "The tag";
    const TEST_DESCRIPTION: &str = "Hey";
    const TEST_NAME: &str = "Name";
    const TEST_CATEGORY: &str = "Category";
    const TEST_ITEM_COUNT: usize = 3;

    fn get_test_document() -> KanbanDocument {
        let mut document = KanbanDocument::new();
        let mut a = document.get_new_task();
        a.name = "Name".to_owned();
        a.tags.push("The tag".to_owned());
        let mut b = document.get_new_task();
        b.description = "Hey".to_owned();
        a.add_child(&b);
        document.replace_task(&a);
        document.replace_task(&b);
        let mut c = document.get_new_task();
        c.category = Some(TEST_CATEGORY.to_owned());
        document.replace_task(&c);
        assert!(document.tasks.len() == TEST_ITEM_COUNT);
        document
    }
    #[test]
    fn test_contains_string() {
        let document = get_test_document();
        let tag_test = TEST_TAG.to_owned();
        let category_test = TEST_CATEGORY.to_owned();
        let name_test = "Name".to_owned();
        let description_test = TEST_DESCRIPTION.to_owned();
        let tag_filter = KanbanFilter::ContainsString(tag_test);
        let name_filter = KanbanFilter::ContainsString(name_test);
        let description_filter = KanbanFilter::ContainsString(description_test);
        let category_filter = KanbanFilter::ContainsString(category_test);
        let name_matches: Vec<KanbanItem> = document
            .get_tasks()
            .filter(|x| name_filter.matches(x, &document))
            .cloned()
            .collect();
        assert_eq!(name_matches[0].name.as_str(), TEST_NAME);
        let tag_matches: Vec<KanbanItem> = document
            .get_tasks()
            .filter(|x| tag_filter.matches(x, &document))
            .cloned()
            .collect();
        assert_eq!(tag_matches[0].tags[0].as_str(), TEST_TAG);
        let description_matches: Vec<KanbanItem> = document
            .get_tasks()
            .filter(|x| description_filter.matches(x, &document))
            .cloned()
            .collect();
        assert_eq!(description_matches[0].description, TEST_DESCRIPTION);
        let category_matches: Vec<KanbanItem> = document
            .get_tasks()
            .filter(|x| category_filter.matches(x, &document))
            .cloned()
            .collect();
        assert!(category_matches[0]
            .category
            .as_ref()
            .is_some_and(|x| x == TEST_CATEGORY));
    }
    #[test]
    fn test_category() {
        let document = get_test_document();
        let category_filter = KanbanFilter::MatchesCategory(TEST_CATEGORY.to_owned());
        let matches: Vec<KanbanItem> = document
            .get_tasks()
            .filter(|x| category_filter.matches(x, &document))
            .cloned()
            .collect();
        assert_eq!(matches.len(), 1);
    }
    #[test]
    fn test_none_filter() {
        let document = get_test_document();
        let tasks: Vec<KanbanItem> = document
            .get_tasks()
            .filter(|x| KanbanFilter::None.matches(x, &document))
            .cloned()
            .collect();
        assert_eq!(tasks.len(), TEST_ITEM_COUNT);
    }
    #[test]
    fn test_related_to() {
        let document = get_test_document();
        let parent_id = document.get_tasks().next().unwrap().id;
        let child_id = document.get_tasks().nth(1).unwrap().id;
        let filter = KanbanFilter::RelatedTo(parent_id);
        assert_eq!(
            document
                .get_tasks()
                .filter(|x| filter.matches(x, &document))
                .count(),
            2
        );
        // Do the reciprocal
        let filter = KanbanFilter::RelatedTo(child_id);
        assert_eq!(
            document
                .get_tasks()
                .filter(|x| filter.matches(x, &document))
                .count(),
            2
        );
    }
}
