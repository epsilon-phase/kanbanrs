mod kanban;
use chrono::Utc;
use circular_buffer::CircularBuffer;
use clap::*;
use eframe::egui::{self, ComboBox, RichText, Vec2};
use kanban::{
    category_editor::State, editor::EditorRequest, filter::KanbanFilter, node_layout::NodeLayout,
    priority_editor::PriorityEditor, queue_view::QueueState, search::SearchState,
    sorting::ItemSort, tree_outline_layout::TreeOutline, undo::CreationEvent, KanbanDocument,
    SummaryAction,
};
use parking_lot::RwLock;
use std::{
    borrow::BorrowMut,
    fs,
    io::Write,
    path::PathBuf,
    sync::{mpsc, Arc},
};
mod document_layout;
use document_layout::*;

struct KanbanRS {
    document: Arc<RwLock<KanbanDocument>>,
    task_name: String,
    open_editors: Vec<Arc<RwLock<kanban::editor::State>>>,
    save_file_name: Option<PathBuf>,
    current_layout: KanbanDocumentLayout,
    #[cfg(unix)]
    base_dirs: xdg::BaseDirectories,
    hovered_task: Option<i32>,
    close_application: bool,
    layout_cache_needs_updating: bool,
    // Both of these might merit renaming at some point
    summary_actions_pending: Vec<SummaryAction>,
    sorting_type: kanban::sorting::ItemSort,
    category_editor: kanban::category_editor::State,
    priority_editor: PriorityEditor,
    modified_since_last_saved: bool,
    editor_rx: std::sync::mpsc::Receiver<EditorRequest>,
    editor_tx: std::sync::mpsc::Sender<EditorRequest>,
    undo_buffer: CircularBuffer<35, kanban::undo::UndoItem>,
    filter: kanban::filter::KanbanFilter,
}
impl KanbanRS {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        KanbanRS {
            document: Arc::new(RwLock::new(KanbanDocument::default())),
            task_name: String::new(),
            open_editors: Vec::new(),
            save_file_name: None,
            current_layout: KanbanDocumentLayout::default(),
            #[cfg(unix)]
            base_dirs: xdg::BaseDirectories::with_prefix("kanbanrs").unwrap(),
            hovered_task: None,
            close_application: false,
            layout_cache_needs_updating: true,
            summary_actions_pending: Vec::new(),
            sorting_type: kanban::sorting::ItemSort::None,
            category_editor: State::new(),
            priority_editor: PriorityEditor::new(),
            modified_since_last_saved: false,
            editor_rx: rx,
            editor_tx: tx,
            undo_buffer: CircularBuffer::new(),
            filter: KanbanFilter::None,
        }
    }
}
#[derive(clap::Parser, PartialEq, Eq, Clone, Copy, Debug, ValueEnum)]
enum StartupLayout {
    Node,
    Column,
    TreeOutline,
    Queue,
    Search,
}
impl std::fmt::Display for StartupLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
impl From<StartupLayout> for KanbanDocumentLayout {
    fn from(value: StartupLayout) -> Self {
        match value {
            StartupLayout::Column => {
                KanbanDocumentLayout::Columnar([Vec::new(), Vec::new(), Vec::new()])
            }
            StartupLayout::Node => KanbanDocumentLayout::NodeLayout(NodeLayout::new()),
            StartupLayout::Queue => KanbanDocumentLayout::Queue(QueueState::new()),
            StartupLayout::Search => KanbanDocumentLayout::Search(SearchState::new()),
            StartupLayout::TreeOutline => KanbanDocumentLayout::TreeOutline(TreeOutline::new()),
        }
    }
}
#[derive(clap::Parser)]
struct KanbanArgs {
    filename: Option<String>,
    #[arg(short,long,value_enum,default_value_t=StartupLayout::Column)]
    default_view: StartupLayout,
}

