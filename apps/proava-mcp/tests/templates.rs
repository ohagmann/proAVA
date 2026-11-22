//! `projekt_anlegen` mit Vorlage: die mitgelieferte GA-Vorlage, eine eigene
//! Vorlage aus dem Vorlagenordner und Fehlerfälle (nur temporäre Ordner).

use std::path::{Path, PathBuf};

use proava_application::{Preferences, ProjectRepository};
use proava_domain::{Decimal, LvRow, Position, ProjectName};
use proava_exchange::GaebPdfExchange;
use proava_infrastructure::XmlProjectFile;
use proava_mcp::{Server, TemplateDefaults};
use serde_json::{json, Value};

/// A folder only this test uses: a unique name, created exclusively (two
/// tests – also of other test runs at the same time – never share one); it is
/// removed with this value, nothing else.
struct TempFolder(PathBuf, #[allow(dead_code)] tempfile::TempDir);

impl TempFolder {
    fn new(name: &str) -> Self {
        let dir = tempfile::Builder::new()
            .prefix(&format!("proava-mcp-vorlage-{name}-"))
            .tempdir()
            .unwrap();
        Self(proava_mcp::real_path(dir.path()).unwrap(), dir)
    }
}

fn server(root: &Path, templates: &Path) -> Server {
    let preferences = Preferences {
        new_lv_mask: "11.222.33.PPPP".into(),
        ..Preferences::default()
    };
    Server::new(Box::new(XmlProjectFile::new()), Box::new(GaebPdfExchange))
        .with_allowed_roots(vec![root.to_path_buf()])
        .with_template_defaults(TemplateDefaults {
            folder: Some(templates.to_path_buf()),
            preferences,
        })
}

fn call(server: &mut Server, arguments: Value) -> Result<String, String> {
    server.call_tool("projekt_anlegen", arguments.as_object().unwrap())
}

fn titles(file: &Path) -> Vec<String> {
    let project = XmlProjectFile::new().load_project(file).unwrap();
    project
        .lv()
        .rows()
        .iter()
        .filter_map(LvRow::as_group)
        .map(|group| format!("{} {}", group.ordinal(), group.short_text()))
        .collect()
}

/// The GA template brings its own mask 11.222.PPPPI, whatever the settings
/// say (here 11.222.33.PPPP); `oz_maske` replaces it.
#[test]
fn the_ga_template_brings_its_own_mask() {
    let folder = TempFolder::new("ga");
    let templates = folder.0.join("Vorlagen");
    let mut server = server(&folder.0, &templates);
    let answer = call(
        &mut server,
        json!({"ordner": folder.0, "name": "Schule", "vorlage": "GA"}),
    )
    .unwrap();
    assert!(answer.contains("Vorlage: GA nach DIN 276"), "{answer}");
    assert!(answer.contains("OZ-Maske 11.222.PPPPI ·"), "{answer}");
    let file = folder.0.join("Schule/Schule.proava");
    let rows = titles(&file);
    assert_eq!(
        rows,
        [
            "01 Gebäudeautomation",
            "01.481 Automationseinrichtungen",
            "01.482 Schaltschränke",
            "01.483 Management- und Bedieneinrichtungen",
            "01.484 Kabel und Leitungen",
            "01.485 Netzwerke",
            "01.489 Sonstiges",
        ]
    );
    call(
        &mut server,
        json!({"ordner": folder.0, "name": "Tief", "vorlage": "GA", "oz_maske": "11.222.33.PPPP"}),
    )
    .unwrap();
    assert_eq!(
        titles(&folder.0.join("Tief/Tief.proava"))[1],
        "01.481 Automationseinrichtungen"
    );

    // Own mask in the call wins for the built-in template.
    call(
        &mut server,
        json!({"ordner": folder.0, "name": "Flach", "vorlage": "ga", "oz_maske": "11.22.PPPPI"}),
    )
    .unwrap();
    assert_eq!(
        titles(&folder.0.join("Flach/Flach.proava"))[0],
        "01 Automationseinrichtungen"
    );
}

#[test]
fn an_own_template_comes_with_or_without_positions() {
    let folder = TempFolder::new("eigen");
    let templates = folder.0.join("Vorlagen");
    std::fs::create_dir_all(&templates).unwrap();
    // An own template: the GA one with one position, stored as a file.
    let repository = XmlProjectFile::new();
    let ga = proava_domain::template::ga_template(
        proava_domain::OzMask::parse("11.22.PPPPI").unwrap(),
        10,
        Decimal::from(19),
        "2026-11-17T20:00:00",
    );
    let mut own = proava_domain::template::project_from_template(
        &ga,
        ProjectName::parse("GA Schulen").unwrap(),
        "2026-11-17T20:00:00",
        true,
    );
    own.lv_mut().push_position(Position::new(
        "01.01.0010",
        "DDC-Station",
        Decimal::ONE,
        "St",
        None,
    ));
    let file = repository.create_project(&templates, own.name()).unwrap();
    repository.save_project(&file, &own).unwrap();

    let mut server = server(&folder.0, &templates);
    let answer = call(
        &mut server,
        json!({"ordner": folder.0, "name": "A", "vorlage": "ga schulen"}),
    )
    .unwrap();
    assert!(answer.contains("1 Position"), "{answer}");
    let without = call(
        &mut server,
        json!({"ordner": folder.0, "name": "B", "vorlage": "GA Schulen", "positionen": false}),
    )
    .unwrap();
    assert!(without.contains("keine Positionen"), "{without}");

    let unknown = call(
        &mut server,
        json!({"ordner": folder.0, "name": "C", "vorlage": "Krankenhaus"}),
    )
    .unwrap_err();
    assert!(
        unknown.contains("Vorhanden: leer, GA, GA Schulen"),
        "{unknown}"
    );
    let mask = call(
        &mut server,
        json!({"ordner": folder.0, "name": "D", "vorlage": "GA Schulen", "oz_maske": "11.PPPP"}),
    )
    .unwrap_err();
    assert!(mask.contains("bringt ihre Gliederung mit"), "{mask}");
    assert!(
        !folder.0.join("C").exists() && !folder.0.join("D").exists(),
        "nichts angelegt"
    );
}

/// An empty project takes the mask of the settings: a stored one (here
/// 11.222.33.PPPP) or, without settings, the Vorgabe 11.222.PPPPI.
#[test]
fn an_empty_project_takes_the_mask_of_the_settings() {
    let folder = TempFolder::new("leer");
    let templates = folder.0.join("Vorlagen");
    let mut server = server(&folder.0, &templates);
    let answer = call(&mut server, json!({"ordner": folder.0, "name": "Leer"})).unwrap();
    assert!(answer.contains("OZ-Maske 11.222.33.PPPP ·"), "{answer}");

    let mut plain = Server::new(Box::new(XmlProjectFile::new()), Box::new(GaebPdfExchange))
        .with_allowed_roots(vec![folder.0.clone()]);
    let answer = call(&mut plain, json!({"ordner": folder.0, "name": "Vorgabe"})).unwrap();
    assert!(answer.contains("OZ-Maske 11.222.PPPPI ·"), "{answer}");
    let answer = call(
        &mut plain,
        json!({"ordner": folder.0, "name": "Eigen", "oz_maske": "11.22.PPPPI"}),
    )
    .unwrap();
    assert!(answer.contains("OZ-Maske 11.22.PPPPI ·"), "{answer}");
}

/// The cost templates: GA outline plus a Kostenermittlung after the template
/// of the Reiter Kosten (not mistaken for the GA template because of „DIN 276“).
#[test]
fn the_cost_templates_bring_a_cost_estimate() {
    let folder = TempFolder::new("kosten");
    let templates = folder.0.join("Vorlagen");
    let mut server = server(&folder.0, &templates);
    for (name, wanted, stage) in [
        (
            "KS",
            "Kostenschätzung DIN 276 (2. Ebene)",
            proava_domain::CostStage::Estimate,
        ),
        (
            "KB",
            "kostenberechnung",
            proava_domain::CostStage::Calculation,
        ),
    ] {
        let answer = call(
            &mut server,
            json!({"ordner": folder.0, "name": name, "vorlage": wanted}),
        )
        .unwrap();
        assert!(
            answer.contains(&format!("Vorlage: {}", stage.template_name())),
            "{answer}"
        );
        assert!(answer.contains("1 Kostenermittlung"), "{answer}");
        let project = XmlProjectFile::new()
            .load_project(&folder.0.join(format!("{name}/{name}.proava")))
            .unwrap();
        let estimate = &project.cost_estimates()[0];
        assert_eq!(estimate.stage, stage);
        assert_eq!(
            estimate.sections[0].rows.len(),
            1,
            "eine leere Zeile zum Losschreiben"
        );
        assert_eq!(
            titles(&folder.0.join(format!("{name}/{name}.proava")))[1],
            "01.481 Automationseinrichtungen"
        );
    }
}
