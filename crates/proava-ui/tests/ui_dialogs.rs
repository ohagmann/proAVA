//! Shell: tabs, print dialog, settings, Projektstammdaten and the LV list.

mod common;

use common::*;
use proava_application::ThemeChoice;
use proava_ui::Tab;

fn tab(harness: &UiHarness) -> Tab {
    harness.state().workspace().unwrap().tab()
}

fn dialog_open(harness: &UiHarness) -> bool {
    harness.state().workspace().unwrap().dialog_open()
}

/// Clicks the first widget whose name starts with `text`.
fn click_starting(harness: &mut UiHarness, text: &str) {
    harness
        .query_all_by(|node| node.label().is_some_and(|label| label.starts_with(text)))
        .next()
        .unwrap_or_else(|| panic!("„{text}…“ fehlt"))
        .click();
    settle(harness);
}

/// Focuses the text field named `label` (its caption), selects all and types.
fn fill_field(harness: &mut UiHarness, label: &str, text: &str) {
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::TextInput
                && node.label().as_deref() == Some(label)
        })
        .next()
        .unwrap_or_else(|| panic!("Feld „{label}“ fehlt"))
        .click();
    settle(harness);
    press_with(harness, Modifiers::COMMAND, Key::A);
    type_text(harness, text);
}

#[test]
fn tabs_switch_between_lv_bids_and_comparison() {
    let folder = TempDir::new("tabs");
    let mut harness = open_demo(&folder.0);
    assert_eq!(tab(&harness), Tab::Lv);
    click_starting(&mut harness, "Angebote (");
    assert_eq!(tab(&harness), Tab::Bids);
    let bidder = project(&harness).bids()[0].bidder().name1.clone();
    assert!(shows_part(&harness, &bidder), "Bieter „{bidder}“ sichtbar");
    click(&mut harness, "Preisspiegel");
    assert_eq!(tab(&harness), Tab::Comparison);
    press_with(&mut harness, Modifiers::COMMAND, Key::Num1);
    assert_eq!(tab(&harness), Tab::Lv);
}

#[test]
fn print_dialog_opens_with_ctrl_p_and_closes() {
    let folder = TempDir::new("print");
    let mut harness = open_demo(&folder.0);
    press_with(&mut harness, Modifiers::COMMAND, Key::P);
    assert!(dialog_open(&harness), "Druckdialog offen");
    assert!(shows(&harness, "PDF speichern …"), "Knopf sichtbar");
    click(&mut harness, "Abbrechen");
    assert!(!dialog_open(&harness), "Druckdialog zu");
    assert!(!shows(&harness, "PDF speichern …"));
    press_with(&mut harness, Modifiers::COMMAND, Key::P);
    press(&mut harness, Key::Escape);
    assert!(!dialog_open(&harness), "Esc schließt");
}

#[test]
fn settings_change_is_applied_and_stored() {
    let folder = TempDir::new("settings");
    let mut harness = open_demo(&folder.0);
    assert_eq!(launcher(&folder.0).theme(), ThemeChoice::Light);
    click(&mut harness, "Einstellungen");
    assert!(dialog_open(&harness));
    click(&mut harness, "Dunkel");
    click(&mut harness, "Übernehmen");
    assert!(!dialog_open(&harness), "Dialog zu");
    assert_eq!(
        launcher(&folder.0).theme(),
        ThemeChoice::Dark,
        "gespeichert"
    );
    assert!(
        shows(&harness, "Helles Aussehen"),
        "Knopf zeigt jetzt das helle Aussehen an"
    );
    click(&mut harness, "Helles Aussehen");
    assert_eq!(
        launcher(&folder.0).theme(),
        ThemeChoice::Light,
        "Sonne schaltet zurück"
    );
    assert!(shows(&harness, "Dunkles Aussehen"), "Mond-Knopf");
}

#[test]
fn master_data_with_an_invalid_date_shows_the_error_at_the_field() {
    let folder = TempDir::new("master");
    let mut harness = open_demo(&folder.0);
    let info = project(&harness).info().clone();
    click(&mut harness, "Projekt");
    click_menu(&mut harness, "Projektstammdaten …");
    assert!(dialog_open(&harness));
    click(&mut harness, "Vergabe & Termine");
    fill_field(&mut harness, "Abgabetermin", "31.02.2026");
    click(&mut harness, "Übernehmen");
    assert!(dialog_open(&harness), "Dialog bleibt offen");
    assert_eq!(project(&harness).info(), &info, "nichts übernommen");
    assert!(shows(&harness, "Fehler im Reiter Vergabe & Termine."));

    fill_field(&mut harness, "Abgabetermin", "27.02.2026");
    click(&mut harness, "Übernehmen");
    assert!(!dialog_open(&harness), "gültiges Datum übernommen");
    assert!(project(&harness)
        .info()
        .submission_deadline
        .starts_with("27.02.2026"));
}

