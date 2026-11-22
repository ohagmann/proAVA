//! Dragging rows from one proAVA window into another over the local bridge:
//! the pointer leaves window A, A tells window B where it is (`ziehen_ueber`),
//! B shows the insert mark at the row under it, letting go inserts there
//! (`ablegen`) the way of Strg+V – one Strg+Z in B takes it back, A stays as
//! it is.
//!
//! Both windows are harnesses of this test with real sockets in the test's
//! temporary folder (never `$XDG_RUNTIME_DIR`); their places on the desktop
//! are set in the input, A on the left (0 … 1400), B to the right of it
//! (from 1500). No process is started.

#![cfg(unix)]

mod common;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use proava_domain::{Lv, LvGroup, LvRow, OzMask, Position, Project, ProjectName};
use proava_infrastructure::window_bridge::{BridgeFolder, SocketChannel};
use proava_ui::{App, StartFile};

const PID_A: u32 = 4_200_001;
const PID_B: u32 = 4_200_002;
/// Left edge of window B on the desktop (A is 1400 wide at 0).
const B_LEFT: f32 = 1500.0;

struct Pane {
    harness: UiHarness,
    launcher: Arc<proava_application::Launcher>,
}

fn pane(bridge: &Path, folder: &Path, start: StartFile, pid: u32, left: f32) -> Pane {
    proava_ui::viewer_copies::use_folder_on_this_thread(Some(folder.join("pdf-anzeige")));
    // Both windows run in this test program with made-up PIDs: they count as running.
    let channel = SocketChannel::in_folder(BridgeFolder::at(bridge))
        .with_process_check(Arc::new(|_| Some(true)));
    let launcher = launcher(folder);
    let app = App::new(launcher.clone(), Some(start))
        .with_bridge(Box::new(channel), pid)
        .expect("Brücke im Temp-Ordner");
    let mut harness = harness_of(app);
    harness
        .input_mut()
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .inner_rect = Some(egui::Rect::from_min_size(
        egui::pos2(left, 0.0),
        egui::vec2(1400.0, 900.0),
    ));
    settle(&mut harness);
    assert!(harness.state().project().is_some(), "Projekt offen");
    Pane { harness, launcher }
}

fn source_project() -> Project {
    let mut lv = Lv::new(
        "Quelle",
        OzMask::parse(OzMask::DEFAULT_PATTERN).expect("Maske"),
    );
    lv.push_row(LvRow::Group(LvGroup::new(0, "01", "Schaltschränke")));
    for (oz, text) in [
        ("01.0010", "Informationsschwerpunkt ASP01"),
        ("01.0020", "Schaltschrankleuchte LED"),
        ("01.0030", "Schaltschranklüfter"),
    ] {
        lv.push_row(LvRow::Position(Position::new(
            oz,
            text,
            proava_domain::Decimal::ONE,
            "St",
            Some(proava_domain::Decimal::from(85)),
        )));
    }
    Project::new(
        ProjectName::parse("Schule Musterstadt").unwrap(),
        "2026-11-18T18:00:00",
        lv,
    )
}

fn target_project() -> Project {
    target_with(0)
}

/// The target LV with `more` further positions after the first two.
fn target_with(more: usize) -> Project {
    let mut lv = Lv::new(
        "Ziel",
        OzMask::parse(OzMask::DEFAULT_PATTERN).expect("Maske"),
    );
    lv.push_row(LvRow::Group(LvGroup::new(0, "01", "Gebäudeautomation")));
    let mut names = vec![
        ("01.0010".to_string(), "Automationsstation".to_string()),
        ("01.0020".to_string(), "Raumfühler".to_string()),
    ];
    for number in 0..more {
        names.push((
            format!("01.{:04}", 30 + number * 10),
            format!("Feldgerät {}", number + 1),
        ));
    }
    for (oz, text) in names {
        lv.push_row(LvRow::Position(Position::new(
            oz,
            text,
            proava_domain::Decimal::ONE,
            "St",
            None,
        )));
    }
    Project::new(
        ProjectName::parse("Rathaus 3").unwrap(),
        "2026-11-18T18:00:00",
        lv,
    )
}

struct Windows {
    a: Pane,
    b: Pane,
    _folders: Vec<TempDir>,
}

fn two_windows(label: &str, b_read_only: bool) -> Windows {
    two_windows_with(label, b_read_only, target_project())
}

