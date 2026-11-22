//! Example project for the Nachträge: the demo LV with the first bid awarded,
//! the contract taken over and six invented change orders in different
//! states (announced … approved, rejected) – new positions in the
//! Nachtragsbereich, Mehr-/Mindermengen, checked prices with notes, one
//! rejected item. Everything is made up.

use proava_domain::{
    ChangeBasis, ChangeOrder, ChangeOrderId, ChangeOrderStatus, Decimal, ItemChange, Position,
    Project, RichText, RowId,
};

fn dec(text: &str) -> Decimal {
    text.parse().expect("demo number")
}

fn row_of(project: &Project, short_text: &str) -> RowId {
    project
        .lv()
        .positions()
        .find(|position| position.short_text() == short_text)
        .map(Position::id)
        .expect("demo position")
}

/// New position with long text, offered price; checked price and note optional.
#[allow(clippy::too_many_arguments)]
fn new_position(
    project: &mut Project,
    id: ChangeOrderId,
    text: (&str, &str),
    quantity: &str,
    unit: &str,
    offered: &str,
    checked: Option<&str>,
    note: &str,
) -> RowId {
    let mut position = Position::new("", text.0, dec(quantity), unit, None);
    position.set_long_text(RichText::plain(text.1));
    if unit == "psch" {
        position.set_lump_sum(true);
    }
    if unit == "h" {
        position.set_hourly(true);
    }
    let row = project
        .add_change_order_position(id, position, Some(dec(offered)))
        .expect("demo position fits");
    if let Some(item) = project
        .change_order_mut(id)
        .and_then(|order| order.item_mut(row))
    {
        item.checked_unit_price = checked.map(dec);
        item.note = note.to_string();
    }
    row
}

fn quantity_change(
    project: &mut Project,
    id: ChangeOrderId,
    short_text: &str,
    quantity: &str,
    note: &str,
) {
    let row = row_of(project, short_text);
    project
        .assign_to_change_order(id, row)
        .expect("contract position");
    if let Some(item) = project
        .change_order_mut(id)
        .and_then(|order| order.item_mut(row))
    {
        item.offered_quantity = dec(quantity);
        item.note = note.to_string();
    }
}

fn order(
    project: &mut Project,
    title: &str,
    basis: ChangeBasis,
    reason: &str,
    announced: &str,
) -> ChangeOrderId {
    let mut order = ChangeOrder::new(ChangeOrderId(0), 0, title);
    order.basis = basis;
    order.reason = reason.to_string();
    order.announced_date = announced.to_string();
    project.add_change_order(order)
}

fn steps(project: &mut Project, id: ChangeOrderId, steps: &[(ChangeOrderStatus, &str, &str)]) {
    if let Some(order) = project.change_order_mut(id) {
        for (status, date, note) in steps {
            order.set_status(*status, date, note);
        }
    }
}