#[test]
fn lv_list_creates_and_switches_lvs() {
    let folder = TempDir::new("lvlist");
    let mut harness = open_demo(&folder.0);
    let first = project(&harness).lv().title().to_string();
    let count = project(&harness).lvs().count();

    // Commands for LVs are in the menu LV (and at the LV head rows).
    open_bar_menu(&mut harness, "LV");
    click_menu(&mut harness, "Neues LV …");
    assert!(dialog_open(&harness));
    press(&mut harness, Key::Tab);
    type_text(&mut harness, "Elektro");
    click(&mut harness, "Anlegen");
    assert!(!dialog_open(&harness));
    assert_eq!(project(&harness).lvs().count(), count + 1);
    assert_eq!(project(&harness).lv().title(), "Elektro", "neues LV aktiv");
    assert!(shows_part(&harness, "Elektro"), "Titel sichtbar");

    // Both LVs stand in the table, each with its head row; the new one is
    // shown from its head row. „Gehe zu LV“ goes back to the first.
    assert!(
        harness
            .query_all_by(|node| node
                .label()
                .is_some_and(|label| label.starts_with("LV-Kopf") && label.ends_with("Elektro")))
            .next()
            .is_some(),
        "Kopfzeile des neuen LV im Blick"
    );
    open_bar_menu(&mut harness, "LV");
    open_submenu(&mut harness, "Gehe zu LV");
    harness
        .query_all_by(|node| node.label().is_some_and(|label| label.contains(&first)))
        .last()
        .expect("Eintrag des ersten LV")
        .click();
    settle(&mut harness);
    assert_eq!(project(&harness).active_lv(), 0, "zurück auf das erste LV");
}

/// GAEB-Export: Kästchen „Bilder mitgeben“ im Speichern-Dialog, Vorgabe an;
/// die Wahl wird gemerkt und gilt für den Export.
#[test]
fn gaeb_export_dialog_offers_images_and_remembers_the_choice() {
    use egui_kittest::kittest::NodeT;
    let folder = TempDir::new("bilder");
    let mut harness = open_demo(&folder.0);
    let open_export = |harness: &mut UiHarness| {
        click(harness, "GAEB");
        click_menu(harness, "Export Leistungsbeschreibung (X81) …");
        if shows(harness, "Trotzdem exportieren") {
            click(harness, "Trotzdem exportieren");
        }
        assert!(
            shows_part(harness, "Bilder mitgeben"),
            "Kästchen im Exportdialog"
        );
    };
    open_export(&mut harness);
    let checked = |harness: &UiHarness| {
        harness
            .query_all_by(|node| {
                node.label()
                    .is_some_and(|label| label.starts_with("Bilder mitgeben"))
            })
            .next()
            .expect("Kästchen")
            .accesskit_node()
            .toggled()
    };
    assert_eq!(
        checked(&harness),
        Some(egui::accesskit::Toggled::True),
        "Vorgabe an"
    );
    click_starting(&mut harness, "Bilder mitgeben");
    assert_eq!(checked(&harness), Some(egui::accesskit::Toggled::False));
    // „Speichern“ gibt es auch in der Werkzeugleiste: der letzte ist der im Dialog.
    harness
        .get_all_by_label("Speichern")
        .last()
        .expect("Speichern")
        .click();
    settle(&mut harness);
    assert!(!dialog_open(&harness), "Dialog geschlossen");
    for _ in 0..200 {
        harness.step();
    }
    let settings = std::fs::read_to_string(folder.0.join("settings.json")).unwrap();
    assert!(
        settings.contains("\"gaeb_images\": false"),
        "gemerkt: {settings}"
    );
    let written = std::fs::read_dir(&folder.0)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .any(|entry| entry.file_name().to_string_lossy().ends_with(".X81"));
    assert!(written, "X81 geschrieben");

    open_export(&mut harness);
    assert_eq!(
        checked(&harness),
        Some(egui::accesskit::Toggled::False),
        "beim nächsten Export aus"
    );
}

