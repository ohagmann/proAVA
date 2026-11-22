//! Zuschlagsposition im Fenster „Eigenschaften“ (Reiter Allgemein): statt
//! Menge und Einheit die Bezugssumme und „%“, EP = Prozentsatz; der Bezug
//! „auf genannte Positionen“ wird gewählt – hier aus den in der Tabelle
//! markierten Zeilen, während „Festhalten“ das Fenster auf dem Zuschlag hält.

mod common;

use common::*;
use proava_domain::{
    Decimal, Lv, LvGroup, OzMask, Position, Project, ProjectName, RowId, Surcharge, SurchargeKind,
};

/// Wide test screen: the table stays free of the window.
const WIDTH: f32 = 2600.0;

fn open(folder: &std::path::Path) -> (UiHarness, RowId) {
    std::fs::write(
        folder.join("settings.json"),
        r#"{"preferences":{"detail_visible":false},"position_window":[1000,950],"position_window_at":[1560,40]}"#,
    )
    .unwrap();
    let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "Erdarbeiten"));
    lv.push_position(Position::new(
        "01.0010",
        "Aushub",
        Decimal::TEN,
        "m3",
        Some(Decimal::TEN),
    ));
    lv.push_position(Position::new(
        "01.0020",
        "Verfüllen",
        Decimal::from(5),
        "m3",
        Some(Decimal::from(20)),
    ));
    let mut zuschlag = Position::new(
        "01.0030",
        "Zuschlag Kleinmengen",
        Decimal::ZERO,
        "",
        Some(Decimal::from(5)),
    );
    zuschlag.set_surcharge(Some(Surcharge {
        split: false,
        kind: SurchargeKind::Listed,
        references: Vec::new(),
    }));
    let zuschlag = lv.push_position(zuschlag);
    let project = Project::new(ProjectName::parse("Zuschlag").unwrap(), "2026-11-18", lv);
    let file = save(folder, &project);
    proava_ui::viewer_copies::use_folder_on_this_thread(Some(folder.join("pdf-anzeige")));
    let app = proava_ui::App::new(launcher(folder), Some(proava_ui::StartFile::Project(file)));
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(WIDTH, 1100.0))
        .with_context_setup(proava_ui::App::setup)
        .with_accessibility_check(false)
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut proava_ui::App| app.ui(ui), app);
    settle(&mut harness);
    (harness, zuschlag)
}

fn selected_text(harness: &UiHarness) -> String {
    project(harness).lv().rows()[selected(harness).unwrap()]
        .short_text()
        .to_string()
}

fn go_to(harness: &mut UiHarness, text: &str) {
    press(harness, Key::Home);
    for _ in 0..10 {
        if selected_text(harness) == text {
            return;
        }
        press(harness, Key::ArrowDown);
    }
    panic!("Zeile „{text}“ nicht erreicht");
}

fn references(harness: &UiHarness, zuschlag: RowId) -> usize {
    project(harness)
        .lv()
        .position_by_id(zuschlag)
        .unwrap()
        .surcharge()
        .unwrap()
        .references
        .len()
}

#[test]
fn the_reference_of_a_zuschlag_is_chosen_from_the_marked_rows() {
    let folder = TempDir::new("zuschlag-fenster");
    let (mut harness, zuschlag) = open(&folder.0);
    go_to(&mut harness, "Zuschlag Kleinmengen");
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    assert!(shows(&harness, "Bezugssumme"), "Bezugssumme statt Menge");
    assert!(shows(&harness, "Prozentsatz (%)"), "EP als Prozentsatz");
    assert!(shows_part(&harness, "Noch keine Position"));
    click(&mut harness, "Festhalten");
    // mark Aushub and Verfüllen in the table
    go_to(&mut harness, "Aushub");
    press_with(&mut harness, Modifiers::SHIFT, Key::ArrowDown);
    click(&mut harness, "Markierte Zeilen übernehmen (2)");
    assert_eq!(references(&harness, zuschlag), 2);
    // 10 × 10 + 5 × 20 = 200,00; 5 % = 10,00
    assert!(
        shows_part(&harness, "Bezugssumme 200,00 € · Zuschlag 10,00 €"),
        "Bezugssumme im Fenster"
    );
    assert_eq!(
        project(&harness).lv().net_total(),
        proava_domain::Money::from_cents(21_000)
    );
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(references(&harness, zuschlag), 0, "Strg+Z nimmt es zurück");
}
