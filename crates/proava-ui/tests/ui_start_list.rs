//! Startfenster, Liste „Zuletzt verwendet“: lange Namen und Pfade, Projekte
//! mit fehlender Datei. Text, „Datei fehlt“, „Pfad suchen …“, „Entfernen“
//! und „zuletzt geöffnet …“ überdecken sich bei keiner Fensterbreite und
//! bleiben in ihrer Zeile; keine Zeile ragt in die Vorschau-Spalte
//! (Fehler vom Anwender, 18.11.2026 09:56).

mod common;

use common::*;
use egui::accesskit::Role;
use egui::Rect;
use egui_kittest::kittest::NodeT;
use egui_kittest::Harness;
use proava_ui::App;

const LONG: &str = "MUSTER_ING_MSR_Sporthalle_Sued_Leistungsverzeichnis_LV Kostenanschlag mit Nachtrag 02 und Bedarfspositionen";

/// Recent list: two projects that exist, three whose file is missing.
fn prepare(folder: &std::path::Path) {
    let deep = folder
        .join("Abgaben/2026-11-18/Leistungsverzeichnisse/sehr_langer_Ordnername_fuer_die_Pruefung");
    std::fs::create_dir_all(&deep).unwrap();
    let existing = save(&deep, &proava_ui::demo::demo_project());
    let other_folder = deep.join("zweites");
    std::fs::create_dir_all(&other_folder).unwrap();
    let other = save(&other_folder, &proava_ui::demo::demo_project());
    let text = |value: &str| format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""));
    let entry = |name: &str, path: &std::path::Path| {
        format!(
            r#"{{"name":{},"path":{},"opened_at":"2026-11-18T09:30:00","kind":"proava"}}"#,
            text(name),
            text(&path.to_string_lossy())
        )
    };
    let missing = |name: &str| deep.join(format!("{}.proava", name.replace(' ', "_")));
    let recent = [
        entry(&format!("{LONG} Stand Abgabe"), &existing),
        entry("Kurz", &other),
        entry(LONG, &missing(LONG)),
        entry(
            "MUSTER_ING_MSR_Schule_Nord LV Kostenanschlag",
            &missing("Schule Nord"),
        ),
        entry("Kurz fehlt", &missing("Kurz")),
    ];
    let settings = format!(r#"{{"theme":"dark","recent":[{}]}}"#, recent.join(","));
    std::fs::write(folder.join("settings.json"), settings).unwrap();
}

fn start_screen(folder: &std::path::Path, width: f32) -> UiHarness {
    let app = App::new(launcher(folder), None);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(width, 900.0))
        .with_context_setup(App::setup)
        .with_accessibility_check(false)
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    harness.run_steps(6);
    harness
}

fn inside(inner: Rect, outer: Rect) -> bool {
    outer.expand(0.5).contains_rect(inner)
}

fn overlap(a: Rect, b: Rect) -> bool {
    a.shrink(0.5).intersects(b.shrink(0.5))
}

#[test]
fn recent_rows_never_overlap_and_stay_in_the_list() {
    let folder = TempDir::new("startliste");
    prepare(&folder.0);
    for width in [900.0, 1100.0, 1300.0, 1600.0, 1950.0, 2600.0] {
        let harness = start_screen(&folder.0, width);
        let preview = harness
            .query_all_by(|node| {
                node.value()
                    .is_some_and(|value| value.starts_with("Ein Projekt in der Liste wählen"))
            })
            .next()
            .expect("Vorschau-Spalte")
            .rect();
        let names = [
            format!("{LONG} Stand Abgabe"),
            "Kurz".into(),
            LONG.into(),
            "MUSTER_ING_MSR_Schule_Nord LV Kostenanschlag".into(),
            "Kurz fehlt".into(),
        ];
        let rows: Vec<Rect> = names
            .iter()
            .map(|name| {
                harness
                    .query_all_by(|node| {
                        node.role() == Role::Button
                            && node.label().as_deref() == Some(name.as_str())
                    })
                    .next()
                    .unwrap_or_else(|| panic!("{width}: Zeile „{name}“ fehlt"))
                    .rect()
            })
            .collect();
        let parts: Vec<(Rect, String)> = harness
            .query_all_by(|node| matches!(node.role(), Role::Label | Role::Button))
            .map(|node| {
                let text = node
                    .accesskit_node()
                    .label()
                    .or_else(|| node.value())
                    .unwrap_or_default()
                    .to_string();
                (node.rect(), text)
            })
            .collect();
        let mut missing_rows = 0;
        for (row, name) in rows.iter().zip(&names) {
            assert!(
                row.right() <= preview.left(),
                "{width}: Zeile „{name}“ ragt in die Vorschau ({row:?}, {preview:?})"
            );
            assert!(
                row.right() <= width,
                "{width}: Zeile „{name}“ ragt aus dem Fenster"
            );
            let in_row: Vec<&(Rect, String)> = parts
                .iter()
                .filter(|(rect, text)| {
                    row.contains(rect.center()) && !(rect == row && text == name)
                })
                .collect();
            assert!(
                in_row.len() >= 3,
                "{width}: „{name}“ hat zu wenig Teile: {in_row:?}"
            );
            if in_row.iter().any(|(_, text)| text == "Datei fehlt") {
                missing_rows += 1;
                for wanted in ["Pfad suchen …", "Entfernen"] {
                    assert!(
                        in_row.iter().any(|(_, text)| text == wanted),
                        "{width}: „{wanted}“ fehlt in „{name}“"
                    );
                }
            } else {
                assert!(
                    in_row
                        .iter()
                        .any(|(_, text)| text.starts_with("zuletzt geöffnet")),
                    "{width}: Datum fehlt in „{name}“"
                );
            }
            for (index, (rect, text)) in in_row.iter().enumerate() {
                assert!(
                    inside(*rect, *row),
                    "{width}: „{text}“ ragt aus der Zeile „{name}“ ({rect:?} in {row:?})"
                );
                for (other, other_text) in in_row.iter().skip(index + 1) {
                    assert!(
                        !overlap(*rect, *other),
                        "{width}: „{text}“ überdeckt „{other_text}“ in „{name}“"
                    );
                }
            }
        }
        assert_eq!(missing_rows, 3, "{width}");
    }
}

#[test]
fn rows_with_a_missing_file_keep_their_spacing() {
    let folder = TempDir::new("startliste-abstand");
    prepare(&folder.0);
    let harness = start_screen(&folder.0, 1950.0);
    let tops: Vec<Rect> = ["Kurz", LONG, "Kurz fehlt"]
        .iter()
        .map(|name| {
            harness
                .query_all_by(|node| {
                    node.role() == Role::Button && node.label().as_deref() == Some(*name)
                })
                .next()
                .expect(name)
                .rect()
        })
        .collect();
    // Each row starts below the previous one with a gap (the buttons in a row
    // must not move the list's cursor back up).
    assert!(tops[1].top() >= tops[0].bottom() + 4.0, "{tops:?}");
    assert!(tops[2].top() > tops[1].bottom() + 4.0 + 56.0, "{tops:?}");
}