#[test]
fn a_new_bidder_gets_the_typing_in_its_name_field() {
    let folder = TempDir::new("new-bidder");
    let mut harness = open_demo(&folder.0);
    click_starting(&mut harness, "Angebote (");
    let before = project(&harness).invitees().len();
    click(&mut harness, "Bieter hinzufügen");
    type_text(&mut harness, "Neue Firma GmbH");
    let invitees = project(&harness).invitees();
    assert_eq!(invitees.len(), before + 1);
    assert_eq!(invitees.last().unwrap().name(), "Neue Firma GmbH");
}

#[test]
fn project_menu_opens_another_project_and_pruefen_menu_checks_the_lv() {
    let folder = TempDir::new("open-other");
    let mut harness = open_demo(&folder.0);
    // „LV prüfen …“ only under Prüfen (Anwender 18.11.), no longer under GAEB.
    open_bar_menu(&mut harness, "GAEB");
    assert!(!shows_part(&harness, "LV prüfen"), "nicht mehr unter GAEB");
    press(&mut harness, Key::Escape);
    menu_path(&mut harness, "Prüfen", "LV prüfen …");
    assert!(dialog_open(&harness), "LV prüfen aus dem Menü Prüfen");
    press(&mut harness, Key::Escape);

    click(&mut harness, "Projekt");
    click_menu(&mut harness, "Projekt öffnen …");
    assert!(
        harness.state().project().is_none(),
        "zurück auf der Startseite"
    );
    assert!(
        harness.get_all_by_label("Öffnen").next().is_some(),
        "Dialog „Projekt öffnen“ ist offen"
    );
}

#[test]
fn master_data_tabs_put_the_cursor_into_their_first_field() {
    let folder = TempDir::new("master-focus");
    let mut harness = open_demo(&folder.0);
    click(&mut harness, "Projekt");
    click_menu(&mut harness, "Projektstammdaten …");
    press_with(&mut harness, Modifiers::COMMAND, Key::A);
    type_text(&mut harness, "Neubau Verwaltung");
    click(&mut harness, "Vergabe & Termine");
    press_with(&mut harness, Modifiers::COMMAND, Key::A);
    type_text(&mut harness, "V-2026-1");
    click(&mut harness, "Adressen");
    press_with(&mut harness, Modifiers::COMMAND, Key::A);
    type_text(&mut harness, "Stadt Musterhausen");
    click(&mut harness, "Übernehmen");
    let info = project(&harness).info();
    assert_eq!(info.label, "Neubau Verwaltung");
    assert_eq!(info.award_number, "V-2026-1");
    assert_eq!(
        info.owner.name1, "Stadt Musterhausen",
        "Adressen: Cursor im Namen des Bauherrn"
    );
}

#[test]
fn tab_in_the_mask_dialog_goes_from_label_to_label() {
    let folder = TempDir::new("mask-tab");
    let mut harness = open_demo(&folder.0);
    menu_path(&mut harness, "LV", "Gliederung und OZ-Maske …");
    // Leere Bezeichnungen zeigt der Dialog als „Titel“, „Untertitel“.
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::TextInput
                && node.value().is_some_and(|value| value == "Titel")
        })
        .next()
        .expect("erste Bezeichnung")
        .click();
    settle(&mut harness);
    press(&mut harness, Key::Tab);
    press_with(&mut harness, Modifiers::COMMAND, Key::A);
    type_text(&mut harness, "Gruppe");
    click(&mut harness, "Übernehmen");
    let after = project(&harness).lv().level_labels().to_vec();
    assert_eq!(
        after.get(1).map(String::as_str),
        Some("Gruppe"),
        "{after:?}"
    );
    assert_ne!(
        after.first().map(String::as_str),
        Some("Gruppe"),
        "{after:?}"
    );
}

#[test]
fn a_bidder_named_like_a_bid_gets_it_with_its_address() {
    let folder = TempDir::new("bidder-link");
    let mut harness = open_demo(&folder.0);
    let linked: Vec<_> = project(&harness)
        .invitees()
        .iter()
        .filter_map(|invitee| invitee.bid)
        .collect();
    let bid = project(&harness)
        .bids()
        .iter()
        .find(|bid| !linked.contains(&bid.id()))
        .expect("Demo: ein freies Angebot")
        .clone();
    click_starting(&mut harness, "Angebote (");
    click(&mut harness, "Bieter hinzufügen");
    type_text(&mut harness, &bid.name().to_uppercase());
    click(&mut harness, "Angebot verknüpfen");
    let invitee = project(&harness).invitees().last().unwrap().clone();
    assert_eq!(invitee.bid, Some(bid.id()));
    assert!(!bid.bidder().city.is_empty(), "Demo-Angebot mit Ort");
    assert_eq!(
        invitee.address.city,
        bid.bidder().city,
        "Ort aus dem Angebot"
    );
}

