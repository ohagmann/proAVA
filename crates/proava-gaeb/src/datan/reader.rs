use std::collections::{BTreeMap, HashMap};

use rust_decimal::Decimal;

use super::encoding::{decode, detect, Encoding};
use super::record::{parse_fixed, OzMask, Record, SplitOz};
use super::{DatanError, ReadReport, Warning};
use crate::model::*;
use crate::text_reflow::{append, reflow, reflow_plain, strip_rtf, tabular};

/// Reads a DATAN file (D81–D86) with automatic character set detection.
pub fn read(bytes: &[u8]) -> Result<ReadReport, DatanError> {
    read_with(bytes, detect(bytes))
}

/// Reads a DATAN file in the given character set.
pub fn read_with(bytes: &[u8], encoding: Encoding) -> Result<ReadReport, DatanError> {
    let text = decode(bytes, encoding);
    let mut reader = Reader::default();
    for (number, line) in split_lines(&text).into_iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        // Control characters inside a record (form feed, escape …) become
        // blanks: the columns stay, nothing strange reaches the project.
        let line: String = line
            .chars()
            .map(|character| {
                if character.is_control() && character != '\t' {
                    ' '
                } else {
                    character
                }
            })
            .collect();
        reader.record(&Record::new(&line, number + 1))?;
    }
    reader.finish(encoding)
}

fn split_lines(text: &str) -> Vec<String> {
    let text = text.trim_end_matches(['\u{1a}', '\0']);
    if !text.contains('\n') && text.chars().count() > 80 {
        let chars: Vec<char> = text.chars().collect();
        return chars.chunks(80).map(|c| c.iter().collect()).collect();
    }
    text.split('\n')
        .map(|l| l.trim_end_matches('\r').to_string())
        .collect()
}

#[derive(Debug, Clone, Copy)]
enum NodeRef {
    Category(usize),
    Item(usize),
    Remark(usize),
    PerfDescr(usize),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Target {
    None,
    Category(usize),
    Item(usize),
    Remark(usize),
    PerfDescr(usize),
    Sub(usize, usize),
}

struct CategorySlot {
    category: Category,
    children: Vec<NodeRef>,
    parent: Option<usize>,
}

/// Line of a text complement: marker (KNR), kind (TEKZ) and text.
struct ComplementLine {
    marker: String,
    kind: char,
    text: String,
}

#[derive(Default)]
struct Header {
    phase: Option<Phase>,
    award_no: String,
    client_dp_no: String,
    lots: bool,
    lv_label: String,
    lv_date: Option<String>,
    open_date: Option<String>,
    open_time: Option<String>,
    award_deadline: Option<String>,
    project_lines: Vec<String>,
    owner_lines: Vec<String>,
    bidder_lines: Vec<String>,
    currency: String,
    up_labels: Vec<String>,
    time_label: String,
    level_labels: BTreeMap<usize, String>,
    contract_text: Vec<String>,
    totals: Option<Totals>,
    declared_items: Option<u32>,
}

struct Reader {
    header: Header,
    mask: Option<OzMask>,
    categories: Vec<CategorySlot>,
    category_index: HashMap<Vec<String>, usize>,
    items: Vec<Item>,
    item_index: HashMap<String, usize>,
    complement_lines: HashMap<usize, Vec<ComplementLine>>,
    remarks: Vec<Remark>,
    perf_descrs: Vec<PerfDescr>,
    root: Vec<NodeRef>,
    pending: Vec<NodeRef>,
    container: Option<usize>,
    lot: Option<String>,
    target: Target,
    warnings: Vec<Warning>,
    skipped: BTreeMap<String, (usize, usize)>,
    /// Items priced by a record 23 already, and how often a record 23 came
    /// again for the same OZ (first line, count) – a sign of cut numbers.
    priced: std::collections::HashSet<usize>,
    repriced: BTreeMap<String, (usize, usize)>,
    /// Longest line seen per text record kind (25, 26, T1, 02).
    widths: HashMap<&'static str, usize>,
}

impl Default for Reader {
    fn default() -> Self {
        Self {
            header: Header::default(),
            mask: None,
            categories: Vec::new(),
            category_index: HashMap::new(),
            items: Vec::new(),
            item_index: HashMap::new(),
            complement_lines: HashMap::new(),
            remarks: Vec::new(),
            perf_descrs: Vec::new(),
            root: Vec::new(),
            pending: Vec::new(),
            container: None,
            lot: None,
            target: Target::None,
            warnings: Vec::new(),
            skipped: BTreeMap::new(),
            priced: Default::default(),
            repriced: BTreeMap::new(),
            widths: HashMap::new(),
        }
    }
}

impl Reader {
    fn text_line(&mut self, rec: &Record, kind: &'static str, from: usize, to: usize) -> String {
        let line = rec.text(from, to);
        let width = self.widths.entry(kind).or_default();
        *width = (*width).max(line.chars().count());
        line
    }

