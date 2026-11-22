//! Aufteilungszeilen im Kosten-LV (Anwender 19.11.: „Dann können wir die
//! Kostenberechnung eigentlich auch über ein Kurztext-LV machen, und die
//! Darstellung ist einheitlich … ich brauche dann nur die Möglichkeit, die
//! Prozentzeilen anzulegen“): the GA as Kurztext positions – the Datenpunkte
//! (AI, BI … × EP) are the Rechengrundlage, below them one line per KG
//! 481–489 with its percentage carries the money.
//!
//! An Aufteilungszeile is a Zuschlagsposition with [`Surcharge::split`]
//! (percentage in the EP field, Bezug „alle Positionen davor“ or a list,
//! KG of its own); the rules are in [`crate::surcharge`]. Here: the block
//! with the split of the Datenpunktrechnung, the Kurztext template of the GA
//! and the findings of „LV prüfen“.

use std::ops::Range;

use rust_decimal::Decimal;

use crate::cost_estimate::{default_point_kinds, default_shares, CostShare, PointKind};
use crate::cost_lv::din_group_label;
use crate::lv::{Lv, LvGroup, LvRow, RowId};
use crate::lv_purpose::LvPurpose;
use crate::money::Money;
use crate::position::Position;
use crate::surcharge::{percent_text, Surcharge};

/// Title of the Kurztext template of the GA.
pub const POINTS_TEMPLATE_TITLE: &str = "Gebäudeautomation nach Datenpunkten";

/// The Aufteilungszeilen of `shares` (KG, percentage): „KG 481
/// Automationseinrichtungen“ with 40 % …, Bezug „alle Positionen davor“.
pub fn split_rows(shares: &[CostShare]) -> Vec<LvRow> {
    split_rows_on(shares, &Surcharge::split_on_all())
}

/// [`split_rows`] with another Bezug (e.g. a list of positions).
pub fn split_rows_on(shares: &[CostShare], surcharge: &Surcharge) -> Vec<LvRow> {
    shares
        .iter()
        .map(|share| {
            let label = din_group_label(&share.code);
            let text = if label.is_empty() {
                format!("KG {}", share.code)
            } else {
                format!("KG {} {label}", share.code)
            };
            let mut position =
                Position::surcharge_position(text, surcharge.clone(), Some(share.percent));
            position.set_cost_group(Some(share.code.clone()));
            LvRow::Position(position)
        })
        .collect()
}

/// The Datenpunkte of the template as Kurztext positions: „1.1.1 Analoge
/// Eingabe (AI)“, St, EP of the Datenpunktrechnung (hardware 1.1.n, virtual
/// 1.2.n as in the GA-Funktionsliste), quantity 0 to fill in.
pub fn point_rows(kinds: &[PointKind]) -> Vec<LvRow> {
    let mut hardware = 0;
    let mut virtual_points = 0;
    kinds
        .iter()
        .map(|kind| {
            let number = if kind.is_virtual() {
                virtual_points += 1;
                format!("1.2.{virtual_points}")
            } else {
                hardware += 1;
                format!("1.1.{hardware}")
            };
            LvRow::Position(Position::new(
                "",
                format!("{number} {}", kind.caption()),
                Decimal::ZERO,
                "St",
                Some(kind.unit_price),
            ))
        })
        .collect()
}

impl Lv {
    /// „Aufteilung nach KG einfügen …“ (only in a Kosten-LV): the six lines of
    /// the Datenpunktrechnung (481 40 %, 482 12 % … 489 3 %) after the row
    /// `after` – on a title at the end of its own positions –, referring to the
    /// positions before them in the title. Returns the inserted range.
    pub fn insert_split_block(&mut self, after: Option<usize>) -> Result<Range<usize>, String> {
        self.check_cost_lv("Aufteilungszeilen")?;
        let index = self.split_place(after);
        self.insert_rows_numbered(index, split_rows(&default_shares()))
    }

    /// „Aufteilung nach KG einfügen“ on several chosen positions: the block
    /// with Bezug „Liste“ on `positions` (ordinary ones, Zuschläge left out),
    /// right after the last of them.
    pub fn insert_split_block_on(&mut self, positions: &[RowId]) -> Result<Range<usize>, String> {
        self.check_cost_lv("Aufteilungszeilen")?;
        let ids: Vec<RowId> = positions
            .iter()
            .copied()
            .filter(|id| {
                self.position_by_id(*id)
                    .is_some_and(|position| !position.is_surcharge())
            })
            .collect();
        let last = ids
            .iter()
            .filter_map(|id| self.index_of(*id))
            .max()
            .ok_or_else(|| "Keine Position als Rechengrundlage gewählt.".to_string())?;
        let surcharge = Surcharge {
            kind: crate::surcharge::SurchargeKind::Listed,
            references: ids
                .iter()
                .map(|&position| crate::surcharge::SurchargeReference {
                    position,
                    quantity: None,
                })
                .collect(),
            split: true,
        };
        self.insert_rows_numbered(last + 1, split_rows_on(&default_shares(), &surcharge))
    }

