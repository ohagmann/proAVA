use rust_decimal::{Decimal, RoundingStrategy};

use super::encoding::{encode, Encoding};
use super::record::OzMask;
use super::DatanError;
use crate::model::*;
use crate::text_reflow::{logical_lines, wrap_lines};

/// Writes the document as GAEB 90 in code page 850 with CRLF line ends.
/// Phases 81, 82, 83 and 86 are written with texts (long form), 84 and 85
/// as the short form with prices only.
pub fn write(doc: &GaebDocument) -> Result<Vec<u8>, DatanError> {
    Ok(encode(&write_text(doc)?, Encoding::Cp850))
}

/// Same as [`write`], before the character set conversion.
pub fn write_text(doc: &GaebDocument) -> Result<String, DatanError> {
    let boq = &doc.award.boq;
    if boq.breakdown.is_empty() {
        return Err(DatanError::Unwritable(
            "die OZ-Gliederung (breakdown) fehlt".into(),
        ));
    }
    let mask = OzMask::from_breakdown(&boq.breakdown).map_err(DatanError::InvalidMask)?;
    let lots = boq.breakdown.first().map(|l| l.kind) == Some(BreakdownKind::Lot);
    let mut writer = Writer {
        doc,
        mask,
        lots,
        full: !matches!(doc.phase, Phase::Dp84 | Phase::Dp85),
        lines: Vec::new(),
        item_count: 0,
    };
    writer.header()?;
    writer.nodes(&boq.body, &mut Vec::new())?;
    writer.closing()?;
    Ok(writer
        .lines
        .iter()
        .map(|line| format!("{line}\r\n"))
        .collect())
}

struct Line(Vec<char>);

impl Line {
    fn new(kind: &str) -> Self {
        let mut line = Line(vec![' '; 74]);
        line.put(1, 2, kind);
        line
    }

    fn put(&mut self, from: usize, to: usize, text: &str) {
        for (i, c) in text.chars().take(to + 1 - from).enumerate() {
            self.0[from - 1 + i] = if c == '\n' || c == '\r' { ' ' } else { c };
        }
    }

    fn put_right(&mut self, from: usize, to: usize, text: &str) {
        let width = to + 1 - from;
        let chars: Vec<char> = text.chars().collect();
        let skip = chars.len().saturating_sub(width);
        let start = to + 1 - (chars.len() - skip);
        let text: String = chars[skip..].iter().collect();
        self.put(start, to, &text);
    }

    fn flag(&mut self, col: usize, on: bool, c: char) {
        if on {
            self.0[col - 1] = c;
        }
    }

    fn num(
        &mut self,
        from: usize,
        to: usize,
        value: Option<Decimal>,
        decimals: u32,
        sign: Option<usize>,
    ) -> Result<(), DatanError> {
        let Some(value) = value else {
            return Ok(());
        };
        let digits = fixed_digits(value, decimals, to + 1 - from)?;
        if value.is_sign_negative() && !value.is_zero() {
            let Some(col) = sign else {
                return Err(DatanError::Unwritable(format!(
                    "negativer Wert {value} an Stelle {from}–{to} nicht erlaubt"
                )));
            };
            self.put(col, col, "-");
        }
        self.put(from, to, &digits);
        Ok(())
    }
}

fn fixed_digits(value: Decimal, decimals: u32, width: usize) -> Result<String, DatanError> {
    let rounded = value
        .abs()
        .round_dp_with_strategy(decimals, RoundingStrategy::MidpointAwayFromZero);
    let scaled = (rounded * Decimal::from(10u64.pow(decimals))).trunc();
    let digits = scaled.normalize().to_string();
    if digits.len() > width {
        return Err(DatanError::Unwritable(format!(
            "Zahl {value} passt nicht in {width} Stellen"
        )));
    }
    Ok(format!("{digits:0>width$}"))
}

struct Writer<'a> {
    doc: &'a GaebDocument,
    mask: OzMask,
    lots: bool,
    full: bool,
    lines: Vec<String>,
    item_count: usize,
}

