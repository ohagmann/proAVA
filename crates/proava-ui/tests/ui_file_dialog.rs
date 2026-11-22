//! Der Dateidialog wie ein Dateimanager (Anwender 18.11.): ein Klick wählt einen
//! Ordner, Doppelklick oder Enter öffnet ihn, Rücktaste und Alt+↑ gehen eine
//! Ebene höher. „Ordner wählen“ übernimmt den gewählten Unterordner, ohne
//! Wahl den gezeigten; der Knopf nennt ihn.

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use common::*;
use egui_kittest::kittest::NodeT;
use egui_kittest::Harness;
use proava_application::{Launcher, ThemeChoice};
use proava_ui::file_dialog::{DialogResult, FileDialog};
use proava_ui::theme::Palette;

struct DialogState {
    launcher: Arc<Launcher>,
    dialog: FileDialog,
    result: Option<DialogResult>,
}

type DialogHarness = Harness<'static, DialogState>;

fn dialog_harness(folder: &Path, make: impl FnOnce(&Launcher) -> FileDialog) -> DialogHarness {
    let launcher = launcher(folder);
    let dialog = make(&launcher);
    let state = DialogState {
        launcher,
        dialog,
        result: None,
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        // Real frame time, so double clicks (≤ 0,3 s apart) are recognised.
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(
            |ui, state: &mut DialogState| {
                if state.result.is_none() {
                    let palette = Palette::of(ThemeChoice::Light);
                    let result = state.dialog.show(ui, &palette, &state.launcher);
                    if result != DialogResult::Open {
                        state.result = Some(result);
                    }
                }
            },
            state,
        );
    harness.run_steps(4);
    harness
}

/// The folder „Projekt“ with the subfolder „Pläne“ (with a PDF) and a PDF
/// beside it; returns the test folder, „Projekt“ and „Pläne“.
fn plans(name: &str) -> (TempDir, PathBuf, PathBuf) {
    let folder = TempDir::new(name);
    let project = folder.0.join("Projekt");
    let sub = project.join("Pläne");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("Schema.pdf"), "%PDF").unwrap();
    std::fs::write(project.join("Bericht.pdf"), "%PDF").unwrap();
    (folder, project, sub)
}

fn row(harness: &DialogHarness, label: &str) -> egui::Rect {
    harness
        .get_all_by_label(label)
        .next()
        .unwrap_or_else(|| panic!("„{label}“ fehlt"))
        .rect()
}

fn click_row(harness: &mut DialogHarness, label: &str) {
    harness.get_all_by_label(label).next().expect(label).click();
    harness.run_steps(4);
}