    /// Line length the file was wrapped at, at least three quarters of the
    /// standard length (otherwise the standard length).
    fn width(&self, kind: &str, standard: usize) -> usize {
        let seen = self.widths.get(kind).copied().unwrap_or(0);
        if seen * 4 >= standard * 3 {
            seen
        } else {
            standard
        }
    }

    fn warn(&mut self, line: usize, message: impl Into<String>) {
        self.warnings.push(Warning {
            line,
            message: message.into(),
        });
    }

    fn number(
        &mut self,
        rec: &Record,
        from: usize,
        to: usize,
        decimals: u32,
        sign: Option<usize>,
    ) -> Option<Decimal> {
        match rec.number(from, to, decimals, sign) {
            Ok(value) => value,
            Err(msg) => {
                self.warn(
                    rec.line,
                    format!("Satzart {}, Stelle {from}–{to}: {msg}", rec.kind()),
                );
                None
            }
        }
    }

    fn record(&mut self, rec: &Record) -> Result<(), DatanError> {
        let kind = rec.kind();
        if self.header.phase.is_none() && !matches!(kind.as_str(), "00" | "T0" | "T1" | "T9") {
            if self.header.contract_text.is_empty() && self.warnings.is_empty() && rec.line == 1 {
                return Err(DatanError::NotDatan);
            }
            self.warn(
                rec.line,
                format!("Satzart „{kind}“ vor dem Eröffnungssatz 00 übersprungen"),
            );
            return Ok(());
        }
        match kind.as_str() {
            "T0" | "T9" => {}
            "T1" => {
                let line = self.text_line(rec, "T1", 3, 74);
                self.header.contract_text.push(line);
            }
            "00" => self.opening(rec)?,
            "01" => {
                self.header.lv_label = strip_rtf(&rec.text(3, 42));
                self.header.lv_date = self.date(rec, 43, 50);
                self.header.open_date = self.date(rec, 51, 58);
                self.header.open_time = Some(rec.trimmed(59, 63)).filter(|t| !t.is_empty());
                self.header.award_deadline = self.date(rec, 64, 71);
            }
            "02" => {
                let line = self.text_line(rec, "02", 3, 74);
                self.header.project_lines.push(line);
            }
            "03" => self.header.owner_lines.push(strip_rtf(&rec.text(3, 74))),
            "04" => self.header.bidder_lines.push(strip_rtf(&rec.text(3, 74))),
            "06" => self.up_labels(rec),
            "08" => self.header.currency = rec.trimmed(3, 8),
            "10" => self.lot_start(rec),
            "11" => self.group_start(rec),
            "12" => self.group_label(rec),
            "20" => {
                self.remarks.push(Remark::default());
                self.pending.push(NodeRef::Remark(self.remarks.len() - 1));
                self.target = Target::Remark(self.remarks.len() - 1);
            }
            "21" => self.item_start(rec),
            "22" => self.estimate(rec),
            "23" => self.bid_data(rec),
            "24" => self.sub_description(rec),
            "25" => self.short_text(rec),
            "26" => self.long_text(rec),
            "28" => self.repetition(rec),
            "30" => self.up_components(rec),
            "31" => self.group_end(rec),
            "32" => self.group_totals(rec),
            "33" => self.lot_end(rec),
            "41" => {
                self.perf_descrs.push(PerfDescr {
                    number: perf_number(&rec.trimmed(3, 6)),
                    label: strip_rtf(&rec.text(7, 61)),
                    ..PerfDescr::default()
                });
                self.pending
                    .push(NodeRef::PerfDescr(self.perf_descrs.len() - 1));
                self.target = Target::PerfDescr(self.perf_descrs.len() - 1);
            }
            "99" => self.closing(rec),
            "07" | "27" | "34" | "35" | "42" | "45" | "46" | "47" => {}
            _ => {
                let entry = self.skipped.entry(kind).or_insert((rec.line, 0));
                entry.1 += 1;
            }
        }
        Ok(())
    }

    fn opening(&mut self, rec: &Record) -> Result<(), DatanError> {
        if self.header.phase.is_some() {
            self.warn(rec.line, "Weiterer Eröffnungssatz 00 übersprungen");
            return Ok(());
        }
        let dp = rec.trimmed(11, 12);
        let phase = dp
            .parse::<u8>()
            .ok()
            .and_then(Phase::from_number)
            .ok_or(DatanError::UnknownPhase(dp))?;
        if rec.raw(63, 71).trim().is_empty() {
            // Seen in a D84 from a bidder's program: no mask, and the OZ cut to
            // the titles – the positions cannot be assigned.
            return Err(DatanError::InvalidMask(
                "Die Datei nennt keine OZ-Maske (Satz 00, Stellen 63–71); ohne sie lassen sich die \
                 Ordnungszahlen nicht zuordnen – bitte beim Absender eine vollständige Datei anfordern"
                    .into(),
            ));
        }
        let mask = OzMask::parse(&rec.raw(63, 71)).map_err(DatanError::InvalidMask)?;
        self.header.phase = Some(phase);
        self.header.award_no = rec.trimmed(14, 28);
        self.header.client_dp_no = rec.trimmed(29, 36);
        self.header.lots = rec.flag(74) == 'X';
        self.mask = Some(mask);
        Ok(())
    }

