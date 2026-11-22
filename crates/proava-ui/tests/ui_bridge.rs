//! The local bridge in the window: an assistant shows, marks, reads,
//! copies, searches and inserts rows over the channel of the window – the
//! commands run on the UI thread through the normal editing (selection,
//! Strg+V, Strg+Z).
//!
//! The channel is a real Unix socket, but in the test's temporary folder
//! (never `$XDG_RUNTIME_DIR`); requests come from a helper thread while the
//! test thread runs the frames. No window, no process is started.

#![cfg(unix)]

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::*;
use proava_bridge_protocol::{
    Adopt, Command, Insert, Mark, OpenProject, Read, Reply, Request, Rows, Search, Show, Take,
};
use proava_domain::{EditStatus, Lv, LvGroup, LvRow, OzMask, Project, ProjectName};
use proava_infrastructure::window_bridge::{ask, BridgeFolder, SocketChannel};
use proava_ui::{App, StartFile};

/// A PID for the window in the test (each window has a folder of its own,
/// so no other entry is ever tidied away).
const PID: u32 = 4_100_001;

struct Window {
    harness: UiHarness,
    socket: PathBuf,
    folder: PathBuf,
    launcher: std::sync::Arc<proava_application::Launcher>,
}

fn window_with(folder: &Path, start: Option<StartFile>) -> Window {
    proava_ui::viewer_copies::use_folder_on_this_thread(Some(folder.join("pdf-anzeige")));
    let bridge = BridgeFolder::at(folder.join("bruecke"));
    let channel = SocketChannel::in_folder(bridge.clone());
    let launcher = launcher(folder);
    let app = App::new(launcher.clone(), start)
        .with_bridge(Box::new(channel), PID)
        .expect("Brücke im Temp-Ordner");
    let mut harness = harness_of(app);
    settle(&mut harness);
    Window {
        harness,
        socket: bridge.socket_file(PID),
        folder: bridge.path().to_path_buf(),
        launcher,
    }
}

fn window(folder: &Path, project: &Project) -> Window {
    let file = save(folder, project);
    let window = window_with(folder, Some(StartFile::Project(file)));
    assert!(window.harness.state().project().is_some(), "Projekt offen");
    window
}

/// Sends `command` from a helper thread and runs frames until the answer is there.
fn request(window: &mut Window, command: Command) -> Reply {
    let pending = send(window, command);
    finish(window, pending)
}

fn send(
    window: &Window,
    command: Command,
) -> std::thread::JoinHandle<Result<proava_bridge_protocol::Response, String>> {
    let socket = window.socket.to_string_lossy().into_owned();
    std::thread::spawn(move || ask(&socket, &Request::new(7, command), Duration::from_secs(60)))
}

fn finish(
    window: &mut Window,
    pending: std::thread::JoinHandle<Result<proava_bridge_protocol::Response, String>>,
) -> Reply {
    for _ in 0..2000 {
        if pending.is_finished() {
            break;
        }
        window.harness.step();
        std::thread::sleep(Duration::from_millis(1));
    }
    let response = pending.join().unwrap().expect("Antwort");
    assert_eq!(response.id, 7);
    settle(&mut window.harness);
    response.antwort
}

fn rows_of(reply: &Reply) -> Vec<proava_bridge_protocol::RowInfo> {
    match reply {
        Reply::Zeilen(rows) => rows.zeilen.clone(),
        Reply::Kopie(copy) => copy.zeilen.clone(),
        Reply::Erledigt(done) => done.zeilen.clone(),
        other => panic!("unerwartet: {other:?}"),
    }
}

fn workspace(window: &Window) -> &proava_ui::WorkspaceScreen {
    window.harness.state().workspace().expect("Arbeitsbereich")
}

fn ordinals(lv: &Lv, positions: usize) -> Vec<String> {
    lv.rows()
        .iter()
        .filter(|row| row.is_position())
        .take(positions)
        .map(|row| row.ordinal().to_string())
        .collect()
}

fn target_project(name: &str) -> Project {
    let mut lv = Lv::new(
        "Ziel",
        OzMask::parse(OzMask::DEFAULT_PATTERN).expect("Maske"),
    );
    lv.push_row(LvRow::Group(LvGroup::new(0, "01", "Schaltschränke")));
    Project::new(ProjectName::parse(name).unwrap(), "2026-11-18T10:00:00", lv)
}

