mod kanban;
use chrono::Utc;
use circular_buffer::CircularBuffer;
#[cfg(not(target_arch = "wasm32"))]
use clap::*;
use eframe::egui::{self, ComboBox, Modifiers, Rect, RichText, ViewportCommand};
use kanban::{
    category_editor::State, filter::KanbanFilter, priority_editor::PriorityEditor,
    queue_view::QueueState, search::SearchState, sorting::ItemSort,
    tree_outline_layout::TreeOutline, undo::CreationEvent, AppCommand, KanbanDocument, KanbanId,
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::{mpsc, Arc};
#[cfg(not(target_arch = "wasm32"))]
use std::{
    fs,
    io::Write,
    path::PathBuf,
    thread::{self, JoinHandle},
};
mod document_layout;
mod preferences;
mod web_storage;
use document_layout::*;
use log::{debug, error};
#[cfg(target_os = "linux")]
mod desktop_file_creator;
#[cfg(feature = "fast_allocator")]
use mimalloc::MiMalloc;

#[cfg(feature = "fast_allocator")]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[cfg(target_arch = "wasm32")]
struct WebState {
    pending_import: Arc<std::sync::Mutex<Option<Result<Vec<u8>, String>>>>,
    /// The localStorage name of the currently-open document, if it has been saved.
    document_name: Option<String>,
    show_file_browser: bool,
    show_save_as_dialog: bool,
    save_as_input: String,
}

#[cfg(target_arch = "wasm32")]
impl Default for WebState {
    fn default() -> Self {
        WebState {
            pending_import: Arc::new(std::sync::Mutex::new(None)),
            document_name: None,
            show_file_browser: false,
            show_save_as_dialog: false,
            save_as_input: String::new(),
        }
    }
}

struct KanbanRS {
    document: Arc<RwLock<KanbanDocument>>,
    task_name: String,
    open_editors: Vec<Arc<RwLock<kanban::editor::State>>>,
    #[cfg(not(target_arch = "wasm32"))]
    save_file_name: Option<PathBuf>,
    current_layout: KanbanDocumentLayout,
    #[cfg(unix)]
    base_dirs: xdg::BaseDirectories,
    hovered_task: Option<i32>,
    close_requested: bool,
    close_confirmed: bool,
    asking_for_new_file: bool,
    layout_cache_needs_updating: bool,
    pending_commands: Vec<AppCommand>,
    sorting_type: kanban::sorting::ItemSort,
    category_editor: kanban::category_editor::State,
    priority_editor: PriorityEditor,
    modified_since_last_saved: bool,
    editor_rx: std::sync::mpsc::Receiver<AppCommand>,
    editor_tx: std::sync::mpsc::Sender<AppCommand>,
    undo_buffer: CircularBuffer<35, kanban::undo::UndoItem>,
    filter: kanban::filter::KanbanFilter,
    last_rect: Option<Rect>,
    messages: Vec<String>,
    #[cfg(not(target_arch = "wasm32"))]
    save_thread: Option<JoinHandle<Result<(), String>>>,
    preferences: Arc<RwLock<preferences::Preferences>>,
    #[cfg(target_arch = "wasm32")]
    web: WebState,
}
impl KanbanRS {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        KanbanRS {
            document: Arc::new(RwLock::new(KanbanDocument::default())),
            task_name: String::new(),
            open_editors: Vec::new(),
            #[cfg(not(target_arch = "wasm32"))]
            save_file_name: None,
            current_layout: KanbanDocumentLayout::default(),
            #[cfg(unix)]
            base_dirs: xdg::BaseDirectories::with_prefix("kanbanrs").unwrap(),
            hovered_task: None,
            close_requested: false,
            close_confirmed: false,
            layout_cache_needs_updating: true,
            pending_commands: Vec::new(),
            sorting_type: kanban::sorting::ItemSort::None,
            category_editor: State::new(),
            priority_editor: PriorityEditor::new(),
            modified_since_last_saved: false,
            editor_rx: rx,
            editor_tx: tx,
            undo_buffer: CircularBuffer::new(),
            filter: KanbanFilter::None,
            last_rect: None,
            messages: Vec::new(),
            #[cfg(not(target_arch = "wasm32"))]
            save_thread: None,
            asking_for_new_file: false,
            preferences: preferences::PREFERENCES.clone(),
            #[cfg(target_arch = "wasm32")]
            web: WebState::default(),
        }
    }
}
#[cfg_attr(not(target_arch = "wasm32"), derive(clap::Parser, ValueEnum))]
#[derive(PartialEq, Eq, Clone, Copy, Debug, Deserialize, Serialize, Default)]
/// The startup layout is used entirely in preferences and argument
/// parsing to represent an empty layout.
enum StartupLayout {
    /// Converts into the NodeLayout
    Node,
    ///Converted into the columnar layout
    #[default]
    Column,
    ///Converted into the TreeLayout
    TreeOutline,
    ///Converted into the QueueLayout
    Queue,
    ///Converted into the SearchLayout
    Search,
    ///Placeholder specifying that the layout should be initialized
    ///from the stored preferences
    NotSelected,
}
impl std::fmt::Display for StartupLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl From<StartupLayout> for KanbanDocumentLayout {
    fn from(value: StartupLayout) -> Self {
        let layout_type = match value {
            StartupLayout::Column => {
                KanbanDocumentLayoutType::Columnar([Vec::new(), Vec::new(), Vec::new()])
            }
            StartupLayout::Node => KanbanDocumentLayoutType::NodeLayout(Box::default()),
            StartupLayout::Queue => KanbanDocumentLayoutType::Queue(QueueState::new()),
            StartupLayout::Search => KanbanDocumentLayoutType::Search(SearchState::new()),
            StartupLayout::TreeOutline => KanbanDocumentLayoutType::TreeOutline(TreeOutline::new()),
            StartupLayout::NotSelected => KanbanDocumentLayoutType::Unloaded,
        };
        Self {
            layout: layout_type,
            scroll_to: None,
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
#[derive(clap::Parser)]
struct KanbanArgs {
    filename: Option<String>,
    #[arg(short,long,value_enum,default_value_t=StartupLayout::NotSelected)]
    default_view: StartupLayout,
}
pub static ICON_DATA: &[u8] = include_bytes!("../assets/kanban icon.png");
static NOTO_SANS: &[u8] = include_bytes!("../assets/NotoSans-Regular.ttf");

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "NotoSans".to_owned(),
        egui::FontData::from_static(NOTO_SANS).into(),
    );
    // Add as last fallback for all font families so it fills in missing glyphs.
    for family in fonts.families.values_mut() {
        family.push("NotoSans".to_owned());
    }
    ctx.set_fonts(fonts);
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    // This is used to clean up the desktop file at the end.
    #[cfg(target_os = "linux")]
    let _thing = desktop_file_creator::create_dot_desktop_file();

    env_logger::init();
    let icon = eframe::icon_data::from_png_bytes(ICON_DATA).expect("Must be a valid icon");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 240.0])
            .with_icon(icon),

        ..Default::default()
    };
    let args = KanbanArgs::parse();
    let app = KanbanRS::from_args(args);
    if let Err(x) = eframe::run_native(
        "KanbanRS",
        options,
        Box::new(|cc| {
            install_fonts(&cc.egui_ctx);
            let mut app = Box::new(app);
            app.initialize_preferences(cc.storage.unwrap());
            cc.egui_ctx
                .set_visuals(if app.preferences.read().dark_mode {
                    egui::Visuals::dark()
                } else {
                    egui::Visuals::light()
                });
            Ok(app)
        }),
    ) {
        error!("{x}");
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();
    let web_options = eframe::WebOptions::default();
    wasm_bindgen_futures::spawn_local(async {
        let canvas = web_sys::window()
            .expect("no window")
            .document()
            .expect("no document")
            .get_element_by_id("kanbanrs_canvas")
            .expect("no element with id 'kanbanrs_canvas'")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("kanbanrs_canvas is not a HtmlCanvasElement");
        eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cc| {
                    install_fonts(&cc.egui_ctx);
                    let mut app = KanbanRS::new();
                    if let Some(storage) = cc.storage {
                        app.initialize_preferences(storage);
                    }
                    cc.egui_ctx
                        .set_visuals(if app.preferences.read().dark_mode {
                            egui::Visuals::dark()
                        } else {
                            egui::Visuals::light()
                        });
                    Ok(Box::new(app))
                }),
            )
            .await
            .expect("failed to start eframe");
    });
}