impl Writer<'_> {
    fn push(&mut self, line: Line) {
        let content: String = line.0.into_iter().collect();
        let number = self.lines.len() + 1;
        self.lines.push(format!("{content}{number:06}"));
    }

    fn header(&mut self) -> Result<(), DatanError> {
        let doc = self.doc;
        let boq = &doc.award.boq;

        if !doc.award.contract_text.is_empty() {
            self.push(Line::new("T0"));
            for part in wrap_lines(&doc.award.contract_text, 72) {
                let mut line = Line::new("T1");
                line.put(3, 74, &part);
                self.push(line);
            }
            self.push(Line::new("T9"));
        }

        let mut line = Line::new("00");
        line.put(11, 12, &doc.phase.number().to_string());
        line.flag(13, self.full, 'L');
        line.put(14, 28, &doc.award.award_no);
        if doc.award.award_no.is_empty() {
            line.put(29, 36, &doc.project.name);
        }
        line.put(63, 71, &self.mask.to_mask_string());
        line.put(72, 73, "90");
        line.flag(74, self.lots, 'X');
        self.push(line);

        let mut line = Line::new("01");
        let label = first_line(&boq.label);
        line.put(3, 42, label);
        line.put(43, 50, &german_date(doc.date.as_deref()));
        line.put(
            51,
            58,
            &german_date(doc.award.submission.open_date.as_deref()),
        );
        line.put(
            59,
            63,
            doc.award.submission.open_time.as_deref().unwrap_or(""),
        );
        line.put(
            64,
            71,
            &german_date(doc.award.submission.award_deadline.as_deref()),
        );
        line.flag(72, doc.phase.has_prices(), 'X');
        self.push(line);

        let project = if doc.project.label.trim().is_empty() {
            doc.project.name.clone()
        } else {
            doc.project.label.clone()
        };
        let project: Vec<String> = project.split('\n').map(str::to_string).collect();
        for text in wrap_lines(&project, 60).into_iter().take(9) {
            let mut line = Line::new("02");
            line.put(3, 62, &text);
            self.push(line);
        }
        let mut line = Line::new("03");
        if let Some(owner) = &doc.award.owner {
            line.put(3, 62, &owner.name1);
        }
        self.push(line);
        if matches!(doc.phase, Phase::Dp84 | Phase::Dp85 | Phase::Dp86) {
            let mut line = Line::new("04");
            if let Some(bidder) = &doc.award.bidder {
                line.put(3, 62, &bidder.name1);
            }
            self.push(line);
        }
        if !boq.up_component_labels.is_empty() || !boq.time_label.is_empty() {
            let mut line = Line::new("06");
            line.put(3, 3, &boq.up_component_labels.len().min(4).to_string());
            line.put(4, 17, &boq.time_label);
            for (i, label) in boq.up_component_labels.iter().take(4).enumerate() {
                line.put(18 + 14 * i, 31 + 14 * i, label);
            }
            self.push(line);
        }
        let currency = if doc.project.currency.is_empty() {
            "EUR"
        } else {
            doc.project.currency.as_str()
        };
        let mut line = Line::new("08");
        line.put(3, 8, currency);
        line.put(9, 58, if currency == "EUR" { "Euro" } else { currency });
        self.push(line);
        Ok(())
    }

    fn oz(&self, levels: &[String], item: Option<&str>, index: &str) -> String {
        let levels = if self.lots {
            levels.get(1..).unwrap_or(&[])
        } else {
            levels
        };
        self.mask.join(levels, item, index)
    }

    fn nodes(&mut self, nodes: &[BoqNode], levels: &mut Vec<String>) -> Result<(), DatanError> {
        for node in nodes {
            match node {
                BoqNode::Category(category) => self.category(category, levels)?,
                BoqNode::Item(item) => self.item(item, levels)?,
                BoqNode::Remark(remark) if self.full => {
                    self.push(Line::new("20"));
                    // A Hinweistext has no heading in GAEB 90 (other programs
                    // leave it out): it opens the text, then an empty line –
                    // not lost.
                    if !remark.short_text.trim().is_empty() {
                        let mut text = remark.text.clone();
                        text.blocks.insert(
                            0,
                            TextBlock::paragraph(vec![Run {
                                text: remark.short_text.trim().to_string(),
                                ..Run::default()
                            }]),
                        );
                        text.blocks.insert(1, TextBlock::paragraph(Vec::new()));
                        self.long_lines(&text);
                    } else {
                        self.long_lines(&remark.text);
                    }
                    // GAEB 90 has no Textergänzungen in Hinweistexten: they
                    // follow the text as plain lines.
                    for complement in &remark.complements {
                        let body = if complement.body.is_empty() {
                            ".".repeat(20)
                        } else {
                            complement.body.clone()
                        };
                        for part in wrap(&complement_line(complement, &body), 55) {
                            let mut line = Line::new("26");
                            line.put(6, 60, &part);
                            self.push(line);
                        }
                    }
                }
                BoqNode::PerfDescr(perf) if self.full => {
                    let mut line = Line::new("41");
                    line.put_right(3, 6, &perf_number(&perf.number));
                    line.put(7, 61, &perf.label);
                    self.push(line);
                    self.push(Line::new("42"));
                    self.short_lines(&perf.short_text);
                    self.long_lines(&perf.long_text);
                }
                BoqNode::Remark(_) | BoqNode::PerfDescr(_) => {}
            }
        }
        Ok(())
    }

    fn category(
        &mut self,
        category: &Category,
        levels: &mut Vec<String>,
    ) -> Result<(), DatanError> {
        levels.push(category.rno_part.clone());
        let is_lot = self.lots && levels.len() == 1;
        let oz = self.oz(levels, None, "");
        if is_lot {
            let mut line = Line::new("10");
            line.put_right(3, 4, &category.rno_part);
            line.put(5, 44, first_line(&category.label));
            self.push(line);
        } else if self.full {
            let mut line = Line::new("11");
            line.put(3, 11, &oz);
            line.put(12, 12, "N");
            let depth = levels.len() - usize::from(self.lots);
            let keyword = self
                .doc
                .award
                .boq
                .breakdown
                .iter()
                .filter(|l| l.kind == BreakdownKind::Level)
                .nth(depth - 1)
                .map(|l| l.label.as_str())
                .unwrap_or("");
            line.put(17, 30, keyword);
            self.push(line);
            for text in wrap(&category.label, 40).into_iter().take(9) {
                let mut line = Line::new("12");
                line.put(3, 42, &text);
                self.push(line);
            }
        }
        self.nodes(&category.body, levels)?;
        let priced = self.doc.phase.has_prices();
        let totals = category.totals.clone().or_else(|| {
            priced.then(|| Totals {
                total: Some(sum_totals(&category.body)),
                ..Totals::default()
            })
        });
        if is_lot {
            let mut line = Line::new("33");
            line.put_right(3, 4, &category.rno_part);
            if let Some(t) = totals.as_ref().filter(|_| priced) {
                line.num(5, 16, t.total, 2, None)?;
                line.num(17, 23, discount_factor(t), 6, None)?;
                line.num(24, 27, t.vat_percent, 2, None)?;
                line.num(28, 39, t.total_gross, 2, None)?;
                line.num(41, 52, t.discount_amount.map(|a| -a), 2, Some(40))?;
                line.num(54, 65, t.lump_sum, 2, Some(53))?;
            }
            self.push(line);
        } else {
            if self.full {
                let mut line = Line::new("31");
                line.put(3, 11, &oz);
                self.push(line);
            }
            if let Some(t) = totals.filter(|_| priced) {
                let mut line = Line::new("32");
                line.put(3, 11, &oz);
                line.num(13, 24, t.total, 2, Some(12))?;
                line.num(25, 31, discount_factor(&t), 6, None)?;
                line.num(33, 44, t.discount_amount.map(|a| -a), 2, Some(32))?;
                line.num(46, 57, t.lump_sum, 2, Some(45))?;
                self.push(line);
            }
        }
        levels.pop();
        Ok(())
    }

    fn item(&mut self, item: &Item, levels: &[String]) -> Result<(), DatanError> {
        self.item_count += 1;
        let oz = self.oz(levels, Some(&item.rno_part), &item.rno_index);
        let phase = self.doc.phase;
        let has_owner = item.complements.iter().any(|c| c.kind == "Owner");
        let has_bidder = item.complements.iter().any(|c| c.kind == "Bidder");

        if self.full || item.qty_to_be_determined {
            let mut line = Line::new("21");
            line.put(3, 11, &oz);
            let kind = &item.kind;
            let posart1 = match &kind.alternative {
                Some(alt) if alt.serial == "0" || alt.serial.is_empty() => 'G',
                Some(_) => 'A',
                None if kind.hourly => 'S',
                None => 'N',
            };
            let posart2 = match kind.provisional {
                Some(Provisional::WithoutTotal) => 'E',
                Some(Provisional::WithTotal) => 'M',
                None => 'N',
            };
            let postyp = if item.sub_descriptions.is_empty() {
                'N'
            } else {
                'L'
            };
            line.put(12, 14, &format!("{posart1}{posart2}{postyp}"));
            if let Some(alt) = &kind.alternative {
                line.put_right(15, 17, &format!("{:0>3}", alt.group));
                if posart1 == 'A' {
                    line.put(18, 18, &alt.serial);
                }
            }
            line.flag(19, !self.doc.award.boq.up_component_labels.is_empty(), 'X');
            line.flag(20, kind.markup, 'Z');
            if kind.repetition_of.is_some() {
                line.put(21, 21, "W");
            } else if kind.reference_description.is_some() {
                line.put(21, 21, "A");
            }
            line.flag(22, item.qty_to_be_determined, 'X');
            if !item.qty_to_be_determined {
                line.num(24, 34, item.qty, 3, Some(23))?;
            }
            line.put(35, 38, &item.unit);
            line.flag(42, has_owner, 'X');
            line.flag(43, has_bidder, 'X');
            // STLB-Bau keys have no place in GAEB 90.
            put_standard_number(&mut line, item.standard_number.as_ref(), 44, 45);
            self.push(line);
        }

        if phase == Phase::Dp82 {
            let mut line = Line::new("22");
            line.put(3, 11, &oz);
            unit_price(&mut line, item.unit_price)?;
            line.num(25, 36, item.total, 2, Some(24))?;
            self.push(line);
        } else if phase.has_prices() {
            let mut line = Line::new("23");
            line.put(3, 11, &oz);
            unit_price(&mut line, item.unit_price)?;
            line.num(25, 36, item.total, 2, Some(24))?;
            if item.qty_to_be_determined {
                line.num(38, 48, item.qty, 3, Some(37))?;
            }
            let factor = item
                .discount_percent
                .map(|p| Decimal::ONE - p / Decimal::ONE_HUNDRED);
            line.num(49, 55, factor, 6, None)?;
            line.num(57, 61, item.markup_percent, 2, Some(56))?;
            line.num(63, 74, item.markup_base, 2, Some(62))?;
            self.push(line);
        }

        if phase.has_prices()
            && (item.time_quantity.is_some() || item.up_components.iter().any(Option::is_some))
        {
            let mut line = Line::new("30");
            line.num(3, 12, item.time_quantity, 4, None)?;
            for (i, part) in item.up_components.iter().take(4).enumerate() {
                line.num(13 + 10 * i, 22 + 10 * i, *part, 2, None)?;
            }
            self.push(line);
        }

        if self.full {
            let kind = &item.kind;
            if kind.repetition_of.is_some() || kind.reference_description.is_some() {
                let mut line = Line::new("28");
                line.put(3, 57, first_line(&item.short_text));
                if let Some(base) = &kind.repetition_of {
                    line.put(58, 66, &self.dotted_to_oz(base));
                } else if let Some(perf) = &kind.reference_description {
                    line.put_right(67, 70, &perf_number(perf));
                }
                self.push(line);
            }
            self.short_lines(&item.short_text);
            self.long_lines(&item.long_text);
        }
        self.complements(&item.complements, !self.full);

        if self.full {
            for sub in &item.sub_descriptions {
                let mut line = Line::new("24");
                line.put_right(3, 4, &sub.number);
                line.num(6, 16, sub.qty, 3, Some(5))?;
                line.put(17, 20, &sub.unit);
                put_standard_number(&mut line, sub.standard_number.as_ref(), 23, 24);
                self.push(line);
                self.short_lines(&sub.short_text);
                self.long_lines(&sub.long_text);
            }
        }
        Ok(())
    }

    fn dotted_to_oz(&self, dotted: &str) -> String {
        let mut pieces: Vec<String> = dotted.split('.').map(str::to_string).collect();
        let has_index = self
            .mask
            .segments
            .iter()
            .any(|s| s.kind == BreakdownKind::Index);
        let index = if has_index && pieces.len() > self.mask.level_count() + 1 {
            pieces.pop().unwrap_or_default()
        } else {
            String::new()
        };
        let item = pieces.pop().unwrap_or_default();
        self.mask.join(&pieces, Some(&item), &index)
    }

    fn short_lines(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let lines: Vec<String> = text.split('\n').map(str::to_string).collect();
        for text in wrap_lines(&lines, 70) {
            let mut line = Line::new("25");
            line.put(3, 72, &text);
            self.push(line);
        }
    }

    fn long_lines(&mut self, text: &RichText) {
        for text in wrap_lines(&logical_lines(&text.blocks), 55) {
            let mut line = Line::new("26");
            line.put(6, 60, &text);
            self.push(line);
        }
    }

    fn complements(&mut self, complements: &[TextComplement], bidder_only: bool) {
        let mut free_marker = 60;
        for complement in complements {
            let kind = match complement.kind.as_str() {
                "Owner" => 'A',
                "Bidder" => 'B',
                _ => ' ',
            };
            if bidder_only && kind != 'B' {
                continue;
            }
            let marker = if complement.marker.is_empty() {
                free_marker += 1;
                (free_marker - 1).to_string()
            } else {
                complement.marker.clone()
            };
            let body = if complement.body.is_empty() {
                ".".repeat(20)
            } else {
                complement.body.clone()
            };
            let text = complement_line(complement, &body);
            for part in wrap(&text, 55) {
                let mut line = Line::new("26");
                line.put_right(3, 4, &marker);
                line.put(5, 5, &kind.to_string());
                line.put(6, 60, &part);
                self.push(line);
            }
        }
    }

    fn closing(&mut self) -> Result<(), DatanError> {
        let boq = &self.doc.award.boq;
        let mut line = Line::new("99");
        if self.doc.phase.has_prices() {
            let totals = boq.totals.clone().unwrap_or_else(|| Totals {
                total: Some(sum_totals(&boq.body)),
                ..Totals::default()
            });
            line.num(3, 14, totals.total, 2, None)?;
            line.num(15, 21, discount_factor(&totals), 6, None)?;
            line.num(22, 25, totals.vat_percent, 2, None)?;
            line.num(26, 37, totals.total_gross, 2, None)?;
            line.num(39, 50, totals.discount_amount.map(|a| -a), 2, Some(38))?;
            line.num(52, 63, totals.lump_sum, 2, Some(51))?;
        }
        line.put(70, 74, &format!("{:05}", self.item_count.min(99_999)));
        self.push(line);
        Ok(())
    }
}

