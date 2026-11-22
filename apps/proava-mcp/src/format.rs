//! Short German texts for numbers, rows and sums.

use proava_domain::{format_decimal_de, Decimal, Lv, LvRow, Money, Position, PositionKind, RowId};

pub(crate) fn quantity(value: Decimal) -> String {
    let value = value.normalize();
    format_decimal_de(value, value.scale().min(3))
}

pub(crate) fn price(value: Option<Decimal>) -> String {
    match value {
        Some(value) => format_decimal_de(value, value.normalize().scale().clamp(2, 3)),
        None => "-".to_string(),
    }
}

pub(crate) fn money(value: Money) -> String {
    format!("{} €", value.plain())
}

pub(crate) fn percent(value: Decimal) -> String {
    let value = value.normalize();
    format!("{} %", format_decimal_de(value, value.scale().min(2)))
}

pub(crate) fn kind_key(kind: PositionKind) -> String {
    match kind {
        PositionKind::Normal => "normal".to_string(),
        PositionKind::Provisional { with_total: false } => "bedarf".to_string(),
        PositionKind::Provisional { with_total: true } => "bedarf_mit_gb".to_string(),
        PositionKind::Base { group } => format!("grund (Wahlgruppe {group})"),
        PositionKind::Alternative { group, serial } => {
            format!("wahl (Wahlgruppe {group}, Nr. {serial})")
        }
        PositionKind::Supplement => "zulage".to_string(),
    }
}

/// "Pos", "Pos B Psch", "T1", "Hinweis".
pub(crate) fn row_kind(row: &LvRow) -> String {
    match row {
        LvRow::Group(group) => format!("T{}", group.level() + 1),
        LvRow::Position(position) => {
            let marks = position.marks();
            if marks.is_empty() {
                "Pos".to_string()
            } else {
                format!("Pos {}", marks.join(" "))
            }
        }
        LvRow::Note(note) if note.is_preliminary() => "Vorbemerkung".to_string(),
        LvRow::Note(note) if note.is_closing() => "Schlussbemerkung".to_string(),
        LvRow::Note(_) => "Hinweis".to_string(),
        LvRow::Performance(performance) => performance.mark(),
        LvRow::Spacer(_) => "Abstand".to_string(),
    }
}

/// "01.02.0010 „Kabel“ (#12)" – how answers name a row.
pub(crate) fn row_name(row: &LvRow) -> String {
    let text = shorten(&row.short_text(), 60);
    match row {
        LvRow::Note(_) if row.is_blank_note() => format!("Hinweis ohne Text (#{})", row.id().0),
        LvRow::Note(_) => format!("Hinweis „{text}“ (#{})", row.id().0),
        LvRow::Spacer(_) => format!("Abstandshalter (#{})", row.id().0),
        LvRow::Performance(_) => format!("„{text}“ (#{})", row.id().0),
        _ => format!(
            "{} „{text}“ (#{})",
            display_ordinal(row.ordinal()),
            row.id().0
        ),
    }
}

pub(crate) fn display_ordinal(ordinal: &str) -> &str {
    if ordinal.is_empty() {
        "(ohne OZ)"
    } else {
        ordinal
    }
}

pub(crate) fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let short: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{short}…")
}

/// Menge/Einheit and EP of a position for lists; a Zuschlagsposition shows
/// its Bezugssumme and percentage („Zuschlag auf 3.778,58 €“, „5 %“).
pub(crate) fn amount_columns(lv: &Lv, position: &Position) -> (String, String) {
    if position.is_surcharge() {
        let base = lv
            .surcharge_values()
            .get(&position.id())
            .map(|value| money(value.base))
            .unwrap_or_else(|| "-".to_string());
        let percent = position
            .unit_price()
            .map(proava_domain::surcharge::percent_text)
            .unwrap_or_else(|| "- %".to_string());
        return (format!("Zuschlag auf {base}"), percent);
    }
    (
        format!("{} {}", quantity(position.quantity()), position.unit()),
        price(position.unit_price()),
    )
}

