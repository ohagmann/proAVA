//! Rückfrage bei ungespeicherten Änderungen – „Projekt verlassen“ (Knopf
//! „Start“, Projekt ▸ Projekt schließen / Projekt öffnen …) und „proAVA
//! beenden“ (Fenster-X, auch bei minimiertem Fenster): Speichern, Verwerfen
//! und Abbrechen schließen die Rückfrage jedes Mal (Befund Anwender 18.11.:
//! der Dialog ging nicht weg).

mod common;

use std::path::{Path, PathBuf};

use common::*;
use egui::{ViewportCommand, ViewportEvent, ViewportId};
use egui_kittest::Harness;
use proava_application::launcher::autosave_path;
use proava_application::ProjectRepository;
use proava_infrastructure::XmlProjectFile;
use proava_ui::{strings, App, StartFile};

const LEAVE: &str = strings::WS_LEAVE_TITLE;
const QUIT: &str = strings::WS_QUIT_TITLE;
const CHANGED: &str = "Geändert vor dem Schließen";

/// The demo project in `folder/projekt/` (a folder of its own, so a test can
/// take it away), opened.
fn open(folder: &Path) -> (UiHarness, PathBuf) {
    let projects = folder.join("projekt");
    std::fs::create_dir_all(&projects).unwrap();
    let file = save(&projects, &proava_ui::demo::demo_project());
    let harness = harness(folder, Some(StartFile::Project(file.clone())));
    assert!(
        harness.state().project().is_some(),
        "Projekt nicht geöffnet"
    );
    (harness, file)
}

/// Changes the short text of the first position (unsaved).
fn change(harness: &mut UiHarness) -> usize {
    let index = select_first_position(harness);
    press(harness, Key::F2);
    press_with(harness, Modifiers::COMMAND, Key::A);
    type_text(harness, CHANGED);
    press(harness, Key::Enter);
    assert!(harness.state().workspace().unwrap().is_dirty());
    index
}

fn saved_text(file: &Path, index: usize) -> String {
    XmlProjectFile::new()
        .load_project(file)
        .unwrap()
        .lv()
        .rows()[index]
        .short_text()
}

fn question(harness: &UiHarness) -> bool {
    shows(harness, LEAVE) || shows(harness, QUIT)
}

fn open_question_by(harness: &mut UiHarness, route: Route) {
    match route {
        Route::Button => click(harness, strings::WS_BACK),
        Route::Menu => {
            click(harness, "Projekt");
            click_menu(harness, "Projekt schließen");
        }
    }
    assert!(shows(harness, LEAVE), "Rückfrage „Projekt verlassen“");
}

#[derive(Clone, Copy, Debug)]
enum Route {
    Button,
    Menu,
}

/// The window's X (or Alt+F4) for one frame; returns the commands of it.
fn close_window(harness: &mut UiHarness) -> Vec<ViewportCommand> {
    harness
        .input_mut()
        .viewports
        .entry(ViewportId::ROOT)
        .or_default()
        .events
        .push(ViewportEvent::Close);
    harness.step();
    let commands = root_commands(harness);
    settle(harness);
    commands
}

fn root_commands(harness: &UiHarness) -> Vec<ViewportCommand> {
    harness
        .output()
        .viewport_output
        .get(&ViewportId::ROOT)
        .map(|output| output.commands.clone())
        .unwrap_or_default()
}

/// Clicks a button of the question and returns the commands of that frame
/// (`Close` ends the program).
fn answer(harness: &mut UiHarness, label: &str) -> Vec<ViewportCommand> {
    harness.get_all_by_label(label).next().expect(label).click();
    let mut commands = Vec::new();
    for _ in 0..4 {
        harness.step();
        commands.extend(root_commands(harness));
    }
    commands
}