fn unit_price(line: &mut Line, value: Option<Decimal>) -> Result<(), DatanError> {
    let Some(value) = value else {
        return Ok(());
    };
    let digits = fixed_digits(value, 3, 11)?;
    let (ep, zpf) = digits.split_at(10);
    if value.is_sign_negative() && !value.is_zero() {
        line.put(12, 12, "-");
    }
    line.put(13, 22, ep);
    if zpf != "0" {
        line.put(23, 23, zpf);
    }
    Ok(())
}

fn discount_factor(totals: &Totals) -> Option<Decimal> {
    totals
        .discount_percent
        .map(|p| Decimal::ONE - p / Decimal::ONE_HUNDRED)
}

fn sum_totals(nodes: &[BoqNode]) -> Decimal {
    nodes
        .iter()
        .map(|node| match node {
            BoqNode::Category(c) => c
                .totals
                .as_ref()
                .and_then(|t| t.total)
                .unwrap_or_else(|| sum_totals(&c.body)),
            BoqNode::Item(item) => item.total.unwrap_or_default(),
            BoqNode::Remark(_) | BoqNode::PerfDescr(_) => Decimal::ZERO,
        })
        .sum()
}

/// „Ansprache 'Wert' Nachsatz“ as one text (caption and tail as written in
/// DA XML may carry their own blanks).
fn complement_line(complement: &TextComplement, body: &str) -> String {
    let mut text = complement.caption.trim().to_string();
    if !text.is_empty() {
        text.push(' ');
    }
    text.push('\'');
    text.push_str(body);
    text.push('\'');
    let tail = complement.tail.trim();
    if !tail.is_empty() {
        if !tail.starts_with([',', '.', ';', ':']) {
            text.push(' ');
        }
        text.push_str(tail);
    }
    text
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or("")
}

