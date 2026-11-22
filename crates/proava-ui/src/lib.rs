//! Presentation layer (egui). Depends on the application layer, never on storage.

pub mod about;
pub mod autosave;
pub mod award_dialog;
pub mod bidders_view;
pub mod bids_view;
pub mod billing_view;
pub mod bridge;
pub mod cell_edit;
pub mod change_orders_view;
pub mod columns;
pub mod compare_view;
pub mod comparison_view;
pub mod complement_display;
pub mod context_print;
pub mod correspondence_view;
pub mod costs_view;
pub mod date_picker;
pub mod demo;
pub mod demo_change_orders;
pub mod file_dialog;
pub mod find_replace_bar;
pub mod fonts;
pub mod form_kit;
pub mod format;
pub mod frame_form;
pub mod help;
pub mod history;
pub mod icons;
pub mod images;
pub mod import_dialog;
pub mod jobs;
pub mod launcher;
pub mod letters_view;
pub mod library_sync;
pub mod library_view;
pub mod list_keys;
pub mod long_text_window;
pub mod lv_check;
pub mod lv_editor;
pub mod lv_list;
pub mod lv_ops;
pub mod lv_properties;
pub mod master_data_view;
pub mod menus;
pub mod numbers;
pub mod open_points;
mod owner_window;
pub mod oz_mask_dialog;
pub mod pdf_clip_dialog;
pub mod pdf_preview;
pub mod print_dialog;
pub mod protocol_view;
pub mod replace_question;
pub mod rich_editor;
pub mod search_replace;
pub mod settings_dialog;
pub mod shortcuts;
pub mod spell;
pub mod spell_dialog;
pub mod status_view;
pub mod stlb_dialog;
pub mod stlb_update_dialog;
pub mod strings;
pub mod struct_ops;
pub mod system_clipboard;
mod tender_search;
pub mod theme;
pub mod tracking_view;
pub mod transfer_view;
pub mod undo;
pub mod update_notice;
pub mod view;
pub mod viewer_copies;
pub mod web_window;
pub mod widgets;
pub mod window_drag;
mod window_focus;
mod window_level;
pub mod workspace;

pub use launcher::{LauncherScreen, OpenRequest};
pub use theme::Palette;
pub use workspace::{Outcome, Tab, WorkspaceScreen};

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;
use proava_application::Launcher;

/// Which of the two screens is in front.
enum Screen {
    Starter(Box<LauncherScreen>),
    Workspace(Box<WorkspaceScreen>),
}

/// Hosts the screens and switches between them.
struct Host {
    launcher: Arc<Launcher>,
    screen: Screen,
    /// Zoom from the settings last handed to egui (Strg+Plus/Minus stay free in between).
    applied_zoom: f32,
    /// Project from the command line, opened in the first frame.
    pending: Option<OpenRequest>,
    about: Option<about::AboutDialog>,
    help: Option<help::HelpWindow>,
    /// The project is open in another proAVA (lock file): ask first.
    lock_question: Option<(OpenRequest, proava_application::ProjectLock)>,
    /// Window had the focus last frame (settings reloaded on return).
    had_focus: bool,
    /// Local bridge to assistants (only the real program, see `run_with_bridge`).
    bridge: Option<bridge::HostBridge>,
    /// „Eigenschaften“ and „Langtext“ belong to the main window (`owner_window`).
    owner: owner_window::Owner,
    /// Was the program file replaced while it runs (update waiting)?
    update: update_notice::UpdateWatch,
}

/// A file given on the command line (double click in the file manager).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartFile {
    /// `.proava`: open the project.
    Project(PathBuf),
    /// `--schreibgeschuetzt <Projekt>`: open without saving (second window
    /// for copying from an old project).
    ProjectReadOnly(PathBuf),
    /// GAEB LV (X81–X86, D81–D86): „Neues Projekt aus GAEB“ with this file.
    Gaeb(PathBuf),
    /// CSV table (proAVA CSV format, `.csv`/`.tsv`): the same dialog with this file.
    Csv(PathBuf),
}

