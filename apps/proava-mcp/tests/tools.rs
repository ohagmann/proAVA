//! The MCP server end to end: over stdio and through the tool handler,
//! with projects in temporary folders only.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use proava_application::ProjectRepository;
use proava_domain::Decimal;
use proava_exchange::{lv_to_gaeb, GaebPdfExchange};
use proava_gaeb::{BoqNode, Phase};
use proava_infrastructure::XmlProjectFile;
use proava_mcp::Server;
use serde_json::{json, Value};

/// A folder only this test uses: a unique name, created exclusively (two
/// tests – also of other test runs at the same time – never share one); it is
/// removed with this value, nothing else.
struct TempFolder(PathBuf, #[allow(dead_code)] tempfile::TempDir);

impl TempFolder {
    fn new(name: &str) -> Self {
        let dir = tempfile::Builder::new()
            .prefix(&format!("proava-mcp-{name}-"))
            .tempdir()
            .unwrap();
        // As the server writes it (Windows without `\\?\`).
        Self(proava_mcp::real_path(dir.path()).unwrap(), dir)
    }
}

/// Empty projects take the OZ-Maske of the settings; these tests are written
/// for 11.22.PPPPI (the Vorgabe before 18.11.2026, still in many settings).
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

fn call(server: &mut Server, tool: &str, arguments: Value) -> Result<String, String> {
    server.call_tool(tool, arguments.as_object().unwrap())
}

fn ok(server: &mut Server, tool: &str, arguments: Value) -> String {
    match call(server, tool, arguments.clone()) {
        Ok(text) => text,
        Err(error) => panic!("{tool} {arguments}: {error}"),
    }
}

fn new_project(server: &mut Server, folder: &Path, name: &str) -> String {
    let answer = ok(
        server,
        "projekt_anlegen",
        json!({"ordner": folder, "name": name}),
    );
    assert!(answer.contains("Projekt angelegt"), "{answer}");
    folder
        .join(name)
        .join(format!("{name}.proava"))
        .to_string_lossy()
        .to_string()
}

fn backups(folder: &Path) -> usize {
    std::fs::read_dir(folder)
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().contains("_Sicherung_"))
        .count()
}

#[test]
fn stdio_initialize_list_and_call() {
    let folder = TempFolder::new("stdio");
    let mut child = Command::new(env!("CARGO_BIN_EXE_proava-mcp"))
        .env("PROAVA_MCP_ORDNER", &folder.0)
        // The bridge to open windows looks into the test's folder, never into
        // the user's runtime folder.
        .env("XDG_RUNTIME_DIR", &folder.0)
        // Never a browser window from a test (no test starts a process).
        .env(proava_mcp::NO_PROGRAMS, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut ask = |message: Value| -> Option<Value> {
        writeln!(input, "{message}").unwrap();
        input.flush().unwrap();
        message.get("id")?;
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        Some(serde_json::from_str(&line).unwrap())
    };

    let init = ask(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "1"}}}))
    .unwrap();
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "proava");
    assert!(ask(json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).is_none());

    let list = ask(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).unwrap();
    let tools = list["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 86);
    let insert = tools
        .iter()
        .find(|tool| tool["name"] == "position_einfuegen")
        .unwrap();
    assert!(
        insert["inputSchema"]["properties"]["lv"].is_object(),
        "LV-Werkzeuge nehmen lv"
    );
    let search = tools
        .iter()
        .find(|tool| tool["name"] == "projekte_suchen")
        .unwrap();
    assert!(search["inputSchema"]["properties"]["lv"].is_null());
    // Jedes Werkzeug zeigt seine eigenen Felder (bild_einfuegen hatte die der
    // Ausführungsbeschreibung überdeckt).
    let fields = |name: &str| {
        let tool = tools.iter().find(|tool| tool["name"] == name).unwrap();
        tool["inputSchema"]["properties"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    };
    let perf = fields("ausfuehrungsbeschreibung_einfuegen");
    for key in ["nummer", "bezeichnung", "langtext", "nach_oz"] {
        assert!(perf.contains(&key.to_string()), "{key} fehlt: {perf:?}");
    }
    assert!(!perf.contains(&"datei".to_string()), "{perf:?}");
    let image = fields("bild_einfuegen");
    for key in ["datei", "breite_mm", "absatz", "id"] {
        assert!(image.contains(&key.to_string()), "{key} fehlt: {image:?}");
    }
    for tool in tools {
        assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
        assert!(!tool["description"].as_str().unwrap().is_empty());
    }

    let created = ask(json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": {"name": "projekt_anlegen", "arguments": {"ordner": folder.0, "name": "Stdio"}}}))
    .unwrap();
    assert_eq!(created["result"]["isError"], false, "{created}");
    assert!(folder.0.join("Stdio/Stdio.proava").is_file());

    let outside = ask(json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call",
        "params": {"name": "projekt_lesen", "arguments": {"projekt": "/etc/passwd.proava"}}}))
    .unwrap();
    assert_eq!(outside["result"]["isError"], true);
    assert!(outside["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("außerhalb"));

    let windows = ask(json!({"jsonrpc": "2.0", "id": 6, "method": "tools/call",
        "params": {"name": "fenster_lesen", "arguments": {}}}))
    .unwrap();
    assert!(windows["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("Kein proAVA-Fenster offen"));

    let unknown = ask(json!({"jsonrpc": "2.0", "id": 5, "method": "gibtsnicht"})).unwrap();
    assert_eq!(unknown["error"]["code"], -32601);

    drop(input);
    assert!(child.wait().unwrap().success());
}

#[test]
fn writing_reading_changing_and_deleting() {
    let folder = TempFolder::new("lv");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "GA");
    let project_folder = folder.0.join("GA");

    let answer = ok(
        &mut server,
        "titel_einfuegen",
        json!({"projekt": project, "kurztext": "Automationsstationen"}),
    );
    assert!(answer.contains("01 „Automationsstationen“"), "{answer}");
    let answer = ok(
        &mut server,
        "titel_einfuegen",
        json!({"projekt": project, "kurztext": "ASP01", "unter_titel": "01"}),
    );
    assert!(answer.contains("01.01 „ASP01“"), "{answer}");
    assert!(
        !answer.contains("Sicherung"),
        "neues Projekt braucht keine Sicherung: {answer}"
    );

    let answer = ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "unter_titel": "01.01", "kurztext": "DDC-Station", "menge": 1, "einheit": "St",
               "ep": "2.500,00", "langtext": "Liefern und montieren:\n- **DDC-Station** mit *Bedienteil*\n- Netzteil 24 V"}),
    );
    assert!(answer.contains("01.01.0010 „DDC-Station“"), "{answer}");
    assert!(answer.contains("2.500,00 €"), "{answer}");

    let answer = ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "unter_titel": "01.01", "zeilen": [
            {"kurztext": "E/A-Modul", "menge": 4, "einheit": "St", "ep": 350},
            {"kurztext": "Schaltschrank", "pauschal": true, "ep": 4000},
            {"titel": "Feldgeräte", "ebene": 1},
            {"titel": "Fühler"},
            {"hinweis": "Fühler nach **Fühlerliste**."},
            {"kurztext": "Raumfühler", "menge": "12", "einheit": "St", "ep": 80.5},
            {"kurztext": "Außenfühler", "menge": 1, "einheit": "St", "art": "bedarf"}
        ]}),
    );
    assert!(answer.contains("7 Zeilen eingefügt"), "{answer}");
    assert!(answer.contains("01.01.0030 „Schaltschrank“"), "{answer}");
    assert!(answer.contains("02.01 „Fühler“"), "{answer}");
    assert!(answer.contains("02.01.0020 „Außenfühler“"), "{answer}");

    let lv = ok(&mut server, "lv_lesen", json!({"projekt": project}));
    assert!(
        lv.contains("01.01.0020 | Pos | E/A-Modul | 4 St | 350,00 | 1.400,00 |"),
        "{lv}"
    );
    assert!(
        lv.contains("02.01.0020 | Pos B | Außenfühler | 1 St | - | - |"),
        "{lv}"
    );
    assert!(
        lv.contains("01 | T1 | Automationsstationen | | | 7.900,00 |"),
        "{lv}"
    );
    assert!(
        lv.contains("| Hinweis | Fühler nach Fühlerliste. |"),
        "{lv}"
    );
    assert!(lv.contains("LV netto 8.866,00 €"), "{lv}");
    let titles = ok(
        &mut server,
        "lv_lesen",
        json!({"projekt": project, "nur_titel": true}),
    );
    assert!(!titles.contains("E/A-Modul"));
    let part = ok(
        &mut server,
        "lv_lesen",
        json!({"projekt": project, "ab_oz": "2", "mit_langtext": true}),
    );
    assert!(
        !part.contains("DDC-Station") && part.contains("    Fühler nach **Fühlerliste**."),
        "{part}"
    );

    let position = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "1.1.10"}),
    );
    assert!(
        position.contains("- **DDC-Station** mit *Bedienteil*"),
        "{position}"
    );
    assert!(
        position.contains("Unter: 01 Automationsstationen › 01.01 ASP01"),
        "{position}"
    );

    let answer = ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01.0020", "menge": 6, "ep": "92,5"}),
    );
    assert!(
        answer.contains("Menge, EP") && answer.contains("555,00"),
        "{answer}"
    );
    let answer = ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "02.01.0020", "ep": null, "art": "normal", "menge": 2}),
    );
    assert!(answer.contains("Art"), "{answer}");
    assert!(call(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "02.01.0020"})
    )
    .is_err());
    assert!(call(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "09.09.0090", "menge": 1})
    )
    .is_err());

    let refused = call(
        &mut server,
        "zeile_loeschen",
        json!({"projekt": project, "oz": "01.01"}),
    )
    .unwrap_err();
    assert!(refused.contains("mit_inhalt"), "{refused}");
    let answer = ok(
        &mut server,
        "zeile_loeschen",
        json!({"projekt": project, "oz": "01.01.0010"}),
    );
    assert!(
        answer.contains("Gelöscht: 01.01.0010")
            && answer.contains("2 weitere Zeilen haben neue OZ"),
        "{answer}"
    );

    let answer = ok(
        &mut server,
        "zeile_verschieben",
        json!({"projekt": project, "oz": "02", "richtung": "hoch"}),
    );
    assert!(answer.contains("01 „Feldgeräte“"), "{answer}");
    let lv = ok(&mut server, "lv_lesen", json!({"projekt": project}));
    assert!(lv.contains("02.01.0010 | Pos | E/A-Modul"), "{lv}");

    let check = ok(&mut server, "lv_pruefen", json!({"projekt": project}));
    assert!(check.contains("Fehler: keine"), "{check}");
    assert!(check.contains("Ohne EP"), "{check}");
    assert!(
        check.contains("Fachregeln der X83, wie proAVA sie schreibt (R1–R13): 0 Fehler"),
        "{check}"
    );
    let priced = ok(
        &mut server,
        "lv_pruefen",
        json!({"projekt": project, "art": "X82"}),
    );
    assert!(priced.contains("Fachregeln der X82"), "{priced}");
    let wrong = call(
        &mut server,
        "lv_pruefen",
        json!({"projekt": project, "art": "X99"}),
    );
    assert!(wrong.is_err_and(|error| error.contains("X99")));

    ok(
        &mut server,
        "hinweis_einfuegen",
        json!({"projekt": project, "unter_titel": "02", "am_anfang": true, "text": "Vorbemerkung"}),
    );
    let removed = ok(
        &mut server,
        "zeile_loeschen",
        json!({"projekt": project, "oz": "02", "mit_inhalt": true}),
    );
    assert!(removed.contains("samt"), "{removed}");

    let info = ok(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": project, "projektbezeichnung": "Neubau Schule",
        "bauherr": {"name1": "Stadt Musterstadt", "ort": "Musterstadt"}, "schrittweite": 100}),
    );
    assert!(
        info.contains("Projektbezeichnung") && info.contains("OZ neu vergeben"),
        "{info}"
    );
    let overview = ok(&mut server, "projekt_lesen", json!({"projekt": project}));
    assert!(
        overview.contains("Bauherr: Stadt Musterstadt, Musterstadt")
            && overview.contains("Auftraggeber: wie Bauherr"),
        "{overview}"
    );
    // Befund 3: an own Auftraggeber beside the Bauherr, and back.
    ok(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": project, "auftraggeber": {"name1": "Zentrale Vergabestelle"}}),
    );
    let overview = ok(&mut server, "projekt_lesen", json!({"projekt": project}));
    assert!(
        overview.contains("Bauherr: Stadt Musterstadt, Musterstadt")
            && overview.contains("Auftraggeber: Zentrale Vergabestelle, Musterstadt"),
        "{overview}"
    );
    ok(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": project, "auftraggeber_wie_bauherr": true}),
    );
    assert!(
        ok(&mut server, "projekt_lesen", json!({"projekt": project}))
            .contains("Auftraggeber: wie Bauherr")
    );
    assert!(
        overview.contains("Schrittweite 100") && overview.contains("fortlaufend"),
        "{overview}"
    );
    assert!(ok(&mut server, "lv_lesen", json!({"projekt": project})).contains("01.01.0100"));

    assert_eq!(backups(&project_folder), 0, "selbst angelegtes Projekt");
    let found = ok(&mut server, "projekte_suchen", json!({"ordner": folder.0}));
    assert!(found.contains("GA.proava"), "{found}");
}

#[test]
fn imported_numbers_get_free_ordinals_in_between() {
    let folder = TempFolder::new("oz");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "OZ");
    ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "zeilen": [
            {"titel": "Titel"}, {"titel": "Untertitel", "ebene": 2},
            {"kurztext": "A", "menge": 1, "einheit": "St"}, {"kurztext": "B", "menge": 1, "einheit": "St"}
        ]}),
    );
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01.0020", "neue_oz": "01.01.0050"}),
    );
    assert!(ok(&mut server, "projekt_lesen", json!({"projekt": project})).contains("eigene OZ"));

    let answer = ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "nach_oz": "01.01.0010", "kurztext": "C", "menge": 1, "einheit": "St"}),
    );
    assert!(answer.contains("01.01.0020 „C“"), "{answer}");
    let answer = ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "nach_oz": "01.01.0020", "kurztext": "D", "menge": 1, "einheit": "St"}),
    );
    assert!(
        answer.contains("01.01.0030 „D“") && !answer.contains("neue OZ"),
        "{answer}"
    );
    let lv = ok(&mut server, "lv_lesen", json!({"projekt": project}));
    assert!(lv.contains("01.01.0050 | Pos | B"), "{lv}");

    let moved = ok(
        &mut server,
        "zeile_verschieben",
        json!({"projekt": project, "oz": "01.01.0050", "richtung": "hoch", "schritte": 3}),
    );
    assert!(moved.contains("01.01.0005 „B“"), "{moved}");
    assert!(
        call(&mut server, "neu_nummerieren", json!({"projekt": project}))
            .unwrap()
            .contains("4 OZ neu vergeben")
    );
}

/// A GAEB index may be a letter (as the import keeps it from other
/// programs): by hand per MCP too, written in capitals, fits the mask.
#[test]
fn position_index_may_be_a_letter() {
    let folder = TempFolder::new("index");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Index");
    let answer = ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "zeilen": [
            {"titel": "Titel", "oz": "01"}, {"titel": "Untertitel", "ebene": 2, "oz": "01.01"},
            {"kurztext": "Grundausführung", "menge": 1, "einheit": "St", "oz": "01.01.0004.a"},
            {"kurztext": "Variante", "menge": 1, "einheit": "St", "oz": "01.01.0004.B"},
            {"kurztext": "Ziffer", "menge": 1, "einheit": "St", "oz": "01.01.0005.1"}
        ]}),
    );
    assert!(
        answer.contains("01.01.0004.A „Grundausführung“"),
        "{answer}"
    );
    assert!(answer.contains("01.01.0004.B „Variante“"), "{answer}");
    assert!(answer.contains("01.01.0005.1 „Ziffer“"), "{answer}");
    let read = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.01.0004.B"}),
    );
    assert!(read.contains("Variante"), "{read}");
    let check = ok(&mut server, "lv_pruefen", json!({"projekt": project}));
    assert!(!check.contains("OZ passt nicht zur Maske"), "{check}");
    let target = folder.0.join("index.X83");
    ok(
        &mut server,
        "gaeb_exportieren",
        json!({"projekt": project, "art": "X83", "ziel": target}),
    );
    let xml = std::fs::read_to_string(&target).unwrap();
    assert!(xml.contains(r#"RNoPart="0004" RNoIndex="A""#), "{xml}");
    assert!(xml.contains(r#"RNoPart="0004" RNoIndex="B""#));
}

/// Unterpunkte by indent in the markup: kept in the project, shown by
/// position_lesen the same way, written to GAEB as a list in the item and
/// read back by gaeb_importieren with their levels.
#[test]
fn list_levels_by_indent_go_to_gaeb_and_back() {
    let folder = TempFolder::new("listen");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Listen");
    let langtext = "Inhalte:\n1. Automationsebene\n  - Parameter\n  - Alarme\n2. Leitebene\n  1. Software\n    - Firmware";
    ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "zeilen": [
            {"titel": "Titel", "oz": "01"}, {"titel": "Untertitel", "ebene": 2, "oz": "01.01"},
            {"kurztext": "Wartung", "menge": 1, "einheit": "psch", "oz": "01.01.0010", "langtext": langtext}
        ]}),
    );
    let read = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.01.0010"}),
    );
    assert!(read.contains(langtext), "{read}");
    let target = folder.0.join("listen.X83");
    ok(
        &mut server,
        "gaeb_exportieren",
        json!({"projekt": project, "art": "X83", "ziel": target}),
    );
    let xml = std::fs::read_to_string(&target)
        .unwrap()
        .replace("<span>", "")
        .replace("</span>", "");
    assert!(
        xml.contains("<ol><li><p>Automationsebene</p><ul><li><p>Parameter</p></li><li><p>Alarme</p></li></ul></li><li><p>Leitebene</p><ol><li><p>Software</p><ul><li><p>Firmware</p></li></ul></li></ol></li></ol>"),
        "{xml}"
    );
    let answer = ok(
        &mut server,
        "gaeb_importieren",
        json!({"datei": target, "ziel": "projekt", "ordner": folder.0, "name": "Zurueck"}),
    );
    assert!(!answer.contains("Fehler"), "{answer}");
    let back = folder.0.join("Zurueck").join("Zurueck.proava");
    let read = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": back, "oz": "01.01.0010"}),
    );
    assert!(read.contains(langtext), "{read}");
}

#[test]
fn zuschlaege_and_what_the_import_knows_are_entered_by_hand() {
    let folder = TempFolder::new("zuschlag");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Zuschlag");
    ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "zeilen": [
            {"titel": "Rohbau", "oz": "01"}, {"titel": "Erdarbeiten", "ebene": 2, "oz": "01.01"},
            {"kurztext": "Aushub", "menge": 100, "einheit": "m3", "ep": 12.5, "oz": "01.01.0010"},
            {"kurztext": "Verfüllen", "menge": 40, "einheit": "m3", "ep": 8, "oz": "01.01.0020",
             "zu_bezuschlagen": true,
             "unterbeschreibungen": [{"nummer": "1", "kurztext": "Lage 1", "menge": 25, "einheit": "m3"}]},
            {"kurztext": "Zuschlag Erschwernis", "oz": "01.01.0030",
             "zuschlag": {"art": "alle", "prozent": 5}},
            {"kurztext": "Zuschlag Kleinmengen", "oz": "01.01.0040", "ep": 2,
             "zuschlag": {"art": "liste", "bezug": ["01.01.0010", {"oz": "01.01.0020", "teilmenge": 10}]}},
            {"kurztext": "Zuschlag Winter", "oz": "01.01.0050",
             "zuschlag": {"art": "gekennzeichnet", "prozent": 10}},
            {"kurztext": "Abfuhr", "menge": 1, "einheit": "psch", "ep": 500, "oz": "01.01.0060",
             "entfaellt": true}
        ]}),
    );
    let lv = ok(&mut server, "lv_lesen", json!({"projekt": project}));
    // 100 × 12,50 + 40 × 8 = 1.570,00 → 5 % = 78,50
    assert!(lv.contains("Zuschlag auf 1.570,00"), "{lv}");
    assert!(lv.contains("| 5 % | 78,50"), "{lv}");
    // 1.250,00 + 10 × 8 = 1.330,00 → 2 % = 26,60; marked 320,00 → 32,00
    assert!(lv.contains("| 2 % | 26,60"), "{lv}");
    assert!(lv.contains("| 10 % | 32,00"), "{lv}");
    let read = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.01.0040"}),
    );
    assert!(
        read.contains("auf die genannten Positionen (liste)"),
        "{read}"
    );
    assert!(
        read.contains("Bezug: 01.01.0010, 01.01.0020 (Teilmenge 10 m3)"),
        "{read}"
    );
    let read = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.01.0020"}),
    );
    assert!(read.contains("zu bezuschlagen: ja"), "{read}");
    assert!(
        read.contains("Unterbeschreibung 1: Lage 1 · 25 m3"),
        "{read}"
    );
    let read = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.01.0060"}),
    );
    assert!(read.contains("entfällt: ja"), "{read}");

    // Pauschalsumme of a title: its sum, the LV sums with it.
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01", "pauschalsumme": "1.500,00"}),
    );
    let lv = ok(&mut server, "lv_lesen", json!({"projekt": project}));
    assert!(
        lv.contains("01.01 | T2 | Erdarbeiten | | pauschal | 1.500,00"),
        "{lv}"
    );
    assert!(lv.contains("01 | T1 | Rohbau | | | 1.500,00"), "{lv}");
    let read = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.01"}),
    );
    assert!(
        read.contains("(Pauschalsumme; Positionen zusammen 1.707,10"),
        "{read}"
    );
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01", "pauschalsumme": null}),
    );

    // A Zuschlag has no quantity; a list needs positions.
    let error = call(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "kurztext": "Falsch", "menge": 3, "zuschlag": "alle"}),
    )
    .unwrap_err();
    assert!(error.contains("keine Menge"), "{error}");
    let error = call(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01.0030", "zuschlag": {"art": "liste"}}),
    )
    .unwrap_err();
    assert!(error.contains("mindestens einer Position"), "{error}");
    // Back to an ordinary position and again a Zuschlag.
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01.0050", "zuschlag": false, "menge": 1, "einheit": "St"}),
    );
    let read = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.01.0050"}),
    );
    assert!(!read.contains("Zuschlag:"), "{read}");
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01.0050", "zuschlag": {"art": "gekennzeichnet"}}),
    );

    // GAEB: MarkupItems without R9.
    let target = folder.0.join("zuschlag.X82");
    let answer = ok(
        &mut server,
        "gaeb_exportieren",
        json!({"projekt": project, "art": "X82", "ziel": target}),
    );
    assert!(!answer.contains("R9"), "{answer}");
    let xml = std::fs::read_to_string(&target).unwrap();
    assert_eq!(xml.matches("<MarkupItem").count(), 3, "{xml}");
    assert!(xml.contains("<MarkupType>ListInSubQty</MarkupType>"));
    assert!(xml.contains("<MarkupIt>Yes</MarkupIt>"));

    // Positionsnummer with letters only where the LV allows them.
    let error = call(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "kurztext": "Nachtrag", "menge": 1, "einheit": "St", "oz": "01.01.3A"}),
    )
    .unwrap_err();
    assert!(error.contains("positionen_mit_buchstaben"), "{error}");
    ok(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": project, "positionen_mit_buchstaben": true}),
    );
    let answer = ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "kurztext": "Nachtrag", "menge": 1, "einheit": "St", "oz": "01.01.3a"}),
    );
    assert!(answer.contains("01.01.3A „Nachtrag“"), "{answer}");
    // Right under a title above the deepest level („2.1.3A“ in files of other programs).
    let answer = ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "kurztext": "Zulage", "menge": 1, "einheit": "St", "oz": "1.3b",
               "unter_titel": "01"}),
    );
    assert!(answer.contains("01.3B „Zulage“"), "{answer}");
    let error = call(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "kurztext": "Falsch", "menge": 1, "einheit": "St", "oz": "02.3C",
               "unter_titel": "01"}),
    )
    .unwrap_err();
    assert!(error.contains("passt nicht unter Titel 01"), "{error}");
}

#[test]
fn every_address_field_and_the_dp_number_are_set_and_read() {
    let folder = TempFolder::new("adresse");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Adresse");
    ok(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": project, "vergabenummer": "VG-17", "datenaustauschnummer": "430-V03",
               "pauschalsumme": "1.460.000,00",
               "auftraggeber": {"name1": "Stadt Muster", "name3": "Hochbauamt", "land": "Deutschland",
                                "ansprechpartner": "Frau Muster", "fax": "0211 124", "ust_id": "DE123456789",
                                "iln": "4012345000009"}}),
    );
    let read = ok(&mut server, "projekt_lesen", json!({"projekt": project}));
    assert!(
        read.contains("Auftraggeber: Stadt Muster, Hochbauamt, Deutschland, Ansprechpartner Frau Muster, Fax 0211 124, USt-IdNr. DE123456789, ILN 4012345000009"),
        "{read}"
    );
    assert!(
        read.contains("Datenaustauschnummer (DPNo): 430-V03"),
        "{read}"
    );
    assert!(
        read.contains("LV netto 1.460.000,00 € (Pauschalsumme"),
        "{read}"
    );
    let target = folder.0.join("adresse.X83");
    ok(
        &mut server,
        "gaeb_exportieren",
        json!({"projekt": project, "art": "X83", "ziel": target}),
    );
    let xml = std::fs::read_to_string(&target).unwrap();
    for part in [
        "<Name3>Hochbauamt</Name3>",
        "<Country>Deutschland</Country>",
        "<Contact>Frau Muster</Contact>",
        "<Fax>0211 124</Fax>",
        "<VATID>DE123456789</VATID>",
        "<DPNo>430-V03</DPNo>",
    ] {
        assert!(xml.contains(part), "{part}: {xml}");
    }
}

