//! „Aus STLB-Bau übernehmen“ takes a file once (Anwender 19.11.): with
//! „sofort übernehmen“ a download goes in at once and „Übernehmen“ is grey
//! afterwards; without it the button takes once; a new file makes it active
//! again; the same positions again only after the question „noch einmal?“;
//! Strg+Z in the dialog takes the last take-over back. The browser window
//! never starts in a test – its downloads are simulated, made-up files only.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use egui_kittest::kittest::NodeT;
use proava_ui::stlb_dialog::{AGAIN_LABEL, REPEAT_QUESTION, TAKEN_LABEL};
use proava_web_protocol::FromHelper;

/// A synthetic test file as if the browser window had just saved it under
/// `name`.
fn downloaded(folder: &Path, test_file: &str, name: &str) -> PathBuf {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../proava-gaeb/tests/data")
        .join(test_file);
    let file = folder.join(name);
    std::fs::copy(source, &file).unwrap();
    file
}

/// Three STLB-Bau positions (STLB-Bau Online, DA XML 3.3).
const THREE: &str = "syn7_stlb_online_33.X81";
/// Four other positions (two with STLB-Bau key).
const FOUR: &str = "syn4_stlb_33.X83";

fn open_dialog(harness: &mut UiHarness) {
    select_first_position(harness);
    menu_path(harness, "Einfügen", "Neue Position aus STLB-Bau …");
    assert!(shows(harness, "Aus STLB-Bau übernehmen"), "Dialog offen");
}

fn download(harness: &mut UiHarness, file: PathBuf) {
    assert!(harness.state_mut().browser_event(FromHelper::Downloaded {
        file,
        url: "https://www.stlb-bau-online.de/export".into(),
    }));
    settle(harness);
    settle(harness);
}

fn count(harness: &UiHarness) -> usize {
    project(harness).lv().position_count()
}

/// The button `label`: Some(enabled) when it is shown.
fn button_state(harness: &UiHarness, label: &str) -> Option<bool> {
    let buttons: Vec<bool> = harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::Button && node.label().as_deref() == Some(label)
        })
        .map(|node| !node.accesskit_node().is_disabled())
        .collect();
    assert!(buttons.len() <= 1, "„{label}“ mehrfach");
    buttons.first().copied()
}

fn button(harness: &mut UiHarness, label: &str) {
    harness
        .get_all_by_label(label)
        .next()
        .unwrap_or_else(|| panic!("Knopf „{label}“ fehlt"))
        .click();
    settle(harness);
}

#[test]
fn an_automatic_take_over_is_not_repeated_by_the_button() {
    let folder = TempDir::new("stlb-einmal-auto");
    let mut harness = open_demo(&folder.0);
    let starts = proava_ui::jobs::refused_starts();
    let before = count(&harness);
    open_dialog(&mut harness);
    assert!(
        shows(&harness, "Neue Dateien werden sofort übernommen."),
        "Hinweis über dem Knopf"
    );

    download(
        &mut harness,
        downloaded(&folder.0, THREE, "Wandschrank.X81"),
    );
    assert_eq!(count(&harness), before + 3, "genau eine Übernahme");
    assert!(shows_part(
        &harness,
        "3 Positionen übernommen – Strg+Z nimmt es zurück."
    ));
    assert!(shows(&harness, "bereits übernommen (3 Positionen)"));
    assert!(shows(&harness, TAKEN_LABEL), "Knopf heißt „Übernommen“");
    assert!(
        !shows(&harness, "Übernehmen (3)"),
        "kein aktiver Hauptknopf"
    );

    // The grey button takes nothing.
    button(&mut harness, TAKEN_LABEL);
    assert_eq!(count(&harness), before + 3, "keine zweite Übernahme");
    assert!(!shows(&harness, REPEAT_QUESTION));

    // On explicit wish: the small button beside it, once more.
    button(&mut harness, AGAIN_LABEL);
    assert_eq!(count(&harness), before + 6, "bewusst ein zweites Mal");

    assert_eq!(
        proava_ui::jobs::refused_starts(),
        starts,
        "kein Programmstart"
    );
}

