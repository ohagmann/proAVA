//! Schriftverkehr-Archiv: letters filed in the project („Im Projekt
//! ablegen“), the list „Schriftverkehr im Projekt“ and Rückgängig.

mod common;

use common::*;
use egui::{Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};
use proava_domain::{Address, CorrespondenceKind, InvitedBidder, LetterKind, Project};

fn project_with_bidders() -> Project {
    let mut project = proava_ui::demo::demo_project();
    for name in ["Müller GmbH", "Schmidt KG"] {
        project.add_invitee(InvitedBidder::new(Address::named(name)));
    }
    project
}

/// The project opened on the Angebote tab.
fn open(folder: &std::path::Path, project: &Project) -> UiHarness {
    let file = save(folder, project);
    let mut harness = harness(folder, Some(proava_ui::StartFile::Project(file)));
    press_with(&mut harness, Modifiers::COMMAND, Key::Num2);
    harness
}

/// Steps until `done` holds (the letters are written in the background).
fn wait(harness: &mut UiHarness, done: impl Fn(&UiHarness) -> bool) {
    for _ in 0..600 {
        if done(harness) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        harness.step();
    }
    settle(harness);
}

fn last(harness: &mut UiHarness, label: &str) {
    harness.get_all_by_label(label).last().expect(label).click();
    settle(harness);
}

fn toggled(harness: &UiHarness, label: &str) -> Option<egui::accesskit::Toggled> {
    harness
        .query_all_by(|node| node.label().as_deref() == Some(label))
        .next()
        .expect(label)
        .accesskit_node()
        .toggled()
}

fn pdfs_in(folder: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(folder)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".pdf"))
        .collect()
}

#[test]
fn collective_pdf_is_filed_per_bidder_and_undone_in_one_step() {
    let folder = TempDir::new("archiv-sammel");
    let mut harness = open(&folder.0, &project_with_bidders());
    click(&mut harness, "Schriftverkehr …");
    click(&mut harness, "PDF erzeugen (2) …");
    // The print dialog: Stil and Ausgabe, „Im Projekt ablegen“ on by default.
    assert!(
        shows(&harness, "Schreiben (Schriftverkehr)"),
        "Druckdialog der Schreiben"
    );
    assert!(
        shows(&harness, "Klassisch") && shows(&harness, "proAVA"),
        "Stil oben"
    );
    assert_eq!(
        toggled(&harness, "Im Projekt ablegen"),
        Some(egui::accesskit::Toggled::True),
        "Vorgabe an"
    );
    press(&mut harness, Key::Enter);
    // Save dialog with the suggested name in the project folder.
    last(&mut harness, "Speichern");
    wait(&mut harness, |harness| {
        project(harness).correspondence().len() == 2
    });

    let archive = project(&harness).correspondence().to_vec();
    assert_eq!(
        archive.len(),
        2,
        "je Bieter ein Eintrag, auch beim Sammel-PDF"
    );
    assert_eq!(
        pdfs_in(&folder.0).len(),
        1,
        "nur das Sammel-PDF liegt im Ordner"
    );
    assert_eq!(
        archive[0].kind,
        CorrespondenceKind::Letter(LetterKind::Invitation)
    );
    assert_eq!(archive[0].recipient_name, "Müller GmbH");
    assert_eq!(archive[1].recipient_name, "Schmidt KG");
    assert!(archive.iter().all(|entry| entry.pdf.starts_with(b"%PDF")));
    let invitees = project(&harness).invitees().to_vec();
    assert!(
        invitees.iter().all(|invitee| !invitee.requested.is_empty()),
        "Versanddatum wie bisher"
    );
    assert!(shows_part(&harness, "2 Schreiben im Projekt abgelegt"));

    // One Strg+Z: Versanddatum and archive together.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    settle(&mut harness);
    assert!(project(&harness).correspondence().is_empty());
    assert!(project(&harness)
        .invitees()
        .iter()
        .all(|invitee| invitee.requested.is_empty()));
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Y);
    settle(&mut harness);
    assert_eq!(project(&harness).correspondence().len(), 2);

    // Saved and opened again: the archive is in the project file.
    press_with(&mut harness, Modifiers::COMMAND, Key::S);
    let file = folder
        .0
        .join(format!("{}.proava", project(&harness).name().as_str()));
    let loaded = proava_application::ProjectRepository::load_project(
        &proava_infrastructure::XmlProjectFile::new(),
        &file,
    )
    .unwrap();
    assert_eq!(loaded.correspondence(), project(&harness).correspondence());
}

#[test]
fn without_filing_nothing_goes_into_the_project() {
    let folder = TempDir::new("archiv-aus");
    let mut harness = open(&folder.0, &project_with_bidders());
    click(&mut harness, "Schriftverkehr …");
    click(&mut harness, "PDF erzeugen (2) …");
    click(&mut harness, "Im Projekt ablegen");
    assert_eq!(
        toggled(&harness, "Im Projekt ablegen"),
        Some(egui::accesskit::Toggled::False)
    );
    last(&mut harness, "PDF speichern …");
    last(&mut harness, "Speichern");
    wait(&mut harness, |harness| {
        !project(harness).invitees()[0].requested.is_empty()
    });
    assert!(project(&harness).correspondence().is_empty());
    assert_eq!(pdfs_in(&folder.0).len(), 1);
}

