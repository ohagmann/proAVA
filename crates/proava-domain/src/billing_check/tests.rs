//! Worked examples of the release calculation. All figures are synthetic
//! and computed by hand in the comments.

use std::collections::BTreeMap;

use rust_decimal::Decimal;

use crate::billing::{
    Billing, CheckBasis, Correction, Invoice, InvoiceId, InvoiceItem, InvoiceKind, Mode, Progress,
    TakeoffLine, VatRule,
};
use crate::billing_check::{BillingContext, Finding, InvoiceCheck};
use crate::change_order::{
    ChangeOrder, ChangeOrderId, ChangeOrderItem, ChangeOrderStatus, ItemChange,
};
use crate::contract::{Contract, ContractItem};
use crate::lv::{Lv, LvGroup, RowId};
use crate::money::Money;
use crate::position::{Position, PositionKind};
use crate::project::{ContractTerms, Deduction};
use crate::OzMask;

fn dec(text: &str) -> Decimal {
    text.parse().unwrap()
}

fn eur(cents: i64) -> Money {
    Money::from_cents(cents)
}

struct Setup {
    lv: Lv,
    contract: Contract,
    orders: Vec<ChangeOrder>,
    /// A: 100 m à 2,50; B: 10 St à 150; H: 20 h à 60 (Stundenlohn);
    /// L: 1 psch à 250.000; P: Bedarfsposition 5 St à 10;
    /// N, X: rows outside the contract (Nachtrag, ohne Auftrag).
    a: RowId,
    b: RowId,
    h: RowId,
    l: RowId,
    p: RowId,
    n: RowId,
    x: RowId,
}

fn setup() -> Setup {
    let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "Titel"));
    let a = lv.push_position(Position::new("01.0010", "Kabel", dec("100"), "m", None));
    let b = lv.push_position(Position::new("01.0020", "Gerät", dec("10"), "St", None));
    let h = lv.push_position(Position::new("01.0030", "Stunden", dec("20"), "h", None));
    let l = lv.push_position(Position::new(
        "01.0040",
        "Pauschale",
        dec("1"),
        "psch",
        None,
    ));
    let p = lv.push_position(Position::new("01.0050", "Bedarf", dec("5"), "St", None));
    lv.push_group(LvGroup::new(0, "02", "Nachtrag"));
    let n = lv.push_position(Position::new("02.0010", "Mast", dec("1"), "St", None));
    let x = lv.push_position(Position::new(
        "02.0020",
        "Ohne Auftrag",
        dec("1"),
        "St",
        None,
    ));
    let item = |ordinal: &str, unit: &str, quantity: &str, price: &str| ContractItem {
        ordinal: ordinal.into(),
        short_text: ordinal.into(),
        unit: unit.into(),
        quantity: dec(quantity),
        unit_price: dec(price),
        kind: PositionKind::Normal,
        lump_sum: false,
        hourly: false,
    };
    let items = BTreeMap::from([
        (a, item("01.0010", "m", "100", "2.50")),
        (b, item("01.0020", "St", "10", "150")),
        (
            h,
            ContractItem {
                hourly: true,
                ..item("01.0030", "h", "20", "60")
            },
        ),
        (
            l,
            ContractItem {
                lump_sum: true,
                ..item("01.0040", "psch", "1", "250000")
            },
        ),
        (
            p,
            ContractItem {
                kind: PositionKind::Provisional { with_total: false },
                ..item("01.0050", "St", "5", "10")
            },
        ),
    ]);
    let contract = Contract {
        contractor: "Firma".into(),
        items,
        total: eur(25_295_000),
        ..Contract::default()
    };
    Setup {
        lv,
        contract,
        orders: Vec::new(),
        a,
        b,
        h,
        l,
        p,
        n,
        x,
    }
}

impl Setup {
    fn context(&self) -> BillingContext<'_> {
        BillingContext {
            contract: &self.contract,
            change_orders: &self.orders,
            lv: &self.lv,
        }
    }

    fn check(&self, billing: &Billing, id: InvoiceId) -> InvoiceCheck {
        billing.check(&self.context(), id).unwrap()
    }
}