fn double_click_row(harness: &mut DialogHarness, label: &str) {
    let center = row(harness, label).center();
    // Half a second after the last click: a new double click, not a triple.
    harness.run_steps(30);
    harness.event(egui::Event::PointerMoved(center));
    harness.step();
    for _ in 0..2 {
        for pressed in [true, false] {
            harness.event(egui::Event::PointerButton {
                pos: center,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
            harness.step();
        }
    }
    harness.run_steps(4);
}

fn key(harness: &mut DialogHarness, key: Key, modifiers: Modifiers) {
    harness.key_press_modifiers(modifiers, key);
    harness.run_steps(4);
}

fn dir(harness: &DialogHarness) -> PathBuf {
    harness.state().dialog.dir().to_path_buf()
}

fn button_starting(harness: &DialogHarness, start: &str) -> Option<String> {
    harness
        .query_all_by(|node| node.label().is_some_and(|label| label.starts_with(start)))
        .next()
        .and_then(|node| node.accesskit_node().label())
}

#[test]
fn one_click_chooses_a_folder_double_click_and_enter_open_it() {
    let (folder, project, sub) = plans("dateidialog-klick");
    let mut harness = dialog_harness(&folder.0, |launcher| {
        FileDialog::open(
            launcher,
            "Datei öffnen",
            "",
            project.clone(),
            &["pdf"],
            false,
        )
    });
    assert_eq!(dir(&harness), project);

    // One click: chosen, not opened.
    click_row(&mut harness, "Pläne/");
    assert_eq!(dir(&harness), project, "ein Klick öffnet nicht");
    assert!(harness.state().result.is_none());

    // Double click: one level down.
    double_click_row(&mut harness, "Pläne/");
    assert_eq!(dir(&harness), sub, "Doppelklick öffnet den Ordner");
    assert!(harness.get_all_by_label("Schema.pdf").next().is_some());

    // Backspace and Alt+↑: one level up.
    key(&mut harness, Key::Backspace, Modifiers::NONE);
    assert_eq!(dir(&harness), project, "Rücktaste geht nach oben");
    double_click_row(&mut harness, "Pläne/");
    assert_eq!(dir(&harness), sub, "wieder unten");
    key(&mut harness, Key::ArrowUp, Modifiers::ALT);
    assert_eq!(dir(&harness), project, "Alt+↑ geht nach oben");

    // Arrow keys choose (folders first), Enter opens.
    key(&mut harness, Key::ArrowDown, Modifiers::NONE);
    assert_eq!(dir(&harness), project, "Pfeiltaste wählt nur");
    key(&mut harness, Key::Enter, Modifiers::NONE);
    assert_eq!(dir(&harness), sub, "Enter öffnet den gewählten Ordner");
    key(&mut harness, Key::Backspace, Modifiers::NONE);

    // „Öffnen“ with a chosen folder opens it (no file is taken).
    click_row(&mut harness, "Pläne/");
    click_row(&mut harness, proava_ui::strings::OPEN);
    assert_eq!(dir(&harness), sub, "Öffnen mit gewähltem Ordner öffnet ihn");
    assert!(harness.state().result.is_none());

    // A file: one click chooses, a double click takes it.
    double_click_row(&mut harness, "Schema.pdf");
    assert_eq!(
        harness.state().result,
        Some(DialogResult::Chosen(vec![sub.join("Schema.pdf")]))
    );
}

#[test]
fn choose_folder_takes_the_chosen_subfolder_or_else_the_shown_one() {
    let (folder, project, sub) = plans("ordner-waehlen");

    // With a chosen subfolder: the button names it and takes it.
    let mut harness = dialog_harness(&folder.0, |launcher| {
        FileDialog::folder(launcher, "Ordner wählen", project.clone())
    });
    assert_eq!(
        button_starting(&harness, "Ordner „").as_deref(),
        Some("Ordner „Projekt“ wählen"),
        "ohne Wahl der gezeigte Ordner"
    );
    click_row(&mut harness, "Pläne/");
    assert_eq!(dir(&harness), project, "ein Klick öffnet nicht");
    let label = button_starting(&harness, "Ordner „").unwrap();
    assert_eq!(label, "Ordner „Pläne“ wählen");
    click_row(&mut harness, &label);
    assert_eq!(
        harness.state().result,
        Some(DialogResult::Chosen(vec![sub]))
    );

    // Without a choice: the folder shown.
    let mut harness = dialog_harness(&folder.0, |launcher| {
        FileDialog::folder(launcher, "Ordner wählen", project.clone())
    });
    let label = button_starting(&harness, "Ordner „").unwrap();
    click_row(&mut harness, &label);
    assert_eq!(
        harness.state().result,
        Some(DialogResult::Chosen(vec![project.clone()]))
    );
}

#[test]
fn places_and_path_parts_still_jump_with_one_click() {
    let (folder, project, sub) = plans("dateidialog-orte");
    let mut harness = dialog_harness(&folder.0, |launcher| {
        FileDialog::open(launcher, "Datei öffnen", "", sub.clone(), &["pdf"], false)
            .with_place("Testablage", &folder.0)
    });
    assert_eq!(dir(&harness), sub);
    // The path bar: one click on the parent's part.
    click_row(&mut harness, "Projekt");
    assert_eq!(dir(&harness), project, "Pfadleiste springt mit einem Klick");
    // The side bar: one click on a place.
    click_row(&mut harness, "Testablage");
    assert_eq!(dir(&harness), folder.0, "Orte springen mit einem Klick");
}

// ----- Neuer Ordner (Anwender 18.11.) -----

fn type_in(harness: &mut DialogHarness, text: &str) {
    harness.event(egui::Event::Text(text.to_string()));
    harness.run_steps(4);
}

fn shows_message(harness: &DialogHarness, part: &str) -> bool {
    harness
        .query_all_by(|node| {
            node.label().is_some_and(|label| label.contains(part))
                || node.value().is_some_and(|value| value.contains(part))
        })
        .next()
        .is_some()
}

fn subfolders(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .unwrap()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

#[test]
fn a_new_folder_is_named_made_and_chosen() {
    let (folder, project, _) = plans("neuer-ordner");
    let mut harness = dialog_harness(&folder.0, |launcher| {
        FileDialog::folder(launcher, "Ordner für das neue Projekt", project.clone())
    });
    click_row(&mut harness, "Neuer Ordner");
    assert!(
        harness
            .query_all_by(|node| node.role() == egui::accesskit::Role::TextInput
                && node.value().as_deref() == Some("Neuer Ordner"))
            .next()
            .is_some(),
        "Feld mit „Neuer Ordner“"
    );
    // The name is chosen: typing replaces it, Enter makes the folder.
    type_in(&mut harness, "Angebote 2026");
    key(&mut harness, Key::Enter, Modifiers::NONE);
    assert_eq!(subfolders(&project), ["Angebote 2026", "Pläne"]);
    assert_eq!(dir(&harness), project, "angelegt, nicht geöffnet");
    assert_eq!(
        button_starting(&harness, "Ordner „").as_deref(),
        Some("Ordner „Angebote 2026“ wählen"),
        "der neue Ordner ist gewählt"
    );
    // Enter opens it (as a chosen folder).
    key(&mut harness, Key::Enter, Modifiers::NONE);
    assert_eq!(dir(&harness), project.join("Angebote 2026"));
}

#[test]
fn esc_drops_the_new_folder_and_the_dialog_stays() {
    let (folder, project, _) = plans("neuer-ordner-esc");
    let mut harness = dialog_harness(&folder.0, |launcher| {
        FileDialog::save(
            launcher,
            "PDF speichern",
            "",
            project.clone(),
            "LV.pdf",
            "pdf",
        )
    });
    key(&mut harness, Key::N, Modifiers::COMMAND | Modifiers::SHIFT);
    assert!(shows_message(&harness, "Neuer Ordner"), "Strg+Umschalt+N");
    type_in(&mut harness, "Verworfen");
    key(&mut harness, Key::Escape, Modifiers::NONE);
    assert_eq!(subfolders(&project), ["Pläne"], "nichts angelegt");
    assert!(
        harness.state().result.is_none(),
        "Esc schließt nur das Namensfeld"
    );
    assert!(
        harness
            .query_all_by(|node| node.value().as_deref() == Some("Verworfen"))
            .next()
            .is_none(),
        "Feld weg"
    );
    // A second Esc closes the dialog as usual.
    key(&mut harness, Key::Escape, Modifiers::NONE);
    assert_eq!(harness.state().result, Some(DialogResult::Cancelled));
}

#[test]
fn a_taken_or_invalid_name_is_reported_at_the_field() {
    let (folder, project, _) = plans("neuer-ordner-fehler");
    let mut harness = dialog_harness(&folder.0, |launcher| {
        FileDialog::folder(launcher, "Ordner wählen", project.clone())
    });
    // From the right click on the list.
    harness
        .get_all_by_label("Pläne/")
        .next()
        .unwrap()
        .click_secondary();
    harness.run_steps(4);
    harness
        .query_all_by(|node| {
            node.label()
                .is_some_and(|label| label.starts_with("Neuer Ordner "))
        })
        .next()
        .expect("Rechtsklick ▸ Neuer Ordner")
        .click();
    harness.run_steps(6);

    type_in(&mut harness, "Pläne");
    key(&mut harness, Key::Enter, Modifiers::NONE);
    assert!(
        shows_message(&harness, "„Pläne“ gibt es hier schon."),
        "Meldung am Feld"
    );
    assert_eq!(subfolders(&project), ["Pläne"]);

    key(&mut harness, Key::A, Modifiers::COMMAND);
    type_in(&mut harness, "Plan/Alt");
    key(&mut harness, Key::Enter, Modifiers::NONE);
    assert!(
        shows_message(
            &harness,
            "Der Ordnername enthält ein unerlaubtes Zeichen: „/“"
        ),
        "ungültiges Zeichen"
    );
    assert_eq!(subfolders(&project), ["Pläne"], "nichts angelegt");
    assert!(harness.state().result.is_none());
}

#[test]
fn opening_offers_no_new_folder() {
    let (folder, project, _) = plans("neuer-ordner-oeffnen");
    let mut harness = dialog_harness(&folder.0, |launcher| {
        FileDialog::open(
            launcher,
            "Datei öffnen",
            "",
            project.clone(),
            &["pdf"],
            false,
        )
    });
    assert!(harness.query_all_by_label("Neuer Ordner").next().is_none());
    key(&mut harness, Key::N, Modifiers::COMMAND | Modifiers::SHIFT);
    assert!(!shows_message(&harness, "Neuer Ordner"));
}

#[cfg(unix)]
#[test]
fn no_right_to_write_is_a_message() {
    use std::os::unix::fs::PermissionsExt;
    let (folder, project, _) = plans("neuer-ordner-rechte");
    std::fs::set_permissions(&project, std::fs::Permissions::from_mode(0o555)).unwrap();
    let mut harness = dialog_harness(&folder.0, |launcher| {
        FileDialog::folder(launcher, "Ordner wählen", project.clone())
    });
    click_row(&mut harness, "Neuer Ordner");
    key(&mut harness, Key::Enter, Modifiers::NONE);
    let refused = shows_message(&harness, "Keine Schreibrechte");
    std::fs::set_permissions(&project, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(refused, "Meldung bei fehlenden Schreibrechten");
    assert_eq!(subfolders(&project), ["Pläne"]);
}