#[test]
fn places_of_the_award_are_set_read_and_exported() {
    let folder = TempFolder::new("orte");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Orte");
    ok(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": project,
               "baustelle": {"name1": "Schulzentrum", "strasse": "Allee 3", "plz": "40211", "ort": "Düsseldorf"},
               "baustelle_nummer": "B-1", "baustelle_bezeichnung": "Haus A",
               "benachrichtigungsort": {"bezeichnung": "Bauleitung", "nummer": "N-1"},
               "weitere_ausfuehrungsorte": [{"bezeichnung": "Haus B", "nummer": "B-2"}],
               "bedarfstraeger": [{"name1": "Schulamt", "strasse": "Weg 1", "plz": "40210", "ort": "Düsseldorf", "nummer": "R-1"}],
               "debitorennummer": "D-4711", "auftraggeber_staat": "ewr", "auftraggeber_kfz_kennzeichen": "d"}),
    );
    let read = ok(&mut server, "projekt_lesen", json!({"projekt": project}));
    for part in [
        "Baustelle: Haus A, Nr. B-1, Schulzentrum",
        "Benachrichtigungsort: Bauleitung, Nr. N-1",
        "Weiterer Ausführungsort: Haus B, Nr. B-2",
        "Bedarfsträger: Nr. R-1, Schulamt",
        "Debitorennummer AG (AcctRecNo): D-4711",
        "Staat AG: EWR (D)",
    ] {
        assert!(read.contains(part), "{part}: {read}");
    }
    let target = folder.0.join("orte.X83");
    ok(
        &mut server,
        "gaeb_exportieren",
        json!({"projekt": project, "art": "X83", "ziel": target}),
    );
    let xml = std::fs::read_to_string(&target).unwrap();
    for part in [
        "<CnstSiteIDNo>B-1</CnstSiteIDNo>",
        "<NotifSiteName>Bauleitung</NotifSiteName>",
        "<CnstSiteName>Haus B</CnstSiteName>",
        "<RequestIDNo>R-1</RequestIDNo>",
        "<AcctRecNo>D-4711</AcctRecNo>",
        "<CntryType>EEA</CntryType>",
        "<CntryName>D</CntryName>",
    ] {
        assert!(xml.contains(part), "{part}: {xml}");
    }
    let error = call(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": project, "auftraggeber_staat": "mond"}),
    )
    .unwrap_err();
    assert!(error.contains("auftraggeber_staat"), "{error}");
}

#[test]
fn project_data_names_every_implausible_value_at_once() {
    let folder = TempFolder::new("plausibel");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Plausibel");
    let error = call(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": project, "ausfuehrung_beginn": "18.12.2020", "ausfuehrung_ende": "31.01.2020",
               "mwst": 120, "ep_anteile": ["Lohn", ""], "zeiteinheit": "Stunden",
               "skonto_prozent": 150}),
    )
    .unwrap_err();
    assert!(
        error.contains("Nichts gespeichert – 5 Angaben passen nicht"),
        "{error}"
    );
    assert!(error.contains("Ausführungsende (31.01.2020)"), "{error}");
    assert!(error.contains("Umsatzsteuer"), "{error}");
    assert!(error.contains("ep_anteile: leere Bezeichnung"), "{error}");
    assert!(error.contains("Skonto: 150 %"), "{error}");
    assert!(error.contains("Skonto ohne Zahlungsfrist"), "{error}");
    // All or nothing: not even the valid Zeiteinheit.
    let read = ok(&mut server, "projekt_lesen", json!({"projekt": project}));
    assert!(!read.contains("Stunden"), "{read}");
    assert!(!read.contains("18.12.2020"), "{read}");
}

#[test]
fn second_writer_backs_up_once_and_nothing_is_lost() {
    let folder = TempFolder::new("mtime");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Parallel");
    // A second server stands for proAVA or another session writing in between.
    let mut other = self::server(&folder.0);
    let first = ok(
        &mut other,
        "titel_einfuegen",
        json!({"projekt": project, "kurztext": "Von proAVA"}),
    );
    assert!(
        first.contains("Sicherung vor der ersten Änderung"),
        "{first}"
    );
    let second = ok(
        &mut other,
        "titel_einfuegen",
        json!({"projekt": project, "kurztext": "Noch einer"}),
    );
    assert!(!second.contains("Sicherung"), "{second}");
    assert_eq!(backups(&folder.0.join("Parallel")), 1);
    let answer = ok(
        &mut server,
        "titel_einfuegen",
        json!({"projekt": project, "kurztext": "Von der KI"}),
    );
    assert!(answer.contains("03 „Von der KI“"), "{answer}");
    assert!(ok(&mut server, "lv_lesen", json!({"projekt": project})).contains("Von proAVA"));
}

#[test]
fn several_server_processes_in_the_same_second_all_save() {
    // Each MCP process backs up before its first change; several in one
    // second used to collide on the backup name and saved nothing.
    let folder = TempFolder::new("sekunde");
    let mut first = server(&folder.0);
    let project = new_project(&mut first, &folder.0, "Sekunde");
    for number in 1..=4 {
        let mut process = self::server(&folder.0);
        let answer = ok(
            &mut process,
            "titel_einfuegen",
            json!({"projekt": project, "kurztext": format!("Titel {number}")}),
        );
        assert!(
            answer.contains("Sicherung vor der ersten Änderung"),
            "{answer}"
        );
        assert!(!answer.contains("fehlgeschlagen"), "{answer}");
    }
    let lv = ok(&mut first, "lv_lesen", json!({"projekt": project}));
    for number in 1..=4 {
        assert!(lv.contains(&format!("Titel {number}")), "{lv}");
    }
    assert_eq!(
        backups(&folder.0.join("Sekunde")),
        4,
        "je Stand eine Sicherung"
    );
}

fn scale_prices(nodes: &mut [BoqNode], factor: Decimal) {
    for node in nodes {
        match node {
            BoqNode::Category(category) => scale_prices(&mut category.body, factor),
            BoqNode::Item(item) => {
                item.unit_price = item.unit_price.map(|price| (price * factor).round_dp(2));
                item.total = match (item.unit_price, item.qty) {
                    (Some(price), Some(qty)) => Some((price * qty).round_dp(2)),
                    _ => None,
                };
            }
            _ => {}
        }
    }
}

fn write_d84(project_file: &str, target: &Path, bidder: &str, factor: &str) {
    let project = XmlProjectFile::new()
        .load_project(Path::new(project_file))
        .unwrap();
    let (mut document, _) = lv_to_gaeb(&project, Phase::Dp82);
    document.phase = Phase::Dp84;
    document.award.bidder = Some(proava_gaeb::Address {
        name1: bidder.into(),
        city: "Essen".into(),
        ..proava_gaeb::Address::default()
    });
    scale_prices(&mut document.award.boq.body, factor.parse().unwrap());
    std::fs::write(target, proava_gaeb::datan::write(&document).unwrap()).unwrap();
}

#[test]
fn gaeb90_bids_comparison_and_pdf() {
    let folder = TempFolder::new("austausch");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Vergabe");
    ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "zeilen": [
            {"titel": "Automation"},
            {"kurztext": "DDC-Station", "menge": 2, "einheit": "St", "ep": 2500, "langtext": "mit **Bedienteil**"},
            {"kurztext": "Fühler", "menge": 10, "einheit": "St", "ep": 80},
            {"titel": "Montage", "ebene": 1},
            {"kurztext": "Inbetriebnahme", "menge": 16, "einheit": "h", "ep": 95, "stundenlohn": true}
        ]}),
    );

    let exported = ok(
        &mut server,
        "gaeb_exportieren",
        json!({"projekt": project, "art": "X83", "format": "gaeb90"}),
    );
    assert!(exported.contains("Vergabe.D83"), "{exported}");
    let d83 = folder.0.join("Vergabe/Vergabe.D83");
    assert!(d83.is_file());
    assert!(call(
        &mut server,
        "gaeb_exportieren",
        json!({"projekt": project, "art": "X83", "format": "gaeb90"})
    )
    .unwrap_err()
    .contains("ueberschreiben"));
    assert!(call(
        &mut server,
        "gaeb_exportieren",
        json!({"projekt": project, "art": "X83", "ziel": folder.0.join("falsch.D82")})
    )
    .is_err());

    let imported = ok(
        &mut server,
        "lv_aus_gaeb",
        json!({"datei": d83, "ordner": folder.0, "name": "Kopie"}),
    );
    assert!(imported.contains("3 Positionen"), "{imported}");

    let bids = folder.0.join("Angebote");
    std::fs::create_dir_all(&bids).unwrap();
    write_d84(&project, &bids.join("a.D84"), "Firma A", "0.9");
    write_d84(&project, &bids.join("b.D84"), "Firma B", "1.3");
    let answer = ok(
        &mut server,
        "angebot_einlesen",
        json!({"projekt": project, "datei": bids.join("a.D84")}),
    );
    assert!(answer.contains("Firma A"), "{answer}");
    // Eine Angabe je Zeile.
    for line in [
        "\nDatei: a.D84\n",
        "\nBieter: Firma A\n",
        "\nPositionen ohne EP: 0\n",
        "\nRechenfehler: 0\n",
    ] {
        assert!(answer.contains(line), "{line} fehlt in {answer}");
    }
    // Anwender 18.11.: ins Bieterverzeichnis (Vorgabe), abschaltbar.
    assert!(
        answer.contains("Bieterverzeichnis: aufgenommen als Bieter #1"),
        "{answer}"
    );
    let answer = ok(
        &mut server,
        "angebot_einlesen",
        json!({"projekt": project, "datei": bids.join("b.D84"), "ins_bieterverzeichnis": false}),
    );
    assert!(!answer.contains("Bieterverzeichnis"), "{answer}");

    let overview = ok(&mut server, "angebote_lesen", json!({"projekt": project}));
    assert!(
        overview.contains("2 Angebot(e)") && overview.contains("Firma B"),
        "{overview}"
    );
    let answer = ok(
        &mut server,
        "angebot_nachlass_setzen",
        json!({"projekt": project, "angebot": 2, "prozent": 3}),
    );
    assert!(answer.contains("Nachlass 3 %"), "{answer}");

    let comparison = ok(
        &mut server,
        "preisspiegel_lesen",
        json!({"projekt": project, "je_titel": true, "positionen": true}),
    );
    assert!(comparison.contains("1 | Firma A (#1)"), "{comparison}");
    assert!(comparison.contains("günstigst Firma A"), "{comparison}");
    let position = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.00.0010"}),
    );
    assert!(position.contains("Firma A: EP 2.250,00"), "{position}");

    for (document, file) in [
        ("angebotsaufforderung", "Vergabe_LV_ohne_Preise.pdf"),
        ("lv_mit_preisen", "Vergabe_LV_mit_Preisen.pdf"),
        ("preisspiegel", "Vergabe_Preisspiegel.pdf"),
    ] {
        let answer = ok(
            &mut server,
            "pdf_drucken",
            json!({"projekt": project, "dokument": document}),
        );
        assert!(answer.contains(file), "{answer}");
        let bytes = std::fs::read(folder.0.join("Vergabe").join(file)).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    // Druckdialog-Optionen.
    let answer = ok(
        &mut server,
        "pdf_drucken",
        json!({"projekt": project, "dokument": "kurztext_lv", "titel": "01", "quer": true,
               "schrift_pt": 11, "kopftext": "Vorabzug"}),
    );
    assert!(
        answer.contains("Vergabe_Kurztext-LV.pdf") && answer.contains("Seiten"),
        "{answer}"
    );

    // Gitter (Tabellen voll umrandet): an und aus ergeben verschiedene PDFs.
    let mut gridded = Vec::new();
    for on in [true, false] {
        let answer = ok(
            &mut server,
            "pdf_drucken",
            json!({"projekt": project, "dokument": "lv_mit_preisen", "gitter": on, "ueberschreiben": true}),
        );
        assert!(answer.contains("Vergabe_LV_mit_Preisen.pdf"), "{answer}");
        gridded.push(
            std::fs::read(folder.0.join("Vergabe").join("Vergabe_LV_mit_Preisen.pdf")).unwrap(),
        );
    }
    assert_ne!(
        gridded[0].len(),
        gridded[1].len(),
        "Gitter ändert den Druck"
    );

    // Deckblatt in der Form „Briefkopf“, ohne Deckblatt, falsche Form.
    for (form, file) in [
        ("briefkopf", "Vergabe_LV_mit_Preisen.pdf"),
        ("kein", "Vergabe_LV_mit_Preisen.pdf"),
    ] {
        let answer = ok(
            &mut server,
            "pdf_drucken",
            json!({"projekt": project, "dokument": "bepreistes_lv", "deckblatt": form, "ueberschreiben": true}),
        );
        assert!(answer.contains(file), "{answer}");
    }
    // Mit Mengenermittlung unter den Positionen.
    let answer = ok(
        &mut server,
        "pdf_drucken",
        json!({"projekt": project, "dokument": "bepreistes_lv", "mengenermittlung": true, "ueberschreiben": true}),
    );
    assert!(answer.contains("Vergabe_LV_mit_Preisen.pdf"), "{answer}");
    // PDF-Lesezeichen für Positionen (Vorgabe an) lassen sich abwählen.
    let mut bookmarks = |positions: bool| {
        let target = folder.0.join(format!("Lesezeichen_{positions}.pdf"));
        ok(
            &mut server,
            "pdf_drucken",
            json!({"projekt": project, "dokument": "bepreistes_lv", "lesezeichen_positionen": positions,
                   "ziel": target.to_string_lossy()}),
        );
        let bytes = std::fs::read(&target).unwrap();
        bytes.windows(5).filter(|w| w == b"/Dest").count()
    };
    let (with, without) = (bookmarks(true), bookmarks(false));
    assert!(
        with > without && without > 0,
        "Lesezeichen {with} / {without}"
    );
    assert!(call(
        &mut server,
        "pdf_drucken",
        json!({"projekt": project, "dokument": "bepreistes_lv", "deckblatt": "bunt"})
    )
    .unwrap_err()
    .contains("briefkopf"));

    // Kopf-/Fußzeile: für einen Ausdruck, dann als eigene des Projekts.
    let answer = ok(
        &mut server,
        "pdf_drucken",
        json!({"projekt": project, "dokument": "zusammenstellung", "kopf_links": "{vergabenummer}\n{datei}",
               "fuss_mitte": "", "kopf_logo": "rechts", "kopf_schrift_pt": 9}),
    );
    assert!(answer.contains("Zusammenstellung"), "{answer}");
    assert!(
        !ok(&mut server, "projekt_lesen", json!({"projekt": project})).contains("Kopf-/Fußzeile"),
        "nur für den Ausdruck, nicht gespeichert"
    );
    let wrong = call(
        &mut server,
        "pdf_drucken",
        json!({"projekt": project, "dokument": "zusammenstellung", "kopf_links": "{Projekt}", "ueberschreiben": true}),
    );
    assert!(wrong
        .unwrap_err()
        .contains("Unbekannte Platzhalter: {Projekt}"));
    assert!(call(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": project, "kopf_schrift_pt": 20})
    )
    .is_err());
    ok(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": project, "fuss_rechts": "S. {seite}/{seiten}", "kopf_linie": false}),
    );
    let read = ok(&mut server, "projekt_lesen", json!({"projekt": project}));
    assert!(
        read.contains("Eigene Kopf-/Fußzeile")
            && read.contains("S. {seite}/{seiten}")
            && read.contains("Linie aus"),
        "{read}"
    );
    ok(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": project, "kopf_fusszeile_standard": true}),
    );
    assert!(
        !ok(&mut server, "projekt_lesen", json!({"projekt": project}))
            .contains("Eigene Kopf-/Fußzeile")
    );
    let answer = ok(
        &mut server,
        "pdf_drucken",
        json!({"projekt": project, "dokument": "bieterpreise", "angebot": 2}),
    );
    assert!(answer.contains("Bieterpreise"), "{answer}");
    assert!(call(
        &mut server,
        "pdf_drucken",
        json!({"projekt": project, "dokument": "zusammenstellung", "titel": "77"})
    )
    .unwrap_err()
    .contains("77"));

    // Vergabe-Abschluss.
    assert!(call(
        &mut server,
        "auftrags_lv_exportieren",
        json!({"projekt": project})
    )
    .unwrap_err()
    .contains("beauftragt"));
    let answer = ok(
        &mut server,
        "auftrag_erteilen",
        json!({"projekt": project, "angebot": 1, "datum": "01.12.2026"}),
    );
    assert!(answer.contains("Auftrag an Firma A"), "{answer}");
    for (format, file) in [
        ("x86", "Vergabe_Auftrags-LV.X86"),
        ("d86", "Vergabe_Auftrags-LV.D86"),
        ("pdf", "Vergabe_Auftrags-LV.pdf"),
    ] {
        let answer = ok(
            &mut server,
            "auftrags_lv_exportieren",
            json!({"projekt": project, "format": format}),
        );
        assert!(answer.contains(file), "{answer}");
        assert!(folder.0.join("Vergabe").join(file).is_file());
    }
    let answer = ok(
        &mut server,
        "preise_uebernehmen",
        json!({"projekt": project, "quelle": "minimum", "ueberschreiben": true}),
    );
    assert!(answer.contains("3 übernommen"), "{answer}");
    let position = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.00.0010"}),
    );
    assert!(position.contains("2.250,00"), "{position}");
    let answer = ok(
        &mut server,
        "auftrag_erteilen",
        json!({"projekt": project, "aufheben": true}),
    );
    assert!(answer.contains("aufgehoben"), "{answer}");
}

#[test]
fn finished_positions_are_locked_until_set_back() {
    let folder = TempFolder::new("status");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "GA");
    ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "zeilen": [
            {"titel": "Automation"},
            {"titel": "ASP01"},
            {"kurztext": "DDC-Station", "menge": 1, "einheit": "St"},
            {"kurztext": "E/A-Modul", "menge": 4, "einheit": "St"}
        ]}),
    );
    let answer = ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01.0010", "status": "fertig"}),
    );
    assert!(
        answer.contains("Status") && answer.contains("fertig"),
        "{answer}"
    );

    let refused = call(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01.0010", "menge": 3}),
    )
    .unwrap_err();
    assert!(
        refused.contains("fertig") && refused.contains("in Bearbeitung"),
        "{refused}"
    );
    for (tool, arguments) in [
        (
            "zeile_loeschen",
            json!({"projekt": project, "oz": "01.01.0010"}),
        ),
        (
            "zeile_loeschen",
            json!({"projekt": project, "oz": "01", "mit_inhalt": true}),
        ),
        (
            "zeile_verschieben",
            json!({"projekt": project, "oz": "01.01.0010", "richtung": "runter"}),
        ),
        (
            "zeile_aendern",
            json!({"projekt": project, "oz": "01.01.0010", "neue_oz": "01.01.0015"}),
        ),
    ] {
        let error = call(&mut server, tool, arguments.clone()).unwrap_err();
        assert!(error.contains("fertig"), "{tool} {arguments}: {error}");
    }
    // Der Titel selbst bleibt änderbar.
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01", "kurztext": "Automationsstationen"}),
    );
    let read = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.01.0010"}),
    );
    assert!(read.contains("Status: fertig (gesperrt"), "{read}");

    // Entsperren und ändern in einem Aufruf.
    let answer = ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01.0010", "status": "in Bearbeitung", "menge": 3}),
    );
    assert!(answer.contains("3 St"), "{answer}");
    assert!(answer.contains("in Bearbeitung"), "{answer}");

    // Ein Titel setzt alle Positionen darunter.
    let answer = ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01", "status": "fertig"}),
    );
    assert!(answer.contains("2 von 2 Positionen"), "{answer}");
    let error = call(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01.0020", "status": "fertich"}),
    )
    .unwrap_err();
    assert!(error.contains("gibt es nicht"), "{error}");
}

/// Herstellertexte (AUSSCHREIBEN.DE): synthetische Merkliste syn6, Quelle
/// aus dem Aufruf, Herkunft je Position, Text unverändert.
#[test]
fn manufacturer_texts_take_their_origin() {
    let folder = TempFolder::new("herstellertext");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Hersteller");
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/proava-gaeb/tests/data/syn6_herstellertext_32.X81");
    let gaeb = folder.0.join("Merkliste.X81");
    std::fs::copy(file, &gaeb).unwrap();
    ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "zeilen": [
            {"titel": "Feldgeräte", "ebene": 1},
            {"titel": "Antriebe", "ebene": 2},
            {"kurztext": "Davor", "menge": 1, "einheit": "St"}
        ]}),
    );
    let answer = ok(
        &mut server,
        "stlb_uebernehmen",
        json!({"projekt": project, "datei": gaeb, "quelle": "AUSSCHREIBEN.DE",
               "nach_oz": "01.01.0010", "positionen": ["01.0010"]}),
    );
    assert!(
        answer.contains("1 Position(en) aus AUSSCHREIBEN.DE übernommen"),
        "{answer}"
    );
    assert!(
        answer.contains("Textquelle: AUSSCHREIBEN.DE · Muster Antriebe GmbH"),
        "{answer}"
    );
    let position = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.01.0020"}),
    );
    assert!(position.contains("Artikelnummer MA-20-SR"), "{position}");
    assert!(position.contains("Datei Merkliste.X81"), "{position}");
    assert!(
        position.contains("oder gleichwertig"),
        "Text wie geliefert: {position}"
    );
    // Ohne quelle bleibt der Programmname der Datei.
    let answer = ok(
        &mut server,
        "stlb_uebernehmen",
        json!({"projekt": project, "datei": gaeb, "nach_oz": "01.01.0020", "positionen": ["02.0010"]}),
    );
    assert!(answer.contains("aus der Datei übernommen"), "{answer}");
    assert!(answer.contains("Textquelle: proAVA Testdaten"), "{answer}");
}

#[test]
fn stlb_positions_fill_a_placeholder_and_follow_it() {
    let folder = TempFolder::new("stlb");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "STLB");
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/proava-gaeb/tests/data/syn4_stlb_33.X83");
    let gaeb = folder.0.join("STLB-Export.X83");
    std::fs::copy(file, &gaeb).unwrap();
    ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "zeilen": [
            {"titel": "Gebäudeautomation", "ebene": 1},
            {"titel": "Raumautomation", "ebene": 2},
            {"kurztext": "Davor", "menge": 1, "einheit": "St"},
            {"kurztext": "Raumbediengerät folgt", "stlb": true, "menge": 9},
            {"kurztext": "Danach", "menge": 1, "einheit": "St"}
        ]}),
    );
    let answer = ok(
        &mut server,
        "stlb_uebernehmen",
        json!({"projekt": project, "datei": gaeb, "platzhalter_oz": "01.01.0020",
               "positionen": ["1.10", "01.0020"]}),
    );
    assert!(
        answer.contains("2 Position(en) aus STLB-Bau übernommen"),
        "{answer}"
    );
    assert!(answer.contains("[STLB-Bau 4/2026 070 TA]"), "{answer}");
    let lv = ok(&mut server, "lv_lesen", json!({"projekt": project}));
    assert!(
        lv.contains("01.01.0020 | Pos STLB-Bau | Testgerät Raumbediengerät UP | 9 St"),
        "{lv}"
    );
    assert!(
        lv.contains("01.01.0030 | Pos STLB-Bau | Testgerät Kanalfühler | 2 St"),
        "{lv}"
    );
    assert!(lv.contains("01.01.0040 | Pos | Danach"), "{lv}");

    // Keys survive the project file and the GAEB export.
    let loaded = XmlProjectFile::new()
        .load_project(Path::new(&project))
        .unwrap();
    let (doc, _) = lv_to_gaeb(&loaded, Phase::Dp83);
    let keys = doc
        .award
        .boq
        .items()
        .iter()
        .filter(|(_, item)| item.stlb.is_some())
        .count();
    assert_eq!(keys, 2);

    let error = call(
        &mut server,
        "stlb_uebernehmen",
        json!({"projekt": project, "datei": gaeb, "platzhalter_oz": "01.01.0010"}),
    )
    .unwrap_err();
    assert!(error.contains("kein STLB-Bau-Platzhalter"), "{error}");
    let error = call(
        &mut server,
        "stlb_uebernehmen",
        json!({"projekt": project, "datei": gaeb, "positionen": ["9.99"]}),
    )
    .unwrap_err();
    assert!(error.contains("fehlen die OZ 9.99"), "{error}");
    let answer = ok(
        &mut server,
        "stlb_uebernehmen",
        json!({"projekt": project, "datei": gaeb, "nach_oz": "01.01.0040", "positionen": ["1.30"]}),
    );
    assert!(answer.contains("01.01.0050"), "{answer}");
    assert!(answer.contains("[StLB 12345678901234567]"), "{answer}");

    // Prüfen/Aktualisieren: Prüfdatei schreiben, „STLB-Bau“ ändert Text und
    // Version (hier von Hand nachgestellt), Datei wieder einlesen.
    let check = folder.0.join("Pruefen.X81");
    let answer = ok(
        &mut server,
        "stlb_pruefdatei",
        json!({"projekt": project, "ziel": check}),
    );
    assert!(
        answer.contains("2 STLB-Bau-Position(en) als X81"),
        "{answer}"
    );
    let written = std::fs::read_to_string(&check).unwrap();
    assert_eq!(
        written.matches("<STLBBau>").count(),
        2,
        "nur STLB-Bau-Positionen"
    );
    assert!(
        !written.contains("Davor"),
        "freie Positionen bleiben draußen"
    );
    let updated = folder.0.join("Aktualisiert.X81");
    std::fs::write(
        &updated,
        written
            .replace(
                "<VersDate>2026-04</VersDate>",
                "<VersDate>2026-10</VersDate>",
            )
            .replace("Unterputzmontage", "Aufputzmontage"),
    )
    .unwrap();
    let preview = ok(
        &mut server,
        "stlb_aktualisieren",
        json!({"projekt": project, "datei": updated}),
    );
    assert!(
        preview.contains("1 geändert, 1 unverändert, 0 ohne Partner"),
        "{preview}"
    );
    assert!(preview.contains("01.01.0020"), "{preview}");
    assert!(
        preview.contains("Kennung STLB-Bau 4/2026 070 TA → STLB-Bau 10/2026 070 TA"),
        "{preview}"
    );
    assert!(preview.contains("Aufputzmontage"), "{preview}");
    assert!(preview.contains("Nichts geändert"), "{preview}");
    let answer = ok(
        &mut server,
        "stlb_aktualisieren",
        json!({"projekt": project, "datei": updated, "uebernehmen": true}),
    );
    assert!(answer.contains("1 Position(en) aktualisiert"), "{answer}");
    let position = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.01.0020"}),
    );
    assert!(position.contains("Aufputzmontage"), "{position}");
    assert!(
        position.contains("9 St") || position.contains("Menge: 9"),
        "Menge bleibt: {position}"
    );
}

