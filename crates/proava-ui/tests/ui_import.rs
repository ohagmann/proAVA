//! GAEB ▸ Importieren …: file, preview, the targets and the protocol.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use proava_application::{ExchangeFiles, GaebExport};
use proava_exchange::GaebPdfExchange;

/// Writes the demo LV as GAEB (the extension decides X/D and the phase).
fn gaeb_file(folder: &Path, name: &str, kind: GaebExport) -> PathBuf {
    let file = folder.join(name);
    let mut project = proava_ui::demo::demo_project();
    project.lv_mut().set_title("Zulieferung Heizung");
    project.lv_mut().set_number("7");
    GaebPdfExchange.export_gaeb(&project, kind, &file).unwrap();
    file
}

fn open_import(harness: &mut UiHarness) {
    click(harness, "GAEB");
    click(harness, "Importieren …");
    assert!(shows(harness, "GAEB importieren"), "Dialog offen");
}

/// Types the path into the file field and reads it (waits for the worker).
fn read_file(harness: &mut UiHarness, file: &Path, expect: &str) {
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::TextInput
                && node.label().as_deref() == Some("GAEB-Datei")
        })
        .next()
        .expect("Feld GAEB-Datei")
        .click();
    settle(harness);
    type_text(harness, &file.to_string_lossy());
    click(harness, "Lesen");
    for _ in 0..300 {
        if shows_part(harness, expect) {
            // The dialog grew with the preview: let egui lay it out again.
            settle(harness);
            settle(harness);
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        harness.step();
    }
    panic!("Vorschau „{expect}“ erscheint nicht");
}

fn dialog_open(harness: &UiHarness) -> bool {
    harness.state().workspace().unwrap().dialog_open()
}

#[test]
fn x83_becomes_a_new_lv_with_protocol() {
    let folder = TempDir::new("import-lv");
    let file = gaeb_file(&folder.0, "Lieferung.X83", GaebExport::RequestForQuotation);
    let mut harness = open_demo(&folder.0);
    let count = project(&harness).lv_count();
    let positions = project(&harness).lv().position_count();

    open_import(&mut harness);
    read_file(&mut harness, &file, "X83 – Angebotsaufforderung");
    assert!(shows_part(&harness, "Positionen"), "Inhalt gezeigt");
    assert!(shows(&harness, "Die Datei hat keine Einheitspreise."));
    click(&mut harness, "Importieren");
    assert!(
        shows_part(&harness, "Importprotokoll Lieferung.X83"),
        "Protokoll"
    );
    // Eine Angabe je Zeile: Bezeichnung und Wert als eigene Texte.
    assert!(row(
        &harness,
        "Bearbeitungsstatus",
        "alle Positionen „offen“"
    ));
    assert!(row(&harness, "Art", "X83 – Angebotsaufforderung"));
    assert!(row(&harness, "Preise", "keine in der Datei"));
    let copied = copy_protocol(&mut harness);
    assert!(
        copied.lines().all(|line| !line.contains(" · ")),
        "keine zusammengesetzten Zeilen: {copied}"
    );
    assert!(copied.starts_with("Importprotokoll Lieferung.X83\nDatei: Lieferung.X83\nArt: X83 – Angebotsaufforderung\n"), "{copied}");
    assert!(
        copied.contains("\nÜbernommen als: neues LV 7 – Zulieferung Heizung\n"),
        "{copied}"
    );
    assert!(shows(&harness, "Kopiert"));
    click(&mut harness, "Fertig");
    assert!(!dialog_open(&harness));

    let project = project(&harness);
    assert_eq!(project.lv_count(), count + 1);
    assert_eq!(
        project.lv().title(),
        "Zulieferung Heizung",
        "neues LV aktiv"
    );
    assert_eq!(project.lv().number(), "7");
    assert_eq!(project.lv().position_count(), positions);
    assert!(project
        .lv()
        .positions()
        .all(|position| position.unit_price().is_none()));
}

