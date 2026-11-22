//! Anwender 18.11. 12:48: the lists of the Angebote tab – Bieterverzeichnis,
//! Angebote, Schriftverkehr im Projekt – show the chosen row, follow ↑/↓,
//! remove with Entf, the tool row or the context menu after a question, and
//! Strg+Z brings the entry back.

mod common;

use common::*;
use egui_kittest::kittest::{NodeT, Queryable};
use proava_domain::{Address, InvitedBidder, LetterKind, Project};

/// The demo project with two invited bidders and a letter to the first,
/// opened on the Angebote tab.
fn open_with_bidders(folder: &std::path::Path) -> UiHarness {
    let mut project = proava_ui::demo::demo_project();
    for name in ["Erste GmbH", "Zweite KG"] {
        project.add_invitee(InvitedBidder::new(Address::named(name)));
    }
    let first = project.invitees()[0].id;
    let letter = proava_domain::bidder_letter_entry(
        &project,
        LetterKind::Invitation,
        first,
        "02.11.2026",
        b"%PDF-1.7 test".to_vec(),
    )
    .unwrap();
    project.add_correspondence(letter);
    let file = save(folder, &project);
    let mut harness = harness(folder, Some(proava_ui::StartFile::Project(file)));
    press_with(&mut harness, Modifiers::COMMAND, Key::Num2);
    harness
}

/// Selection state of the list row named `label` (none = no such row).
fn row_selected(harness: &UiHarness, label: &str) -> Option<bool> {
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::Row && node.label().as_deref() == Some(label)
        })
        .next()
        .map(|node| node.accesskit_node().toggled() == Some(egui::accesskit::Toggled::True))
}

/// Clicks the list row named `label` with `button`.
fn click_row(harness: &mut UiHarness, label: &str, button: egui::PointerButton) {
    let center = harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::Row && node.label().as_deref() == Some(label)
        })
        .next()
        .unwrap_or_else(|| panic!("Zeile „{label}“"))
        .rect()
        .center();
    harness.event(egui::Event::PointerMoved(center));
    harness.step();
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: center,
            button,
            pressed,
            modifiers: Modifiers::NONE,
        });
        harness.step();
    }
    settle(harness);
}

/// Whether every button named `label` is greyed out.
fn all_disabled(harness: &UiHarness, label: &str) -> bool {
    // Not the menu „Bearbeiten“ of the menu bar.
    let bar = menu_bar_entry(harness, "GAEB").center().y;
    let mut nodes = harness
        .query_all_by(|node| node.label().as_deref() == Some(label))
        .filter(|node| (node.rect().center().y - bar).abs() > 2.0)
        .peekable();
    assert!(nodes.peek().is_some(), "Knopf „{label}“");
    nodes.all(|node| node.accesskit_node().is_disabled())
}

fn names(project: &Project) -> Vec<String> {
    project
        .invitees()
        .iter()
        .map(|invitee| invitee.name().to_string())
        .collect()
}

#[test]
fn bidder_rows_show_the_selection_and_follow_the_arrow_keys() {
    let folder = TempDir::new("bieterliste-auswahl");
    let mut harness = open_with_bidders(&folder.0);
    // Without a choice the tools are greyed out (bids list: one is always chosen).
    for label in ["Bearbeiten", "Nach oben", "Nach unten"] {
        assert!(all_disabled(&harness, label), "{label} ohne Auswahl aus");
    }
    assert_eq!(row_selected(&harness, "Erste GmbH"), Some(false));

    click_row(&mut harness, "Erste GmbH", egui::PointerButton::Primary);
    assert_eq!(
        row_selected(&harness, "Erste GmbH"),
        Some(true),
        "Klick markiert"
    );
    assert_eq!(row_selected(&harness, "Zweite KG"), Some(false));
    assert!(!all_disabled(&harness, "Bearbeiten"));
    assert!(
        shows(&harness, "Bieter bearbeiten: Erste GmbH"),
        "Formular des gewählten Bieters"
    );

    press(&mut harness, Key::ArrowDown);
    assert_eq!(
        row_selected(&harness, "Zweite KG"),
        Some(true),
        "Pfeil runter"
    );
    assert_eq!(row_selected(&harness, "Erste GmbH"), Some(false));
    press(&mut harness, Key::ArrowUp);
    assert_eq!(
        row_selected(&harness, "Erste GmbH"),
        Some(true),
        "Pfeil hoch"
    );

    // Enter opens the bidder: typing goes into its name.
    press(&mut harness, Key::Enter);
    press_with(&mut harness, Modifiers::COMMAND, Key::A);
    type_text(&mut harness, "Erste Bau GmbH");
    assert_eq!(names(project(&harness))[0], "Erste Bau GmbH");
}