fn claim(quantity: &str) -> InvoiceItem {
    InvoiceItem {
        claimed_quantity: Some(dec(quantity)),
        ..InvoiceItem::default()
    }
}

fn invoice(kind: InvoiceKind, number: &str, quantities: Mode, release: Mode) -> Invoice {
    Invoice {
        quantities,
        release,
        vat: VatRule::ReverseCharge,
        ..Invoice::new(kind, number, "")
    }
}

fn has(check: &InvoiceCheck, position: RowId, wanted: impl Fn(&Finding) -> bool) -> bool {
    check
        .positions
        .iter()
        .find(|line| line.position == position)
        .is_some_and(|line| line.findings.iter().any(wanted))
}

/// Increment invoices of the contractor, released cumulatively with credit
/// of earlier releases (like a client's form „Abschlagszahlung“).
#[test]
fn cumulative_release_of_increment_invoices() {
    let mut s = setup();
    s.contract.terms.performance_security_percent = Some(dec("10"));
    let mut billing = Billing::default();
    let mut first = invoice(
        InvoiceKind::Partial,
        "R1",
        Mode::Increment,
        Mode::Cumulative,
    );
    first.items.insert(s.a, claim("40"));
    first.items.insert(s.b, claim("4"));
    let first = billing.add_invoice(first);
    let mut second = invoice(
        InvoiceKind::Partial,
        "R2",
        Mode::Increment,
        Mode::Cumulative,
    );
    second.items.insert(s.a, claim("30"));
    second.items.insert(
        s.b,
        InvoiceItem {
            checked_quantity: Some(dec("4")),
            ..claim("5")
        },
    );
    let second = billing.add_invoice(second);

    // R1: 40 × 2,50 + 4 × 150 = 700; − 10 % = 630.
    let check = s.check(&billing, first);
    assert_eq!(check.label, "1. Abschlagsrechnung");
    assert_eq!(check.release.performance, eur(70_000));
    assert_eq!(check.release.security, eur(7_000));
    assert_eq!(check.release.release_net, eur(63_000));
    assert_eq!(check.release.release_gross, eur(63_000), "§ 13b: ohne USt");
    assert!(check.findings.contains(&Finding::ReverseCharge));
    assert!(has(&check, s.a, |f| *f == Finding::Unchecked));

    // R2: cumulative 70 m = 175, 8 St = 1.200 → 1.375; − 137,50 = 1.237,50;
    // − 630 credited = 607,50.
    let check = s.check(&billing, second);
    assert_eq!(check.label, "2. Abschlagsrechnung");
    assert_eq!(check.release.performance, eur(137_500));
    assert_eq!(check.release.after_security, eur(123_750));
    assert_eq!(check.release.credited, eur(63_000));
    assert_eq!(check.release.release_net, eur(60_750));
    assert_eq!(check.release.released_total, eur(123_750));
    assert_eq!(check.release.withheld, eur(13_750));
    assert_eq!(check.earlier.len(), 1);
    assert_eq!(check.earlier[0].release_net, eur(63_000));
    let b = check
        .positions
        .iter()
        .find(|line| line.position == s.b)
        .unwrap();
    assert_eq!(
        (b.checked, b.previous, b.cumulative),
        (dec("4"), dec("4"), dec("8"))
    );
    assert!(b.findings.contains(&Finding::QuantityReduced {
        claimed: dec("5"),
        checked: dec("4")
    }));
    assert_eq!(check.performed, eur(137_500));
}