fn two_windows_with(label: &str, b_read_only: bool, target: Project) -> Windows {
    let bridge = TempDir::new(&format!("ziehen-bruecke-{label}"));
    let folder_a = TempDir::new(&format!("ziehen-a-{label}"));
    let folder_b = TempDir::new(&format!("ziehen-b-{label}"));
    let file_a = save(&folder_a.0, &source_project());
    let file_b = save(&folder_b.0, &target);
    let a = pane(
        &bridge.0,
        &folder_a.0,
        StartFile::Project(file_a),
        PID_A,
        0.0,
    );
    let start_b = if b_read_only {
        StartFile::ProjectReadOnly(file_b)
    } else {
        StartFile::Project(file_b)
    };
    let b = pane(&bridge.0, &folder_b.0, start_b, PID_B, B_LEFT);
    let mut windows = Windows {
        a,
        b,
        _folders: vec![bridge, folder_a, folder_b],
    };
    // Both announce themselves (the entry is compared twice a second).
    for _ in 0..45 {
        windows.a.harness.step();
        windows.b.harness.step();
    }
    windows
}

fn workspace(pane: &Pane) -> &proava_ui::WorkspaceScreen {
    pane.harness.state().workspace().expect("Arbeitsbereich")
}

fn rows(pane: &Pane) -> Vec<String> {
    workspace(pane)
        .project()
        .lv()
        .rows()
        .iter()
        .map(LvRow::short_text)
        .collect()
}

/// Centre of a row of the LV table (accessible name „OZ Kurztext“).
fn row_center(pane: &Pane, name: &str) -> egui::Pos2 {
    pane.harness
        .get_all_by_label(name)
        .next()
        .unwrap_or_else(|| panic!("Zeile {name}"))
        .rect()
        .center()
}

/// Runs frames of both windows until `done` (the other window answers on
/// its own socket thread, the frames here).
fn run_until(windows: &mut Windows, mut done: impl FnMut(&Windows) -> bool) -> bool {
    for _ in 0..400 {
        if done(windows) {
            return true;
        }
        windows.a.harness.step();
        windows.b.harness.step();
        std::thread::sleep(Duration::from_millis(2));
    }
    done(windows)
}

fn press_at(harness: &mut UiHarness, pos: egui::Pos2, pressed: bool) {
    harness.event(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    });
    harness.step();
}

/// Grabs the row `name` in A and moves the pointer in steps to `to` (A's
/// coordinates, beyond its right edge), without letting go.
fn drag_from_a(windows: &mut Windows, name: &str, to: egui::Pos2) {
    let from = row_center(&windows.a, name);
    let harness = &mut windows.a.harness;
    harness.event(egui::Event::PointerMoved(from));
    harness.step();
    press_at(harness, from, true);
    for step in 1..=24 {
        let t = step as f32 / 24.0;
        harness.event(egui::Event::PointerMoved(from + (to - from) * t));
        harness.step();
    }
}

/// The point in A's coordinates that lies over row `name` of B (more to the
/// right than A is wide).
fn over_row_of_b(windows: &Windows, name: &str, lower_half: bool) -> egui::Pos2 {
    let center = row_center(&windows.b, name);
    egui::pos2(
        B_LEFT + center.x,
        center.y + if lower_half { 6.0 } else { -6.0 },
    )
}