    fn date(&mut self, rec: &Record, from: usize, to: usize) -> Option<String> {
        let raw = rec.trimmed(from, to);
        if raw.is_empty() {
            return None;
        }
        let parts: Vec<&str> = raw.split('.').collect();
        let parsed = (parts.len() == 3)
            .then(|| {
                let day: u32 = parts[0].trim().parse().ok()?;
                let month: u32 = parts[1].trim().parse().ok()?;
                let mut year: u32 = parts[2].trim().parse().ok()?;
                if parts[2].trim().len() <= 2 {
                    year += if year < 80 { 2000 } else { 1900 };
                }
                ((1..=31).contains(&day) && (1..=12).contains(&month))
                    .then(|| format!("{year:04}-{month:02}-{day:02}"))
            })
            .flatten();
        if parsed.is_none() {
            self.warn(rec.line, format!("Datum „{raw}“ nicht lesbar"));
        }
        parsed
    }

    fn up_labels(&mut self, rec: &Record) {
        let count = rec.trimmed(3, 3).parse::<usize>().unwrap_or(4).min(4);
        self.header.time_label = rec.trimmed(4, 17);
        let labels: Vec<String> = [18, 32, 46, 60]
            .iter()
            .map(|&from| rec.trimmed(from, from + 13))
            .collect();
        let mut labels: Vec<String> = labels.into_iter().take(count).collect();
        while labels.last().is_some_and(|l| l.is_empty()) {
            labels.pop();
        }
        self.header.up_labels = labels;
    }

    fn split(&self, rec: &Record) -> SplitOz {
        self.mask
            .as_ref()
            .map(|mask| mask.split(&rec.raw(3, 11)))
            .unwrap_or_default()
    }

    fn with_lot(&self, path: Vec<String>) -> Vec<String> {
        match &self.lot {
            Some(lot) => std::iter::once(lot.clone()).chain(path).collect(),
            None => path,
        }
    }

    /// Finds or creates the categories along `path`; `None` = LV root.
    fn ensure_path(&mut self, path: &[String]) -> Option<usize> {
        let mut parent: Option<usize> = None;
        for depth in 1..=path.len() {
            let key = path[..depth].to_vec();
            let index = match self.category_index.get(&key) {
                Some(&index) => index,
                None => {
                    let index = self.categories.len();
                    self.categories.push(CategorySlot {
                        category: Category {
                            rno_part: path[depth - 1].clone(),
                            ..Category::default()
                        },
                        children: Vec::new(),
                        parent,
                    });
                    self.category_index.insert(key, index);
                    self.children_mut(parent).push(NodeRef::Category(index));
                    index
                }
            };
            parent = Some(index);
        }
        parent
    }

    fn children_mut(&mut self, container: Option<usize>) -> &mut Vec<NodeRef> {
        match container {
            Some(index) => &mut self.categories[index].children,
            None => &mut self.root,
        }
    }

    fn flush_pending(&mut self, container: Option<usize>) {
        let pending = std::mem::take(&mut self.pending);
        self.children_mut(container).extend(pending);
    }

    fn lot_start(&mut self, rec: &Record) {
        let lot = rec.trimmed(3, 4);
        let lot = format!("{lot:0>2}");
        self.flush_pending(None);
        self.header.lots = true;
        self.lot = None;
        let index = self.ensure_path(std::slice::from_ref(&lot));
        if let Some(index) = index {
            self.categories[index].category.label = strip_rtf(&rec.text(5, 44));
        }
        self.lot = Some(lot);
        self.container = index;
        self.target = Target::None;
    }

    fn lot_end(&mut self, rec: &Record) {
        let lot = format!("{:0>2}", rec.trimmed(3, 4));
        self.flush_pending(self.container);
        let total = self.number(rec, 5, 16, 2, None);
        let factor = self.number(rec, 17, 23, 6, None);
        let vat = self.number(rec, 24, 27, 2, None);
        let gross = self.number(rec, 28, 39, 2, None);
        let amount = self.number(rec, 41, 52, 2, Some(40));
        let lump = self.number(rec, 54, 65, 2, Some(53));
        self.lot = None;
        if let Some(index) = self.ensure_path(&[lot]) {
            self.categories[index].category.totals =
                Some(make_totals(total, factor, amount, vat, gross, lump));
        }
        self.container = None;
        self.target = Target::None;
    }

