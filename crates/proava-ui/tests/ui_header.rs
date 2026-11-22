//! Kopfleiste des Arbeitsbereichs: Zurück, Projektname, LV-Auswahl, Reiter
//! und die Knöpfe rechts (Befehl, Einstellungen, Hell/Dunkel, Speichern)
//! überdecken sich bei keiner Fensterbreite, auch bei langem Projektnamen;
//! der Name wird gekürzt (Sichtprüfung 18.11.2026: „Befehl …“ lag bei
//! 1400 px über dem Reiter „Nachträge“). Schmale Fenster legen die Reiter in
//! eine eigene Zeile, der Name bleibt (Wunsch 18.11.2026, `ui_narrow_window`).

mod common;

use common::*;
use egui::accesskit::Role;
use egui::Rect;
use egui_kittest::kittest::NodeT;
use egui_kittest::Harness;
use proava_domain::ProjectName;
use proava_ui::{App, StartFile};

const LONG: &str = "MUSTER_ING_MSR_Sporthalle_Sued LV Kostenanschlag Nachtrag 02";

fn workspace(folder: &std::path::Path, width: f32) -> UiHarness {
    workspace_named(folder, width, LONG)
}

fn workspace_named(folder: &std::path::Path, width: f32, name: &str) -> UiHarness {
    let project = proava_ui::demo::demo_project().with_name(ProjectName::parse(name).unwrap());
    let file = save(folder, &project);
    let app = App::new(launcher(folder), Some(StartFile::Project(file)));
    let mut harness = Harness::builder()
        .with_size(egui::vec2(width, 800.0))
        .with_context_setup(App::setup)
        .with_accessibility_check(false)
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    harness.run_steps(6);
    assert!(harness.state().project().is_some(), "Projekt offen");
    harness
}

#[test]
fn header_parts_never_overlap() {
    for width in [990.0, 1100.0, 1250.0, 1400.0, 1600.0, 1950.0, 2600.0] {
        let folder = TempDir::new("kopfleiste");
        let harness = workspace(&folder.0, width);
        // Everything in the header down to the menu row (narrow windows put
        // the tabs in a row of their own).
        let bottom = harness
            .query_all_by(|node| {
                node.role() == Role::Button && node.label().as_deref() == Some("Projekt")
            })
            .next()
            .expect("Menü „Projekt“")
            .rect()
            .top();
        let parts: Vec<(Rect, String)> = harness
            .query_all_by(|node| matches!(node.role(), Role::Button | Role::Tab | Role::Label))
            .filter_map(|node| {
                let rect = node.rect();
                let text = node
                    .accesskit_node()
                    .label()
                    .or_else(|| node.value())
                    .unwrap_or_default();
                (rect.center().y < bottom && rect.width() > 1.0).then(|| (rect, text.to_string()))
            })
            .collect();
        for wanted in ["Start", "LV", "Preisspiegel", "Abrechnung", "Einstellungen"] {
            assert!(
                parts.iter().any(|(_, text)| text == wanted),
                "{width}: „{wanted}“ fehlt: {parts:?}"
            );
        }
        for (index, (rect, text)) in parts.iter().enumerate() {
            assert!(
                rect.right() <= width + 0.5,
                "{width}: „{text}“ ragt aus dem Fenster"
            );
            for (other, other_text) in parts.iter().skip(index + 1) {
                assert!(
                    !rect.shrink(0.5).intersects(other.shrink(0.5)),
                    "{width}: „{text}“ {rect:?} überdeckt „{other_text}“ {other:?}"
                );
            }
        }
        // „Start“ leads the tabs, right before „LV“ in the same row; the
        // project name stands first and starts at the left edge (Anwender 18.11.).
        let start = harness
            .query_all_by(|node| {
                node.role() == Role::Button && node.label().as_deref() == Some("Start")
            })
            .next()
            .expect("Knopf Start")
            .rect();
        let lv = harness
            .query_all_by(|node| node.role() == Role::Tab && node.label().as_deref() == Some("LV"))
            .next()
            .expect("Reiter LV")
            .rect();
        assert!(
            (start.center().y - lv.center().y).abs() < 4.0,
            "{width}: Start in der Reiterzeile {start:?} {lv:?}"
        );
        assert!(
            start.right() <= lv.left() && lv.left() - start.right() < 16.0,
            "{width}: Start direkt links von LV {start:?} {lv:?}"
        );
        let name = parts
            .iter()
            .find(|(_, text)| text == LONG)
            .map(|(rect, _)| *rect)
            .unwrap_or_else(|| panic!("{width}: Projektname fehlt"));
        assert!(name.left() < 40.0, "{width}: Name ganz links {name:?}");
        if (start.center().y - name.center().y).abs() < 8.0 {
            assert!(
                start.left() > name.right(),
                "{width}: Start rechts vom Namen"
            );
        } else {
            assert!(
                start.top() > name.bottom(),
                "{width}: Start in der Reiterzeile darunter"
            );
        }
        // Wide windows show everything with its text.
        if width >= 1950.0 {
            assert!(parts.iter().any(|(_, text)| text == "Befehl …"), "{width}");
            assert!(parts.iter().any(|(_, text)| text == "Speichern"), "{width}");
            assert!(
                parts.iter().any(|(_, text)| text.starts_with("Angebote (")),
                "{width}"
            );
        }
    }
}