#[test]
fn the_window_tells_its_state_and_announces_itself() {
    let folder = TempDir::new("bruecke-lesen");
    let mut window = window(&folder.0, &proava_ui::demo::demo_project());
    let Reply::Fenster(info) = request(&mut window, Command::FensterLesen) else {
        panic!("Fensterangaben erwartet");
    };
    assert_eq!(info.pid, PID);
    assert!(info.ki_erlaubt);
    let project = info.projekt.expect("Projekt");
    assert_eq!(project.name, workspace(&window).project().name().as_str());
    assert!(!project.ungespeichert);
    assert_eq!(project.reiter, "LV");
    assert_eq!(project.auswahl.len(), 1, "die gewählte Zeile");
    // The entry in the folder (written within half a second of frames).
    for _ in 0..60 {
        window.harness.step();
    }
    let entries = BridgeFolder::at(&window.folder).entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].pid, PID);
    assert_eq!(entries[0].kanal, window.socket.to_string_lossy());
    assert_eq!(
        entries[0].fenster.projekt.as_ref().map(|p| p.name.clone()),
        Some(project.name)
    );
}

#[test]
fn showing_selects_marks_and_esc_stops_the_assistant() {
    let folder = TempDir::new("bruecke-zeigen");
    let mut window = window(&folder.0, &proava_ui::demo::demo_project());
    let first = ordinals(workspace(&window).project().lv(), 1);
    let reply = request(
        &mut window,
        Command::Zeigen(Show {
            zeilen: Rows {
                oz: first.clone(),
                weitere: 2,
                ..Rows::default()
            },
            positionsfenster: true,
            markierung: Some("Vorschlag Automationsstation".into()),
        }),
    );
    let rows = rows_of(&reply);
    assert_eq!(rows.len(), 3, "die genannte und die nächsten zwei");
    assert_eq!(rows[0].oz, first[0]);
    let ids: std::collections::HashSet<_> = rows
        .iter()
        .map(|row| proava_domain::RowId(row.id))
        .collect();
    assert_eq!(
        workspace(&window).selection_ids(),
        ids,
        "im Fenster gewählt"
    );
    assert_eq!(workspace(&window).ai_marked_rows(), ids, "KI-Markierung");
    assert!(
        workspace(&window).position_window_row().is_some(),
        "Positionsfenster"
    );
    assert!(shows_part(
        &window.harness,
        "KI: Vorschlag Automationsstation"
    ));
    assert!(shows(&window.harness, "KI verbunden"));

    // Esc: Markierung weg, die nächste Anfrage wird abgelehnt.
    press(&mut window.harness, Key::Escape);
    press(&mut window.harness, Key::Escape);
    assert!(workspace(&window).ai_marked_rows().is_empty());
    let reply = request(
        &mut window,
        Command::Zeigen(Show {
            zeilen: Rows::selection(),
            ..Show::default()
        }),
    );
    assert!(
        reply.refusal().unwrap().contains("mit Esc abgebrochen"),
        "{reply:?}"
    );
}

#[test]
fn marking_keeps_the_selection_and_can_be_taken_away() {
    let folder = TempDir::new("bruecke-markieren");
    let mut window = window(&folder.0, &proava_ui::demo::demo_project());
    let before = workspace(&window).selection_ids();
    let last = {
        let lv = workspace(&window).project().lv();
        lv.rows()
            .iter()
            .rev()
            .find(|row| row.is_position())
            .unwrap()
            .ordinal()
            .to_string()
    };
    let reply = request(
        &mut window,
        Command::Markieren(Mark {
            zeilen: Rows {
                oz: vec![last.clone()],
                ..Rows::default()
            },
            text: "prüft die letzte Position".into(),
            sekunden: Some(30),
            aufheben: false,
        }),
    );
    assert_eq!(rows_of(&reply)[0].oz, last);
    assert_eq!(workspace(&window).selection_ids(), before, "Auswahl bleibt");
    assert_eq!(workspace(&window).ai_marked_rows().len(), 1);
    assert!(shows_part(&window.harness, "KI: prüft die letzte Position"));
    let reply = request(
        &mut window,
        Command::Markieren(Mark {
            aufheben: true,
            ..Mark::default()
        }),
    );
    assert!(matches!(reply, Reply::Erledigt(_)));
    assert!(workspace(&window).ai_marked_rows().is_empty());
    assert!(!shows_part(&window.harness, "KI: prüft"));
}

#[test]
fn reading_and_searching_in_the_window() {
    let folder = TempDir::new("bruecke-suchen");
    let mut window = window(&folder.0, &proava_ui::demo::demo_project());
    let reply = request(
        &mut window,
        Command::Lesen(Read {
            ab: Some("01".into()),
            anzahl: Some(4),
            langtext: true,
            ..Read::default()
        }),
    );
    let rows = rows_of(&reply);
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0].art, "Titel");
    assert!(rows.iter().any(|row| row.langtext.is_some()));

    let reply = request(
        &mut window,
        Command::Suchen(Search {
            suche: "automationsstation".into(),
            ..Search::default()
        }),
    );
    let Reply::Zeilen(found) = &reply else {
        panic!("{reply:?}")
    };
    assert!(found.gesamt > 0, "{reply:?}");
    assert_eq!(
        workspace(&window).ai_marked_rows().len(),
        found.gesamt,
        "alle Treffer markiert"
    );
    assert_eq!(workspace(&window).selection_ids().len(), found.gesamt);
    assert!(shows_part(&window.harness, "für „automationsstation“"));

    let reply = request(
        &mut window,
        Command::Suchen(Search {
            suche: "gibtesnicht".into(),
            ..Search::default()
        }),
    );
    assert_eq!(rows_of(&reply).len(), 0);
    let reply = request(&mut window, Command::Suchen(Search::default()));
    assert!(reply.refusal().is_some());
}