impl StartFile {
    /// Classifies a path by its extension; None for anything else.
    pub fn of(path: PathBuf) -> Option<Self> {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        let gaeb = extension.len() == 3
            && matches!(extension.as_bytes()[0], b'x' | b'd')
            && extension[1..]
                .parse::<u8>()
                .is_ok_and(|phase| (81..=86).contains(&phase));
        if extension == "proava" {
            Some(StartFile::Project(path))
        } else if gaeb {
            Some(StartFile::Gaeb(path))
        } else if matches!(extension.as_str(), "csv" | "tsv") {
            Some(StartFile::Csv(path))
        } else {
            None
        }
    }
}

impl Host {
    fn new(launcher: Arc<Launcher>, start: Option<StartFile>) -> Self {
        let mut starter = LauncherScreen::new(launcher.clone());
        let mut pending = None;
        // A damaged settings file was repaired: say so once.
        let notes = launcher.take_settings_notes();
        match start {
            Some(StartFile::Project(path)) => {
                // Into „Zuletzt verwendet“; a broken file is reported by `open`.
                let _ = launcher.open_project(&path);
                pending = Some(OpenRequest {
                    path,
                    messages: notes,
                    ..OpenRequest::default()
                })
            }
            Some(StartFile::ProjectReadOnly(path)) => {
                pending = Some(OpenRequest {
                    path,
                    read_only: true,
                    messages: notes,
                    ..OpenRequest::default()
                })
            }
            Some(StartFile::Gaeb(path) | StartFile::Csv(path)) => {
                starter.import_gaeb_file(&path);
                starter.notify(notes);
            }
            None => starter.notify(notes),
        }
        Self {
            launcher,
            screen: Screen::Starter(Box::new(starter)),
            applied_zoom: 0.0,
            pending,
            about: None,
            help: None,
            lock_question: None,
            had_focus: true,
            bridge: None,
            owner: owner_window::Owner::default(),
            update: update_notice::UpdateWatch::new(),
        }
    }

    /// Opens the channel of this window for assistants (local bridge).
    fn attach_bridge(
        &mut self,
        channel: Box<dyn proava_application::window_bridge::WindowChannel>,
        pid: u32,
        wake: proava_application::window_bridge::Wake,
    ) -> Result<(), String> {
        let bridge = bridge::HostBridge::open(channel, pid, wake)?;
        if let Screen::Workspace(workspace) = &mut self.screen {
            workspace.set_bridge(Some(bridge.handle()));
        }
        self.bridge = Some(bridge);
        Ok(())
    }

    /// Requests of assistants (on the UI thread) and the entry of the window.
    fn bridge_logic(&mut self, ctx: &egui::Context) {
        let Some(bridge) = self.bridge.as_mut() else {
            return;
        };
        let focused = ctx.input(|input| input.viewport().focused.unwrap_or(input.focused));
        bridge.note_focus(focused);
        let requests = bridge.take_requests();
        for incoming in requests {
            self.bridge_request(ctx, incoming);
        }
        let Some(mut bridge) = self.bridge.take() else {
            return;
        };
        let focus_ms = bridge.focus_ms();
        let pid = bridge.pid();
        bridge.publish_if_changed(ctx, || self.window_info(ctx, pid, focus_ms));
        self.bridge = Some(bridge);
    }