#[test]
fn stlb_texts_are_write_protected_until_converted() {
    let folder = TempFolder::new("stlb-sperre");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Sperre");
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/proava-gaeb/tests/data/syn4_stlb_33.X83");
    let gaeb = folder.0.join("STLB-Export.X83");
    std::fs::copy(file, &gaeb).unwrap();
    let image = folder.0.join("bild.png");
    std::fs::write(&image, PNG).unwrap();
    ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "zeilen": [
            {"titel": "Gebäudeautomation", "ebene": 1},
            {"titel": "Raumautomation", "ebene": 2},
            {"kurztext": "Raumbediengerät folgt", "stlb": true, "menge": 9}
        ]}),
    );
    ok(
        &mut server,
        "stlb_uebernehmen",
        json!({"projekt": project, "datei": gaeb, "platzhalter_oz": "01.01.0010", "positionen": ["01.0010"]}),
    );
    let refused = |server: &mut Server, tool: &str, args: Value| {
        let error = call(server, tool, args).unwrap_err();
        assert!(error.contains("schreibgeschützt"), "{tool}: {error}");
        assert!(error.contains("in_freie_position"), "{tool}: {error}");
        assert!(error.contains("Nichts gespeichert"), "{tool}: {error}");
    };
    refused(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01.0010", "langtext": "Eigener Text"}),
    );
    refused(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01.0010", "einheit": "m", "menge": 3}),
    );
    refused(
        &mut server,
        "bieterangabe_einfuegen",
        json!({"projekt": project, "oz": "01.01.0010", "vortext": "Fabrikat:"}),
    );
    refused(
        &mut server,
        "bild_einfuegen",
        json!({"projekt": project, "oz": "01.01.0010", "datei": image}),
    );
    let lv = ok(&mut server, "lv_lesen", json!({"projekt": project}));
    assert!(lv.contains("| 9 St"), "nichts gespeichert: {lv}");

    // Free: Kurztext, Menge, the same unit, the value of a Textergänzung.
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01.0010", "kurztext": "RBG Büro", "menge": 3, "einheit": "St",
               "textergaenzungen": [{"kennung": 1, "art": "AG", "inhalt": "anthrazit"}]}),
    );
    let position = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.01.0010"}),
    );
    assert!(position.contains("RBG Büro"), "{position}");
    assert!(position.contains("anthrazit"), "{position}");
    assert!(position.contains("Unterputzmontage"), "{position}");
    let lv = ok(&mut server, "lv_lesen", json!({"projekt": project}));
    assert!(
        lv.contains("Pos STLB-Bau | RBG Büro | 3 St"),
        "kein STLB-Bau*: {lv}"
    );
    let error = call(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01.0010",
               "textergaenzungen": [{"kennung": 1, "art": "AG", "vortext": "Farbe: ", "inhalt": "rot"}]}),
    )
    .unwrap_err();
    assert!(error.contains("Nichts gespeichert"), "{error}");

    // Converted (and the Langtext changed in the same call): key gone.
    let answer = ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01.0010", "in_freie_position": true, "langtext": "Eigener Text", "einheit": "Stk"}),
    );
    assert!(answer.contains("in freie Position umgewandelt"), "{answer}");
    let lv = ok(&mut server, "lv_lesen", json!({"projekt": project}));
    assert!(lv.contains("Pos | RBG Büro | 3 Stk"), "{lv}");
    let position = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.01.0010"}),
    );
    assert!(position.contains("Eigener Text"), "{position}");
    assert!(position.contains("umgewandelt am"), "{position}");
    ok(
        &mut server,
        "bild_einfuegen",
        json!({"projekt": project, "oz": "01.01.0010", "datei": image}),
    );
    let error = call(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01.0010", "in_freie_position": true}),
    )
    .unwrap_err();
    assert!(error.contains("keine STLB-Bau-Kennung"), "{error}");
}

#[test]
fn several_lvs_and_master_data() {
    let folder = TempFolder::new("lvs");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Lose");
    ok(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": project, "lv_nummer": "1", "lv_titel": "Gebäudeautomation"}),
    );
    let answer = ok(
        &mut server,
        "lv_anlegen",
        json!({"projekt": project, "bezeichnung": "Elektro"}),
    );
    assert!(answer.contains("LV 2 – Elektro"), "{answer}");
    let doubled = call(
        &mut server,
        "lv_anlegen",
        json!({"projekt": project, "bezeichnung": "X", "nummer": "2"}),
    );
    assert!(doubled.unwrap_err().contains("schon"));

    // In LV 2 schreiben, LV 1 bleibt leer, proAVA zeigt weiter LV 1.
    ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "lv": "2", "kurztext": "Unterverteilung", "menge": 1, "einheit": "St"}),
    );
    let lvs = ok(&mut server, "lvs_lesen", json!({"projekt": project}));
    assert!(
        lvs.contains("1 | 1 | Gebäudeautomation | Ausschreibung | 11.22.PPPPI | 0 |"),
        "{lvs}"
    );
    assert!(
        lvs.contains("2 | 2 | Elektro | Ausschreibung | 11.22.PPPPI | 1 |"),
        "{lvs}"
    );
    assert!(
        lvs.lines().nth(1).unwrap().ends_with("aktiv in proAVA"),
        "{lvs}"
    );
    let first = ok(&mut server, "lv_lesen", json!({"projekt": project}));
    assert!(!first.contains("Unterverteilung"), "{first}");
    let second = ok(
        &mut server,
        "lv_lesen",
        json!({"projekt": project, "lv": 2}),
    );
    assert!(second.contains("Unterverteilung"), "{second}");
    let missing = call(
        &mut server,
        "lv_lesen",
        json!({"projekt": project, "lv": "7"}),
    )
    .unwrap_err();
    assert!(
        missing.contains("1 = LV 1 – Gebäudeautomation"),
        "{missing}"
    );
    let loaded = XmlProjectFile::new()
        .load_project(Path::new(&project))
        .unwrap();
    assert_eq!(loaded.lv_count(), 2);
    assert_eq!(loaded.active_lv(), 0);

    // Stammdaten mit Prüfung
    let answer = ok(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": project,
               "vergabeart": "Öffentliche Ausschreibung",
               "abgabetermin": "15.12.2026, 10:00 Uhr",
               "bindefrist": "01.12.2026",
               "ausfuehrung_beginn": "01.03.2027",
               "ausfuehrung_ende": "30.11.2027",
               "baustelle": {"name1": "Schule", "ort": "Musterstadt"},
               "adressen": [{"rolle": "Architekt", "name1": "Büro Plan"}],
               "sicherheit_vertragserfuellung": 5,
               "skonto_prozent": "2", "skonto_tage": 14,
               "abzuege": [{"bezeichnung": "Bauwesenversicherung", "prozent": 0.3}, {"bezeichnung": "Baustrom", "prozent": "0,5"}]}),
    );
    assert!(answer.contains("Hinweis: Die Bindefrist"), "{answer}");
    let wrong = call(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": project, "ausfuehrung_ende": "28.02.2027"}),
    );
    assert!(wrong.unwrap_err().contains("Ausführungsende"));
    let wrong = call(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": project, "bindefrist": "31.02.2027"}),
    );
    assert!(wrong.unwrap_err().contains("gibt es nicht"));
    let read = ok(&mut server, "projekt_lesen", json!({"projekt": project}));
    for expected in [
        "Vergabeart: Öffentliche Ausschreibung",
        "Baustelle: Schule, Musterstadt",
        "Adresse Architekt: Büro Plan",
        "Ausführungsende: 30.11.2027",
        "Skonto: 2 % in 14 Tagen",
        "Abzüge/Umlagen: Bauwesenversicherung 0,3 %, Baustrom 0,5 % (zusammen 0,8 %)",
        "LVs im Projekt: 2",
        "LV 1: „Gebäudeautomation“",
    ] {
        assert!(read.contains(expected), "{expected} fehlt:\n{read}");
    }
    let info = XmlProjectFile::new()
        .load_project(Path::new(&project))
        .unwrap()
        .info()
        .clone();
    assert_eq!(info.terms.cash_discount_days, Some(14));
    assert_eq!(info.contacts.len(), 1);
}

#[test]
fn positions_are_taken_over_from_another_project_and_the_library() {
    let folder = TempFolder::new("transfer");
    let mut server = server(&folder.0);
    let library = new_project(&mut server, &folder.0, "Bibliothek");
    ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": library, "zeilen": [
            {"titel": "Raumautomation"}, {"titel": "Raumbediengeräte", "ebene": 2},
            {"kurztext": "Raumbediengerät mit Display", "menge": 3, "einheit": "St", "ep": 250},
            {"kurztext": "Präsenzmelder", "menge": 2, "einheit": "St", "ep": 90}
        ]}),
    );
    let target = new_project(&mut server, &folder.0, "Ziel");
    ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": target, "zeilen": [
            {"titel": "Bestand"}, {"titel": "Alt", "ebene": 2},
            {"kurztext": "Alt", "menge": 1, "einheit": "St"}
        ]}),
    );
    let found = ok(
        &mut server,
        "bibliothek_lesen",
        json!({"bibliothek": library, "suche": "präsenz"}),
    );
    assert!(
        found.contains("01.01.0020 | Pos | Präsenzmelder"),
        "{found}"
    );
    assert!(
        found.contains("01 | T1 | Raumautomation"),
        "titles above a hit: {found}"
    );
    assert!(!found.contains("Display"), "{found}");

    let answer = ok(
        &mut server,
        "positionen_uebernehmen",
        json!({"projekt": target, "quelle": library, "oz": ["01"], "mengen": false}),
    );
    assert!(
        answer.contains("4 Zeile(n) übernommen (2 Positionen)"),
        "{answer}"
    );
    let lv = ok(&mut server, "lv_lesen", json!({"projekt": target}));
    assert!(
        lv.contains("02.01.0010 | Pos | Raumbediengerät mit Display"),
        "{lv}"
    );
    assert!(!lv.contains("250"), "prices left out: {lv}");

    let answer = ok(
        &mut server,
        "positionen_uebernehmen",
        json!({"projekt": target, "quelle": library, "oz": ["1.1.20"], "unter_titel": "01.01", "preise": true}),
    );
    assert!(answer.contains("1 Zeile(n)"), "{answer}");
    let lv = ok(&mut server, "lv_lesen", json!({"projekt": target}));
    assert!(
        lv.contains("01.01.0020 | Pos | Präsenzmelder | 2 St | 90,00"),
        "{lv}"
    );
    assert!(call(
        &mut server,
        "positionen_uebernehmen",
        json!({"projekt": target, "quelle": library, "oz": ["09"]})
    )
    .is_err());
}

#[test]
fn lv_search_finds_positions_in_projects_folders_and_gaeb_and_they_are_taken_over() {
    let folder = TempFolder::new("lvsuche");
    let mut server = server(&folder.0);
    let projects = folder.0.join("Projekte");
    std::fs::create_dir_all(projects.join("00- Alte Versionen")).unwrap();
    std::fs::create_dir_all(projects.join(".versteckt")).unwrap();
    let school = new_project(&mut server, &projects, "Schule");
    ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": school, "zeilen": [
            {"titel": "Raumautomation"}, {"titel": "Raumbediengeräte", "ebene": 2},
            {"kurztext": "Raumbediengerät KNX", "menge": 24, "einheit": "St", "ep": 185,
             "langtext": "Liefern und montieren: Taster mit Display für KNX, Farbe reinweiß, Temperaturfühler eingebaut."},
            {"kurztext": "Präsenzmelder", "menge": 10, "einheit": "St", "ep": 95},
            {"hinweis": "Alle Bediengeräte in Farbe reinweiß wie das Schalterprogramm."},
            {"titel": "Kabel", "ebene": 1}, {"titel": "Busleitungen", "ebene": 2},
            {"kurztext": "Busleitung", "menge": 400, "einheit": "m", "ep": 2, "langtext": "J-Y(St)Y 2x2x0,8"}
        ]}),
    );
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": school, "oz": "01", "kg": "481"}),
    );
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": school, "oz": "02", "kg": "484"}),
    );
    ok(
        &mut server,
        "bieterangabe_einfuegen",
        json!({"projekt": school, "oz": "01.01.0010", "vortext": "Fabrikat Raumbediengerät:"}),
    );
    let office = new_project(&mut server, &projects, "Buero");
    ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": office, "zeilen": [
            {"titel": "Feldgeräte"},
            {"kurztext": "Raumbediengerät mit Display", "menge": 3, "einheit": "St", "langtext": "Bus KNX, Glasfront"}
        ]}),
    );
    // An old version and a hidden copy are not searched, a broken file is named.
    std::fs::copy(
        &school,
        projects.join("00- Alte Versionen").join("Schule_V1.proava"),
    )
    .unwrap();
    std::fs::copy(&school, projects.join(".versteckt").join("Kopie.proava")).unwrap();
    std::fs::write(projects.join("Kaputt.proava"), b"PK\x03\x04kaputt").unwrap();

    let found = ok(
        &mut server,
        "lv_suchen",
        json!({"ordner": projects, "suche": "RAUMBEDIENGERAET knx"}),
    );
    assert!(
        found.contains("in 3 Projekten unter") && found.contains("2 Treffer in 2 Dateien"),
        "{found}"
    );
    assert!(found.contains("=== Projekt „Buero“"), "{found}");
    assert!(
        found.contains("01.01.0010 | Pos | Raumbediengerät KNX | 24 St | EP 185,00 | KG 481 | #"),
        "{found}"
    );
    assert!(
        !found.contains("| Titel |"),
        "a title matches by its own text: {found}"
    );
    assert!(
        found.contains("in: 01 Raumautomation › 01.01 Raumbediengeräte"),
        "{found}"
    );
    assert!(
        found.contains("Langtext: „…Bus KNX, Glasfront“")
            || found.contains("Langtext: „Bus KNX, Glasfront“"),
        "{found}"
    );
    assert!(found.contains("quelle_lv: \"1\""), "{found}");
    assert!(
        !found.contains("Schule_V1") && !found.contains("Kopie.proava"),
        "{found}"
    );
    assert!(
        found.contains("Übersprungen, nicht lesbar: 1 Datei") && found.contains("Kaputt.proava"),
        "{found}"
    );
    assert!(found.contains("positionen_uebernehmen"), "{found}");
    let with_old = ok(
        &mut server,
        "lv_suchen",
        json!({"ordner": projects, "suche": "präsenzmelder", "mit_alten_versionen": true}),
    );
    assert!(with_old.contains("2 Treffer in 2 Dateien"), "{with_old}");

    // One project: long text, hint, bidder field, filters.
    let in_school = ok(
        &mut server,
        "lv_suchen",
        json!({"projekt": school, "suche": "reinweiss"}),
    );
    assert!(in_school.contains("2 Treffer in 1 Datei"), "{in_school}");
    assert!(
        in_school.contains("| Hinweis | Alle Bediengeräte"),
        "{in_school}"
    );
    assert!(
        in_school.contains("Farbe reinweiß, Temperaturfühler eingebaut.“"),
        "{in_school}"
    );
    let title = ok(
        &mut server,
        "lv_suchen",
        json!({"projekt": school, "suche": "raumbediengeräte", "lv": "1"}),
    );
    assert!(
        title.contains("01.01 | Titel | Raumbediengeräte (2 Positionen) | KG 481 | #"),
        "{title}"
    );
    let field = ok(
        &mut server,
        "lv_suchen",
        json!({"projekt": school, "suche": "fabrikat"}),
    );
    assert!(
        field.contains("Bieterangabe: „Fabrikat Raumbediengerät:“"),
        "{field}"
    );
    let cables = ok(
        &mut server,
        "lv_suchen",
        json!({"projekt": school, "kg": "48", "einheit": "m"}),
    );
    assert!(
        cables.contains("1 Treffer") && cables.contains("Busleitung | 400 m"),
        "{cables}"
    );
    let nothing = ok(
        &mut server,
        "lv_suchen",
        json!({"projekt": school, "suche": "lüftungsgerät"}),
    );
    assert!(nothing.contains("keine Treffer"), "{nothing}");
    assert!(!nothing.contains("positionen_uebernehmen"), "{nothing}");

    // A GAEB file (older LV from another program).
    let x83 = folder.0.join("Alt.X83");
    ok(
        &mut server,
        "gaeb_exportieren",
        json!({"projekt": school, "art": "X83", "ziel": x83}),
    );
    let in_gaeb = ok(
        &mut server,
        "lv_suchen",
        json!({"datei": x83, "suche": "präsenzmelder"}),
    );
    assert!(
        in_gaeb.contains("=== Datei") && in_gaeb.contains("Präsenzmelder | 10 St"),
        "{in_gaeb}"
    );

    // Wrong calls.
    for (arguments, expected) in [
        (json!({"suche": "knx"}), "genau eines"),
        (
            json!({"ordner": projects, "projekt": school, "suche": "knx"}),
            "genau eines",
        ),
        (json!({"ordner": projects}), "suche"),
        (json!({"ordner": "/etc", "suche": "knx"}), "außerhalb"),
        (json!({"projekt": x83, "suche": "knx"}), "datei"),
        (json!({"datei": school, "suche": "knx"}), "projekt"),
    ] {
        let error = call(&mut server, "lv_suchen", arguments.clone()).unwrap_err();
        assert!(error.contains(expected), "{arguments}: {error}");
    }

    // Take over what was found: a hint by its id, a position by its OZ.
    let note_id: u64 = in_school
        .lines()
        .find(|line| line.contains("| Hinweis |"))
        .and_then(|line| line.rsplit('#').next())
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let target = new_project(&mut server, &folder.0, "Ziel");
    let answer = ok(
        &mut server,
        "positionen_uebernehmen",
        json!({"projekt": target, "quelle": school, "quelle_lv": "1", "oz": ["01.01.0010"], "ids": [note_id]}),
    );
    assert!(
        answer.contains("2 Zeile(n) übernommen (1 Positionen)"),
        "{answer}"
    );
    let lv = ok(&mut server, "lv_lesen", json!({"projekt": target}));
    assert!(
        lv.contains("Raumbediengerät KNX") && lv.contains("Alle Bediengeräte"),
        "{lv}"
    );
    let gaeb_id = in_gaeb
        .lines()
        .find(|line| line.contains("| Pos |"))
        .and_then(|line| line.rsplit('#').next())
        .unwrap()
        .trim()
        .to_string();
    let answer = ok(
        &mut server,
        "positionen_uebernehmen",
        json!({"projekt": target, "quelle": x83, "ids": [format!("#{gaeb_id}")]}),
    );
    assert!(
        answer.contains("1 Zeile(n) übernommen (1 Positionen)"),
        "{answer}"
    );
    assert!(answer.contains("Präsenzmelder"), "{answer}");
    let error = call(
        &mut server,
        "positionen_uebernehmen",
        json!({"projekt": target, "quelle": school, "ids": [9999]}),
    )
    .unwrap_err();
    assert!(error.contains("#9999"), "{error}");
    let error = call(
        &mut server,
        "positionen_uebernehmen",
        json!({"projekt": target, "quelle": school}),
    )
    .unwrap_err();
    assert!(error.contains("ids"), "{error}");
}

#[test]
fn bidder_list_links_bids_and_prints_the_submission_record() {
    let folder = TempFolder::new("bieter");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Vergabe");
    ok(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": project, "abgabetermin": "15.12.2026, 10:00 Uhr"}),
    );
    ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "kurztext": "Fühler", "menge": 10, "einheit": "St", "ep": 80}),
    );
    let answer = ok(
        &mut server,
        "bieter_anlegen",
        json!({"projekt": project, "name1": "Müller Elektro GmbH", "ort": "Musterstadt", "angefordert": "01.11.2026"}),
    );
    assert!(answer.contains("#1"), "{answer}");
    ok(
        &mut server,
        "bieter_anlegen",
        json!({"projekt": project, "name1": "Absage AG", "status": "kein Angebot"}),
    );
    let wrong = call(
        &mut server,
        "bieter_anlegen",
        json!({"projekt": project, "name1": "X", "angefordert": "31.11.2026"}),
    );
    assert!(wrong.unwrap_err().contains("gibt es nicht"));

    // Angebot einlesen (X84 aus dem eigenen LV mit Preisen) und verknüpfen
    let bid_file = folder.0.join("mueller.D84");
    write_d84(&project, &bid_file, "Müller-Elektro", "1.1");
    ok(
        &mut server,
        "angebot_einlesen",
        json!({"projekt": project, "datei": bid_file, "ins_bieterverzeichnis": false}),
    );
    let list = ok(&mut server, "bieter_lesen", json!({"projekt": project}));
    assert!(list.contains("→ passt zu #1"), "{list}");
    ok(
        &mut server,
        "bieter_aendern",
        json!({"projekt": project, "bieter": 1, "angebot": 1, "nebenangebote": true, "eingang": "14.12.2026, 09:30 Uhr"}),
    );
    let list = ok(&mut server, "bieter_lesen", json!({"projekt": project}));
    assert!(list.contains("#1 | Müller Elektro GmbH | Musterstadt |  |  | 01.11.2026 | Angebot abgegeben | Angebot 1"), "{list}");
    assert!(list.contains("#2 | Absage AG"), "{list}");

    let answer = ok(
        &mut server,
        "submission_drucken",
        json!({"projekt": project}),
    );
    let pdf = folder
        .0
        .join("Vergabe/Vergabe_Niederschrift_Submission.pdf");
    assert!(answer.contains("Niederschrift"), "{answer}");
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));
    let again = call(
        &mut server,
        "submission_drucken",
        json!({"projekt": project}),
    );
    assert!(again.is_err(), "überschreibt nicht ohne ueberschreiben");
    let removed = ok(
        &mut server,
        "bieter_aendern",
        json!({"projekt": project, "bieter": 2, "entfernen": true}),
    );
    assert!(removed.contains("entfernt"));

    // Anwender 18.11.: Standard – vorhandener Bieter in anderer Schreibweise wird
    // zugeordnet, nicht doppelt angelegt.
    ok(
        &mut server,
        "bieter_anlegen",
        json!({"projekt": project, "name1": "Schmidt Regeltechnik GmbH"}),
    );
    let second = folder.0.join("schmidt.D84");
    write_d84(&project, &second, "Schmidt-Regeltechnik", "1.2");
    let answer = ok(
        &mut server,
        "angebot_einlesen",
        json!({"projekt": project, "datei": second}),
    );
    assert!(
        answer.contains("Bieterverzeichnis: Bieter #") && answer.contains("zugeordnet"),
        "{answer}"
    );
    let list = ok(&mut server, "bieter_lesen", json!({"projekt": project}));
    assert_eq!(
        list.matches("Schmidt").count(),
        1,
        "kein zweiter Eintrag: {list}"
    );
}

