//! Every tool works in the open window: a project open in a proAVA window
//! is read from the window (also unsaved) and a change goes back to it
//! (`stand` / `uebernehmen`) instead of into the file.
//!
//! The window is a stand-in that keeps the project in memory and answers
//! like a window (no window, no socket, no process); the exchange folder is
//! the test's temporary folder. What the real window does with a change
//! (undo step, mark, question) is tested in `proava-ui` (`ui_bridge.rs`).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use proava_application::ProjectRepository;
use proava_bridge_protocol::{
    Command, Done, LvInfo, ProjectInfo, Reply, StateReply, WindowEntry, WindowInfo,
};
use proava_domain::{Address, Bid, Decimal, Lv, LvGroup, OzMask, Position, Project, ProjectName};
use proava_exchange::{lv_to_gaeb, GaebPdfExchange};
use proava_gaeb::Phase;
use proava_infrastructure::XmlProjectFile;
use proava_mcp::{Server, WindowLink};
use serde_json::{json, Value};

/// A folder only this test uses (unique, created exclusively); it is
/// removed with this value, nothing else.
struct TempFolder(PathBuf, #[allow(dead_code)] tempfile::TempDir);

impl TempFolder {
    fn new(name: &str) -> Self {
        let dir = tempfile::Builder::new()
            .prefix(&format!("proava-mcp-fensterschreiben-{name}-"))
            .tempdir()
            .unwrap();
        Self(proava_mcp::real_path(dir.path()).unwrap(), dir)
    }
}

const PID: u32 = 4711;

/// The open window: its project in memory, what it was asked.
#[derive(Clone)]
struct Window(Arc<Mutex<State>>);

struct State {
    file: PathBuf,
    project: Project,
    exchange: PathBuf,
    lent: Option<u64>,
    next: u64,
    /// The tools whose change the window took over.
    adopted: Vec<String>,
    saved: usize,
    asked: Vec<&'static str>,
    /// Extras ▸ Einstellungen: assistants may work in the window.
    ai: bool,
    /// proAVA asks the user after a change (versandt, Angebote …).
    question: bool,
    /// The user changes the project right after the tool read it.
    meddles: bool,
}

impl Window {
    fn open(file: &Path, exchange: &Path) -> Self {
        let project = XmlProjectFile::new().load_project(file).unwrap();
        Self(Arc::new(Mutex::new(State {
            file: file.to_path_buf(),
            project,
            exchange: exchange.to_path_buf(),
            lent: None,
            next: 0,
            adopted: Vec::new(),
            saved: 0,
            asked: Vec::new(),
            ai: true,
            question: false,
            meddles: false,
        })))
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.0.lock().unwrap()
    }
}

impl WindowLink for Window {
    fn windows(&self) -> Vec<WindowEntry> {
        let state = self.state();
        vec![WindowEntry {
            v: 1,
            pid: PID,
            kanal: format!("/nirgends/{PID}.sock"),
            fenster: WindowInfo {
                pid: PID,
                ki_erlaubt: state.ai,
                fokus: true,
                projekt: Some(ProjectInfo {
                    name: state.project.name().as_str().to_string(),
                    datei: state.file.to_string_lossy().into_owned(),
                    lv: LvInfo::default(),
                    ungespeichert: true,
                    ..ProjectInfo::default()
                }),
                ..WindowInfo::default()
            },
            ..WindowEntry::default()
        }]
    }

    fn ask(
        &self,
        window: &WindowEntry,
        command: Command,
        _timeout: Duration,
    ) -> Result<Reply, String> {
        let mut state = self.state();
        state.asked.push(command.name());
        if !state.ai {
            return Ok(Reply::Aus {
                text: "KI-Assistenten ausgeschaltet".into(),
            });
        }
        Ok(match command {
            Command::FensterLesen => Reply::Fenster(window.fenster.clone()),
            Command::Stand => {
                state.next += 1;
                let number = state.next;
                std::fs::create_dir_all(&state.exchange).unwrap();
                let file = state.exchange.join(format!("{PID}-stand-{number}.proava"));
                XmlProjectFile::new()
                    .save_project(&file, &state.project)
                    .unwrap();
                state.lent = (!state.meddles).then_some(number);
                Reply::Stand(StateReply {
                    datei: file.to_string_lossy().into_owned(),
                    stand: number,
                    projekt: state.file.to_string_lossy().into_owned(),
                    ungespeichert: true,
                })
            }
            Command::Uebernehmen(adopt) => {
                if state.lent != Some(adopt.stand) {
                    return Ok(Reply::refused(
                        "Das Projekt wurde im Fenster inzwischen geändert – nichts übernommen.",
                    ));
                }
                state.lent = None;
                let mut changed = XmlProjectFile::new()
                    .load_project(Path::new(&adopt.datei))
                    .unwrap();
                changed.adopt_session_keys(&state.project);
                let _ = state.project.swap_state(changed);
                state.adopted.push(adopt.werkzeug.clone());
                let text = if state.question {
                    "Im Fenster übernommen – proAVA fragt den Anwender im Fenster („LV hat bereits \
                     Angebote“) – wartet auf den Anwender."
                } else {
                    "Im Fenster übernommen – ein Schritt, Strg+Z im Fenster nimmt ihn zurück; nicht \
                     gespeichert (fenster_speichern)."
                };
                Reply::Erledigt(Done {
                    text: text.into(),
                    wartet: state.question,
                    ..Done::default()
                })
            }
            Command::Speichern => {
                let mut copy = state.project.clone();
                copy.remove_unused_images();
                XmlProjectFile::new()
                    .save_project(&state.file, &copy)
                    .unwrap();
                state.saved += 1;
                Reply::Erledigt(Done {
                    text: format!("Gespeichert: {}.", state.file.display()),
                    ..Done::default()
                })
            }
            _ => Reply::refused("nicht in diesem Test"),
        })
    }

    fn exchange_folder(&self) -> Option<PathBuf> {
        Some(self.state().exchange.clone())
    }
}

fn server(root: &Path) -> Server {
    let preferences = proava_application::Preferences {
        new_lv_mask: "11.22.PPPPI".into(),
        ..proava_application::Preferences::default()
    };
    Server::new(Box::new(XmlProjectFile::new()), Box::new(GaebPdfExchange))
        .with_allowed_roots(vec![root.to_path_buf()])
        .with_template_defaults(proava_mcp::TemplateDefaults {
            folder: None,
            preferences,
        })
}

fn call(server: &mut Server, tool: &str, arguments: &Value) -> Result<String, String> {
    server.call_tool(tool, arguments.as_object().unwrap())
}

fn dec(text: &str) -> Decimal {
    text.parse().unwrap()
}

/// LV „11.22.PPPP“: 01 Automation / 01.01 ASP01 with two priced positions.
fn plain(folder: &Path) -> PathBuf {
    let mut lv = Lv::new("LV 480 GA", OzMask::parse("11.22.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "Automation"));
    lv.push_group(LvGroup::new(1, "01.01", "ASP01"));
    lv.push_position(Position::new(
        "01.01.0010",
        "CO2-Fühler Raum",
        dec("12"),
        "St",
        Some(dec("250")),
    ));
    lv.push_position(Position::new(
        "01.01.0020",
        "Kabel J-Y(St)Y",
        dec("100"),
        "m",
        Some(dec("1.5")),
    ));
    let project = Project::new(ProjectName::parse("Rathaus").unwrap(), "2026-11-17", lv);
    save(folder, project)
}

/// As [`plain`], with a bid of Firma A that got the Auftrag.
fn awarded(folder: &Path) -> PathBuf {
    let file = plain(folder);
    let mut project = XmlProjectFile::new().load_project(&file).unwrap();
    let ids: Vec<_> = project
        .lv()
        .rows()
        .iter()
        .filter(|row| row.is_position())
        .map(|row| row.id())
        .collect();
    let mut bid = Bid::new(Address::named("Firma A"), "a.x84");
    bid.set_unit_price(ids[0], Some(dec("265")));
    bid.set_unit_price(ids[1], Some(dec("1.65")));
    let bid = project.add_bid(bid);
    project.award_bid(bid, "15.09.2026").unwrap();
    save(folder, project)
}

fn save(folder: &Path, project: Project) -> PathBuf {
    std::fs::create_dir_all(folder).unwrap();
    let file = folder.join("Rathaus.proava");
    XmlProjectFile::new().save_project(&file, &project).unwrap();
    file
}

/// Files the tools read, in the shared folder (same paths for both runs).
fn fixtures(root: &Path) {
    const PNG: [u8; 67] = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    let shared = root.join("gemeinsam");
    std::fs::create_dir_all(&shared).unwrap();
    std::fs::write(shared.join("schema.png"), PNG).unwrap();
    std::fs::write(
        shared.join("Topologie.pdf"),
        proava_print::sample_vector_pdf(),
    )
    .unwrap();
    // Another project to take positions from, and GAEB/CSV of it.
    let source = plain(&shared.join("quelle"));
    let project = XmlProjectFile::new().load_project(&source).unwrap();
    let (document, _) = lv_to_gaeb(&project, Phase::Dp83);
    std::fs::write(
        shared.join("Zulieferung.X83"),
        proava_gaeb::xml::write(&document, Phase::Dp83).unwrap(),
    )
    .unwrap();
    let (mut bid, _) = lv_to_gaeb(&project, Phase::Dp82);
    bid.phase = Phase::Dp84;
    bid.award.bidder = Some(proava_gaeb::Address {
        name1: "Firma B".into(),
        city: "Essen".into(),
        ..proava_gaeb::Address::default()
    });
    std::fs::write(
        shared.join("b.D84"),
        proava_gaeb::datan::write(&bid).unwrap(),
    )
    .unwrap();
    std::fs::write(
        shared.join("liste.csv"),
        "OZ:;Positionstext:;Einheit:;Menge:;EP:;GP:;KG:;Positionsart:\n\
         01;Feldgeräte;;;;;;Titel\n01.0010;Raumfühler;St;4;80;320;481;Position\n",
    )
    .unwrap();
}

/// One tool: a template, steps before (on the file, before the window
/// opens), the call, and whether the result can be compared field by field
/// (not when it records the time of the call or a path of its run).
struct Case {
    tool: &'static str,
    template: fn(&Path) -> PathBuf,
    before: Vec<(&'static str, Value)>,
    args: Value,
    compare: bool,
}

fn case(tool: &'static str, args: Value) -> Case {
    Case {
        tool,
        template: plain,
        before: Vec::new(),
        args,
        compare: true,
    }
}

impl Case {
    fn awarded(mut self) -> Self {
        self.template = awarded;
        self
    }

    fn after(mut self, steps: Vec<(&'static str, Value)>) -> Self {
        self.before = steps;
        self
    }

    fn not_compared(mut self) -> Self {
        self.compare = false;
        self
    }
}

/// `projekt` and the shared folder go into the arguments.
fn with_paths(args: &Value, project: &Path, shared: &Path) -> Value {
    let text = args
        .to_string()
        .replace("GEMEINSAM", &shared.to_string_lossy());
    let mut args: Value = serde_json::from_str(&text).unwrap();
    args["projekt"] = json!(project);
    args
}

fn every_case() -> Vec<Case> {
    let rows = json!([{"titel": "Fühler", "ebene": 2}, {"kurztext": "Raumfühler", "menge": 12, "einheit": "St"}]);
    let order_steps = || {
        vec![
            ("auftrag_festschreiben", json!({})),
            (
                "rechnung_anlegen",
                json!({"nummer": "AR 1", "datum": "31.10.2026"}),
            ),
        ]
    };
    let change_order = || {
        vec![(
            "nachtrag_anlegen",
            json!({"bezeichnung": "Lüftung Werkstatt"}),
        )]
    };
    let estimate = || vec![("kosten_anlegen", json!({"stufe": "kostenschaetzung"}))];
    vec![
        case("titel_einfuegen", json!({"kurztext": "Feldgeräte"})),
        case(
            "position_einfuegen",
            json!({"unter_titel": "01.01", "kurztext": "Kanalfühler", "menge": 4, "einheit": "St", "ep": 85,
                   "langtext": "PT1000\n- **Messbereich** -30…+70 °C"}),
        ),
        case(
            "positionen_einfuegen",
            json!({"unter_titel": "01", "zeilen": rows}),
        ),
        case(
            "hinweis_einfuegen",
            json!({"unter_titel": "01", "am_anfang": true, "kurztext": "Vorbemerkung", "text": "Gilt für alle."}),
        ),
        case("abstand_einfuegen", json!({"nach_oz": "01.01.0010"})),
        case(
            "ausfuehrungsbeschreibung_einfuegen",
            json!({"unter_titel": "01", "bezeichnung": "Kabel", "langtext": "Die folgenden Kabel …"}),
        ),
        case(
            "bild_einfuegen",
            json!({"oz": "01.01.0010", "datei": "GEMEINSAM/schema.png", "breite_mm": 60}),
        ),
        case(
            "pdf_ausschnitt_einfuegen",
            json!({"oz": "01.01.0010", "datei": "GEMEINSAM/Topologie.pdf",
                   "ausschnitt_anteile": {"links": 0, "oben": 0, "rechts": 0.5, "unten": 0.5}}),
        ),
        case(
            "bieterangabe_einfuegen",
            json!({"oz": "01.01.0010", "vortext": "Fabrikat:"}),
        ),
        case(
            "zeile_aendern",
            json!({"oz": "01.01.0010", "menge": 6, "ep": "92,50", "kg": "481"}),
        ),
        case("zeile_loeschen", json!({"oz": "01.01.0020"})),
        case(
            "zeile_umwandeln",
            json!({"oz": "01.01.0020", "zeilenart": "hinweis", "bestaetigen": true}),
        ),
        case(
            "zeile_verschieben",
            json!({"oz": "01.01.0020", "richtung": "hoch"}),
        ),
        case("neu_nummerieren", json!({})).after(vec![(
            "position_einfuegen",
            json!({"unter_titel": "01.01", "kurztext": "Nachzügler", "oz": "01.01.0015"}),
        )]),
        case(
            "projektangaben_setzen",
            json!({"vergabeart": "Öffentliche Ausschreibung", "bauherr": {"name1": "Stadt Musterstadt"},
                   "kopf_rechts": "Seite {seite} von {seiten}"}),
        ),
        case("lv_anlegen", json!({"bezeichnung": "Elektro", "nummer": "2"})),
        case(
            "positionen_uebernehmen",
            json!({"quelle": "GEMEINSAM/quelle/Rathaus.proava", "oz": ["01.01.0010"], "unter_titel": "01.01"}),
        ),
        case(
            "stlb_uebernehmen",
            json!({"datei": "GEMEINSAM/Zulieferung.X83", "unter_titel": "01", "positionen": ["01.01.0010"]}),
        ),
        case(
            "gaeb_importieren",
            json!({"datei": "GEMEINSAM/Zulieferung.X83", "ziel": "lv", "nummer": "7"}),
        ),
        case(
            "csv_importieren",
            json!({"datei": "GEMEINSAM/liste.csv", "ziel": "lv", "nummer": "8"}),
        ),
        case(
            "bieter_anlegen",
            json!({"name1": "Müller Elektro GmbH", "ort": "Musterstadt"}),
        ),
        case("bieter_aendern", json!({"bieter": 1, "ort": "Neustadt"}))
            .after(vec![("bieter_anlegen", json!({"name1": "Müller"}))]),
        case(
            "angebot_einlesen",
            json!({"datei": "GEMEINSAM/b.D84"}),
        ),
        case("kosten_anlegen", json!({"stufe": "kostenschaetzung"})),
        case("kosten_aendern", json!({"kennwert": 480})).after(estimate()),
        case(
            "kosten_zeilen_schreiben",
            json!({"zeilen": [{"bauteil": "Gebäude", "anlage": "RLT 1", "dp": {"AI": 12, "BI": 30}}]}),
        )
        .after(estimate()),
        case("kostenvorgabe_setzen", json!({"budget": {"481": 62500}})),
        case(
            "kostenverfolgung_aendern",
            json!({"erlaeuterungen": {"481": "Angebote über der Kostenberechnung"}}),
        ),
        case(
            "angebot_nachlass_setzen",
            json!({"angebot": 1, "prozent": 3}),
        )
        .awarded(),
        case(
            "auftrag_erteilen",
            json!({"angebot": 1, "datum": "01.10.2026"}),
        )
        .awarded(),
        case(
            "preise_uebernehmen",
            json!({"quelle": "angebot", "angebot": 1, "ueberschreiben": true}),
        )
        .awarded(),
        case("auftrag_festschreiben", json!({}))
            .awarded()
            .not_compared(),
        case(
            "rechnung_anlegen",
            json!({"nummer": "AR 1", "datum": "31.10.2026"}),
        )
        .awarded()
        .after(vec![("auftrag_festschreiben", json!({}))])
        .not_compared(),
        case("rechnung_aendern", json!({"rechnung": 1, "nummer": "AR 1a"}))
            .awarded()
            .after(order_steps())
            .not_compared(),
        case(
            "rechnungsposition_setzen",
            json!({"rechnung": 1, "oz": "01.01.0010", "rechnung_menge": 4}),
        )
        .awarded()
        .after(order_steps())
        .not_compared(),
        case(
            "aufmass_setzen",
            json!({"rechnung": 1, "oz": "01.01.0020", "zeilen": [{"ort": "EG", "rechenansatz": "2*12,5"}]}),
        )
        .awarded()
        .after(order_steps())
        .not_compared(),
        case(
            "nachtrag_anlegen",
            json!({"bezeichnung": "Lüftung Werkstatt"}),
        )
        .awarded(),
        case(
            "nachtrag_aendern",
            json!({"nachtrag": 1, "begruendung": "Nutzerwunsch"}),
        )
        .awarded()
        .after(change_order()),
        case(
            "nachtragsposition_setzen",
            json!({"nachtrag": 1, "neu": true, "kurztext": "Raumfühler", "menge": 2, "einheit": "St"}),
        )
        .awarded()
        .after(change_order()),
        case(
            "nachtrag_status",
            json!({"nachtrag": 1, "status": "angeboten"}),
        )
        .awarded()
        .after(change_order()),
        case(
            "nachtragsangebot_einlesen",
            json!({"nachtrag": 1, "datei": "GEMEINSAM/b.D84"}),
        )
        .awarded()
        .after(change_order()),
        case(
            "bibliothek_einfuegen",
            json!({"bibliothek": "GEMEINSAM/quelle/Rathaus.proava", "oz": ["01.01.0020"], "unter_titel": "01.01"}),
        ),
        // Files the letters (PDF with the time of printing) in the project.
        case(
            "anschreiben_drucken",
            json!({"art": "aufforderung", "alle": true, "ziel": "GEMEINSAM/Aufforderung.pdf", "ueberschreiben": true}),
        )
        .after(vec![("bieter_anlegen", json!({"name1": "Müller Elektro GmbH"}))])
        .not_compared(),
    ]
}

/// Runs one case on the file and in the window; the window's project must
/// equal the file's, and the file of the window must stay untouched.
fn run(root: &Path, number: usize, case: &Case) {
    let shared = root.join("gemeinsam");
    let base = root.join(format!("fall-{number}"));
    let on_file = (case.template)(&base.join("datei"));
    let in_window = (case.template)(&base.join("fenster"));
    for file in [&on_file, &in_window] {
        let mut prepare = server(root);
        for (tool, args) in &case.before {
            let args = with_paths(args, file, &shared);
            if let Err(error) = call(&mut prepare, tool, &args) {
                panic!("{}: Vorbereitung {tool}: {error}", case.tool);
            }
        }
    }
    let untouched = std::fs::read(&in_window).unwrap();

    let mut plain_server = server(root);
    let file_answer = call(
        &mut plain_server,
        case.tool,
        &with_paths(&case.args, &on_file, &shared),
    );
    let window = Window::open(&in_window, &base.join("bruecke").join("austausch"));
    let mut window_server = server(root).with_windows(Box::new(window.clone()));
    let window_answer = call(
        &mut window_server,
        case.tool,
        &with_paths(&case.args, &in_window, &shared),
    );
    let file_answer = file_answer.unwrap_or_else(|error| panic!("{} Datei: {error}", case.tool));
    let window_answer =
        window_answer.unwrap_or_else(|error| panic!("{} Fenster: {error}", case.tool));
    assert!(
        window_answer.contains("Im Fenster übernommen"),
        "{}: {window_answer}",
        case.tool
    );
    assert!(
        !window_answer.contains("Sicherung"),
        "{}: keine Sicherung, nichts gespeichert",
        case.tool
    );
    let state = window.state();
    assert_eq!(state.adopted, vec![case.tool.to_string()], "{}", case.tool);
    assert_eq!(
        std::fs::read(&in_window).unwrap(),
        untouched,
        "{}: die Datei des Fensters bleibt, wie sie war",
        case.tool
    );
    let leftovers = std::fs::read_dir(&state.exchange)
        .map(|entries| entries.count())
        .unwrap_or(0);
    assert_eq!(leftovers, 0, "{}: Austauschordner aufgeräumt", case.tool);
    if case.compare {
        let from_file = XmlProjectFile::new().load_project(&on_file).unwrap();
        assert!(
            from_file.same_content(&state.project),
            "{}: im Fenster anders als auf der Datei\nDatei: {file_answer}\nFenster: {window_answer}\n{:#?}\n---\n{:#?}",
            case.tool,
            from_file.lv(),
            state.project.lv()
        );
    }
}

#[test]
fn every_write_tool_works_in_the_open_window_as_on_the_file() {
    let folder = TempFolder::new("alle");
    fixtures(&folder.0);
    let cases = every_case();
    for (number, case) in cases.iter().enumerate() {
        run(&folder.0, number, case);
    }
    assert!(cases.len() >= 40, "{} Fälle", cases.len());
}

#[test]
fn reading_tools_see_the_unsaved_state_of_the_window() {
    let folder = TempFolder::new("lesen");
    let file = plain(&folder.0.join("p"));
    let window = Window::open(&file, &folder.0.join("austausch"));
    window
        .state()
        .project
        .lv_mut()
        .set_title("Ungespeichert im Fenster");
    let mut server = server(&folder.0).with_windows(Box::new(window.clone()));
    let answer = call(&mut server, "projekt_lesen", &json!({"projekt": file})).unwrap();
    assert!(answer.contains("Ungespeichert im Fenster"), "{answer}");
    assert!(window.state().adopted.is_empty(), "Lesen ändert nichts");
}

#[test]
fn a_change_in_the_window_meanwhile_is_not_overwritten() {
    let folder = TempFolder::new("inzwischen");
    let file = plain(&folder.0.join("p"));
    let window = Window::open(&file, &folder.0.join("austausch"));
    let mut server = server(&folder.0).with_windows(Box::new(window.clone()));
    // The user changes something after the tool read the state.
    window.state().meddles = true;
    let error = call(
        &mut server,
        "zeile_aendern",
        &json!({"projekt": file, "oz": "01.01.0010", "menge": 7}),
    )
    .unwrap_err();
    assert!(error.contains("Nichts geändert"), "{error}");
    assert!(error.contains("inzwischen geändert"), "{error}");
    assert!(window.state().adopted.is_empty());
}

#[test]
fn with_assistants_switched_off_reading_uses_the_file_and_changes_are_refused() {
    let folder = TempFolder::new("aus");
    let file = plain(&folder.0.join("p"));
    let window = Window::open(&file, &folder.0.join("austausch"));
    window.state().ai = false;
    let before = std::fs::read(&file).unwrap();
    let mut server = server(&folder.0).with_windows(Box::new(window.clone()));
    let answer = call(&mut server, "lv_lesen", &json!({"projekt": file})).unwrap();
    assert!(answer.contains("CO2-Fühler Raum"), "{answer}");
    let error = call(
        &mut server,
        "zeile_aendern",
        &json!({"projekt": file, "oz": "01.01.0010", "menge": 7}),
    )
    .unwrap_err();
    assert!(
        error.contains("KI darf geöffnete Fenster bedienen"),
        "{error}"
    );
    assert!(error.contains("speichert und schließt"), "{error}");
    assert_eq!(std::fs::read(&file).unwrap(), before, "Datei unverändert");
}

#[test]
fn a_question_in_the_window_replaces_bestaetigen() {
    let folder = TempFolder::new("rueckfrage");
    let file = awarded(&folder.0.join("p"));
    // On the file: the question comes as an answer, nothing saved.
    let mut on_file = server(&folder.0);
    let args = json!({"projekt": file, "unter_titel": "01.01", "kurztext": "Raumfühler", "menge": 2, "einheit": "St"});
    let error = call(&mut on_file, "position_einfuegen", &args).unwrap_err();
    assert!(error.contains("bestaetigen"), "{error}");
    // In the window: proAVA asks the user there.
    let window = Window::open(&file, &folder.0.join("austausch"));
    window.state().question = true;
    let mut server = server(&folder.0).with_windows(Box::new(window.clone()));
    let answer = call(&mut server, "position_einfuegen", &args).unwrap();
    assert!(answer.contains("wartet auf den Anwender"), "{answer}");
    assert_eq!(window.state().adopted, vec!["position_einfuegen"]);
}

#[test]
fn fenster_speichern_saves_like_strg_s_with_one_backup() {
    let folder = TempFolder::new("speichern");
    let file = plain(&folder.0.join("p"));
    let window = Window::open(&file, &folder.0.join("austausch"));
    let mut server = server(&folder.0).with_windows(Box::new(window.clone()));
    call(
        &mut server,
        "zeile_aendern",
        &json!({"projekt": file, "oz": "01.01.0010", "kurztext": "CO2-Fühler Kanal"}),
    )
    .unwrap();
    assert!(!std::fs::read_to_string(&file)
        .unwrap_or_default()
        .contains("CO2-Fühler Kanal"));
    let answer = call(&mut server, "fenster_speichern", &json!({"projekt": file})).unwrap();
    assert!(answer.contains("Gespeichert"), "{answer}");
    assert!(answer.contains("Sicherung vorher"), "{answer}");
    let saved = XmlProjectFile::new().load_project(&file).unwrap();
    assert!(saved
        .lv()
        .rows()
        .iter()
        .any(|row| row.short_text() == "CO2-Fühler Kanal"));
    let backups = || {
        std::fs::read_dir(file.parent().unwrap())
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().contains("_Sicherung_"))
            .count()
    };
    assert_eq!(backups(), 1);
    // Second time: no second backup; also by window name.
    let answer = call(
        &mut server,
        "fenster_speichern",
        &json!({"fenster": "aktiv"}),
    )
    .unwrap();
    assert!(!answer.contains("Sicherung"), "{answer}");
    assert_eq!(backups(), 1);
    assert_eq!(window.state().saved, 2);
}

#[test]
fn an_older_window_is_named() {
    #[derive(Clone)]
    struct Old(Window);
    impl WindowLink for Old {
        fn windows(&self) -> Vec<WindowEntry> {
            self.0.windows()
        }
        fn ask(&self, _: &WindowEntry, _: Command, _: Duration) -> Result<Reply, String> {
            Ok(Reply::refused(
                "Anfrage nicht verstanden: unknown variant `stand`, expected one of …",
            ))
        }
    }
    let folder = TempFolder::new("alt");
    let file = plain(&folder.0.join("p"));
    let window = Window::open(&file, &folder.0.join("austausch"));
    let mut server = server(&folder.0).with_windows(Box::new(Old(window)));
    let error = call(&mut server, "lv_lesen", &json!({"projekt": file})).unwrap_err();
    assert!(error.contains("proAVA neu"), "{error}");
}

#[test]
fn a_lock_of_an_proava_out_of_reach_stops_changes_a_stale_one_does_not() {
    let folder = TempFolder::new("sperre");
    let file = plain(&folder.0.join("p"));
    let lock = file.with_file_name(".~lock.Rathaus.proava#");
    let mut server = server(&folder.0);
    let change = json!({"projekt": file, "oz": "01.01.0010", "menge": 7});

    // On another computer (network drive): the lock counts.
    std::fs::write(
        &lock,
        "proAVA;kollege;anderer-rechner;4711;19.11.2026 08:00\n",
    )
    .unwrap();
    let before = std::fs::read(&file).unwrap();
    let error = call(&mut server, "zeile_aendern", &change).unwrap_err();
    assert!(
        error.contains("geöffnet von kollege auf anderer-rechner seit 19.11.2026 08:00"),
        "{error}"
    );
    assert_eq!(std::fs::read(&file).unwrap(), before, "nichts gespeichert");
    let answer = call(&mut server, "lv_lesen", &json!({"projekt": file})).unwrap();
    assert!(answer.contains("CO2-Fühler Raum"), "Lesen geht: {answer}");

    // Left by a proAVA of this computer that no longer runs: no matter.
    XmlProjectFile::new().write_lock(&file).unwrap();
    let line = std::fs::read_to_string(&lock).unwrap();
    let mut fields: Vec<String> = line.trim().split(';').map(str::to_string).collect();
    fields[3] = (u32::MAX - 1).to_string();
    std::fs::write(&lock, fields.join(";") + "\n").unwrap();
    if XmlProjectFile::new()
        .read_lock(&file)
        .is_some_and(|lock| lock.stale)
    {
        call(&mut server, "zeile_aendern", &change).unwrap();
        assert_ne!(std::fs::read(&file).unwrap(), before, "gespeichert");
    }
}
