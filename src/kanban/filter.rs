use egui::{ComboBox, Ui};
use log::{debug, info};
use std::cell::RefCell;

use super::*;
///A kanbanfilter
#[derive(PartialEq, Clone, Default)]
pub enum KanbanFilter {
    #[default]
    ///Matches every task
    None,
    ///Matches tasks which contain a specific string
    ContainsString(String),
    ///Matches tasks with a specific category
    MatchesCategory(String),
    ///Matches tasks that are related to a specified task
    RelatedTo(KanbanId),
    ///Matches tasks that is either completed or not, as specified
    CompletionStatus(bool),
    ///Matches tasks that match a specific name
    NameContains(String),
    ///Matches tasks with a specific tag
    TagContains(String),
    ///'Full text' matches tasks with a fuzzy matcher
    FuzzyMatch(String),
    ///'Full text' matches tasks with an exact match
    ExactMatch(String),
}

thread_local! {
    ///The fuzzy matcher state.
    static NUCLEO_MATCHER:RefCell<nucleo_matcher::Matcher> = RefCell::new(nucleo_matcher::Matcher::new(nucleo_matcher::Config::DEFAULT));
    ///A fuzzy matching pattern, as parsed
    static NUCLEO_PATTERN:RefCell<nucleo_matcher::pattern::Pattern>=RefCell::new(nucleo_matcher::pattern::Pattern::new("", nucleo_matcher::pattern::CaseMatching::Smart, nucleo_matcher::pattern::Normalization::Smart, nucleo_matcher::pattern::AtomKind::Fuzzy));
    ///A buffer to match a task with Nucleo-matcher
    static NUCLEO_BUFFER:RefCell<Vec<char>>=const {RefCell::new(Vec::new())};
    ///A buffer(read string) for exact matching
    static MATCH_BUFFER:RefCell<String>=const {RefCell::new(String::new())};
}
impl KanbanFilter {
    ///Returns the name of the filter type
    fn option_name(&self) -> &'static str {
        match self {
            Self::None => "No filter",
            Self::ContainsString(_) => "Contains string",
            Self::MatchesCategory(_) => "Matches Category",
            Self::RelatedTo(_) => "Related To",
            Self::CompletionStatus(true) => "Completed",
            Self::CompletionStatus(false) => "Uncompleted",
            Self::NameContains(_) => "Name Contains",
            Self::TagContains(_) => "Tag Contains",
            Self::FuzzyMatch(_) => "Fuzzy Match",
            Self::ExactMatch(_) => "Exact Match",
        }
    }
    ///Show the ui necessary to collect and display the filter settings
    pub fn show_ui(&mut self, ui: &mut Ui, _document: &KanbanDocument) -> egui::Response {
        let mut response: Option<Response> = None;
        ui.group(|ui| {
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
                        ui.selectable_value(
                            self,
                            Self::NameContains("".to_string()),
                            "Name Contains",
                        );
                        ui.selectable_value(
                            self,
                            Self::TagContains("".to_string()),
                            "Contains Tag",
                        );
                        ui.selectable_value(self, Self::FuzzyMatch("".to_string()), "Fuzzy Match");
                        ui.selectable_value(self, Self::ExactMatch("".to_string()), "Exact Match");
                    })
                    .response;
                // I need to report this to egui as this seems as if it shouldn't be necessary
                if *self != previous {
                    box_response.mark_changed();
                }
                let mut text_response: Option<Response> = None;
                let alter_match = matches!(self, Self::ExactMatch(_));
                match self {
                    Self::ContainsString(ref mut str)
                    | Self::MatchesCategory(ref mut str)
                    | Self::NameContains(ref mut str)
                    | Self::TagContains(ref mut str)
                    | Self::FuzzyMatch(ref mut str)
                    | Self::ExactMatch(ref mut str) => {
                        ui.allocate_ui(
                            Vec2::new(
                                ui.available_width() / 3. - ui.spacing().item_spacing.x,
                                ui.available_height(),
                            ),
                            |ui| {
                                text_response = Some(ui.text_edit_singleline(str));
                                if let Some(ref text_response) = text_response {
                                    if text_response.changed() {
                                        info!(target:"fuzzy_match","Search filter changed!");
                                        NUCLEO_PATTERN.with_borrow_mut(|x| {
                                            let str = if alter_match {
                                                // A few things should be escaped here, not the
                                                // least of which being the "'" character itself
                                                "'".to_owned() + str
                                            } else {
                                                str.clone()
                                            };
                                            x.reparse(
                                                &str,
                                                nucleo_matcher::pattern::CaseMatching::Smart,
                                                nucleo_matcher::pattern::Normalization::Smart,
                                            );
                                        });
                                    }
                                }
                            },
                        );
                    }

                    _ => {}
                }
                if let Some(tr) = text_response {
                    response = Some(tr.union(box_response));
                } else {
                    response = Some(box_response);
                }
            });
        });
        response.unwrap()
    }
    ///Returns true if a task matches the filter
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
            Self::NameContains(substr) => item.name.contains(substr.as_str()),
            Self::TagContains(tag) => item.tags.contains(tag),
            Self::FuzzyMatch(_pattern) => {
                MATCH_BUFFER.with_borrow_mut(|x| {
                    x.clear();
                    item.fill_searchable_buffer(x);
                    NUCLEO_BUFFER.with_borrow_mut(|buffer| {
                        buffer.clear();
                        buffer.extend(x.chars());
                    })
                });

                // TODO In the future, it would be useful if the patterns that were possible were
                // mentioned in the application. There isn't a large number of them and an explanation
                // should be possible
                let score = NUCLEO_MATCHER
                    .with_borrow_mut(|x| {
                        NUCLEO_PATTERN.with(|pattern| {
                            NUCLEO_BUFFER.with_borrow(|haystack| {
                                pattern
                                    .borrow()
                                    .score(nucleo_matcher::Utf32Str::Unicode(haystack), x)
                            })
                        })
                    })
                    .unwrap_or(0);
                debug!(target:"fuzzy_match", "'{}' has score {}", &item.name, score);
                score > 0
            }
            Self::ExactMatch(_pattern) => {
                MATCH_BUFFER.with_borrow_mut(|x| {
                    x.clear();
                    item.fill_searchable_buffer(x);
                    NUCLEO_BUFFER.with_borrow_mut(|buffer| {
                        buffer.clear();
                        buffer.extend(x.chars());
                    })
                });

                let score = NUCLEO_MATCHER
                    .with_borrow_mut(|x| {
                        NUCLEO_PATTERN.with(|pattern| {
                            NUCLEO_BUFFER.with_borrow(|haystack| {
                                pattern
                                    .borrow()
                                    .score(nucleo_matcher::Utf32Str::Unicode(haystack), x)
                            })
                        })
                    })
                    .unwrap_or(0);
                debug!(target:"fuzzy_match", "'{}' has score {}", &item.name, score);
                score > 0
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
    #[test]
    fn test_name_contains() {
        let document = get_test_document();
        let filter = KanbanFilter::NameContains("Name".to_string());
        assert_eq!(
            document
                .get_tasks()
                .filter(|x| filter.matches(x, &document))
                .count(),
            1
        );
    }
    #[test]
    fn test_tag_contains() {
        let document = get_test_document();
        let filter = KanbanFilter::TagContains(TEST_TAG.to_string());
        assert_eq!(
            document
                .get_tasks()
                .filter(|x| filter.matches(x, &document))
                .count(),
            1
        );
    }
}