fn main() {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([640.0, 240.0]),
        ..Default::default()
    };
    let args = KanbanArgs::parse();
    let app = KanbanRS::from_args(args);

    if let Err(x) = eframe::run_native("KanbanRS", options, Box::new(|_cc| Ok(Box::new(app)))) {
        println!("{}", x);
    }
}
impl eframe::App for KanbanRS {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.close_application {
            let mut confirmed = false;
            if self.modified_since_last_saved {
                ctx.show_viewport_immediate(
                    egui::ViewportId::from_hash_of("Save confirmation"),
                    egui::ViewportBuilder::default()
                        .with_inner_size(Vec2::new(300., 100.))
                        .with_window_type(egui::X11WindowType::Dialog)
                        .with_always_on_top(),
                    |ctx, _class| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            ui.label("You may lose information if you don't save, do you want to?");
                            if ui.button("Save").clicked() {
                                self.save_file(false);
                                confirmed = true;
                            }
                            if ui.button("Don't save").clicked() {
                                confirmed = true;
                            }
                            if ui.button("Cancel").clicked() {
                                self.close_application = false;
                            }
                        });
                    },
                );
            } else {
                confirmed = true;
            }
            if confirmed {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }
        if self.layout_cache_needs_updating {
            self.current_layout.update_cache(
                &self.document.read(),
                &self.sorting_type,
                ctx.style().as_ref(),
                &self.filter,
            );
            self.current_layout
                .sort_cache(&self.document.read(), &self.sorting_type);
            self.layout_cache_needs_updating = false;
        }
        ctx.input_mut(|i| {
            let save_shortcut = egui::KeyboardShortcut {
                modifiers: egui::Modifiers {
                    alt: false,
                    ctrl: true,
                    shift: false,
                    mac_cmd: false,
                    command: false,
                },
                logical_key: egui::Key::S,
            };
            let save_as_shortcut = egui::KeyboardShortcut {
                modifiers: egui::Modifiers {
                    alt: false,
                    ctrl: true,
                    shift: true,
                    mac_cmd: false,
                    command: false,
                },
                logical_key: egui::Key::S,
            };
            i.consume_shortcut(&save_as_shortcut).then(|| {
                self.save_file(true);
            });
            i.consume_shortcut(&save_shortcut).then(|| {
                self.save_file(false);
            });
            let find_shortcut = egui::KeyboardShortcut {
                modifiers: egui::Modifiers {
                    alt: false,
                    ctrl: true,
                    shift: false,
                    mac_cmd: false,
                    command: false,
                },
                logical_key: egui::Key::F,
            };
            i.consume_shortcut(&find_shortcut).then(|| {
                self.current_layout = KanbanDocumentLayout::Search(SearchState::new());
                self.layout_cache_needs_updating = true;
                println!("FINDING");
            })
        });
        self.hovered_task = None;
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Save").clicked() {
                        // Save to already existing file, as most applications tend to do.
                        self.save_file(false);
                        ui.close_menu();
                    }
                    if ui.button("Save As").clicked() {
                        self.save_file(true);
                        ui.close_menu();
                    }
                    if ui.button("Open").clicked() {
                        let filename = rfd::FileDialog::new()
                            .add_filter("Kanban", &["kan"])
                            .pick_file();
                        if let Some(filename) = filename {
                            self.open_file(&filename);
                        }
                        self.current_layout.update_cache(
                            &self.document.read(),
                            &self.sorting_type,
                            ui.style(),
                            &self.filter,
                        );
                        ui.close_menu();
                    }
                    ui.menu_button("Recently Used", |ui| {
                        for i in self.read_recents() {
                            let s: String = String::from(i.to_str().unwrap());
                            if fs::exists(&s).is_ok_and(|x| x) && ui.button(&s).clicked() {
                                self.open_file(&i);
                                ui.close_menu();
                                self.layout_cache_needs_updating = true;
                            }
                        }
                    });
                    if ui.button("Export to graphviz").clicked() {
                        self.write_dot();
                    }
                    if ui.button("Quit").clicked() {
                        self.close_application = true;
                    }
                });
                ui.menu_button("Edit", |ui| {
                    ui.add_enabled_ui(!self.undo_buffer.is_empty(), |ui| {
                        if ui.button("Undo").clicked() {
                            self.undo();
                            self.layout_cache_needs_updating = true;
                        }
                    });
                    if ui.button("Category style editor").clicked() {
                        self.category_editor.open = true;
                        ui.close_menu();
                    }
                    if ui.button("Priority editor").clicked() {
                        self.priority_editor.open = true;
                        ui.close_menu();
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Layout"));
                ComboBox::from_id_salt("Layout")
                    .selected_text(String::from(&self.current_layout))
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_value(
                                &mut self.current_layout,
                                KanbanDocumentLayout::default(),
                                "Columnar",
                            )
                            .clicked()
                        {
                            self.layout_cache_needs_updating = true;
                        }
                        if ui
                            .selectable_value(
                                &mut self.current_layout,
                                KanbanDocumentLayout::Queue(QueueState::new()),
                                "Queue",
                            )
                            .clicked()
                        {
                            self.layout_cache_needs_updating = true;
                        }
                        if ui
                            .selectable_value(
                                &mut self.current_layout,
                                KanbanDocumentLayout::Search(SearchState::new()),
                                "Search",
                            )
                            .clicked()
                        {
                            self.layout_cache_needs_updating = true;
                        }
                        if ui
                            .selectable_value(
                                &mut self.current_layout,
                                KanbanDocumentLayout::TreeOutline(TreeOutline::new()),
                                "Tree Outline",
                            )
                            .clicked()
                        {
                            self.layout_cache_needs_updating = true;
                        }
                        ui.selectable_value(
                            &mut self.current_layout,
                            KanbanDocumentLayout::NodeLayout(NodeLayout::new()),
                            "Node",
                        )
                        .clicked()
                        .then(|| {
                            self.layout_cache_needs_updating = true;
                        })
                    });
                if let KanbanDocumentLayout::Search(_) = self.current_layout {
                } else {
                    self.layout_cache_needs_updating |= self.sorting_type.combobox(ui);
                }
                if self.filter.show_ui(ui, &self.document.read()).changed() {
                    self.layout_cache_needs_updating |= true;
                }
            });
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.task_name);
                if ui.button("Add Task").clicked() {
                    let mut document = self.document.write();
                    let thing = document.get_new_task_mut();
                    thing.name = self.task_name.clone();
                    self.undo_buffer
                        .push_back(kanban::undo::UndoItem::Create(CreationEvent {
                            new_task: thing.clone(),
                            parent_id: None,
                        }));
                    self.layout_cache_needs_updating = true;
                    self.modified_since_last_saved = true;
                    self.current_layout.inform_of_new_items();
                }
            });

            ui.end_row();
            if let KanbanDocumentLayout::Columnar(_) = self.current_layout {
                self.layout_columnar(ui);
            } else if let KanbanDocumentLayout::Search(_) = self.current_layout {
                self.layout_search(ui);
            } else if let KanbanDocumentLayout::Focused(_) = self.current_layout {
                self.layout_focused(ui);
            } else if let KanbanDocumentLayout::TreeOutline(tr) = &mut self.current_layout {
                tr.show(
                    ui,
                    &self.document.read(),
                    &mut self.summary_actions_pending,
                    &mut self.hovered_task,
                )
            } else if let KanbanDocumentLayout::NodeLayout(nl) = &mut self.current_layout {
                self.layout_cache_needs_updating |=
                    nl.show(&self.document.read(), ui, &mut self.summary_actions_pending);
            } else {
                self.layout_queue(ui);
            }
            let mut undo_items: Vec<kanban::undo::UndoItem> = Vec::new();
            self.open_editors
                .iter()
                .filter(|editor| !editor.read().open)
                .for_each(|editor| {
                    if !editor.read().cancelled {
                        let undo = self.document.write().replace_task(&editor.read().item_copy);
                        undo_items.push(undo);
                        self.layout_cache_needs_updating = true;
                        self.modified_since_last_saved = true;
                    }
                });
            undo_items.drain(..).for_each(|x| self.record_undo(x));
            self.open_editors.retain(|editor| editor.read().open);
            for editor in self.open_editors.iter_mut() {
                let viewport_id = ui.ctx().viewport_id();
                let document = self.document.clone();
                let tx = self.editor_tx.clone();
                let editor = editor.clone();
                let id = editor.read().item_copy.id;
                let window_title = format!("Editing '{}'", editor.read().item_copy.name);
                ui.ctx().show_viewport_deferred(
                    egui::ViewportId::from_hash_of(id),
                    egui::ViewportBuilder::default()
                        .with_window_type(egui::X11WindowType::Dialog)
                        .with_title(&window_title),
                    move |ctx, _class| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let request = kanban::editor::editor(
                                ui,
                                &document.read(),
                                editor.write().borrow_mut(),
                            );
                            if !matches!(request, EditorRequest::NoRequest) {
                                println! {"{:?}",request}
                                tx.send(request).unwrap();
                                println!("Sent?");
                                // If we don't do this then it won't open a new editor when the
                                // add child button is clicked
                                ctx.request_repaint_of(viewport_id);
                            }
                        });
                        if ctx.input(|i| i.viewport().close_requested()) {
                            editor.write().open = false;
                        }
                    },
                );
            }

            // I would prefer this in an iterator or a for loop, but, I am simply not brain enough tonight
            while let Some(x) = self.summary_actions_pending.pop() {
                self.handle_summary_action(&x);
            }

            if self.category_editor.open {
                ui.ctx().show_viewport_immediate(
                    egui::ViewportId::from_hash_of("Category Editor"),
                    egui::ViewportBuilder::default(),
                    |ctx, _class| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let action = self.category_editor.show(ui, &self.document.read());
                            match action {
                                kanban::category_editor::EditorAction::CreateCategory(
                                    name,
                                    style,
                                ) => {
                                    self.document.write().replace_category_style(&name, style);
                                    self.modified_since_last_saved = true;
                                }
                                kanban::category_editor::EditorAction::ApplyStyle(name, style) => {
                                    self.document.write().replace_category_style(&name, style);
                                    self.modified_since_last_saved = true;
                                }
                                kanban::category_editor::EditorAction::Nothing => (),
                            }
                        });
                        if ctx.input(|i| i.viewport().close_requested()) {
                            self.category_editor.open = false;
                        }
                    },
                );
            }
            while let Ok(mut x) = self.editor_rx.try_recv() {
                println!("Received");
                self.handle_editor_request(&mut x);
            }
            if self.priority_editor.open {
                ui.ctx().show_viewport_immediate(
                    egui::ViewportId::from_hash_of("Category Editor"),
                    egui::ViewportBuilder::default(),
                    |ctx, _class| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            self.layout_cache_needs_updating |=
                                self.priority_editor.show(&mut self.document.write(), ui);
                        });
                        if ctx.input(|i| i.viewport().close_requested()) {
                            self.priority_editor.open = false;
                        }
                    },
                );
            }
        });
    }
}