    fn bridge_request(
        &mut self,
        ctx: &egui::Context,
        incoming: proava_application::window_bridge::Incoming,
    ) {
        use proava_bridge_protocol::{Command, DropTarget, Reply};
        // Dragging rows between windows is the user's own work: it goes also
        // when assistants may not work in the window.
        if incoming.request.befehl.is_drag() {
            match &mut self.screen {
                Screen::Workspace(workspace) => workspace.drag_command(ctx, incoming),
                Screen::Starter(_) => {
                    let text = "Startseite – erst ein Projekt öffnen";
                    if matches!(incoming.request.befehl, Command::Ablegen(_)) {
                        incoming.answer(Reply::refused(text));
                    } else {
                        incoming.answer(Reply::Ziel(DropTarget {
                            text: text.to_string(),
                            moeglich: false,
                        }));
                    }
                }
            }
            return;
        }
        if !self.launcher.preferences().ai_windows {
            incoming.answer(bridge::switched_off());
            return;
        }
        match &incoming.request.befehl {
            Command::FensterLesen => {
                let (pid, focus_ms) = self
                    .bridge
                    .as_ref()
                    .map(|bridge| (bridge.pid(), bridge.focus_ms()))
                    .unwrap_or_default();
                let info = self.window_info(ctx, pid, focus_ms);
                incoming.answer(Reply::Fenster(info));
            }
            Command::ProjektOeffnen(open) => {
                let reply = self.bridge_open(open);
                incoming.answer(reply);
            }
            _ => match &mut self.screen {
                Screen::Workspace(workspace) => workspace.bridge_command(ctx, incoming),
                Screen::Starter(_) => incoming.answer(Reply::refused(
                    "Dieses Fenster zeigt die Startseite – kein Projekt offen. Erst ein Projekt öffnen \
                     (fenster_projekt_oeffnen).",
                )),
            },
        }
    }