#[test]
fn letters_to_bidders_as_one_pdf_or_one_per_bidder() {
    let folder = TempFolder::new("anschreiben");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Briefe");
    ok(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": project, "abgabetermin": "15.12.2026, 10:00 Uhr", "aufsteller": {"name1": "Planungsbüro Muster", "ort": "Musterstadt"}}),
    );
    ok(
        &mut server,
        "bieter_anlegen",
        json!({"projekt": project, "name1": "Müller GmbH", "ort": "Musterstadt"}),
    );
    ok(
        &mut server,
        "bieter_anlegen",
        json!({"projekt": project, "name1": "Schmidt & Söhne KG"}),
    );
    let answer = ok(
        &mut server,
        "anschreiben_drucken",
        json!({"projekt": project, "art": "aufforderung"}),
    );
    assert!(answer.contains("an 2 Bieter"), "{answer}");
    assert!(answer.contains("Versanddatum"), "{answer}");
    let iso = chrono::Local::now().format("%Y-%m-%d").to_string();
    assert!(
        folder
            .0
            .join(format!("Briefe/Briefe_Aufforderung_{iso}.pdf"))
            .is_file(),
        "{answer}"
    );
    let list = ok(&mut server, "bieter_lesen", json!({"projekt": project}));
    let today = chrono::Local::now().format("%d.%m.%Y").to_string();
    assert!(list.contains(&format!("| {today} |")), "{list}");

    let out = folder.0.join("einzeln");
    std::fs::create_dir_all(&out).unwrap();
    let answer = ok(
        &mut server,
        "anschreiben_drucken",
        json!({"projekt": project, "art": "aufforderung", "einzeln": true, "ziel": out}),
    );
    assert!(
        out.join(format!("Briefe_Aufforderung_{iso}_Müller_GmbH.pdf"))
            .is_file(),
        "{answer}"
    );
    assert!(
        out.join(format!("Briefe_Aufforderung_{iso}_Schmidt___Söhne_KG.pdf"))
            .is_file(),
        "{answer}"
    );
    let again = call(
        &mut server,
        "anschreiben_drucken",
        json!({"projekt": project, "art": "aufforderung", "einzeln": true, "ziel": out}),
    );
    assert!(again.is_err(), "überschreibt nicht");

    let award = call(
        &mut server,
        "anschreiben_drucken",
        json!({"projekt": project, "art": "zuschlag"}),
    );
    assert!(award.unwrap_err().contains("Kein Empfänger"));
    let wrong = call(
        &mut server,
        "anschreiben_drucken",
        json!({"projekt": project, "art": "brief"}),
    );
    assert!(wrong.is_err());
}

/// Schriftverkehr-Archiv: anschreiben_drucken files each recipient's letter
/// in the project (ablegen, default true); schriftverkehr_lesen lists them,
/// schriftverkehr_pdf_speichern writes one PDF.
#[test]
fn letters_are_filed_in_the_project_and_listed() {
    let folder = TempFolder::new("schriftverkehr");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Archiv");
    ok(
        &mut server,
        "bieter_anlegen",
        json!({"projekt": project, "name1": "Müller GmbH"}),
    );
    ok(
        &mut server,
        "bieter_anlegen",
        json!({"projekt": project, "name1": "Schmidt | Söhne KG"}),
    );
    let empty = ok(
        &mut server,
        "schriftverkehr_lesen",
        json!({"projekt": project}),
    );
    assert!(
        empty.contains("0 Schreiben") && empty.contains("Noch nichts abgelegt"),
        "{empty}"
    );

    // Sammel-PDF: one entry per bidder.
    let answer = ok(
        &mut server,
        "anschreiben_drucken",
        json!({"projekt": project, "art": "aufforderung"}),
    );
    assert!(
        answer.contains("2 Schreiben je Empfänger im Projekt abgelegt"),
        "{answer}"
    );
    let loaded = XmlProjectFile::new()
        .load_project(Path::new(&project))
        .unwrap();
    assert_eq!(loaded.correspondence().len(), 2);
    assert!(loaded
        .correspondence()
        .iter()
        .all(|entry| entry.pdf.starts_with(b"%PDF")));
    assert!(loaded
        .invitees()
        .iter()
        .all(|invitee| !invitee.requested.is_empty()));

    // Without ablegen nothing is filed (and absage needs no save then).
    let out = folder.0.join("einzeln");
    std::fs::create_dir_all(&out).unwrap();
    let answer = ok(
        &mut server,
        "anschreiben_drucken",
        json!({"projekt": project, "art": "absage", "alle": true, "einzeln": true, "ziel": out, "ablegen": false}),
    );
    assert!(!answer.contains("abgelegt"), "{answer}");
    let answer = ok(
        &mut server,
        "anschreiben_drucken",
        json!({"projekt": project, "art": "absage", "bieter": 2}),
    );
    assert!(
        answer.contains("1 Schreiben im Projekt abgelegt"),
        "{answer}"
    );

    let list = ok(
        &mut server,
        "schriftverkehr_lesen",
        json!({"projekt": project}),
    );
    assert!(
        list.starts_with("Schriftverkehr im Projekt: 3 Schreiben"),
        "{list}"
    );
    let row = list
        .lines()
        .find(|line| line.starts_with("| 3 |"))
        .expect(&list);
    assert!(
        row.contains("| Absage |") && row.contains("Schmidt / Söhne KG (Bieter #2)"),
        "{row}"
    );
    let only = ok(
        &mut server,
        "schriftverkehr_lesen",
        json!({"projekt": project, "art": "absage"}),
    );
    assert!(only.contains("nach Filter 1"), "{only}");
    let only = ok(
        &mut server,
        "schriftverkehr_lesen",
        json!({"projekt": project, "empfaenger": "müller"}),
    );
    assert!(
        only.contains("nach Filter 1") && only.contains("| 1 |"),
        "{only}"
    );
    assert!(call(
        &mut server,
        "schriftverkehr_lesen",
        json!({"projekt": project, "art": "brief"})
    )
    .is_err());

    let answer = ok(
        &mut server,
        "schriftverkehr_pdf_speichern",
        json!({"projekt": project, "id": 3}),
    );
    let saved = folder
        .0
        .join("Archiv")
        .join("0003_Absage_Schmidt_Söhne_KG.pdf");
    assert!(answer.contains("gespeichert"), "{answer}");
    let loaded = XmlProjectFile::new()
        .load_project(Path::new(&project))
        .unwrap();
    assert_eq!(
        std::fs::read(&saved).unwrap(),
        loaded.correspondence()[2].pdf.as_slice()
    );
    assert!(
        call(
            &mut server,
            "schriftverkehr_pdf_speichern",
            json!({"projekt": project, "id": 3})
        )
        .is_err(),
        "überschreibt nicht"
    );
    assert!(call(
        &mut server,
        "schriftverkehr_pdf_speichern",
        json!({"projekt": project, "id": 9})
    )
    .unwrap_err()
    .contains("#9"));
}

#[test]
fn spelling_lists_unknown_words_with_places_and_suggestions() {
    let folder = TempFolder::new("spell");
    let aff = "SET UTF-8\nTRY esianrtolcdugmphbyfvkwzESIANRTOLCDUGMPHBYFVKWZüÜ\n";
    let dic = "5\nFühler\nliefern\nund\nmontieren\nHaus\n";
    let speller = proava_spell::Speller::from_text(aff, dic, &["Hausgerät".to_string()]).unwrap();
    let mut server = server(&folder.0).with_speller(speller);
    let project = new_project(&mut server, &folder.0, "Spell");

    let answer = ok(
        &mut server,
        "rechtschreibung_pruefen",
        json!({"projekt": project}),
    );
    assert!(answer.contains("Keine unbekannten Wörter"), "{answer}");

    ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "kurztext": "Fühler montieren",
        "menge": 1, "einheit": "St", "langtext": "Liefern und montieren: Fühlr, DDC, BACnet, Pt1000 und Hausgerät"}),
    );
    ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "kurztext": "Fühlr", "menge": 1, "einheit": "St"}),
    );
    let answer = ok(
        &mut server,
        "rechtschreibung_pruefen",
        json!({"projekt": project}),
    );
    assert!(answer.starts_with("1 unbekanntes Wort im LV"), "{answer}");
    let line = answer
        .lines()
        .find(|line| line.starts_with("- Fühlr"))
        .expect(&answer);
    assert!(line.contains("(2×)"), "{line}");
    assert!(
        line.contains("Langtext") && line.contains("Kurztext"),
        "{line}"
    );
    assert!(
        line.contains("→ ") && line.contains("Fühler"),
        "Vorschlag: {line}"
    );

    let answer = ok(
        &mut server,
        "rechtschreibung_pruefen",
        json!({"projekt": project, "vorschlaege": 0}),
    );
    assert!(!answer.contains('→'), "ohne Vorschläge: {answer}");
}

#[test]
fn two_lvs_are_compared() {
    let folder = TempFolder::new("compare");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Vergleich");
    ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "zeilen": [
            {"titel": "Kabel"}, {"titel": "Leitungen", "ebene": 2},
            {"kurztext": "Kabelabzweigkasten", "menge": 652, "einheit": "St"},
            {"kurztext": "Ventilantrieb", "menge": 4, "einheit": "St"}
        ]}),
    );
    ok(
        &mut server,
        "lv_anlegen",
        json!({"projekt": project, "bezeichnung": "Fassung 2", "nummer": "F2"}),
    );
    ok(
        &mut server,
        "positionen_uebernehmen",
        json!({"projekt": project, "lv": "F2", "quelle": project, "quelle_lv": "1", "oz": ["01"]}),
    );
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "lv": "F2", "oz": "01.01.0010", "menge": 150}),
    );
    ok(
        &mut server,
        "zeile_loeschen",
        json!({"projekt": project, "lv": "F2", "oz": "01.01.0020"}),
    );
    let answer = ok(
        &mut server,
        "lv_vergleichen",
        json!({"projekt": project, "lv": "F2", "vergleich_lv": "1"}),
    );
    assert!(
        answer.contains("geändert | Position | 01.01.0010 | Kabelabzweigkasten | Menge: 652 → 150"),
        "{answer}"
    );
    assert!(
        answer.contains("entfallen | Position | 01.01.0020 | Ventilantrieb"),
        "{answer}"
    );
    let only = ok(
        &mut server,
        "lv_vergleichen",
        json!({"projekt": project, "lv": "F2", "vergleich_lv": "1", "nur": "entfallen"}),
    );
    assert!(!only.contains("Kabelabzweigkasten"), "{only}");
    assert!(call(&mut server, "lv_vergleichen", json!({"projekt": project})).is_err());
}

/// Abnahme mit einem echten LV: was ein von Hand per MCP geschriebenes LV braucht, um
/// mit dem Original deckungsgleich zu sein – Ebenenbezeichnungen, EP-Anteile,
/// Hinweise mit Kurztext (auch ohne Text), Ausführungsbeschreibungen mit
/// Bezug, leere Absätze am Textende und Untertitel per positionen_einfuegen.
#[test]
fn hand_built_lv_carries_headings_shares_and_performance_descriptions() {
    let folder = TempFolder::new("abnahme");
    let mut server = server(&folder.0);
    let answer = ok(
        &mut server,
        "projekt_anlegen",
        json!({"ordner": folder.0, "name": "Abnahme", "oz_maske": "111.22.PPPP", "schrittweite": 1}),
    );
    assert!(answer.contains("Projekt angelegt"), "{answer}");
    let project = folder
        .0
        .join("Abnahme")
        .join("Abnahme.proava")
        .to_string_lossy()
        .to_string();
    ok(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": project, "ebenen": ["LOS", "Titel"], "ep_anteile": ["Lohn", "Material", "Gerät", "Sonstiges"]}),
    );
    let error = call(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": project, "ebenen": ["A", "B", "C"]}),
    )
    .unwrap_err();
    assert!(error.contains("nur 2 Titelebene"), "{error}");
    let overview = ok(&mut server, "projekt_lesen", json!({"projekt": project}));
    assert!(
        overview.contains("Ebenen: LOS, Titel · EP-Anteile: Lohn, Material, Gerät, Sonstiges"),
        "{overview}"
    );

    ok(
        &mut server,
        "titel_einfuegen",
        json!({"projekt": project, "kurztext": "Kabel"}),
    );
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "001", "neue_oz": "484"}),
    );
    ok(
        &mut server,
        "titel_einfuegen",
        json!({"projekt": project, "kurztext": "Hardware", "unter_titel": "484"}),
    );
    // Hinweis nur mit Kurztext, Hinweis mit beidem
    let answer = ok(
        &mut server,
        "hinweis_einfuegen",
        json!({"projekt": project, "unter_titel": "484.01", "kurztext": "Hinweis: nachfolgende Kabel"}),
    );
    assert!(answer.contains("Hinweis eingefügt"), "{answer}");
    assert!(call(
        &mut server,
        "hinweis_einfuegen",
        json!({"projekt": project})
    )
    .is_err());
    // text „“ = leerer Hinweis (wie aus GAEB-Dateien): geht, die Prüfung meldet ihn;
    // die Leerzeile ohne Warnung ist der Abstandshalter.
    let answer = ok(
        &mut server,
        "hinweis_einfuegen",
        json!({"projekt": project, "unter_titel": "484.01", "text": ""}),
    );
    assert!(
        answer.contains("Hinweis eingefügt") && answer.contains("abstand_einfuegen"),
        "{answer}"
    );
    let id = answer
        .rsplit("(#")
        .next()
        .unwrap()
        .split(')')
        .next()
        .unwrap()
        .to_string();
    let check = ok(&mut server, "lv_pruefen", json!({"projekt": project}));
    assert!(check.contains(&format!("Hinweis ohne Text (für eine Leerzeile in der Bearbeitung: abstand_einfuegen) (1): #{id}")), "{check}");
    ok(
        &mut server,
        "zeile_loeschen",
        json!({"projekt": project, "id": id.parse::<u64>().unwrap()}),
    );
    let answer = ok(
        &mut server,
        "abstand_einfuegen",
        json!({"projekt": project, "unter_titel": "484.01"}),
    );
    assert!(
        answer.starts_with("Abstandshalter eingefügt hinter Hinweis „Hinweis: nachfolgende Kabel“"),
        "{answer}"
    );
    assert!(
        answer.contains("erscheint weder im Druck noch in GAEB oder CSV"),
        "{answer}"
    );
    let spacer = answer
        .rsplit("(#")
        .next()
        .unwrap()
        .split(')')
        .next()
        .unwrap()
        .to_string();
    let lv = ok(&mut server, "lv_lesen", json!({"projekt": project}));
    assert!(
        lv.contains(&format!(" | Abstand | | | | | #{spacer}")),
        "{lv}"
    );
    let row = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "id": spacer.parse::<u64>().unwrap()}),
    );
    assert!(
        row.starts_with(&format!("Abstandshalter (#{spacer})")),
        "{row}"
    );
    let error = call(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "id": spacer.parse::<u64>().unwrap(), "kurztext": "x"}),
    )
    .unwrap_err();
    assert!(error.contains("weder OZ noch Text"), "{error}");
    ok(
        &mut server,
        "hinweis_einfuegen",
        json!({"projekt": project, "unter_titel": "484.01", "kurztext": "Vorbemerkung", "text": "Gilt für alle Kabel.\n\n"}),
    );
    let answer = ok(
        &mut server,
        "ausfuehrungsbeschreibung_einfuegen",
        json!({"projekt": project, "unter_titel": "484.01", "nummer": "2", "bezeichnung": "Busleitungen", "langtext": "Für Feldbus-Anbindungen:\n- Lüftungsgerät\n- Zähler\n\n"}),
    );
    assert!(
        answer.contains("Ausführungsbeschreibung A0002 eingefügt")
            && answer.contains("ausfuehrungsbeschreibung: „A0002“"),
        "{answer}"
    );
    let error = call(
        &mut server,
        "ausfuehrungsbeschreibung_einfuegen",
        json!({"projekt": project, "nummer": "2", "bezeichnung": "doppelt"}),
    )
    .unwrap_err();
    assert!(error.contains("schon eine andere"), "{error}");
    let error = call(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "unter_titel": "484.01", "kurztext": "X", "ausfuehrungsbeschreibung": "7"}),
    )
    .unwrap_err();
    assert!(
        error.contains("vorhanden: A0002")
            && error.contains("Ausführungsbeschreibung A0007 gibt es"),
        "{error}"
    );
    ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "unter_titel": "484.01", "kurztext": "Busleitung geschirmt\nauf Trasse",
               "langtext": "Busleitung geschirmt\n\n", "menge": 120, "einheit": "m", "ep": 3.8,
               "ausfuehrungsbeschreibung": "AB 0002"}),
    );
    // Untertitel per positionen_einfuegen hinter den vorhandenen
    let answer = ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "unter_titel": "484", "zeilen": [
            {"titel": "Dienstleistungen", "ebene": 2},
            {"hinweis": "", "kurztext": "Hinweis zur Montage"},
            {"kurztext": "Leitungen anschließen", "menge": 6, "einheit": "St", "ep": 55, "ausfuehrungsbeschreibung": "A0002"}
        ]}),
    );
    assert!(answer.contains("484.02 „Dienstleistungen“"), "{answer}");
    assert!(answer.contains("484.02.0001"), "{answer}");

    let check = ok(&mut server, "lv_pruefen", json!({"projekt": project}));
    assert!(
        !check.contains("Ohne OZ") && !check.contains("#"),
        "Ausführungsbeschreibungen brauchen keine OZ: {check}"
    );
    let position = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "484.01.0001"}),
    );
    assert!(
        position.contains("Ausführungsbeschreibung: Ausführungsbeschreibung A0002 – Busleitungen"),
        "{position}"
    );
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "484.02.0001", "ausfuehrungsbeschreibung": ""}),
    );

    // Export: alles kommt im GAEB an
    let target = folder.0.join("Abnahme.X82");
    ok(
        &mut server,
        "gaeb_exportieren",
        json!({"projekt": project, "art": "X82", "ziel": target}),
    );
    let xml = std::fs::read_to_string(&target).unwrap();
    for expected in [
        "<LblBoQBkdn>LOS</LblBoQBkdn>",
        "<LblBoQBkdn>Titel</LblBoQBkdn>",
        "<NoUPComps>4</NoUPComps>",
        "<LblUPComp3 Type=\"Plant\">Gerät</LblUPComp3>",
        "<span>Hinweis: nachfolgende Kabel</span>",
        "<PerfNo>2</PerfNo>",
        "<PerfLbl>Busleitungen</PerfLbl>",
        "<CurLbl>Euro</CurLbl>",
        "<TotalGross>",
    ] {
        assert!(xml.contains(expected), "{expected} fehlt:\n{xml}");
    }
    assert!(
        !xml.contains("<Type>Lot</Type>"),
        "LOS bleibt eine LV-Stufe:\n{xml}"
    );
    assert_eq!(xml.matches("<RefPerfNo").count(), 1, "{xml}");
    // leere Absätze am Ende: Hinweis, Ausführungsbeschreibung, Position
    assert!(
        xml.contains("Gilt für alle Kabel.</span></p><p/></Text>"),
        "{xml}"
    );
    assert!(xml.contains("</ul><p/></Text>"), "{xml}");
    assert!(
        xml.contains("Busleitung geschirmt</span></p><p/></Text>"),
        "{xml}"
    );
    let gaeb = proava_gaeb::xml::read(xml.as_bytes()).unwrap();
    let ordinals: Vec<String> = gaeb
        .award
        .boq
        .items()
        .into_iter()
        .map(|(path, _)| path.join("."))
        .collect();
    assert_eq!(ordinals, ["484.01.0001", "484.02.0001"]);
}

#[test]
fn gaeb_import_as_lv_or_project_with_preview_prices_and_protocol() {
    let folder = TempFolder::new("gaebimport");
    let mut server = server(&folder.0);
    let source = new_project(&mut server, &folder.0, "Quelle");
    ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": source, "zeilen": [
            {"titel": "Heizung"},
            {"kurztext": "Konvektor", "menge": 3, "einheit": "St", "ep": 400},
            {"kurztext": "Ventil", "menge": 3, "einheit": "St", "ep": 50}
        ]}),
    );
    ok(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": source, "lv_nummer": "5"}),
    );
    let x82 = folder.0.join("Kostenanschlag.X82");
    ok(
        &mut server,
        "gaeb_exportieren",
        json!({"projekt": source, "art": "X82", "ziel": x82}),
    );
    let d81 = folder.0.join("Vortexte.D81");
    ok(
        &mut server,
        "gaeb_exportieren",
        json!({"projekt": source, "art": "X81", "ziel": d81}),
    );

    let preview = ok(
        &mut server,
        "gaeb_importieren",
        json!({"datei": x82, "vorschau": true}),
    );
    assert!(
        preview.contains("\nArt: X82 – Bepreistes LV (GAEB: Kostenanschlag)\n"),
        "{preview}"
    );
    assert!(
        preview.contains("\nGespeichert: nichts (vorschau)"),
        "{preview}"
    );
    assert!(
        preview.lines().all(|line| !line.contains(" · ")),
        "eine Angabe je Zeile: {preview}"
    );

    let target = new_project(&mut server, &folder.0, "Ziel");
    ok(
        &mut server,
        "titel_einfuegen",
        json!({"projekt": target, "kurztext": "Vorhanden"}),
    );
    let answer = ok(
        &mut server,
        "gaeb_importieren",
        json!({"datei": x82, "projekt": target}),
    );
    assert!(
        answer.contains("Importprotokoll Kostenanschlag.X82"),
        "{answer}"
    );
    assert!(
        answer.contains("\nPreise: 2 Einheitspreise als Planerpreise\n"),
        "{answer}"
    );
    assert!(answer.contains("\nLV netto: 1.350,00 €\n"), "{answer}");
    assert!(
        answer.lines().all(|line| !line.contains(" · ")),
        "eine Angabe je Zeile: {answer}"
    );
    assert!(answer.contains("lv: „5“"), "{answer}");
    let error = call(
        &mut server,
        "gaeb_importieren",
        json!({"datei": x82, "projekt": target, "nummer": "5"}),
    )
    .unwrap_err();
    assert!(error.contains("gibt es im Projekt schon"), "{error}");
    let without = ok(
        &mut server,
        "gaeb_importieren",
        json!({"datei": d81, "projekt": target, "nummer": "6", "bezeichnung": "Vortexte", "preise": false}),
    );
    assert!(without.contains("D81 – Leistungsbeschreibung"), "{without}");
    assert!(
        without.contains("\nPreise: keine in der Datei\n"),
        "{without}"
    );
    let lvs = ok(&mut server, "lvs_lesen", json!({"projekt": target}));
    assert!(
        lvs.contains("| 5 |") && lvs.contains("| 6 | Vortexte |"),
        "{lvs}"
    );

    let copies = folder.0.join("Kopien");
    std::fs::create_dir_all(&copies).unwrap();
    let answer = ok(
        &mut server,
        "gaeb_importieren",
        json!({"datei": x82, "ziel": "projekt", "ordner": copies, "name": "Zum Rauskopieren", "preise": false}),
    );
    assert!(answer.contains("Übernommen als: neues Projekt"), "{answer}");
    assert!(
        answer.contains("nicht übernommen (2 Positionen)"),
        "{answer}"
    );
    assert!(copies
        .join("Zum Rauskopieren/Zum Rauskopieren.proava")
        .is_file());
    assert!(call(
        &mut server,
        "gaeb_importieren",
        json!({"datei": x82, "ziel": "projekt", "ordner": copies, "name": "Zum Rauskopieren"})
    )
    .unwrap_err()
    .contains("schon ein Projekt"));
}

#[test]
fn cost_groups_are_set_and_summed() {
    let folder = TempFolder::new("kg");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "GA");
    ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "zeilen": [
            {"titel": "Automation"},
            {"titel": "ASP01"},
            {"kurztext": "DDC-Station", "menge": 2, "einheit": "St", "ep": 1000},
            {"kurztext": "Schaltschrank", "menge": 1, "einheit": "St", "ep": 500, "kg": "482"},
            {"kurztext": "Kabel", "menge": 100, "einheit": "m", "ep": 2}
        ]}),
    );
    let answer = ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01", "kg": "481"}),
    );
    assert!(answer.contains("KG"), "{answer}");
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01.0030", "kg": "484"}),
    );
    let error = call(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01.0030", "kg": "48"}),
    )
    .unwrap_err();
    assert!(error.contains("drei Ziffern"), "{error}");
    let report = ok(
        &mut server,
        "kostengruppen_lesen",
        json!({"projekt": project}),
    );
    assert!(
        report.contains("481 | Automationseinrichtungen | 1 | 2.000,00 €"),
        "{report}"
    );
    assert!(
        report.contains("482 | Schaltschränke, Automationsschwerpunkte | 1 | 500,00 €"),
        "{report}"
    );
    assert!(
        report.contains("484 | Kabel, Leitungen und Verlegesysteme | 1 | 200,00 €"),
        "{report}"
    );
    assert!(
        report.contains("Summe 480 | Gebäude- und Anlagenautomation | 3 | 2.700,00 €"),
        "{report}"
    );
    let row = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.01.0010"}),
    );
    assert!(
        row.contains("KG: 481 Automationseinrichtungen (vom Titel 01)"),
        "{row}"
    );
    let row = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.01.0020"}),
    );
    assert!(
        row.contains("KG: 482 Schaltschränke, Automationsschwerpunkte\n"),
        "{row}"
    );

    // A finished position inherits from the title: only with fertige_erben.
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01.0010", "status": "fertig"}),
    );
    let error = call(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01", "kg": "489"}),
    )
    .unwrap_err();
    assert!(
        error.contains("1 fertige Position erbt die neue KG 489"),
        "{error}"
    );
    assert!(
        error.contains("01.01.0010") && error.contains("fertige_erben"),
        "{error}"
    );
    let unchanged = ok(
        &mut server,
        "kostengruppen_lesen",
        json!({"projekt": project}),
    );
    assert!(
        unchanged.contains("481 | Automationseinrichtungen | 1"),
        "{unchanged}"
    );
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01", "kg": "489", "fertige_erben": true}),
    );
    let report = ok(
        &mut server,
        "kostengruppen_lesen",
        json!({"projekt": project}),
    );
    assert!(
        report.contains("489 | Sonstiges zur KG 480 | 1 | 2.000,00 €"),
        "{report}"
    );
    // Nothing changes for the finished position: no confirmation needed.
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01", "kg": "489"}),
    );
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01", "kg": ""}),
    );
    let error = call(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01", "kg": "481"}),
    )
    .unwrap_err();
    assert!(
        error.contains("1 fertige Position erbt die neue KG 481"),
        "{error}"
    );
}

