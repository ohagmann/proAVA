//! Positionen kommen oder gehen in einem LV mit Angeboten oder Auftrag:
//! die Schreibwerkzeuge speichern erst mit `bestaetigen` (Anwender 19.11.);
//! `lv_pruefen` mit `alle` prüft alle LVs der Ausschreibung, je LV gegliedert
//! – Projekte nur in temporären Ordnern.

use std::path::{Path, PathBuf};

use proava_application::ProjectRepository;
use proava_domain::{Address, Bid, Decimal, Lv, LvGroup, OzMask, Position, Project, ProjectName};
use proava_exchange::GaebPdfExchange;
use proava_infrastructure::XmlProjectFile;
use proava_mcp::Server;
use serde_json::{json, Value};

struct TempFolder(PathBuf, #[allow(dead_code)] tempfile::TempDir);

impl TempFolder {
    fn new(name: &str) -> Self {
        let dir = tempfile::Builder::new()
            .prefix(&format!("proava-mcp-{name}-"))
            .tempdir()
            .unwrap();
        Self(proava_mcp::real_path(dir.path()).unwrap(), dir)
    }
}

fn server(root: &Path) -> Server {
    Server::new(Box::new(XmlProjectFile::new()), Box::new(GaebPdfExchange))
        .with_allowed_roots(vec![root.to_path_buf()])
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

fn refused(server: &mut Server, tool: &str, arguments: Value) -> String {
    match call(server, tool, arguments.clone()) {
        Ok(text) => panic!("{tool} {arguments} sollte fragen: {text}"),
        Err(error) => error,
    }
}

fn lv(title: &str) -> Lv {
    let mut lv = Lv::new(title, OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "Titel"));
    lv.push_position(Position::new(
        "01.0010",
        "DDC-Station",
        Decimal::ONE,
        "St",
        Some(Decimal::from(2800)),
    ));
    lv.push_position(Position::new(
        "01.0020",
        "E/A-Modul",
        Decimal::TWO,
        "St",
        Some(Decimal::from(400)),
    ));
    lv
}

/// LV 1 with a bid that prices both positions; with `awarded` also the
/// Auftrag (festgeschrieben). LV 2 without bids.
fn project_file(folder: &Path, awarded: bool) -> String {
    let mut project = Project::new(
        ProjectName::parse("Angebote").unwrap(),
        "2026-11-19",
        lv("Gebäudeautomation"),
    );
    project.lv_mut().set_number("1");
    let mut bid = Bid::new(Address::named("Nord"), "Nord.X84");
    let ids: Vec<_> = project.lv().positions().map(|p| p.id()).collect();
    for id in ids {
        bid.set_unit_price(id, Some(Decimal::from(300)));
    }
    let bid = project.add_bid(bid);
    if awarded {
        project.award_bid(bid, "19.11.2026").unwrap();
        project.take_over_contract().unwrap();
    }
    let index = project.add_lv(lv("Elektro"));
    project.lv_at_mut(index).unwrap().set_number("2");
    let path = folder.join("Angebote.proava");
    XmlProjectFile::new().save_project(&path, &project).unwrap();
    path.to_string_lossy().to_string()
}

fn short_texts(path: &str, lv: usize) -> Vec<String> {
    let project = XmlProjectFile::new().load_project(Path::new(path)).unwrap();
    project
        .lv_at(lv)
        .unwrap()
        .positions()
        .map(|p| p.short_text().to_string())
        .collect()
}

#[test]
fn new_or_removed_positions_in_an_lv_with_bids_need_bestaetigen() {
    let folder = TempFolder::new("angebote-rueckfrage");
    let mut server = server(&folder.0);
    let project = project_file(&folder.0, false);
    let saved = std::fs::read(&project).unwrap();
    let insert = json!({"projekt": project, "lv": "1", "unter_titel": "01", "kurztext": "Raumfühler", "menge": 4, "einheit": "St"});

    // Einfügen: Rückfrage, nichts gespeichert.
    let question = refused(&mut server, "position_einfuegen", insert.clone());
    assert!(
        question.contains(
            "Rückfrage – nichts gespeichert: LV 1 – Gebäudeautomation hat bereits ein Angebot – die neue Position fehlt dort."
        ),
        "{question}"
    );
    assert!(
        question.contains("mit bestaetigen: true wiederholen"),
        "{question}"
    );
    assert!(
        !question.contains("Nachtrag"),
        "ohne Auftrag kein Nachtrag: {question}"
    );
    assert_eq!(std::fs::read(&project).unwrap(), saved, "Datei unverändert");

    // Mit bestaetigen gespeichert.
    let mut confirmed = insert.clone();
    confirmed["bestaetigen"] = json!(true);
    ok(&mut server, "position_einfuegen", confirmed);
    assert!(short_texts(&project, 0).contains(&"Raumfühler".to_string()));

    // Die neue Position steht in keinem Angebot: Löschen fragt nicht.
    ok(
        &mut server,
        "zeile_loeschen",
        json!({"projekt": project, "lv": "1", "oz": "01.0030"}),
    );
    assert!(!short_texts(&project, 0).contains(&"Raumfühler".to_string()));

    // Eine Position, die das Angebot enthält: Rückfrage.
    let question = refused(
        &mut server,
        "zeile_loeschen",
        json!({"projekt": project, "lv": "1", "oz": "01.0010"}),
    );
    assert!(
        question.contains(
            "LV 1 – Gebäudeautomation hat bereits ein Angebot – die gelöschte Position ist dort enthalten."
        ),
        "{question}"
    );
    ok(
        &mut server,
        "zeile_loeschen",
        json!({"projekt": project, "lv": "1", "oz": "01.0010", "bestaetigen": true}),
    );
    assert_eq!(short_texts(&project, 0), vec!["E/A-Modul".to_string()]);

    // LV 2 hat keine Angebote: keine Rückfrage.
    ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "lv": "2", "unter_titel": "01", "kurztext": "Kabel", "menge": 10, "einheit": "m"}),
    );
    assert_eq!(short_texts(&project, 1).len(), 3);

    // Die Werkzeuge nehmen bestaetigen.
    let answer = server
        .handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}))
        .unwrap();
    let list = answer["result"]["tools"].as_array().unwrap().clone();
    for name in [
        "position_einfuegen",
        "positionen_einfuegen",
        "zeile_loeschen",
        "positionen_uebernehmen",
    ] {
        let tool = list.iter().find(|tool| tool["name"] == name).unwrap();
        assert!(
            tool["inputSchema"]["properties"]["bestaetigen"].is_object(),
            "{name}"
        );
    }
    // Wann bestaetigen nötig ist, sagen die Server-Hinweise (einmal für alle).
    let init = server
        .handle(&json!({"jsonrpc": "2.0", "id": 2, "method": "initialize",
            "params": {"protocolVersion": "2025-06-18"}}))
        .unwrap();
    let instructions = init["result"]["instructions"].as_str().unwrap();
    assert!(
        instructions.contains("Kommen in einem LV mit Angeboten oder Auftrag"),
        "{instructions}"
    );
}