/// Befund 1 (17.11.): the logo is chosen in a complete browser – places,
/// drives, typed path, preview – and the folder is offered next time.
#[test]
fn logo_is_chosen_in_the_file_browser_with_places_path_and_preview() {
    let folder = TempDir::new("logo");
    let logos = folder.0.join("Logos");
    std::fs::create_dir_all(logos.join("Kunde A")).unwrap();
    image::RgbImage::from_pixel(40, 16, image::Rgb([30, 80, 160]))
        .save(logos.join("Firmenlogo.png"))
        .unwrap();
    image::RgbImage::new(4, 4)
        .save(logos.join("rot.jpg"))
        .unwrap();
    std::fs::write(logos.join("notiz.txt"), "kein Bild").unwrap();
    let mut harness = open_demo(&folder.0);
    click(&mut harness, "Einstellungen");
    click_print_tab(&mut harness);
    click(&mut harness, "Datei wählen …");
    for place in ["Orte", "Persönlicher Ordner", "Laufwerke"] {
        assert!(shows(&harness, place), "„{place}“ fehlt im Dateidialog");
    }

    // Type the path into the path field and press Enter.
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::TextInput
                && node.label().as_deref() == Some("Pfad")
        })
        .next()
        .expect("Pfadfeld")
        .click();
    settle(&mut harness);
    press_with(&mut harness, Modifiers::COMMAND, Key::A);
    type_text(&mut harness, &logos.to_string_lossy());
    press(&mut harness, Key::Enter);
    assert!(shows(&harness, "Kunde A/"), "Unterordner sichtbar");
    assert!(shows(&harness, "Firmenlogo.png"));
    assert!(!shows(&harness, "notiz.txt"), "Filter PNG/JPEG");

    click(&mut harness, "Firmenlogo.png");
    assert!(shows(&harness, "40 × 16 Pixel"), "Vorschau des Bildes");
    click(&mut harness, "Öffnen");
    click(&mut harness, "Übernehmen");
    assert!(!dialog_open(&harness));
    let stored = launcher(&folder.0);
    assert_eq!(
        stored.preferences().logo_file,
        Some(logos.join("Firmenlogo.png"))
    );
    assert_eq!(
        stored.recent_file_folders().first(),
        Some(&logos),
        "Zuletzt benutzt"
    );
}

/// The list is fully usable with the keyboard: arrows, Enter, Backspace.
#[test]
fn file_browser_works_with_the_keyboard() {
    let folder = TempDir::new("logokeys");
    let logos = folder.0.join("Logos");
    std::fs::create_dir_all(logos.join("Kunde A")).unwrap();
    image::RgbImage::new(4, 4)
        .save(logos.join("a.png"))
        .unwrap();
    image::RgbImage::new(4, 4)
        .save(logos.join("b.png"))
        .unwrap();
    let mut harness = open_demo(&folder.0);
    click(&mut harness, "Einstellungen");
    click_print_tab(&mut harness);
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::TextInput
                && node.label().as_deref() == Some("Bilddatei (optional)")
        })
        .next()
        .expect("Logofeld")
        .click();
    settle(&mut harness);
    type_text(&mut harness, &logos.join("a.png").to_string_lossy());
    click(&mut harness, "Datei wählen …");
    // Opens in the logo's folder with a.png marked.
    assert!(shows(&harness, "Kunde A/"));
    press(&mut harness, Key::ArrowDown);
    press(&mut harness, Key::Enter);
    let stored = |harness: &UiHarness| {
        harness
            .query_all_by(|node| {
                node.role() == egui::accesskit::Role::TextInput
                    && node.label().as_deref() == Some("Bilddatei (optional)")
            })
            .next()
            .and_then(|node| node.value())
    };
    assert_eq!(
        stored(&harness),
        Some(logos.join("b.png").to_string_lossy().to_string())
    );

    // Backspace goes up, Enter on a folder opens it.
    click(&mut harness, "Datei wählen …");
    press(&mut harness, Key::Backspace);
    assert!(shows(&harness, "Logos/"), "eine Ebene höher");
    press(&mut harness, Key::ArrowDown);
    press(&mut harness, Key::Enter);
    assert!(shows(&harness, "Kunde A/"), "wieder im Ordner Logos");
    press(&mut harness, Key::Escape);
    assert!(
        stored(&harness).is_some(),
        "Esc schließt nur den Dateidialog"
    );
}

