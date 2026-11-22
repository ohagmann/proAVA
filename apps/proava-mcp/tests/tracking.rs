//! Kostenverfolgung über den MCP-Server: lesen, Budget setzen und
//! übernehmen, Erläuterung, Angebotswahl, Kostenermittlungen wählen,
//! Kostenstand festhalten und vergleichen, PDF – Projekte nur in temporären
//! Ordnern.

use std::path::{Path, PathBuf};

use proava_application::ProjectRepository;
use proava_domain::{
    Address, Bid, CostStage, Decimal, Lv, LvGroup, LvPurpose, OzMask, Position, Project,
    ProjectName,
};
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

fn lv(title: &str, number: &str, items: &[(&str, &str, i64, &str, i64)]) -> Lv {
    let mut lv = Lv::new(title, OzMask::parse("11.22.PPPP").unwrap());
    lv.set_number(number);
    for (code, text, quantity, unit, euro) in items {
        let mut group = LvGroup::new(0, "", *text);
        group.set_cost_group(Some((*code).to_string()));
        lv.push_group(group);
        lv.push_position(Position::new(
            "",
            *text,
            Decimal::from(*quantity),
            *unit,
            Some(Decimal::from(*euro)),
        ));
    }
    lv.renumber();
    lv
}

fn bid(lv: &Lv, name: &str, prices: [i64; 2]) -> Bid {
    let mut bid = Bid::new(Address::named(name), format!("{name}.X84"));
    for (position, euro) in lv.positions().zip(prices) {
        bid.set_unit_price(position.id(), Some(Decimal::from(euro)));
    }
    bid
}

/// Heizung (KV 55.000 €) with two bids – Nord 57.500 €, Süd 55.000 € –
/// and a Kostenberechnung as Kosten-LV (53.500 €). Returns the file and the
/// id of the dearer bid.
fn project(folder: &Path) -> (String, u64) {
    let heating = lv(
        "Heizung",
        "420",
        &[
            ("421", "Wärmepumpe", 1, "St", 50_000),
            ("422", "Rohrleitung", 100, "m", 50),
        ],
    );
    let north = bid(&heating, "Wärme Nord", [52_000, 55]);
    let south = bid(&heating, "Wärme Süd", [49_000, 60]);
    let mut project = Project::new(
        ProjectName::parse("Rathaus 3").unwrap(),
        "2026-11-18T12:00:00",
        heating,
    );
    let dear = project.add_bid_to(0, north);
    project.add_bid_to(0, south);
    let mut calculation = lv(
        "Kostenberechnung Technik",
        "KB",
        &[
            ("421", "Wärmeerzeugung", 1, "psch", 48_000),
            ("422", "Wärmeverteilung", 1, "psch", 5_500),
        ],
    );
    calculation.set_purpose(LvPurpose::Cost(CostStage::Calculation));
    project.add_lv(calculation);
    let file = folder.join("Rathaus 3.proava");
    XmlProjectFile::new().save_project(&file, &project).unwrap();
    (file.to_string_lossy().to_string(), dear.0)
}

