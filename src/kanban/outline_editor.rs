use std::collections::{HashMap, HashSet};

use eframe::egui::{self, ComboBox, Margin, RichText};
use filter::KanbanFilter;
use sorting::ItemSort;

use super::*;

/// Indentation width per depth level, in points.
const INDENT: f32 = 20.0;

/// Which part of a task is currently being edited inline.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditField {
    Name,
    Description,
}

/// A text-centric outline view of the kanban document.
#[derive(Default, Clone)]
pub struct OutlineEditor {
    /// Set of task IDs whose children are collapsed.
    collapsed: HashSet<KanbanId>,
    /// Which task currently has keyboard focus for navigation.
    focused_id: Option<KanbanId>,
    /// Which task & field is currently being inline-edited (`None` = navigate mode).
    editing: Option<(KanbanId, EditField)>,
    /// Draft text for inline editors, keyed by task id.
    draft_names: HashMap<KanbanId, String>,
    draft_descriptions: HashMap<KanbanId, String>,
    /// Scratch buffer used to build the visible task list each frame.
    visible_tasks: Vec<KanbanId>,
    /// Maps each visible task to its parent in the outline (for sibling creation).
    task_parents: HashMap<KanbanId, KanbanId>,
    /// A newly-created task id that should be focused on the next frame.
    pending_focus: Option<KanbanId>,
    /// Whether the priority dropdown is open for a given task.
    editing_priority: Option<KanbanId>,
    /// Whether the category dropdown is open for a given task.
    editing_category: Option<KanbanId>,
    /// Draft for a new category name.
    new_category_draft: String,
    /// Monotonically increasing render sequence counter to disambiguate widget IDs.
    render_seq: u32,
    /// Maps each visible task to its render-order sequence number this frame.
    task_seq: HashMap<KanbanId, u32>,
}

impl OutlineEditor {
    pub fn new() -> Self {
        Self {
            collapsed: HashSet::new(),
            focused_id: None,
            editing: None,
            draft_names: HashMap::new(),
            draft_descriptions: HashMap::new(),
            visible_tasks: Vec::new(),
            task_parents: HashMap::new(),
            pending_focus: None,
            editing_priority: None,
            editing_category: None,
            new_category_draft: String::new(),
            render_seq: 0,
            task_seq: HashMap::new(),
        }
    }

    /// Toggle the collapsed state of a task's children.
    pub fn toggle_collapsed(&mut self, id: KanbanId) {
        if self.collapsed.contains(&id) {
            self.collapsed.remove(&id);
        } else {
            self.collapsed.insert(id);
        }
    }

    /// Focus a newly created task for immediate editing.
    pub fn focus_new_task(&mut self, task_id: KanbanId, _document: &KanbanDocument) {
        self.pending_focus = Some(task_id);
        self.focused_id = Some(task_id);
        self.draft_names.insert(task_id, String::new());
        self.editing = Some((task_id, EditField::Name));
    }

    /// Start editing a specific field of a task inline.
    fn start_editing(&mut self, task_id: KanbanId, field: EditField, document: &KanbanDocument) {
        if let Some(task) = document.get_task(task_id) {
            match field {
                EditField::Name => {
                    self.draft_names.insert(task_id, task.name.clone());
                }
                EditField::Description => {
                    self.draft_descriptions
                        .insert(task_id, task.description.clone());
                }
            }
            self.editing = Some((task_id, field));
            self.focused_id = Some(task_id);
        }
    }

    /// Commit the current inline edit, emitting an update command.
    fn commit_edit(
        &mut self,
        task_id: KanbanId,
        field: EditField,
        document: &KanbanDocument,
        actions: &mut Vec<AppCommand>,
    ) {
        if let Some(task) = document.get_task(task_id) {
            let mut updated = task.clone();
            let mut changed = false;

            match field {
                EditField::Name => {
                    if let Some(draft) = self.draft_names.remove(&task_id) {
                        if task.name != draft {
                            updated.name = draft;
                            changed = true;
                        }
                    }
                }
                EditField::Description => {
                    if let Some(draft) = self.draft_descriptions.remove(&task_id) {
                        if task.description != draft {
                            updated.description = draft;
                            changed = true;
                        }
                    }
                }
            }

            if changed {
                actions.push(AppCommand::UpdateTask(updated));
            }
        }
        self.editing = None;
    }