/// Free room left of the buttons on the right when the name is shortened:
/// the fixed 8 px plus less than one letter (the name is cut at a letter
/// and ends with „…“). Before the fix 40 px and more stayed free.
const GAP: f32 = 26.0;

/// The header row of `name`: its rectangle, the last element left of the
/// buttons on the right (tab or LV choice) and the first of those buttons.
fn name_row(harness: &UiHarness, name: &str) -> (Rect, Rect, Rect) {
    let rect_of = |role: Role, text: &str| {
        harness
            .query_all_by(|node| node.role() == role)
            .find(|node| {
                node.accesskit_node()
                    .label()
                    .or_else(|| node.value())
                    .as_deref()
                    == Some(text)
            })
            .map(|node| node.rect())
    };
    let title = rect_of(Role::Label, name).expect("Projektname");
    let search = rect_of(Role::Button, "Befehl …").expect("Knopf Befehl");
    let last = harness
        .query_all_by(|node| matches!(node.role(), Role::Button | Role::Tab))
        .map(|node| node.rect())
        .filter(|rect| {
            (rect.center().y - title.center().y).abs() < 8.0 && rect.right() < search.left()
        })
        .max_by(|a, b| a.right().total_cmp(&b.right()))
        .expect("Teil links der Knöpfe");
    (title, last, search)
}

/// The project name gets the room up to the buttons on the right: while it
/// is shortened, no gap stays free behind the tabs (Sichtprüfung 18.11.2026:
/// at 1400 px 42 px stayed free behind „Nachträge“ and the name was cut).
#[test]
fn the_name_uses_the_free_room() {
    const NAME: &str = "Neubau Sporthalle Süd Bauteil B";
    let folder = TempDir::new("kopfleiste-name");
    let wide = workspace_named(&folder.0, 2600.0, NAME);
    let natural = name_row(&wide, NAME).0.width();
    drop(wide);
    let mut shortened = 0;
    for width in [1100.0, 1250.0, 1400.0, 1500.0, 1600.0, 1800.0] {
        let folder = TempDir::new("kopfleiste-name");
        let harness = workspace_named(&folder.0, width, NAME);
        let (title, last, search) = name_row(&harness, NAME);
        let gap = search.left() - last.right();
        if title.width() < natural - 0.5 {
            shortened += 1;
            assert!(
                gap <= GAP,
                "{width}: Name gekürzt ({:.0} von {natural:.0} px), aber {gap:.0} px frei vor den Knöpfen",
                title.width()
            );
        } else {
            assert!(
                (title.width() - natural).abs() < 0.5,
                "{width}: Name {title:?}"
            );
        }
    }
    // The widths cover both cases.
    assert!(shortened > 0 && shortened < 6, "{shortened} von 6 gekürzt");
}

/// „Befehl …“ keeps its text only while the whole name fits beside it
/// (Anwender 18.11.: at 1400 px the name was cut although „Befehl“ as a symbol
/// made room for it).
#[test]
fn the_command_button_gives_up_its_text_for_the_whole_name() {
    for name in ["Demo Preisspiegel GA", "Neubau Sporthalle Süd Bauteil B"] {
        let folder = TempDir::new("kopfleiste-befehl");
        let wide = workspace_named(&folder.0, 2600.0, name);
        let natural = name_row(&wide, name).0.width();
        drop(wide);
        let mut symbol = 0;
        for step in 0..40 {
            let width = 900.0 + 20.0 * step as f32;
            let folder = TempDir::new("kopfleiste-befehl");
            let harness = workspace_named(&folder.0, width, name);
            let (title, _, search) = name_row(&harness, name);
            if search.width() > 31.0 {
                assert!(
                    title.width() >= natural - 0.5,
                    "{width}: „{name}“ gekürzt ({:.0} von {natural:.0} px) neben „Befehl …“ mit Text",
                    title.width()
                );
            } else {
                symbol += 1;
            }
        }
        assert!(symbol > 0 && symbol < 40, "{name}: beide Fälle ({symbol})");
    }
}