impl KanbanRS {
    fn from_args(args: KanbanArgs) -> Self {
        let mut result = KanbanRS::new();
        if let Some(filename) = args.filename {
            result.open_file(&PathBuf::from(filename));
        }
        result.current_layout = args.default_view.into();
        result
    }
    fn handle_summary_action(&mut self, action: &SummaryAction) {
        match action {
            SummaryAction::NoAction => (),
            SummaryAction::OpenEditor(id) => {
                let mut editor =
                    kanban::editor::state_from(self.document.read().get_task(*id).unwrap());
                editor.open = true;
                self.open_editors.push(Arc::new(RwLock::new(editor)));
            }
            SummaryAction::CreateChildOf(id) => {
                let (child_creation, new_task, mut task_copy) = {
                    let mut document = self.document.write();
                    let mut new_task = document.get_new_task();
                    let task_copy = document.get_task(*id).unwrap().clone();
                    new_task.inherit(&task_copy, &document);
                    (document.replace_task(&new_task), new_task, task_copy)
                };

                task_copy.add_child(&new_task);
                let editor = kanban::editor::state_from(&new_task);
                self.undo_buffer
                    .push_back(self.document.write().replace_task(&task_copy));
                self.record_undo(child_creation);
                self.open_editors.push(Arc::new(RwLock::new(editor)));

                self.layout_cache_needs_updating = true;
                self.modified_since_last_saved = true;
                self.current_layout.inform_of_new_items();
            }
            SummaryAction::MarkCompleted(id) => {
                let (new, mut task) = {
                    let document = self.document.read();
                    let task = document.get_task(*id).unwrap().clone();
                    (
                        match task.completed {
                            Some(_) => None,
                            None => Some(Utc::now()),
                        },
                        task,
                    )
                };
                task.completed = new;
                let undo = self.document.write().replace_task(&task);
                self.record_undo(undo);
                self.layout_cache_needs_updating = true;
            }
            SummaryAction::FocusOn(id) => {
                if let KanbanDocumentLayout::TreeOutline(t_o) = &mut self.current_layout {
                    t_o.set_focus(*id);
                } else if let KanbanDocumentLayout::NodeLayout(nl) = &mut self.current_layout {
                    nl.set_focus(id);
                    //This shouldn't trigger a switch to the focused view
                } else {
                    self.current_layout =
                        KanbanDocumentLayout::Focused(kanban::focused_layout::Focus::new(*id));
                }
                self.layout_cache_needs_updating = true;
            }
            SummaryAction::AddChildTo(parent, child) => {
                let undoitem = {
                    let mut document = self.document.write();
                    if document.can_add_as_child(
                        document.get_task(*parent).unwrap(),
                        document.get_task(*child).unwrap(),
                    ) {
                        let mut task = document.get_task(*parent).unwrap().clone();
                        task.child_tasks.insert(*child);
                        Some(document.replace_task(&task))
                    } else {
                        None
                    }
                };
                if let Some(item) = undoitem {
                    self.record_undo(item);
                }
                self.layout_cache_needs_updating = true;
                self.modified_since_last_saved = true;
            }
            SummaryAction::UpdateLayout => {
                self.layout_cache_needs_updating = true;
            }
        }
    }
    fn handle_editor_request(&mut self, request: &mut EditorRequest) {
        match request {
            kanban::editor::EditorRequest::NewItem(parent, new_task) => {
                self.record_undo({
                    let mut document = self.document.write();
                    new_task.inherit(parent, &document);
                    document.replace_task(new_task)
                });
                self.open_editors
                    .push(Arc::new(RwLock::new(kanban::editor::state_from(new_task))));

                self.layout_cache_needs_updating = true;
                self.modified_since_last_saved = true;
                self.current_layout.inform_of_new_items();
            }
            // The main distinction between the two is that opening an
            // existing task shouldn't change the state of the item in the
            // document.
            kanban::editor::EditorRequest::OpenItem(item_to_open) => {
                self.open_editors
                    .push(Arc::new(RwLock::new(kanban::editor::state_from(
                        item_to_open,
                    ))));
            }
            kanban::editor::EditorRequest::DeleteItem(to_delete) => {
                let undo = self.document.write().remove_task(to_delete);
                self.record_undo(undo);
                for editor in self.open_editors.iter() {
                    editor.write().item_copy.remove_child(to_delete);
                }
                self.layout_cache_needs_updating = true;
                self.modified_since_last_saved = true;
                self.current_layout.inform_of_new_items();
            }
            kanban::editor::EditorRequest::UpdateItem(item) => {
                let undo = self.document.write().replace_task(item);
                self.record_undo(undo);
                self.modified_since_last_saved = true;
                self.layout_cache_needs_updating = true;
            }
            _ => {}
        }
    }
}