/// The way of the user's two windows: copy in one, insert at the cursor of
/// the other – one Strg+Z there takes it back.
#[test]
fn rows_of_one_window_go_into_the_other_and_back_with_undo() {
    let (folder_a, folder_b) = (TempDir::new("bruecke-a"), TempDir::new("bruecke-b"));
    let mut source = window(&folder_a.0, &proava_ui::demo::demo_project());
    let mut target = window(&folder_b.0, &target_project("Rathaus 3"));
    let wanted = ordinals(workspace(&source).project().lv(), 1);

    let copy = request(
        &mut source,
        Command::Kopieren(Take {
            zeilen: Rows {
                oz: wanted.clone(),
                weitere: 1,
                ..Rows::default()
            },
            mengen: true,
            preise: false,
            markierung: Some("kopiert nach Rathaus 3".into()),
        }),
    );
    let Reply::Kopie(copy) = copy else {
        panic!("{copy:?}")
    };
    assert_eq!(copy.zeilen.len(), 2);
    assert!(copy.daten.starts_with("proAVA-Zwischenablage|1|zeilen|"));
    assert_eq!(workspace(&source).ai_marked_rows().len(), 2);
    assert!(!workspace(&source).is_dirty(), "Kopieren ändert nichts");

    let before = workspace(&target).project().lv().rows().len();
    let reply = request(
        &mut target,
        Command::Einfuegen(Insert {
            daten: copy.daten.clone(),
            nach: Some("01".into()),
            lv: None,
            herkunft: "Demo (Fenster rechts)".into(),
        }),
    );
    let Reply::Erledigt(done) = &reply else {
        panic!("{reply:?}")
    };
    assert!(!done.wartet);
    assert_eq!(done.zeilen.len(), 2);
    let lv = workspace(&target).project().lv();
    assert_eq!(lv.rows().len(), before + 2);
    let texts: Vec<String> = done.zeilen.iter().map(|row| row.kurztext.clone()).collect();
    assert_eq!(
        texts,
        copy.zeilen
            .iter()
            .map(|row| row.kurztext.clone())
            .collect::<Vec<_>>()
    );
    assert!(
        lv.rows()
            .iter()
            .filter_map(LvRow::as_position)
            .all(|position| position.unit_price().is_none()),
        "ohne Preise"
    );
    assert!(workspace(&target).is_dirty());
    assert_eq!(workspace(&target).ai_marked_rows().len(), 2);
    assert!(shows_part(
        &target.harness,
        "aus Demo (Fenster rechts) eingefügt"
    ));

    // Strg+Z im Zielfenster: ein Schritt, alles weg, Markierung auch.
    press_with(&mut target.harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(workspace(&target).project().lv().rows().len(), before);
    assert!(workspace(&target).ai_marked_rows().is_empty());
}

/// One demo position goes over the bridge into `project`: the question
/// `title` (with the texts `parts`) waits for the user, „Abbrechen“ takes it
/// back and the answer says so, `keep` inserts it.
fn bridge_insert_asks(name: &str, project: &Project, title: &str, parts: &[&str], keep: &str) {
    let folder = TempDir::new(name);
    let source_folder = TempDir::new("bruecke-quelle");
    let mut source = window(&source_folder.0, &proava_ui::demo::demo_project());
    let one = ordinals(workspace(&source).project().lv(), 1);
    let copy = request(
        &mut source,
        Command::Kopieren(Take {
            zeilen: Rows {
                oz: one,
                ..Rows::default()
            },
            ..Take::default()
        }),
    );
    let Reply::Kopie(copy) = copy else {
        panic!("{copy:?}")
    };
    let mut target = window(&folder.0, project);
    let first = ordinals(workspace(&target).project().lv(), 1);
    let insert = Command::Einfuegen(Insert {
        daten: copy.daten.clone(),
        nach: Some(first[0].clone()),
        ..Insert::default()
    });

    // „Abbrechen“ in der Rückfrage nimmt es zurück – die Antwort sagt es.
    let rows = workspace(&target).project().lv().rows().len();
    let pending = send(&target, insert.clone());
    for _ in 0..30 {
        target.harness.step();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(shows(&target.harness, title), "Rückfrage {title}");
    for part in parts {
        assert!(shows_part(&target.harness, part), "Text: {part}");
    }
    assert!(!pending.is_finished(), "wartet auf den Anwender");
    assert!(shows_part(&target.harness, "KI: wartet auf den Anwender"));
    click(&mut target.harness, "Abbrechen");
    let reply = finish(&mut target, pending);
    assert!(
        reply.refusal().unwrap().contains("zurückgenommen"),
        "{reply:?}"
    );
    assert_eq!(workspace(&target).project().lv().rows().len(), rows);

    // `keep`: eingefügt.
    let pending = send(&target, insert);
    for _ in 0..30 {
        target.harness.step();
        std::thread::sleep(Duration::from_millis(1));
    }
    click(&mut target.harness, keep);
    let reply = finish(&mut target, pending);
    let Reply::Erledigt(done) = &reply else {
        panic!("{reply:?}")
    };
    assert!(!done.wartet);
    assert_eq!(workspace(&target).project().lv().rows().len(), rows + 1);
}

#[test]
fn a_sent_lv_asks_the_user_and_the_answer_waits() {
    // Without bids: only the question after sending.
    let mut project = proava_ui::demo::demo_project();
    let bids: Vec<_> = project.bids().iter().map(|bid| bid.id()).collect();
    for bid in bids {
        project.remove_bid(bid);
    }
    project
        .lv_mut()
        .mark_sent("2026-11-18T09:00:00".into(), None);
    bridge_insert_asks(
        "bruecke-versandt",
        &project,
        "LV bereits versandt",
        &["Das LV wurde am 18.11.2026 versandt. Die OZ ab"],
        "Trotzdem ändern",
    );
}

#[test]
fn an_lv_with_bids_asks_the_user_also_when_it_was_not_sent_from_proava() {
    // The demo has four bids read in, no X83 export (Anwender 19.11.).
    bridge_insert_asks(
        "bruecke-angebote",
        &proava_ui::demo::demo_project(),
        "LV hat bereits Angebote",
        &["Das LV hat bereits 4 Angebote – die neue Position fehlt dort. Trotzdem einfügen?"],
        "Trotzdem einfügen",
    );
}

#[test]
fn a_sent_lv_with_bids_asks_once_with_both_reasons() {
    let mut project = proava_ui::demo::demo_project();
    project
        .lv_mut()
        .mark_sent("2026-11-18T09:00:00".into(), None);
    bridge_insert_asks(
        "bruecke-versandt-angebote",
        &project,
        "LV hat bereits Angebote",
        &[
            "Das LV hat bereits 4 Angebote – die neue Position fehlt dort.",
            "Das LV wurde am 18.11.2026 versandt. Die OZ ab",
            "Trotzdem einfügen?",
        ],
        "Trotzdem einfügen",
    );
}

#[test]
fn finished_positions_and_read_only_windows_behave_as_by_hand() {
    let folder = TempDir::new("bruecke-gesperrt");
    let mut project = proava_ui::demo::demo_project();
    let index = project
        .lv()
        .rows()
        .iter()
        .position(LvRow::is_position)
        .unwrap();
    let ordinal = project.lv().rows()[index].ordinal().to_string();
    if let Some(LvRow::Position(position)) = project.lv_mut().row_mut(index) {
        position.set_status(EditStatus::Done);
    }
    let mut window = window(&folder.0, &project);
    let copy = request(
        &mut window,
        Command::Kopieren(Take {
            zeilen: Rows {
                oz: vec![ordinal.clone()],
                ..Rows::default()
            },
            ..Take::default()
        }),
    );
    let Reply::Kopie(copy) = copy else {
        panic!("{copy:?}")
    };
    assert_eq!(copy.zeilen[0].status, "fertig");
    // Hinter einer fertigen Position einfügen geht wie Strg+V; sie selbst bleibt gesperrt.
    let reply = request(
        &mut window,
        Command::Einfuegen(Insert {
            daten: copy.daten.clone(),
            nach: Some(ordinal.clone()),
            ..Insert::default()
        }),
    );
    let inserted = rows_of(&reply);
    assert_eq!(inserted.len(), 1);
    assert_eq!(inserted[0].status, "", "die Kopie ist offen");
    let lv = workspace(&window).project().lv();
    let locked = lv
        .rows()
        .iter()
        .find(|row| row.ordinal() == ordinal)
        .unwrap();
    assert_eq!(
        locked.as_position().unwrap().status(),
        EditStatus::Done,
        "gesperrte Position unverändert"
    );

    // Schreibgeschützt geöffnet: Einfügen abgelehnt, Zeigen geht.
    let other = TempDir::new("bruecke-schreibgeschuetzt");
    let file = save(&other.0, &proava_ui::demo::demo_project());
    let mut read_only = window_with(&other.0, Some(StartFile::ProjectReadOnly(file)));
    let reply = request(
        &mut read_only,
        Command::Einfuegen(Insert {
            daten: copy.daten,
            ..Insert::default()
        }),
    );
    assert!(
        reply.refusal().unwrap().contains("schreibgeschützt"),
        "{reply:?}"
    );
    let reply = request(
        &mut read_only,
        Command::Zeigen(Show {
            zeilen: Rows::selection(),
            ..Show::default()
        }),
    );
    assert_eq!(rows_of(&reply).len(), 1);
}

#[test]
fn switched_off_the_window_only_answers_with_a_hint() {
    let folder = TempDir::new("bruecke-aus");
    let mut window = window(&folder.0, &proava_ui::demo::demo_project());
    let mut preferences = window.launcher.preferences();
    preferences.ai_windows = false;
    window.launcher.set_preferences(preferences).unwrap();
    for command in [
        Command::FensterLesen,
        Command::Zeigen(Show {
            zeilen: Rows::selection(),
            ..Show::default()
        }),
        Command::Stand,
        Command::Speichern,
    ] {
        let reply = request(&mut window, command);
        let Reply::Aus { text } = &reply else {
            panic!("{reply:?}")
        };
        assert!(text.contains("ausgeschaltet"), "{text}");
    }
    assert!(workspace(&window).ai_marked_rows().is_empty());
    assert!(!shows(&window.harness, "KI verbunden"));
    // The entry stays (the file tools must know about unsaved changes), without the selection.
    for _ in 0..60 {
        window.harness.step();
    }
    let entry = BridgeFolder::at(&window.folder).entries().remove(0);
    assert!(!entry.fenster.ki_erlaubt);
    let project = entry.fenster.projekt.expect("Projekt");
    assert!(project.auswahl.is_empty());
}

#[test]
fn a_project_opens_in_the_start_window_or_asks_for_a_new_window() {
    let folder = TempDir::new("bruecke-oeffnen");
    let file = save(&folder.0, &proava_ui::demo::demo_project());
    let mut start = window_with(&folder.0, None);
    assert!(start.harness.state().project().is_none());
    let reply = request(
        &mut start,
        Command::ProjektOeffnen(OpenProject {
            pfad: file.to_string_lossy().into_owned(),
            schreibgeschuetzt: false,
        }),
    );
    assert!(matches!(reply, Reply::Erledigt(_)), "{reply:?}");
    settle(&mut start.harness);
    assert!(
        start.harness.state().project().is_some(),
        "im Startfenster geöffnet"
    );

    // Ein anderes Projekt aus einem offenen Fenster: neues Fenster – im Test
    // gesperrt (kein Prozessstart), aber der Weg wird erreicht.
    let other = save(&folder.0, &target_project("Rathaus 3"));
    let refused = proava_ui::jobs::refused_starts();
    let reply = request(
        &mut start,
        Command::ProjektOeffnen(OpenProject {
            pfad: other.to_string_lossy().into_owned(),
            schreibgeschuetzt: true,
        }),
    );
    assert!(matches!(reply, Reply::Erledigt(_)), "{reply:?}");
    assert_eq!(
        proava_ui::jobs::refused_starts(),
        refused + 1,
        "kein Prozess gestartet"
    );
    let reply = request(
        &mut start,
        Command::ProjektOeffnen(OpenProject {
            pfad: "relativ.proava".into(),
            schreibgeschuetzt: false,
        }),
    );
    assert!(reply.refusal().unwrap().contains("absoluten Pfad"));
    // Startseite ohne Projekt lehnt Zeilenbefehle ab.
    let empty_folder = TempDir::new("bruecke-leer");
    let mut empty = window_with(&empty_folder.0, None);
    let reply = request(
        &mut empty,
        Command::Zeigen(Show {
            zeilen: Rows::selection(),
            ..Show::default()
        }),
    );
    assert!(reply.refusal().unwrap().contains("Startseite"));
}

#[test]
fn an_open_window_eigenschaften_follows_the_rows_the_assistant_shows() {
    let folder = TempDir::new("bruecke-folgen");
    let mut window = window(&folder.0, &proava_ui::demo::demo_project());
    let ordinals = ordinals(workspace(&window).project().lv(), 3);
    let row_of = |window: &Window, oz: &str| {
        workspace(window)
            .project()
            .lv()
            .rows()
            .iter()
            .find(|row| row.ordinal() == oz)
            .unwrap()
            .id()
    };
    select_first_position(&mut window.harness);
    press_with(&mut window.harness, Modifiers::ALT, Key::Enter);
    assert_eq!(
        workspace(&window).position_window_row(),
        Some(row_of(&window, &ordinals[0]))
    );
    let show = |oz: &str| {
        Command::Zeigen(Show {
            zeilen: Rows {
                oz: vec![oz.to_string()],
                ..Rows::default()
            },
            ..Show::default()
        })
    };
    request(&mut window, show(&ordinals[2]));
    assert_eq!(
        workspace(&window).position_window_row(),
        Some(row_of(&window, &ordinals[2])),
        "das Fenster folgt der gezeigten Zeile"
    );
    // Festgehalten bleibt es stehen.
    click(&mut window.harness, "Festhalten");
    request(&mut window, show(&ordinals[1]));
    assert_eq!(
        workspace(&window).position_window_row(),
        Some(row_of(&window, &ordinals[2])),
        "festgehalten"
    );
}

/// Two LVs one below the other: the window tells, reads and inserts in the
/// LV of the Schreibmarke – the other LV stays as it is.
#[test]
fn with_several_lvs_reading_and_inserting_go_to_the_lv_of_the_cursor() {
    use proava_domain::{Decimal, Position};
    let (folder_a, folder_b) = (
        TempDir::new("bruecke-mehrlv-a"),
        TempDir::new("bruecke-mehrlv-b"),
    );
    let mut source = window(&folder_a.0, &proava_ui::demo::demo_project());
    let mut two = target_project("Zwei LV");
    two.lv_mut().set_number("1");
    two.lv_mut().set_title("Gebäudeautomation");
    two.lv_mut()
        .push_position(Position::new("", "DDC-Station", Decimal::ONE, "St", None));
    two.lv_mut().renumber();
    let mut second = Lv::new(
        "Elektro",
        OzMask::parse(OzMask::DEFAULT_PATTERN).expect("Maske"),
    );
    second.set_number("2");
    second.push_row(LvRow::Group(LvGroup::new(0, "01", "Leitungen")));
    second.push_position(Position::new("", "Mantelleitung", Decimal::TEN, "m", None));
    second.push_position(Position::new("", "Schalter", Decimal::TWO, "St", None));
    second.renumber();
    two.add_lv(second);
    let mut target = window(&folder_b.0, &two);

    // The Schreibmarke into LV 2.
    let mantel = workspace(&target)
        .project()
        .lv_at(1)
        .unwrap()
        .positions()
        .next()
        .unwrap()
        .ordinal()
        .to_string();
    target
        .harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::Row
                && node.label().as_deref() == Some(format!("{mantel} Mantelleitung").as_str())
        })
        .next()
        .expect("Zeile Mantelleitung")
        .click();
    settle(&mut target.harness);
    assert_eq!(workspace(&target).project().active_lv(), 1);

    let Reply::Fenster(info) = request(&mut target, Command::FensterLesen) else {
        panic!("Fensterangaben erwartet");
    };
    let project = info.projekt.expect("Projekt");
    assert_eq!(project.lv.nummer, "2", "das LV der Schreibmarke");
    assert_eq!(project.lvs.len(), 2);
    assert_eq!(project.auswahl[0].kurztext, "Mantelleitung");
    let read = request(&mut target, Command::Lesen(Read::default()));
    let rows = rows_of(&read);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kurztext, "Mantelleitung");

    // Rows of the other window land after the Schreibmarke in LV 2.
    let wanted = ordinals(workspace(&source).project().lv(), 1);
    let Reply::Kopie(copy) = request(
        &mut source,
        Command::Kopieren(Take {
            zeilen: Rows {
                oz: wanted,
                ..Rows::default()
            },
            mengen: true,
            preise: false,
            markierung: None,
        }),
    ) else {
        panic!("Kopie erwartet")
    };
    let first_lv = workspace(&target).project().lv_at(0).unwrap().clone();
    let before = workspace(&target).project().lv_at(1).unwrap().rows().len();
    let reply = request(
        &mut target,
        Command::Einfuegen(Insert {
            daten: copy.daten.clone(),
            nach: None,
            lv: None,
            herkunft: "Demo".into(),
        }),
    );
    let Reply::Erledigt(done) = &reply else {
        panic!("{reply:?}")
    };
    assert!(!done.wartet);
    let shown = workspace(&target).project();
    assert_eq!(shown.active_lv(), 1);
    let lv = shown.lv_at(1).unwrap();
    assert_eq!(lv.rows().len(), before + done.zeilen.len());
    let texts: Vec<&str> = lv.positions().map(|p| p.short_text()).collect();
    assert_eq!(texts.first(), Some(&"Mantelleitung"));
    assert_eq!(
        texts.last(),
        Some(&"Schalter"),
        "nach der Schreibmarke, vor Schalter"
    );
    assert!(texts.contains(&copy.zeilen[0].kurztext.as_str()));
    assert_eq!(*shown.lv_at(0).unwrap(), first_lv, "LV 1 unverändert");

    press_with(&mut target.harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(
        workspace(&target).project().lv_at(1).unwrap().rows().len(),
        before
    );
}