#[test]
fn x82_brings_its_prices_unless_switched_off_and_a_taken_number_is_refused() {
    let folder = TempDir::new("import-x82");
    let file = gaeb_file(&folder.0, "Kostenanschlag.X82", GaebExport::PricedLv);
    let mut harness = open_demo(&folder.0);
    let net = project(&harness).lv().net_total();

    open_import(&mut harness);
    read_file(
        &mut harness,
        &file,
        "X82 – Bepreistes LV (GAEB: Kostenanschlag)",
    );
    assert!(shows_part(&harness, "Preise übernehmen ("), "Schalter da");
    // A number the project already has (the file's own, after a first import).
    click(&mut harness, "Importieren");
    click(&mut harness, "Fertig");
    assert_eq!(project(&harness).lv().number(), "7");
    open_import(&mut harness);
    read_file(
        &mut harness,
        &file,
        "X82 – Bepreistes LV (GAEB: Kostenanschlag)",
    );
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::TextInput
                && node.label().as_deref() == Some("LV-Nr.")
        })
        .next()
        .expect("LV-Nr.")
        .click();
    settle(&mut harness);
    press_with(&mut harness, Modifiers::COMMAND, Key::A);
    type_text(&mut harness, "7");
    assert!(
        shows_part(&harness, "gibt es im Projekt schon"),
        "doppelte LV-Nr. abgefangen"
    );
    press_with(&mut harness, Modifiers::COMMAND, Key::A);
    type_text(&mut harness, "8");
    click(&mut harness, "Importieren");
    click(&mut harness, "Fertig");
    assert_eq!(project(&harness).lv().number(), "8");
    assert_eq!(
        project(&harness).lv().net_total(),
        net,
        "Preise als Planerpreise"
    );

    open_import(&mut harness);
    read_file(
        &mut harness,
        &file,
        "X82 – Bepreistes LV (GAEB: Kostenanschlag)",
    );
    click_starting(&mut harness, "Preise übernehmen (");
    click(&mut harness, "Importieren");
    assert!(
        row(&harness, "Preise", "nicht übernommen (Schalter aus)"),
        "Protokoll nennt die Preise"
    );
    click(&mut harness, "Fertig");
    assert!(project(&harness)
        .lv()
        .positions()
        .all(|position| position.unit_price().is_none()));
}

#[test]
fn d81_as_a_new_project_under_another_name_is_opened() {
    let folder = TempDir::new("import-project");
    let file = gaeb_file(&folder.0, "Vortexte.D81", GaebExport::Description);
    let target = folder.0.join("Projekte");
    std::fs::create_dir_all(&target).unwrap();
    let mut harness = open_demo(&folder.0);

    open_import(&mut harness);
    read_file(&mut harness, &file, "D81 – Leistungsbeschreibung");
    click_starting(&mut harness, "Als neues Projekt");
    fill(&mut harness, "Projektname", "Vortexte zum Rauskopieren");
    fill(&mut harness, "Ordner", &target.to_string_lossy());
    click(&mut harness, "Importieren");
    for _ in 0..10 {
        harness.step();
    }
    settle(&mut harness);
    let project = project(&harness);
    assert_eq!(
        project.name().as_str(),
        "Vortexte zum Rauskopieren",
        "neues Projekt offen"
    );
    assert!(target
        .join("Vortexte zum Rauskopieren/Vortexte zum Rauskopieren.proava")
        .is_file());
    assert!(
        shows_part(&harness, "Importprotokoll Vortexte.D81"),
        "Protokoll im neuen Projekt"
    );
    assert!(row(&harness, "Übernommen als", "neues Projekt"));
    // Der Ordner steht ganz in einer Zeile (der Dialog ist breit genug).
    let place = target.join("Vortexte zum Rauskopieren");
    assert!(
        row(&harness, "Speicherort", &place.to_string_lossy()),
        "Pfad ungekürzt"
    );
    assert!(row(
        &harness,
        "Projektdatei",
        "Vortexte zum Rauskopieren.proava"
    ));
}