impl KanbanRS {
    #[inline]
    fn record_undo(&mut self, item: kanban::undo::UndoItem) {
        if let Some(i) = self.undo_buffer.back_mut() {
            if let Some(combined) = i.merge(&item) {
                *i = combined;
            } else {
                self.undo_buffer.push_back(item);
            }
        } else {
            self.undo_buffer.push_back(item);
        }
    }
    fn get_recents_file(&self) -> Option<PathBuf> {
        #[cfg(unix)]
        return self.base_dirs.find_state_file("recent");
        #[cfg(windows)]
        if fs::exists("~/Application Data/Roaming/kanbanrs/recents").unwrap() {
            Some(PathBuf::from("~/Application Data/Roaming/kanbanrs/recents"))
        } else {
            None
        }
    }

    fn place_recents_file(&self) -> Result<PathBuf, std::io::Error> {
        #[cfg(unix)]
        return self.base_dirs.place_state_file("recent");
        #[cfg(windows)]
        {
            if !fs::exists("~/Application Data/Roaming/kanbanrs/").unwrap() {
                fs::create_dir("~/Application Data/Roaming/kanbanrs").unwrap();
            }
            if !fs::exists("~/Application Data/Roaming/kanbanrs/recent").unwrap() {
                fs::File::create("~/Application Data/Roaming/kanbanrs/recent")?;
            }
            Ok("~/Application Data/Roaming/kanbanrs/recent".into())
        }
    }
    pub fn read_recents(&self) -> Vec<PathBuf> {
        let recents_file = self.get_recents_file();
        if recents_file.is_none() {
            return Vec::new();
        }
        let recents_file = recents_file.unwrap();
        std::fs::read_to_string(recents_file)
            .unwrap_or("".to_string())
            .split("\n")
            .filter(|x| !x.is_empty())
            .map(|x| x.into())
            .collect()
    }
    pub fn write_recents(&self) {
        let recents_file = self
            .place_recents_file()
            .expect("Could not create recents file");
        if !std::fs::exists(&recents_file).unwrap() {
            if let Err(x) = std::fs::File::create(&recents_file) {
                println!("Failed to open file with error '{}'", x);
            }
        }
        let mut old_recents: Vec<String> = std::fs::read_to_string(&recents_file)
            .unwrap()
            .split('\n')
            .filter(|x| x.len() > 1)
            .map(String::from)
            .collect();
        let pb: String = String::from(self.save_file_name.as_ref().unwrap().to_str().unwrap());
        // If the file is already in recents then we should avoid adding it.
        if old_recents.contains(&pb) {
            return;
        }
        if old_recents.len() > 10 {
            old_recents.rotate_right(1);
            old_recents[0] = pb;
        } else {
            old_recents.push(pb);
            old_recents.rotate_right(1);
        }
        if let Err(x) = std::fs::write(recents_file, old_recents.join("\n")) {
            println!("{}", x);
            std::process::abort();
        }
    }
    fn open_file(&mut self, path: &PathBuf) {
        let file = fs::File::open(path).unwrap();
        *self.document.write() = serde_json::from_reader(file).unwrap();
        self.open_editors.clear();
        self.save_file_name = Some(path.into());
    }
    fn write_dot(&self) {
        let filename = rfd::FileDialog::new()
            .add_filter("Graphviz", &["dot"])
            .save_file();
        if filename.is_none() {
            return;
        }
        let file = fs::File::create(filename.as_ref().unwrap());
        if let Ok(mut file) = file {
            writeln!(&mut file, "Digraph G{{").unwrap();
            for i in self.document.read().get_tasks() {
                writeln!(
                    &mut file,
                    " {} [label=\"{}\"];",
                    i.id,
                    i.name.clone().replace("\"", "\\\"")
                )
                .unwrap();
                write!(&mut file, "{} -> {{ ", i.id).unwrap();
                let mut needs_comma = false;
                for id in i.child_tasks.iter() {
                    if needs_comma {
                        write!(&mut file, ",").unwrap();
                    }
                    write!(&mut file, "{}", id).unwrap();
                    needs_comma = true;
                }
                writeln!(&mut file, "}};").unwrap();
            }
            writeln!(&mut file, "}}").unwrap();
        }
    }
    pub fn save_file(&mut self, force_choose_file: bool) {
        if self.save_file_name.is_none() || force_choose_file {
            let filename = rfd::FileDialog::new()
                .add_filter("Kanban", &["kan"])
                .save_file();
            if filename.is_none() {
                return;
            }
            self.save_file_name = filename;
        }
        // I lost some work on this due to a deadlock caused by locking the document.next_id
        // field while trying to write to it, instead of the source object.
        //
        // This should prevent that
        let mut tmp_path = self.save_file_name.clone().unwrap();
        tmp_path.set_extension("kan.bak");
        let file = fs::File::create(&tmp_path);
        if let Err(x) =
            serde_json::to_writer(file.unwrap(), &self.document.try_read().unwrap().clone())
        {
            println!("Error on saving: {}", x);
        }
        if let Err(x) = fs::rename(&tmp_path, self.save_file_name.as_ref().unwrap()) {
            println!("Error! {}", x);
        }
        self.modified_since_last_saved = false;
        self.write_recents();
    }

    fn undo(&mut self) {
        if let Some(item) = self.undo_buffer.pop_back() {
            item.undo(&mut self.document.write());
        }
    }
}