#[test]
fn discard_closes_the_question_and_the_project_without_saving() {
    for route in [Route::Button, Route::Menu] {
        let folder = TempDir::new("leave-discard");
        let (mut harness, file) = open(&folder.0);
        let index = change(&mut harness);
        let recovery = autosave_path(&file);
        std::fs::write(&recovery, b"alt").unwrap();

        open_question_by(&mut harness, route);
        let commands = answer(&mut harness, strings::WS_DISCARD);
        assert!(!question(&harness), "{route:?}: Rückfrage weg");
        assert!(
            harness.state().workspace().is_none(),
            "{route:?}: zurück auf der Startseite"
        );
        assert!(
            !commands.contains(&ViewportCommand::Close),
            "{route:?}: Programm läuft weiter"
        );
        assert_ne!(
            saved_text(&file, index),
            CHANGED,
            "{route:?}: nicht gespeichert"
        );
        assert!(
            !recovery.exists(),
            "{route:?}: Wiederherstellungsdatei verworfen"
        );
    }
}

#[test]
fn cancel_closes_the_question_and_keeps_the_project_open() {
    for route in [Route::Button, Route::Menu] {
        let folder = TempDir::new("leave-cancel");
        let (mut harness, file) = open(&folder.0);
        let index = change(&mut harness);
        let recovery = autosave_path(&file);
        std::fs::write(&recovery, b"alt").unwrap();

        open_question_by(&mut harness, route);
        answer(&mut harness, strings::CANCEL);
        assert!(!question(&harness), "{route:?}: Rückfrage weg");
        let workspace = harness.state().workspace().expect("Projekt bleibt offen");
        assert!(workspace.is_dirty(), "{route:?}: Änderung bleibt");
        assert!(
            !workspace.dialog_open(),
            "{route:?}: kein Dialog liegen geblieben"
        );
        assert_eq!(project(&harness).lv().rows()[index].short_text(), CHANGED);
        assert_ne!(saved_text(&file, index), CHANGED);
        assert!(
            recovery.exists(),
            "{route:?}: Wiederherstellungsdatei bleibt"
        );

        // Weiterarbeiten geht, und die nächste Anfrage fragt wieder.
        press(&mut harness, Key::ArrowDown);
        open_question_by(&mut harness, route);
        answer(&mut harness, strings::CANCEL);
        assert!(!question(&harness));
    }
}

#[test]
fn escape_works_like_cancel() {
    let folder = TempDir::new("leave-esc");
    let (mut harness, _file) = open(&folder.0);
    change(&mut harness);
    open_question_by(&mut harness, Route::Button);
    press(&mut harness, Key::Escape);
    assert!(!question(&harness), "Rückfrage weg");
    assert!(harness
        .state()
        .workspace()
        .is_some_and(|workspace| workspace.is_dirty()));
}

#[test]
fn save_saves_and_closes_the_project() {
    for route in [Route::Button, Route::Menu] {
        let folder = TempDir::new("leave-save");
        let (mut harness, file) = open(&folder.0);
        let index = change(&mut harness);

        open_question_by(&mut harness, route);
        let commands = answer(&mut harness, strings::WS_SAVE_AND_LEAVE);
        assert!(!question(&harness), "{route:?}: Rückfrage weg");
        assert!(
            harness.state().workspace().is_none(),
            "{route:?}: zurück auf der Startseite"
        );
        assert!(
            !commands.contains(&ViewportCommand::Close),
            "{route:?}: Programm läuft weiter"
        );
        assert_eq!(saved_text(&file, index), CHANGED, "{route:?}: gespeichert");
    }
}

#[test]
fn a_failed_save_keeps_the_question_with_the_reason() {
    let folder = TempDir::new("leave-save-error");
    let (mut harness, file) = open(&folder.0);
    change(&mut harness);
    // The project folder becomes a file: saving cannot write there.
    let projects = file.parent().unwrap().to_path_buf();
    std::fs::remove_dir_all(&projects).unwrap();
    std::fs::write(&projects, b"kein Ordner").unwrap();

    open_question_by(&mut harness, Route::Button);
    answer(&mut harness, strings::WS_SAVE_AND_LEAVE);
    assert!(shows(&harness, LEAVE), "Rückfrage bleibt");
    assert!(
        shows_part(&harness, "Speichern fehlgeschlagen"),
        "Grund in der Rückfrage"
    );
    assert!(harness
        .state()
        .workspace()
        .is_some_and(|workspace| workspace.is_dirty()));

    answer(&mut harness, strings::CANCEL);
    assert!(!question(&harness), "Abbrechen schließt sie");
    assert!(harness.state().workspace().is_some());
}