/// Increment releases with deductions, then the final invoice with the
/// warranty retention, then the release of the retention.
#[test]
fn increment_releases_final_invoice_and_security_release() {
    let mut s = setup();
    s.contract.terms = ContractTerms {
        performance_security_percent: Some(dec("10")),
        warranty_security_percent: Some(dec("5")),
        deductions: vec![
            Deduction {
                label: "Bauumlage".into(),
                percent: dec("0.9"),
            },
            Deduction {
                label: "Projektrabatt".into(),
                percent: dec("1.1"),
            },
        ],
        ..ContractTerms::default()
    };
    let mut billing = Billing::default();
    let mut tr1 = invoice(InvoiceKind::Partial, "T1", Mode::Increment, Mode::Increment);
    tr1.items.insert(s.a, claim("40"));
    tr1.items.insert(s.b, claim("4"));
    let tr1 = billing.add_invoice(tr1);
    let mut tr2 = invoice(InvoiceKind::Partial, "T2", Mode::Increment, Mode::Increment);
    tr2.items.insert(s.a, claim("60"));
    tr2.items.insert(s.b, claim("6"));
    let tr2 = billing.add_invoice(tr2);
    let mut sr = invoice(InvoiceKind::Final, "S", Mode::Increment, Mode::Increment);
    sr.items.insert(s.a, claim("100"));
    sr.items.insert(s.b, claim("10"));
    let sr = billing.add_invoice(sr);
    let release = billing.add_invoice(invoice(
        InvoiceKind::SecurityRelease,
        "SF",
        Mode::Cumulative,
        Mode::Cumulative,
    ));

    // T1: 700 − 6,30 − 7,70 = 686; − 68,60 = 617,40.
    let check = s.check(&billing, tr1);
    assert_eq!(check.release.deductions.len(), 2);
    assert_eq!(check.release.deductions[0].2, eur(630));
    assert_eq!(check.release.deductions[1].2, eur(770));
    assert_eq!(check.release.net, eur(68_600));
    assert_eq!(check.release.release_net, eur(61_740));
    // T2: increment 150 + 900 = 1.050 − 9,45 − 11,55 = 1.029; − 102,90 = 926,10.
    let check = s.check(&billing, tr2);
    assert_eq!(check.release.performance, eur(105_000));
    assert_eq!(check.release.release_net, eur(92_610));
    assert_eq!(check.release.credited, Money::ZERO);
    // Final: always cumulative: 1.750 − 15,75 − 19,25 = 1.715; − 5 % 85,75 =
    // 1.629,25; − 1.543,50 credited = 85,75.
    let check = s.check(&billing, sr);
    assert_eq!(check.label, "Schlussrechnung");
    assert_eq!(check.release.mode, Mode::Cumulative);
    assert_eq!(check.release.net, eur(171_500));
    assert_eq!(check.release.security_percent, dec("5"));
    assert_eq!(check.release.security, eur(8_575));
    assert_eq!(check.release.credited, eur(154_350));
    assert_eq!(check.release.release_net, eur(8_575));
    assert_eq!(check.release.released_total, eur(162_925));
    assert_eq!(check.release.withheld, eur(8_575));
    assert_eq!(check.earlier.len(), 2);
    // Release of the retention: all that is withheld.
    let check = s.check(&billing, release);
    assert_eq!(check.release.release_net, eur(8_575));
    assert_eq!(check.release.withheld, Money::ZERO);
    assert_eq!(check.release.released_total, eur(171_500));
}

#[test]
fn vat_discount_cash_discount_and_rounding() {
    let mut s = setup();
    s.contract.discount_percent = dec("3");
    s.contract.terms.cash_discount_percent = Some(dec("2"));
    s.contract.terms.cash_discount_days = Some(14);
    let mut billing = Billing::default();
    let mut first = invoice(
        InvoiceKind::Partial,
        "R1",
        Mode::Cumulative,
        Mode::Cumulative,
    );
    first.vat = VatRule::Rate(dec("19"));
    first.items.insert(s.a, claim("33.333"));
    first.items.insert(s.b, claim("3"));
    let first = billing.add_invoice(first);
    let check = s.check(&billing, first);
    // 33,333 × 2,50 = 83,3325 → 83,33; + 450 = 533,33; − 3 % = 16,00 (15,9999);
    // 517,33 + 19 % 98,29 (98,2927) = 615,62; Skonto 2 % = 12,31.
    assert_eq!(check.release.performance, eur(53_333));
    assert_eq!(check.release.discount, eur(1_600));
    assert_eq!(check.release.release_net, eur(51_733));
    assert_eq!(check.release.vat_amount, eur(9_829));
    assert_eq!(check.release.release_gross, eur(61_562));
    assert_eq!(check.release.cash_discount, eur(1_231));
    assert!(!check.findings.contains(&Finding::ReverseCharge));
}