impl eframe::App for KanbanRS {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        #[cfg(not(target_arch = "wasm32"))]
        if self.save_thread.is_some() && self.save_thread.as_ref().unwrap().is_finished() {
            match self.save_thread.take().unwrap().join() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => self.messages.push(format!("Save failed: {e}")),
                Err(_) => self.messages.push("Save thread panicked".to_string()),
            }
            debug!("Joined save thread");
        }

        #[cfg(target_arch = "wasm32")]
        if let Some(result) = self.web.pending_import.lock().unwrap().take() {
            match result {
                Ok(bytes) => match serde_json::from_slice::<KanbanDocument>(&bytes) {
                    Ok(doc) => {
                        *self.document.write() = doc;
                        self.document.write().collect_tags();
                        self.open_editors.clear();
                        self.layout_cache_needs_updating = true;
                        self.modified_since_last_saved = false;
                        self.web.document_name = None;
                    }
                    Err(e) => self.messages.push(format!("Import failed: {e}")),
                },
                Err(e) => self.messages.push(format!("Import failed: {e}")),
            }
        }

        #[cfg(target_arch = "wasm32")]
        self.show_web_modals(ui);

        if self.layout_cache_needs_updating {
            self.current_layout.update_cache(
                &self.document.read(),
                &self.sorting_type,
                ui.style().as_ref(),
                &self.filter,
            );
            self.current_layout
                .sort_cache(&self.document.read(), &self.sorting_type);
            self.layout_cache_needs_updating = false;
        }
        ui.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                Modifiers::COMMAND,
                egui::Key::N,
            ))
            .then(|| self.asking_for_new_file = true);

            #[cfg(not(target_arch = "wasm32"))]
            {
                i.consume_shortcut(&egui::KeyboardShortcut::new(
                    Modifiers::COMMAND,
                    egui::Key::S,
                ))
                .then(|| self.save_file(false));
                i.consume_shortcut(&egui::KeyboardShortcut::new(
                    Modifiers {
                        shift: true,
                        ..Modifiers::COMMAND
                    },
                    egui::Key::S,
                ))
                .then(|| self.save_file(true));
            }

            #[cfg(target_arch = "wasm32")]
            {
                i.consume_shortcut(&egui::KeyboardShortcut::new(
                    Modifiers::COMMAND,
                    egui::Key::S,
                ))
                .then(|| self.web_save());
                i.consume_shortcut(&egui::KeyboardShortcut::new(
                    Modifiers {
                        shift: true,
                        ..Modifiers::COMMAND
                    },
                    egui::Key::S,
                ))
                .then(|| {
                    self.web.save_as_input = self.web.document_name.clone().unwrap_or_default();
                    self.web.show_save_as_dialog = true;
                });
            }

            i.consume_shortcut(&egui::KeyboardShortcut::new(Modifiers::CTRL, egui::Key::F))
                .then(|| {
                    self.current_layout.layout =
                        KanbanDocumentLayoutType::Search(SearchState::new());
                    self.layout_cache_needs_updating = true;
                });
        });
        if self.asking_for_new_file {
            let mut confirmed = false;
            if *self.document.read() != self.preferences.read().template {
                egui::Modal::new("new_file_confirmation".into()).show(ui.ctx(), |ui| {
                    ui.label("You may lose information if you don't save, do you want to?");
                    ui.horizontal(|ui| {
                        #[cfg(not(target_arch = "wasm32"))]
                        if ui.button("Save").clicked() {
                            self.save_file(false);
                            confirmed = true;
                            self.asking_for_new_file = false;
                        }
                        if ui.button("Don't save").clicked() {
                            confirmed = true;
                            self.asking_for_new_file = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.asking_for_new_file = false;
                        }
                    });
                });
                if confirmed {
                    self.new_file();
                }
            } else {
                self.new_file();
            }
        }
        self.hovered_task = None;
        egui::CentralPanel::default().show_inside(ui, |ui| {
            #[cfg(not(target_arch = "wasm32"))]
            {
                if ui
                    .ctx()
                    .input(|i| i.viewport().close_requested() && !self.close_confirmed)
                {
                    self.close_requested = true;
                    ui.send_viewport_cmd(ViewportCommand::CancelClose);
                }
                if self.close_requested {
                    let mut confirmed = false;
                    if self.modified_since_last_saved {
                        egui::Modal::new("save_before_closing".into()).show(ui.ctx(), |ui| {
                            ui.label("You may lose information if you don't save, do you want to?");
                            ui.horizontal(|ui| {
                                if ui.button("Save").clicked() {
                                    self.save_file(false);
                                    self.close_confirmed = true;
                                    confirmed = true;
                                }
                                if ui.button("Don't save").clicked() {
                                    self.close_confirmed = true;
                                    confirmed = true;
                                }
                                if ui.button("Cancel").clicked() {
                                    self.close_requested = false;
                                }
                            });
                        });
                    } else {
                        self.close_confirmed = true;
                        confirmed = true;
                    }
                    if confirmed {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        return;
                    }
                }
            }
            let current_rect = Rect {
                min: egui::Pos2 { x: 0., y: 0. },
                max: ui.available_size().to_pos2(),
            };
            if self.last_rect != Some(current_rect) {
                // println!("Clearing layout cache");
                kanban::layout_cache::clear_layout_cache();
                self.last_rect = Some(current_rect);
            }
            let ctx = ui.ctx().clone();
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New").clicked() {
                        self.asking_for_new_file = true;
                        ui.close();
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        if ui.button("Save").clicked() {
                            self.save_file(false);
                            ui.close();
                        }
                        if ui.button("Save As").clicked() {
                            self.save_file(true);
                            ui.close();
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
                            ui.close();
                        }
                        ui.menu_button("Recently Used", |ui| {
                            for i in self.read_recents() {
                                let s: String = String::from(i.to_str().unwrap());
                                if fs::exists(&s).is_ok_and(|x| x) && ui.button(&s).clicked() {
                                    self.open_file(&i);
                                    ui.close();
                                    self.layout_cache_needs_updating = true;
                                }
                            }
                        });
                        if ui.button("Export to graphviz").clicked() {
                            self.write_dot();
                        }
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        if ui.button("Save").clicked() {
                            self.web_save();
                            ui.close();
                        }
                        if ui.button("Save As").clicked() {
                            self.web.save_as_input =
                                self.web.document_name.clone().unwrap_or_default();
                            self.web.show_save_as_dialog = true;
                            ui.close();
                        }
                        if ui.button("Open").clicked() {
                            self.web.show_file_browser = true;
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Export to file").clicked() {
                            self.web_export_file();
                            ui.close();
                        }
                        if ui.button("Import from file").clicked() {
                            self.web_import_file();
                            ui.close();
                        }
                    }
                    if ui.button("Preferences").clicked() {
                        self.preferences.write().showing_preference = true;
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(ViewportCommand::Close);
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
                        ui.close();
                    }
                    if ui.button("Priority editor").clicked() {
                        self.priority_editor.open = true;
                        ui.close();
                    }
                    if ui
                        .button("Open recording tasks")
                        .on_hover_text("Open items with active time tracking")
                        .clicked()
                    {
                        let mut editor_opened = false;
                        for i in self
                            .document
                            .read()
                            .get_tasks()
                            .filter(|x| x.time_records.is_recording())
                        {
                            editor_opened = true;
                            let editor = kanban::editor::state_from(i, self.editor_tx.clone());
                            self.open_editors.push(Arc::new(RwLock::new(editor)));
                        }
                        if !editor_opened {
                            self.messages.push("No open tasks".into());
                        }
                        ui.close();
                    }
                });
                ui.menu_button("Window", |ui| {
                    ui.add_enabled_ui(!self.open_editors.is_empty(), |ui| {
                        ui.menu_button("Task editors", |ui| {
                            // Snapshot display info before UI callbacks to avoid holding a
                            // read lock while close handling needs a write lock.
                            let editor_info: Vec<(String, KanbanId)> = self
                                .open_editors
                                .iter()
                                .map(|e| {
                                    let r = e.read();
                                    (r.item_copy.name.clone(), r.item_copy.id)
                                })
                                .collect();
                            for (name, task_id) in editor_info {
                                ui.horizontal(|ui| {
                                    if ui.button("Close").clicked() {
                                        self.pending_commands
                                            .push(AppCommand::CloseEditor(task_id));
                                        ui.close();
                                    }
                                    ui.separator();
                                    ui.label(&name);
                                });
                            }
                        });
                    });
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
                                KanbanDocumentLayout {
                                    layout: KanbanDocumentLayoutType::Queue(QueueState::new()),
                                    scroll_to: None,
                                },
                                "Queue",
                            )
                            .clicked()
                        {
                            self.layout_cache_needs_updating = true;
                        }
                        if ui
                            .selectable_value(
                                &mut self.current_layout,
                                KanbanDocumentLayout {
                                    layout: KanbanDocumentLayoutType::Search(SearchState::new()),
                                    scroll_to: None,
                                },
                                "Search",
                            )
                            .clicked()
                        {
                            self.layout_cache_needs_updating = true;
                        }
                        if ui
                            .selectable_value(
                                &mut self.current_layout,
                                KanbanDocumentLayout {
                                    layout: KanbanDocumentLayoutType::TreeOutline(
                                        TreeOutline::new(),
                                    ),
                                    scroll_to: None,
                                },
                                "Tree Outline",
                            )
                            .clicked()
                        {
                            self.layout_cache_needs_updating = true;
                        }
                        ui.selectable_value(
                            &mut self.current_layout,
                            KanbanDocumentLayout {
                                layout: KanbanDocumentLayoutType::NodeLayout(Box::default()),
                                scroll_to: None,
                            },
                            "Node",
                        )
                        .clicked()
                        .then(|| {
                            self.layout_cache_needs_updating = true;
                        })
                    });
                if let KanbanDocumentLayoutType::Search(_) = self.current_layout.layout {
                } else {
                    self.layout_cache_needs_updating |= self.sorting_type.combobox(ui);
                }
                ui.separator();
                if self.filter.show_ui(ui, &self.document.read()).changed() {
                    self.layout_cache_needs_updating |= true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("New task name");
                let response = ui.text_edit_singleline(&mut self.task_name);
                let enter_pressed =
                    response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button("Add Task").clicked() || enter_pressed {
                    let new_task = {
                        let mut document = self.document.write();
                        let thing = document.get_new_task_mut();
                        thing.name = self.task_name.clone();
                        let new_task = thing.clone();
                        self.undo_buffer
                            .push_back(kanban::undo::UndoItem::Create(CreationEvent {
                                new_task: new_task.clone(),
                                parent_id: None,
                            }));
                        self.layout_cache_needs_updating = true;
                        self.modified_since_last_saved = true;
                        self.current_layout.inform_of_new_items();
                        new_task
                    };
                    self.task_name.clear();
                    let editor = kanban::editor::state_from(&new_task, self.editor_tx.clone());
                    self.open_editors.push(Arc::new(RwLock::new(editor)));
                }
            });

            ui.end_row();
            if let KanbanDocumentLayoutType::Columnar(_) = self.current_layout.layout {
                self.layout_columnar(ui);
            } else if let KanbanDocumentLayoutType::Search(_) = self.current_layout.layout {
                self.layout_search(ui);
            } else if let KanbanDocumentLayoutType::Focused(_) = self.current_layout.layout {
                self.layout_focused(ui);
            } else if let KanbanDocumentLayoutType::TreeOutline(tr) =
                &mut self.current_layout.layout
            {
                tr.show(
                    ui,
                    &self.document.read(),
                    &mut self.pending_commands,
                    &mut self.hovered_task,
                    &self.current_layout.scroll_to,
                )
            } else if let KanbanDocumentLayoutType::NodeLayout(nl) = &mut self.current_layout.layout
            {
                self.layout_cache_needs_updating |=
                    nl.show(&self.document.read(), ui, &mut self.pending_commands);
            } else {
                self.layout_queue(ui);
            }
            // Should be cleared after each layout update.
            self.current_layout.scroll_to = None;
            let closed_updates: Vec<kanban::KanbanItem> = self
                .open_editors
                .iter()
                .filter(|editor| !editor.read().open && !editor.read().cancelled)
                .map(|editor| editor.read().item_copy.clone())
                .collect();
            for item in closed_updates {
                self.handle_command(AppCommand::UpdateTask(item));
            }
            self.open_editors.retain(|editor| editor.read().open);
            let document = self.document.clone();
            for editor in &self.open_editors {
                let title = format!("Editing '{}'", editor.read().item_copy.name);
                let id = editor.read().item_copy.id;
                if let Some(inner) = egui::Window::new(&title)
                    .id(egui::Id::new(id))
                    .show(ui, |ui| editor.write().editor(ui, &document.read()))
                {
                    if inner.inner.unwrap_or(false) {
                        ui.ctx().request_repaint();
                    }
                }
            }
            self.messages.retain(|x| {
                let mut keep = true;
                egui::Window::new(x)
                    .id(egui::Id::new(x))
                    .collapsible(false)
                    .resizable(false)
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(x);
                            if ui.button("Close").clicked() {
                                keep = false;
                            }
                        });
                    });
                keep
            });

            let pending = std::mem::take(&mut self.pending_commands);
            for cmd in pending {
                self.handle_command(cmd);
            }

            if self.category_editor.open {
                let mut open = true;
                egui::Window::new("Category Editor")
                    .open(&mut open)
                    .show(ui, |ui| {
                        let cmd = self.category_editor.show(ui, &self.document.read());
                        if let Some(cmd) = cmd {
                            self.handle_command(cmd);
                        }
                    });
                if !open {
                    self.category_editor.open = false;
                }
            }
            while let Ok(cmd) = self.editor_rx.try_recv() {
                self.handle_command(cmd);
            }
            if self.priority_editor.open {
                let mut open = true;
                egui::Window::new("Priority Editor")
                    .open(&mut open)
                    .show(ui, |ui| {
                        let cmd = self.priority_editor.show(&self.document.read(), ui);
                        if let Some(cmd) = cmd {
                            self.handle_command(cmd);
                        }
                    });
                if !open {
                    self.priority_editor.open = false;
                }
            }
            {
                let preferences = &mut self.preferences.write();
                if preferences.showing_preference {
                    let mut open = true;
                    egui::Window::new("Preferences")
                        .open(&mut open)
                        .show(ui, |ui| {
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                let recents: Vec<String> = self
                                    .read_recents()
                                    .iter()
                                    .map(|p| p.to_string_lossy().to_string())
                                    .collect();
                                preferences.show_ui(ui, &recents);
                            }
                            #[cfg(target_arch = "wasm32")]
                            {
                                preferences.show_ui(ui);
                            }
                        });
                    if !open {
                        preferences.showing_preference = false;
                    }
                }
            }
        });
    }
    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        log::debug!("Save function called");
        if let Ok(prefs) = serde_json::to_string(&*self.preferences.read()) {
            _storage.set_string("preferences", prefs);
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            self.write_recovery();
            if self.preferences.read().autosave.is_some() && self.save_file_name.is_some() {
                log::info!("Saving file");
                self.save_file(false);
            }
        }

        #[cfg(target_arch = "wasm32")]
        if let Ok(json) = serde_json::to_string(&*self.document.read()) {
            if let Some(name) = &self.web.document_name.clone() {
                web_storage::save_document(name, &json);
                _storage.set_string("web_document_name", name.clone());
            } else {
                _storage.set_string("document", json);
            }
            self.modified_since_last_saved = false;
        }
    }
    fn auto_save_interval(&self) -> std::time::Duration {
        let default_interval = std::time::Duration::from_secs(6);
        self.preferences.read().autosave.unwrap_or(default_interval)
    }
}