/// The demo project with contract and Nachträge N01–N06.
pub fn change_order_demo_project() -> Project {
    let mut project = crate::demo::demo_project();
    let bid = project.bids()[0].id();
    project.award_bid(bid, "15.09.2026").expect("demo award");
    project.take_over_contract().expect("demo contract");
    if let Some(contract) = project.contract_mut() {
        contract.number = "AG-2026-031".to_string();
    }

    // N01 beauftragt: Wetterstation mit Mast.
    let n01 = order(
        &mut project,
        "Wetterstation auf dem Dach",
        ChangeBasis::AdditionalPerformance,
        "Bauherrenwunsch zur Ausführungszeit: Wind und Regen für die Sonnenschutzsteuerung.",
        "05.10.2026",
    );
    new_position(
        &mut project,
        n01,
        ("Wetterstation mit Wind, Regen, Helligkeit, BACnet", "Kompakte Wetterstation zur Dachmontage, Messwerte Wind, Regen, Helligkeit und Außentemperatur, Anbindung BACnet/IP."),
        "1",
        "St",
        "1947",
        Some("1697"),
        "EK laut Lieferantenangebot 1.350 €, Zuschlag 25 % angemessen",
    );
    new_position(
        &mut project,
        n01,
        (
            "Mast 2 m für Wetterstation",
            "Mast aus verzinktem Stahl, 2 m, mit Dachhalterung.",
        ),
        "1",
        "St",
        "350",
        None,
        "in Ordnung",
    );
    if let Some(order) = project.change_order_mut(n01) {
        order.offer_number = "NA-2026-118".into();
        order.checked_by = "O. Planer".into();
        order.check_note = "Fachtechnisch und rechnerisch geprüft.".into();
        order.recommendation = "Das Angebot kann in der geprüften Höhe beauftragt werden.".into();
    }
    steps(
        &mut project,
        n01,
        &[
            (ChangeOrderStatus::Offered, "12.10.2026", "Angebot per Mail"),
            (ChangeOrderStatus::Reviewed, "20.10.2026", ""),
            (
                ChangeOrderStatus::Approved,
                "28.10.2026",
                "Schreiben des Bauherrn",
            ),
        ],
    );

    // N02 geprüft: Mehr- und Mindermengen von Auftragspositionen.
    let n02 = order(
        &mut project,
        "Zusätzliche CO2-Fühler Seminarräume",
        ChangeBasis::QuantityChange,
        "Nutzungsänderung zweier Räume zu Seminarräumen.",
        "14.10.2026",
    );
    quantity_change(
        &mut project,
        n02,
        "CO2-Fühler Raum, 0–10 V",
        "6",
        "Raumliste Architekt geprüft",
    );
    quantity_change(
        &mut project,
        n02,
        "Kanaltemperaturfühler Pt1000",
        "-2",
        "entfallen mit RLT03",
    );
    steps(
        &mut project,
        n02,
        &[
            (ChangeOrderStatus::Offered, "21.10.2026", ""),
            (
                ChangeOrderStatus::Reviewed,
                "03.11.2026",
                "zur Beauftragung empfohlen",
            ),
        ],
    );
    if let Some(order) = project.change_order_mut(n02) {
        order.recommendation = "Mehrmengen zu Auftragspreisen, beauftragen.".into();
    }

    // N03 in Prüfung: Aufschaltung mit einer abgelehnten Position.
    let n03 = order(
        &mut project,
        "Aufschaltung Lüftung Werkstatt",
        ChangeBasis::ChangedPerformance,
        "Position fehlte in der Ausschreibung: Lüftungsanlage Werkstatt wird aufgeschaltet.",
        "22.10.2026",
    );
    new_position(
        &mut project,
        n03,
        (
            "IO-Erweiterung ASP02",
            "Erweiterung der Automationsstation um 8 DI, 4 AI, 4 AO inklusive Verdrahtung.",
        ),
        "1",
        "St",
        "1280",
        Some("1150"),
        "Module zu Auftragspreisen, Verdrahtung 2 h",
    );
    new_position(
        &mut project,
        n03,
        (
            "Inbetriebnahme Werkstatt",
            "Inbetriebnahme und Datenpunkttest der Lüftungsanlage Werkstatt.",
        ),
        "8",
        "h",
        "95",
        Some("85"),
        "Stundensatz laut Auftrag",
    );
    let weekend = new_position(
        &mut project,
        n03,
        (
            "Zuschlag Wochenendarbeit",
            "Zuschlag für Arbeiten am Wochenende.",
        ),
        "1",
        "psch",
        "450",
        None,
        "nicht angeordnet",
    );
    if let Some(item) = project
        .change_order_mut(n03)
        .and_then(|order| order.item_mut(weekend))
    {
        item.status = Some(ChangeOrderStatus::Rejected);
    }
    steps(
        &mut project,
        n03,
        &[
            (ChangeOrderStatus::Offered, "30.10.2026", ""),
            (
                ChangeOrderStatus::InReview,
                "04.11.2026",
                "Rückfrage Stundenansatz",
            ),
        ],
    );

    // N04 angeboten: Beschleunigung, noch ungeprüft.
    let n04 = order(
        &mut project,
        "Beschleunigung Inbetriebnahme",
        ChangeBasis::Hindrance,
        "Verkürzte Inbetriebnahme wegen verschobenem Übergabetermin.",
        "02.11.2026",
    );
    new_position(
        &mut project,
        n04,
        (
            "Mehraufwand Beschleunigung",
            "Zusätzliches Personal für die verkürzte Inbetriebnahme.",
        ),
        "1",
        "psch",
        "2400",
        None,
        "",
    );
    steps(
        &mut project,
        n04,
        &[(ChangeOrderStatus::Offered, "09.11.2026", "")],
    );

    // N05 abgelehnt.
    let n05 = order(
        &mut project,
        "Touchpanel statt Bedientableau",
        ChangeBasis::Other,
        "Vorschlag der Firma, nicht vom Bauherrn gewünscht.",
        "01.10.2026",
    );
    new_position(
        &mut project,
        n05,
        (
            "Touchpanel 15 Zoll",
            "Touchpanel 15 Zoll anstelle Bedien- und Anzeigeeinheit.",
        ),
        "1",
        "St",
        "3100",
        None,
        "",
    );
    quantity_change(
        &mut project,
        n05,
        "Bedien- und Anzeigeeinheit 7\"",
        "-1",
        "entfällt bei Touchpanel",
    );
    steps(
        &mut project,
        n05,
        &[
            (ChangeOrderStatus::Offered, "06.10.2026", ""),
            (
                ChangeOrderStatus::Rejected,
                "15.10.2026",
                "Bauherr lehnt ab",
            ),
        ],
    );

    // N06 angekündigt, noch ohne Angebot.
    let n06 = order(
        &mut project,
        "Mehrkosten Kabelwege Bauteil B",
        ChangeBasis::ChangedPerformance,
        "Geänderte Trassenführung laut Ausführungsplanung Elektro.",
        "12.11.2026",
    );
    if let Some(order) = project.change_order_mut(n06) {
        order.offer_due = "26.11.2026".into();
        order.history.push(proava_domain::StatusChange {
            status: ChangeOrderStatus::Announced,
            date: "12.11.2026".into(),
            note: "Anzeige der Firma".into(),
        });
    }
    debug_assert!(project
        .change_orders()
        .iter()
        .flat_map(|order| &order.items)
        .any(|item| item.change == ItemChange::Quantity));
    project
}

#[cfg(test)]
mod tests {
    use proava_domain::{change_order_overview, ChangeOrderStatus, Money};

    use super::change_order_demo_project;

    #[test]
    fn six_change_orders_in_different_states() {
        let project = change_order_demo_project();
        let states: Vec<ChangeOrderStatus> = project
            .change_orders()
            .iter()
            .map(|order| order.status)
            .collect();
        assert_eq!(
            states,
            [
                ChangeOrderStatus::Approved,
                ChangeOrderStatus::Reviewed,
                ChangeOrderStatus::InReview,
                ChangeOrderStatus::Offered,
                ChangeOrderStatus::Rejected,
                ChangeOrderStatus::Announced,
            ]
        );
        let overview = change_order_overview(project.contract(), project.change_orders());
        // N01 beauftragt: 1.697 + 350 = 2.047 € netto (Nachlass gilt, Angebot 1 hat keinen).
        assert_eq!(overview.lines[0].approved, Money::from_cents(204_700));
        assert!(overview.forecast > overview.total_with_change_orders);
        for order in project.change_orders() {
            assert!(
                order.problems(project.contract(), project.lv()).is_empty(),
                "{}",
                order.label()
            );
        }
    }
}