#[test]
fn progress_from_zero_to_one_on_a_lump_sum() {
    let s = setup();
    let mut billing = Billing::default();
    let mut ids = Vec::new();
    for (number, factor) in [
        ("R1", "0"),
        ("R2", "0.0001"),
        ("R3", "0.9999"),
        ("R4", "1"),
        ("R5", "0.5"),
    ] {
        let mut next = invoice(
            InvoiceKind::Partial,
            number,
            Mode::Cumulative,
            Mode::Cumulative,
        );
        next.items.insert(
            s.l,
            InvoiceItem {
                progress: Some(Progress {
                    factor: dec(factor),
                    note: "Absprache mit AN".into(),
                }),
                ..InvoiceItem::default()
            },
        );
        ids.push(billing.add_invoice(next));
    }
    let releases: Vec<Money> = ids
        .iter()
        .map(|id| s.check(&billing, *id).release.release_net)
        .collect();
    // Cumulative 0; 25,00; 249.975,00; 250.000,00 → releases 0; 25; 249.950; 25.
    assert_eq!(
        releases[..4],
        [Money::ZERO, eur(2_500), eur(24_995_000), eur(2_500)]
    );
    let r2 = s.check(&billing, ids[1]);
    let line = &r2.positions[0];
    assert_eq!(line.basis, Some(CheckBasis::Progress));
    assert_eq!(line.cumulative, dec("0.0001"));
    assert_eq!(line.cumulative_total, eur(2_500));
    // Back to 0,5: a hint and an overpayment of 125.000.
    let r5 = s.check(&billing, ids[4]);
    assert_eq!(r5.release.release_net, eur(-12_500_000));
    assert!(has(&r5, s.l, |f| *f
        == Finding::ProgressDecreased {
            previous: dec("1"),
            now: dec("0.5")
        }));
    assert!(r5
        .findings
        .iter()
        .any(|f| matches!(f, Finding::Overpayment { .. })));
}

#[test]
fn progress_on_quantities_sources_and_invalid_values() {
    let s = setup();
    let mut billing = Billing::default();
    let mut first = invoice(
        InvoiceKind::Partial,
        "R1",
        Mode::Increment,
        Mode::Cumulative,
    );
    // 37,50 % of 10 St = 3,75 St = 562,50; a checked quantity is information.
    first.items.insert(
        s.b,
        InvoiceItem {
            progress: Some(Progress {
                factor: dec("0.375"),
                note: "Montage zu 3/8".into(),
            }),
            checked_quantity: Some(dec("4")),
            ..claim("5")
        },
    );
    // Invalid progress: ignored, the invoice quantity counts.
    first.items.insert(
        s.a,
        InvoiceItem {
            progress: Some(Progress {
                factor: dec("1.0001"),
                note: "x".into(),
            }),
            ..claim("10")
        },
    );
    // No note: invalid as well.
    first.items.insert(
        s.h,
        InvoiceItem {
            progress: Some(Progress {
                factor: dec("0.5"),
                note: String::new(),
            }),
            evidence: true,
            ..InvoiceItem::default()
        },
    );
    let first = billing.add_invoice(first);
    let check = s.check(&billing, first);
    let b = check
        .positions
        .iter()
        .find(|line| line.position == s.b)
        .unwrap();
    assert_eq!(b.cumulative, dec("3.75"));
    assert_eq!(b.cumulative_total, eur(56_250));
    assert!(b.findings.contains(&Finding::SeveralSources {
        used: CheckBasis::Progress,
        others: vec![CheckBasis::Quantity]
    }));
    assert!(b.findings.contains(&Finding::QuantityReduced {
        claimed: dec("5"),
        checked: dec("3.75")
    }));
    let a = check
        .positions
        .iter()
        .find(|line| line.position == s.a)
        .unwrap();
    assert_eq!(a.basis, Some(CheckBasis::Claimed));
    assert!(a
        .findings
        .iter()
        .any(|f| matches!(f, Finding::ProgressInvalid(text) if text.contains("zwischen 0 und 1"))));
    assert!(has(
        &check,
        s.h,
        |f| matches!(f, Finding::ProgressInvalid(text) if text.contains("Vermerk"))
    ));
    // 25 + 562,50
    assert_eq!(check.release.performance, eur(58_750));

    // The planner chooses the checked quantity instead.
    billing
        .invoice_mut(first)
        .unwrap()
        .items
        .get_mut(&s.b)
        .unwrap()
        .basis = Some(CheckBasis::Quantity);
    let check = s.check(&billing, first);
    let b = check
        .positions
        .iter()
        .find(|line| line.position == s.b)
        .unwrap();
    assert_eq!(
        (b.basis, b.cumulative),
        (Some(CheckBasis::Quantity), dec("4"))
    );
}