/// Lines of `position_lesen` about a Zuschlag: kind, percentage, Bezugssumme,
/// listed positions; „zu bezuschlagen“ at a marked position.
pub(crate) fn surcharge_lines(lv: &Lv, position: &Position) -> Vec<String> {
    let mut lines = Vec::new();
    if position.is_marked_for_surcharge() {
        lines.push(
            "zu bezuschlagen: ja (zählt für Zuschläge auf die gekennzeichneten Positionen)"
                .to_string(),
        );
    }
    let Some(surcharge) = position.surcharge() else {
        return lines;
    };
    let value = lv
        .surcharge_values()
        .get(&position.id())
        .copied()
        .unwrap_or_default();
    let word = if surcharge.split {
        "Aufteilung"
    } else {
        "Zuschlag"
    };
    lines.push(format!(
        "{word}: {} {} ({}) · Bezugssumme {} · {word} {}",
        position
            .unit_price()
            .map(proava_domain::surcharge::percent_text)
            .unwrap_or_else(|| "ohne Prozentsatz".to_string()),
        surcharge.kind.label(),
        surcharge.kind.key(),
        money(value.base),
        money(value.amount)
    ));
    if !surcharge.references.is_empty() {
        let listed: Vec<String> = surcharge
            .references
            .iter()
            .filter_map(|reference| {
                let item = lv.position_by_id(reference.position)?;
                Some(match reference.quantity {
                    Some(part) => format!(
                        "{} (Teilmenge {} {})",
                        display_ordinal(item.ordinal()),
                        quantity(part),
                        item.unit()
                    ),
                    None => display_ordinal(item.ordinal()).to_string(),
                })
            })
            .collect();
        lines.push(format!("Bezug: {}", listed.join(", ")));
    }
    lines
}

/// GP of a position for lists: "-" when unpriced, in brackets when it does
/// not count; a Zuschlagsposition with base × percentage of the LV `lv`.
pub(crate) fn position_total(lv: &Lv, position: &Position) -> String {
    // Rechengrundlage of Aufteilungszeilen (only LVs that have them pay for it).
    let basis = !position.is_surcharge()
        && lv
            .positions()
            .any(|other| other.surcharge().is_some_and(|surcharge| surcharge.split))
        && lv.split_bases().contains(&position.id());
    position_total_with(lv, position, basis)
}

/// [`position_total`] with the Rechengrundlage known: its GP in brackets.
pub(crate) fn position_total_with(lv: &Lv, position: &Position, basis: bool) -> String {
    if position.unit_price().is_none() {
        return "-".to_string();
    }
    let total = lv.shown_total(position).plain();
    if position.counts_in_total() && !basis {
        total
    } else {
        format!("({total})")
    }
}

/// Net, VAT and gross of the planner's prices in one line.
pub(crate) fn lv_totals(lv: &Lv) -> String {
    let net = lv.net_total();
    let vat = lv.vat(net);
    let lump = lv
        .lump_sum_total()
        .map(|_| {
            format!(
                " (Pauschalsumme; Summe der Positionen {})",
                money(lv.positions_total())
            )
        })
        .unwrap_or_default();
    format!(
        "LV netto {}{lump} · MwSt {} {} · brutto {}",
        money(net),
        percent(lv.vat_percent()),
        money(vat),
        money(net + vat)
    )
}

/// The headings above a row, outermost first.
pub(crate) fn headings_above(lv: &Lv, index: usize) -> Vec<usize> {
    let mut found: Vec<usize> = Vec::new();
    let mut level = usize::MAX;
    for (candidate, row) in lv.rows()[..index.min(lv.rows().len())]
        .iter()
        .enumerate()
        .rev()
    {
        if let LvRow::Group(group) = row {
            if group.level() < level {
                found.push(candidate);
                level = group.level();
                if level == 0 {
                    break;
                }
            }
        }
    }
    found.reverse();
    found
}

/// "Titel 01.02 „Trassen“: 3.450,00 € · LV netto …" for answers after a change.
pub(crate) fn sums_after_change(lv: &Lv, row: Option<RowId>) -> String {
    let heading = row.and_then(|id| lv.index_of(id)).and_then(|index| {
        if lv.rows()[index].is_group() {
            Some(index)
        } else {
            headings_above(lv, index).last().copied()
        }
    });
    let mut text = String::new();
    if let Some(heading) = heading {
        let total = lv
            .group_totals()
            .into_iter()
            .find(|total| total.group_row == heading)
            .map(|total| total.total)
            .unwrap_or(Money::ZERO);
        let group = &lv.rows()[heading];
        text.push_str(&format!(
            "Titel {} „{}“: {} · ",
            display_ordinal(group.ordinal()),
            shorten(&group.short_text(), 40),
            money(total)
        ));
    }
    text.push_str(&lv_totals(lv));
    text
}

/// How many rows (other than `except`) got a new order number.
pub(crate) fn renumbered_count(before: &[(RowId, String)], lv: &Lv, except: &[RowId]) -> usize {
    before
        .iter()
        .filter(|(id, _)| !except.contains(id))
        .filter(|(id, ordinal)| {
            lv.row_by_id(*id)
                .map(|row| row.ordinal() != ordinal)
                .unwrap_or(false)
        })
        .count()
}

pub(crate) fn ordinals_by_id(lv: &Lv) -> Vec<(RowId, String)> {
    lv.rows()
        .iter()
        .map(|row| (row.id(), row.ordinal().to_string()))
        .collect()
}