// ----- Jedes MCP-Werkzeug im offenen Fenster (stand / uebernehmen / speichern) -----

/// What an MCP tool does: reads the window's state, changes it, hands it
/// back. Returns the pending answer of `uebernehmen`.
fn change_in_window(
    window: &mut Window,
    tool: &str,
    change: impl FnOnce(&mut Project),
) -> std::thread::JoinHandle<Result<proava_bridge_protocol::Response, String>> {
    let reply = request(window, Command::Stand);
    let Reply::Stand(state) = reply else {
        panic!("{reply:?}")
    };
    let read = PathBuf::from(&state.datei);
    assert!(
        read.starts_with(window.folder.join("austausch")),
        "{read:?}"
    );
    let (mut project, _) = window.launcher.read_source(&read).unwrap();
    std::fs::remove_file(&read).unwrap();
    change(&mut project);
    let file = read.with_file_name(format!("mcp-{tool}.proava"));
    window.launcher.save_project(&file, &project).unwrap();
    send(
        window,
        Command::Uebernehmen(Adopt {
            datei: file.to_string_lossy().into_owned(),
            stand: state.stand,
            werkzeug: tool.into(),
        }),
    )
}

fn without_bids(mut project: Project) -> Project {
    let bids: Vec<_> = project.bids().iter().map(|bid| bid.id()).collect();
    for bid in bids {
        project.remove_bid(bid);
    }
    project
}