    /// Cancel the current inline edit without saving.
    fn cancel_edit(&mut self) {
        if let Some((id, field)) = self.editing {
            match field {
                EditField::Name => {
                    self.draft_names.remove(&id);
                }
                EditField::Description => {
                    self.draft_descriptions.remove(&id);
                }
            }
        }
        self.editing = None;
    }

    /// Move focus to the previous visible task.
    fn focus_prev(&mut self) {
        if let Some(focused) = self.focused_id {
            if let Some(pos) = self.visible_tasks.iter().position(|&id| id == focused) {
                if pos > 0 {
                    self.focused_id = Some(self.visible_tasks[pos - 1]);
                }
            }
        } else if let Some(&first) = self.visible_tasks.first() {
            self.focused_id = Some(first);
        }
    }

    /// Move focus to the next visible task.
    fn focus_next(&mut self) {
        if let Some(focused) = self.focused_id {
            if let Some(pos) = self.visible_tasks.iter().position(|&id| id == focused) {
                if pos + 1 < self.visible_tasks.len() {
                    self.focused_id = Some(self.visible_tasks[pos + 1]);
                }
            }
        } else if let Some(&first) = self.visible_tasks.first() {
            self.focused_id = Some(first);
        }
    }

    /// Compute the next edit target when Tab is pressed.
    fn tab_next(
        &self,
        current_task: KanbanId,
        current_field: Option<EditField>,
    ) -> Option<(KanbanId, EditField)> {
        let task_pos = self
            .visible_tasks
            .iter()
            .position(|&id| id == current_task)?;

        match current_field {
            Some(EditField::Name) => {
                // Name → Description (same task)
                Some((current_task, EditField::Description))
            }
            Some(EditField::Description) | None => {
                // Description or navigate mode → next task's Name
                let next_pos = if task_pos + 1 < self.visible_tasks.len() {
                    task_pos + 1
                } else {
                    0 // wrap around
                };
                Some((self.visible_tasks[next_pos], EditField::Name))
            }
        }
    }

    /// Compute the previous edit target when Shift+Tab is pressed.
    fn tab_prev(
        &self,
        current_task: KanbanId,
        current_field: Option<EditField>,
    ) -> Option<(KanbanId, EditField)> {
        let task_pos = self
            .visible_tasks
            .iter()
            .position(|&id| id == current_task)?;

        match current_field {
            Some(EditField::Description) => {
                // Description → Name (same task)
                Some((current_task, EditField::Name))
            }
            Some(EditField::Name) | None => {
                // Name or navigate mode → previous task's Description
                let prev_pos = if task_pos > 0 {
                    task_pos - 1
                } else {
                    self.visible_tasks.len().saturating_sub(1)
                };
                Some((self.visible_tasks[prev_pos], EditField::Description))
            }
        }
    }

    /// Request focus on a specific task's editable field.
    fn request_focus_on(&self, ctx: &egui::Context, task_id: KanbanId, field: EditField) {
        let seq = self.task_seq.get(&task_id).copied().unwrap_or(0);
        let id = match field {
            EditField::Name => egui::Id::new(("outline_name", task_id, seq)),
            EditField::Description => egui::Id::new(("outline_desc", task_id, seq)),
        };
        ctx.memory_mut(|m| m.request_focus(id));
    }

    /// Render a bold `[label]` clickable as a TUI-style link.
    fn tui_link(ui: &mut egui::Ui, label: &str) -> egui::Response {
        ui.add(
            egui::Label::new(RichText::new(format!("[{}]", label)).strong().monospace())
                .sense(egui::Sense::click()),
        )
    }

