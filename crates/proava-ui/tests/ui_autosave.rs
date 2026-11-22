//! Automatic recovery copy: written on a worker thread after the interval,
//! with the unsaved change; saving removes it again.

mod common;

use std::path::PathBuf;

use common::*;
use proava_application::ProjectRepository;
use proava_infrastructure::XmlProjectFile;

fn recovery_file(folder: &std::path::Path) -> PathBuf {
    let name = proava_ui::demo::demo_project().name().as_str().to_string();
    folder.join(format!(".{name}.proava.autosave"))
}

#[test]
fn recovery_copy_is_written_in_the_background_and_removed_by_saving() {
    let folder = TempDir::new("autosave");
    let mut harness = open_demo(&folder.0);
    let recovery = recovery_file(&folder.0);

    let index = select_first_position(&mut harness);
    press(&mut harness, Key::F2);
    press_with(&mut harness, Modifiers::COMMAND, Key::A);
    type_text(&mut harness, "Gesichert im Hintergrund");
    press(&mut harness, Key::Enter);
    assert!(harness.state().workspace().unwrap().is_dirty());
    assert!(!recovery.exists(), "noch nicht fällig");

    // Ten minutes later (default: every 5 minutes with unsaved changes).
    let now = harness.ctx.input(|input| input.time);
    harness.input_mut().time = Some(now + 600.0);
    harness.step();
    harness.input_mut().time = None;
    for _ in 0..500 {
        if recovery.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        harness.step();
    }
    settle(&mut harness);
    assert!(recovery.exists(), "Wiederherstellungsdatei geschrieben");
    let copy = XmlProjectFile::new().load_project(&recovery).unwrap();
    assert_eq!(
        copy.lv().rows()[index].short_text(),
        "Gesichert im Hintergrund"
    );
    assert!(
        harness.state().workspace().unwrap().is_dirty(),
        "Sicherung speichert nicht"
    );

    press_with(&mut harness, Modifiers::COMMAND, Key::S);
    assert!(!harness.state().workspace().unwrap().is_dirty());
    assert!(!recovery.exists(), "nach dem Speichern entfernt");
}
