//! The take-over dialog with proAVA's browser window: a download the window
//! reports is taken like a new file in the Downloads folder. The window
//! itself never starts in a test (`jobs::start_helper`); its messages are
//! simulated here.

mod common;

use std::path::PathBuf;

use common::*;
use proava_web_protocol::FromHelper;

/// The synthetic manufacturer file (made-up maker, DA XML 3.2) as if the
/// window had just saved it.
fn downloaded(folder: &std::path::Path) -> PathBuf {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../proava-gaeb/tests/data/syn6_herstellertext_32.X81");
    let file = folder.join("Drehantrieb 20 Nm.X81");
    std::fs::copy(source, &file).unwrap();
    file
}

fn open_dialog(harness: &mut UiHarness) {
    select_first_position(harness);
    menu_path(harness, "Einfügen", "Neue Position aus AUSSCHREIBEN.DE …");
    assert!(
        shows(harness, "Aus AUSSCHREIBEN.DE übernehmen"),
        "Dialog offen"
    );
}

#[test]
fn a_download_from_the_browser_window_is_taken_over_at_once() {
    let folder = TempDir::new("webfenster-download");
    let mut harness = open_demo(&folder.0);
    let before = project(&harness).lv().position_count();
    assert!(
        !harness.state_mut().browser_event(FromHelper::Ready),
        "ohne Dialog geht keine Meldung durch"
    );
    open_dialog(&mut harness);

    // A data sheet (PDF) is no text: ignored.
    let pdf = folder.0.join("Datenblatt.pdf");
    std::fs::write(&pdf, b"%PDF-1.4").unwrap();
    assert!(harness.state_mut().browser_event(FromHelper::Downloaded {
        file: pdf,
        url: "https://ausschreiben.de/x".into(),
    }));
    settle(&mut harness);
    assert_eq!(project(&harness).lv().position_count(), before);

    let file = downloaded(&folder.0);
    assert!(harness.state_mut().browser_event(FromHelper::Downloaded {
        file,
        url: "https://export.ausschreiben.de/api/v3/export/download/0e1f".into(),
    }));
    settle(&mut harness);
    settle(&mut harness);

    let lv = project(&harness).lv();
    assert_eq!(lv.position_count(), before + 3, "drei Positionen der Datei");
    let taken: Vec<_> = lv
        .positions()
        .filter(|position| position.short_text() == "Drehantrieb 20 Nm, stetig")
        .collect();
    assert_eq!(taken.len(), 1);
    let origin = taken[0].text_origin().expect("Textquelle");
    assert_eq!(origin.source, "AUSSCHREIBEN.DE");
    assert!(
        shows_part(&harness, "Aus dem Browserfenster: Drehantrieb 20 Nm.X81"),
        "Zeile im Dialog"
    );
    assert!(shows_part(
        &harness,
        "3 Positionen aus AUSSCHREIBEN.DE übernommen"
    ));
    assert!(
        harness.state().workspace().unwrap().dialog_open(),
        "Dialog bleibt offen"
    );
    // The demo LV has four bids: the dialog stays open for the next file, so
    // the reason stands in the message instead of a question (Anwender 19.11.).
    assert!(
        shows_part(
            &harness,
            "Das LV hat bereits 4 Angebote – 3 neue Positionen fehlen dort. Strg+Z nimmt es zurück."
        ),
        "Grund in der Meldung"
    );
    assert!(
        !shows(&harness, "LV hat bereits Angebote"),
        "keine Rückfrage darüber"
    );

    // The user closed the window: the dialog says so and stays usable.
    assert!(harness.state_mut().browser_event(FromHelper::Error {
        message: "Ein Fenster für AUSSCHREIBEN.DE ist schon offen – bitte dort weiterarbeiten."
            .into(),
    }));
    assert!(harness.state_mut().browser_event(FromHelper::Closed));
    settle(&mut harness);
    assert!(shows_part(&harness, "Das Browserfenster ist geschlossen."));
    assert!(shows_part(&harness, "schon offen"));
}

#[test]
fn the_web_button_never_starts_a_program_in_a_test() {
    let folder = TempDir::new("webfenster-knopf");
    let mut harness = open_demo(&folder.0);
    open_dialog(&mut harness);
    // Never silently (19.11.2026 00:08): the dialog says that and why the
    // system browser takes over – in a test, the program is no proAVA.
    assert!(
        shows_part(
            &harness,
            "Öffnet im System-Browser: Das Browserfenster startet nur aus proAVA selbst"
        ),
        "Grund sichtbar"
    );
    let before = proava_ui::jobs::refused_starts();
    click(&mut harness, "AUSSCHREIBEN.DE öffnen");
    // In a test the helper is never available: the system browser path is
    // taken and stopped by the brake.
    assert_eq!(proava_ui::jobs::refused_starts(), before + 1);
    assert!(!proava_ui::jobs::helper_available());
}
