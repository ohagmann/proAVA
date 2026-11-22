//! Zeilen- und Absatzabstand der Texte (gleichmäßig und einstellbar):
//! Extras ▸ Einstellungen ▸ Darstellung, gilt für
//! den Texteditor und den Druck, wird gespeichert.

mod common;

use common::*;
use proava_application::{LineSpacing, ParagraphGap, TextSpacing};

#[test]
fn text_spacing_is_set_in_the_settings_stored_and_used_by_the_editor() {
    let folder = TempDir::new("textabstand");
    let mut harness = open_demo(&folder.0);
    assert_eq!(
        proava_ui::rich_editor::text_spacing(&harness.ctx),
        TextSpacing::default(),
        "Vorgabe: einfach, kein Abstand nach Absatz"
    );
    click(&mut harness, "Einstellungen");
    assert!(shows(&harness, "Zeilen- und Absatzabstand der Texte"));
    click(&mut harness, "1,5");
    click(&mut harness, "6 pt");
    click(&mut harness, "Übernehmen");
    let wanted = TextSpacing {
        line: LineSpacing::OneHalf,
        gap: ParagraphGap::Points6,
    };
    assert_eq!(launcher(&folder.0).text_spacing(), wanted, "gespeichert");
    assert_eq!(
        launcher(&folder.0).preferences().print_look.text,
        wanted,
        "der Druck nimmt denselben Wert"
    );
    settle(&mut harness);
    assert_eq!(
        proava_ui::rich_editor::text_spacing(&harness.ctx),
        wanted,
        "der Texteditor auch"
    );
}