#[test]
fn x81_into_the_active_lv_opens_the_takeover_window_and_x84_points_to_bids() {
    let folder = TempDir::new("import-transfer");
    let file = gaeb_file(&folder.0, "Texte.X81", GaebExport::Description);
    let mut harness = open_demo(&folder.0);
    open_import(&mut harness);
    read_file(&mut harness, &file, "X81 – Leistungsbeschreibung");
    click_starting(&mut harness, "In das aktive LV übernehmen");
    click(&mut harness, "Importieren");
    assert!(
        row(&harness, "Übernommen als", "Quelle im Übernahme-Fenster"),
        "Protokoll"
    );
    click(&mut harness, "Fertig");
    assert!(
        shows(&harness, "Übernehmen aus Projekt, GAEB oder Bibliothek"),
        "Fenster offen"
    );
    assert!(shows_part(&harness, "Texte.X81"), "Datei als Quelle");

    // An X84 is a bid: the dialog says so and offers „Angebote einlesen“.
    let bid = folder.0.join("Angebot.X84");
    let mut project = proava_ui::demo::demo_project();
    project.lv_mut().set_title("Angebot");
    let x83 = folder.0.join("a.X83");
    GaebPdfExchange
        .export_gaeb(&project, GaebExport::RequestForQuotation, &x83)
        .unwrap();
    let text = std::fs::read_to_string(&x83)
        .unwrap()
        .replace("DA83", "DA84")
        .replace("<DP>83</DP>", "<DP>84</DP>");
    std::fs::write(&bid, text).unwrap();
    open_import(&mut harness);
    read_file(&mut harness, &bid, "X84 – Angebotsabgabe");
    assert!(shows(&harness, "Als Angebot einlesen …"));
}

/// A line of a protocol: the label and right of it its value (own texts,
/// the value on the same height).
fn row(harness: &UiHarness, label: &str, value: &str) -> bool {
    let labels: Vec<egui::Rect> = harness
        .query_all_by(|node| {
            node.label().as_deref() == Some(label) || node.value().as_deref() == Some(label)
        })
        .map(|node| node.rect())
        .collect();
    harness
        .query_all_by(|node| {
            node.label().as_deref() == Some(value) || node.value().as_deref() == Some(value)
        })
        .map(|node| node.rect())
        .any(|rect| {
            labels.iter().any(|caption| {
                caption.right() <= rect.left() && (caption.top() - rect.top()).abs() < 6.0
            })
        })
}

/// Clicks „Kopieren“ and returns the text that went to the clipboard.
fn copy_protocol(harness: &mut UiHarness) -> String {
    harness
        .get_all_by_label("Kopieren")
        .next()
        .expect("Kopieren")
        .click();
    for _ in 0..6 {
        harness.step();
        let copied = harness
            .output()
            .platform_output
            .commands
            .iter()
            .find_map(|command| match command {
                egui::OutputCommand::CopyText(text) => Some(text.clone()),
                _ => None,
            });
        if let Some(text) = copied {
            settle(harness);
            return text;
        }
    }
    panic!("nichts in die Zwischenablage kopiert");
}

fn click_starting(harness: &mut UiHarness, text: &str) {
    harness
        .query_all_by(|node| node.label().is_some_and(|label| label.starts_with(text)))
        .next()
        .unwrap_or_else(|| panic!("„{text}…“ fehlt"))
        .click();
    settle(harness);
}

fn fill(harness: &mut UiHarness, label: &str, text: &str) {
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::TextInput
                && node.label().as_deref() == Some(label)
        })
        .next()
        .unwrap_or_else(|| panic!("Feld „{label}“ fehlt"))
        .click();
    settle(harness);
    press_with(harness, Modifiers::COMMAND, Key::A);
    type_text(harness, text);
}