#[test]
fn work_categories_are_set_and_read() {
    let folder = TempFolder::new("lb");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "GA");
    ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "zeilen": [
            {"titel": "Automation"},
            {"titel": "ASP01"},
            {"kurztext": "DDC-Station", "menge": 2, "einheit": "St", "ep": 1000, "lb": "070"},
            {"kurztext": "Kabel", "menge": 100, "einheit": "m", "ep": 2}
        ]}),
    );
    let row = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.01.0010"}),
    );
    assert!(
        row.contains("Leistungsbereich: 070 Gebäudeautomation"),
        "{row}"
    );
    let answer = ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01.0020", "lb": "MSR"}),
    );
    assert!(answer.contains("Leistungsbereich"), "{answer}");
    let row = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.01.0020"}),
    );
    assert!(row.contains("Leistungsbereich: MSR"), "{row}");
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01.0020", "lb": ""}),
    );
    let row = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.01.0020"}),
    );
    assert!(row.contains("Leistungsbereich: –"), "{row}");

    // An Titeln: Vorgabe für alles darunter, fertige nur mit fertige_erben.
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01", "lb": "075"}),
    );
    let row = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.01.0020"}),
    );
    assert!(
        row.contains("Leistungsbereich: 075 Raumlufttechnische Anlagen (vom Titel 01)"),
        "{row}"
    );
    let row = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.01.0010"}),
    );
    assert!(
        row.contains("Leistungsbereich: 070 Gebäudeautomation\n"),
        "eigener bleibt: {row}"
    );
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01.0020", "status": "fertig"}),
    );
    let error = call(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01", "lb": "070"}),
    )
    .unwrap_err();
    assert!(
        error.contains("1 fertige Position erbt den neuen Leistungsbereich 070"),
        "{error}"
    );
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01", "lb": "070", "fertige_erben": true}),
    );
    let title = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01"}),
    );
    assert!(
        title.contains("Leistungsbereich: 070 Gebäudeautomation"),
        "{title}"
    );
}

/// Eigene OZ gleich beim Einfügen (Titel 481 …, 489.05) statt nachträglich
/// mit zeile_aendern – geprüft gegen Maske, vergebene OZ und den Titel darüber.
#[test]
fn ordinals_are_given_when_inserting() {
    let folder = TempFolder::new("ozeinfuegen");
    let mut server = server(&folder.0);
    ok(
        &mut server,
        "projekt_anlegen",
        json!({"ordner": folder.0, "name": "OZ", "oz_maske": "111.22.PPPP", "schrittweite": 1}),
    );
    let project = folder
        .0
        .join("OZ")
        .join("OZ.proava")
        .to_string_lossy()
        .to_string();
    let answer = ok(
        &mut server,
        "titel_einfuegen",
        json!({"projekt": project, "kurztext": "Automation", "oz": "481"}),
    );
    assert!(answer.contains("481 „Automation“"), "{answer}");
    let answer = ok(
        &mut server,
        "titel_einfuegen",
        json!({"projekt": project, "kurztext": "Hardware", "unter_titel": "481", "oz": "481.5"}),
    );
    assert!(
        answer.contains("481.05 „Hardware“"),
        "führende Null ergänzt: {answer}"
    );
    let answer = ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "unter_titel": "481.05", "kurztext": "Station", "menge": 1, "einheit": "St", "oz": "481.5.20"}),
    );
    assert!(answer.contains("481.05.0020"), "{answer}");
    // Fehler: doppelt, falsche Maske, falscher Titel, falsche Ebene, zu groß
    let cases = [
        (
            "position_einfuegen",
            json!({"unter_titel": "481.05", "kurztext": "X", "oz": "481.05.0020"}),
            "schon vergeben",
        ),
        (
            "position_einfuegen",
            json!({"unter_titel": "481.05", "kurztext": "X", "oz": "481.0021"}),
            "schreibt sich wie „001.01.0001“",
        ),
        (
            "position_einfuegen",
            json!({"unter_titel": "481.05", "kurztext": "X", "oz": "482.05.0001"}),
            "Positionen dort beginnen mit 481.05",
        ),
        (
            "position_einfuegen",
            json!({"unter_titel": "481.05", "kurztext": "X", "oz": "481.05.10000"}),
            "bis 9999",
        ),
        (
            "titel_einfuegen",
            json!({"kurztext": "X", "oz": "482.01"}),
            "Ebene 2, der Titel steht auf Ebene 1",
        ),
        (
            "titel_einfuegen",
            json!({"kurztext": "X", "unter_titel": "481", "oz": "482.01"}),
            "beginnt mit 481",
        ),
        (
            "titel_einfuegen",
            json!({"kurztext": "X", "oz": "1000"}),
            "bis 999",
        ),
    ];
    for (tool, mut arguments, expected) in cases {
        arguments["projekt"] = json!(project);
        let error = call(&mut server, tool, arguments.clone()).unwrap_err();
        assert!(error.contains(expected), "{arguments}: {error}");
    }
    // positionen_einfuegen mit OZ je Zeile, Titelnummer mit Lücke
    let answer = ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "zeilen": [
            {"titel": "Sonstiges", "ebene": 1, "oz": "489"},
            {"titel": "Dokumentation", "oz": "489.05"},
            {"kurztext": "Revisionsunterlagen", "pauschal": true, "oz": "489.05.0001"},
            {"kurztext": "Montagepläne", "pauschal": true}
        ]}),
    );
    assert!(
        answer.contains("489.05.0002"),
        "freie OZ hinter der eigenen: {answer}"
    );
    let list = ok(&mut server, "lv_lesen", json!({"projekt": project}));
    let ordinals: Vec<&str> = list
        .lines()
        .skip(2)
        .filter_map(|line| line.split(" | ").next())
        .collect();
    assert_eq!(
        ordinals,
        [
            "481",
            "481.05",
            "481.05.0020",
            "489",
            "489.05",
            "489.05.0001",
            "489.05.0002"
        ],
        "{list}"
    );
}

/// Textergänzungen an ihrer Stelle, Zeilenumbruch im Absatz und
/// EP-Aufgliederung per MCP – bis in den GAEB-Export.
#[test]
fn complements_line_breaks_and_price_breakdown_reach_gaeb() {
    let folder = TempFolder::new("textergaenzung");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "TE");
    ok(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": project, "ep_anteile": ["Lohn", "Material"], "oz_maske": "11.PPPP"}),
    );
    ok(
        &mut server,
        "titel_einfuegen",
        json!({"projekt": project, "kurztext": "Schaltanlage"}),
    );
    let error = call(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "kurztext": "X", "langtext": "Text {TE 7}"}),
    )
    .unwrap_err();
    assert!(
        error.contains("keine Textergänzung mit Kennung 7"),
        "{error}"
    );
    ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "kurztext": "Sammelschiene\nmit Abgängen", "menge": 1, "einheit": "St", "ep": 100,
               "langtext": "Schiene 400 V AC,\n{TE 11}\nHz, Strom {TE 12}A,\\\nzweite Zeile",
               "textergaenzungen": [
                   {"kennung": 11, "art": "AG", "vortext": "Spannung DC", "inhalt": "220", "nachtext": "V, Frequenz 50"},
                   {"kennung": "12", "art": "AG", "inhalt": "16"},
                   {"kennung": 13, "vortext": "Fabrikat:"}
               ],
               "ep_aufgliederung": true, "ep_anteile_werte": [60, 40]}),
    );
    let shown = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.0010"}),
    );
    assert!(
        shown.contains("Schiene 400 V AC,\n{TE 11}\nHz, Strom {TE 12}A,\\\nzweite Zeile"),
        "{shown}"
    );
    assert!(
        shown.contains("Textergänzung 13 (Bieter), nach dem Text"),
        "{shown}"
    );
    assert!(
        shown.contains("EP-Aufgliederung: ja · Lohn 60,00, Material 40,00"),
        "{shown}"
    );
    let error = call(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.0010", "ep_anteile_werte": [1]}),
    )
    .unwrap_err();
    assert!(error.contains("2 EP-Anteile"), "{error}");
    // Nur den Inhalt ändern: die Stelle bleibt.
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.0010", "textergaenzungen": [
            {"kennung": 11, "inhalt": "230"}, {"kennung": 12}, {"kennung": 13}]}),
    );
    let shown = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.0010"}),
    );
    assert!(
        shown.contains("{TE 11}")
            && shown.contains("Inhalt „230“")
            && shown.contains("Vortext „Spannung DC“"),
        "{shown}"
    );

    let target = folder.0.join("TE.X83");
    ok(
        &mut server,
        "gaeb_exportieren",
        json!({"projekt": project, "art": "X83", "ziel": target}),
    );
    let xml = std::fs::read_to_string(&target).unwrap();
    for expected in [
        "<span>Schiene 400 V AC,</span></p></Text><TextComplement MarkLbl=\"11\" Kind=\"Owner\">",
        "<Text><span>Hz, Strom </span></Text><TextComplement MarkLbl=\"12\"",
        "<span>zweite Zeile</span>",
        "<br/>",
        "<UPBkdn>Yes</UPBkdn>",
        "<NoUPComps>2</NoUPComps>",
        "<TextOutlTxt><span>Sammelschiene</span><br/><span>mit Abgängen</span></TextOutlTxt>",
    ] {
        assert!(xml.contains(expected), "{expected} fehlt:\n{xml}");
    }
}

/// Bild aus einer Datei in einen Hinweis einfügen; GAEB DA XML gibt es mit
/// (abschaltbar), GAEB 90 warnt.
/// 1×1-PNG for the image tests.
const PNG: [u8; 67] = [
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

#[test]
fn images_are_inserted_and_exported() {
    let folder = TempFolder::new("bilder");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Bild");
    let image = folder.0.join("topologie.png");
    std::fs::write(&image, PNG).unwrap();
    std::fs::write(folder.0.join("kein.png"), b"kein Bild").unwrap();
    ok(
        &mut server,
        "titel_einfuegen",
        json!({"projekt": project, "kurztext": "Vorbemerkungen"}),
    );
    ok(
        &mut server,
        "hinweis_einfuegen",
        json!({"projekt": project, "kurztext": "Projektbeschreibung", "text": "GA-Systemtopologie:\nWeiterer Text"}),
    );
    let shown = ok(&mut server, "lv_lesen", json!({"projekt": project}));
    let id: u64 = shown
        .lines()
        .find(|line| line.contains("| Hinweis |"))
        .and_then(|line| line.rsplit('#').next())
        .and_then(|id| id.trim().parse().ok())
        .expect(&shown);
    let error = call(
        &mut server,
        "bild_einfuegen",
        json!({"projekt": project, "id": id, "datei": folder.0.join("kein.png")}),
    )
    .unwrap_err();
    assert!(error.contains("Nur PNG- oder JPEG"), "{error}");
    let error = call(
        &mut server,
        "bild_einfuegen",
        json!({"projekt": project, "id": id, "datei": image, "absatz": 5}),
    )
    .unwrap_err();
    assert!(error.contains("nur 2 Absätze"), "{error}");
    let answer = ok(
        &mut server,
        "bild_einfuegen",
        json!({"projekt": project, "id": id, "datei": image, "breite_mm": 100.27, "absatz": 1}),
    );
    assert!(
        answer.contains("nach Block 1") && answer.contains("[Bild: topologie.png, 100,3 mm]"),
        "{answer}"
    );
    let note = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "id": id}),
    );
    assert!(
        note.contains("GA-Systemtopologie:\n[Bild: topologie.png, 100,3 mm]\nWeiterer Text"),
        "{note}"
    );
    let check = ok(&mut server, "lv_pruefen", json!({"projekt": project}));
    assert!(
        check.contains("Bilder im Langtext: 1, zusammen 1 KB"),
        "{check}"
    );

    let with = folder.0.join("mit.X83");
    let answer = ok(
        &mut server,
        "gaeb_exportieren",
        json!({"projekt": project, "art": "X83", "ziel": with}),
    );
    assert!(
        answer.contains("1 Bild aus den Langtexten mitgegeben"),
        "{answer}"
    );
    let xml = std::fs::read_to_string(&with).unwrap();
    assert!(
        xml.contains("<image Encoding=\"base64\" Type=\"image/png\" width=\"379\">"),
        "{xml}"
    );
    let without = folder.0.join("ohne.X83");
    let answer = ok(
        &mut server,
        "gaeb_exportieren",
        json!({"projekt": project, "art": "X83", "ziel": without, "bilder": false}),
    );
    assert!(answer.contains("nicht mitgegeben"), "{answer}");
    assert!(!std::fs::read_to_string(&without)
        .unwrap()
        .contains("<image"));
    let answer = ok(
        &mut server,
        "gaeb_exportieren",
        json!({"projekt": project, "art": "X83", "format": "gaeb90", "ziel": folder.0.join("lv.D83")}),
    );
    assert!(answer.contains("GAEB 90 kann keine Bilder"), "{answer}");
}

/// Ausschnitt aus einer PDF-Seite: Anteile oder mm, Seite, Breite; das
/// Projekt hält PDF, Seite und Rechteck, GAEB bekommt ein PNG.
#[test]
fn pdf_clips_are_inserted_and_exported() {
    let folder = TempFolder::new("ausschnitt");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Ausschnitt");
    let pdf = folder.0.join("Topologie.pdf");
    std::fs::write(&pdf, proava_print::sample_vector_pdf()).unwrap();
    std::fs::write(folder.0.join("kein.pdf"), b"kein PDF").unwrap();
    ok(
        &mut server,
        "titel_einfuegen",
        json!({"projekt": project, "kurztext": "Vorbemerkungen"}),
    );
    ok(
        &mut server,
        "hinweis_einfuegen",
        json!({"projekt": project, "kurztext": "Systemtopologie", "text": "Übersicht:"}),
    );
    let shown = ok(&mut server, "lv_lesen", json!({"projekt": project}));
    let id: u64 = shown
        .lines()
        .find(|line| line.contains("| Hinweis |"))
        .and_then(|line| line.rsplit('#').next())
        .and_then(|id| id.trim().parse().ok())
        .expect(&shown);
    let error = call(
        &mut server,
        "pdf_ausschnitt_einfuegen",
        json!({"projekt": project, "id": id, "datei": folder.0.join("kein.pdf")}),
    )
    .unwrap_err();
    assert!(error.contains("keine PDF"), "{error}");
    let error = call(
        &mut server,
        "pdf_ausschnitt_einfuegen",
        json!({"projekt": project, "id": id, "datei": pdf, "seite": 3}),
    )
    .unwrap_err();
    assert!(error.contains("2 Seite(n)"), "{error}");
    let error = call(
        &mut server,
        "pdf_ausschnitt_einfuegen",
        json!({"projekt": project, "id": id, "datei": pdf, "ausschnitt_mm": {"links": 0, "oben": 0, "breite": 2, "hoehe": 50}}),
    )
    .unwrap_err();
    assert!(error.contains("zu klein"), "{error}");
    let answer = ok(
        &mut server,
        "pdf_ausschnitt_einfuegen",
        json!({"projekt": project, "id": id, "datei": pdf,
               "ausschnitt_anteile": {"links": 0, "oben": 0, "rechts": 0.5, "unten": 1}, "breite_mm": 120}),
    );
    assert!(
        answer.contains("„Topologie_S1.pdf“") && answer.contains("120 mm breit"),
        "{answer}"
    );
    assert!(
        answer.contains("Ausschnitt 100 × 100 mm aus Topologie.pdf, Seite 1"),
        "{answer}"
    );
    let answer = ok(
        &mut server,
        "pdf_ausschnitt_einfuegen",
        json!({"projekt": project, "id": id, "datei": pdf, "seite": 2,
               "ausschnitt_mm": {"links": 10, "oben": 10, "breite": 80, "hoehe": 40}, "absatz": 1}),
    );
    assert!(
        answer.contains("„Topologie_S2.pdf“")
            && answer.contains("80 mm breit")
            && answer.contains("nach Block 1"),
        "{answer}"
    );
    let loaded = XmlProjectFile::new()
        .load_project(Path::new(&project))
        .unwrap();
    let first = loaded
        .image("Topologie_S1.pdf")
        .unwrap()
        .clip
        .clone()
        .unwrap();
    assert!((first.rect.width_mm() - 100.0).abs() < 0.1);
    let second = loaded.image("Topologie_S2.pdf").unwrap();
    assert!(
        (second.clip.as_ref().unwrap().rect.left - 10.0 * proava_domain::PT_PER_MM).abs() < 0.01
    );

    let file = folder.0.join("lv.X83");
    let answer = ok(
        &mut server,
        "gaeb_exportieren",
        json!({"projekt": project, "art": "X83", "ziel": file}),
    );
    assert!(
        answer.contains("2 Bilder aus den Langtexten mitgegeben"),
        "{answer}"
    );
    let xml = std::fs::read_to_string(&file).unwrap();
    assert_eq!(
        xml.matches("Type=\"image/png\"").count(),
        2,
        "Ausschnitte als PNG"
    );
}

/// LV with two titles and two positions, bid of „Firma A“ awarded – saved
/// as project file in `folder`.
fn awarded_project(folder: &Path) -> String {
    use proava_domain::{Address, Bid, Lv, LvGroup, OzMask, Position, Project, ProjectName};
    let dec = |text: &str| text.parse::<Decimal>().unwrap();
    let mut lv = Lv::new("LV 480 GA", OzMask::parse("11.22.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "Automation"));
    lv.push_group(LvGroup::new(1, "01.01", "ASP01"));
    let a = lv.push_position(Position::new(
        "01.01.0010",
        "CO2-Fühler Raum",
        dec("12"),
        "St",
        Some(dec("250")),
    ));
    let b = lv.push_position(Position::new(
        "01.01.0020",
        "Kabel J-Y(St)Y",
        dec("100"),
        "m",
        None,
    ));
    let mut project = Project::new(ProjectName::parse("Nachtraege").unwrap(), "2026-11-17", lv);
    let mut bid = Bid::new(Address::named("Firma A"), "a.x84");
    bid.set_unit_price(a, Some(dec("265")));
    bid.set_unit_price(b, Some(dec("1.65")));
    let bid = project.add_bid(bid);
    project.award_bid(bid, "15.09.2026").unwrap();
    let path = folder.join("Nachtraege.proava");
    XmlProjectFile::new().save_project(&path, &project).unwrap();
    path.display().to_string()
}

#[test]
fn change_orders_from_offer_to_approval() {
    let folder = TempFolder::new("nachtraege");
    let mut server = server(&folder.0);
    let project = awarded_project(&folder.0);

    let answer = ok(
        &mut server,
        "nachtrag_anlegen",
        json!({"projekt": project, "bezeichnung": "Wetterstation", "anlass": "zusätzlich", "begruendung": "Bauherrenwunsch"}),
    );
    assert!(
        answer.contains("N01 angelegt")
            && answer.contains("Auftrag aus dem beauftragten Angebot übernommen"),
        "{answer}"
    );
    assert!(call(
        &mut server,
        "nachtrag_anlegen",
        json!({"projekt": project, "bezeichnung": "x", "anlass": "irgendwas"})
    )
    .unwrap_err()
    .contains("Anlass"));
    let answer = ok(
        &mut server,
        "nachtragsposition_setzen",
        json!({"projekt": project, "nachtrag": "N01", "neu": true,
        "kurztext": "Wetterstation", "langtext": "Kompakte Station, **BACnet/IP**", "menge": 1, "einheit": "St", "angeboten_ep": 1947}),
    );
    assert!(
        answer.contains("neue Position 02.01.0010") && answer.contains("1.947,00"),
        "{answer}"
    );
    let answer = ok(
        &mut server,
        "nachtragsposition_setzen",
        json!({"projekt": project, "nachtrag": 1, "oz": "01.01.0010", "menge": 6, "vermerk": "Raumliste geprüft"}),
    );
    assert!(
        answer.contains("zugeordnet") && answer.contains("6 × 265,00 = 1.590,00"),
        "{answer}"
    );
    ok(
        &mut server,
        "nachtragsposition_setzen",
        json!({"projekt": project, "nachtrag": "N01", "oz": "02.01.0010", "geprueft_ep": 1697, "vermerk": "EK geprüft"}),
    );

    let read = ok(
        &mut server,
        "nachtraege_lesen",
        json!({"projekt": project, "nachtrag": "N01"}),
    );
    assert!(read.contains("02.01.0010 | Wetterstation | neu | St | 1 | 1.947,00 | 1.947,00 € | 1 | 1.697,00 | 1.697,00 € | -12,8"), "{read}");
    assert!(
        read.contains("01.01.0010 | CO2-Fühler Raum | Mehrmenge"),
        "{read}"
    );
    assert!(
        read.contains("Prüfhilfe: EP Auftrag 01.01.0010 265,00"),
        "{read}"
    );
    assert!(read.contains("Nachtragsbereich im LV: 02.01 N01 Wetterstation – nächste neue Position bekäme OZ 02.01.0020"), "{read}");

    // GAEB hin und zurück: X83 an die Firma, ihr Angebot als Datei zurück.
    let answer = ok(
        &mut server,
        "nachtrag_gaeb_exportieren",
        json!({"projekt": project, "nachtrag": "N01", "art": "X83"}),
    );
    assert!(
        answer.contains(".X83") && answer.contains("Mindermenge einer Auftragsposition"),
        "{answer}"
    );
    let file = folder
        .0
        .read_dir()
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|e| e == "X83"))
        .unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("<CONo>1</CONo>") && text.contains("<COStatus>Filed</COStatus>"));
    let offer = folder.0.join("Angebot_N01.X86");
    std::fs::write(
        &offer,
        text.replace("DA83", "DA86")
            .replace("<DP>83</DP>", "<DP>86</DP>"),
    )
    .unwrap();
    let answer = ok(
        &mut server,
        "nachtragsangebot_einlesen",
        json!({"projekt": project, "nachtrag": "N01", "datei": offer}),
    );
    assert!(
        answer.contains("1 Position übernommen, 0 neu") && answer.contains("Status angeboten"),
        "{answer}"
    );
    // Die Datei hat keine Preise: der angebotene EP bleibt.
    assert!(answer.contains("angeboten 3.537,00 €"), "{answer}");

    let answer = ok(
        &mut server,
        "nachtrag_status",
        json!({"projekt": project, "nachtrag": "N01", "status": "beauftragt", "datum": "28.10.2026", "vermerk": "Schreiben Bauherr"}),
    );
    assert!(
        answer.contains("beauftragt am 28.10.2026") && answer.contains("abrechenbar"),
        "{answer}"
    );
    ok(
        &mut server,
        "nachtrag_aendern",
        json!({"projekt": project, "nachtrag": "N01", "geprueft_am": "20.10.2026", "geprueft_von": "Planer", "empfehlung": "beauftragen"}),
    );
    let list = ok(&mut server, "nachtraege_lesen", json!({"projekt": project}));
    // 1.697 + 6 × 265 = 3.287 beauftragt; Auftrag 12 × 265 + 100 × 1,65 = 3.345.
    assert!(
        list.contains("beauftragt seit 28.10.2026 | 3.537,00 € | 3.287,00 € | 3.287,00 € | ja"),
        "{list}"
    );
    assert!(
        list.contains(
            "Auftragssumme alt 3.345,00 € + beauftragte Nachträge 3.287,00 € = neu 6.632,00 €"
        ),
        "{list}"
    );

    for art in ["pruefung", "uebersicht"] {
        let answer = ok(
            &mut server,
            "nachtrag_drucken",
            json!({"projekt": project, "art": art, "nachtrag": "N01"}),
        );
        assert!(
            answer.contains("geschrieben") && answer.contains("(1 Seite)"),
            "{answer}"
        );
    }
    let answer = ok(
        &mut server,
        "nachtrag_aendern",
        json!({"projekt": project, "nachtrag": "N01", "loeschen": true}),
    );
    assert!(answer.contains("gelöscht"), "{answer}");
    assert!(ok(&mut server, "lv_lesen", json!({"projekt": project})).contains("01.01.0010"));
    assert!(!ok(&mut server, "lv_lesen", json!({"projekt": project})).contains("Wetterstation"));
}