    /// „GA nach Datenpunkten (Kurztext-Vorlage) einfügen“ (only in a
    /// Kosten-LV): a title „Gebäudeautomation nach Datenpunkten“ (KG 480; a
    /// subtitle when the title at `after` has a level below it), the
    /// Datenpunkte AI … BV with the EP of the Datenpunktrechnung and below them
    /// the Aufteilung on 481–489. Returns the inserted range.
    pub fn insert_points_template(&mut self, after: Option<usize>) -> Result<Range<usize>, String> {
        self.check_cost_lv("Die GA-Vorlage")?;
        // The title the cursor belongs to; the template goes to the end of its
        // block, one level below it when the mask has one.
        let owner = after.and_then(|index| {
            (0..=index.min(self.rows().len().saturating_sub(1)))
                .rev()
                .find(|&row| self.rows()[row].is_group())
        });
        let (index, level) = match owner {
            Some(owner) => {
                let level = self.rows()[owner].as_group().map_or(0, LvGroup::level);
                let deeper = level + 1 < self.level_count();
                (
                    self.block_range(owner).end,
                    if deeper { level + 1 } else { level },
                )
            }
            None => (self.rows().len(), 0),
        };
        let mut rows = Vec::new();
        if self.level_count() > 0 {
            let mut title = LvGroup::new(level, "", POINTS_TEMPLATE_TITLE);
            title.set_cost_group(Some(crate::cost_points::POINTS_GROUP.to_string()));
            rows.push(LvRow::Group(title));
        }
        rows.extend(point_rows(&default_point_kinds()));
        rows.extend(split_rows(&default_shares()));
        self.insert_rows_numbered(index, rows)
    }

    /// „Datenpunkt-Abschnitt in Kurztext-Positionen umwandeln“ (GU-Fall →
    /// Kurztext): the title at `index` loses its data points and gets, per
    /// Bauteil, the Datenpunkte as positions (quantity = the sum of the plant
    /// list, EP = the price of the DP-Art; in a Kostenschätzung with one
    /// Kennwert that one) and – in a Kostenberechnung – below them the
    /// Aufteilung of the Bauteil. Several Bauteile get a subtitle each when the
    /// mask has a level below the title, otherwise their name before the
    /// Kurztext. The sum stays (the rounding rest as in the Datenpunktrechnung);
    /// the rows of the plant list (Ort, Anlage, Bemerkung) are not taken over.
    /// Returns the inserted range.
    pub fn point_section_to_kurztext(&mut self, index: usize) -> Result<Range<usize>, String> {
        self.check_cost_lv("Kurztext-Positionen aus Datenpunkten")?;
        let Some(group) = self.rows().get(index).and_then(LvRow::as_group) else {
            return Err("Die Zeile ist kein Titel.".to_string());
        };
        let Some(points) = group.points().cloned() else {
            return Err("Der Titel ist kein Datenpunkt-Abschnitt.".to_string());
        };
        let level = group.level();
        let subtitles = points.sections.len() > 1 && level + 1 < self.level_count();
        let calculation = points.stage == crate::cost_estimate::CostStage::Calculation;
        let mut rows = Vec::new();
        for section in &points.sections {
            let name = section.name.trim();
            if subtitles {
                rows.push(LvRow::Group(LvGroup::new(level + 1, "", name)));
            }
            let mut kinds = points.kinds.clone();
            if !calculation && !points.rate_per_kind {
                for kind in &mut kinds {
                    kind.unit_price = points.rate;
                }
            }
            for (kind_index, row) in point_rows(&kinds).into_iter().enumerate() {
                let LvRow::Position(mut position) = row else {
                    continue;
                };
                let count: u64 = section
                    .rows
                    .iter()
                    .map(|line| u64::from(line.count(kind_index)))
                    .sum();
                position.set_quantity(Decimal::from(count));
                if points.sections.len() > 1 && !subtitles && !name.is_empty() {
                    position.set_short_text(format!("{name} – {}", position.short_text()));
                }
                rows.push(LvRow::Position(position));
            }
            if calculation {
                let shares = section.shares.as_ref().unwrap_or(&points.shares);
                rows.extend(split_rows(shares));
            }
        }
        let mut work = self.clone();
        if let Some(LvRow::Group(group)) = work.row_mut(index) {
            group.set_points(None);
        }
        let range = work.insert_rows_numbered(index + 1, rows)?;
        *self = work;
        Ok(range)
    }