    fn group_start(&mut self, rec: &Record) {
        let split = self.split(rec);
        let path = self.with_lot(split.path());
        if split.path().is_empty() {
            self.warn(rec.line, "LV-Gruppe ohne Ordnungszahl übersprungen");
            return;
        }
        let keyword = rec.trimmed(17, 30);
        if !keyword.is_empty() {
            self.header
                .level_labels
                .entry(split.path().len())
                .or_insert(keyword);
        }
        let parent = self.ensure_path(&path[..path.len() - 1]);
        self.flush_pending(parent);
        let index = self.ensure_path(&path);
        self.container = index;
        self.target = index.map_or(Target::None, Target::Category);
    }

    fn group_label(&mut self, rec: &Record) {
        let Target::Category(index) = self.target else {
            self.warn(rec.line, "Bezeichnung (12) ohne LV-Gruppe übersprungen");
            return;
        };
        let label = &mut self.categories[index].category.label;
        if !label.is_empty() {
            label.push('\n');
        }
        label.push_str(&strip_rtf(&rec.text(3, 74)));
    }

    fn group_end(&mut self, rec: &Record) {
        self.flush_pending(self.container);
        let path = self.with_lot(self.split(rec).path());
        if let Some(&index) = self.category_index.get(&path) {
            self.container = self.categories[index].parent;
        }
        self.target = Target::None;
    }

    fn group_totals(&mut self, rec: &Record) {
        self.flush_pending(self.container);
        let path = self.with_lot(self.split(rec).path());
        let total = self.number(rec, 13, 24, 2, Some(12));
        let factor = self.number(rec, 25, 31, 6, None);
        let amount = self.number(rec, 33, 44, 2, Some(32));
        let lump = self.number(rec, 46, 57, 2, Some(45));
        if path.is_empty() {
            self.warn(rec.line, "Summe (32) ohne Ordnungszahl übersprungen");
            return;
        }
        if let Some(index) = self.ensure_path(&path) {
            self.categories[index].category.totals =
                Some(make_totals(total, factor, amount, None, None, lump));
        }
        self.target = Target::None;
    }

    /// Item for the OZ of this record: the current one, a known one or a new one.
    fn item_for(&mut self, rec: &Record) -> Option<usize> {
        let split = self.split(rec);
        if split.item.is_empty() {
            self.warn(
                rec.line,
                format!("Satzart {} ohne Positionsnummer übersprungen", rec.kind()),
            );
            return None;
        }
        let key = self.with_lot(vec![split.dotted()]).join(".");
        if let Some(&index) = self.item_index.get(&key) {
            return Some(index);
        }
        let container = self.ensure_path(&self.with_lot(split.path()));
        self.flush_pending(container);
        let index = self.items.len();
        self.items.push(Item {
            rno_part: split.item,
            rno_index: split.index,
            ..Item::default()
        });
        self.item_index.insert(key, index);
        self.children_mut(container).push(NodeRef::Item(index));
        self.container = container;
        Some(index)
    }

    fn item_start(&mut self, rec: &Record) {
        let Some(index) = self.item_for(rec) else {
            return;
        };
        let qty = self.number(rec, 24, 34, 3, Some(23));
        let item = &mut self.items[index];
        let zz: Vec<char> = rec.raw(15, 18).chars().collect();
        let group = zz[..3].iter().collect::<String>().trim().to_string();
        let group = if !group.is_empty() && group.chars().all(|c| c.is_ascii_digit()) {
            format!("{group:0>3}")
        } else {
            group
        };
        let serial = zz[3..].iter().collect::<String>().trim().to_string();
        match rec.flag(12) {
            'G' => {
                item.kind.alternative = Some(Alternative {
                    group,
                    serial: "0".into(),
                })
            }
            'A' => {
                item.kind.alternative = Some(Alternative {
                    group,
                    serial: if serial.is_empty() {
                        "1".into()
                    } else {
                        serial
                    },
                })
            }
            'S' => item.kind.hourly = true,
            _ => {}
        }
        item.kind.provisional = match rec.flag(13) {
            'E' => Some(Provisional::WithoutTotal),
            'M' => Some(Provisional::WithTotal),
            _ => None,
        };
        item.kind.markup = rec.flag(20) == 'Z';
        item.qty_to_be_determined = rec.flag(22) == 'X';
        if qty.is_some_and(|q| !(item.qty_to_be_determined && q.is_zero())) {
            item.qty = qty;
        }
        item.unit = rec.trimmed(35, 38);
        item.kind.lump_sum = item.unit.eq_ignore_ascii_case("psch");
        item.standard_number = standard_number(rec, 44, 45, 61);
        self.target = Target::Item(index);
    }