fn first_position(project: &Project) -> (usize, proava_domain::RowId) {
    let index = project
        .lv()
        .rows()
        .iter()
        .position(LvRow::is_position)
        .unwrap();
    (index, project.lv().rows()[index].id())
}

fn rename(project: &mut Project, index: usize, text: &str) {
    if let Some(LvRow::Position(position)) = project.lv_mut().row_mut(index) {
        position.set_short_text(text);
    }
}

#[test]
fn an_mcp_change_is_one_undo_step_marked_and_not_saved() {
    let folder = TempDir::new("bruecke-uebernehmen");
    let mut window = window(&folder.0, &without_bids(proava_ui::demo::demo_project()));
    let file = workspace(&window).project_file().to_path_buf();
    let on_disk = std::fs::read(&file).unwrap();
    let original = workspace(&window).project().clone();
    let (index, id) = first_position(&original);

    // Read and handed back unchanged: nothing to do (the file round trip
    // adds no difference).
    let pending = change_in_window(&mut window, "lv_lesen", |_| {});
    let reply = finish(&mut window, pending);
    let Reply::Erledigt(done) = &reply else {
        panic!("{reply:?}")
    };
    assert!(done.text.contains("nichts zu ändern"), "{}", done.text);
    assert!(!workspace(&window).is_dirty());

    let pending = change_in_window(&mut window, "zeile_aendern", |project| {
        rename(project, index, "Geändert durch die KI")
    });
    let reply = finish(&mut window, pending);
    let Reply::Erledigt(done) = &reply else {
        panic!("{reply:?}")
    };
    assert!(!done.wartet);
    assert!(done.text.contains("Im Fenster übernommen"), "{}", done.text);
    assert!(done.text.contains("1 Zeile geändert"), "{}", done.text);
    assert!(done.text.contains("fenster_speichern"), "{}", done.text);
    assert_eq!(done.zeilen.len(), 1);
    let screen = workspace(&window);
    assert_eq!(
        screen.project().lv().rows()[index].short_text(),
        "Geändert durch die KI"
    );
    assert!(screen.ai_marked_rows().contains(&id), "markiert");
    assert!(screen.is_dirty(), "ungespeichert wie nach Handarbeit");
    assert_eq!(std::fs::read(&file).unwrap(), on_disk, "Datei unberührt");
    assert!(shows_part(
        &window.harness,
        "KI: zeile_aendern: 1 Zeile geändert"
    ));

    // Strg+Z nimmt den ganzen Aufruf zurück, Strg+Y bringt ihn wieder.
    press_with(&mut window.harness, Modifiers::COMMAND, Key::Z);
    assert!(workspace(&window).project().same_content(&original));
    assert!(shows_part(&window.harness, "Rückgängig: Änderung der KI"));
    assert!(
        workspace(&window).ai_marked_rows().is_empty(),
        "zurückgenommen: die Markierung geht mit"
    );
    assert!(!shows_part(&window.harness, "KI: zeile_aendern"));
    press_with(&mut window.harness, Modifiers::COMMAND, Key::Y);
    assert_eq!(
        workspace(&window).project().lv().rows()[index].short_text(),
        "Geändert durch die KI"
    );

    // fenster_speichern: wie Strg+S.
    let reply = request(&mut window, Command::Speichern);
    let Reply::Erledigt(done) = &reply else {
        panic!("{reply:?}")
    };
    assert!(done.text.starts_with("Gespeichert: "), "{}", done.text);
    assert!(!workspace(&window).is_dirty());
    let saved = window.launcher.read_source(&file).unwrap().0;
    assert_eq!(
        saved.lv().rows()[index].short_text(),
        "Geändert durch die KI"
    );
}