/// CSV im proAVA-Format: GAEB ▸ CSV importieren …, Vorschau mit Spalten und
/// GP-Abweichung, Währung CHF, orange Prüfmarke, „GP-Abweichung bestätigen“.
#[test]
fn csv_import_marks_a_wrong_gp_in_the_chosen_currency_until_confirmed() {
    let folder = TempDir::new("import-csv");
    let file = folder.0.join("Zulieferung.csv");
    std::fs::write(
        &file,
        "OZ:;Positionstext:;Einheit:;Menge:;EP:;GP:;KG:;Positionsart:\n\
         01.481.01.0010;Raumregler;St.;2;10;24;481;Position\n\
         ;mit Display;;;;;;\n\
         ;;;;;;;\n\
         01.481.01.0020;Fühler;St.;3;5,50;16,50;481;Position\n\
         ;Langtext;;;;;;\n",
    )
    .unwrap();
    let mut harness = open_demo(&folder.0);
    let count = project(&harness).lv_count();

    click(&mut harness, "GAEB");
    click_menu(&mut harness, "CSV importieren …");
    assert!(shows(&harness, "CSV importieren"), "Dialog offen");
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::TextInput
                && node.label().as_deref() == Some("CSV-Datei")
        })
        .next()
        .expect("Feld CSV-Datei")
        .click();
    settle(&mut harness);
    type_text(&mut harness, &file.to_string_lossy());
    click(&mut harness, "Lesen");
    for _ in 0..300 {
        if shows_part(&harness, "CSV-Tabelle") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        harness.step();
    }
    settle(&mut harness);
    settle(&mut harness);
    assert!(
        shows_part(
            &harness,
            "OZ, Positionstext, Einheit, Menge, EP, GP, KG, Positionsart"
        ),
        "Spalten"
    );
    assert!(row(&harness, "GP-Abweichungen", "1"), "Prüfung");
    assert!(row(&harness, "Summe der Abweichungen", "4,00 €"), "Prüfung");
    assert!(row(&harness, "Trenner", "Semikolon"));
    // Währung Schweizer Franken.
    assert!(shows(&harness, "Euro (EUR)"), "Vorgabe Euro");
    // „Währung“ steht auch in der Vorschau: das Auswahlfeld kommt zuletzt.
    harness
        .get_all_by_label("Währung")
        .last()
        .expect("Auswahl Währung")
        .click();
    settle(&mut harness);
    click(&mut harness, "Schweizer Franken (CHF)");
    assert!(
        row(&harness, "Währung", "Schweizer Franken (CHF)"),
        "Vorschau folgt der Wahl"
    );
    assert!(
        row(&harness, "Summe der Abweichungen", "4,00 CHF"),
        "Prüfung in CHF"
    );
    click(&mut harness, "Importieren");
    assert!(
        shows_part(&harness, "Importprotokoll Zulieferung.csv"),
        "Protokoll"
    );
    assert!(
        shows_part(
            &harness,
            "01.481.01.0010: GP laut Datei 24,00 CHF, errechnet 20,00 CHF – Abweichung 4,00 CHF"
        ),
        "Abweichung im Protokoll"
    );
    click(&mut harness, "Fertig");

    let lv = project(&harness).lv();
    assert_eq!(project(&harness).lv_count(), count + 1);
    assert_eq!(lv.currency(), "CHF");
    assert_eq!(lv.oz_mask().pattern(), "11.222.33.PPPP");
    let marked: Vec<&str> = lv
        .positions()
        .filter(|position| position.price_check().is_some())
        .map(|position| position.ordinal())
        .collect();
    assert_eq!(marked, ["01.481.01.0010"]);
    assert!(
        shows_part(&harness, "Raumregler – GP prüfen"),
        "Status in der Tabelle"
    );

    // Bestätigen über das Kontextmenü der Zeile (Status ▸ …).
    select_first_position(&mut harness);
    harness
        .query_all_by(|node| {
            node.label()
                .is_some_and(|label| label.starts_with("01.481.01.0010 Raumregler"))
        })
        .next()
        .expect("Tabellenzeile")
        .click_secondary();
    settle(&mut harness);
    assert!(row_menu_open(&harness), "Kontextmenü offen");
    // „Status“ heißt auch das Feld im Eigenschaftenfenster: das Menü kommt zuletzt.
    harness
        .get_all_by_label("Status")
        .last()
        .expect("Untermenü Status")
        .hover();
    harness.run_steps(30);
    click_menu(&mut harness, "GP-Abweichung bestätigen");
    assert!(project(&harness)
        .lv()
        .positions()
        .all(|position| position.price_check().is_none()));
    assert!(!shows_part(&harness, "GP prüfen"), "Marke weg");
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(
        project(&harness)
            .lv()
            .positions()
            .filter(|position| position.price_check().is_some())
            .count(),
        1,
        "Strg+Z holt die Marke zurück"
    );
}