    /// Opens a project for an assistant: in this window when it shows the
    /// start page, else in a new window (same brakes as Projekt ▸ Neues Fenster).
    fn bridge_open(
        &mut self,
        open: &proava_bridge_protocol::OpenProject,
    ) -> proava_bridge_protocol::Reply {
        use proava_bridge_protocol::{Done, Reply};
        let path = PathBuf::from(open.pfad.trim());
        let is_project = path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("proava"));
        if !path.is_absolute() || !is_project {
            return Reply::refused(format!(
                "Bitte den absoluten Pfad einer proAVA-Projektdatei (.proava) angeben: {}",
                path.display()
            ));
        }
        if !path.is_file() {
            return Reply::refused(format!("Projekt nicht gefunden: {}", path.display()));
        }
        let name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let how = if open.schreibgeschuetzt {
            " schreibgeschützt"
        } else {
            ""
        };
        match &self.screen {
            Screen::Starter(_) => {
                self.pending = Some(OpenRequest {
                    path,
                    read_only: open.schreibgeschuetzt,
                    ..OpenRequest::default()
                });
                Reply::Erledigt(Done {
                    text: format!(
                        "„{name}“ wird in diesem Fenster{how} geöffnet (ist es schon in einem anderen \
                         Fenster offen, fragt proAVA den Anwender)."
                    ),
                    ..Done::default()
                })
            }
            Screen::Workspace(workspace) if workspace.project_file() == path => {
                Reply::Erledigt(Done {
                    text: format!("„{name}“ ist schon in diesem Fenster offen."),
                    ..Done::default()
                })
            }
            Screen::Workspace(_) => {
                let mut arguments: Vec<std::ffi::OsString> = Vec::new();
                if open.schreibgeschuetzt {
                    arguments.push("--schreibgeschuetzt".into());
                }
                arguments.push(path.into_os_string());
                match jobs::new_window(&arguments) {
                    Ok(()) => Reply::Erledigt(Done {
                        text: format!(
                            "Neues proAVA-Fenster mit „{name}“{how} wird gestartet – es erscheint in \
                             einigen Sekunden (ist das Projekt schon offen, fragt proAVA den Anwender)."
                        ),
                        ..Done::default()
                    }),
                    Err(error) => Reply::refused(error),
                }
            }
        }
    }

    /// The state of this window for assistants.
    fn window_info(
        &self,
        ctx: &egui::Context,
        pid: u32,
        focus_ms: u64,
    ) -> proava_bridge_protocol::WindowInfo {
        let (outer, inner, monitor, focused, minimized, pixels_per_point) = ctx.input(|input| {
            let viewport = input.viewport();
            (
                viewport.outer_rect,
                viewport.inner_rect,
                viewport.monitor_size,
                viewport.focused.unwrap_or(input.focused),
                viewport.minimized.unwrap_or(false),
                input.pixels_per_point(),
            )
        });
        let allowed = self.launcher.preferences().ai_windows;
        let (mut project, activity, dialog) = match &self.screen {
            Screen::Workspace(workspace) => (
                Some(workspace.bridge_info()),
                workspace.ai_activity(),
                workspace.dialog_title(),
            ),
            Screen::Starter(_) => (
                None,
                None,
                self.lock_question
                    .as_ref()
                    .map(|_| "Projekt ist schon geöffnet".to_string()),
            ),
        };
        if !allowed {
            // Switched off: only what keeps the file tools from overwriting.
            if let Some(project) = project.as_mut() {
                project.auswahl.clear();
            }
        }
        proava_bridge_protocol::WindowInfo {
            pid,
            programm: about::VERSION.to_string(),
            ki_erlaubt: allowed,
            lage: outer.map(|rect| proava_bridge_protocol::Geometry {
                x: rect.min.x,
                y: rect.min.y,
                breite: rect.width(),
                hoehe: rect.height(),
            }),
            innen_px: inner.map(|rect| {
                let rect = rect * pixels_per_point;
                proava_bridge_protocol::Geometry {
                    x: rect.min.x,
                    y: rect.min.y,
                    breite: rect.width(),
                    hoehe: rect.height(),
                }
            }),
            bildschirm: monitor.map(|size| [size.x, size.y]),
            fokus: focused,
            fokus_zuletzt_ms: focus_ms,
            minimiert: minimized,
            projekt: project,
            ki_taetigkeit: activity,
            dialog,
        }
    }

    fn open(&mut self, ctx: &egui::Context, request: OpenRequest) {
        if !request.read_only && !request.ignore_lock {
            match self.launcher.foreign_lock(&request.path) {
                // Left by a proAVA of this computer that no longer runs
                // (killed, crashed): away with it, open as usual.
                Some(lock) if lock.stale => self.launcher.remove_stale_lock(&request.path),
                Some(lock) => {
                    self.lock_question = Some((request, lock));
                    return;
                }
                None => {}
            }
        }
        let path = request.path.as_path();
        match self.launcher.load_project(path) {
            Ok(project) => {
                let mut messages = request.messages;
                if !request.read_only {
                    if let Err(error) = self.launcher.lock_project(path) {
                        messages.push(format!("Sperrdatei nicht angelegt – ein zweites Fenster bemerkt das Öffnen nicht: {error}"));
                    }
                }
                let mut workspace =
                    WorkspaceScreen::new(self.launcher.clone(), path.to_path_buf(), project)
                        .read_only(request.read_only)
                        .with_messages(strings::WARNINGS_TITLE, messages)
                        .with_protocol(request.protocol);
                workspace.set_bridge(self.bridge.as_ref().map(bridge::HostBridge::handle));
                self.screen = Screen::Workspace(Box::new(workspace));
            }
            Err(error) => {
                if let Screen::Starter(starter) = &mut self.screen {
                    starter.failed(ctx, error);
                }
            }
        }
    }

    /// „Projekt ist schon geöffnet“ (another proAVA that runs, or one on
    /// another computer): read-only or anyway. A stale lock never gets here.
    fn lock_dialog(&mut self, ui: &mut egui::Ui) -> Option<OpenRequest> {
        let (request, lock) = self.lock_question.as_ref()?;
        let palette = Palette::of(self.launcher.theme());
        let name = request
            .path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut answer: Option<bool> = None; // read_only
        let mut cancel = false;
        let (_, close) = widgets::modal(ui, &palette, 600.0, |ui| {
            widgets::title(
                ui,
                &palette,
                "Projekt ist schon geöffnet",
                &format!(
                    "„{name}“ ist geöffnet von {}. Schreibgeschützt öffnen erlaubt Lesen und Kopieren. \
                     Trotzdem öffnen kann Änderungen des anderen Fensters überschreiben.",
                    lock.who()
                ),
            );
            ui.add_space(8.0);
            // Right to left: the main answer on the right.
            widgets::button_row(ui, |ui| {
                if widgets::button(ui, &palette, "Schreibgeschützt öffnen", true, 230.0).clicked()
                {
                    answer = Some(true);
                }
                ui.add_space(8.0);
                if widgets::button(ui, &palette, "Trotzdem öffnen", false, 200.0).clicked() {
                    answer = Some(false);
                }
                ui.add_space(8.0);
                if widgets::button(ui, &palette, strings::CANCEL, false, 120.0).clicked() {
                    cancel = true;
                }
            });
        });
        if cancel || close {
            self.lock_question = None;
            return None;
        }
        let read_only = answer?;
        let (mut request, _) = self.lock_question.take()?;
        request.read_only = read_only;
        request.ignore_lock = true;
        Some(request)
    }

    /// „Neu starten“ after an update: starts the new program file (same
    /// project) and closes this window; if that fails, the start page says
    /// so and the window stays.
    fn restart(&mut self, ctx: &egui::Context, arguments: &[std::ffi::OsString]) {
        match jobs::restart(arguments) {
            Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Err(error) => {
                if let Screen::Starter(starter) = &mut self.screen {
                    starter.say(
                        ctx,
                        format!("Neu starten ging nicht: {error} Bitte proAVA selbst neu starten."),
                    );
                }
            }
        }
    }

    fn back(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(
            strings::WINDOW_TITLE.to_string(),
        ));
        self.launcher.reload_settings();
        self.screen = Screen::Starter(Box::new(LauncherScreen::new(self.launcher.clone())));
    }
}

