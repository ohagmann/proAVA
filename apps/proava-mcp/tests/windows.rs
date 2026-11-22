//! The window tools against a recording stand-in of the open windows: no
//! window, no socket, no process – the stand-in answers like a window and
//! keeps every request, so the tests see what the server asks in which order.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use proava_bridge_protocol::{
    ClipReply, Command, Done, Geometry, LvInfo, ProjectInfo, Reply, RowInfo, RowsReply,
    WindowEntry, WindowInfo,
};
use proava_infrastructure::XmlProjectFile;
use proava_mcp::{Server, WindowLink};
use serde_json::{json, Value};

/// A folder only this test uses: a unique name, created exclusively (two
/// tests – also of other test runs at the same time – never share one); it is
/// removed with this value, nothing else.
struct TempFolder(PathBuf, #[allow(dead_code)] tempfile::TempDir);

impl TempFolder {
    fn new(name: &str) -> Self {
        let dir = tempfile::Builder::new()
            .prefix(&format!("proava-mcp-fenster-{name}-"))
            .tempdir()
            .unwrap();
        Self(proava_mcp::real_path(dir.path()).unwrap(), dir)
    }
}

/// The open windows as the test sets them, and every request (PID, command).
#[derive(Clone, Default)]
struct Recording {
    windows: Arc<Mutex<Vec<WindowEntry>>>,
    asked: Arc<Mutex<Vec<(u32, Command)>>>,
    /// Window that appears after `projekt_oeffnen` (a new window).
    appears: Arc<Mutex<Option<WindowEntry>>>,
}

impl Recording {
    fn commands(&self) -> Vec<(u32, Command)> {
        self.asked.lock().unwrap().clone()
    }

    fn without_reads(&self) -> Vec<(u32, Command)> {
        self.commands()
            .into_iter()
            .filter(|(_, command)| *command != Command::FensterLesen)
            .collect()
    }
}

fn row(id: u64, oz: &str, text: &str) -> RowInfo {
    RowInfo {
        id,
        oz: oz.into(),
        art: "Position".into(),
        kurztext: text.into(),
        menge: "1".into(),
        einheit: "St".into(),
        ..RowInfo::default()
    }
}

impl WindowLink for Recording {
    fn windows(&self) -> Vec<WindowEntry> {
        self.windows.lock().unwrap().clone()
    }

    fn ask(
        &self,
        window: &WindowEntry,
        command: Command,
        _timeout: Duration,
    ) -> Result<Reply, String> {
        self.asked
            .lock()
            .unwrap()
            .push((window.pid, command.clone()));
        Ok(match command {
            Command::FensterLesen => Reply::Fenster(window.fenster.clone()),
            Command::Zeigen(_) | Command::Markieren(_) | Command::Lesen(_) | Command::Suchen(_) => {
                Reply::Zeilen(RowsReply {
                    lv: Some(LvInfo {
                        stelle: 1,
                        nummer: "1".into(),
                        titel: "GA".into(),
                        positionen: 3,
                    }),
                    zeilen: vec![row(10, "01.0010", "Schaltschrank ASP01")],
                    gesamt: 1,
                    text: "gezeigt".into(),
                })
            }
            Command::Kopieren(_) => Reply::Kopie(ClipReply {
                zusammenfassung: "1 Position".into(),
                daten: "proAVA-Zwischenablage|1|zeilen|1 Position|DATEN".into(),
                lv: LvInfo::default(),
                zeilen: vec![row(10, "01.0010", "Schaltschrank ASP01")],
                projekt: window
                    .fenster
                    .projekt
                    .as_ref()
                    .map(|project| project.name.clone())
                    .unwrap_or_default(),
            }),
            Command::Einfuegen(_) => Reply::Erledigt(Done {
                text: "1 Position eingefügt ab 01.0030.".into(),
                zeilen: vec![row(31, "01.0030", "Schaltschrank ASP01")],
                wartet: false,
            }),
            Command::ZiehenUeber(_) | Command::ZiehenEnde { .. } | Command::Ablegen(_) => {
                Reply::refused("kein Ziehen in diesem Test")
            }
            Command::Stand | Command::Uebernehmen(_) | Command::Speichern => {
                Reply::refused("kein Projektstand in diesem Test (tests/window_writes.rs)")
            }
            Command::ProjektOeffnen(_) => {
                if let Some(entry) = self.appears.lock().unwrap().take() {
                    self.windows.lock().unwrap().push(entry);
                }
                Reply::Erledigt(Done {
                    text: "Neues proAVA-Fenster wird gestartet.".into(),
                    ..Done::default()
                })
            }
        })
    }
}

fn entry(pid: u32, project: Option<(&str, &Path)>, x: f32, focus_ms: u64) -> WindowEntry {
    WindowEntry {
        v: 1,
        pid,
        kanal: format!("/nirgends/{pid}.sock"),
        fenster: WindowInfo {
            pid,
            programm: "0.2.0".into(),
            ki_erlaubt: true,
            lage: Some(Geometry {
                x,
                y: 0.0,
                breite: 950.0,
                hoehe: 1000.0,
            }),
            fokus_zuletzt_ms: focus_ms,
            projekt: project.map(|(name, file)| ProjectInfo {
                name: name.into(),
                datei: file.to_string_lossy().into_owned(),
                lv: LvInfo {
                    stelle: 1,
                    nummer: "1".into(),
                    titel: "GA".into(),
                    positionen: 3,
                },
                lvs: vec![],
                reiter: "LV".into(),
                auswahl: vec![row(10, "01.0010", "Schaltschrank ASP01")],
                ..ProjectInfo::default()
            }),
            ..WindowInfo::default()
        },
        ..WindowEntry::default()
    }
}

fn server(root: &Path, recording: &Recording) -> Server {
    Server::new(
        Box::new(XmlProjectFile::new()),
        Box::new(proava_exchange::GaebPdfExchange),
    )
    .with_allowed_roots(vec![root.to_path_buf()])
    .with_windows(Box::new(recording.clone()))
}

fn call(server: &mut Server, tool: &str, arguments: Value) -> Result<String, String> {
    server.call_tool(tool, arguments.as_object().unwrap())
}

fn project(server: &mut Server, folder: &Path, name: &str) -> PathBuf {
    call(
        server,
        "projekt_anlegen",
        json!({"ordner": folder, "name": name, "vorlage": "GA", "positionen": false}),
    )
    .unwrap();
    folder.join(name).join(format!("{name}.proava"))
}

/// Two windows: Rathaus 3 left, Schule Musterstadt right (used last).
fn two_windows(recording: &Recording, left: &Path, right: &Path) {
    *recording.windows.lock().unwrap() = vec![
        entry(101, Some(("Rathaus 3", left)), 0.0, 1_000),
        entry(202, Some(("Schule Musterstadt", right)), 960.0, 2_000),
    ];
}

#[test]
fn without_windows_the_tools_say_so() {
    let folder = TempFolder::new("ohne");
    let recording = Recording::default();
    let mut server = server(&folder.0, &recording);
    let answer = call(&mut server, "fenster_lesen", json!({})).unwrap();
    assert!(answer.contains("Kein proAVA-Fenster offen"), "{answer}");
    let error = call(&mut server, "fenster_zeigen", json!({"auswahl": true})).unwrap_err();
    assert!(error.contains("Kein proAVA-Fenster"), "{error}");
    // Ohne Brücke (z. B. andere Einbindung) ebenso.
    let mut plain = Server::new(
        Box::new(XmlProjectFile::new()),
        Box::new(proava_exchange::GaebPdfExchange),
    );
    let error = call(&mut plain, "fenster_suchen", json!({"suche": "x"})).unwrap_err();
    assert!(error.contains("Kein proAVA-Fenster"), "{error}");
}

#[test]
fn reading_lists_the_windows_with_their_sides() {
    let folder = TempFolder::new("lesen");
    let recording = Recording::default();
    let mut server = server(&folder.0, &recording);
    two_windows(
        &recording,
        &folder.0.join("a.proava"),
        &folder.0.join("b.proava"),
    );
    let answer = call(&mut server, "fenster_lesen", json!({})).unwrap();
    assert!(answer.contains("2 proAVA-Fenster offen"), "{answer}");
    assert!(answer.contains("[links] PID 101 · Rathaus 3"), "{answer}");
    assert!(
        answer.contains("[rechts, aktiv] PID 202 · Schule Musterstadt"),
        "{answer}"
    );
    assert!(answer.contains("Auswahl: 01.0010 Position Schaltschrank ASP01"));
    let answer = call(&mut server, "fenster_lesen", json!({"fenster": "links"})).unwrap();
    assert!(
        answer.contains("Fenster links (PID 101, Rathaus 3)"),
        "{answer}"
    );
    assert!(
        answer.contains("die erste gewählte Zeile ist die Schreibmarke")
            || answer.contains("Schreibmarke")
    );
    let answer = call(
        &mut server,
        "fenster_lesen",
        json!({"fenster": "rechts", "ab": "01.0010", "anzahl": 3, "langtext": true}),
    )
    .unwrap();
    assert!(answer.contains("Schaltschrank ASP01"), "{answer}");
    let reads = recording.without_reads();
    let (pid, Command::Lesen(read)) = &reads[0] else {
        panic!("{reads:?}")
    };
    assert_eq!(*pid, 202);
    assert_eq!(read.ab.as_deref(), Some("01.0010"));
    assert_eq!(read.anzahl, Some(3));
    assert!(read.langtext);
}

/// Ablauf (a): rechts suchen → zeigen → links an der Schreibmarke einfügen.
#[test]
fn rows_go_from_the_right_window_to_the_cursor_of_the_left_one() {
    let folder = TempFolder::new("a");
    let recording = Recording::default();
    let mut server = server(&folder.0, &recording);
    two_windows(
        &recording,
        &folder.0.join("a.proava"),
        &folder.0.join("b.proava"),
    );
    call(
        &mut server,
        "fenster_suchen",
        json!({"fenster": "rechts", "suche": "Schaltschrank"}),
    )
    .unwrap();
    call(
        &mut server,
        "fenster_zeigen",
        json!({"fenster": "rechts", "oz": ["01.0010"], "markierung": "Vorschlag", "positionsfenster": true}),
    )
    .unwrap();
    let answer = call(
        &mut server,
        "fenster_einfuegen",
        json!({"fenster": "links", "quelle_fenster": "rechts", "auswahl": true, "weitere": 3}),
    )
    .unwrap();
    assert!(
        answer.contains("Fenster links (PID 101, Rathaus 3): 1 Position eingefügt"),
        "{answer}"
    );
    assert!(
        answer.contains("Preise geleert"),
        "Standard ohne Preise: {answer}"
    );
    let asked = recording.without_reads();
    assert_eq!(asked.len(), 4, "{asked:?}");
    assert!(matches!(&asked[0], (202, Command::Suchen(search)) if search.suche == "Schaltschrank"));
    assert!(matches!(&asked[1], (202, Command::Zeigen(show))
        if show.zeilen.oz == ["01.0010"] && show.positionsfenster && show.markierung.as_deref() == Some("Vorschlag")));
    let (202, Command::Kopieren(take)) = &asked[2] else {
        panic!("{asked:?}")
    };
    assert!(take.zeilen.auswahl);
    assert_eq!(take.zeilen.weitere, 3);
    assert!(take.mengen && !take.preise);
    assert!(take
        .markierung
        .as_deref()
        .unwrap()
        .contains("Rathaus 3 (Fenster links)"));
    let (101, Command::Einfuegen(insert)) = &asked[3] else {
        panic!("{asked:?}")
    };
    assert_eq!(
        insert.daten,
        "proAVA-Zwischenablage|1|zeilen|1 Position|DATEN"
    );
    assert_eq!(insert.herkunft, "Schule Musterstadt (Fenster rechts)");
    assert_eq!(insert.nach, None, "an der Schreibmarke");

    let error = call(
        &mut server,
        "fenster_einfuegen",
        json!({"fenster": "links", "quelle_fenster": "links"}),
    )
    .unwrap_err();
    assert!(error.contains("dasselbe Fenster"), "{error}");
    let error = call(
        &mut server,
        "fenster_einfuegen",
        json!({"fenster": "links"}),
    )
    .unwrap_err();
    assert!(error.contains("Woher?"), "{error}");
}

#[test]
fn rows_of_a_file_go_into_a_window_as_clipboard_text() {
    let folder = TempFolder::new("datei");
    let recording = Recording::default();
    let mut server = server(&folder.0, &recording);
    let source = project(&mut server, &folder.0, "Schule Musterstadt");
    call(
        &mut server,
        "position_einfuegen",
        json!({"projekt": source, "unter_titel": "01.482", "kurztext": "Informationsschwerpunkt ASP01", "menge": 1, "einheit": "St", "ep": 18500, "langtext": "Standschrank RAL 7035"}),
    )
    .unwrap();
    two_windows(
        &recording,
        &folder.0.join("a.proava"),
        &folder.0.join("b.proava"),
    );
    let answer = call(
        &mut server,
        "fenster_einfuegen",
        json!({"fenster": "links", "quelle": source, "oz": ["01.482.0010"], "nach_oz": "01.482", "preise": true}),
    )
    .unwrap();
    assert!(answer.contains("Aus Schule Musterstadt.proava"), "{answer}");
    let asked = recording.without_reads();
    let (101, Command::Einfuegen(insert)) = &asked[0] else {
        panic!("{asked:?}")
    };
    assert_eq!(insert.nach.as_deref(), Some("01.482"));
    let header = proava_application::ClipHeader::find(&insert.daten).expect("Zwischenablage");
    assert_eq!(header.summary, "1 Position");
    let clip = proava_application::clipboard::decode(&XmlProjectFile::new(), &header).unwrap();
    let proava_application::Clip::Rows(rows) = clip else {
        panic!("Zeilen erwartet")
    };
    let position = rows.rows[0].as_position().unwrap();
    assert_eq!(position.short_text(), "Informationsschwerpunkt ASP01");
    assert!(position.unit_price().is_some(), "mit Preisen");
    assert!(position.long_text().to_plain().contains("RAL 7035"));
    let error = call(
        &mut server,
        "fenster_einfuegen",
        json!({"fenster": "links", "quelle": "/etc/passwd.proava", "oz": ["1"]}),
    )
    .unwrap_err();
    assert!(error.contains("außerhalb"), "{error}");
}

#[test]
fn a_project_opens_through_a_running_window() {
    let folder = TempFolder::new("oeffnen");
    let recording = Recording::default();
    let mut server = server(&folder.0, &recording);
    let file = project(&mut server, &folder.0, "Schule Musterstadt");
    let error = call(
        &mut server,
        "fenster_projekt_oeffnen",
        json!({"projekt": file}),
    )
    .unwrap_err();
    assert!(error.contains("Kein proAVA-Fenster offen"), "{error}");

    *recording.windows.lock().unwrap() = vec![entry(
        101,
        Some(("Rathaus 3", &folder.0.join("a.proava"))),
        0.0,
        5,
    )];
    *recording.appears.lock().unwrap() =
        Some(entry(303, Some(("Schule Musterstadt", &file)), 960.0, 0));
    let answer = call(
        &mut server,
        "fenster_projekt_oeffnen",
        json!({"projekt": file, "schreibgeschuetzt": true}),
    )
    .unwrap();
    assert!(answer.contains("Offen im Fenster PID 303"), "{answer}");
    let asked = recording.without_reads();
    let (101, Command::ProjektOeffnen(open)) = &asked[0] else {
        panic!("{asked:?}")
    };
    assert!(open.schreibgeschuetzt);
    assert_eq!(Path::new(&open.pfad), file);
    // Schon offen: nur der Hinweis.
    let answer = call(
        &mut server,
        "fenster_projekt_oeffnen",
        json!({"projekt": file}),
    )
    .unwrap();
    assert!(answer.contains("schon offen im Fenster"), "{answer}");
    assert_eq!(recording.without_reads().len(), 1);
    let error = call(
        &mut server,
        "fenster_projekt_oeffnen",
        json!({"projekt": "/etc/x.proava"}),
    )
    .unwrap_err();
    assert!(error.contains("außerhalb"), "{error}");
}

#[test]
fn a_window_without_permission_is_refused() {
    let folder = TempFolder::new("aus");
    let recording = Recording::default();
    let mut server = server(&folder.0, &recording);
    let mut window = entry(101, Some(("Rathaus 3", &folder.0.join("a.proava"))), 0.0, 1);
    window.fenster.ki_erlaubt = false;
    *recording.windows.lock().unwrap() = vec![window];
    let error = call(&mut server, "fenster_zeigen", json!({"auswahl": true})).unwrap_err();
    assert!(error.contains("KI-Assistenten ausgeschaltet"), "{error}");
    let answer = call(&mut server, "fenster_lesen", json!({})).unwrap();
    assert!(
        answer.contains("KI-Bedienung in diesem Fenster ausgeschaltet"),
        "{answer}"
    );
}

#[test]
fn the_window_tools_are_in_the_catalogue() {
    let folder = TempFolder::new("katalog");
    let mut server = server(&folder.0, &Recording::default());
    let list = server
        .handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}))
        .unwrap();
    let tools = list["result"]["tools"].as_array().unwrap().clone();
    let tool = |name: &str| {
        tools
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap_or_else(|| panic!("{name} fehlt"))
            .clone()
    };
    for (name, read_only) in [
        ("fenster_lesen", true),
        ("fenster_zeigen", true),
        ("fenster_suchen", true),
        ("fenster_einfuegen", false),
        ("fenster_projekt_oeffnen", false),
    ] {
        let tool = tool(name);
        assert_eq!(tool["annotations"]["readOnlyHint"], read_only, "{name}");
    }
    assert!(tool("fenster_projekt_oeffnen")["inputSchema"]["properties"]["lv"].is_null());
    // Abläufe, Freigabe und „im Fenster arbeiten“ stehen einmal in den Server-Hinweisen.
    let init = server
        .handle(&json!({"jsonrpc": "2.0", "id": 2, "method": "initialize",
            "params": {"protocolVersion": "2025-06-18"}}))
        .unwrap();
    let instructions = init["result"]["instructions"].as_str().unwrap();
    for part in [
        "Typische Abläufe",
        "readOnlyHint false braucht die Freigabe des Anwenders",
        "Ist das Projekt in einem proAVA-Fenster offen",
        "fenster_einfuegen",
    ] {
        assert!(instructions.contains(part), "{part}");
    }
}