#[test]
fn rows_dragged_out_of_one_window_are_copied_into_the_other_at_the_mark() {
    let mut windows = two_windows("kopieren", false);
    let before_a = rows(&windows.a);
    let before_b = rows(&windows.b);
    let to = over_row_of_b(&windows, "01.0010 Automationsstation", true);
    drag_from_a(&mut windows, "01.0020 Schaltschrankleuchte LED", to);

    // B zeigt die Marke an der Zeile unter dem Zeiger (darunter).
    assert!(
        run_until(&mut windows, |w| workspace(&w.b).drag_mark().is_some()),
        "Marke in B"
    );
    let (row, after) = workspace(&windows.b).drag_mark().unwrap();
    let marked = workspace(&windows.b).project().lv().row_by_id(row).unwrap();
    assert_eq!(marked.short_text(), "Automationsstation");
    assert!(after, "untere Hälfte: dahinter");
    assert!(workspace(&windows.a).dragging_out());
    assert!(
        shows_part(&windows.b.harness, "1 Position aus Schule Musterstadt"),
        "Hinweis am Zeiger in B"
    );
    assert!(shows_part(
        &windows.b.harness,
        "loslassen fügt hinter 01.0010 ein"
    ));

    // Loslassen: in B eingefügt, A bleibt unverändert.
    press_at(&mut windows.a.harness, to, false);
    assert!(
        run_until(&mut windows, |w| rows(&w.b).len() == before_b.len() + 1),
        "eingefügt"
    );
    let after_b = rows(&windows.b);
    assert_eq!(
        after_b,
        [
            "Gebäudeautomation",
            "Automationsstation",
            "Schaltschrankleuchte LED",
            "Raumfühler"
        ]
    );
    assert_eq!(rows(&windows.a), before_a, "Quelle bleibt (Kopie)");
    assert!(!workspace(&windows.a).is_dirty());
    assert!(workspace(&windows.b).is_dirty());
    assert!(workspace(&windows.b).drag_mark().is_none(), "Marke weg");
    let copied = workspace(&windows.b)
        .project()
        .lv()
        .rows()
        .iter()
        .find(|row| row.short_text() == "Schaltschrankleuchte LED")
        .and_then(LvRow::as_position)
        .unwrap()
        .clone();
    assert_eq!(
        copied.ordinal(),
        "01.0015",
        "freie OZ zwischen den Nachbarn in B (wie Strg+V)"
    );
    assert!(copied.unit_price().is_some(), "wie Strg+C mit Preis");
    assert!(run_until(&mut windows, |w| shows_part(
        &w.a.harness,
        "Rathaus 3: 1 Position eingefügt ab 01.0015 (Strg+Z dort nimmt es zurück)"
    )));

    // Strg+Z in B: ein Schritt.
    press_with(&mut windows.b.harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(rows(&windows.b), before_b);
}

#[test]
fn dropping_on_the_upper_half_of_the_first_row_inserts_before_it() {
    let mut windows = two_windows("davor", false);
    let to = over_row_of_b(&windows, "01 Gebäudeautomation", false);
    drag_from_a(&mut windows, "01.0030 Schaltschranklüfter", to);
    assert!(run_until(&mut windows, |w| workspace(&w.b)
        .drag_mark()
        .is_some()));
    assert_eq!(
        workspace(&windows.b).drag_mark().map(|(_, after)| after),
        Some(false)
    );
    press_at(&mut windows.a.harness, to, false);
    assert!(run_until(&mut windows, |w| rows(&w.b).len() == 4));
    assert_eq!(
        rows(&windows.b)[0],
        "Schaltschranklüfter",
        "vor der ersten Zeile"
    );
    press_with(&mut windows.b.harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(rows(&windows.b).len(), 3, "ein Strg+Z");
}

#[test]
fn esc_and_letting_go_outside_cancel_the_drag() {
    let mut windows = two_windows("abbrechen", false);
    let before_b = rows(&windows.b);
    let to = over_row_of_b(&windows, "01.0020 Raumfühler", true);
    drag_from_a(&mut windows, "01.0010 Informationsschwerpunkt ASP01", to);
    assert!(run_until(&mut windows, |w| workspace(&w.b)
        .drag_mark()
        .is_some()));
    windows.a.harness.key_press(Key::Escape);
    windows.a.harness.step();
    assert!(shows_part(&windows.a.harness, "Ziehen abgebrochen."));
    assert!(
        run_until(&mut windows, |w| workspace(&w.b).drag_mark().is_none()),
        "Esc nimmt die Marke in B weg"
    );
    assert!(!workspace(&windows.a).dragging_out());
    press_at(&mut windows.a.harness, to, false);
    run_until(&mut windows, |_| false);
    assert_eq!(rows(&windows.b), before_b, "nichts eingefügt");

    // Zwischen den Fenstern loslassen (kein Fenster darunter): nichts geschieht.
    let gap = egui::pos2(1450.0, 300.0);
    drag_from_a(&mut windows, "01.0010 Informationsschwerpunkt ASP01", gap);
    assert!(workspace(&windows.a).dragging_out());
    press_at(&mut windows.a.harness, gap, false);
    settle(&mut windows.a.harness);
    assert!(shows_part(
        &windows.a.harness,
        "Ziehen abgebrochen – kein proAVA-Fenster unter dem Zeiger"
    ));
    for _ in 0..30 {
        windows.b.harness.step();
    }
    assert_eq!(rows(&windows.b), before_b);
}

#[test]
fn a_read_only_window_or_an_open_dialog_takes_nothing() {
    let mut windows = two_windows("schreibgeschuetzt", true);
    let before_b = rows(&windows.b);
    let to = over_row_of_b(&windows, "01.0010 Automationsstation", true);
    drag_from_a(&mut windows, "01.0020 Schaltschrankleuchte LED", to);
    assert!(run_until(&mut windows, |w| shows_part(
        &w.b.harness,
        "schreibgeschützt geöffnet – hier wird nichts eingefügt"
    )));
    press_at(&mut windows.a.harness, to, false);
    assert!(run_until(&mut windows, |w| shows_part(
        &w.a.harness,
        "Rathaus 3: Das Projekt ist in diesem Fenster schreibgeschützt"
    )));
    assert_eq!(rows(&windows.b), before_b);

    // Offener Dialog in B (Befehlssuche): abgelehnt.
    let mut windows = two_windows("dialog", false);
    let before_b = rows(&windows.b);
    press_with(&mut windows.b.harness, Modifiers::COMMAND, Key::K);
    let to = over_row_of_b(&windows, "01.0010 Automationsstation", true);
    drag_from_a(&mut windows, "01.0020 Schaltschrankleuchte LED", to);
    run_until(&mut windows, |w| shows_part(&w.b.harness, "Dialog offen"));
    press_at(&mut windows.a.harness, to, false);
    assert!(run_until(&mut windows, |w| shows_part(
        &w.a.harness,
        "Dialog offen"
    )));
    assert_eq!(rows(&windows.b), before_b);
}

#[test]
fn dragging_between_windows_goes_without_the_assistant_setting() {
    let mut windows = two_windows("ohne-ki", false);
    let mut preferences = windows.b.launcher.preferences();
    preferences.ai_windows = false;
    windows.b.launcher.set_preferences(preferences).unwrap();
    let to = over_row_of_b(&windows, "01.0020 Raumfühler", true);
    drag_from_a(&mut windows, "01.0020 Schaltschrankleuchte LED", to);
    assert!(run_until(&mut windows, |w| workspace(&w.b)
        .drag_mark()
        .is_some()));
    press_at(&mut windows.a.harness, to, false);
    assert!(run_until(&mut windows, |w| rows(&w.b).len() == 4));
    assert_eq!(
        rows(&windows.b)[3],
        "Schaltschrankleuchte LED",
        "hinter Raumfühler"
    );
    assert!(
        !shows(&windows.b.harness, "KI verbunden"),
        "kein KI-Hinweis"
    );
    assert!(workspace(&windows.b).ai_marked_rows().is_empty());
}

#[test]
fn without_window_places_a_hint_names_strg_c_strg_v() {
    let bridge = TempDir::new("ziehen-wayland-bruecke");
    let folder = TempDir::new("ziehen-wayland");
    let file = save(&folder.0, &source_project());
    let mut a = pane(&bridge.0, &folder.0, StartFile::Project(file), PID_A, 0.0);
    // Like Wayland: the system does not tell where the window is.
    a.harness
        .input_mut()
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .inner_rect = None;
    let from = row_center(&a, "01.0020 Schaltschrankleuchte LED");
    a.harness.event(egui::Event::PointerMoved(from));
    a.harness.step();
    press_at(&mut a.harness, from, true);
    for step in 1..=20 {
        let to = from + egui::vec2(step as f32 * 60.0, 0.0);
        a.harness.event(egui::Event::PointerMoved(to));
        a.harness.step();
    }
    assert!(shows(&a.harness, "Zwischen Fenstern ziehen: Strg+C/Strg+V"));
    assert!(!a.harness.state().workspace().unwrap().dragging_out());
    press_at(&mut a.harness, from + egui::vec2(1200.0, 0.0), false);
}

#[test]
fn at_the_lower_edge_of_the_other_window_its_table_scrolls_along() {
    let mut windows = two_windows_with("scrollen", false, target_with(80));
    let shown = workspace(&windows.b).table_rows_shown();
    assert_eq!(shown.start, 0, "oben");
    // Over the row B shows only in part, at its lower table edge.
    let last = workspace(&windows.b).project().lv().rows()[shown.end].clone();
    let name = format!("{} {}", last.ordinal(), last.short_text());
    let near_edge = over_row_of_b(&windows, &name, false);
    drag_from_a(
        &mut windows,
        "01.0010 Informationsschwerpunkt ASP01",
        near_edge,
    );
    assert!(
        run_until(&mut windows, |w| workspace(&w.b).table_rows_shown().start
            > 5),
        "B scrollt mit: {:?}",
        workspace(&windows.b).table_rows_shown()
    );
    windows.a.harness.key_press(Key::Escape);
    windows.a.harness.step();
    press_at(&mut windows.a.harness, near_edge, false);
}