/// The tab „Drucken“ of the settings (the menu bar has a „Drucken“ too).
fn click_print_tab(harness: &mut UiHarness) {
    harness
        .get_all_by_label("Drucken")
        .last()
        .expect("Reiter Drucken")
        .click();
    settle(harness);
}

/// Befund 3 (17.11.): every address shows its role; Auftraggeber is its own
/// address („wie Bauherr“ unless unticked).
#[test]
fn master_data_addresses_show_their_roles_and_a_separate_client() {
    let folder = TempDir::new("adressen");
    let mut harness = open_demo(&folder.0);
    let owner = project(&harness).info().owner.name1.clone();
    click(&mut harness, "Projekt");
    click_menu(&mut harness, "Projektstammdaten …");
    click(&mut harness, "Adressen");
    for role in [
        "Bauherr",
        "Auftraggeber",
        "Aufsteller / Planer",
        "Baustelle",
    ] {
        assert!(shows(&harness, role), "Rolle „{role}“ fehlt in der Liste");
    }
    click(&mut harness, "Auftraggeber");
    assert!(
        shows(&harness, "Auftraggeber wie Bauherr"),
        "Schalter sichtbar"
    );
    click(&mut harness, "Auftraggeber wie Bauherr");
    fill_field(&mut harness, "Name", "Zentrale Vergabestelle");
    click(&mut harness, "Übernehmen");
    assert!(!dialog_open(&harness), "übernommen");
    let info = project(&harness).info();
    assert_eq!(info.owner.name1, owner, "Bauherr bleibt");
    assert_eq!(info.client().name1, "Zentrale Vergabestelle");
    assert_eq!(
        info.own_client.as_ref().map(|client| client.city.clone()),
        Some(info.owner.city.clone()),
        "Auftraggeber startet mit den Angaben des Bauherrn"
    );
}

fn field_value(harness: &UiHarness, label: &str) -> Option<String> {
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::TextInput
                && node.label().as_deref() == Some(label)
        })
        .next()
        .and_then(|node| node.value())
}

/// Befund 4 (17.11.): date fields take short input and offer a calendar.
#[test]
fn date_fields_complete_short_input_and_offer_a_calendar() {
    let folder = TempDir::new("datum");
    let mut harness = open_demo(&folder.0);
    click(&mut harness, "Projekt");
    click_menu(&mut harness, "Projektstammdaten …");
    click(&mut harness, "Vergabe & Termine");

    // By hand: „1.12.26“ is written out when the field is left.
    fill_field(&mut harness, "Abgabetermin", "1.12.26");
    assert!(
        !shows_part(&harness, "kein Datum"),
        "Kurzform gilt schon beim Tippen"
    );
    press(&mut harness, Key::Tab);
    assert_eq!(
        field_value(&harness, "Abgabetermin").as_deref(),
        Some("01.12.2026")
    );

    // Calendar: opens on the typed date, a click picks the day.
    fill_field(&mut harness, "Ausführungsbeginn", "3.3.27");
    press(&mut harness, Key::Tab);
    click(&mut harness, "Kalender: Ausführungsbeginn");
    for text in ["KW", "Mo", "So", "Heute", "März", "2027"] {
        assert!(shows(&harness, text), "Kalender zeigt „{text}“");
    }
    click(&mut harness, "15");
    assert_eq!(
        field_value(&harness, "Ausführungsbeginn").as_deref(),
        Some("15.03.2027")
    );
    assert!(!shows(&harness, "KW"), "Kalender nach der Wahl zu");

    // Keyboard: arrow right + Enter = next day; Esc closes only the calendar.
    click(&mut harness, "Kalender: Ausführungsbeginn");
    press(&mut harness, Key::ArrowRight);
    press(&mut harness, Key::Enter);
    assert_eq!(
        field_value(&harness, "Ausführungsbeginn").as_deref(),
        Some("16.03.2027")
    );
    click(&mut harness, "Kalender: Ausführungsbeginn");
    press(&mut harness, Key::Escape);
    assert!(!shows(&harness, "KW"), "Esc schließt den Kalender");
    assert!(dialog_open(&harness), "der Dialog bleibt offen");

    fill_field(&mut harness, "Ausführungsende", "31.12.27");
    press(&mut harness, Key::Tab);
    click(&mut harness, "Übernehmen");
    assert!(!dialog_open(&harness));
    let info = project(&harness).info();
    assert!(
        info.submission_deadline.starts_with("01.12.2026"),
        "{}",
        info.submission_deadline
    );
    assert_eq!(info.execution_start, "16.03.2027");
    assert_eq!(info.execution_end, "31.12.2027");
}
