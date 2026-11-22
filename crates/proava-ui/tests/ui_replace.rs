//! Befund 6 (17.11.): one click on „Speichern“; an existing file brings the
//! question „… gibt es schon. Ersetzen?“ (focus on „Abbrechen“, Esc cancels).

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use common::*;
use egui_kittest::Harness;
use proava_application::{Launcher, ThemeChoice};
use proava_ui::file_dialog::{DialogResult, FileDialog};
use proava_ui::replace_question::{ReplaceAnswer, ReplaceQuestion};
use proava_ui::theme::Palette;

struct DialogState {
    launcher: Arc<Launcher>,
    dialog: FileDialog,
    result: Option<DialogResult>,
}

fn save_dialog(folder: &Path, name: &str) -> Harness<'static, DialogState> {
    let launcher = launcher(folder);
    let dialog = FileDialog::save(
        &launcher,
        "PDF speichern",
        "",
        folder.to_path_buf(),
        name,
        "pdf",
    );
    let state = DialogState {
        launcher,
        dialog,
        result: None,
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
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

fn step(harness: &mut Harness<'static, DialogState>) {
    harness.run_steps(4);
}

fn shows<S>(harness: &Harness<'static, S>, text: &str) -> bool {
    harness
        .query_all_by(|node| {
            node.label().as_deref() == Some(text) || node.value().as_deref() == Some(text)
        })
        .next()
        .is_some()
}

fn click_label(harness: &mut Harness<'static, DialogState>, label: &str) {
    harness.get_all_by_label(label).next().expect(label).click();
    step(harness);
}

const QUESTION: &str = "Die Datei „Vorhanden.pdf“ gibt es schon. Ersetzen?";

#[test]
fn existing_file_is_replaced_only_after_the_question() {
    let folder = TempDir::new("ersetzen");
    let existing = folder.0.join("Vorhanden.pdf");
    std::fs::write(&existing, "alt").unwrap();
    let mut harness = save_dialog(&folder.0, "Vorhanden.pdf");

    click_label(&mut harness, "Speichern");
    assert!(shows(&harness, QUESTION), "Rückfrage nach einem Klick");
    assert!(shows(&harness, "3 Byte"), "Größe der vorhandenen Datei");
    assert!(shows(&harness, "Geändert"));
    assert!(harness.state().result.is_none());

    // Enter presses the focused button – „Abbrechen“.
    harness.key_press(egui::Key::Enter);
    step(&mut harness);
    assert!(!shows(&harness, QUESTION), "Enter = Abbrechen");
    assert!(harness.state().result.is_none(), "Dateidialog bleibt offen");

    // Esc cancels as well and leaves the file dialog open.
    click_label(&mut harness, "Speichern");
    harness.key_press(egui::Key::Escape);
    step(&mut harness);
    assert!(!shows(&harness, QUESTION), "Esc = Abbrechen");
    assert!(
        harness.state().result.is_none(),
        "Esc schließt nur die Rückfrage"
    );

    click_label(&mut harness, "Speichern");
    click_label(&mut harness, "Ersetzen");
    assert_eq!(
        harness.state().result,
        Some(DialogResult::Chosen(vec![existing.clone()]))
    );
    assert_eq!(
        std::fs::read_to_string(&existing).unwrap(),
        "alt",
        "geschrieben wird erst danach"
    );
}

#[test]
fn letter_e_replaces_and_a_new_name_needs_no_question() {
    let folder = TempDir::new("ersetzen-taste");
    std::fs::write(folder.0.join("Vorhanden.pdf"), "alt").unwrap();
    let mut harness = save_dialog(&folder.0, "Vorhanden.pdf");
    click_label(&mut harness, "Speichern");
    harness.key_press(egui::Key::E);
    step(&mut harness);
    assert_eq!(
        harness.state().result,
        Some(DialogResult::Chosen(vec![folder.0.join("Vorhanden.pdf")]))
    );

    let mut harness = save_dialog(&folder.0, "Neu.pdf");
    click_label(&mut harness, "Speichern");
    assert_eq!(
        harness.state().result,
        Some(DialogResult::Chosen(vec![folder.0.join("Neu.pdf")])),
        "keine Rückfrage, wenn es die Datei nicht gibt"
    );
}

struct QuestionState {
    question: ReplaceQuestion,
    answer: ReplaceAnswer,
}

#[test]
fn several_files_are_asked_once_with_the_list() {
    let folder = TempDir::new("ersetzen-viele");
    let files: Vec<PathBuf> = ["A_Schreiben_Müller.pdf", "A_Schreiben_Schmidt.pdf"]
        .iter()
        .map(|name| folder.0.join(name))
        .collect();
    for file in &files {
        std::fs::write(file, "alt").unwrap();
    }
    let question = ReplaceQuestion::new(&launcher(&folder.0), files.clone());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .build_ui_state(
            |ui, state: &mut QuestionState| {
                if state.answer == ReplaceAnswer::Open {
                    state.answer = state.question.show(ui, &Palette::of(ThemeChoice::Light));
                }
            },
            QuestionState {
                question,
                answer: ReplaceAnswer::Open,
            },
        );
    harness.run_steps(4);
    assert!(shows(&harness, "2 Dateien gibt es schon. Alle ersetzen?"));
    assert!(
        harness
            .query_all_by(|node| node
                .label()
                .is_some_and(|label| label.starts_with("A_Schreiben_Müller.pdf")))
            .next()
            .is_some()
            || harness
                .query_all_by(|node| node
                    .value()
                    .is_some_and(|value| value.starts_with("A_Schreiben_Müller.pdf")))
                .next()
                .is_some(),
        "Liste der Dateien"
    );
    harness
        .get_all_by_label("Alle ersetzen")
        .next()
        .expect("Alle ersetzen")
        .click();
    harness.run_steps(4);
    assert_eq!(harness.state().answer, ReplaceAnswer::Replace);
}
