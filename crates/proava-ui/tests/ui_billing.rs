//! Reiter „Abrechnung“: take over the contract, invoices, editing a
//! position by keyboard, takeoff window, undo.

mod common;

use common::*;
use proava_domain::{Decimal, Project};
use proava_ui::StartFile;

fn open(folder: &std::path::Path, project: &Project) -> UiHarness {
    let file = save(folder, project);
    let mut harness = harness(folder, Some(StartFile::Project(file)));
    press_with(&mut harness, Modifiers::COMMAND, Key::Num4);
    harness
}

/// Brings the invoice row into view (the list scrolls) and clicks it.
fn click_invoice(harness: &mut UiHarness, number: &str) {
    let label = format!("Rechnung {number}");
    harness.get_by_label(&label).scroll_to_me();
    settle(harness);
    click(harness, &label);
}

fn dec(text: &str) -> Decimal {
    text.parse().unwrap()
}

#[test]
fn contract_is_taken_over_from_the_awarded_bid() {
    let folder = TempDir::new("billing-contract");
    let mut demo = proava_ui::demo::demo_project();
    let bid = demo.bids()[1].id();
    demo.award_bid(bid, "01.12.2026").unwrap();
    let mut harness = open(&folder.0, &demo);
    assert!(
        shows_part(&harness, "Auftrag übernehmen"),
        "Knopf zum Übernehmen"
    );
    harness
        .query_all_by(|node| {
            node.label()
                .is_some_and(|label| label.starts_with("Auftrag übernehmen"))
        })
        .next()
        .unwrap()
        .click();
    settle(&mut harness);
    let contract = project(&harness)
        .contract()
        .expect("Auftrag übernommen")
        .clone();
    assert_eq!(contract.contractor, project(&harness).bids()[1].name());
    assert!(shows(&harness, "Neue Rechnung"));
    click(&mut harness, "Neue Rechnung");
    assert_eq!(project(&harness).billing().invoices().len(), 1);
    // Undo: the invoice goes, then the contract.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    settle(&mut harness);
    assert!(project(&harness).billing().invoices().is_empty());
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    settle(&mut harness);
    assert!(project(&harness).contract().is_none());
}

#[test]
fn checked_quantity_by_keyboard_and_release() {
    let folder = TempDir::new("billing-edit");
    let demo = proava_ui::demo::billing_demo_project();
    let mut harness = open(&folder.0, &demo);
    // The last invoice (Schlussrechnung) is selected; pick AR-2.
    click_invoice(&mut harness, "AR-2");
    let invoice = project(&harness).billing().invoices()[1].id;
    assert!(shows(&harness, "Rechenweg der Freigabe"));
    assert!(shows_part(&harness, "= Freigabe brutto"));

    // First position selected: Enter edits „geprüft“, Pfeil runter keeps the column.
    press(&mut harness, Key::Enter);
    type_text(&mut harness, "0");
    press(&mut harness, Key::ArrowDown);
    type_text(&mut harness, "1");
    press(&mut harness, Key::Enter);
    let billing = project(&harness).billing();
    let items: Vec<_> = billing.invoice(invoice).unwrap().items.values().collect();
    let checked: Vec<Decimal> = items
        .iter()
        .filter_map(|item| item.checked_quantity)
        .collect();
    assert!(
        checked.contains(&dec("0")) && checked.contains(&dec("1")),
        "{checked:?}"
    );

    // The release follows the check.
    let context = project(&harness).billing_context().unwrap();
    let check = project(&harness)
        .billing()
        .check(&context, invoice)
        .unwrap();
    assert!(shows(
        &harness,
        &format!("{} €", check.release.release_gross.plain())
    ));

    // Undo puts the quantities back.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    settle(&mut harness);
    let items = &project(&harness).billing().invoice(invoice).unwrap().items;
    assert!(items
        .values()
        .all(|item| item.checked_quantity.is_none() || item.note.contains("Modul")));
}

#[test]
fn takeoff_window_adds_lines() {
    let folder = TempDir::new("billing-takeoff");
    let demo = proava_ui::demo::billing_demo_project();
    let mut harness = open(&folder.0, &demo);
    let invoice = project(&harness).billing().invoices().last().unwrap().id;
    let row = project(&harness)
        .billing()
        .invoice(invoice)
        .unwrap()
        .items
        .keys()
        .next()
        .copied()
        .unwrap();
    press_with(&mut harness, Modifiers::COMMAND, Key::M);
    assert!(shows_part(&harness, "Aufmaß – "), "Fenster offen");
    let before = project(&harness).billing().takeoff_of(row).len();
    click(&mut harness, "Zeile hinzufügen");
    let lines = project(&harness).billing().takeoff_of(row);
    assert_eq!(lines.len(), before + 1);
    assert_eq!(lines.last().unwrap().invoice, Some(invoice));
    click(&mut harness, "Schließen");
    assert!(!shows_part(&harness, "Aufmaß – "));
}

