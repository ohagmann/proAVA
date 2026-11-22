//! „proAVA wurde aktualisiert – bitte neu starten“: a proAVA whose program
//! file was replaced keeps the old code (18./19.11.2026: two false error
//! reports). The status bar says so and offers „Neu starten“ – save question
//! as when quitting, then the new program with the same project.
//!
//! A test never watches its program file and never starts a program: the
//! switch `update_notice::pretend_updated` stands in for the update, and
//! `jobs::restart` is stopped by the brake (`refused_starts` proves it was
//! reached).

mod common;

use common::*;
use egui::{Key, Modifiers};
use proava_application::ProjectRepository;
use proava_ui::update_notice::{self, RESTART, TEXT};
use proava_ui::{jobs, strings};

fn lock_file(folder: &std::path::Path) -> Option<std::path::PathBuf> {
    std::fs::read_dir(folder)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(".~lock."))
        })
}

#[test]
fn without_an_update_there_is_no_notice() {
    let folder = TempDir::new("update-keins");
    let mut harness = open_demo(&folder.0);
    settle(&mut harness);
    assert!(!shows_part(&harness, TEXT));
    assert!(!shows(&harness, RESTART));
}

#[test]
fn a_clean_project_restarts_at_once_with_its_file_unlocked() {
    update_notice::pretend_updated(true);
    let folder = TempDir::new("update-sauber");
    let mut harness = open_demo(&folder.0);
    settle(&mut harness);
    assert!(shows_part(&harness, TEXT), "Hinweis in der Statusleiste");
    assert!(
        lock_file(&folder.0).is_some(),
        "Projekt gesperrt, solange offen"
    );

    let before = jobs::refused_starts();
    click(&mut harness, RESTART);
    settle(&mut harness);
    assert_eq!(
        jobs::refused_starts(),
        before + 1,
        "das neue Programm wäre gestartet worden (im Test gebremst)"
    );
    assert!(harness.state().workspace().is_none(), "Projekt geschlossen");
    assert!(
        lock_file(&folder.0).is_none(),
        "Sperre weg, damit das neue Programm das Projekt öffnen kann"
    );
    // The start page keeps the notice (should the restart fail).
    assert!(shows_part(&harness, TEXT));
}

#[test]
fn unsaved_changes_are_asked_for_first() {
    update_notice::pretend_updated(true);
    let folder = TempDir::new("update-ungespeichert");
    let mut harness = open_demo(&folder.0);
    select_first_position(&mut harness);
    press(&mut harness, Key::F2);
    press_with(&mut harness, Modifiers::COMMAND, Key::A);
    type_text(&mut harness, "Vor dem Neustart");
    press(&mut harness, Key::Enter);
    assert!(harness.state().workspace().unwrap().is_dirty());

    let before = jobs::refused_starts();
    click(&mut harness, RESTART);
    settle(&mut harness);
    assert!(
        shows(&harness, strings::WS_RESTART_TITLE),
        "Frage wie beim Beenden"
    );
    click(&mut harness, strings::CANCEL);
    settle(&mut harness);
    assert!(
        harness.state().workspace().is_some(),
        "Abbrechen: bleibt im Projekt"
    );
    assert_eq!(jobs::refused_starts(), before, "nichts gestartet");
    assert!(shows_part(&harness, TEXT), "Hinweis bleibt");

    click(&mut harness, RESTART);
    settle(&mut harness);
    click(&mut harness, strings::WS_SAVE_AND_RESTART);
    settle(&mut harness);
    assert_eq!(
        jobs::refused_starts(),
        before + 1,
        "nach dem Speichern neu gestartet"
    );
    assert!(harness.state().workspace().is_none());
    let file = folder.0.join(format!(
        "{}.proava",
        proava_ui::demo::demo_project().name().as_str()
    ));
    let saved = proava_infrastructure::XmlProjectFile::new()
        .load_project(&file)
        .unwrap();
    assert!(
        saved
            .lv()
            .rows()
            .iter()
            .any(|row| row.short_text() == "Vor dem Neustart"),
        "gespeichert"
    );
}

#[test]
fn the_start_page_shows_the_notice_too() {
    update_notice::pretend_updated(true);
    let folder = TempDir::new("update-start");
    let mut harness = harness(&folder.0, None);
    settle(&mut harness);
    assert!(shows_part(&harness, TEXT));
    let before = jobs::refused_starts();
    click(&mut harness, RESTART);
    settle(&mut harness);
    assert_eq!(jobs::refused_starts(), before + 1);
}