    fn unit_price(&mut self, rec: &Record) -> Option<Decimal> {
        let zpf = rec.flag(23);
        let value = if zpf.is_ascii_digit() {
            let raw = format!("{}{zpf}", rec.raw(13, 22));
            parse_fixed(&raw, 3).map_err(|e| (e, 3))
        } else {
            parse_fixed(&rec.raw(13, 22), 2).map_err(|e| (e, 2))
        };
        match value {
            Ok(Some(v)) => Some(if rec.flag(12) == '-' { -v } else { v }),
            Ok(None) => None,
            Err((msg, _)) => {
                self.warn(rec.line, format!("Einheitspreis: {msg}"));
                None
            }
        }
    }

    fn estimate(&mut self, rec: &Record) {
        let Some(index) = self.item_for(rec) else {
            return;
        };
        if self.header.phase == Some(Phase::Dp82) {
            let up = self.unit_price(rec);
            let total = self.number(rec, 25, 36, 2, Some(24));
            let item = &mut self.items[index];
            item.unit_price = up;
            item.total = total;
        }
        self.target = Target::Item(index);
    }

    fn bid_data(&mut self, rec: &Record) {
        let Some(index) = self.item_for(rec) else {
            return;
        };
        if !self.priced.insert(index) {
            let oz = self.split(rec).dotted();
            self.repriced.entry(oz).or_insert((rec.line, 0)).1 += 1;
        }
        let up = self.unit_price(rec);
        let total = self.number(rec, 25, 36, 2, Some(24));
        let free_qty = self.number(rec, 38, 48, 3, Some(37));
        let factor = self.number(rec, 49, 55, 6, None);
        let markup = self.number(rec, 57, 61, 2, Some(56));
        let markup_base = self.number(rec, 63, 74, 2, Some(62));
        let item = &mut self.items[index];
        item.unit_price = up;
        item.total = total;
        if free_qty.is_some() && (item.qty.is_none() || item.qty_to_be_determined) {
            item.qty = free_qty;
        }
        item.discount_percent = factor.and_then(factor_to_discount);
        if markup.is_some() || markup_base.is_some() {
            item.kind.markup = true;
            item.markup_percent = markup;
            item.markup_base = markup_base;
        }
        self.target = Target::Item(index);
    }

    fn up_components(&mut self, rec: &Record) {
        let Target::Item(index) = self.target else {
            self.warn(rec.line, "Preisanteile (30) ohne Position übersprungen");
            return;
        };
        let time = self.number(rec, 3, 12, 4, None);
        let mut parts: Vec<Option<Decimal>> = [13, 23, 33, 43]
            .iter()
            .map(|&from| self.number(rec, from, from + 9, 2, None))
            .collect();
        let labels = self.header.up_labels.len();
        if labels > 0 {
            parts.truncate(labels);
        } else {
            while parts.last() == Some(&None) {
                parts.pop();
            }
        }
        let item = &mut self.items[index];
        item.time_quantity = time;
        item.up_components = parts;
    }

    fn sub_description(&mut self, rec: &Record) {
        let Some(index) = self.current_item() else {
            self.warn(
                rec.line,
                "Unterbeschreibung (24) ohne Position übersprungen",
            );
            return;
        };
        let qty = self.number(rec, 6, 16, 3, Some(5));
        let standard_number = standard_number(rec, 23, 24, 40);
        let item = &mut self.items[index];
        item.sub_descriptions.push(SubDescription {
            number: rec.trimmed(3, 4),
            qty,
            unit: rec.trimmed(17, 20),
            standard_number,
            ..SubDescription::default()
        });
        self.target = Target::Sub(index, item.sub_descriptions.len() - 1);
    }

    fn current_item(&self) -> Option<usize> {
        match self.target {
            Target::Item(index) | Target::Sub(index, _) => Some(index),
            _ => None,
        }
    }

    fn short_text(&mut self, rec: &Record) {
        let line = self.text_line(rec, "25", 3, 72);
        let text = match self.target {
            Target::Item(i) => &mut self.items[i].short_text,
            Target::Sub(i, s) => &mut self.items[i].sub_descriptions[s].short_text,
            Target::PerfDescr(i) => &mut self.perf_descrs[i].short_text,
            Target::Remark(i) => {
                push_line(&mut self.remarks[i].text, line);
                return;
            }
            Target::Category(_) | Target::None => {
                self.warn(rec.line, "Kurztext (25) ohne Bezug übersprungen");
                return;
            }
        };
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&line);
    }