impl eframe::App for Host {
    /// Runs before every `ui` and alone while the window is minimized or
    /// hidden – so closing it from the window list asks too.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.logic(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.frame(ui);
        use raw_window_handle::HasWindowHandle;
        let main = frame.window_handle().ok().map(|handle| handle.as_raw());
        let own = self.launcher.windows_in_front() && !window_level::main_dialog(ui.ctx());
        self.owner.update(ui.ctx(), main, own);
    }

    /// End of the program: close the project (its lock file goes) and
    /// remove the own copies for the PDF viewer (Schriftverkehr im Projekt).
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.screen = Screen::Starter(Box::new(LauncherScreen::new(self.launcher.clone())));
        viewer_copies::remove_own();
        if let Some(bridge) = self.bridge.as_mut() {
            bridge.close();
        }
    }

    /// Befund 2 (17.11.): eframe clears with almost black (12, 12, 12). At a
    /// zoom or screen scale that is not a whole number, neighbouring panels
    /// (header, Bearbeitungsfenster, table, status bar) can miss each other by
    /// a fraction of a pixel, and the clear colour showed as a thin black line
    /// across the window. Clear with the panel colour instead.
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        clear_color_of(visuals)
    }
}

/// The colour behind all panels: the opaque panel colour of the theme.
fn clear_color_of(visuals: &egui::Visuals) -> [f32; 4] {
    visuals.panel_fill.to_opaque().to_normalized_gamma_f32()
}