#[test]
fn change_orders_decide_what_is_billable() {
    let mut s = setup();
    let mut order = ChangeOrder::new(ChangeOrderId(1), 1, "Mast");
    order.status = ChangeOrderStatus::Approved;
    order.items.push(ChangeOrderItem {
        position: s.n,
        offered_quantity: dec("1"),
        offered_unit_price: Some(dec("1000")),
        ..ChangeOrderItem::default()
    });
    let mut more = ChangeOrder::new(ChangeOrderId(2), 2, "Mehr Geräte");
    more.status = ChangeOrderStatus::InReview;
    more.items.push(ChangeOrderItem {
        position: s.b,
        change: ItemChange::Quantity,
        offered_quantity: dec("5"),
        ..ChangeOrderItem::default()
    });
    s.orders = vec![order, more];
    let mut billing = Billing::default();
    let mut first = invoice(
        InvoiceKind::Partial,
        "R1",
        Mode::Cumulative,
        Mode::Cumulative,
    );
    first.items.insert(s.n, claim("1"));
    first.items.insert(s.b, claim("12"));
    first.items.insert(
        s.x,
        InvoiceItem {
            claimed_unit_price: Some(dec("80")),
            ..claim("2")
        },
    );
    let first = billing.add_invoice(first);

    let check = s.check(&billing, first);
    // N billable at 1.000; B 12 × 150 = 1.800; X not in the contract.
    assert_eq!(check.release.performance, eur(280_000));
    assert!(has(&check, s.x, |f| *f == Finding::NotInContract));
    let over = check
        .positions
        .iter()
        .find(|line| line.position == s.b)
        .unwrap()
        .findings
        .iter()
        .find(|f| matches!(f, Finding::OverOrdered { .. }))
        .unwrap()
        .message();
    assert!(
        over.contains("(120,0 %)") && over.contains("§ 2 Abs. 3 VOB/B"),
        "{over}"
    );
    // Ordered: contract + N01 (1.000); forecast includes X at the claimed price.
    assert_eq!(check.change_orders_total, eur(100_000));
    assert_eq!(check.ordered_total, eur(25_395_000));

    // Approving N02 raises the ordered quantity of B to 15: no warning.
    s.orders[1].status = ChangeOrderStatus::Approved;
    let check = s.check(&billing, first);
    assert!(!has(&check, s.b, |f| matches!(
        f,
        Finding::OverOrdered { .. }
    )));

    // N01 not approved: N is pending and not billed.
    s.orders[0].status = ChangeOrderStatus::Reviewed;
    let check = s.check(&billing, first);
    assert!(has(
        &check,
        s.n,
        |f| matches!(f, Finding::ChangeOrderPending { label, status } if label == "N01" && *status == ChangeOrderStatus::Reviewed)
    ));
    assert_eq!(check.release.performance, eur(180_000));
}