#[test]
fn without_auto_the_button_takes_once_and_a_new_file_makes_it_active_again() {
    let folder = TempDir::new("stlb-einmal-knopf");
    let mut harness = open_demo(&folder.0);
    let before = count(&harness);
    open_dialog(&mut harness);
    click(
        &mut harness,
        "Neue Datei aus dem Download-Ordner sofort übernehmen",
    );
    assert!(!shows(&harness, "Neue Dateien werden sofort übernommen."));

    download(
        &mut harness,
        downloaded(&folder.0, THREE, "Wandschrank.X81"),
    );
    assert_eq!(count(&harness), before, "ohne „sofort“ nur eingelesen");
    button(&mut harness, "Übernehmen (3)");
    assert_eq!(count(&harness), before + 3);
    assert_eq!(button_state(&harness, TAKEN_LABEL), Some(false));
    button(&mut harness, TAKEN_LABEL);
    assert_eq!(count(&harness), before + 3, "zweiter Klick: gesperrt");

    // A new file: the button is active again.
    download(&mut harness, downloaded(&folder.0, FOUR, "Regelung.X83"));
    assert_eq!(
        button_state(&harness, "Übernehmen (4)"),
        Some(true),
        "wieder aktiv"
    );
    assert_eq!(button_state(&harness, AGAIN_LABEL), None);
    assert!(!shows(&harness, "bereits übernommen (3 Positionen)"));
    button(&mut harness, "Übernehmen (4)");
    assert_eq!(count(&harness), before + 7);

    // The first file once more (read again): active, but it asks first.
    download(
        &mut harness,
        downloaded(&folder.0, THREE, "Wandschrank (1).X81"),
    );
    assert!(shows(&harness, "Übernehmen (3)"));
    button(&mut harness, "Übernehmen (3)");
    assert!(shows(&harness, REPEAT_QUESTION), "Rückfrage");
    assert_eq!(
        button_state(&harness, "Übernehmen (3)"),
        Some(false),
        "während der Rückfrage grau"
    );
    assert_eq!(count(&harness), before + 7, "erst nach der Antwort");
    button(&mut harness, "Nicht übernehmen");
    assert!(!shows(&harness, REPEAT_QUESTION));
    assert_eq!(count(&harness), before + 7);
    button(&mut harness, "Übernehmen (3)");
    button(&mut harness, AGAIN_LABEL);
    assert_eq!(count(&harness), before + 10, "auf ausdrücklichen Wunsch");
}

#[test]
fn the_same_download_again_asks_instead_of_taking_it_at_once() {
    let folder = TempDir::new("stlb-einmal-wieder");
    let mut harness = open_demo(&folder.0);
    let before = count(&harness);
    open_dialog(&mut harness);
    download(
        &mut harness,
        downloaded(&folder.0, THREE, "Wandschrank.X81"),
    );
    assert_eq!(count(&harness), before + 3);

    // STLB-Bau Online exports the same positions once more (new name).
    download(
        &mut harness,
        downloaded(&folder.0, THREE, "Wandschrank (1).X81"),
    );
    assert_eq!(count(&harness), before + 3, "nicht sofort übernommen");
    assert!(shows(&harness, REPEAT_QUESTION));
    // A different file in between counts as new.
    download(&mut harness, downloaded(&folder.0, FOUR, "Regelung.X83"));
    assert!(!shows(&harness, REPEAT_QUESTION));
    assert_eq!(count(&harness), before + 7, "neue Datei sofort");
}

#[test]
fn strg_z_in_the_dialog_takes_the_take_over_back() {
    let folder = TempDir::new("stlb-einmal-zurueck");
    let mut harness = open_demo(&folder.0);
    let before = count(&harness);
    open_dialog(&mut harness);
    download(
        &mut harness,
        downloaded(&folder.0, THREE, "Wandschrank.X81"),
    );
    assert_eq!(count(&harness), before + 3);

    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(count(&harness), before, "Übernahme zurückgenommen");
    assert!(
        harness.state().workspace().unwrap().dialog_open(),
        "Dialog bleibt offen"
    );
    assert!(shows_part(&harness, "3 Positionen zurückgenommen"));
    assert_eq!(
        button_state(&harness, "Übernehmen (3)"),
        Some(true),
        "Knopf wieder aktiv"
    );
    button(&mut harness, "Übernehmen (3)");
    assert_eq!(count(&harness), before + 3, "ohne Rückfrage wieder hinein");
    assert!(!shows(&harness, REPEAT_QUESTION));
}

#[test]
fn the_ausschreiben_dialog_takes_a_download_once_too() {
    let folder = TempDir::new("ausschreiben-einmal");
    let mut harness = open_demo(&folder.0);
    let before = count(&harness);
    select_first_position(&mut harness);
    menu_path(
        &mut harness,
        "Einfügen",
        "Neue Position aus AUSSCHREIBEN.DE …",
    );
    assert!(shows(&harness, "Aus AUSSCHREIBEN.DE übernehmen"));
    let file = downloaded(
        &folder.0,
        "syn6_herstellertext_32.X81",
        "Drehantrieb 20 Nm.X81",
    );
    download(&mut harness, file);
    assert_eq!(count(&harness), before + 3);
    assert_eq!(button_state(&harness, TAKEN_LABEL), Some(false));
    button(&mut harness, TAKEN_LABEL);
    assert_eq!(count(&harness), before + 3, "keine zweite Übernahme");
    let again = downloaded(
        &folder.0,
        "syn6_herstellertext_32.X81",
        "Drehantrieb 20 Nm (1).X81",
    );
    download(&mut harness, again);
    assert!(shows(&harness, REPEAT_QUESTION));
    assert_eq!(count(&harness), before + 3);
}