    fn long_text(&mut self, rec: &Record) {
        let marker = rec.trimmed(3, 4);
        let kind = rec.flag(5);
        let line = rec.text(6, 74);
        let rich = match self.target {
            Target::Item(i) | Target::Sub(i, _)
                if !marker.is_empty() || matches!(kind, 'A' | 'B') =>
            {
                self.complement_lines
                    .entry(i)
                    .or_default()
                    .push(ComplementLine {
                        marker,
                        kind,
                        text: strip_rtf(&line),
                    });
                return;
            }
            Target::Item(i) => &mut self.items[i].long_text,
            Target::Sub(i, s) => &mut self.items[i].sub_descriptions[s].long_text,
            Target::PerfDescr(i) => &mut self.perf_descrs[i].long_text,
            Target::Remark(i) => &mut self.remarks[i].text,
            Target::Category(_) | Target::None => {
                self.warn(rec.line, "Langtext (26) ohne Bezug übersprungen");
                return;
            }
        };
        push_line(rich, line.clone());
        let width = self.widths.entry("26").or_default();
        *width = (*width).max(line.chars().count());
    }

    fn repetition(&mut self, rec: &Record) {
        let Some(index) = self.current_item() else {
            return;
        };
        let oz = rec.raw(58, 66);
        let reference = self
            .mask
            .as_ref()
            .map(|mask| mask.split(&oz).dotted())
            .filter(|dotted| !dotted.is_empty());
        let perf_no = perf_number(&rec.trimmed(67, 70));
        let item = &mut self.items[index];
        if reference.is_some() {
            item.kind.repetition_of = reference;
        }
        if !perf_no.is_empty() {
            item.kind.reference_description = Some(perf_no);
        }
    }

    fn closing(&mut self, rec: &Record) {
        self.flush_pending(self.container);
        let total = self.number(rec, 3, 14, 2, None);
        let factor = self.number(rec, 15, 21, 6, None);
        let vat = self.number(rec, 22, 25, 2, None);
        let gross = self.number(rec, 26, 37, 2, None);
        let amount = self.number(rec, 39, 50, 2, Some(38));
        let lump = self.number(rec, 52, 63, 2, Some(51));
        self.header.declared_items = rec.trimmed(70, 74).parse().ok();
        self.header.totals = Some(make_totals(total, factor, amount, vat, gross, lump));
        self.target = Target::None;
    }

    fn finish(mut self, encoding: Encoding) -> Result<ReadReport, DatanError> {
        let Some(phase) = self.header.phase else {
            return Err(DatanError::NotDatan);
        };
        self.flush_pending(self.container);
        for (kind, (line, count)) in std::mem::take(&mut self.skipped) {
            self.warn(
                line,
                format!("Unbekannte Satzart „{kind}“ übersprungen ({count}-mal)"),
            );
        }
        let repriced = std::mem::take(&mut self.repriced);
        if let Some((_, (line, _))) = repriced.iter().min_by_key(|(_, (line, _))| *line) {
            let records: usize = repriced.values().map(|(_, count)| count).sum();
            let mut examples: Vec<(&String, usize)> = repriced
                .iter()
                .map(|(oz, (_, count))| (oz, count + 1))
                .collect();
            examples.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            let examples: Vec<String> = examples
                .iter()
                .take(3)
                .map(|(oz, count)| format!("{oz} {count}×"))
                .collect();
            self.warn(
                *line,
                format!(
                    "ACHTUNG: {records} Preissätze (23) wiederholen die Ordnungszahl einer früheren \
                     Position ({} {}, z. B. {}) – die Positionsnummern sind vermutlich \
                     gekürzt (GAEB 90 fasst höchstens 9 Stellen); die Preise lassen sich nicht \
                     sicher zuordnen, es gilt jeweils der letzte Satz. Bitte das Angebot als GAEB \
                     DA XML (X84) anfordern.",
                    repriced.len(),
                    if repriced.len() == 1 { "Ordnungszahl" } else { "Ordnungszahlen" },
                    examples.join(", ")
                ),
            );
        }
        for (index, lines) in std::mem::take(&mut self.complement_lines) {
            self.items[index].complements = group_complements(lines);
        }
        self.reflow_texts();

        let mask = self.mask.clone().expect("mask set with phase");
        let mut breakdown = mask.breakdown();
        let mut level = 0;
        for entry in &mut breakdown {
            if entry.kind == BreakdownKind::Level {
                level += 1;
                if let Some(label) = self.header.level_labels.get(&level) {
                    entry.label = label.clone();
                }
            }
        }
        if self.header.lots {
            breakdown.insert(
                0,
                BreakdownLevel {
                    kind: BreakdownKind::Lot,
                    label: "Los".into(),
                    length: 2,
                    numeric: true,
                },
            );
        }

        let mut tree = Tree {
            categories: self.categories.into_iter().map(Some).collect(),
            items: self.items.into_iter().map(Some).collect(),
            remarks: self.remarks.into_iter().map(Some).collect(),
            perf_descrs: self.perf_descrs.into_iter().map(Some).collect(),
        };
        let body = tree.build(&self.root);

        let header = self.header;
        let address = |lines: &[String]| {
            let lines: Vec<&String> = lines.iter().filter(|l| !l.trim().is_empty()).collect();
            (!lines.is_empty()).then(|| Address {
                name1: lines[0].trim().to_string(),
                name2: lines
                    .iter()
                    .skip(1)
                    .map(|l| l.trim())
                    .collect::<Vec<_>>()
                    .join(" "),
                ..Address::default()
            })
        };
        let document = GaebDocument {
            phase,
            program: String::new(),
            date: header.lv_date.clone(),
            project: ProjectInfo {
                name: if header.award_no.is_empty() {
                    header.client_dp_no.clone()
                } else {
                    header.award_no.clone()
                },
                label: header.project_lines.join("\n").trim_end().to_string(),
                currency: header.currency.clone(),
                ..ProjectInfo::default()
            },
            award: Award {
                owner: address(&header.owner_lines),
                bidder: address(&header.bidder_lines),
                bid_date: None,
                submission: Submission {
                    open_date: header.open_date.clone(),
                    open_time: header.open_time.clone(),
                    award_deadline: header.award_deadline.clone(),
                    ..Submission::default()
                },
                boq: Boq {
                    name: String::new(),
                    label: header.lv_label.clone(),
                    breakdown,
                    body,
                    totals: header.totals.clone(),
                    up_component_labels: header.up_labels.clone(),
                    time_label: header.time_label.clone(),
                    ..Boq::default()
                },
                award_no: header.award_no.clone(),
                contract_text: header.contract_text.clone(),
                ..Award::default()
            },
        };
        Ok(ReadReport {
            document,
            encoding,
            warnings: self.warnings,
            declared_item_count: header.declared_items,
            file_totals: header.totals,
        })
    }
}

