//! Eine beschädigte settings.json: das Startfenster zeigt „Zuletzt
//! verwendet“ weiter und sagt, was auf die Vorgabe zurückgefallen ist.

mod common;

use common::*;

#[test]
fn start_screen_keeps_recent_projects_and_says_what_was_repaired() {
    let folder = TempDir::new("einstellungen");
    let project = save(&folder.0, &proava_ui::demo::demo_project());
    let settings = format!(
        r#"{{"detail_width":"breit","recent":[{{"name":"Demo Preisspiegel GA","path":{:?},"opened_at":"2026-11-17T10:00:00","kind":"proava"}}],"preferences":{{"ui_zoom":"groß"}}}}"#,
        project.to_string_lossy()
    );
    std::fs::write(folder.0.join("settings.json"), settings).unwrap();

    let mut harness = harness(&folder.0, None);
    assert!(
        shows_part(&harness, "Demo Preisspiegel GA"),
        "Zuletzt verwendet bleibt"
    );
    assert!(
        shows_part(&harness, "Einige Einstellungen waren nicht lesbar"),
        "Hinweis im Startfenster"
    );
    assert!(folder.0.join("settings.json.defekt").is_file());
    // A dialog „Hinweise“: stays until confirmed.
    harness.run_steps(30);
    assert!(
        shows_part(&harness, "Einige Einstellungen waren nicht lesbar"),
        "bleibt stehen"
    );
    click(&mut harness, proava_ui::strings::DONE);
    assert!(
        !shows_part(&harness, "Einige Einstellungen waren nicht lesbar"),
        "bestätigt"
    );
    assert!(shows_part(&harness, "Demo Preisspiegel GA"));
}