#[test]
fn removing_a_bidder_asks_names_bid_and_letters_and_undoes() {
    let folder = TempDir::new("bieter-entfernen");
    let mut harness = open_with_bidders(&folder.0);

    // Entf after a click: question, nothing gone yet.
    click_row(&mut harness, "Erste GmbH", egui::PointerButton::Primary);
    press(&mut harness, Key::Delete);
    assert!(shows_part(
        &harness,
        "„Erste GmbH“ aus dem Bieterverzeichnis dieses LV entfernen?"
    ));
    assert!(
        shows_part(&harness, "Ein abgelegtes Schreiben an diesen Bieter bleibt"),
        "Schriftverkehr genannt"
    );
    assert_eq!(
        project(&harness).invitees().len(),
        2,
        "erst nach der Rückfrage"
    );
    click(&mut harness, crate_cancel());
    assert_eq!(project(&harness).invitees().len(), 2, "Abbrechen");

    // Tool row „Entfernen …“, then confirm: the neighbour keeps the selection.
    harness
        .get_all_by_label("Entfernen …")
        .next()
        .expect("Entfernen in der Werkzeugzeile")
        .click();
    settle(&mut harness);
    harness
        .get_all_by_label("Entfernen")
        .last()
        .unwrap()
        .click();
    settle(&mut harness);
    assert_eq!(names(project(&harness)), ["Zweite KG"]);
    assert_eq!(row_selected(&harness, "Zweite KG"), Some(true));
    assert_eq!(
        project(&harness).correspondence().len(),
        1,
        "Schreiben bleibt"
    );

    // Strg+Z holt ihn zurück.
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(names(project(&harness)), ["Erste GmbH", "Zweite KG"]);

    // Rechtsklick ▸ Entfernen … on the second (the entry the menu adds).
    let rects = |harness: &UiHarness| -> Vec<egui::Rect> {
        harness
            .get_all_by_label("Entfernen …")
            .map(|node| node.rect())
            .collect()
    };
    let before = rects(&harness);
    click_row(&mut harness, "Zweite KG", egui::PointerButton::Secondary);
    assert_eq!(
        row_selected(&harness, "Zweite KG"),
        Some(true),
        "Rechtsklick wählt"
    );
    harness
        .get_all_by_label("Entfernen …")
        .find(|node| !before.contains(&node.rect()))
        .expect("Entfernen … im Kontextmenü")
        .click();
    settle(&mut harness);
    assert!(shows_part(
        &harness,
        "„Zweite KG“ aus dem Bieterverzeichnis dieses LV entfernen?"
    ));
    press(&mut harness, Key::Enter);
    assert_eq!(names(project(&harness)), ["Erste GmbH"]);
}

#[test]
fn removing_a_bid_asks_and_undoes() {
    let folder = TempDir::new("angebot-entfernen");
    let mut harness = open_with_bidders(&folder.0);
    let count = project(&harness).bids().len();
    let name = project(&harness).bids()[1].name().to_string();

    click_row(&mut harness, &name, egui::PointerButton::Primary);
    assert_eq!(row_selected(&harness, &name), Some(true));
    press(&mut harness, Key::Delete);
    assert!(shows_part(
        &harness,
        &format!("„{name}“ mit allen Preisen aus dem Projekt entfernen?")
    ));
    assert!(shows_part(&harness, "Strg+Z holt es zurück."));
    harness
        .get_all_by_label("Entfernen")
        .last()
        .unwrap()
        .click();
    settle(&mut harness);
    assert_eq!(project(&harness).bids().len(), count - 1);
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(project(&harness).bids().len(), count, "Strg+Z");
    assert_eq!(project(&harness).bids()[1].name(), name);
}

#[test]
fn filed_letters_keep_their_tools_in_place_and_follow_the_keys() {
    let folder = TempDir::new("schriftverkehr-auswahl");
    let mut harness = open_with_bidders(&folder.0);
    click(&mut harness, "Abgelegt (1) …");
    let letter = "Aufforderung an Erste GmbH vom 02.11.2026";
    // No choice: the buttons are there, greyed out.
    for label in ["Öffnen", "Speichern unter …"] {
        assert!(all_disabled(&harness, label), "{label} ohne Auswahl aus");
    }
    assert_eq!(row_selected(&harness, letter), Some(false));
    press(&mut harness, Key::ArrowDown);
    assert_eq!(row_selected(&harness, letter), Some(true), "Pfeil wählt");
    assert!(!all_disabled(&harness, "Öffnen"));
    press(&mut harness, Key::Delete);
    assert!(shows_part(
        &harness,
        &format!("{letter} mit dem PDF aus dem Projekt entfernen?")
    ));
    press(&mut harness, Key::Enter);
    assert!(project(&harness).correspondence().is_empty());
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(project(&harness).correspondence().len(), 1, "Strg+Z");
}

fn crate_cancel() -> &'static str {
    proava_ui::strings::CANCEL
}