#[test]
fn open_other_project_then_discard_shows_the_open_dialog() {
    let folder = TempDir::new("leave-open");
    let (mut harness, _file) = open(&folder.0);
    change(&mut harness);
    click(&mut harness, "Projekt");
    click_menu(&mut harness, strings::OPEN_PROJECT);
    assert!(shows(&harness, LEAVE));
    answer(&mut harness, strings::WS_DISCARD);
    assert!(!question(&harness));
    assert!(harness.state().workspace().is_none());
    assert!(
        harness.get_all_by_label("Öffnen").next().is_some(),
        "Dialog „Projekt öffnen“"
    );
}

#[test]
fn open_other_project_then_cancel_forgets_the_open_request() {
    let folder = TempDir::new("leave-open-cancel");
    let (mut harness, _file) = open(&folder.0);
    change(&mut harness);
    click(&mut harness, "Projekt");
    click_menu(&mut harness, strings::OPEN_PROJECT);
    answer(&mut harness, strings::CANCEL);
    assert!(!question(&harness));

    // Later the window's X: ends the program, no open dialog in between.
    let commands = close_window(&mut harness);
    assert!(commands.contains(&ViewportCommand::CancelClose));
    assert!(shows(&harness, QUIT));
    let commands = answer(&mut harness, strings::WS_DISCARD);
    assert!(commands.contains(&ViewportCommand::Close), "beendet");
}

#[test]
fn window_close_asks_and_discard_ends_the_program() {
    let folder = TempDir::new("quit-discard");
    let (mut harness, file) = open(&folder.0);
    let index = change(&mut harness);

    let commands = close_window(&mut harness);
    assert!(
        commands.contains(&ViewportCommand::CancelClose),
        "Schließen angehalten"
    );
    assert!(shows(&harness, QUIT), "Rückfrage „proAVA beenden“");
    let commands = answer(&mut harness, strings::WS_DISCARD);
    assert!(commands.contains(&ViewportCommand::Close), "Programm endet");
    assert!(!question(&harness), "Rückfrage weg");
    assert!(harness.state().workspace().is_none());
    assert_ne!(saved_text(&file, index), CHANGED);
}

#[test]
fn window_close_then_cancel_stays_and_the_next_close_asks_again() {
    let folder = TempDir::new("quit-cancel");
    let (mut harness, file) = open(&folder.0);
    let index = change(&mut harness);

    close_window(&mut harness);
    assert!(shows(&harness, QUIT));
    let commands = answer(&mut harness, strings::CANCEL);
    assert!(!commands.contains(&ViewportCommand::Close));
    assert!(
        !question(&harness),
        "Rückfrage weg, nicht „Projekt verlassen“"
    );
    assert!(harness
        .state()
        .workspace()
        .is_some_and(|workspace| workspace.is_dirty()));

    // „Start“ danach: verlassen, nicht beenden.
    open_question_by(&mut harness, Route::Button);
    answer(&mut harness, strings::CANCEL);

    // Noch einmal X: wieder „proAVA beenden“; Speichern beendet nach dem Speichern.
    close_window(&mut harness);
    assert!(shows(&harness, QUIT));
    let commands = answer(&mut harness, strings::WS_SAVE_AND_QUIT);
    assert!(commands.contains(&ViewportCommand::Close), "Programm endet");
    assert_eq!(saved_text(&file, index), CHANGED, "gespeichert");
}

#[test]
fn window_close_without_changes_ends_at_once() {
    let folder = TempDir::new("quit-clean");
    let (mut harness, _file) = open(&folder.0);
    let commands = close_window(&mut harness);
    assert!(!commands.contains(&ViewportCommand::CancelClose));
    assert!(!question(&harness));
}