#[test]
fn the_chain_is_read_budgeted_explained_recorded_and_printed() {
    let folder = TempFolder::new("kostenverfolgung");
    let mut server = server(&folder.0);
    let (project, dear) = project(&folder.0);

    let read = ok(
        &mut server,
        "kostenverfolgung_lesen",
        json!({"projekt": project}),
    );
    assert!(
        read.starts_with("Kostenverfolgung (DIN 276, 4.4–4.6)"),
        "{read}"
    );
    assert!(read.contains("Keine Kostenvorgabe"), "{read}");
    assert!(
        read.contains(
            "|   420 Wärmeversorgungsanlagen | – | 53.500,00 | 55.000,00 | 55.000,00 | 55.000,00 |"
        ),
        "Kostenvorgabe, KB, KV, Angebot (Mindestbieter), Prognose: {read}"
    );
    assert!(read.contains("Wärme Süd (Mindestbieter)"), "{read}");
    assert!(
        read.contains("| Kostenberechnung | Kostenberechnung Technik |"),
        "{read}"
    );

    // Budget aus der Kostenberechnung, dann 420 knapper als Zielgröße.
    let answer = ok(
        &mut server,
        "kostenvorgabe_setzen",
        json!({"projekt": project, "aus": "kostenberechnung", "stand": "01.06.2026"}),
    );
    assert!(
        answer.contains("Kostenvorgabe aus Kostenberechnung"),
        "{answer}"
    );
    assert!(
        answer.contains("Kostenvorgabe gesamt 53.500,00 €"),
        "{answer}"
    );
    assert!(
        answer.contains("Sicherung vor der ersten Änderung"),
        "{answer}"
    );
    let answer = ok(
        &mut server,
        "kostenvorgabe_setzen",
        json!({"projekt": project, "budget": {"421": null, "422": null, "420": "50.000"},
               "art": "zielgroesse", "grundlage": "Kostenrahmen", "schwelle": 5}),
    );
    assert!(answer.contains("KG 420 50.000,00 €"), "{answer}");
    assert!(answer.contains("Zielgröße"), "{answer}");

    let read = ok(
        &mut server,
        "kostenverfolgung_lesen",
        json!({"projekt": project, "ebene": 3, "abweichung": "budget"}),
    );
    assert!(
        read.contains("55.000,00 (+5.000,00 / +10,0 %) [über Kostenvorgabe]"),
        "Prognose 420 über der Kostenvorgabe: {read}"
    );
    assert!(
        read.contains("Kostenvorgabe 50.000,00 € (Zielgröße, Stand 01.06.2026, Kostenrahmen)"),
        "{read}"
    );

    // Erläuterung, Angebot Nord mit Begründung, Kostenstand.
    let answer = ok(
        &mut server,
        "kostenverfolgung_aendern",
        json!({"projekt": project, "lv": "420",
               "erlaeuterungen": {"420": "Wärmepumpe über der Kostenberechnung – Maßnahme: Variante prüfen."},
               "angebot_wahl": {"angebot": dear, "begruendung": "Nebenangebot des Mindestbieters unvollständig"},
               "kostenstand_festhalten": {"datum": "01.12.2026", "anlass": "vor der Vergabe"}}),
    );
    assert!(answer.contains("Angebot Wärme Nord gewählt"), "{answer}");
    assert!(
        answer.contains("Kostenstand 01.12.2026 – vor der Vergabe festgehalten"),
        "{answer}"
    );
    let read = ok(
        &mut server,
        "kostenverfolgung_lesen",
        json!({"projekt": project, "ansicht": "vergabeeinheiten", "vergleich": 1}),
    );
    assert!(read.contains("Wärme Nord (gewählt"), "{read}");
    // With a recorded state every stage shows its change since then (none yet).
    assert!(read.contains("Abweichung zum Stand"), "{read}");
    assert!(read.contains("57.500,00 (0,00 / 0,0 %)"), "{read}");
    assert!(read.contains("Festgehaltene Kostenstände"), "{read}");
    let groups = ok(
        &mut server,
        "kostenverfolgung_lesen",
        json!({"projekt": project}),
    );
    assert!(groups.contains("Variante prüfen."), "{groups}");

    // The Kostenberechnung no longer counts.
    let key = groups
        .lines()
        .find(|line| line.starts_with("| Kostenberechnung | Kostenberechnung Technik |"))
        .and_then(|line| line.trim_end_matches(" |").rsplit(" | ").next())
        .unwrap()
        .to_string();
    let answer = ok(
        &mut server,
        "kostenverfolgung_aendern",
        json!({"projekt": project, "kostenermittlungen": {key.clone(): false}}),
    );
    assert!(
        answer.contains("Kostenberechnung Technik zählt nicht"),
        "{answer}"
    );
    let read = ok(
        &mut server,
        "kostenverfolgung_lesen",
        json!({"projekt": project}),
    );
    assert!(read.contains(&format!("| nein | ja | {key} |")), "{read}");

    // PDF wie im Reiter eingestellt.
    let target = folder.0.join("Kostenverfolgung.pdf");
    let answer = ok(
        &mut server,
        "pdf_drucken",
        json!({"projekt": project, "dokument": "kostenverfolgung", "ebene": 3, "abweichung": "vorstufe",
               "vergleich": "01.12.2026", "deckblatt": true, "ziel": target}),
    );
    assert!(answer.contains("Seiten"), "{answer}");
    let bytes = std::fs::read(&target).unwrap();
    assert!(bytes.starts_with(b"%PDF"));

    // Mistakes are named.
    for (arguments, expected) in [
        (json!({"projekt": project, "ebene": 4}), "ebene: 4"),
        (
            json!({"projekt": project, "vergleich": 9}),
            "Einen Kostenstand „9“ gibt es nicht",
        ),
        (
            json!({"projekt": project, "ansicht": "lv-weise"}),
            "ansicht: „lv-weise“",
        ),
        (
            json!({"projekt": project, "abweichung": "stand"}),
            "abweichung stand braucht vergleich",
        ),
    ] {
        let error = call(&mut server, "kostenverfolgung_lesen", arguments).unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
    let error = call(
        &mut server,
        "kostenvorgabe_setzen",
        json!({"projekt": project, "budget": {"420": -5}}),
    )
    .unwrap_err();
    assert!(error.contains("keine negative Kostenvorgabe"), "{error}");
    let error = call(
        &mut server,
        "kostenverfolgung_aendern",
        json!({"projekt": project}),
    )
    .unwrap_err();
    assert!(error.contains("Nichts zu ändern"), "{error}");
}