/// ISO `YYYY-MM-DD` as `TT.MM.JJ`; anything else stays empty.
fn german_date(iso: Option<&str>) -> String {
    let Some(iso) = iso else {
        return String::new();
    };
    let parts: Vec<&str> = iso.split('-').collect();
    match parts.as_slice() {
        [year, month, day] if year.len() == 4 => format!("{day}.{month}.{}", &year[2..]),
        _ => String::new(),
    }
}

/// Splits at line breaks and wraps every line at `width` characters,
/// preferably at a blank.
fn wrap(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for line in text.split('\n') {
        let mut rest: Vec<char> = line.trim_end().chars().collect();
        loop {
            if rest.len() <= width {
                out.push(rest.iter().collect());
                break;
            }
            let cut = rest[..=width]
                .iter()
                .rposition(|&c| c == ' ')
                .filter(|&pos| pos > 0)
                .unwrap_or(width);
            out.push(
                rest[..cut]
                    .iter()
                    .collect::<String>()
                    .trim_end()
                    .to_string(),
            );
            let skip = if rest.get(cut) == Some(&' ') {
                cut + 1
            } else {
                cut
            };
            rest = rest[skip..].to_vec();
        }
    }
    out
}

/// TEXTSYST (1 = StLB, 2 = STLK) at `system` and the 17-digit STLNR from `from`.
fn put_standard_number(
    line: &mut Line,
    standard: Option<&StandardNumber>,
    system: usize,
    from: usize,
) {
    let Some(standard) = standard else {
        return;
    };
    let digits = standard.number.trim();
    if digits.is_empty() || digits.len() > 17 || !digits.chars().all(|ch| ch.is_ascii_digit()) {
        return;
    }
    match standard.catalog.to_ascii_lowercase().as_str() {
        "stlb" => line.put(system, system, "1"),
        "stlk" => line.put(system, system, "2"),
        _ => {}
    }
    line.put(from, from + 16, digits);
}

/// Nummer einer Ausführungsbeschreibung in Satz 41 (Stellen 3–6) und im Bezug
/// von Satz 28 (Stellen 67–70): numerisch vierstellig mit führenden Nullen wie
/// die übrigen Zahlenfelder („1“ → „0001“); anderes bleibt, wie es ist.
fn perf_number(number: &str) -> String {
    let number = number.trim();
    if !number.is_empty() && number.len() <= 4 && number.bytes().all(|byte| byte.is_ascii_digit()) {
        format!("{number:0>4}")
    } else {
        number.to_string()
    }
}