#[test]
fn release_letter_and_pdfs_are_reached_from_the_tab() {
    let folder = TempDir::new("billing-letter");
    let demo = proava_ui::demo::billing_demo_project();
    let mut harness = open(&folder.0, &demo);
    click_invoice(&mut harness, "AR-2");
    click(&mut harness, "Anschreiben an den Bauherrn …");
    assert!(shows(&harness, "Schriftverkehr"));
    assert!(shows_part(
        &harness,
        "Empfänger (Auftraggeber, sonst Bauherr)"
    ));
    // Preview with the invoice chosen in the tab and its release.
    let invoice = project(&harness).billing().invoices()[1].id;
    let context = project(&harness).billing_context().unwrap();
    let check = project(&harness)
        .billing()
        .check(&context, invoice)
        .unwrap();
    assert!(shows_part(
        &harness,
        "Rechnungsfreigabe 2. Abschlagsrechnung AR-2"
    ));
    assert!(shows_part(
        &harness,
        &check.release.release_gross.to_string()
    ));
    click(&mut harness, "Schließen");

    click(&mut harness, "PDF Rechnungsfreigabe …");
    assert!(shows(&harness, "Drucken"));
    assert!(
        shows_part(&harness, "AR-2 – Abschlagsrechnung"),
        "Rechnung im Druckdialog vorgewählt"
    );
}

/// Picks `file` in the file dialog: path into „Pfad“, Enter, the file, „Öffnen“.
fn pick_file(harness: &mut UiHarness, file: &std::path::Path) {
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::TextInput
                && node.label().as_deref() == Some("Pfad")
        })
        .next()
        .expect("Pfadfeld")
        .click();
    settle(harness);
    press_with(harness, Modifiers::COMMAND, Key::A);
    type_text(harness, &file.parent().unwrap().to_string_lossy());
    press(harness, Key::Enter);
    click(harness, &file.file_name().unwrap().to_string_lossy());
    click(harness, "Öffnen");
}

#[test]
fn invoice_is_read_from_an_x89_and_undone() {
    let folder = TempDir::new("billing-x89");
    let demo = proava_ui::demo::billing_demo_project();
    let row = *demo.contract().unwrap().items.keys().next().unwrap();
    let ordinal = demo.lv().position_by_id(row).unwrap().ordinal().to_string();
    // Synthetic X89 (Zuwachs) for one position.
    let pieces: Vec<&str> = ordinal.split('.').collect();
    let (levels, last) = pieces.split_at(pieces.len() - 1);
    let mut body = format!(
        "<Itemlist><Item RNoPart=\"{}\"><BillQty>1</BillQty><QtyDeterm><Qty>1</Qty></QtyDeterm></Item></Itemlist>",
        last[0]
    );
    for level in levels.iter().rev() {
        body = format!("<BoQCtgy RNoPart=\"{level}\"><BoQBody>{body}</BoQBody></BoQCtgy>");
    }
    let x89 = format!(
        "<GAEB><Invoice><DP>89</DP><BoQ><BoQBody>{body}</BoQBody></BoQ><InvoiceHeader><InvoiceNo>NR-77</InvoiceNo>\
         <InvoiceDate>2027-12-15</InvoiceDate><InvoiceType>single invoice</InvoiceType>\
         <SettlementType>periodic</SettlementType></InvoiceHeader></Invoice></GAEB>"
    );
    let gaeb = folder.0.join("GAEB");
    std::fs::create_dir_all(&gaeb).unwrap();
    let file = gaeb.join("Rechnung.X89");
    std::fs::write(&file, x89).unwrap();

    let mut harness = open(&folder.0, &demo);
    let before = project(&harness).billing().invoices().len();
    click(&mut harness, "Rechnung aus GAEB (X89/X31) …");
    pick_file(&mut harness, &file);
    for _ in 0..400 {
        if project(&harness).billing().invoices().len() > before {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        harness.step();
    }
    settle(&mut harness);
    let billing = project(&harness).billing();
    assert_eq!(billing.invoices().len(), before + 1, "Rechnung eingelesen");
    let invoice = billing.invoices().last().unwrap();
    assert_eq!(invoice.number, "NR-77");
    assert_eq!(invoice.kind, proava_domain::InvoiceKind::Single);
    assert_eq!(invoice.date, "15.12.2027");
    let id = invoice.id;
    assert!(billing
        .takeoff_of(row)
        .iter()
        .any(|line| line.invoice == Some(id) && line.formula == "1"));
    assert!(
        shows_part(&harness, "Rechnungskopf – "),
        "neue Rechnung gewählt"
    );

    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    settle(&mut harness);
    assert_eq!(
        project(&harness).billing().invoices().len(),
        before,
        "Rückgängig in einem Schritt"
    );
    assert!(!project(&harness)
        .billing()
        .takeoff_of(row)
        .iter()
        .any(|line| line.invoice == Some(id)));
}