#[test]
fn findings_of_the_invoice_check() {
    let s = setup();
    let mut billing = Billing::default();
    let mut first = invoice(
        InvoiceKind::Partial,
        "R1",
        Mode::Increment,
        Mode::Cumulative,
    );
    first.claimed_net = Some(eur(200_000));
    // EP differs and GP written wrong.
    first.items.insert(
        s.a,
        InvoiceItem {
            claimed_unit_price: Some(dec("2.60")),
            claimed_total: Some(dec("110")),
            ..claim("40")
        },
    );
    first.items.insert(s.b, claim("10"));
    first.items.insert(s.h, claim("5"));
    first.items.insert(s.p, claim("1"));
    let first = billing.add_invoice(first);
    let mut second = invoice(
        InvoiceKind::Partial,
        "R2",
        Mode::Increment,
        Mode::Cumulative,
    );
    second.items.insert(s.b, claim("1"));
    let second = billing.add_invoice(second);

    let check = s.check(&billing, first);
    assert!(has(&check, s.a, |f| *f
        == Finding::UnitPriceDiffers {
            claimed: dec("2.60"),
            contract: dec("2.50")
        }));
    // 40 × 2,60 = 104,00 written as 110,00.
    assert!(has(&check, s.a, |f| *f
        == Finding::ArithmeticError {
            stated: eur(11_000),
            computed: eur(10_400)
        }));
    assert!(has(&check, s.h, |f| *f == Finding::EvidenceMissing));
    assert!(has(&check, s.p, |f| *f == Finding::ProvisionalPosition));
    // Claimed: 110 + 1.500 + 300 + 10 = 1.920 ≠ 2.000.
    assert!(check.findings.contains(&Finding::ClaimedSumDiffers {
        stated: eur(200_000),
        computed: eur(192_000)
    }));
    // Checked with the contract price: 100 + 1.500 + 300 + 10.
    assert_eq!(check.release.performance, eur(191_000));

    // B claimed again although all 10 St are billed.
    let check = s.check(&billing, second);
    assert!(has(&check, s.b, |f| *f
        == Finding::PossiblyBilledTwice {
            claimed_total: dec("11"),
            ordered: dec("10")
        }));
    for finding in check.all_findings() {
        assert!(!finding.message().is_empty());
    }
}

#[test]
fn takeoff_lines_count_as_increment() {
    let s = setup();
    let mut billing = Billing::default();
    let mut first = invoice(
        InvoiceKind::Partial,
        "R1",
        Mode::Cumulative,
        Mode::Cumulative,
    );
    first.items.insert(s.a, claim("45"));
    let first = billing.add_invoice(first);
    let second = billing.add_invoice(invoice(
        InvoiceKind::Partial,
        "R2",
        Mode::Cumulative,
        Mode::Cumulative,
    ));
    billing.takeoff_mut(s.a).extend([
        TakeoffLine {
            label: "EG".into(),
            formula: "2*12,5".into(),
            invoice: Some(first),
        },
        TakeoffLine {
            label: "OG".into(),
            formula: "15".into(),
            invoice: Some(first),
        },
        TakeoffLine {
            label: "Fehler".into(),
            formula: "3*".into(),
            invoice: Some(first),
        },
        TakeoffLine {
            label: "DG".into(),
            formula: "30".into(),
            invoice: Some(second),
        },
        TakeoffLine {
            label: "später".into(),
            formula: "99".into(),
            invoice: None,
        },
    ]);
    let check = s.check(&billing, first);
    let a = &check.positions[0];
    assert_eq!(
        (a.basis, a.cumulative),
        (Some(CheckBasis::Takeoff), dec("40"))
    );
    assert!(a.findings.contains(&Finding::QuantityReduced {
        claimed: dec("45"),
        checked: dec("40")
    }));
    assert!(a
        .findings
        .iter()
        .any(|f| matches!(f, Finding::TakeoffError(text) if text.contains("„Fehler“"))));
    // R2 has no item for A, only takeoff: 40 + 30 = 70 cumulative.
    let check = s.check(&billing, second);
    let a = &check.positions[0];
    assert_eq!(
        (a.previous, a.cumulative, a.checked),
        (dec("40"), dec("70"), dec("70"))
    );
    assert_eq!(check.release.release_net, eur(7_500));
}