/// GAEB ▸ Als CSV exportieren …: Kästchen Windows-1252, Datei im Format.
#[test]
fn csv_export_writes_the_format_with_bom() {
    let folder = TempDir::new("export-csv");
    let mut harness = open_demo(&folder.0);
    click(&mut harness, "GAEB");
    click_menu(&mut harness, "Als CSV exportieren …");
    assert!(
        shows_part(&harness, "Windows-1252 statt UTF-8"),
        "Kästchen im Exportdialog"
    );
    harness
        .get_all_by_label("Speichern")
        .last()
        .expect("Speichern")
        .click();
    settle(&mut harness);
    for _ in 0..200 {
        harness.step();
    }
    let file = std::fs::read_dir(&folder.0)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|extension| extension == "csv"))
        .expect("CSV geschrieben");
    let bytes = std::fs::read(&file).unwrap();
    assert!(bytes.starts_with(
        b"\xEF\xBB\xBFOZ:;Positionstext:;Einheit:;Menge:;EP:;GP:;KG:;Positionsart:\r\n"
    ));
    std::fs::remove_file(&file).unwrap();

    // Windows-1252 ankreuzen: ohne BOM, und beim nächsten Mal schon angekreuzt.
    use egui_kittest::kittest::NodeT;
    let checked = |harness: &UiHarness| {
        harness
            .query_all_by(|node| {
                node.label()
                    .is_some_and(|label| label.starts_with("Windows-1252 statt UTF-8"))
            })
            .next()
            .expect("Kästchen")
            .accesskit_node()
            .toggled()
    };
    click(&mut harness, "GAEB");
    click_menu(&mut harness, "Als CSV exportieren …");
    assert_eq!(
        checked(&harness),
        Some(egui::accesskit::Toggled::False),
        "Vorgabe UTF-8"
    );
    click_starting(&mut harness, "Windows-1252 statt UTF-8");
    harness
        .get_all_by_label("Speichern")
        .last()
        .expect("Speichern")
        .click();
    settle(&mut harness);
    for _ in 0..200 {
        harness.step();
    }
    let bytes = std::fs::read(&file).expect("CSV geschrieben");
    assert!(bytes.starts_with(b"OZ:;Positionstext:"), "ohne BOM");
    let settings = std::fs::read_to_string(folder.0.join("settings.json")).unwrap();
    assert!(
        settings.contains("\"csv_windows_1252\": true"),
        "gemerkt: {settings}"
    );
    click(&mut harness, "GAEB");
    click_menu(&mut harness, "Als CSV exportieren …");
    assert_eq!(
        checked(&harness),
        Some(egui::accesskit::Toggled::True),
        "beim nächsten Export angekreuzt"
    );
}

/// `proava Zulieferung.csv` (Doppelklick): „Neues Projekt aus CSV-Tabelle“
/// mit dieser Datei, danach ist das Projekt offen und zeigt das Protokoll.
#[test]
fn a_csv_start_file_becomes_a_new_project() {
    let folder = TempDir::new("start-csv");
    let file = folder.0.join("Zulieferung.csv");
    std::fs::write(
        &file,
        "OZ;Positionstext;Einheit;Menge;EP;GP\n01.0010;Regler;St;2;10;24\n;mit Display;;;;\n",
    )
    .unwrap();
    let target = folder.0.join("Projekte");
    std::fs::create_dir_all(&target).unwrap();
    let mut harness = harness(&folder.0, Some(proava_ui::StartFile::Csv(file)));
    assert!(
        shows(&harness, "Neues Projekt aus CSV-Tabelle"),
        "Startdialog für CSV"
    );
    assert!(shows(&harness, "Zulieferung"), "Name aus der Datei");
    fill(&mut harness, "Ordner", &target.to_string_lossy());
    harness
        .get_all_by_label("Anlegen")
        .last()
        .expect("Anlegen")
        .click();
    for _ in 0..300 {
        harness.step();
        if harness.state().project().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    settle(&mut harness);
    let project = project(&harness);
    assert_eq!(project.name().as_str(), "Zulieferung");
    assert!(target.join("Zulieferung/Zulieferung.proava").is_file());
    assert_eq!(
        project
            .lv()
            .positions()
            .filter(|position| position.price_check().is_some())
            .count(),
        1
    );
    assert!(
        shows_part(&harness, "Importprotokoll Zulieferung.csv"),
        "Protokoll im neuen Projekt"
    );
}