impl Host {
    /// The window is to close (X, Alt+F4, window list): with unsaved changes
    /// the workspace cancels it and asks. eframe calls this without `ui`
    /// while the window is minimized or covered – then the window comes back,
    /// so the question can be seen and answered.
    fn logic(&mut self, ctx: &egui::Context) {
        self.bridge_logic(ctx);
        if !ctx.input(|input| input.viewport().close_requested()) {
            return;
        }
        let Screen::Workspace(workspace) = &mut self.screen else {
            return;
        };
        if workspace.ask_before_quit(ctx)
            && ctx.input(|input| input.viewport().visible() == Some(false))
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            ctx.request_repaint();
        }
    }

    /// One frame of the whole program.
    fn frame(&mut self, ui: &mut egui::Ui) {
        // Same reason as `clear_color`: whatever lies between the panels is
        // panel colour, also where the clear colour is not used.
        ui.painter().rect_filled(
            ui.max_rect(),
            egui::CornerRadius::ZERO,
            ui.visuals().panel_fill,
        );
        // „Heute“ for the date fields (the launcher owns the clock).
        date_picker::set_today(ui.ctx(), &self.launcher.now_iso());
        // Zeilen- und Absatzabstand of the text editors (as printed).
        rich_editor::set_text_spacing(ui.ctx(), self.launcher.text_spacing());
        let zoom = self.launcher.ui_zoom();
        if zoom != self.applied_zoom {
            self.applied_zoom = zoom;
            ui.ctx().set_zoom_factor(zoom);
        }
        // Typed and pasted text without stray control characters (Word and
        // PDF bring them along; the project file is XML).
        ui.ctx().input_mut(|input| {
            for event in &mut input.events {
                if let egui::Event::Text(text) | egui::Event::Paste(text) = event {
                    if let std::borrow::Cow::Owned(clean) = proava_domain::strip_control_chars(text)
                    {
                        *text = clean;
                    }
                }
            }
        });
        // F1: help on the view in front (before the screens see the key).
        if self.help.is_none()
            && ui
                .ctx()
                .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F1))
        {
            let topic = match &self.screen {
                Screen::Starter(_) => help::Topic::Start,
                Screen::Workspace(workspace) => workspace.help_topic(),
            };
            help::request(ui.ctx(), topic);
        }
        // Back in the window: another proAVA may have changed the settings
        // (recent projects) meanwhile.
        let focused = ui.ctx().input(|input| input.focused);
        if focused && !self.had_focus && matches!(self.screen, Screen::Starter(_)) {
            self.launcher.reload_settings();
        }
        self.had_focus = focused;
        let mut open: Option<OpenRequest> = self.pending.take();
        let mut back = false;
        let mut open_dialog = false;
        // „Neu starten“ after an update: the arguments for the new program.
        let mut restart: Option<Vec<std::ffi::OsString>> = None;
        let updated = self.update.poll(ui.ctx());

        match &mut self.screen {
            Screen::Starter(starter) => {
                starter.set_program_updated(updated);
                if let Some(request) = starter.ui(ui) {
                    open = Some(request);
                }
                if starter.take_restart() {
                    restart = Some(Vec::new());
                }
            }
            Screen::Workspace(workspace) => {
                workspace.set_program_updated(updated);
                match workspace.ui(ui) {
                    Outcome::Stay => {}
                    Outcome::Back => back = true,
                    Outcome::BackToOpen => {
                        back = true;
                        open_dialog = true;
                    }
                    Outcome::Restart => {
                        restart = Some(workspace.restart_arguments());
                        back = true;
                    }
                }
            }
        }

        // A GAEB import in the workspace may ask to open the new project.
        if let Some(request) = import_dialog::take_open_request(ui.ctx()) {
            open = Some(request);
        }
        if let Some(request) = open {
            self.open(ui.ctx(), request);
        }
        if let Some(request) = self.lock_dialog(ui) {
            self.open(ui.ctx(), request);
        }
        if back {
            self.back(ui.ctx());
            if open_dialog {
                if let Screen::Starter(starter) = &mut self.screen {
                    starter.show_open_dialog();
                }
            }
        }
        // After `back`: the project is closed and its lock is gone, so the
        // new program can open it.
        if let Some(arguments) = restart {
            self.restart(ui.ctx(), &arguments);
        }
        if about::take_request(ui.ctx()) {
            self.about = Some(about::AboutDialog::new());
        }
        if let Some(topic) = help::take_request(ui.ctx()) {
            self.help = Some(help::HelpWindow::new(topic));
        }
        if help::take_handbook_request(ui.ctx()) {
            if let Err(error) = help::open_handbook() {
                self.help = Some(help::HelpWindow::new(help::Topic::All).with_notice(error));
            }
        }
        // Own id scopes: a dialog of the workspace (e.g. the command palette)
        // may be open at the same time, and modals take their id from `ui`.
        if let Some(window) = &mut self.help {
            let palette = Palette::of(self.launcher.theme());
            if !ui
                .push_id("help_window", |ui| window.show(ui, &palette))
                .inner
            {
                self.help = None;
            }
        }
        if let Some(dialog) = &mut self.about {
            let palette = Palette::of(self.launcher.theme());
            if !ui
                .push_id("about_dialog", |ui| dialog.show(ui, &palette))
                .inner
            {
                self.about = None;
            }
        }
    }
}