#[test]
fn a_change_on_an_older_state_is_refused() {
    let folder = TempDir::new("bruecke-alter-stand");
    let mut window = window(&folder.0, &without_bids(proava_ui::demo::demo_project()));
    let (index, _) = first_position(workspace(&window).project());

    // The user changes something after the tool read the state.
    let reply = request(&mut window, Command::Stand);
    let Reply::Stand(state) = reply else {
        panic!("{reply:?}")
    };
    let (mut project, _) = window
        .launcher
        .read_source(Path::new(&state.datei))
        .unwrap();
    std::fs::remove_file(&state.datei).unwrap();
    let one = ordinals(workspace(&window).project().lv(), 1);
    let copy = request(
        &mut window,
        Command::Kopieren(Take {
            zeilen: Rows {
                oz: one,
                ..Rows::default()
            },
            ..Take::default()
        }),
    );
    let Reply::Kopie(copy) = copy else {
        panic!("{copy:?}")
    };
    let rows = workspace(&window).project().lv().rows().len();
    request(
        &mut window,
        Command::Einfuegen(Insert {
            daten: copy.daten,
            ..Insert::default()
        }),
    );
    assert_eq!(workspace(&window).project().lv().rows().len(), rows + 1);
    rename(&mut project, index, "Alter Stand");
    let file = Path::new(&state.datei).with_file_name("mcp-alt.proava");
    window.launcher.save_project(&file, &project).unwrap();
    let reply = request(
        &mut window,
        Command::Uebernehmen(Adopt {
            datei: file.to_string_lossy().into_owned(),
            stand: state.stand,
            werkzeug: "zeile_aendern".into(),
        }),
    );
    assert!(
        reply.refusal().unwrap().contains("inzwischen geändert"),
        "{reply:?}"
    );
    assert_eq!(workspace(&window).project().lv().rows().len(), rows + 1);
    assert_ne!(
        workspace(&window).project().lv().rows()[index].short_text(),
        "Alter Stand"
    );

    // Only files of the exchange folder.
    let elsewhere = folder.0.join("woanders.proava");
    window.launcher.save_project(&elsewhere, &project).unwrap();
    let reply = request(&mut window, Command::Stand);
    let Reply::Stand(state) = reply else {
        panic!("{reply:?}")
    };
    std::fs::remove_file(&state.datei).unwrap();
    let reply = request(
        &mut window,
        Command::Uebernehmen(Adopt {
            datei: elsewhere.to_string_lossy().into_owned(),
            stand: state.stand,
            werkzeug: "zeile_aendern".into(),
        }),
    );
    assert!(
        reply.refusal().unwrap().contains("Austauschordner"),
        "{reply:?}"
    );
}