#[test]
fn takeoff_lines_give_the_quantity() {
    let folder = TempFolder::new("me");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "GA");
    ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "zeilen": [
            {"titel": "Automation"},
            {"titel": "ASP01"},
            {"kurztext": "Kabel", "einheit": "m", "ep": 2, "mengenermittlung": [
                {"ort": "EG Strang 1", "rechenansatz": "2*12,5"},
                {"ort": "OG", "rechenansatz": 7.5}
            ]}
        ]}),
    );
    let row = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.01.0010"}),
    );
    assert!(row.contains("Menge: 32,5 m"), "Summe wird die Menge: {row}");
    assert!(
        row.contains("Mengenermittlung: 2 Zeile(n), Summe 32,5"),
        "{row}"
    );
    assert!(row.contains("1. EG Strang 1 | 2*12,5 = 25"), "{row}");
    let error = call(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01.0010", "mengenermittlung": [{"ort": "EG", "rechenansatz": "2*"}]}),
    )
    .unwrap_err();
    assert!(error.contains("mengenermittlung Zeile 1"), "{error}");
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.01.0010", "menge": 40, "mengenermittlung": [{"ort": "alles", "rechenansatz": "30"}]}),
    );
    let row = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.01.0010"}),
    );
    assert!(row.contains("Menge: 40 m"), "menge angegeben: bleibt {row}");
    assert!(row.contains("Summe 30"), "{row}");
}

#[test]
fn library_store_search_insert_and_compare() {
    let folder = TempFolder::new("bibliothek");
    let own = folder.0.join("Meine-Bibliothek.proava");
    let mut server = server(&folder.0).with_library_defaults(proava_mcp::LibraryDefaults {
        own: Some(own.clone()),
        others: Vec::new(),
    });
    let project = new_project(&mut server, &folder.0, "Quelle");
    ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "zeilen": [
            {"titel": "Raumautomation"},
            {"kurztext": "Raumbediengerät KNX", "menge": 12, "einheit": "St", "ep": 180, "langtext": "Taster mit Farbdisplay"},
            {"kurztext": "Präsenzmelder", "menge": 30, "einheit": "St", "ep": 95}
        ]}),
    );
    let stored = ok(
        &mut server,
        "bibliothek_ablegen",
        json!({"projekt": project, "oz": ["01"], "neuer_titel": "Raumautomation KNX", "stichwoerter": "KNX, Raum", "preis_datum": "11/2026"}),
    );
    assert!(stored.contains("3 Zeile(n) (2 Positionen)"), "{stored}");
    assert!(stored.contains("neu angelegt"), "{stored}");
    assert!(own.is_file());

    let found = ok(
        &mut server,
        "bibliothek_suchen",
        json!({"suche": "farbdisplay"}),
    );
    assert!(found.contains("1 Positionen gefunden"), "{found}");
    assert!(found.contains("Raumbediengerät KNX"), "{found}");
    assert!(found.contains("(Preis 11/2026)"), "{found}");
    assert!(found.contains("Stichwörter: KNX, Raum"), "{found}");
    let by_keyword = ok(
        &mut server,
        "bibliothek_suchen",
        json!({"stichwort": "raum"}),
    );
    assert!(by_keyword.contains("2 Positionen gefunden"), "{by_keyword}");
    let id: u64 = found.rsplit('#').next().unwrap().trim().parse().unwrap();

    let target = new_project(&mut server, &folder.0, "Ziel");
    let inserted = ok(
        &mut server,
        "bibliothek_einfuegen",
        json!({"projekt": target, "ids": [id]}),
    );
    assert!(inserted.contains("Herkunft gemerkt"), "{inserted}");
    let read = ok(&mut server, "lv_lesen", json!({"projekt": target}));
    assert!(
        read.contains("Raumbediengerät KNX | 0 St | 180,00"),
        "{read}"
    );

    let same = ok(
        &mut server,
        "bibliothek_abgleichen",
        json!({"projekt": target}),
    );
    assert!(
        same.contains("1 Positionen aus einer Bibliothek · 0 weichen"),
        "{same}"
    );
    // The library entry gets a better text.
    let library = own.to_string_lossy().to_string();
    let lines = ok(&mut server, "lv_lesen", json!({"projekt": library}));
    let entry = lines
        .lines()
        .find(|line| line.contains("Raumbediengerät"))
        .unwrap();
    let oz = entry.split(" | ").next().unwrap().trim().to_string();
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": library, "oz": oz, "langtext": "Taster mit Farbdisplay und Präsenz"}),
    );
    let changed = ok(
        &mut server,
        "bibliothek_abgleichen",
        json!({"projekt": target}),
    );
    assert!(
        !changed.contains("0 weichen") && changed.contains("Langtext"),
        "{changed}"
    );
    let taken = ok(
        &mut server,
        "bibliothek_abgleichen",
        json!({"projekt": target, "uebernehmen": ["alle"]}),
    );
    assert!(
        taken.contains("1 Position(en) auf den Bibliotheksstand"),
        "{taken}"
    );
    let after = ok(
        &mut server,
        "lv_lesen",
        json!({"projekt": target, "mit_langtext": true}),
    );
    assert!(after.contains("Farbdisplay und Präsenz"), "{after}");

    assert!(call(
        &mut server,
        "bibliothek_einfuegen",
        json!({"projekt": target, "bibliothek": "Gibtsnicht", "ids": [id]})
    )
    .unwrap_err()
    .contains("unbekannt"));
}

#[test]
fn billing_from_contract_to_release() {
    let folder = TempFolder::new("abrechnung");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Abrechnung");
    ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "zeilen": [
            {"titel": "Automation"},
            {"kurztext": "DDC-Station", "menge": 2, "einheit": "St", "ep": 2500},
            {"kurztext": "Fühler", "menge": 10, "einheit": "St", "ep": 80},
            {"kurztext": "Inbetriebnahme", "menge": 1, "einheit": "psch", "ep": 1200, "pauschal": true}
        ]}),
    );
    ok(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": project, "sicherheit_vertragserfuellung": 10, "bauherr": {"name1": "Stadt Musterstadt", "ort": "Musterstadt"}}),
    );

    // Without contract: a clear answer, no error.
    let answer = ok(&mut server, "abrechnung_lesen", json!({"projekt": project}));
    assert!(answer.contains("noch kein Auftrag"), "{answer}");
    assert!(
        call(&mut server, "rechnung_anlegen", json!({"projekt": project}))
            .unwrap_err()
            .contains("auftrag_festschreiben")
    );

    let bid = folder.0.join("a.D84");
    write_d84(&project, &bid, "Firma A", "1.0");
    ok(
        &mut server,
        "angebot_einlesen",
        json!({"projekt": project, "datei": bid}),
    );
    ok(
        &mut server,
        "auftrag_erteilen",
        json!({"projekt": project, "angebot": 1, "datum": "01.12.2026"}),
    );
    let answer = ok(
        &mut server,
        "auftrag_festschreiben",
        json!({"projekt": project, "auftrags_nr": "A-480"}),
    );
    assert!(
        answer.contains("Firma A")
            && answer.contains("3 Positionen")
            && answer.contains("7.000,00"),
        "{answer}"
    );
    assert!(call(
        &mut server,
        "auftrag_festschreiben",
        json!({"projekt": project})
    )
    .unwrap_err()
    .contains("neu: true"));

    // AR-1 by hand: DDC 1 of 2, Fühler 6 claimed, 5 checked, commissioning 50 %.
    let answer = ok(
        &mut server,
        "rechnung_anlegen",
        json!({"projekt": project, "nummer": "AR-1", "datum": "28.02.2027", "ust": 19}),
    );
    assert!(answer.contains("#1") && answer.contains("AR-1"), "{answer}");
    let answer = ok(
        &mut server,
        "rechnungsposition_setzen",
        json!({"projekt": project, "rechnung": 1, "positionen": [
            {"oz": "01.00.0010", "rechnung_menge": 1},
            {"oz": "01.00.0020", "rechnung_menge": 6, "gepruefte_menge": 5, "vermerk": "1 Fühler fehlt"},
        ]}),
    );
    assert!(
        answer.contains("gekürzt") || answer.contains("Freigabe"),
        "{answer}"
    );
    let error = call(
        &mut server,
        "rechnungsposition_setzen",
        json!({"projekt": project, "rechnung": 1, "oz": "01.00.0030", "fortschritt_prozent": 50}),
    )
    .unwrap_err();
    assert!(error.contains("vermerk"), "{error}");
    ok(
        &mut server,
        "rechnungsposition_setzen",
        json!({"projekt": project, "rechnung": 1, "oz": "01.00.0030", "fortschritt_prozent": 50, "vermerk": "Absprache vom 20.02.2027"}),
    );

    // Release: 2.500 + 5 × 80 + 600 = 3.500, minus 10 % security = 3.150, + 19 % = 3.748,50.
    let release = ok(
        &mut server,
        "freigabe_berechnen",
        json!({"projekt": project, "rechnung": 1}),
    );
    assert!(
        release.contains("3.500,00 €")
            && release.contains("= Freigabe netto: 3.150,00 €")
            && release.contains("3.748,50 €"),
        "{release}"
    );
    let detail = ok(
        &mut server,
        "abrechnung_lesen",
        json!({"projekt": project, "rechnung": 1}),
    );
    assert!(
        detail.contains("1 Fühler fehlt") && detail.contains("Fortschritt 50 %"),
        "{detail}"
    );

    // AR-2 with takeoff for the Fühler: 3 more (Zuwachs) → 8 cumulative.
    ok(
        &mut server,
        "rechnung_anlegen",
        json!({"projekt": project, "nummer": "AR-2", "datum": "30.04.2027"}),
    );
    let answer = ok(
        &mut server,
        "aufmass_setzen",
        json!({"projekt": project, "rechnung": 2, "oz": "01.00.0020", "zeilen": [
            {"ort": "EG Flur", "rechenansatz": "2"}, {"ort": "OG", "rechenansatz": "0,5*2"}
        ]}),
    );
    assert!(answer.contains("Summe 3"), "{answer}");
    let error = call(&mut server, "aufmass_setzen", json!({"projekt": project, "rechnung": 2, "oz": "01.00.0020", "zeilen": [{"ort": "x", "rechenansatz": "2*"}]})).unwrap_err();
    assert!(error.contains("zeilen[0]"), "{error}");
    let detail = ok(
        &mut server,
        "abrechnung_lesen",
        json!({"projekt": project, "rechnung": 2}),
    );
    assert!(
        detail.contains("Fühler | St | 10 | 5 |") && detail.contains("| 8 | Aufmaß |"),
        "{detail}"
    );
    let overview = ok(&mut server, "abrechnung_lesen", json!({"projekt": project}));
    assert!(
        overview.contains("Rechnungen (2)") && overview.contains("Auftrags-Nr. A-480"),
        "{overview}"
    );
    let release = ok(
        &mut server,
        "freigabe_berechnen",
        json!({"projekt": project, "rechnung": 2}),
    );
    assert!(
        release.contains("Bisherige Freigaben")
            && release.contains("anrechenbare Freigaben: -3.150,00 €"),
        "{release}"
    );

    // PDFs and the letter to the client.
    for (document, file) in [
        ("aufmasspruefung", "Abrechnung_Aufmasspruefung.pdf"),
        ("rechnungsfreigabe", "Abrechnung_Rechnungsfreigabe.pdf"),
    ] {
        let answer = ok(
            &mut server,
            "pdf_drucken",
            json!({"projekt": project, "dokument": document, "rechnung": 1}),
        );
        assert!(answer.contains(file), "{answer}");
        assert!(folder.0.join("Abrechnung").join(file).is_file());
    }
    let answer = ok(
        &mut server,
        "anschreiben_drucken",
        json!({"projekt": project, "art": "freigabe", "rechnung": 2}),
    );
    assert!(
        answer.contains("Freigabe an den Bauherrn") && answer.contains("_Freigabe_AR-2_"),
        "{answer}"
    );
    assert!(call(
        &mut server,
        "pdf_drucken",
        json!({"projekt": project, "dokument": "rechnungsfreigabe", "rechnung": 9})
    )
    .unwrap_err()
    .contains("#9"));

    // Change the header, then delete AR-2: its takeoff stays unassigned.
    let answer = ok(
        &mut server,
        "rechnung_aendern",
        json!({"projekt": project, "rechnung": 2, "art": "schluss", "pruefvermerk": "geprüft"}),
    );
    assert!(
        answer.contains("Art") && answer.contains("Prüfvermerk"),
        "{answer}"
    );
    let answer = ok(
        &mut server,
        "rechnung_aendern",
        json!({"projekt": project, "rechnung": 2, "loeschen": true}),
    );
    assert!(answer.contains("gelöscht"), "{answer}");
    let overview = ok(&mut server, "abrechnung_lesen", json!({"projekt": project}));
    assert!(overview.contains("Rechnungen (1)"), "{overview}");
}

#[test]
fn csv_import_and_export_in_the_proava_format() {
    let folder = TempFolder::new("csv");
    let mut server = server(&folder.0);
    let file = folder.0.join("Zulieferung.csv");
    std::fs::write(
        &file,
        "OZ:;Positionstext:;Einheit:;Menge:;EP:;GP:;KG:;Positionsart:\n\
         01.481.01.0010;Kurztext;St.;1;10;12;481;Position\n\
         ;Langtext;;;;;;\n\
         ;;;;;;;\n\
         01.481.01.0010;Ausführungsbeschreibung;;;;;481;Ausführungsbeschreibung\n\
         ;Langtext;;;;;;\n\
         ;;;;;;;\n\
         01.481.01.0010;Vorbemerkung;;;;;481;Text\n\
         ;Langtext;;;;;;\n",
    )
    .unwrap();
    let preview = ok(
        &mut server,
        "csv_importieren",
        json!({"datei": file, "vorschau": true}),
    );
    assert!(preview.contains("CSV-Tabelle"), "{preview}");
    assert!(
        preview.contains("Spalten: OZ, Positionstext, Einheit, Menge, EP, GP, KG, Positionsart"),
        "{preview}"
    );
    assert!(
        preview.contains(
            "01.481.01.0010: GP laut Datei 12,00 €, errechnet 10,00 € – Abweichung 2,00 €"
        ),
        "{preview}"
    );
    assert!(
        preview.contains("Gespeichert: nichts (vorschau)"),
        "{preview}"
    );

    let target = new_project(&mut server, &folder.0, "Ziel");
    let answer = ok(
        &mut server,
        "csv_importieren",
        json!({"datei": file, "projekt": target, "nummer": "2", "waehrung": "CHF"}),
    );
    assert!(
        answer.contains("Importprotokoll Zulieferung.csv"),
        "{answer}"
    );
    assert!(
        answer.contains("Währung: Schweizer Franken (CHF)"),
        "{answer}"
    );
    assert!(
        answer.contains("Summe der Abweichungen: 2,00 CHF"),
        "{answer}"
    );
    assert!(answer.contains("3 Titel aus den OZ angelegt"), "{answer}");
    let lv = ok(
        &mut server,
        "lv_lesen",
        json!({"projekt": target, "lv": "2"}),
    );
    assert!(lv.contains("Kurztext"), "{lv}");

    let csv = folder.0.join("Export.csv");
    let written = ok(
        &mut server,
        "csv_exportieren",
        json!({"projekt": target, "lv": "2", "ziel": csv, "zeichensatz": "windows-1252"}),
    );
    assert!(written.contains("Windows-1252"), "{written}");
    let bytes = std::fs::read(&csv).unwrap();
    assert!(bytes.starts_with(b"OZ:;Positionstext:"), "ohne BOM");
    assert!(
        bytes.windows(11).any(|w| w == b"Ausf\xFChrungs"),
        "ü in Windows-1252"
    );
    let again = call(
        &mut server,
        "csv_exportieren",
        json!({"projekt": target, "lv": "2", "ziel": csv}),
    )
    .unwrap_err();
    assert!(again.contains("ueberschreiben"), "{again}");
    let error = call(
        &mut server,
        "csv_importieren",
        json!({"datei": file, "projekt": target, "waehrung": "YEN"}),
    )
    .unwrap_err();
    assert!(error.contains("EUR, USD oder CHF"), "{error}");
}

/// bieterangabe_einfuegen: Feld im Satz, auf eigener Zeile, unter dem
/// Langtext; Angabe des Auftraggebers; Prüfung zählt die Bieterangaben und
/// meldet eine ohne Vortext; GAEB X83 trägt sie als TextComplement.
#[test]
fn bidder_fields_in_the_long_text() {
    let folder = TempFolder::new("bieterangaben");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Bieterangaben");
    ok(
        &mut server,
        "titel_einfuegen",
        json!({"projekt": project, "kurztext": "Automationseinrichtungen"}),
    );
    let answer = ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "kurztext": "Raumbediengerät", "langtext": "Raumbediengerät mit Display,\nAufputzmontage.", "menge": 4, "einheit": "St"}),
    );
    let oz = answer
        .split("Position eingefügt: ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .expect("OZ in der Antwort")
        .to_string();
    let answer = ok(
        &mut server,
        "bieterangabe_einfuegen",
        json!({"projekt": project, "oz": oz, "vortext": "Fabrikat:", "nach": "mit Display,"}),
    );
    assert!(
        answer.starts_with(&format!("Bieterangabe 01 in {oz}")),
        "{answer}"
    );
    assert!(
        answer.contains("(hinter „mit Display,“)") && answer.ends_with("Im Druck: Fabrikat: …………"),
        "{answer}"
    );
    let answer = ok(
        &mut server,
        "bieterangabe_einfuegen",
        json!({"projekt": project, "oz": oz, "vortext": "Nennweite", "art": "AG", "inhalt": "DN 20", "absatz": 1}),
    );
    assert!(
        answer.starts_with("Angabe des Auftraggebers 02") && answer.contains("Nennweite DN 20"),
        "{answer}"
    );
    let answer = ok(
        &mut server,
        "bieterangabe_einfuegen",
        json!({"projekt": project, "oz": oz, "vortext": "Typ:"}),
    );
    assert!(
        answer.contains("Bieterangabe 03") && answer.contains("(unter dem Langtext)"),
        "{answer}"
    );
    let row = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": oz}),
    );
    assert!(
        row.contains("Raumbediengerät mit Display, {TE 01}\n{TE 02}\nAufputzmontage."),
        "{row}"
    );
    for (args, part) in [
        (
            json!({"projekt": project, "oz": oz, "vortext": "x", "nach": "gibt es nicht"}),
            "steht nicht im Langtext",
        ),
        (
            json!({"projekt": project, "oz": oz, "vortext": "x", "kennung": "1"}),
            "Kennung 1 hat schon",
        ),
        (
            json!({"projekt": project, "oz": oz, "vortext": "x", "nach": "Display", "absatz": 1}),
            "nicht beides",
        ),
        (
            json!({"projekt": project, "oz": "01", "vortext": "x"}),
            "Positionen, Hinweisen und Ausführungsbeschreibungen",
        ),
    ] {
        let error = call(&mut server, "bieterangabe_einfuegen", args).unwrap_err();
        assert!(error.contains(part), "{error}");
    }
    let check = ok(&mut server, "lv_pruefen", json!({"projekt": project}));
    assert!(
        check.contains(&format!(
            "Bieterangaben (vom Bieter einzutragen): 2 (1): {oz}"
        )),
        "{check}"
    );
    assert!(!check.contains("Bieterangabe ohne Vortext"), "{check}");
    assert!(
        check.contains("0 Fehler · 0 Warnungen"),
        "Fachregeln: {check}"
    );
    let target = folder.0.join("bieterangaben.x83");
    ok(
        &mut server,
        "gaeb_exportieren",
        json!({"projekt": project, "art": "x83", "ziel": target}),
    );
    let xml = std::fs::read_to_string(&target).unwrap();
    assert_eq!(xml.matches("Kind=\"Bidder\"").count(), 2, "{xml}");
    assert!(xml.contains("<ComplCaption><span"), "{xml}");

    // An image before the field: the field keeps its place in the text.
    let image = folder.0.join("schema.png");
    std::fs::write(&image, PNG).unwrap();
    ok(
        &mut server,
        "bild_einfuegen",
        json!({"projekt": project, "oz": oz, "datei": image, "absatz": 0}),
    );
    let row = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": oz}),
    );
    assert!(
        row.contains("Raumbediengerät mit Display, {TE 01}\n{TE 02}\nAufputzmontage."),
        "{row}"
    );
    let first = row
        .lines()
        .position(|line| line.starts_with("[Bild: schema.png"))
        .expect(&row);
    let field = row
        .lines()
        .position(|line| line.contains("{TE 01}"))
        .unwrap();
    assert!(first < field, "Bild vorne: {row}");
}

/// Kostenermittlung: anlegen (Schätzung), Zeilen schreiben, Berechnung daraus,
/// Aufteilung je Bauteil, lesen mit Prüfsumme, PDF.
#[test]
fn cost_estimates_from_the_template_to_the_pdf() {
    let folder = TempFolder::new("kosten");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Kosten");
    let empty = ok(&mut server, "kosten_lesen", json!({"projekt": project}));
    assert!(empty.contains("noch keine Kostenermittlung"), "{empty}");

    let answer = ok(
        &mut server,
        "kosten_anlegen",
        json!({"projekt": project, "stufe": "kostenschaetzung", "titel": "KS LPH 2", "stand": "15.06.2026", "bauteil": "Haus A", "kennwert_von": 400, "kennwert_bis": 500}),
    );
    assert!(
        answer.contains("KS LPH 2") && answer.contains("2. Ebene"),
        "{answer}"
    );
    let answer = ok(
        &mut server,
        "kosten_zeilen_schreiben",
        json!({"projekt": project, "zeilen": [
            {"bauteil": "Haus A", "ort": "ASP01", "gewerk": "RLT", "anlage": "RLT-01", "dp": {"AI": 12, "BI": 30, "AO": 6, "BO": 6}},
            {"bauteil": "Haus B", "gewerk": "HZG", "anlage": "Heizkreis", "dp": {"AI": 2, "BV": 2}}
        ]}),
    );
    // 58 DP × 450 € = 26.100 €.
    assert!(
        answer.contains("2 Zeilen angehängt") && answer.contains("26.100,00 €"),
        "{answer}"
    );
    assert!(
        answer.contains("Spanne 23.200,00 € bis 29.000,00 €"),
        "{answer}"
    );

    let answer = ok(
        &mut server,
        "kosten_anlegen",
        json!({"projekt": project, "aus": "KS LPH 2", "titel": "KB LPH 3"}),
    );
    // EP: (12+30+6+6+2) × 400 + 2 × 200 = 22.800 €.
    assert!(
        answer.contains("KB LPH 3") && answer.contains("22.800,00 €"),
        "{answer}"
    );
    let answer = ok(
        &mut server,
        "kosten_aendern",
        json!({"projekt": project, "kostenermittlung": 2, "bauteil": "Haus B", "aufteilung": {"481": 50, "484": 48}}),
    );
    assert!(answer.contains("98 % statt 100 %"), "{answer}");
    let read = ok(
        &mut server,
        "kosten_lesen",
        json!({"projekt": project, "kostenermittlung": "KB", "zeilen": true}),
    );
    assert!(
        read.contains("Bauteil „Haus B“") && read.contains("(eigene Aufteilung)"),
        "{read}"
    );
    assert!(
        read.contains("Prüfsumme -24,00 €"),
        "1.200 € × 2 % fehlen: {read}"
    );
    assert!(read.contains("| RLT-01 |"), "Anlagenliste: {read}");
    let list = ok(&mut server, "kosten_lesen", json!({"projekt": project}));
    assert!(
        list.contains("| 1 | Kostenschätzung (2. Ebene) | KS LPH 2 |"),
        "{list}"
    );

    // Without zeilen nothing is written (the CSV import is gone, Anwender 18.11.).
    let error = call(
        &mut server,
        "kosten_zeilen_schreiben",
        json!({"projekt": project, "kostenermittlung": 1, "csv": "/tmp/dp.csv"}),
    )
    .unwrap_err();
    assert!(error.contains("zeilen angeben"), "{error}");

    let answer = ok(
        &mut server,
        "pdf_drucken",
        json!({"projekt": project, "dokument": "kostenermittlung", "kostenermittlung": "KB", "deckblatt": true}),
    );
    assert!(answer.contains("Seiten"), "{answer}");
    let pdf = folder.0.join("Kosten").join("Kosten_Kostenberechnung.pdf");
    assert!(
        std::fs::read(&pdf).unwrap().starts_with(b"%PDF"),
        "{answer}"
    );

    assert!(call(
        &mut server,
        "kosten_lesen",
        json!({"projekt": project, "kostenermittlung": "XY"})
    )
    .is_err());
    let answer = ok(
        &mut server,
        "kosten_aendern",
        json!({"projekt": project, "kostenermittlung": 1, "loeschen": true}),
    );
    assert!(answer.contains("Gelöscht: KS LPH 2"), "{answer}");
}