#[test]
fn with_an_order_the_question_points_to_the_change_order_tools() {
    let folder = TempFolder::new("auftrag-rueckfrage");
    let mut server = server(&folder.0);
    let project = project_file(&folder.0, true);
    let question = refused(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "lv": "1", "unter_titel": "01", "kurztext": "Raumfühler", "menge": 4, "einheit": "St"}),
    );
    assert!(
        question.contains(
            "LV 1 – Gebäudeautomation hat bereits ein Angebot und einen Auftrag – die neue Position fehlt dort. \
             Zusätzliche Leistungen gehören in einen Nachtrag."
        ),
        "{question}"
    );
    assert!(
        question.contains(
            "Der Weg über den Nachtrag fragt nicht: nachtrag_anlegen, dann nachtragsposition_setzen mit neu: true."
        ),
        "{question}"
    );

    // Der Weg über den Nachtrag fragt nicht.
    ok(
        &mut server,
        "nachtrag_anlegen",
        json!({"projekt": project, "lv": "1", "bezeichnung": "Raumfühler zusätzlich"}),
    );
    ok(
        &mut server,
        "nachtragsposition_setzen",
        json!({"projekt": project, "lv": "1", "nachtrag": 1, "neu": true, "kurztext": "Raumfühler", "menge": 4, "einheit": "St"}),
    );
    assert!(short_texts(&project, 0).contains(&"Raumfühler".to_string()));
}

#[test]
fn checking_all_lvs_goes_over_the_tender_and_names_each_lv() {
    let folder = TempFolder::new("alle-lvs-pruefen");
    let mut server = server(&folder.0);
    let project = project_file(&folder.0, false);
    // A position without unit in LV 2: an error there only.
    ok(
        &mut server,
        "position_einfuegen",
        json!({"projekt": project, "lv": "2", "unter_titel": "01", "kurztext": "Ohne Einheit", "menge": 1}),
    );
    ok(
        &mut server,
        "lv_anlegen",
        json!({"projekt": project, "bezeichnung": "Kosten GA", "nummer": "9", "art": "kostenschaetzung"}),
    );

    let one = ok(
        &mut server,
        "lv_pruefen",
        json!({"projekt": project, "lv": "1"}),
    );
    assert!(!one.contains("Alle LVs geprüft"), "ohne alle: ein LV");

    let all = ok(
        &mut server,
        "lv_pruefen",
        json!({"projekt": project, "lv": "1", "alle": true}),
    );
    assert!(all.starts_with("Alle LVs geprüft (2)"), "{all}");
    assert!(
        all.contains("- LV 1 – Gebäudeautomation: 0 Fehler"),
        "{all}"
    );
    assert!(all.contains("- LV 2 – Elektro: 1 Fehler"), "{all}");
    let first = all.find("== LV 1 – Gebäudeautomation ==").expect("LV 1");
    let second = all.find("== LV 2 – Elektro ==").expect("LV 2");
    assert!(first < second, "in der Reihenfolge der LVs");
    assert!(
        all[second..].contains("Position ohne Einheit"),
        "der Fehler steht bei LV 2: {all}"
    );
    assert!(!all[first..second].contains("Position ohne Einheit"));
    assert!(
        !all.contains("Kosten GA"),
        "Kosten-LV nur, wenn es gewählt ist"
    );

    // Ist das Kosten-LV gewählt, kommt es mit.
    let with_costs = ok(
        &mut server,
        "lv_pruefen",
        json!({"projekt": project, "lv": "9", "alle": true}),
    );
    assert!(
        with_costs.starts_with("Alle LVs geprüft (3)"),
        "{with_costs}"
    );
    assert!(
        with_costs.contains("== LV 9 – Kosten GA (Kostenschätzung) =="),
        "{with_costs}"
    );
}