#[test]
fn single_invoices_stand_alone_corrections_and_lump_sums() {
    let s = setup();
    let mut billing = Billing::default();
    let mut first = invoice(
        InvoiceKind::Partial,
        "R1",
        Mode::Cumulative,
        Mode::Cumulative,
    );
    first.claimed_lump = Some(eur(50_000));
    first.checked_lump = Some(eur(40_000));
    let first = billing.add_invoice(first);
    let mut regie = invoice(
        InvoiceKind::Single,
        "Regie",
        Mode::Cumulative,
        Mode::Cumulative,
    );
    regie.items.insert(
        s.h,
        InvoiceItem {
            evidence: true,
            ..claim("4")
        },
    );
    let regie = billing.add_invoice(regie);
    let mut second = invoice(
        InvoiceKind::Partial,
        "R2",
        Mode::Cumulative,
        Mode::Cumulative,
    );
    second.items.insert(s.b, claim("4"));
    second.correction = Some(Correction {
        amount: eur(-1),
        text: "Rundung".into(),
    });
    let second = billing.add_invoice(second);

    // R1: lump 400 checked of 500 claimed.
    let check = s.check(&billing, first);
    assert_eq!(check.release.release_net, eur(40_000));
    assert_eq!(check.claimed_total, eur(50_000));
    // Regie on its own: 4 h × 60, nothing credited.
    let check = s.check(&billing, regie);
    assert_eq!(check.label, "Einzelrechnung");
    assert_eq!(check.release.release_net, eur(24_000));
    // R2 cumulative: positions only (the lump is replaced) 600 − 400 − 0,01.
    let check = s.check(&billing, second);
    assert_eq!(check.release.credited, eur(40_000));
    assert_eq!(check.release.release_net, eur(19_999));
    assert_eq!(
        check.earlier.len(),
        1,
        "die Einzelrechnung zählt nicht zur Folge"
    );
    // Credited by hand.
    billing.invoice_mut(second).unwrap().credited = Some(eur(30_000));
    assert_eq!(s.check(&billing, second).release.release_net, eur(29_999));
}

#[test]
fn contract_exceeded_and_forecast() {
    let mut s = setup();
    // Small contract: only A and B (1.750).
    s.contract.items.retain(|id, _| *id == s.a || *id == s.b);
    s.contract.total = eur(175_000);
    let mut billing = Billing::default();
    let mut first = invoice(
        InvoiceKind::Partial,
        "R1",
        Mode::Cumulative,
        Mode::Cumulative,
    );
    first.items.insert(s.a, claim("130"));
    let first = billing.add_invoice(first);
    let check = s.check(&billing, first);
    // Forecast: 130 m × 2,50 + 10 St × 150 = 1.825 > 1.750.
    assert_eq!(check.forecast, eur(182_500));
    assert!(check.findings.contains(&Finding::ForecastExceeds {
        forecast: eur(182_500),
        ordered: eur(175_000)
    }));
    assert_eq!(check.performed_percent, dec("18.6"));
    let mut second = invoice(
        InvoiceKind::Partial,
        "R2",
        Mode::Cumulative,
        Mode::Cumulative,
    );
    second.items.insert(s.a, claim("130"));
    second.items.insert(s.b, claim("10"));
    let second = billing.add_invoice(second);
    let check = s.check(&billing, second);
    assert!(check.findings.contains(&Finding::ContractExceeded {
        performed: eur(182_500),
        ordered: eur(175_000)
    }));
    assert!(billing.check(&s.context(), InvoiceId(99)).is_err());
}

#[test]
fn progress_on_a_change_order_position() {
    let mut s = setup();
    let mut order = ChangeOrder::new(ChangeOrderId(1), 1, "Mast");
    order.status = ChangeOrderStatus::Approved;
    order.items.push(ChangeOrderItem {
        position: s.n,
        offered_quantity: dec("4"),
        offered_unit_price: Some(dec("333.33")),
        ..ChangeOrderItem::default()
    });
    s.orders = vec![order];
    let mut billing = Billing::default();
    let mut first = invoice(
        InvoiceKind::Partial,
        "R1",
        Mode::Cumulative,
        Mode::Cumulative,
    );
    first.items.insert(
        s.n,
        InvoiceItem {
            progress: Some(Progress {
                factor: dec("0.3333"),
                note: "Absprache vom 01.02.".into(),
            }),
            ..InvoiceItem::default()
        },
    );
    let first = billing.add_invoice(first);
    // 4 × 0,3333 = 1,3332 St × 333,33 = 444,395556 → 444,40.
    let check = s.check(&billing, first);
    let n = &check.positions[0];
    assert_eq!((n.ordered, n.cumulative), (Some(dec("4")), dec("1.3332")));
    assert_eq!(n.cumulative_total, eur(44_440));
    assert_eq!(check.release.release_net, eur(44_440));
}