struct Tree {
    categories: Vec<Option<CategorySlot>>,
    items: Vec<Option<Item>>,
    remarks: Vec<Option<Remark>>,
    perf_descrs: Vec<Option<PerfDescr>>,
}

impl Tree {
    fn build(&mut self, refs: &[NodeRef]) -> Vec<BoqNode> {
        refs.iter()
            .filter_map(|node| match *node {
                NodeRef::Category(i) => {
                    let slot = self.categories[i].take()?;
                    let mut category = slot.category;
                    category.body = self.build(&slot.children);
                    Some(BoqNode::Category(category))
                }
                NodeRef::Item(i) => self.items[i].take().map(BoqNode::Item),
                NodeRef::Remark(i) => self.remarks[i].take().map(BoqNode::Remark),
                NodeRef::PerfDescr(i) => self.perf_descrs[i].take().map(BoqNode::PerfDescr),
            })
            .collect()
    }
}

impl Reader {
    /// Joins the fixed GAEB 90 lines of all texts into paragraphs.
    fn reflow_texts(&mut self) {
        let long = self.width("26", 55);
        let short = self.width("25", 70);
        let rich = |text: &mut RichText| {
            let lines: Vec<String> = text
                .blocks
                .iter()
                .map(|block| match block {
                    TextBlock::Paragraph { runs, .. } => {
                        runs.iter().map(|r| r.text.as_str()).collect()
                    }
                    _ => String::new(),
                })
                .collect();
            text.blocks = reflow(&lines, long);
        };
        let plain = |text: &mut String| {
            if !text.is_empty() {
                let lines: Vec<String> = text.split('\n').map(str::to_string).collect();
                let mut lines = reflow_plain(&lines, short);
                // A line break at the end (empty last records 25, as some
                // programs pad the Kurztext) carries nothing: left out. Empty
                // lines in the middle stay.
                while lines.last().is_some_and(|line| line.trim().is_empty()) {
                    lines.pop();
                }
                *text = lines.join("\n");
            }
        };
        for item in &mut self.items {
            rich(&mut item.long_text);
            plain(&mut item.short_text);
            for sub in &mut item.sub_descriptions {
                rich(&mut sub.long_text);
                plain(&mut sub.short_text);
            }
        }
        for remark in &mut self.remarks {
            rich(&mut remark.text);
        }
        for perf in &mut self.perf_descrs {
            rich(&mut perf.long_text);
            plain(&mut perf.short_text);
        }
        let contract = self.width("T1", 72);
        self.header.contract_text = reflow_plain(&self.header.contract_text, contract);
        let project = self.width("02", 60);
        let lines = reflow_plain(&self.header.project_lines, project);
        let first = lines
            .iter()
            .position(|l| !l.trim().is_empty())
            .unwrap_or(lines.len());
        self.header.project_lines = lines[first..].to_vec();
    }
}

fn push_line(text: &mut RichText, line: String) {
    text.blocks.push(TextBlock::paragraph(vec![Run {
        text: line,
        ..Run::default()
    }]));
}