#[test]
fn an_mcp_change_in_an_lv_with_bids_asks_the_user() {
    let folder = TempDir::new("bruecke-uebernehmen-angebote");
    let original = proava_ui::demo::demo_project();
    let mut window = window(&folder.0, &original);
    let add = |project: &mut Project| {
        let (index, _) = first_position(project);
        let mut row = project.lv().rows()[index].clone();
        if let LvRow::Position(position) = &mut row {
            position.set_short_text("Neue Position der KI");
        }
        project.lv_mut().insert_row(index + 1, row);
    };

    // „Abbrechen“ nimmt es zurück – die Antwort sagt es.
    let rows = workspace(&window).project().lv().rows().len();
    let pending = change_in_window(&mut window, "position_einfuegen", add);
    for _ in 0..30 {
        window.harness.step();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(shows(&window.harness, "LV hat bereits Angebote"));
    assert!(!pending.is_finished(), "wartet auf den Anwender");
    assert!(shows_part(&window.harness, "KI: wartet auf den Anwender"));
    click(&mut window.harness, "Abbrechen");
    let reply = finish(&mut window, pending);
    assert!(
        reply.refusal().unwrap().contains("zurückgenommen"),
        "{reply:?}"
    );
    assert_eq!(workspace(&window).project().lv().rows().len(), rows);

    // „Trotzdem einfügen“: übernommen.
    let pending = change_in_window(&mut window, "position_einfuegen", add);
    for _ in 0..30 {
        window.harness.step();
        std::thread::sleep(Duration::from_millis(1));
    }
    click(&mut window.harness, "Trotzdem einfügen");
    let reply = finish(&mut window, pending);
    let Reply::Erledigt(done) = &reply else {
        panic!("{reply:?}")
    };
    assert!(!done.wartet);
    assert!(done.text.contains("1 Zeile neu"), "{}", done.text);
    assert_eq!(workspace(&window).project().lv().rows().len(), rows + 1);
}

#[test]
fn a_read_only_window_gives_its_state_but_takes_no_change() {
    let folder = TempDir::new("bruecke-uebernehmen-schreibgeschuetzt");
    let file = save(&folder.0, &proava_ui::demo::demo_project());
    let mut window = window_with(&folder.0, Some(StartFile::ProjectReadOnly(file)));
    let pending = change_in_window(&mut window, "zeile_aendern", |project| {
        let (index, _) = first_position(project);
        rename(project, index, "Nicht erlaubt");
    });
    let reply = finish(&mut window, pending);
    assert!(
        reply.refusal().unwrap().contains("schreibgeschützt"),
        "{reply:?}"
    );
    let reply = request(&mut window, Command::Speichern);
    assert!(
        reply.refusal().unwrap().contains("Schreibgeschützt"),
        "{reply:?}"
    );
}