/// Dieselben Grenzen wie in der Oberfläche (`proava_domain::input_limits`):
/// zu große, zu genaue oder negative Werte lehnt der MCP-Server mit klarer
/// Meldung ab und ändert nichts; zu lange Texte nennt er als Hinweis (die
/// Oberfläche schneidet auch nichts ab).
#[test]
fn values_beyond_the_input_limits_are_refused() {
    let folder = TempFolder::new("grenzen");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Grenzen");
    ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "zeilen": [{"titel": "Automation"}]}),
    );
    let refused = |server: &mut Server, tool: &str, args: Value, part: &str| {
        let error =
            call(server, tool, args.clone()).expect_err(&format!("{tool} {args} angenommen"));
        assert!(error.contains(part), "{tool} {args}: {error}");
    };

    // Positionen: Menge und EP mit drei Nachkommastellen, unter 100 Mio.
    for (field, value, part) in [
        (
            "menge",
            json!(100_000_000),
            "„menge“: die Zahl ist zu groß (höchstens 99.999.999,999).",
        ),
        ("menge", json!(1e30), "„menge“: die Zahl ist zu groß"),
        (
            "menge",
            json!("0,0001"),
            "„menge“: höchstens 3 Nachkommastellen.",
        ),
        (
            "ep",
            json!("1,23456"),
            "„ep“: höchstens 3 Nachkommastellen.",
        ),
        ("ep", json!(-123_456_789), "„ep“: die Zahl ist zu groß"),
        (
            "ep",
            json!("79228162514264337593543950335"),
            "„ep“: die Zahl ist zu groß",
        ),
    ] {
        let mut args =
            json!({"projekt": project, "unter_titel": "01", "kurztext": "DDC", "einheit": "St"});
        args[field] = value;
        refused(&mut server, "position_einfuegen", args, part);
    }
    let lv = ok(&mut server, "lv_lesen", json!({"projekt": project}));
    assert!(!lv.contains("DDC"), "nichts eingefügt: {lv}");
    // At the limit and German like the input fields („1.150“ = 1150).
    let long = "Raumbediengerät mit Temperaturfühler, Sollwertsteller und Präsenztaste, Aufputz";
    let answer = ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "unter_titel": "01", "kurztext": long, "menge": "1.150", "einheit": "Stück", "ep": 99_999_999.999}),
    );
    assert!(
        answer.contains("1.150") && answer.contains("99.999.999,999"),
        "{answer}"
    );
    assert!(
        answer.contains(&format!(
            "Hinweis: Kurztext hat {} Zeichen",
            long.chars().count()
        )),
        "{answer}"
    );
    assert!(
        answer.contains("Hinweis: Einheit „Stück“ hat 5 Zeichen"),
        "{answer}"
    );
    let oz = answer
        .trim_start_matches("Position eingefügt: ")
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();
    refused(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": oz, "menge": "123.456.789"}),
        "„menge“: die Zahl ist zu groß",
    );
    refused(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "zeilen": [{"kurztext": "x", "ep": 1e9}]}),
        "„ep“: die Zahl ist zu groß",
    );
    let row = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": oz}),
    );
    assert!(row.contains("1.150"), "Menge unverändert: {row}");

    // Kostenermittlung: Kennwert, Spanne, Preise, Anteile (zwei Nachkommastellen, nicht negativ), DP-Anzahlen.
    ok(
        &mut server,
        "kosten_anlegen",
        json!({"projekt": project, "stufe": "kostenberechnung", "titel": "KB"}),
    );
    for (args, part) in [
        (
            json!({"kennwert": 100_000_000}),
            "„kennwert“: die Zahl ist zu groß (höchstens 99.999.999,99).",
        ),
        (
            json!({"kennwert": "450,005"}),
            "„kennwert“: höchstens 2 Nachkommastellen.",
        ),
        (
            json!({"kennwert_von": -5}),
            "„kennwert_von“: keine negativen Werte.",
        ),
        (
            json!({"aufteilung": {"481": 1e9}}),
            "aufteilung KG 481: die Zahl ist zu groß",
        ),
        (
            json!({"aufteilung": {"481": -10, "484": 110}}),
            "aufteilung KG 481: keine negativen Werte.",
        ),
        (
            json!({"dp_arten": [{"kuerzel": "AI", "preis": 1e12}]}),
            "„preis“: die Zahl ist zu groß",
        ),
    ] {
        let mut args = args;
        args["projekt"] = json!(project);
        args["kostenermittlung"] = json!(1);
        refused(&mut server, "kosten_aendern", args, part);
    }
    refused(
        &mut server,
        "kosten_zeilen_schreiben",
        json!({"projekt": project, "zeilen": [{"anlage": "RLT-01", "dp": {"AI": 4_000_000_000u64}}]}),
        "dp.AI: die Zahl ist zu groß (höchstens 99.999.999).",
    );
    refused(
        &mut server,
        "kosten_zeilen_schreiben",
        json!({"projekt": project, "zeilen": [{"anlage": "RLT-01", "dp": {"AI": 100_000_000}}]}),
        "dp.AI: die Zahl ist zu groß",
    );
    let read = ok(
        &mut server,
        "kosten_lesen",
        json!({"projekt": project, "kostenermittlung": 1, "zeilen": true}),
    );
    assert!(!read.contains("RLT-01"), "nichts geschrieben: {read}");

    // Nachlass: über 0 und unter 100 %, Betrag über 0 € mit zwei Nachkommastellen (0 = kein Nachlass).
    // (GAEB 90 carries a total of at most 12 digits: a smaller EP for the D84.)
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": oz, "ep": 10}),
    );
    let bids = folder.0.join("Angebote");
    std::fs::create_dir_all(&bids).unwrap();
    write_d84(&project, &bids.join("a.D84"), "Firma A", "0.9");
    ok(
        &mut server,
        "angebot_einlesen",
        json!({"projekt": project, "datei": bids.join("a.D84")}),
    );
    for (args, part) in [
        (
            json!({"prozent": 100}),
            "Der Nachlass muss zwischen 0 und 100 % liegen.",
        ),
        (json!({"prozent": -3}), "zwischen 0 und 100 %"),
        (json!({"betrag": -5}), "„betrag“: keine negativen Werte."),
        (
            json!({"betrag": 1e9}),
            "„betrag“: die Zahl ist zu groß (höchstens 99.999.999,99).",
        ),
        (
            json!({"betrag": "12,345"}),
            "„betrag“: höchstens 2 Nachkommastellen.",
        ),
    ] {
        let mut args = args;
        args["projekt"] = json!(project);
        args["angebot"] = json!(1);
        refused(&mut server, "angebot_nachlass_setzen", args, part);
    }
    let answer = ok(
        &mut server,
        "angebot_nachlass_setzen",
        json!({"projekt": project, "angebot": 1, "prozent": "3,5"}),
    );
    assert!(answer.contains("Nachlass 3,5 %"), "{answer}");
}

/// „Lokale Testkopie“ der Katalogverzeichnisse: synthetische Listen im
/// veröffentlichten Aufbau (Hersteller und Nummern ausgedacht), aus einem
/// Temp-Ordner gelesen – keine Netzabfrage. Merkt sich jede Adresse.
struct LocalLists {
    folder: PathBuf,
    asked: std::sync::Mutex<Vec<String>>,
}

impl LocalLists {
    fn file(url: &str) -> String {
        url.trim_start_matches("https://ausschreiben.de/")
            .replace('/', "_")
    }
}

impl proava_application::WebTexts for LocalLists {
    fn get_text(&self, url: &str, _max_age_days: u32) -> Result<String, String> {
        self.asked.lock().unwrap().push(url.to_string());
        std::fs::read_to_string(self.folder.join(Self::file(url)))
            .map_err(|_| format!("nicht in der Testkopie: {url}"))
    }
}

fn local_lists(folder: &Path) -> std::sync::Arc<LocalLists> {
    let lists = [
        (
            "https://ausschreiben.de/robots.txt",
            "User-agent: *\nDisallow: /search/\nDisallow: /*searchTerm=\nDisallow: /catalog/*?*\nDisallow: /v2/\n",
        ),
        (
            "https://ausschreiben.de/catalogs/llms.txt",
            "# Catalogs\n\n- [Muster Antriebe](https://ausschreiben.de/catalog/muster_antriebe/llms.txt): Muster Antriebe GmbH, German\n- [Beispiel Fühler](https://ausschreiben.de/catalog/beispiel/llms.txt): Beispiel Sensorik AG, German\n",
        ),
        (
            "https://ausschreiben.de/catalog/muster_antriebe/llms.txt",
            "# Muster Antriebe\n\n- Drehantriebe (3 items): [page 1](https://ausschreiben.de/catalog/muster_antriebe/llms-1.txt)\n",
        ),
        (
            "https://ausschreiben.de/catalog/muster_antriebe/llms-1.txt",
            "# Muster Antriebe – page 1 of 1\n\n## Antriebe\n\n### Drehantriebe\n\n- [Drehantrieb 20 Nm, stetig](https://ausschreiben.de/catalog/muster_antriebe/catalogItem/aaaa-1) – Product number MA-20-SR\n- [Drehantrieb 10 Nm, Auf-Zu](https://ausschreiben.de/catalog/muster_antriebe/catalogItem/aaaa-2) – Product number MA-10-AZ\n- [Drehantrieb 20 Nm, Auf-Zu](https://ausschreiben.de/catalog/muster_antriebe/catalogItem/aaaa-3) – Product number MA-20-AZ\n",
        ),
    ];
    for (url, text) in lists {
        std::fs::write(folder.join(LocalLists::file(url)), text).unwrap();
    }
    std::sync::Arc::new(LocalLists {
        folder: folder.to_path_buf(),
        asked: std::sync::Mutex::new(Vec::new()),
    })
}

#[test]
fn ausschreiben_search_reads_only_the_published_lists() {
    let folder = TempFolder::new("ausschreiben-suche");
    let lists = local_lists(&folder.0);
    let mut server = server(&folder.0).with_web(lists.clone());

    let answer = ok(
        &mut server,
        "ausschreiben_suchen",
        json!({"suche": "Muster Drehantrieb 20 Nm"}),
    );
    assert!(
        answer.contains("2 Treffer in Muster Antriebe (Muster Antriebe GmbH)"),
        "{answer}"
    );
    assert!(answer.contains("- Drehantrieb 20 Nm, stetig · Art.-Nr. MA-20-SR · Muster Antriebe › Antriebe › Drehantriebe · https://ausschreiben.de/catalog/muster_antriebe/catalogItem/aaaa-1"), "{answer}");
    assert!(!answer.contains("10 Nm"), "{answer}");
    assert!(answer.contains("ausschreiben_oeffnen"), "{answer}");
    let asked = lists.asked.lock().unwrap().clone();
    assert_eq!(
        asked[0], "https://ausschreiben.de/robots.txt",
        "robots.txt zuerst"
    );
    assert!(
        asked
            .iter()
            .all(|url| proava_application::tender_index::is_allowed(url)),
        "nur veröffentlichte Verzeichnisse: {asked:?}"
    );

    let limited = ok(
        &mut server,
        "ausschreiben_suchen",
        json!({"suche": "Muster Drehantrieb", "max_treffer": 1}),
    );
    assert!(limited.contains("3 Treffer"), "{limited}");
    assert!(limited.contains("… 2 weitere"), "{limited}");

    let no_maker = call(
        &mut server,
        "ausschreiben_suchen",
        json!({"suche": "Drehantrieb"}),
    )
    .unwrap_err();
    assert!(no_maker.contains("Hersteller"), "{no_maker}");
    assert!(call(&mut server, "ausschreiben_suchen", json!({"suche": " "})).is_err());

    // Ohne Internet (Vorgabe des Servers): klare Meldung, nichts gestartet.
    let offline = call(
        &mut crate::server(&folder.0),
        "ausschreiben_suchen",
        json!({"suche": "Muster"}),
    )
    .unwrap_err();
    assert!(offline.contains("robots.txt"), "{offline}");
}

/// Merkt sich die Seiten statt ein Fenster zu starten.
struct RecordingWindow(std::sync::Arc<std::sync::Mutex<Vec<String>>>);

impl proava_mcp::BrowserWindow for RecordingWindow {
    fn open(&mut self, url: &str) -> Result<String, String> {
        self.0.lock().unwrap().push(url.to_string());
        Ok(format!("Geöffnet für den Anwender: {url}"))
    }
}

#[test]
fn ausschreiben_open_shows_only_pages_of_the_site_and_starts_nothing_in_tests() {
    let folder = TempFolder::new("ausschreiben-oeffnen");
    let opened = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut server =
        server(&folder.0).with_browser_window(Box::new(RecordingWindow(opened.clone())));
    let page = "https://ausschreiben.de/catalog/muster_antriebe/catalogItem/aaaa-1";
    assert!(ok(&mut server, "ausschreiben_oeffnen", json!({"url": page})).contains(page));
    ok(&mut server, "ausschreiben_oeffnen", json!({}));
    for foreign in [
        "https://example.com/",
        "http://ausschreiben.de/",
        "file:///etc/passwd",
        "https://ausschreiben.de.example.com/x",
    ] {
        let error = call(&mut server, "ausschreiben_oeffnen", json!({"url": foreign})).unwrap_err();
        assert!(
            error.contains("Nur Seiten von AUSSCHREIBEN.DE"),
            "{foreign}: {error}"
        );
    }
    assert_eq!(
        *opened.lock().unwrap(),
        vec![page.to_string(), "https://ausschreiben.de".to_string()]
    );

    // Ohne Fenster (Vorgabe des Servers): der Link für den Browser.
    let without = call(
        &mut crate::server(&folder.0),
        "ausschreiben_oeffnen",
        json!({"url": page}),
    )
    .unwrap_err();
    assert!(without.contains(page), "{without}");

    let tools = proava_mcp_tools(&mut crate::server(&folder.0));
    let tool = |name: &str| {
        tools
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap()
            .clone()
    };
    assert_eq!(
        tool("ausschreiben_suchen")["annotations"]["readOnlyHint"],
        true
    );
    assert_eq!(
        tool("ausschreiben_suchen")["annotations"]["openWorldHint"],
        true
    );
    assert_eq!(
        tool("ausschreiben_oeffnen")["annotations"]["readOnlyHint"],
        false
    );
    assert!(
        tool("ausschreiben_suchen")["inputSchema"]["properties"]["lv"].is_null(),
        "kein Projekt, kein LV"
    );
}

fn proava_mcp_tools(server: &mut Server) -> Vec<Value> {
    let answer = server
        .handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}))
        .unwrap();
    answer["result"]["tools"].as_array().unwrap().clone()
}

/// `zeile_umwandeln` (Anwender 18.11.): eine Position wird Hinweis – mit
/// Vorschau, solange etwas wegfiele –, ein Hinweis wird Ausführungsbeschreibung
/// und zurück Position mit freier OZ; Text bleibt.
#[test]
fn rows_change_their_kind() {
    let folder = TempFolder::new("umwandeln");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Arten");
    ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "zeilen": [
            {"titel": "GA", "ebene": 1},
            {"titel": "Management"},
            {"kurztext": "Standardbeschreibung Management", "menge": 2, "einheit": "St", "ep": 10,
             "langtext": "Gefordert sind die Einrichtungen."},
            {"kurztext": "Danach", "menge": 1, "einheit": "St"}
        ]}),
    );
    let preview = ok(
        &mut server,
        "zeile_umwandeln",
        json!({"projekt": project, "oz": "01.01.0010", "zeilenart": "hinweis"}),
    );
    assert!(
        preview.contains("Vorschau")
            && preview.contains("Menge 2 St")
            && preview.contains("Nichts gespeichert"),
        "{preview}"
    );
    let lv = ok(&mut server, "lv_lesen", json!({"projekt": project}));
    assert!(
        lv.contains("01.01.0010 | Pos | Standardbeschreibung"),
        "unverändert: {lv}"
    );

    let answer = ok(
        &mut server,
        "zeile_umwandeln",
        json!({"projekt": project, "oz": "01.01.0010", "zeilenart": "hinweis", "bestaetigen": true}),
    );
    assert!(answer.contains("Umgewandelt in Hinweis"), "{answer}");
    let lv = ok(
        &mut server,
        "lv_lesen",
        json!({"projekt": project, "mit_langtext": true}),
    );
    assert!(lv.contains("Hinweis"), "{lv}");
    assert!(lv.contains("Gefordert sind die Einrichtungen."), "{lv}");
    assert!(
        lv.contains("01.01.0010 | Pos | Danach"),
        "fortlaufend neu nummeriert: {lv}"
    );

    // Hinweis → Ausführungsbeschreibung: nichts geht verloren, also sofort.
    let id: String = answer
        .rsplit("(#")
        .next()
        .unwrap()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let answer = ok(
        &mut server,
        "zeile_umwandeln",
        json!({"projekt": project, "id": id.parse::<u64>().unwrap(), "zeilenart": "ausfuehrungsbeschreibung"}),
    );
    assert!(
        answer.contains("Umgewandelt in Ausführungsbeschreibung"),
        "{answer}"
    );
    let answer = ok(
        &mut server,
        "zeile_umwandeln",
        json!({"projekt": project, "id": id.parse::<u64>().unwrap(), "zeilenart": "position", "bestaetigen": true}),
    );
    assert!(
        answer.contains("01.01.0010 „Standardbeschreibung Management“"),
        "{answer}"
    );
    let error = call(
        &mut server,
        "zeile_umwandeln",
        json!({"projekt": project, "oz": "01", "zeilenart": "hinweis"}),
    )
    .unwrap_err();
    assert!(error.contains("Titel"), "{error}");
}

/// Vorbemerkungen (GAEB AddText) per MCP: hinweis_einfuegen und
/// positionen_einfuegen mit `vorbemerkung`, zeile_aendern schaltet um;
/// GAEB DA XML trägt sie vor dem LV, die übrigen Hinweise bleiben Remark.
#[test]
fn preliminary_notes_are_written_before_the_lv() {
    let folder = TempFolder::new("vorbemerkung");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Rathaus 3");
    ok(
        &mut server,
        "titel_einfuegen",
        json!({"projekt": project, "kurztext": "Automation"}),
    );
    let answer = ok(
        &mut server,
        "hinweis_einfuegen",
        json!({"projekt": project, "am_anfang": true, "unter_titel": "01", "vorbemerkung": true,
               "kurztext": "Allgemeines", "text": "Die Texte gelten für **alle** Geräte.\n- Musterstadt"}),
    );
    assert!(answer.starts_with("Vorbemerkung eingefügt"), "{answer}");
    let answer = ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "unter_titel": "01", "zeilen": [
            {"hinweis": "Zweite Vorbemerkung", "kurztext": "Baustelle", "vorbemerkung": true},
            {"hinweis": "Hinweis im Titel"},
            {"kurztext": "Raumfühler", "menge": 3, "einheit": "St", "ep": 40}
        ]}),
    );
    assert!(answer.contains("3 Zeilen eingefügt"), "{answer}");
    let lv = ok(&mut server, "lv_lesen", json!({"projekt": project}));
    assert_eq!(lv.matches("| Vorbemerkung |").count(), 2, "{lv}");
    assert_eq!(lv.matches("| Hinweis |").count(), 1, "{lv}");
    let id = lv
        .lines()
        .find(|line| line.contains("Vorbemerkung | Baustelle"))
        .and_then(|line| line.rsplit('#').next())
        .unwrap()
        .trim()
        .parse::<u64>()
        .unwrap();
    let row = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "id": id}),
    );
    assert!(row.starts_with(&format!("Vorbemerkung (#{id})")), "{row}");

    let x83 = folder.0.join("vorbemerkung.X83");
    ok(
        &mut server,
        "gaeb_exportieren",
        json!({"projekt": project, "art": "X83", "ziel": x83}),
    );
    let gaeb = proava_gaeb::xml::read_file(&x83).unwrap();
    let headings: Vec<&str> = gaeb
        .award
        .add_texts
        .iter()
        .map(|text| text.outline.as_str())
        .collect();
    assert_eq!(headings, ["Allgemeines", "Baustelle"]);
    let remarks = gaeb
        .award
        .boq
        .body
        .iter()
        .flat_map(|node| match node {
            BoqNode::Category(category) => category.body.clone(),
            other => vec![other.clone()],
        })
        .filter(|node| matches!(node, BoqNode::Remark(_)))
        .count();
    assert_eq!(remarks, 1);

    let answer = ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "id": id, "vorbemerkung": false}),
    );
    assert!(answer.contains("Vorbemerkung"), "{answer}");
    let lv = ok(&mut server, "lv_lesen", json!({"projekt": project}));
    assert_eq!(lv.matches("| Vorbemerkung |").count(), 1, "{lv}");
    assert_eq!(lv.matches("| Hinweis |").count(), 2, "{lv}");
}

/// Schlussbemerkung (Anwender 18.11., „Zulieferungen originalgetreu“): ein Text
/// nach dem LV per hinweis_einfuegen mit `schlussbemerkung` (ohne Ort ans Ende),
/// lv_lesen zeigt die Art, GAEB DA XML trägt ihn nach dem LV; nie zusammen mit
/// `vorbemerkung`; zeile_aendern schaltet um.
#[test]
fn a_closing_note_goes_after_the_lv() {
    let folder = TempFolder::new("schlussbemerkung");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Rathaus 3");
    ok(
        &mut server,
        "titel_einfuegen",
        json!({"projekt": project, "kurztext": "Automation"}),
    );
    ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "unter_titel": "01", "kurztext": "Raumfühler", "menge": 3, "einheit": "St"}),
    );
    let answer = ok(
        &mut server,
        "hinweis_einfuegen",
        json!({"projekt": project, "schlussbemerkung": true, "kurztext": "Schlussblatt",
               "text": "Der Bieter gibt die Zahl der Arbeitskräfte an."}),
    );
    assert!(
        answer.starts_with("Schlussbemerkung eingefügt hinter") && answer.contains("Raumfühler"),
        "am Ende des LV: {answer}"
    );
    let lv = ok(&mut server, "lv_lesen", json!({"projekt": project}));
    let last = lv.lines().rev().find(|line| line.contains(" | ")).unwrap();
    assert!(last.contains("| Schlussbemerkung | Schlussblatt"), "{lv}");
    let error = call(
        &mut server,
        "hinweis_einfuegen",
        json!({"projekt": project, "schlussbemerkung": true, "vorbemerkung": true, "text": "x"}),
    )
    .unwrap_err();
    assert!(error.contains("nicht beides"), "{error}");

    let x83 = folder.0.join("schluss.X83");
    ok(
        &mut server,
        "gaeb_exportieren",
        json!({"projekt": project, "art": "X83", "ziel": x83}),
    );
    let text = std::fs::read_to_string(&x83).unwrap();
    assert!(text.find("Schlussblatt").unwrap() > text.find("</Award>").unwrap());
    let gaeb = proava_gaeb::xml::read_file(&x83).unwrap();
    assert_eq!(gaeb.award.closing_texts[0].outline, "Schlussblatt");
    assert!(gaeb.award.add_texts.is_empty());

    let id = last
        .rsplit('#')
        .next()
        .unwrap()
        .trim()
        .parse::<u64>()
        .unwrap();
    let answer = ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "id": id, "schlussbemerkung": false}),
    );
    assert!(answer.contains("Schlussbemerkung"), "{answer}");
    let lv = ok(&mut server, "lv_lesen", json!({"projekt": project}));
    assert!(!lv.contains("| Schlussbemerkung |"), "{lv}");
}