impl KanbanRS {
    fn initialize_preferences(&mut self, storage: &dyn eframe::Storage) {
        let str = storage.get_string("preferences");
        let str = str.unwrap_or(
            "{'autosave':{'secs':60,'nanos':0},'store_undo_history_for_files':false}".to_string(),
        );
        if let Ok(x) = serde_json::from_str(&str) {
            self.preferences.write().clone_from(&x);
        }
        if matches!(
            self.current_layout.layout,
            KanbanDocumentLayoutType::Unloaded
        ) {
            self.current_layout = self.preferences.read().startup_layout.into();
            self.layout_cache_needs_updating = true;
        }
        #[cfg(target_arch = "wasm32")]
        {
            let restored_name = storage.get_string("web_document_name");
            let doc_json = restored_name
                .as_deref()
                .and_then(web_storage::load_document)
                .or_else(|| storage.get_string("document"));
            if let Some(json) = doc_json {
                if let Ok(doc) = serde_json::from_str::<KanbanDocument>(&json) {
                    *self.document.write() = doc;
                    self.document.write().collect_tags();
                    self.web.document_name = restored_name;
                    return;
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        let auto_open = self.preferences.read().auto_open_file.clone();
        #[cfg(not(target_arch = "wasm32"))]
        if self.document.read().is_empty() {
            if let Some(path) = auto_open {
                let p = PathBuf::from(path);
                if p.exists() {
                    self.open_file(&p);
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.document.read().is_empty() {
            let recovery_path = self
                .base_dirs
                .find_cache_file("recovery.kan")
                .unwrap_or_else(|| {
                    std::env::var_os("LOCALAPPDATA")
                        .map(PathBuf::from)
                        .map(|p| p.join("kanbanrs").join("recovery.kan"))
                        .unwrap_or_default()
                });
            if recovery_path.exists() {
                self.messages
                    .push("Restored from recovery file — save to a named file to keep".to_string());
                self.open_file(&recovery_path);
            }
        }
        if self.document.read().is_empty() {
            self.new_file();
        }
    }
    fn new_file(&mut self) {
        self.document
            .write()
            .clone_from(&self.preferences.read().template);
        self.current_layout = self.preferences.read().startup_layout.into();
        self.asking_for_new_file = false;
        #[cfg(target_arch = "wasm32")]
        {
            self.web.document_name = None;
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn from_args(args: KanbanArgs) -> Self {
        let mut result = KanbanRS::new();
        if let Some(filename) = args.filename {
            result.open_file(&PathBuf::from(filename));
        }
        result.current_layout = args.default_view.into();

        result
    }
    fn handle_command(&mut self, cmd: AppCommand) {
        match cmd {
            AppCommand::OpenEditor(id) => {
                let mut editor = kanban::editor::state_from(
                    self.document.read().get_task(id).unwrap(),
                    self.editor_tx.clone(),
                );
                editor.open = true;
                self.open_editors.push(Arc::new(RwLock::new(editor)));
            }
            AppCommand::CloseEditor(id) => {
                for e in &self.open_editors {
                    let mut w = e.write();
                    if w.item_copy.id == id {
                        w.open = false;
                        w.cancelled = true;
                        break;
                    }
                }
            }
            AppCommand::OpenTask(item) => {
                self.open_editors
                    .push(Arc::new(RwLock::new(kanban::editor::state_from(
                        &item,
                        self.editor_tx.clone(),
                    ))));
            }
            AppCommand::CreateChildOf(id) => {
                let (child_creation, new_task, mut task_copy) = {
                    let mut document = self.document.write();
                    let mut new_task = document.get_new_task();
                    let task_copy = document.get_task(id).unwrap().clone();
                    new_task.inherit(&task_copy, &document);
                    (document.replace_task(&new_task), new_task, task_copy)
                };
                task_copy.add_child(&new_task);
                let editor = kanban::editor::state_from(&new_task, self.editor_tx.clone());
                self.undo_buffer
                    .push_back(self.document.write().replace_task(&task_copy));
                self.record_undo(child_creation);
                self.open_editors.push(Arc::new(RwLock::new(editor)));
                self.layout_cache_needs_updating = true;
                self.modified_since_last_saved = true;
                self.current_layout.inform_of_new_items();
            }
            AppCommand::CreateTask(parent, mut new_task) => {
                self.record_undo({
                    let mut document = self.document.write();
                    new_task.inherit(&parent, &document);
                    document.replace_task(&new_task)
                });
                self.open_editors
                    .push(Arc::new(RwLock::new(kanban::editor::state_from(
                        &new_task,
                        self.editor_tx.clone(),
                    ))));
                self.layout_cache_needs_updating = true;
                self.modified_since_last_saved = true;
                self.current_layout.inform_of_new_items();
            }
            AppCommand::UpdateTask(item) => {
                let undo = {
                    let document = &mut self.document.write();
                    self.modified_since_last_saved = if let Some(x) = document.get_task(item.id) {
                        x != &item
                    } else {
                        true
                    };
                    document.replace_task(&item)
                };
                self.record_undo(undo);
                self.layout_cache_needs_updating = self.modified_since_last_saved;
            }
            AppCommand::DeleteTask(to_delete) => {
                let undo = self.document.write().remove_task(&to_delete);
                self.record_undo(undo);
                for editor in self.open_editors.iter() {
                    editor.write().item_copy.remove_child(&to_delete);
                }
                self.layout_cache_needs_updating = true;
                self.modified_since_last_saved = true;
                self.current_layout.inform_of_new_items();
            }
            AppCommand::MarkCompleted(id) => {
                let mut task = self.document.read().get_task(id).unwrap().clone();
                task.completed = match task.completed {
                    Some(_) => None,
                    None => Some(Utc::now()),
                };
                let undo = self.document.write().replace_task(&task);
                self.record_undo(undo);
                self.layout_cache_needs_updating = true;
                self.modified_since_last_saved = true;
            }
            AppCommand::AddChildTo(parent, child) => {
                let undoitem = {
                    let mut document = self.document.write();
                    if document.can_add_as_child(
                        document.get_task(parent).unwrap(),
                        document.get_task(child).unwrap(),
                    ) {
                        let mut task = document.get_task(parent).unwrap().clone();
                        task.child_tasks.insert(child);
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
            AppCommand::FinishTimeRecording(id) => {
                let mut doc = self.document.write();
                let task = doc.get_task_mut(id).unwrap();
                task.time_records.handle_record_request(None);
                self.modified_since_last_saved = true;
            }
            AppCommand::ReplaceCategory(name, style) => {
                let former_style = self.document.read().get_category_style(&name);
                self.document.write().replace_category_style(&name, style);
                self.record_undo(kanban::undo::UndoItem::CategoryStyle(
                    kanban::undo::CategoryStyleEvent { name, former_style },
                ));
                self.modified_since_last_saved = true;
            }
            AppCommand::SetPriority(name, value) => {
                let former_value = self
                    .document
                    .read()
                    .get_sorted_priorities()
                    .into_iter()
                    .find(|(n, _)| *n == &name)
                    .map(|(_, v)| *v);
                self.document.write().set_priority(name.clone(), value);
                self.record_undo(kanban::undo::UndoItem::Priority(
                    kanban::undo::PriorityEvent { name, former_value },
                ));
                self.layout_cache_needs_updating = true;
                self.modified_since_last_saved = true;
            }
            AppCommand::FocusOn(id) => {
                if let KanbanDocumentLayoutType::TreeOutline(t_o) = &mut self.current_layout.layout
                {
                    t_o.set_focus(id);
                } else if let KanbanDocumentLayoutType::NodeLayout(nl) =
                    &mut self.current_layout.layout
                {
                    nl.set_focus(&id);
                } else {
                    self.current_layout.layout =
                        KanbanDocumentLayoutType::Focused(kanban::focused_layout::Focus::new(id));
                }
                self.layout_cache_needs_updating = true;
            }
            AppCommand::ScrollTo(id) => {
                self.current_layout.scroll_to = Some(id);
                if let KanbanDocumentLayoutType::NodeLayout(nl) = &mut self.current_layout.layout {
                    nl.scroll_to(id);
                }
            }
            AppCommand::UpdateLayout => {
                self.layout_cache_needs_updating = true;
            }
        }
    }
}

impl KanbanRS {
    /// Record an undo action.
    /// * **self** The kanban application state
    /// * **item** The undo item.
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
    #[cfg(not(target_arch = "wasm32"))]
    fn get_recents_file(&self) -> Option<PathBuf> {
        #[cfg(unix)]
        return self.base_dirs.find_state_file("recent");
        #[cfg(windows)]
        {
            let path = std::env::var_os("APPDATA")
                .map(PathBuf::from)?
                .join("kanbanrs")
                .join("recent");
            path.exists().then_some(path)
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn place_recents_file(&self) -> Result<PathBuf, std::io::Error> {
        #[cfg(unix)]
        return self.base_dirs.place_state_file("recent");
        #[cfg(windows)]
        {
            let dir = std::env::var_os("APPDATA")
                .map(PathBuf::from)
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no APPDATA"))?
                .join("kanbanrs");
            if !dir.exists() {
                fs::create_dir_all(&dir)?;
            }
            let path = dir.join("recent");
            if !path.exists() {
                fs::File::create(&path)?;
            }
            Ok(path)
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn read_recents(&self) -> Vec<PathBuf> {
        let Some(recents_file) = self.get_recents_file() else {
            return Vec::new();
        };
        std::fs::read_to_string(recents_file)
            .unwrap_or_default()
            .split('\n')
            .filter(|x| !x.is_empty())
            .map(|x| x.into())
            .collect()
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn write_recents(&self) {
        let Some(save_path) = self.save_file_name.as_ref() else {
            return;
        };
        let recents_file = match self.place_recents_file() {
            Ok(p) => p,
            Err(e) => {
                error!("Could not place recents file: {e}");
                return;
            }
        };
        let pb = save_path.to_string_lossy().to_string();
        let mut old_recents: Vec<String> = std::fs::read_to_string(&recents_file)
            .unwrap_or_default()
            .split('\n')
            .filter(|x| !x.is_empty())
            .map(String::from)
            .collect();
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
        if let Err(e) = std::fs::write(recents_file, old_recents.join("\n")) {
            error!("Failed to write recents: {e}");
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn open_file(&mut self, path: &PathBuf) {
        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(e) => {
                self.messages.push(format!("Open failed: {e}"));
                return;
            }
        };
        let doc: KanbanDocument = match serde_json::from_reader(file) {
            Ok(d) => d,
            Err(e) => {
                self.messages.push(format!("Open failed: {e}"));
                return;
            }
        };
        *self.document.write() = doc;
        self.document.write().collect_tags();
        self.open_editors.clear();
        self.save_file_name = Some(path.into());
    }

    fn write_recovery(&self) {
        let Ok(json) = serde_json::to_string(&*self.document.read()) else {
            return;
        };
        #[cfg(unix)]
        let recovery_path = self.base_dirs.place_cache_file("recovery.kan").ok();
        #[cfg(windows)]
        let recovery_path = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|p| p.join("kanbanrs").join("recovery.kan"));
        if let Some(path) = recovery_path {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&path, json);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
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
                    write!(&mut file, "{id}").unwrap();
                    needs_comma = true;
                }
                writeln!(&mut file, "}};").unwrap();
            }
            writeln!(&mut file, "}}").unwrap();
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
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
        let save_path = match &self.save_file_name {
            Some(p) => p.clone(),
            None => return,
        };
        let mut tmp_path = save_path.clone();
        tmp_path.set_extension("kan.bak");
        let file = match fs::File::create(&tmp_path) {
            Ok(f) => f,
            Err(e) => {
                self.messages.push(format!("Save failed: {e}"));
                return;
            }
        };
        let cloned = match self.document.try_read() {
            Some(doc) => doc.clone(),
            None => {
                self.messages
                    .push("Save failed: document locked".to_string());
                return;
            }
        };
        self.save_thread = Some(thread::spawn(move || {
            serde_json::to_writer(file, &cloned).map_err(|e| e.to_string())?;
            fs::rename(&tmp_path, save_path).map_err(|e| e.to_string())?;
            Ok(())
        }));
        self.modified_since_last_saved = false;
        self.write_recents();
        self.write_recovery();
    }

    fn undo(&mut self) {
        if let Some(item) = self.undo_buffer.pop_back() {
            item.undo(&mut self.document.write());
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn show_doc_tree(nodes: &[web_storage::DocTreeNode], ui: &mut egui::Ui) -> Option<String> {
    let mut selected = None;
    for node in nodes {
        if node.is_file {
            if ui.selectable_label(false, &node.name).clicked() {
                selected = Some(node.full_path.clone());
            }
        } else {
            egui::CollapsingHeader::new(&node.name)
                .default_open(false)
                .id_salt(&node.full_path)
                .show(ui, |ui| {
                    if let Some(s) = show_doc_tree(&node.children, ui) {
                        selected = Some(s);
                    }
                });
        }
    }
    selected
}

#[cfg(target_arch = "wasm32")]
impl KanbanRS {
    fn web_save(&mut self) {
        if let Some(name) = self.web.document_name.clone() {
            self.web_save_to_name(&name);
        } else {
            self.web.save_as_input.clear();
            self.web.show_save_as_dialog = true;
        }
    }

    fn web_save_to_name(&mut self, name: &str) {
        if let Ok(json) = serde_json::to_string(&*self.document.read()) {
            web_storage::save_document(name, &json);
            self.web.document_name = Some(name.to_string());
            self.modified_since_last_saved = false;
        }
    }

    fn web_open_document(&mut self, name: &str) {
        match web_storage::load_document(name)
            .and_then(|json| serde_json::from_str::<KanbanDocument>(&json).ok())
        {
            Some(doc) => {
                *self.document.write() = doc;
                self.document.write().collect_tags();
                self.web.document_name = Some(name.to_string());
                self.open_editors.clear();
                self.layout_cache_needs_updating = true;
                self.modified_since_last_saved = false;
            }
            None => self.messages.push(format!("Failed to load '{name}'")),
        }
    }

    /// Renders the file browser and save-as modals. Must be called every frame.
    fn show_web_modals(&mut self, ui: &mut egui::Ui) {
        if self.web.show_file_browser {
            let docs = web_storage::list_documents();
            let tree = web_storage::build_tree(&docs);
            let mut selected: Option<String> = None;
            let mut close = false;

            egui::Modal::new("web_file_browser".into()).show(ui.ctx(), |ui| {
                ui.set_min_width(280.0);
                ui.heading("Open document");
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(400.0)
                    .show(ui, |ui| {
                        if docs.is_empty() {
                            ui.label("No saved documents.");
                        } else {
                            selected = show_doc_tree(&tree, ui);
                        }
                    });
                ui.separator();
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });

            if let Some(name) = selected {
                self.web_open_document(&name);
                close = true;
            }
            if close {
                self.web.show_file_browser = false;
            }
        }

        if self.web.show_save_as_dialog {
            let mut do_save: Option<String> = None;
            let mut close = false;

            egui::Modal::new("web_save_as".into()).show(ui.ctx(), |ui| {
                ui.set_min_width(300.0);
                ui.heading("Save As");
                ui.separator();
                ui.label("Name (use / for folders, e.g. work/my-project):");
                let response = ui.text_edit_singleline(&mut self.web.save_as_input);
                let name = self.web.save_as_input.trim().to_string();
                let valid = !name.is_empty() && !name.starts_with('/') && !name.ends_with('/');
                ui.add_space(4.0);
                let already_exists = web_storage::list_documents().contains(&name);
                if already_exists {
                    ui.label(
                        egui::RichText::new("A document with this name already exists.")
                            .color(ui.visuals().warn_fg_color),
                    );
                }
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(valid, |ui| {
                        if ui.button("Save").clicked()
                            || (response.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                && valid)
                        {
                            do_save = Some(name);
                        }
                    });
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });

            if let Some(name) = do_save {
                self.web_save_to_name(&name);
                self.web.show_save_as_dialog = false;
            } else if close {
                self.web.show_save_as_dialog = false;
            }
        }
    }

    fn web_export_file(&self) {
        let doc = self.document.read().clone();
        wasm_bindgen_futures::spawn_local(async move {
            let json = match serde_json::to_string(&doc) {
                Ok(j) => j,
                Err(e) => {
                    log::error!("Serialization failed: {e}");
                    return;
                }
            };
            if let Some(handle) = rfd::AsyncFileDialog::new()
                .add_filter("Kanban", &["kan"])
                .set_file_name("kanban.kan")
                .save_file()
                .await
            {
                handle.write(json.as_bytes()).await.ok();
            }
        });
    }

    fn web_import_file(&self) {
        let pending = self.web.pending_import.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Some(handle) = rfd::AsyncFileDialog::new()
                .add_filter("Kanban", &["kan"])
                .pick_file()
                .await
            {
                let bytes = handle.read().await;
                *pending.lock().unwrap() = Some(Ok(bytes));
            }
        });
    }
}
