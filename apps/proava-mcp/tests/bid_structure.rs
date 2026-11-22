//! Abweichungen im Angebot über den MCP-Server (Anwender 20.11.):
//! Protokoll in angebot_einlesen und angebote_lesen, Zuordnung über die
//! Struktur (zuordnung: text) und von Hand (zuordnungen) – synthetische
//! Dateien in temporären Ordnern.

use std::path::{Path, PathBuf};

use proava_application::ProjectRepository;
use proava_domain::{Decimal, Lv, LvGroup, OzMask, Position, Project, ProjectName};
use proava_exchange::{lv_to_gaeb, GaebPdfExchange};
use proava_gaeb::{BoqNode, Item, Phase};
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

fn ok(server: &mut Server, tool: &str, arguments: Value) -> String {
    match server.call_tool(tool, arguments.as_object().unwrap()) {
        Ok(text) => text,
        Err(error) => panic!("{tool} {arguments}: {error}"),
    }
}

fn dec(text: &str) -> Decimal {
    text.parse().unwrap()
}

/// LV 01 with four positions; the bid file numbers 0020–0040 anew and has
/// one position more (01.0099, first in the title).
fn files(folder: &Path) -> (String, String) {
    let mut lv = Lv::new("Gebäudeautomation", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "Automation"));
    for (oz, text, qty, ep) in [
        ("01.0010", "DDC-Station", "1", "1000"),
        ("01.0020", "E/A-Modul", "4", "300"),
        ("01.0030", "Netzteil", "2", "80"),
        ("01.0040", "Schaltschrank", "1", "5000"),
    ] {
        lv.push_position(Position::new(oz, text, dec(qty), "St", Some(dec(ep))));
    }
    let project = Project::new(ProjectName::parse("Musterhalle").unwrap(), "2026-11-20", lv);
    let project_file = folder.join("Musterhalle.proava");
    XmlProjectFile::new()
        .save_project(&project_file, &project)
        .unwrap();
    let (mut doc, _) = lv_to_gaeb(&project, Phase::Dp82);
    doc.phase = Phase::Dp84;
    doc.award.bidder = Some(proava_gaeb::Address {
        name1: "Muster Regeltechnik GmbH".into(),
        ..proava_gaeb::Address::default()
    });
    let BoqNode::Category(title) = doc
        .award
        .boq
        .body
        .iter_mut()
        .find(|node| matches!(node, BoqNode::Category(_)))
        .unwrap()
    else {
        unreachable!()
    };
    for node in &mut title.body {
        if let BoqNode::Item(item) = node {
            item.rno_part = match item.rno_part.as_str() {
                "0020" => "0021".into(),
                "0030" => "0022".into(),
                "0040" => "0023".into(),
                other => other.into(),
            };
        }
    }
    title.body.insert(
        0,
        BoqNode::Item(Item {
            rno_part: "0099".into(),
            short_text: "Feuchtefühler".into(),
            qty: Some(dec("2")),
            unit: "St".into(),
            unit_price: Some(dec("150")),
            total: Some(dec("300")),
            ..Item::default()
        }),
    );
    let bid_file = folder.join("Muster.X84");
    std::fs::write(
        &bid_file,
        proava_gaeb::xml::write(&doc, Phase::Dp84).unwrap(),
    )
    .unwrap();
    (
        project_file.to_string_lossy().into_owned(),
        bid_file.to_string_lossy().into_owned(),
    )
}

#[test]
fn the_protocol_proposals_and_both_ways_of_assigning() {
    let folder = TempFolder::new("angebot-struktur");
    let mut server = server(&folder.0);
    let (project, bid) = files(&folder.0);

    // By OZ (default): everything in the protocol, sure proposals named.
    let answer = ok(
        &mut server,
        "angebot_einlesen",
        json!({"projekt": project, "datei": bid}),
    );
    assert!(
        answer.contains("Strukturabweichungen: 4 Positionen nicht im LV, 3 LV-Positionen fehlen"),
        "{answer}"
    );
    assert!(
        answer.contains("3 sichere Zuordnungsvorschläge nicht übernommen"),
        "{answer}"
    );
    let read = ok(&mut server, "angebote_lesen", json!({"projekt": project}));
    assert!(
        read.contains("· 3 Zuordnungsvorschläge (mit angebot)"),
        "{read}"
    );
    let details = ok(
        &mut server,
        "angebote_lesen",
        json!({"projekt": project, "angebot": 1}),
    );
    assert!(
        details.contains("Vorschlag (sicher, 95 %): Angebot 01.0021 → LV 01.0020 E/A-Modul"),
        "{details}"
    );
    assert!(
        // X84 carries no texts.
        details.contains("nicht im LV: Angebot 01.0099 – ohne Text, EP 150, GP 300,00 €"),
        "{details}"
    );

    // By structure: the three are taken, the extra one stays.
    let answer = ok(
        &mut server,
        "angebot_einlesen",
        json!({"projekt": project, "datei": bid, "zuordnung": "text"}),
    );
    assert!(
        answer
            .contains("Strukturabweichungen: 3 umnummeriert (zugeordnet), 1 Position nicht im LV"),
        "{answer}"
    );
    assert!(
        answer.contains("Angebotssumme netto: 7.360,00 €"),
        "{answer}"
    );

    // By hand: the extra one onto 01.0040, the rest by OZ.
    let answer = ok(
        &mut server,
        "angebot_einlesen",
        json!({"projekt": project, "datei": bid, "zuordnungen": {"01.0099": "01.0040"}}),
    );
    assert!(
        answer.contains("umnummeriert: Angebot 01.0099 → LV 01.0040 – von Hand zugeordnet"),
        "{answer}"
    );

    // Wrong values are named.
    let error = server
        .call_tool(
            "angebot_einlesen",
            json!({"projekt": project, "datei": bid, "zuordnung": "egal"})
                .as_object()
                .unwrap(),
        )
        .unwrap_err();
    assert!(error.contains("bitte oz oder text"), "{error}");
    let answer = ok(
        &mut server,
        "angebot_einlesen",
        json!({"projekt": project, "datei": bid, "zuordnungen": {"77.0010": "01.0040"}}),
    );
    assert!(
        answer.contains("Zuordnung von Hand nicht möglich (1): 77.0010 (nicht in der Datei)"),
        "{answer}"
    );
}