#[test]
fn read_only_window_discards_and_cancels() {
    for (label, stays) in [(strings::WS_DISCARD, false), (strings::CANCEL, true)] {
        let folder = TempDir::new("leave-read-only");
        let projects = folder.0.join("projekt");
        std::fs::create_dir_all(&projects).unwrap();
        let file = save(&projects, &proava_ui::demo::demo_project());
        let mut harness = harness(&folder.0, Some(StartFile::ProjectReadOnly(file.clone())));
        assert!(harness
            .state()
            .workspace()
            .is_some_and(|workspace| workspace.is_read_only()));
        let index = change(&mut harness);

        open_question_by(&mut harness, Route::Button);
        assert!(
            !shows(&harness, strings::WS_SAVE_AND_LEAVE),
            "kein Speichern im Schreibschutz"
        );
        answer(&mut harness, label);
        assert!(!question(&harness), "{label}: Rückfrage weg");
        assert_eq!(harness.state().workspace().is_some(), stays, "{label}");
        assert_ne!(saved_text(&file, index), CHANGED);
    }
}

/// The program as eframe runs it while the window is minimized: only
/// `App::logic`, no ui.
struct Minimizable {
    app: App,
    minimized: bool,
}

#[test]
fn closing_the_minimized_window_asks_instead_of_ending() {
    let folder = TempDir::new("quit-minimized");
    let projects = folder.0.join("projekt");
    std::fs::create_dir_all(&projects).unwrap();
    let file = save(&projects, &proava_ui::demo::demo_project());
    let state = Minimizable {
        app: App::new(launcher(&folder.0), Some(StartFile::Project(file.clone()))),
        minimized: false,
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .with_context_setup(App::setup)
        .with_accessibility_check(false)
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(
            |ui, state: &mut Minimizable| {
                if state.minimized {
                    state.app.logic(ui.ctx());
                } else {
                    state.app.ui(ui);
                }
            },
            state,
        );
    harness.run_steps(4);
    change_first_row(&mut harness);
    assert!(harness.state().app.workspace().unwrap().is_dirty());

    // Minimized, then closed from the window list.
    harness.state_mut().minimized = true;
    {
        let root = harness
            .input_mut()
            .viewports
            .entry(ViewportId::ROOT)
            .or_default();
        root.minimized = Some(true);
        root.events.push(ViewportEvent::Close);
    }
    harness.step();
    let commands = root_commands_of(&harness);
    assert!(
        commands.contains(&ViewportCommand::CancelClose),
        "nicht ohne Rückfrage beendet"
    );
    assert!(
        commands.contains(&ViewportCommand::Minimized(false)),
        "Fenster kommt zurück"
    );
    assert!(commands.contains(&ViewportCommand::Focus));

    // Back on the screen: the question is there and works.
    harness.state_mut().minimized = false;
    harness
        .input_mut()
        .viewports
        .entry(ViewportId::ROOT)
        .or_default()
        .minimized = Some(false);
    harness.run_steps(4);
    assert!(
        harness.query_all_by_label(QUIT).next().is_some(),
        "„proAVA beenden“"
    );
    harness
        .get_all_by_label(strings::CANCEL)
        .next()
        .unwrap()
        .click();
    harness.run_steps(4);
    assert!(
        harness.query_all_by_label(QUIT).next().is_none(),
        "Rückfrage weg"
    );
    assert!(harness
        .state()
        .app
        .workspace()
        .is_some_and(|workspace| workspace.is_dirty()));
    assert_ne!(
        XmlProjectFile::new()
            .load_project(&file)
            .unwrap()
            .lv()
            .rows()[0]
            .short_text(),
        CHANGED
    );
}

/// Types a new short text into the first row (F2, as in the table).
fn change_first_row(harness: &mut Harness<'static, Minimizable>) {
    harness.key_press(Key::Home);
    harness.run_steps(4);
    harness.key_press(Key::F2);
    harness.run_steps(4);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.run_steps(4);
    harness.event(egui::Event::Text(CHANGED.to_string()));
    harness.run_steps(4);
    harness.key_press(Key::Enter);
    harness.run_steps(4);
}

fn root_commands_of(harness: &Harness<'static, Minimizable>) -> Vec<ViewportCommand> {
    harness
        .output()
        .viewport_output
        .get(&ViewportId::ROOT)
        .map(|output| output.commands.clone())
        .unwrap_or_default()
}