/// The whole program as one egui UI – what the window shows – without a
/// window, e.g. for automatic tests with `egui_kittest`.
pub struct App {
    host: Host,
}

impl App {
    pub fn new(launcher: Arc<Launcher>, start: Option<StartFile>) -> Self {
        Self {
            host: Host::new(launcher, start),
        }
    }

    /// Opens the local bridge of this app in `channel` (tests: a channel in a
    /// temporary folder; the real program does it in `run_with_bridge`).
    /// Requests are handled in the frames the caller runs.
    pub fn with_bridge(
        mut self,
        channel: Box<dyn proava_application::window_bridge::WindowChannel>,
        pid: u32,
    ) -> Result<Self, String> {
        self.host.attach_bridge(channel, pid, Arc::new(|| {}))?;
        Ok(self)
    }

    /// Fonts the program needs; call once on a new context.
    pub fn setup(ctx: &egui::Context) {
        fonts::install(ctx);
    }

    /// One full frame as eframe runs it: `logic`, then the ui.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.host.logic(ui.ctx());
        self.host.frame(ui);
    }

    /// Only the logic, as eframe runs it while the window is minimized or
    /// hidden (no ui, nothing painted).
    pub fn logic(&mut self, ctx: &egui::Context) {
        self.host.logic(ctx);
    }

    /// The open workspace, None on the start screen.
    pub fn workspace(&self) -> Option<&WorkspaceScreen> {
        match &self.host.screen {
            Screen::Workspace(workspace) => Some(workspace),
            Screen::Starter(_) => None,
        }
    }

    /// A message of the browser window for the open take-over dialog (see
    /// [`WorkspaceScreen::browser_event`]); false without such a dialog.
    pub fn browser_event(&mut self, event: proava_web_protocol::FromHelper) -> bool {
        match &mut self.host.screen {
            Screen::Workspace(workspace) => workspace.browser_event(event),
            Screen::Starter(_) => false,
        }
    }

    /// The open project, None on the start screen.
    pub fn project(&self) -> Option<&proava_domain::Project> {
        self.workspace().map(WorkspaceScreen::project)
    }

    /// Whether long work (import, export, PDF) still runs in the background.
    pub fn busy(&self) -> bool {
        match &self.host.screen {
            Screen::Workspace(workspace) => workspace.busy(),
            Screen::Starter(starter) => starter.busy(),
        }
    }
}