/// Positionen direkt unter einem Titel über der untersten Ebene (wie sie der
/// GAEB-Import aus anderen Programmen übernimmt): per MCP mit eigener OZ
/// „01.1“ → 01.0001 in einer Maske 11.22.PPPP.I; GAEB schreibt sie ohne
/// Zwischenstufe unter den Titel.
#[test]
fn positions_right_under_an_upper_title_get_their_ordinal() {
    let folder = TempFolder::new("oberer-titel");
    let mut server = server(&folder.0);
    let answer = ok(
        &mut server,
        "projekt_anlegen",
        json!({"ordner": folder.0, "name": "Rathaus 3", "oz_maske": "11.22.PPPP.I", "schrittweite": 1}),
    );
    assert!(answer.contains("Projekt angelegt"), "{answer}");
    let project = folder
        .0
        .join("Rathaus 3")
        .join("Rathaus 3.proava")
        .to_string_lossy()
        .to_string();
    ok(
        &mut server,
        "titel_einfuegen",
        json!({"projekt": project, "kurztext": "Installation", "oz": "01"}),
    );
    let answer = ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "unter_titel": "01", "oz": "01.1", "kurztext": "Leitung", "menge": 10, "einheit": "m", "ep": 2.5}),
    );
    assert!(answer.contains("01.0001 „Leitung“"), "{answer}");
    let answer = ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "unter_titel": "01", "oz": "01.12", "kurztext": "Dose", "menge": 4, "einheit": "St", "ep": 3}),
    );
    assert!(answer.contains("01.0012 „Dose“"), "{answer}");
    // Zeitansatz je Einheit (GAEB TimeQu)
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.0012", "zeitansatz": "0,05"}),
    );
    assert!(call(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.0012", "zeitansatz": "0,000001"})
    )
    .unwrap_err()
    .contains("vier Nachkommastellen"));
    let error = call(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "unter_titel": "01", "oz": "01.0001", "kurztext": "doppelt", "menge": 1, "einheit": "St"}),
    )
    .unwrap_err();
    assert!(error.contains("schon vergeben"), "{error}");
    let error = call(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "unter_titel": "01", "oz": "02.3", "kurztext": "falscher Titel", "menge": 1, "einheit": "St"}),
    )
    .unwrap_err();
    assert!(error.contains("passt nicht"), "{error}");
    // die volle Tiefe geht weiter wie bisher (GAEB: in einer Stufe nur Titel
    // oder nur Positionen – deshalb ein zweiter Titel)
    ok(
        &mut server,
        "titel_einfuegen",
        json!({"projekt": project, "kurztext": "Automation", "oz": "02"}),
    );
    ok(
        &mut server,
        "titel_einfuegen",
        json!({"projekt": project, "kurztext": "Bereich", "unter_titel": "02", "oz": "02.01"}),
    );
    let answer = ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "unter_titel": "02.01", "oz": "02.01.0010", "kurztext": "Fühler", "menge": 2, "einheit": "St", "ep": 50}),
    );
    assert!(answer.contains("02.01.0010 „Fühler“"), "{answer}");

    // Zeiteinheit des Zeitansatzes (GAEB LblTime), wie andere Programme sie schreiben
    let answer = ok(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": project, "zeiteinheit": "Stunden"}),
    );
    assert!(answer.contains("Zeiteinheit"), "{answer}");
    let overview = ok(&mut server, "projekt_lesen", json!({"projekt": project}));
    assert!(overview.contains("Zeiteinheit: Stunden"), "{overview}");
    assert!(call(
        &mut server,
        "projektangaben_setzen",
        json!({"projekt": project, "zeiteinheit": "Arbeitsstunden je Einheit"})
    )
    .unwrap_err()
    .contains("20 Zeichen"));
    let x82 = folder.0.join("oberer-titel.X82");
    ok(
        &mut server,
        "gaeb_exportieren",
        json!({"projekt": project, "art": "X82", "ziel": x82}),
    );
    let gaeb = proava_gaeb::xml::read_file(&x82).unwrap();
    let ordinals: Vec<String> = gaeb
        .award
        .boq
        .items()
        .into_iter()
        .map(|(pieces, _)| pieces.join("."))
        .collect();
    assert_eq!(ordinals, ["01.0001", "01.0012", "02.01.0010"]);
    assert_eq!(gaeb.award.boq.time_label, "Stunden");
    let times: Vec<Option<Decimal>> = gaeb
        .award
        .boq
        .items()
        .into_iter()
        .map(|(_, item)| item.time_quantity)
        .collect();
    assert_eq!(times, [None, Some(Decimal::new(5, 2)), None]);
    let row = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.0012"}),
    );
    assert!(row.contains("Zeitansatz: 0,05 Stunden"), "{row}");
    assert_eq!(
        gaeb.award
            .boq
            .totals
            .as_ref()
            .and_then(|totals| totals.total),
        Some(Decimal::new(13700, 2))
    );
}

/// Textergänzungen auch in Ausführungsbeschreibungen und Hinweisen (GAEB
/// erlaubt TextComplement in jedem DetailTxt; andere Programme liefern sie
/// z. B. als „Fabrikat: …“ in einer Ausführungsbeschreibung).
#[test]
fn complements_in_descriptions_and_hints() {
    let folder = TempFolder::new("te-ab");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Rathaus 3");
    ok(
        &mut server,
        "titel_einfuegen",
        json!({"projekt": project, "kurztext": "Installation"}),
    );
    let answer = ok(
        &mut server,
        "ausfuehrungsbeschreibung_einfuegen",
        json!({"projekt": project, "unter_titel": "01", "bezeichnung": "Schalter", "langtext": "Schalterprogramm wie folgt:\n{TE 1}\nFarbe weiß.",
               "textergaenzungen": [{"kennung": 1, "art": "Bieter", "vortext": "Fabrikat: "}]}),
    );
    assert!(
        answer.contains("Ausführungsbeschreibung A0001 eingefügt"),
        "{answer}"
    );
    let answer = ok(
        &mut server,
        "hinweis_einfuegen",
        json!({"projekt": project, "unter_titel": "01", "text": "Gilt für {TE 2} Geräte.",
               "textergaenzungen": [{"kennung": 2, "art": "AG", "inhalt": "alle"}]}),
    );
    assert!(answer.contains("Hinweis eingefügt"), "{answer}");
    let error = call(
        &mut server,
        "hinweis_einfuegen",
        json!({"projekt": project, "text": "Ohne Angabe {TE 9}"}),
    )
    .unwrap_err();
    assert!(
        error.contains("keine Textergänzung mit Kennung 9"),
        "{error}"
    );
    ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "unter_titel": "01", "kurztext": "Schalter", "menge": 3, "einheit": "St", "ausfuehrungsbeschreibung": "A0001"}),
    );
    let x83 = folder.0.join("te-ab.X83");
    ok(
        &mut server,
        "gaeb_exportieren",
        json!({"projekt": project, "art": "X83", "ziel": x83}),
    );
    let gaeb = proava_gaeb::xml::read_file(&x83).unwrap();
    let mut performance = None;
    let mut remark = None;
    fn walk<'a>(
        nodes: &'a [BoqNode],
        performance: &mut Option<&'a proava_gaeb::PerfDescr>,
        remark: &mut Option<&'a proava_gaeb::Remark>,
    ) {
        for node in nodes {
            match node {
                BoqNode::Category(category) => walk(&category.body, performance, remark),
                BoqNode::PerfDescr(found) => *performance = Some(found),
                BoqNode::Remark(found) => *remark = Some(found),
                BoqNode::Item(_) => {}
            }
        }
    }
    walk(&gaeb.award.boq.body, &mut performance, &mut remark);
    let performance = performance.expect("Ausführungsbeschreibung in der X83");
    assert_eq!(performance.complements.len(), 1);
    assert_eq!(performance.complements[0].caption.trim(), "Fabrikat:");
    let remark = remark.expect("Hinweis in der X83");
    assert_eq!(remark.complements.len(), 1);
    assert_eq!(remark.complements[0].body, "alle");
}

/// Wiederholungsbeschreibung („wie Position …, jedoch“): per MCP mit
/// `wiederholung_von`; GAEB schreibt RefDescr Rep mit RefRNo, die
/// Bezugsposition bekommt Ref.
#[test]
fn repetition_descriptions_are_set_and_written() {
    let folder = TempFolder::new("wiederholung");
    let mut server = server(&folder.0);
    ok(
        &mut server,
        "projekt_anlegen",
        json!({"ordner": folder.0, "name": "Rathaus 3", "oz_maske": "11.PPPP", "schrittweite": 10}),
    );
    let project = folder
        .0
        .join("Rathaus 3")
        .join("Rathaus 3.proava")
        .to_string_lossy()
        .to_string();
    ok(
        &mut server,
        "titel_einfuegen",
        json!({"projekt": project, "kurztext": "Ventile"}),
    );
    let answer = ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "unter_titel": "01", "kurztext": "Zonenventil DN20", "menge": 10, "einheit": "St", "ep": 200, "langtext": "Druckunabhängiges Zonenventil DN20"}),
    );
    assert!(answer.contains("01.0010"), "{answer}");
    ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "unter_titel": "01", "zeilen": [
            {"kurztext": "Zonenventil DN15", "menge": 5, "einheit": "St", "ep": 190, "langtext": "Jedoch DN15", "wiederholung_von": "01.0010"}
        ]}),
    );
    let answer = ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "unter_titel": "01", "kurztext": "Zonenventil DN25", "menge": 2, "einheit": "St", "ep": 230, "langtext": "Jedoch DN25", "wiederholung_von": "1.10"}),
    );
    assert!(answer.contains("01.0030"), "{answer}");
    let row = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "oz": "01.0020"}),
    );
    assert!(row.contains("Wiederholung von Position 01.0010"), "{row}");
    assert!(call(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.0030", "wiederholung_von": "01"})
    )
    .unwrap_err()
    .contains("keine Position"));
    let answer = ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "oz": "01.0030", "wiederholung_von": ""}),
    );
    assert!(answer.contains("Wiederholung"), "{answer}");
    let x83 = folder.0.join("wiederholung.X83");
    ok(
        &mut server,
        "gaeb_exportieren",
        json!({"projekt": project, "art": "X83", "ziel": x83}),
    );
    let text = std::fs::read_to_string(&x83).unwrap();
    assert_eq!(
        text.matches("<RefDescr>Ref</RefDescr>").count(),
        1,
        "{text}"
    );
    assert_eq!(
        text.matches("<RefDescr>Rep</RefDescr>").count(),
        1,
        "{text}"
    );
    let gaeb = proava_gaeb::xml::read_file(&x83).unwrap();
    let repeats: Vec<Option<String>> = gaeb
        .award
        .boq
        .items()
        .into_iter()
        .map(|(_, item)| item.kind.repetition_of.clone())
        .collect();
    assert_eq!(repeats[1].as_deref(), Some("01.0010"), "{repeats:?}");
    assert_eq!(repeats[2], None);
}

/// Kosten-LV (Kostenermittlung als Kurztext-LV): anlegen mit Titeln nach KG,
/// Positionen schreiben, lesen, prüfen, drucken; Vergabewerkzeuge lehnen es
/// ab; Kostenberechnung aus der Kostenschätzung.
#[test]
fn cost_lvs_are_kurztext_lvs_that_never_go_to_bidders() {
    let folder = TempFolder::new("kosten-lv");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "KostenLV");

    let answer = ok(
        &mut server,
        "lv_anlegen",
        json!({"projekt": project, "bezeichnung": "Kostenschätzung Heizung", "nummer": "KS", "art": "kostenschaetzung", "kostengruppen": ["420", "430"]}),
    );
    assert!(
        answer.contains("Kostenschätzung (Kosten-LV)") && answer.contains("111.PPPP"),
        "{answer}"
    );
    let lvs = ok(&mut server, "lvs_lesen", json!({"projekt": project}));
    assert!(
        lvs.contains("| KS | Kostenschätzung Heizung | Kostenschätzung (Kosten-LV) |"),
        "{lvs}"
    );
    assert!(lvs.contains("| Ausschreibung |"), "{lvs}");
    let lv = ok(
        &mut server,
        "lv_lesen",
        json!({"projekt": project, "lv": "KS"}),
    );
    assert!(
        lv.contains("420") && lv.contains("Wärmeversorgungsanlagen") && lv.contains("430"),
        "{lv}"
    );
    ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "lv": "KS", "unter_titel": "420", "kurztext": "Wärmeerzeugung Wärmepumpe", "menge": 40, "einheit": "kW", "ep": 1500}),
    );

    // Vergabe and Abrechnung refuse the Kosten-LV.
    for (tool, extra) in [
        ("bieter_anlegen", json!({"name1": "Muster GmbH"})),
        ("auftrag_erteilen", json!({"angebot": 1})),
        ("rechnung_anlegen", json!({})),
        ("gaeb_exportieren", json!({"art": "X83"})),
        ("pdf_drucken", json!({"dokument": "angebotsaufforderung"})),
    ] {
        let mut arguments = json!({"projekt": project, "lv": "KS"});
        arguments
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let error = call(&mut server, tool, arguments).unwrap_err();
        assert!(
            error.contains("Kosten-LV") && error.contains("Ausschreibungs-LVs"),
            "{tool}: {error}"
        );
    }

    let costs = ok(&mut server, "kosten_lesen", json!({"projekt": project}));
    assert!(
        costs.contains("1 Kosten-LV") && costs.contains("420: 60.000,00"),
        "{costs}"
    );

    // Kostenberechnung aus der Kostenschätzung: copied, KG 420 too coarse now.
    let answer = ok(
        &mut server,
        "lv_anlegen",
        json!({"projekt": project, "nummer": "KB", "art": "kostenberechnung", "aus": "KS"}),
    );
    assert!(
        answer.contains("Kostenberechnung Heizung")
            && answer.contains("Kostenberechnung (Kosten-LV)"),
        "{answer}"
    );
    let check = ok(
        &mut server,
        "lv_pruefen",
        json!({"projekt": project, "lv": "KB"}),
    );
    assert!(check.contains("KG 420 ist die 2. Ebene"), "{check}");
    let error = call(
        &mut server,
        "lv_anlegen",
        json!({"projekt": project, "bezeichnung": "X", "art": "kostenberechnung", "aus": 1}),
    )
    .unwrap_err();
    assert!(error.contains("keine Kostenschätzung"), "{error}");

    let pdf = folder.0.join("KS.pdf");
    let answer = ok(
        &mut server,
        "pdf_drucken",
        json!({"projekt": project, "lv": "KS", "dokument": "kosten_lv", "ziel": pdf}),
    );
    assert!(answer.contains("KS.pdf"), "{answer}");
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));
    let error = call(
        &mut server,
        "pdf_drucken",
        json!({"projekt": project, "lv": 1, "dokument": "kosten_lv", "ziel": folder.0.join("x.pdf")}),
    )
    .unwrap_err();
    assert!(error.contains("keine Kostenermittlung"), "{error}");
}

/// GU-Fall: KG 420 as Kurztext positions and 480 as Datenpunkt-Abschnitt in
/// one Kostenberechnung – kosten_zeilen_schreiben with lv creates and fills
/// it, kosten_lesen/kosten_aendern with lv work on it, the sums and the PDF
/// of the Kosten-LV hold both parts.
#[test]
fn point_section_in_a_cost_lv() {
    let folder = TempFolder::new("gufall");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "GU");
    ok(
        &mut server,
        "lv_anlegen",
        json!({"projekt": project, "bezeichnung": "Kostenberechnung KG 400", "nummer": "KB", "art": "kostenberechnung", "kostengruppen": ["420", "480"]}),
    );
    ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "lv": "KB", "unter_titel": "420.421", "kurztext": "Wärmepumpe", "menge": 40, "einheit": "kW", "ep": 1500}),
    );
    let error = call(
        &mut server,
        "kosten_lesen",
        json!({"projekt": project, "lv": "KB"}),
    )
    .unwrap_err();
    assert!(error.contains("keinen Datenpunkt-Abschnitt"), "{error}");

    let answer = ok(
        &mut server,
        "kosten_zeilen_schreiben",
        json!({"projekt": project, "lv": "KB", "zeilen": [
            {"bauteil": "Haus A", "ort": "ASP01", "gewerk": "RLT", "anlage": "RLT-01", "dp": {"AI": 50, "BI": 50}}
        ]}),
    );
    // 100 DP × 400 € = 40.000 €, on 481–489 by the template split.
    assert!(
        answer.contains("Datenpunkt-Abschnitt 480") && answer.contains("angelegt"),
        "{answer}"
    );
    assert!(answer.contains("40.000,00 €"), "{answer}");
    assert!(
        answer.contains("KG 481 Automationseinrichtungen: 16.000,00 €"),
        "{answer}"
    );

    let answer = ok(
        &mut server,
        "kosten_aendern",
        json!({"projekt": project, "lv": "KB", "aufteilung": {"481": 50, "482": 10, "483": 10, "484": 25, "485": 5}}),
    );
    assert!(
        answer.contains("KG 481 Automationseinrichtungen: 20.000,00 €"),
        "{answer}"
    );
    let error = call(
        &mut server,
        "kosten_aendern",
        json!({"projekt": project, "lv": "KB", "titel": "X"}),
    )
    .unwrap_err();
    assert!(error.contains("die des Kosten-LV"), "{error}");

    let read = ok(
        &mut server,
        "kosten_lesen",
        json!({"projekt": project, "lv": "KB", "zeilen": true}),
    );
    assert!(read.contains("| RLT-01 |"), "{read}");
    let overview = ok(&mut server, "kosten_lesen", json!({"projekt": project}));
    assert!(
        overview.contains("Datenpunkt-Abschnitt 480") && overview.contains("100 DP"),
        "{overview}"
    );
    // Kurztext 60.000 € + Datenpunkte 40.000 €, each once.
    assert!(overview.contains("420: 60.000,00"), "{overview}");
    assert!(overview.contains("480: 40.000,00"), "{overview}");
    let lv = ok(
        &mut server,
        "lv_lesen",
        json!({"projekt": project, "lv": "KB"}),
    );
    assert!(lv.contains("100.000,00"), "{lv}");

    // LV prüfen: a Kurztext position with KG 484 next to the section.
    ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "lv": "KB", "unter_titel": "420.421", "kurztext": "Kabel GA", "menge": 1, "einheit": "psch", "ep": 1000, "kg": "484"}),
    );
    let check = ok(
        &mut server,
        "lv_pruefen",
        json!({"projekt": project, "lv": "KB"}),
    );
    assert!(
        check.contains("KG 484 ist auch im Datenpunkt-Abschnitt 480"),
        "{check}"
    );
    let empty = check
        .lines()
        .find(|line| line.contains("Titel ohne Positionen"))
        .unwrap_or_default();
    assert!(
        !empty.contains("480"),
        "der Abschnitt ist kein leerer Titel: {empty}"
    );

    let answer = ok(
        &mut server,
        "pdf_drucken",
        json!({"projekt": project, "lv": "KB", "dokument": "kosten_lv"}),
    );
    assert!(answer.contains("Seiten"), "{answer}");

    // Remove the data points: the title stays.
    let answer = ok(
        &mut server,
        "kosten_aendern",
        json!({"projekt": project, "lv": "KB", "loeschen": true}),
    );
    assert!(answer.contains("der Titel bleibt"), "{answer}");
}

/// The tool list goes to the model with every new session of an MCP client:
/// it must stay small. Measured as the `tools/list` answer in compact JSON,
/// counted in characters (≈ 3,3 per token for German text). Before the
/// slimming (19.11.2026) it was 201.142 characters, about 61.000 tokens;
/// after it 66.140 (48.051 of them schema without any text). 19.11.2026
/// (86 tools): 62.797 – keep at least 5.000 characters free by
/// shortening texts, not by raising the limit.
const TOOLS_LIST_MAX_CHARS: usize = 68_000;
/// No single tool may grow beyond this (characters, compact JSON) …
const TOOL_MAX_CHARS: usize = 3_000;
/// … except `projektangaben_setzen`: its schema alone (all Stammdaten, four
/// addresses, Kopf-/Fußzeile) has about 4.100 characters.
const PROJECT_DATA_MAX_CHARS: usize = 5_500;
/// The server hints (`instructions`) go along with every session too.
const INSTRUCTIONS_MAX_CHARS: usize = 6_000;

fn tool_list(server: &mut Server) -> Vec<Value> {
    let answer = server
        .handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}))
        .unwrap();
    answer["result"]["tools"].as_array().unwrap().clone()
}

#[test]
fn the_tool_list_stays_small() {
    let folder = TempFolder::new("katalog-groesse");
    let mut server = server(&folder.0);
    let tools = tool_list(&mut server);
    let size = |value: &Value| serde_json::to_string(value).unwrap().chars().count();
    let total = size(&Value::Array(tools.clone()));
    assert!(
        total <= TOOLS_LIST_MAX_CHARS,
        "tools/list hat {total} Zeichen, erlaubt sind {TOOLS_LIST_MAX_CHARS} – Beschreibungen kürzen, \
         Allgemeines gehört in die Server-Hinweise (INSTRUCTIONS in server.rs)"
    );
    let too_big: Vec<String> = tools
        .iter()
        .filter(|tool| {
            let limit = if tool["name"] == "projektangaben_setzen" {
                PROJECT_DATA_MAX_CHARS
            } else {
                TOOL_MAX_CHARS
            };
            size(tool) > limit
        })
        .map(|tool| format!("{} ({})", tool["name"], size(tool)))
        .collect();
    assert!(
        too_big.is_empty(),
        "Werkzeuge über {TOOL_MAX_CHARS} Zeichen: {}",
        too_big.join(", ")
    );
    let init = server
        .handle(&json!({"jsonrpc": "2.0", "id": 2, "method": "initialize",
            "params": {"protocolVersion": "2025-06-18"}}))
        .unwrap();
    let instructions = init["result"]["instructions"].as_str().unwrap();
    assert!(
        instructions.chars().count() <= INSTRUCTIONS_MAX_CHARS,
        "INSTRUCTIONS haben {} Zeichen, erlaubt sind {INSTRUCTIONS_MAX_CHARS}",
        instructions.chars().count()
    );
}

/// The schema of every tool without its texts: names, parameters, types,
/// `required`, `enum`, annotations. Shortening descriptions must not change it.
fn without_descriptions(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(key, _)| key.as_str() != "description")
                .map(|(key, value)| (key.clone(), without_descriptions(value)))
                .collect(),
        ),
        Value::Array(list) => Value::Array(list.iter().map(without_descriptions).collect()),
        other => other.clone(),
    }
}

/// Expected schemas: `tests/katalog_schema.jsonl`, one tool per line. A new
/// or changed parameter is wanted? Then write the file anew with
/// `PROAVA_KATALOG_SCHEMA_SCHREIBEN=1 cargo test -p proava-mcp --test tools
/// the_tool_schemas_are_the_known_ones` and commit it with the change.
#[test]
fn the_tool_schemas_are_the_known_ones() {
    let folder = TempFolder::new("katalog-schema");
    let mut server = server(&folder.0);
    let actual: Vec<Value> = tool_list(&mut server)
        .iter()
        .map(without_descriptions)
        .collect();
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/katalog_schema.jsonl");
    if std::env::var_os("PROAVA_KATALOG_SCHEMA_SCHREIBEN").is_some() {
        let lines: Vec<String> = actual
            .iter()
            .map(|tool| serde_json::to_string(tool).unwrap())
            .collect();
        std::fs::write(&file, lines.join("\n") + "\n").unwrap();
        return;
    }
    let expected: Vec<Value> = std::fs::read_to_string(&file)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let names = |list: &[Value]| -> Vec<String> {
        list.iter()
            .map(|tool| tool["name"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(
        names(&actual),
        names(&expected),
        "Werkzeuge und Reihenfolge"
    );
    let changed: Vec<String> = actual
        .iter()
        .zip(&expected)
        .filter(|(actual, expected)| actual != expected)
        .map(|(actual, _)| actual["name"].as_str().unwrap().to_string())
        .collect();
    assert!(
        changed.is_empty(),
        "Schema geändert bei: {} – nur Beschreibungen dürfen sich ändern. Gewollt? Dann \
         PROAVA_KATALOG_SCHEMA_SCHREIBEN=1 (siehe Kommentar am Test).",
        changed.join(", ")
    );
}

/// Aufteilungszeilen im Kosten-LV: positionen_einfuegen mit aufteilung,
/// lv_lesen/position_lesen zeigen Rechengrundlage und Anteil, lv_pruefen die
/// Summe ≠ 100 %, ein Ausschreibungs-LV lehnt ab.
#[test]
fn aufteilungszeilen_in_a_cost_lv() {
    let folder = TempFolder::new("aufteilung");
    let mut server = server(&folder.0);
    let project = new_project(&mut server, &folder.0, "Aufteilung");
    ok(
        &mut server,
        "lv_anlegen",
        json!({"projekt": project, "bezeichnung": "Kostenberechnung GA", "nummer": "KB", "art": "kostenberechnung", "kostengruppen": ["480"]}),
    );
    // 100 AI × 400 € + 50 BI × 400 € = 60.000 €, aufgeteilt 40/60.
    let answer = ok(
        &mut server,
        "positionen_einfuegen",
        json!({"projekt": project, "lv": "KB", "unter_titel": "480", "am_anfang": true, "zeilen": [
            {"kurztext": "Analoge Eingabe (AI)", "menge": 100, "einheit": "St", "ep": 400},
            {"kurztext": "Binäre Eingabe (BI)", "menge": 50, "einheit": "St", "ep": 400},
            {"kurztext": "KG 481", "aufteilung": 40, "kg": "481"},
            {"kurztext": "KG 484", "aufteilung": {"prozent": 60}, "kg": "484"}
        ]}),
    );
    assert!(answer.contains("4 "), "{answer}");
    let lv = ok(
        &mut server,
        "lv_lesen",
        json!({"projekt": project, "lv": "KB"}),
    );
    assert!(lv.contains("Rechengrundlage"), "{lv}");
    assert!(
        lv.contains("(40.000,00)"),
        "GP der Rechengrundlage in Klammern: {lv}"
    );
    assert!(lv.contains("Pos Aufteilung"), "{lv}");
    assert!(lv.contains("36.000,00"), "60 % von 60.000 €: {lv}");
    let costs = ok(&mut server, "kosten_lesen", json!({"projekt": project}));
    assert!(costs.contains("480: 60.000,00"), "einmal gezählt: {costs}");
    let oz = lv
        .lines()
        .find(|line| line.contains("| KG 481 |"))
        .and_then(|line| line.split(" | ").next())
        .unwrap()
        .trim()
        .to_string();
    let row = ok(
        &mut server,
        "position_lesen",
        json!({"projekt": project, "lv": "KB", "oz": oz}),
    );
    assert!(row.contains("Aufteilungszeile"), "{row}");
    assert!(
        row.contains("Aufteilung: 40 %") && row.contains("Bezugssumme 60.000,00"),
        "{row}"
    );

    // 481 to 30 %: the block makes 90 %.
    ok(
        &mut server,
        "zeile_aendern",
        json!({"projekt": project, "lv": "KB", "oz": oz, "ep": 30}),
    );
    let check = ok(
        &mut server,
        "lv_pruefen",
        json!({"projekt": project, "lv": "KB"}),
    );
    assert!(check.contains("90 % statt 100 %"), "{check}");

    // Not in a tender LV.
    let error = call(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "lv": 1, "kurztext": "KG 481", "aufteilung": 40}),
    )
    .unwrap_err();
    assert!(error.contains("nur in einer Kostenermittlung"), "{error}");
}
