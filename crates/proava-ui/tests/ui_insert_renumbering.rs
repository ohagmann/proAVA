//! „Einfügen und neu nummerieren?“: at the Schreibmarke there is no free
//! order number (0078 | 0079 in an LV with its own numbers). F9 asks; Enter
//! inserts and numbers the positions of the title again (one Strg+Z), Esc
//! leaves the LV as it was.

mod common;

use common::*;
use proava_domain::{Decimal, Lv, LvGroup, OzMask, Position, Project, ProjectName};
use proava_ui::StartFile;

const QUESTION: &str = "Einfügen und neu nummerieren?";

/// Title 01.01 with 0078 | 0079, title 01.02 behind it.
fn open_tight(folder: &std::path::Path) -> UiHarness {
    let mut lv = Lv::new("Leitungen", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
    lv.push_group(LvGroup::new(0, "01", "Hardware"));
    lv.push_group(LvGroup::new(1, "01.01", "Leitungen"));
    for (ordinal, text) in [
        ("01.01.0070", "NYM 5x2,5"),
        ("01.01.0075", "NYM 5x1,5 UP"),
        ("01.01.0078", "NYM 5x1,5 Rinne"),
        ("01.01.0079", "NYM 5x1,5 Rohr"),
        ("01.01.0080", "J-Y(St)Y"),
    ] {
        lv.push_position(Position::new(ordinal, text, Decimal::ONE, "m", None));
    }
    lv.push_group(LvGroup::new(1, "01.02", "Busleitungen"));
    lv.push_position(Position::new(
        "01.02.0011",
        "Li2YCYv",
        Decimal::ONE,
        "m",
        None,
    ));
    let file = save(
        folder,
        &Project::new(ProjectName::parse("Musterstadt").unwrap(), "", lv),
    );
    let mut harness = harness(folder, Some(StartFile::Project(file)));
    // Schreibmarke on 0078.
    press(&mut harness, Key::Home);
    for _ in 0..4 {
        press(&mut harness, Key::ArrowDown);
    }
    assert_eq!(
        project(&harness).lv().rows()[selected(&harness).unwrap()].ordinal(),
        "01.01.0078"
    );
    harness
}

fn ordinals(harness: &UiHarness) -> Vec<String> {
    project(harness)
        .lv()
        .rows()
        .iter()
        .map(|row| row.ordinal().to_string())
        .collect()
}

#[test]
fn f9_without_free_number_asks_and_enter_inserts_with_new_numbers() {
    let folder = TempDir::new("einfuegen-nummerieren");
    let mut harness = open_tight(&folder.0);
    let before = ordinals(&harness);

    press(&mut harness, Key::F9);
    assert!(shows(&harness, QUESTION), "Rückfrage statt Meldung");
    assert!(
        shows_part(&harness, "in Titel 01.01 Leitungen neu"),
        "Titel genannt"
    );
    assert!(shows_part(&harness, "01.01.0079"), "Vorschau alt → neu");
    assert_eq!(ordinals(&harness), before, "erst nach der Antwort");

    press(&mut harness, Key::Enter);
    assert!(!shows(&harness, QUESTION));
    assert_eq!(
        ordinals(&harness),
        [
            "01",
            "01.01",
            "01.01.0010",
            "01.01.0020",
            "01.01.0030",
            "01.01.0040",
            "01.01.0050",
            "01.01.0060",
            "01.02",
            "01.02.0011",
        ],
        "neue Position hinter 0078, Titel 01.01 neu, 01.02 unverändert"
    );
    assert_eq!(selected(&harness), Some(5), "die neue Position ist gewählt");

    // Strg+Z: one step, everything as before.
    press(&mut harness, Key::Escape);
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(ordinals(&harness), before);
}

#[test]
fn esc_leaves_the_lv_as_it_was() {
    let folder = TempDir::new("einfuegen-nummerieren-esc");
    let mut harness = open_tight(&folder.0);
    let before = ordinals(&harness);
    press(&mut harness, Key::F9);
    assert!(shows(&harness, QUESTION));
    press(&mut harness, Key::Escape);
    assert!(!shows(&harness, QUESTION));
    assert_eq!(ordinals(&harness), before);
}