    fn check_cost_lv(&self, what: &str) -> Result<(), String> {
        match self.purpose() {
            LvPurpose::Cost(_) => Ok(()),
            LvPurpose::Tender => Err(format!(
                "{what} gibt es nur in einer Kostenermittlung (Kosten-LV) – in einem \
                 Ausschreibungs-LV zählten die Positionen sonst nicht."
            )),
        }
    }

    /// Where a new block goes: after a position (or hint) right behind it, on
    /// a title at the end of its own rows (before its first subtitle), without
    /// a cursor at the end.
    fn split_place(&self, after: Option<usize>) -> usize {
        match after {
            None => self.rows().len(),
            Some(index) if index >= self.rows().len() => self.rows().len(),
            Some(index) if self.rows()[index].is_group() => self.rows()[index + 1..]
                .iter()
                .position(LvRow::is_group)
                .map_or(self.rows().len(), |offset| index + 1 + offset),
            Some(index) => index + 1,
        }
    }

    /// Inserts `rows` from `index` on, each with a free order number (an
    /// automatically numbered LV is renumbered). When the neighbours leave no
    /// free numbers (six lines between 0020 and 0030), the positions of the
    /// title get new ones – titles keep theirs (480 …), finished positions
    /// too. All or nothing.
    fn insert_rows_numbered(
        &mut self,
        index: usize,
        rows: Vec<LvRow>,
    ) -> Result<Range<usize>, String> {
        let count = rows.len();
        let mut work = self.clone();
        let fits = rows
            .iter()
            .enumerate()
            .all(|(offset, row)| work.insert_numbered(index + offset, row.clone()).is_ok());
        if !fits {
            work = self.clone();
            for (offset, row) in rows.into_iter().enumerate() {
                match row {
                    LvRow::Group(_) => {
                        work.insert_numbered(index + offset, row)?;
                    }
                    row => {
                        work.insert_row(index + offset, row);
                    }
                }
            }
            let scope = work.rows()[..index]
                .iter()
                .rev()
                .find(|row| row.is_group())
                .map(LvRow::id);
            work.renumber_with(crate::numbering::RenumberMode {
                positions_only: true,
                keep_done: true,
                scope,
            });
        }
        *self = work;
        Ok(index..index + count)
    }
}

/// Findings of „LV prüfen“ about Aufteilungszeilen: a block that does not make
/// 100 % (with the amount not split), a line without Rechengrundlage, and
/// Aufteilungszeilen in a tender LV.
pub fn split_findings(lv: &Lv) -> Vec<(RowId, String)> {
    let name_of = |id: RowId| {
        lv.position_by_id(id)
            .map(|position| match position.ordinal().trim() {
                "" => position.short_text().trim().to_string(),
                ordinal => ordinal.to_string(),
            })
            .unwrap_or_default()
    };
    let mut findings = Vec::new();
    if lv.purpose().is_tender() {
        for position in lv.positions() {
            if position
                .surcharge()
                .is_some_and(|surcharge| surcharge.split)
            {
                findings.push((
                    position.id(),
                    format!(
                        "{}: Aufteilungszeile im Ausschreibungs-LV – sie gehört in eine Kostenermittlung \
                         (Kosten-LV); ihre Positionen zählen hier nicht selbst.",
                        name_of(position.id())
                    ),
                ));
            }
        }
    }
    for block in lv.split_blocks() {
        let (Some(&first), Some(&last)) = (block.rows.first(), block.rows.last()) else {
            continue;
        };
        let range = if first == last {
            name_of(first)
        } else {
            format!("{} bis {}", name_of(first), name_of(last))
        };
        if block.bases.is_empty() {
            findings.push((
                first,
                format!(
                    "Aufteilung {range}: keine Rechengrundlage – davor stehen im Titel keine Positionen \
                     (oder der Bezug nennt keine)."
                ),
            ));
        }
        if !block.is_complete() {
            let open = block.base - block.amount;
            let (amount, what) = if open.cents() < 0 {
                (Money::from_cents(-open.cents()), "zu viel verteilt")
            } else {
                (open, "nicht verteilt")
            };
            findings.push((
                last,
                format!(
                    "Aufteilung {range}: {} statt 100 % – {amount} der Bezugssumme {} {what}.",
                    percent_text(block.percent),
                    block.base,
                ),
            ));
        }
    }
    findings
}

#[cfg(test)]
mod tests;