/// AUFABFAK (factor, 1 = none) as discount percent, positive = Nachlass.
fn factor_to_discount(factor: Decimal) -> Option<Decimal> {
    let percent = ((Decimal::ONE - factor) * Decimal::ONE_HUNDRED).normalize();
    (!percent.is_zero()).then_some(percent)
}

fn make_totals(
    total: Option<Decimal>,
    factor: Option<Decimal>,
    amount: Option<Decimal>,
    vat: Option<Decimal>,
    gross: Option<Decimal>,
    lump_sum: Option<Decimal>,
) -> Totals {
    let discount_percent = factor.and_then(factor_to_discount);
    let discount_amount = amount.filter(|a| !a.is_zero()).map(|a| -a);
    let total_after_discount = match (total, factor, discount_amount) {
        (Some(t), Some(f), _) if discount_percent.is_some() => Some((t * f).round_dp(2)),
        (Some(t), _, Some(a)) => Some(t - a),
        _ => None,
    };
    Totals {
        total,
        discount_percent,
        total_after_discount,
        vat_percent: vat,
        total_gross: gross,
        discount_amount,
        lump_sum: lump_sum.filter(|l| !l.is_zero()),
        ..Totals::default()
    }
}

/// Groups consecutive lines with the same marker into one complement:
/// caption = text before the first `'`, body = text between the quotes,
/// tail = text after the closing quote. The lines are joined like flowing
/// text (a blank, a split word loses its hyphen): a Textergänzung breaks only
/// because of the fixed record length, not for a new paragraph (Anwender
/// 18.11.; other programs show „Luft Filterwächter“ where the file has
/// „Luft“ / „Filterwächter“ on two records). As in flowing text, tabular
/// lines (tab or a wide gap – a bidder's product list, one per record) keep
/// their line breaks.
fn group_complements(lines: Vec<ComplementLine>) -> Vec<TextComplement> {
    let mut groups: Vec<(String, char, Vec<String>)> = Vec::new();
    for line in lines {
        match groups.last_mut() {
            Some((marker, kind, texts)) if *marker == line.marker && *kind == line.kind => {
                texts.push(line.text)
            }
            _ => groups.push((line.marker, line.kind, vec![line.text])),
        }
    }
    groups
        .into_iter()
        .map(|(marker, kind, texts)| {
            let mut joined = String::new();
            let mut previous = "";
            for text in texts.iter().map(|text| text.trim()) {
                if text.is_empty() {
                    continue;
                }
                if joined.is_empty() {
                    joined.push_str(text);
                } else if tabular(previous) || tabular(text) {
                    joined.push('\n');
                    joined.push_str(text);
                } else {
                    append(&mut joined, text);
                }
                previous = text;
            }
            let (caption, body, tail) = match (joined.find('\''), joined.rfind('\'')) {
                (Some(first), Some(last)) if last > first => (
                    &joined[..first],
                    &joined[first + 1..last],
                    &joined[last + 1..],
                ),
                (Some(first), _) => (&joined[..first], &joined[first + 1..], ""),
                _ => ("", joined.as_str(), ""),
            };
            let body = body.trim_matches(|c: char| c == '\n' || c == ' ');
            let placeholder = body.chars().all(|c| matches!(c, '.' | ' ' | '\n'));
            TextComplement {
                marker,
                kind: match kind {
                    'A' => "Owner".into(),
                    'B' => "Bidder".into(),
                    _ => String::new(),
                },
                caption: caption.trim().to_string(),
                body: if placeholder {
                    String::new()
                } else {
                    body.to_string()
                },
                // Text after the closing apostrophe (Nachsatz, e.g. „ mm,“).
                tail: tail.trim().to_string(),
                ..TextComplement::default()
            }
        })
        .collect()
}

/// TEXTSYST (1 = StLB, 2 = STLK) and STLNR of records 21 and 24.
fn standard_number(rec: &Record, system: usize, from: usize, to: usize) -> Option<StandardNumber> {
    let number = rec.trimmed(from, to);
    (!number.is_empty()).then(|| StandardNumber {
        number,
        catalog: match rec.flag(system) {
            '1' => "StLB",
            '2' => "StLK",
            _ => "",
        }
        .to_string(),
    })
}

/// Nummer einer Ausführungsbeschreibung (Satz 41) bzw. Bezug darauf (Satz 28)
/// ohne führende Nullen – „0001“ (vierstellig, wie es die Zahlenfelder
/// verlangen), „   1“ und „1   “ meinen dieselbe; so passen Satz 41 und
/// Satz 28 auch bei unterschiedlicher Schreibweise zusammen (wie PerfNo in DA XML).
fn perf_number(raw: &str) -> String {
    let number = raw.trim();
    if !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()) {
        let trimmed = number.trim_start_matches('0');
        if trimmed.is_empty() { "0" } else { trimmed }.to_string()
    } else {
        number.to_string()
    }
}