    /// Render the full outline into the given UI.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        document: &KanbanDocument,
        actions: &mut Vec<AppCommand>,
        scroll_to: &Option<KanbanId>,
        sort: ItemSort,
        filter: &KanbanFilter,
    ) {
        // Rebuild the visible task list for keyboard navigation.
        self.visible_tasks.clear();
        self.task_parents.clear();
        self.task_seq.clear();
        self.render_seq = 0;

        // Collect root tasks (those that are not children of any other task)
        let mut children_of_something: HashSet<KanbanId> = HashSet::new();
        document.get_tasks().for_each(|task| {
            children_of_something.extend(task.child_tasks.iter());
        });

        let mut roots: Vec<KanbanId> = document
            .get_tasks()
            .filter(|x| filter.matches(x, document))
            .filter(|x| !children_of_something.contains(&x.id))
            .map(|x| x.id)
            .collect();

        sort.sort_by(&mut roots, document);

        // Gather visible tasks for keyboard nav.
        for &root_id in &roots {
            self.gather_visible(root_id, 0, document, sort, filter, None);
        }

        // Apply pending focus from a newly created task.
        if let Some(pending_id) = self.pending_focus.take() {
            self.focused_id = Some(pending_id);
            self.editing = Some((pending_id, EditField::Name));
            self.request_focus_on(ui.ctx(), pending_id, EditField::Name);
        }

        // ── Tab focus cycling (constrained to outline) ──
        if ui.input(|i| i.key_pressed(egui::Key::Tab)) {
            let shift = ui.input(|i| i.modifiers.shift);
            let target = if let Some((task_id, field)) = self.editing {
                if shift {
                    self.tab_prev(task_id, Some(field))
                } else {
                    self.tab_next(task_id, Some(field))
                }
            } else if let Some(focused) = self.focused_id {
                if shift {
                    self.tab_prev(focused, None)
                } else {
                    self.tab_next(focused, None)
                }
            } else if let Some(&first) = self.visible_tasks.first() {
                Some((first, EditField::Name))
            } else {
                None
            };

            if let Some((task_id, field)) = target {
                // Commit any ongoing edit before switching
                if let Some((current_task, current_field)) = self.editing {
                    self.commit_edit(current_task, current_field, document, actions);
                }
                self.start_editing(task_id, field, document);
                self.request_focus_on(ui.ctx(), task_id, field);
            }
        }

        // ── Other keyboard shortcuts ──
        if self.editing.is_some() {
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.cancel_edit();
            }
        } else {
            if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                self.focus_prev();
            }
            if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                self.focus_next();
            }
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if let Some(focused) = self.focused_id {
                    self.start_editing(focused, EditField::Name, document);
                    self.request_focus_on(ui.ctx(), focused, EditField::Name);
                }
            }
            if ui.input(|i| i.key_pressed(egui::Key::Space)) {
                if let Some(focused) = self.focused_id {
                    actions.push(AppCommand::MarkCompleted(focused));
                }
            }
        }

        ScrollArea::vertical()
            .id_salt("OutlineEditor")
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                for &root_id in &roots {
                    self.render_task(
                        root_id, 0, ui, document, actions, scroll_to, sort, filter, None,
                    );
                }
            });
    }

    /// Recursively gather visible task IDs for keyboard navigation.
    fn gather_visible(
        &mut self,
        task_id: KanbanId,
        depth: u32,
        document: &KanbanDocument,
        sort: ItemSort,
        filter: &KanbanFilter,
        parent_id: Option<KanbanId>,
    ) {
        let Some(task) = document.get_task(task_id) else {
            return;
        };
        if !filter.matches(task, document) {
            return;
        }
        self.visible_tasks.push(task_id);
        self.task_seq.insert(task_id, self.render_seq);
        self.render_seq += 1;
        if let Some(parent) = parent_id {
            self.task_parents.insert(task_id, parent);
        }

        if !self.collapsed.contains(&task_id) {
            let mut children: Vec<KanbanId> = task.child_tasks.iter().copied().collect();
            sort.sort_by(&mut children, document);
            for &child_id in &children {
                self.gather_visible(child_id, depth + 1, document, sort, filter, Some(task_id));
            }
        }
    }

    /// Recursively render a single task and its children.
    #[allow(clippy::too_many_arguments)]
    fn render_task(
        &mut self,
        task_id: KanbanId,
        depth: u32,
        ui: &mut egui::Ui,
        document: &KanbanDocument,
        actions: &mut Vec<AppCommand>,
        scroll_to: &Option<KanbanId>,
        sort: ItemSort,
        filter: &KanbanFilter,
        _parent_id: Option<KanbanId>,
    ) {
        let Some(task) = document.get_task(task_id) else {
            return;
        };

        if !filter.matches(task, document) {
            return;
        }

        if scroll_to.is_some_and(|x| x == task_id) {
            ui.scroll_to_cursor(Some(Align::TOP));
        }

        let seq = self.task_seq.get(&task_id).copied().unwrap_or(0);
        let is_collapsed = self.collapsed.contains(&task_id);
        let has_children = !task.child_tasks.is_empty();
        let is_focused = self.focused_id == Some(task_id);
        let is_editing_name = self.editing == Some((task_id, EditField::Name));
        let is_editing_description = self.editing == Some((task_id, EditField::Description));
        let is_editing_priority = self.editing_priority == Some(task_id);
        let is_editing_category = self.editing_category == Some(task_id);

        // Frame to group the task visually; highlight when focused.
        let frame = if is_focused && self.editing.is_none() {
            egui::Frame::new()
                .inner_margin(Margin::symmetric(4, 2))
                .outer_margin(egui::Vec2::new(0.0, 1.0))
                .fill(ui.visuals().selection.bg_fill)
        } else {
            egui::Frame::new()
                .inner_margin(Margin::symmetric(4, 2))
                .outer_margin(egui::Vec2::new(0.0, 1.0))
        };

        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                // Indentation
                ui.add_space(depth as f32 * INDENT);

                // Collapse/expand button
                if has_children {
                    let button_text = if is_collapsed { "▶" } else { "▼" };
                    if ui.small_button(button_text).clicked() {
                        self.toggle_collapsed(task_id);
                    }
                } else {
                    ui.add_space(ui.spacing().interact_size.x);
                }

                // Completion checkbox
                let mut completed = task.completed.is_some();
                if ui.checkbox(&mut completed, "").changed() {
                    actions.push(AppCommand::MarkCompleted(task_id));
                }

                // Task name: editable when editing, clickable label otherwise.
                if is_editing_name {
                    let draft = self
                        .draft_names
                        .entry(task_id)
                        .or_insert_with(|| task.name.clone());
                    let output = egui::TextEdit::singleline(draft)
                        .id(egui::Id::new(("outline_name", task_id, seq)))
                        .desired_width(f32::INFINITY)
                        .show(ui);

                    // Enter creates a sibling if cursor is at end and name is non-empty.
                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if !draft.trim().is_empty() {
                            self.commit_edit(task_id, EditField::Name, document, actions);
                            actions.push(AppCommand::CreateSiblingOf(task_id));
                        }
                    }
                    // Backspace on empty name deletes the task.
                    else if ui.input(|i| i.key_pressed(egui::Key::Backspace)) && draft.is_empty()
                    {
                        actions.push(AppCommand::DeleteTask(task.clone()));
                        self.cancel_edit();
                        // Move focus to previous task, or next if no previous.
                        if let Some(pos) = self.visible_tasks.iter().position(|&id| id == task_id) {
                            if pos > 0 {
                                self.focused_id = Some(self.visible_tasks[pos - 1]);
                            } else if pos + 1 < self.visible_tasks.len() {
                                self.focused_id = Some(self.visible_tasks[pos + 1]);
                            } else {
                                self.focused_id = None;
                            }
                        }
                    }
                    // Lost focus commits the edit.
                    else if output.response.lost_focus() {
                        self.commit_edit(task_id, EditField::Name, document, actions);
                    }
                } else {
                    let name_text = if task.completed.is_some() {
                        RichText::new(&task.name).strikethrough()
                    } else {
                        RichText::new(&task.name)
                    };
                    let name_response = ui.selectable_label(is_focused, name_text);
                    if name_response.clicked() {
                        self.start_editing(task_id, EditField::Name, document);
                        self.request_focus_on(ui.ctx(), task_id, EditField::Name);
                    }
                    if name_response.clicked_by(egui::PointerButton::Primary) {
                        self.focused_id = Some(task_id);
                    }
                }

                // Priority: inline editable
                if is_editing_priority {
                    let mut new_priority = task.priority.clone();
                    ComboBox::from_id_salt(("outline_prio", task_id, seq))
                        .selected_text(new_priority.as_deref().unwrap_or("None"))
                        .width(80.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut new_priority, None, "None");
                            for (name, _) in document.get_sorted_priorities() {
                                ui.selectable_value(&mut new_priority, Some(name.clone()), name);
                            }
                        });
                    if new_priority != task.priority {
                        let mut updated = task.clone();
                        updated.priority = new_priority;
                        actions.push(AppCommand::UpdateTask(updated));
                        self.editing_priority = None;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        self.editing_priority = None;
                    }
                } else {
                    let prio_text = format!("— {}", task.priority.as_deref().unwrap_or("—"));
                    let prio_response = ui.add(
                        egui::Label::new(
                            RichText::new(prio_text)
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        )
                        .sense(egui::Sense::click()),
                    );
                    if prio_response.clicked() {
                        self.editing_priority = Some(task_id);
                        self.focused_id = Some(task_id);
                    }
                }

                // Category: inline editable
                if is_editing_category {
                    let mut new_category = task.category.clone();
                    ComboBox::from_id_salt(("outline_cat", task_id, seq))
                        .selected_text(new_category.as_deref().unwrap_or("None"))
                        .width(100.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut new_category, None, "None");
                            for name in document.get_categories().keys() {
                                ui.selectable_value(&mut new_category, Some(name.clone()), name);
                            }
                            ui.separator();
                            ui.label("New category:");
                            ui.text_edit_singleline(&mut self.new_category_draft);
                            if ui.button("Add").clicked()
                                && !self.new_category_draft.trim().is_empty()
                            {
                                new_category = Some(self.new_category_draft.trim().to_owned());
                                self.new_category_draft.clear();
                            }
                        });
                    if new_category != task.category {
                        let mut updated = task.clone();
                        updated.category = new_category;
                        actions.push(AppCommand::UpdateTask(updated));
                        self.editing_category = None;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        self.editing_category = None;
                    }
                } else {
                    let cat_display = task
                        .category
                        .as_deref()
                        .map(|c| format!("[{}]", c))
                        .unwrap_or_else(|| "[]".to_string());
                    let mut cat_text = RichText::new(cat_display).small();
                    if let Some(ref category) = task.category {
                        if let Some(style) = document.categories.get(category) {
                            let mut stroke = ui.visuals().noninteractive().bg_stroke;
                            let mut fill = ui.visuals().panel_fill;
                            let mut text_color = ui.visuals().text_color();
                            style.apply_to(&mut stroke, &mut fill, &mut text_color);
                            cat_text = cat_text.color(text_color);
                        }
                    }
                    let cat_response =
                        ui.add(egui::Label::new(cat_text).sense(egui::Sense::click()));
                    if cat_response.clicked() {
                        self.editing_category = Some(task_id);
                        self.focused_id = Some(task_id);
                    }
                }
            });

            // Tags
            if !task.tags.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(depth as f32 * INDENT + INDENT + ui.spacing().interact_size.x);
                    ui.label(
                        RichText::new(task.tags.join(", "))
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    );
                });
            }

            // Description
            if is_editing_description {
                ui.horizontal(|ui| {
                    ui.add_space(depth as f32 * INDENT + INDENT + ui.spacing().interact_size.x);
                    let draft = self
                        .draft_descriptions
                        .entry(task_id)
                        .or_insert_with(|| task.description.clone());
                    let output = egui::TextEdit::multiline(draft)
                        .id(egui::Id::new(("outline_desc", task_id, seq)))
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .lock_focus(true) // Tab inserts tab characters in description
                        .show(ui);

                    // Lost focus commits the edit.
                    if output.response.lost_focus() {
                        self.commit_edit(task_id, EditField::Description, document, actions);
                    }
                });
            } else if !task.description.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(depth as f32 * INDENT + INDENT + ui.spacing().interact_size.x);
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    let desc_response = ui.add(
                        egui::Label::new(
                            RichText::new(&task.description)
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        )
                        .sense(egui::Sense::click()),
                    );
                    if desc_response.clicked() {
                        self.start_editing(task_id, EditField::Description, document);
                        self.request_focus_on(ui.ctx(), task_id, EditField::Description);
                    }
                    if desc_response.clicked_by(egui::PointerButton::Primary) {
                        self.focused_id = Some(task_id);
                    }
                });
            }

            // Action links row: [Edit] [Add Child] [Focus] [Scroll to]
            ui.horizontal(|ui| {
                ui.add_space(depth as f32 * INDENT + INDENT + ui.spacing().interact_size.x);
                if Self::tui_link(ui, "Edit").clicked() {
                    actions.push(AppCommand::OpenEditor(task_id));
                }
                ui.add_space(4.0);
                if Self::tui_link(ui, "Add Child").clicked() {
                    actions.push(AppCommand::CreateChildOf(task_id));
                }
                ui.add_space(4.0);
                if Self::tui_link(ui, "Focus").clicked() {
                    actions.push(AppCommand::FocusOn(task_id));
                }
            });
        });

        // Recurse into children if not collapsed
        if !is_collapsed && has_children {
            let mut children: Vec<KanbanId> = task.child_tasks.iter().copied().collect();
            sort.sort_by(&mut children, document);
            for &child_id in &children {
                self.render_task(
                    child_id,
                    depth + 1,
                    ui,
                    document,
                    actions,
                    scroll_to,
                    sort,
                    filter,
                    Some(task_id),
                );
            }
        }
    }
}