/// Windows: the release program has no console of its own (GUI program);
/// for `--version`, `--help` and command-line errors it writes into the
/// console it was started from (cmd, PowerShell). Without one, and on
/// other systems, nothing happens.
pub fn attach_parent_console() {
    #[cfg(windows)]
    // SAFETY: a plain Win32 call without pointers; it fails harmlessly when
    // there is no parent console or the program already has one (debug build).
    unsafe {
        use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

/// Starts the app on the start screen and hands over to the workspace when a project opens.
pub fn run(launcher: Arc<Launcher>) -> eframe::Result<()> {
    run_with(launcher, None)
}

/// Like [`run`], with a file from the command line.
pub fn run_with(launcher: Arc<Launcher>, start: Option<StartFile>) -> eframe::Result<()> {
    run_with_bridge(launcher, start, None)
}

/// Like [`run_with`], with the local bridge to assistants (`channel` from the
/// composition root; None = no bridge).
pub fn run_with_bridge(
    launcher: Arc<Launcher>,
    start: Option<StartFile>,
    channel: Option<Box<dyn proava_application::window_bridge::WindowChannel>>,
) -> eframe::Result<()> {
    // Only the real program talks to the system clipboard (tests never).
    system_clipboard::enable();
    // Copies for the PDF viewer left by proAVA processes that ended (crash).
    viewer_copies::remove_stale(&std::env::temp_dir(), std::process::id(), |pid| {
        launcher.proava_running(pid)
    });
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1180.0, 760.0])
        .with_min_inner_size([640.0, 420.0])
        .with_title(strings::WINDOW_TITLE)
        // Wayland/X11: matches the .desktop file, so the dock shows the symbol.
        .with_app_id("proava");
    if let Some(icon) = about::window_icon() {
        viewport = viewport.with_icon(Arc::new(icon));
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        strings::APP_TITLE,
        options,
        Box::new(move |creation| {
            fonts::install(&creation.egui_ctx);
            let mut host = Host::new(launcher, start);
            if let Some(channel) = channel {
                let ctx = creation.egui_ctx.clone();
                let wake: proava_application::window_bridge::Wake =
                    Arc::new(move || ctx.request_repaint());
                if let Err(error) = host.attach_bridge(channel, std::process::id(), wake) {
                    eprintln!("[proava] Brücke für KI-Assistenten nicht geöffnet: {error}");
                }
            }
            Ok(Box::new(host))
        }),
    )
}

#[cfg(test)]
mod start_tests {
    use super::StartFile;
    use std::path::PathBuf;

    #[test]
    fn start_files_by_extension() {
        let of = |name: &str| StartFile::of(PathBuf::from(name));
        assert_eq!(
            of("/a/B.proava"),
            Some(StartFile::Project("/a/B.proava".into()))
        );
        assert_eq!(of("LV.X83"), Some(StartFile::Gaeb("LV.X83".into())));
        assert_eq!(of("lv.d81"), Some(StartFile::Gaeb("lv.d81".into())));
        assert_eq!(of("LV.X89"), None);
        assert_eq!(of("notiz.txt"), None);
        assert_eq!(
            of("/a/Zulieferung.CSV"),
            Some(StartFile::Csv("/a/Zulieferung.CSV".into()))
        );
    }
}

#[cfg(test)]
mod clear_color_tests {
    use proava_application::ThemeChoice;

    /// Befund 2: gaps between panels must not show a dark line.
    #[test]
    fn window_is_cleared_with_the_opaque_panel_colour() {
        for choice in [ThemeChoice::Light, ThemeChoice::Dark] {
            let visuals = crate::theme::visuals(choice);
            let [r, g, b, a] = super::clear_color_of(&visuals);
            assert_eq!(a, 1.0, "deckend");
            let panel = crate::theme::Palette::of(choice)
                .panel
                .to_normalized_gamma_f32();
            assert_eq!([r, g, b], [panel[0], panel[1], panel[2]], "{choice:?}");
        }
        let [r, g, b, _] = super::clear_color_of(&crate::theme::visuals(ThemeChoice::Light));
        assert!(
            r + g + b > 2.5,
            "hell, nicht das fast schwarze Grau von eframe"
        );
    }
}