#[test]
fn list_opens_saves_removes_and_undoes() {
    let folder = TempDir::new("archiv-liste");
    let mut project = project_with_bidders();
    let ids: Vec<_> = project
        .invitees()
        .iter()
        .map(|invitee| invitee.id)
        .collect();
    for (kind, id, date) in [
        (LetterKind::Invitation, ids[0], "02.11.2026"),
        (LetterKind::Rejection, ids[1], "18.11.2026"),
    ] {
        let entry =
            proava_domain::bidder_letter_entry(&project, kind, id, date, b"%PDF-1.7 test".to_vec())
                .unwrap();
        project.add_correspondence(entry);
    }
    let mut harness = open(&folder.0, &project);
    click(&mut harness, "Abgelegt (2) …");
    assert!(shows(&harness, "Schriftverkehr im Projekt"));
    let rejection = "Absage an Schmidt KG vom 18.11.2026";
    let invitation = "Aufforderung an Müller GmbH vom 02.11.2026";
    assert!(shows(&harness, rejection) && shows(&harness, invitation));
    // Newest first.
    let top = |harness: &UiHarness, label: &str| harness.get_by_label(label).rect().top();
    assert!(top(&harness, rejection) < top(&harness, invitation));

    // Filter by kind and by recipient.
    let combo = |harness: &mut UiHarness, label: &str| {
        harness
            .query_all_by(|node| {
                node.role() == egui::accesskit::Role::ComboBox
                    && node.label().as_deref() == Some(label)
            })
            .next()
            .expect(label)
            .click();
        settle(harness);
    };
    combo(&mut harness, "Art");
    click(&mut harness, "Absage");
    assert!(shows(&harness, rejection) && !shows(&harness, invitation));
    combo(&mut harness, "Art");
    click(&mut harness, "Alle Arten");
    combo(&mut harness, "Empfänger");
    // The name is also in the Bieterverzeichnis behind the list: the last one is in the popup.
    last(&mut harness, "Müller GmbH");
    assert!(!shows(&harness, rejection) && shows(&harness, invitation));
    combo(&mut harness, "Empfänger");
    click(&mut harness, "Alle Empfänger");
    assert!(shows(&harness, rejection) && shows(&harness, invitation));

    // Open: a temporary copy (never a viewer in tests).
    // egui counts a double click less than 0,3 s after another click (here
    // the filter) as a triple click: let the time pass first.
    harness.run_steps(30);
    double_click(&mut harness, rejection);
    // In the test's own folder (common::harness), removed with it.
    let copy = folder
        .0
        .join("pdf-anzeige")
        .join("0002_Absage_Schmidt_KG.pdf");
    assert_eq!(std::fs::read(&copy).unwrap(), b"%PDF-1.7 test");

    // Speichern unter … into the project folder (suggested name).
    click(&mut harness, rejection);
    click(&mut harness, "Speichern unter …");
    last(&mut harness, "Speichern");
    assert_eq!(
        std::fs::read(folder.0.join("0002_Absage_Schmidt_KG.pdf")).unwrap(),
        b"%PDF-1.7 test"
    );
    assert!(
        shows(&harness, "Schriftverkehr im Projekt"),
        "Liste wieder da"
    );

    // Bemerkung, then Entfernen with question; Strg+Z in the list.
    click(&mut harness, rejection);
    harness
        .query_all_by(|node| node.role() == egui::accesskit::Role::TextInput)
        .last()
        .expect("Bemerkung")
        .click();
    settle(&mut harness);
    type_text(&mut harness, "per Mail");
    assert_eq!(
        common::project(&harness).correspondence()[1].remark,
        "per Mail"
    );
    // The Bieterverzeichnis behind the dialog has its own „Entfernen …“.
    last(&mut harness, "Entfernen …");
    assert!(shows_part(
        &harness,
        "Absage an Schmidt KG vom 18.11.2026 mit dem PDF aus dem Projekt entfernen?"
    ));
    assert_eq!(
        common::project(&harness).correspondence().len(),
        2,
        "erst nach der Rückfrage"
    );
    last(&mut harness, "Entfernen");
    assert_eq!(common::project(&harness).correspondence().len(), 1);
    assert!(!shows(&harness, rejection));
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    settle(&mut harness);
    assert_eq!(
        common::project(&harness).correspondence().len(),
        2,
        "Entfernen rückgängig"
    );
    assert_eq!(
        common::project(&harness).correspondence()[1].remark,
        "per Mail"
    );
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    settle(&mut harness);
    assert_eq!(
        common::project(&harness).correspondence()[1].remark,
        "",
        "Bemerkung rückgängig"
    );
    assert!(
        shows(&harness, "Schriftverkehr im Projekt"),
        "Liste bleibt offen"
    );
}

#[test]
fn own_pdf_is_filed_from_a_file() {
    let folder = TempDir::new("archiv-eigen");
    let own = folder.0.join("Protokoll_Bietergespraech.pdf");
    std::fs::write(&own, b"%PDF-1.4 eigen").unwrap();
    let mut harness = open(&folder.0, &project_with_bidders());
    click(&mut harness, "Abgelegt (0) …");
    assert!(shows_part(&harness, "Noch nichts abgelegt"));
    click(&mut harness, "PDF hinzufügen …");
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
    type_text(&mut harness, &folder.0.to_string_lossy());
    press(&mut harness, Key::Enter);
    click(&mut harness, "Protokoll_Bietergespraech.pdf");
    last(&mut harness, "Öffnen");
    let archive = project(&harness).correspondence();
    assert_eq!(archive.len(), 1);
    assert_eq!(archive[0].kind, CorrespondenceKind::Own);
    assert_eq!(archive[0].subject, "Protokoll Bietergespraech");
    assert_eq!(archive[0].file, "Protokoll_Bietergespraech.pdf");
    assert!(shows(&harness, "Schriftverkehr im Projekt"));
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    settle(&mut harness);
    assert!(project(&harness).correspondence().is_empty());
}
