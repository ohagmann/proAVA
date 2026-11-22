//! Felder der Position (Wünsche 18.11.2026): kein Schalter „STLB-Platzhalter“
//! mehr (Rechtsklick „Platzhalter-Markierung entfernen“ an Platzhaltern),
//! EP und Menge mit Tausenderpunkten wie der GP.

mod common;

use common::*;
use egui::accesskit::Role;
use proava_domain::{Decimal, Lv, LvGroup, OzMask, Position, Project, ProjectName, RowId};

/// A placeholder and a priced position with large numbers.
fn open(folder: &std::path::Path, panel: bool) -> (UiHarness, RowId, RowId) {
    std::fs::write(
        folder.join("settings.json"),
        format!(r#"{{"preferences":{{"detail_visible":{panel}}}}}"#),
    )
    .unwrap();
    let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "Feldgeräte"));
    let mut placeholder = Position::new("01.0010", "STLB-Bau", Decimal::ONE, "St", None);
    placeholder.set_stlb_placeholder(true);
    let placeholder = lv.push_position(placeholder);
    let priced = lv.push_position(Position::new(
        "01.0020",
        "Schaltschrank",
        "1250.5".parse().unwrap(),
        "St",
        Some("11159.54".parse().unwrap()),
    ));
    let project = Project::new(ProjectName::parse("Felder").unwrap(), "2026-11-18", lv);
    let file = save(folder, &project);
    let harness = harness(folder, Some(proava_ui::StartFile::Project(file)));
    (harness, placeholder, priced)
}

fn select(harness: &mut UiHarness, row: RowId) -> usize {
    press(harness, Key::Home);
    for _ in 0..20 {
        let at = selected(harness).unwrap();
        if project(harness).lv().rows()[at].id() == row {
            return at;
        }
        press(harness, Key::ArrowDown);
    }
    panic!("Zeile nicht erreicht");
}

fn right_click_row(harness: &mut UiHarness, index: usize) {
    let row = &project(harness).lv().rows()[index];
    let label = format!("{} {}", row.ordinal(), row.short_text());
    let at = harness
        .get_all_by_label(&label)
        .next()
        .expect("Tabellenzeile")
        .rect()
        .center();
    harness.event(egui::Event::PointerMoved(at));
    harness.step();
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        harness.step();
    }
    settle(harness);
}

fn is_placeholder(harness: &UiHarness, row: RowId) -> bool {
    project(harness)
        .lv()
        .position_by_id(row)
        .unwrap()
        .is_stlb_placeholder()
}

#[test]
fn no_stlb_switch_but_the_context_menu_takes_the_mark_off() {
    let folder = TempDir::new("stlb-markierung");
    let (mut harness, placeholder, priced) = open(&folder.0, true);
    let at = select(&mut harness, placeholder);
    assert!(
        !shows(&harness, "STLB-Platzhalter"),
        "kein Schalter im Eigenschaftenfenster"
    );
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    click(&mut harness, "Weiteres");
    assert!(
        !shows(&harness, "STLB-Platzhalter"),
        "kein Schalter unter „Weiteres“"
    );
    press(&mut harness, Key::Escape);

    // Only at a placeholder.
    let other = select(&mut harness, priced);
    right_click_row(&mut harness, other);
    assert!(shows(&harness, "Bearbeiten"), "Kontextmenü offen");
    assert!(!shows(&harness, "Platzhalter-Markierung entfernen"));
    press(&mut harness, Key::Escape);

    select(&mut harness, placeholder);
    right_click_row(&mut harness, at);
    click_menu(&mut harness, "Platzhalter-Markierung entfernen");
    assert!(
        !is_placeholder(&harness, placeholder),
        "Markierung entfernt"
    );
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert!(
        is_placeholder(&harness, placeholder),
        "Strg+Z bringt sie zurück"
    );
}

fn field_values(harness: &UiHarness) -> Vec<String> {
    harness
        .query_all_by(|node| node.role() == Role::TextInput)
        .filter_map(|node| node.value())
        .collect()
}

#[test]
fn price_and_quantity_fields_show_thousands_dots() {
    for panel in [true, false] {
        let folder = TempDir::new(if panel {
            "tausender-panel"
        } else {
            "tausender-fenster"
        });
        let (mut harness, _, priced) = open(&folder.0, panel);
        select(&mut harness, priced);
        if !panel {
            press_with(&mut harness, Modifiers::ALT, Key::Enter);
        }
        let values = field_values(&harness);
        assert!(
            values.contains(&"11.159,54".to_string()),
            "EP mit Punkt ({panel}): {values:?}"
        );
        assert!(
            values.contains(&"1.250,5".to_string()),
            "Menge mit Punkt ({panel}): {values:?}"
        );

        // Typing stays free; leaving the field shows the dots again.
        harness
            .query_all_by(|node| {
                node.role() == Role::TextInput && node.value().as_deref() == Some("11.159,54")
            })
            .next()
            .unwrap()
            .click();
        settle(&mut harness);
        press_with(&mut harness, Modifiers::COMMAND, Key::A);
        type_text(&mut harness, "2500");
        press(&mut harness, Key::Tab);
        let price = project(&harness)
            .lv()
            .position_by_id(priced)
            .unwrap()
            .unit_price();
        assert_eq!(
            price,
            Some(Decimal::from(2500)),
            "Eingabe ohne Punkt ({panel})"
        );
        assert!(
            field_values(&harness).contains(&"2.500,00".to_string()),
            "danach mit Punkt ({panel})"
        );
    }
}
