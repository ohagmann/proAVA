//! Keys of catalogue texts: STLB-Bau – Dynamische BauDaten and the older
//! Standardleistungsnummer (StLB/STLK). A position keeps them so that STLB-Bau
//! (or any AVA connected to it) can open, check and update the text later.

use rust_decimal::Decimal;

use crate::cost_group::parse_cost_group;
use crate::edit_status::EditStatus;
use crate::lv::{Lv, LvRow};
use crate::position::{ComplementKind, Position, PositionKind};

/// Key of a text built with STLB-Bau (GAEB `STLBBau`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StlbReference {
    /// Catalogue name, normally "STLB-Bau".
    pub catalog: String,
    /// Catalogue version `YYYY-MM`, e.g. "2026-04".
    pub version: Option<String>,
    /// Leistungsbereich, e.g. "070".
    pub work_category: String,
    /// Identifier of the Leistung (group) within the Leistungsbereich.
    pub group: i64,
    /// Cost group (DIN 276), may be empty.
    pub cost_group: String,
    /// Chosen characteristics; their order has no meaning.
    pub characteristics: Vec<StlbCharacteristic>,
    /// The text has Textergänzungen of the Ausschreibender (TA) …
    pub owner_complements: bool,
    /// … or of the Bieter (TB) – for the catalogue line in print.
    pub bidder_complements: bool,
    /// Fingerprint of the Langtext as STLB-Bau delivered it ([`text_fingerprint`],
    /// the Kurztext stays free); None when unknown. See
    /// [`Position::stlb_text_changed`].
    pub text_fingerprint: Option<u64>,
    /// Fingerprint over Kurz- and Langtext from files written before the
    /// Kurztext was free ([`legacy_text_fingerprint`]); only kept when it no
    /// longer matched on loading (see [`Position::upgrade_stlb_fingerprint`]).
    pub legacy_fingerprint: Option<u64>,
    /// Textergänzung (by its marker) → characteristic it belongs to (GAEB
    /// attribute `ArtChrIdent` of `TextComplement`).
    pub complement_links: Vec<(String, i64)>,
    /// Textergänzungen (by marker) that hold the value of a numeric
    /// characteristic (GAEB `ComplBodyDec`/`ComplBodyInt`); the number is the
    /// filled-in text. STLB-Bau Check reports them missing as „numerisch
    /// unvollständig“.
    pub complement_numbers: Vec<(String, ComplementNumber)>,
}

/// Value type of a numeric characteristic filled in by a Textergänzung.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComplementNumber {
    Decimal,
    Integer,
}

impl ComplementNumber {
    /// Value in the project file.
    pub fn key(self) -> &'static str {
        match self {
            ComplementNumber::Decimal => "decimal",
            ComplementNumber::Integer => "integer",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "decimal" => Some(ComplementNumber::Decimal),
            "integer" => Some(ComplementNumber::Integer),
            _ => None,
        }
    }

    /// The number in a filled-in text („1,5“, „1.5“, „1.000,5“, „220“) as
    /// GAEB writes it („1.5“); None when it is no number of this kind. Same
    /// rule as the GAEB writer, which leaves `ComplBodyDec` out then.
    pub fn value_of(self, body: &str) -> Option<String> {
        let text: String = body.chars().filter(|ch| !ch.is_whitespace()).collect();
        let text = match (text.contains(','), text.contains('.')) {
            (true, true) => text.replace('.', "").replace(',', "."),
            (true, false) => text.replace(',', "."),
            _ => text,
        };
        let (sign, digits) = match text.strip_prefix('-') {
            Some(rest) => ("-", rest),
            None => ("", text.strip_prefix('+').unwrap_or(&text)),
        };
        let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
        let numeric = |part: &str| part.chars().all(|ch| ch.is_ascii_digit());
        if whole.is_empty() || !numeric(whole) || !numeric(fraction) {
            return None;
        }
        if digits.contains('.') && fraction.is_empty() {
            return None;
        }
        match self {
            ComplementNumber::Integer if !fraction.is_empty() => None,
            _ if fraction.is_empty() => Some(format!("{sign}{whole}")),
            _ => Some(format!("{sign}{whole}.{fraction}")),
        }
    }
}

/// One chosen characteristic (GAEB `STLBBauID`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StlbCharacteristic {
    /// Beschreibungsmerkmal.
    pub characteristic: i64,
    /// Index to the characteristic; none for text complements.
    pub index: Option<i64>,
    /// Chosen value (Ausprägung); none for text complements.
    pub value: Option<i64>,
    /// The characteristic contributes to the Kurztext.
    pub short_text_part: bool,
}

/// Catalogue keys of a text that is not a position (Unterbeschreibung,
/// Ausführungsbeschreibung): at most one of both is set.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CatalogKeys {
    pub stlb: Option<StlbReference>,
    pub standard_number: Option<StandardNumber>,
}

impl CatalogKeys {
    pub fn is_empty(&self) -> bool {
        self.stlb.is_none() && self.standard_number.is_none()
    }

    /// Catalogue line to print above the text, if any.
    pub fn print_line(&self) -> Option<String> {
        self.stlb
            .as_ref()
            .map(StlbReference::print_line)
            .or_else(|| {
                self.standard_number
                    .as_ref()
                    .map(StandardNumber::print_line)
            })
    }
}

/// Standardleistungsnummer of an older catalogue text (GAEB `StLNo`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StandardNumber {
    /// Digits only, at most 17.
    pub number: String,
    /// "StLB", "StLK", "STLB-BauZ" or empty.
    pub catalog: String,
}

impl StlbReference {
    /// Catalogue line printed above the Langtext (GAEB DA XML 3.3, 4.6.10;
    /// GAEB-FAQ: „STLB-Bau 10/2001 012 TA“).
    pub fn print_line(&self) -> String {
        let mut parts = vec![if self.catalog.trim().is_empty() {
            "STLB-Bau".to_string()
        } else {
            self.catalog.trim().to_string()
        }];
        if let Some(version) = self.version.as_deref().and_then(month_year) {
            parts.push(version);
        }
        if !self.work_category.trim().is_empty() {
            parts.push(self.work_category.trim().to_string());
        }
        if self.owner_complements {
            parts.push("TA".into());
        }
        if self.bidder_complements {
            parts.push("TB".into());
        }
        parts.join(" ")
    }

    /// Short description for the screen, e.g. „STLB-Bau 4/2026 · LB 070 · Leistung 9001 · 3 Merkmale“.
    pub fn summary(&self) -> String {
        let mut text = if self.catalog.trim().is_empty() {
            "STLB-Bau".to_string()
        } else {
            self.catalog.trim().to_string()
        };
        if let Some(version) = self.version.as_deref().and_then(month_year) {
            text.push(' ');
            text.push_str(&version);
        }
        if !self.work_category.trim().is_empty() {
            text.push_str(&format!(" · LB {}", self.work_category.trim()));
        }
        text.push_str(&format!(" · Leistung {}", self.group));
        if !self.cost_group.trim().is_empty() {
            text.push_str(&format!(" · KG {}", self.cost_group.trim()));
        }
        text.push_str(&format!(" · {} Merkmale", self.characteristics.len()));
        text
    }
}

impl StandardNumber {
    /// Catalogue line printed above the Langtext, e.g. „StLB 12345678901234567“.
    pub fn print_line(&self) -> String {
        let catalog = if self.catalog.trim().is_empty() {
            "StL-Nr."
        } else {
            self.catalog.trim()
        };
        format!("{catalog} {}", self.number.trim())
    }
}

/// "2026-04" → "4/2026" as in the GAEB examples.
fn month_year(version: &str) -> Option<String> {
    let (year, month) = version.trim().split_once('-')?;
    let month: u32 = month.parse().ok()?;
    (year.len() == 4 && year.chars().all(|ch| ch.is_ascii_digit()) && (1..=12).contains(&month))
        .then(|| format!("{month}/{year}"))
}

/// FNV-1a over the Langtext (whitespace collapsed, formatting ignored):
/// stable across program versions, so it can be stored. The Kurztext of an
/// STLB-Bau position is free (Anwender 18.11.) and does not count.
pub fn text_fingerprint(position: &Position) -> u64 {
    fnv(&format!("\u{1e}{}", position.long_text().to_plain()))
}

/// The earlier fingerprint over Kurz- and Langtext (project files written
/// before the Kurztext was free) – only for reading them.
pub fn legacy_text_fingerprint(position: &Position) -> u64 {
    fnv(&format!(
        "{}\u{1f}{}",
        position.short_text(),
        position.long_text().to_plain()
    ))
}

fn fnv(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    for byte in normalized.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// Why the Langtext of an STLB-Bau position cannot be edited, and how.
pub const STLB_LOCKED: &str =
    "Langtext aus STLB-Bau – schreibgeschützt (Kurztext frei). Ändern über STLB-Bau \
(Prüfdatei ausgeben, aktualisieren) oder „In freie Position umwandeln …“ (auch Rechtsklick › \
Bearbeiten › Umwandeln in).";

/// Finding for a Langtext that differs from STLB-Bau („STLB-Bau*“).
pub const STLB_CHANGED: &str =
    "Langtext weicht von STLB-Bau ab (STLB-Bau*) – umwandeln oder aus STLB-Bau \
aktualisieren; der GAEB-Export schreibt die Position ohne STLB-Bau-Kennung.";

/// Line about the STLB-Bau key of a position, e.g. „STLB-Bau 10/2026 · LB
/// 070 · Leistung 3578 · KG 482 · 8 Merkmale“ – the KG from the key, else
/// the one the file assigned or the position's own. None without key.
pub fn key_line(position: &Position) -> Option<String> {
    let stlb = position.stlb_reference()?;
    if !stlb.cost_group.trim().is_empty() {
        return Some(stlb.summary());
    }
    let cost_group =
        catalog_cost_group(position).or_else(|| position.cost_group().map(str::to_string));
    let mut key = stlb.clone();
    key.cost_group = cost_group.unwrap_or_default();
    Some(key.summary())
}

/// Question before „In freie Position umwandeln“.
pub const STLB_CONVERT_QUESTION: &str =
    "Die STLB-Bau-Kennung wird entfernt, danach ist der Langtext frei änderbar.";

impl Lv {
    /// Positions among `ids` (a title stands for its block) whose Langtext
    /// is write-protected because it comes from STLB-Bau.
    pub fn text_locked_positions(&self, ids: &[crate::RowId]) -> Vec<crate::RowId> {
        let mut locked = Vec::new();
        for id in ids {
            let Some(index) = self.index_of(*id) else {
                continue;
            };
            for row in &self.rows()[self.block_range(index)] {
                if let LvRow::Position(position) = row {
                    if position.is_text_locked() && !locked.contains(&position.id()) {
                        locked.push(position.id());
                    }
                }
            }
        }
        locked
    }

    /// The central check before the Langtext or the unit of rows change:
    /// Ok when none holds an STLB-Bau text, otherwise the explanation.
    pub fn ensure_text_editable(&self, ids: &[crate::RowId]) -> Result<(), String> {
        let locked = self.text_locked_positions(ids);
        let names: Vec<String> = locked
            .iter()
            .filter_map(|id| self.position_by_id(*id))
            .map(|position| position.ordinal().trim().to_string())
            .filter(|ordinal| !ordinal.is_empty())
            .collect();
        match (locked.len(), names.as_slice()) {
            (0, _) => Ok(()),
            (1, [ordinal]) => Err(format!("Position {ordinal}: {STLB_LOCKED}")),
            (_, _) => Err(format!(
                "{} ({}): {STLB_LOCKED}",
                crate::wording::count(locked.len(), "Position", "Positionen"),
                names.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
                    + if names.len() > 3 { " …" } else { "" }
            )),
        }
    }

    /// „In freie Position umwandeln“ for the positions among `ids` (titles:
    /// their blocks); returns how many were converted.
    pub fn convert_to_free(&mut self, ids: &[crate::RowId], today: &str) -> usize {
        let targets = self.text_locked_positions(ids);
        let mut converted = 0;
        for id in targets {
            if let Some(index) = self.index_of(id) {
                if let Some(LvRow::Position(position)) = self.row_mut(index) {
                    converted += usize::from(position.convert_to_free(today));
                }
            }
        }
        converted
    }
}

/// Fills an STLB placeholder (or any position) with a position taken from
/// STLB-Bau: texts, unit, Pauschal/Stundenlohn and the keys come from
/// `source`; order number and id stay. Quantity, unit price and kind of the
/// target win when they are set (a placeholder often already has its quantity).
pub fn fill_from_catalog(target: &mut Position, source: &Position) {
    target.set_short_text(source.short_text());
    target.set_long_text(source.long_text().clone());
    target.set_unit(source.unit());
    target.set_lump_sum(source.is_lump_sum());
    target.set_hourly(source.is_hourly());
    if target.quantity() == Decimal::ZERO {
        target.set_quantity(source.quantity());
    }
    if target.unit_price().is_none() {
        target.set_unit_price(source.unit_price());
    }
    if target.kind() == PositionKind::Normal {
        target.set_kind(source.kind());
    }
    target.set_complements(source.complements().to_vec());
    target.set_sub_descriptions(source.sub_descriptions().to_vec());
    target.copy_catalog_keys_from(source);
    // Kostengruppe and Leistungsbereich of the catalogue file (STLB-Bau
    // Online sends them as CtlgAssign, not in the key) unless the target
    // has its own; see [`adopt_catalog_cost_group`] for the title.
    if target.cost_group().is_none() {
        target.set_cost_group(catalog_cost_group(source));
    }
    if target.work_category().is_none() {
        target.set_work_category(source.work_category().map(str::to_string));
    }
    target.set_stlb_placeholder(false);
    // Taken from the catalogue = still to be checked by the planner.
    target.set_status(EditStatus::Open);
}

/// What keeps an STLB-Bau position from being handed out unchanged (public
/// clients, STLB-Bau Check), one line each, without the OZ: changed text
/// („STLB-Bau*“), a Textergänzung of the Ausschreibender (TA) left empty, a
/// numeric characteristic without a number. Empty for other positions.
pub fn stlb_problems(position: &Position) -> Vec<String> {
    let Some(stlb) = position.stlb_reference() else {
        return Vec::new();
    };
    let mut problems = Vec::new();
    if position.stlb_text_changed() {
        problems.push(STLB_CHANGED.to_string());
    }
    for complement in position.complements() {
        let marker = complement.marker.trim();
        let body = complement.body.trim();
        let number = stlb
            .complement_numbers
            .iter()
            .find(|(numbered, _)| numbered.trim() == marker)
            .map(|(_, number)| *number);
        if complement.kind == ComplementKind::Owner && body.is_empty() {
            problems.push(format!(
                "Textergänzung {marker} des Ausschreibenden (TA) ist leer."
            ));
        } else if let Some(number) = number.filter(|_| !body.is_empty()) {
            if number.value_of(body).is_none() {
                problems.push(format!(
                    "Textergänzung {marker}: „{body}“ ist kein Zahlenwert – STLB-Bau Check meldet „numerisch unvollständig“."
                ));
            }
        }
    }
    problems
}

/// Cost group (DIN 276) of a catalogue position: that of the STLB-Bau key,
/// else the one the file assigned (CtlgAssign, as STLB-Bau Online does it);
/// only valid codes.
pub fn catalog_cost_group(position: &Position) -> Option<String> {
    position
        .stlb_reference()
        .and_then(|stlb| parse_cost_group(&stlb.cost_group).ok().flatten())
        .or_else(|| {
            position
                .cost_group()
                .and_then(|code| parse_cost_group(code).ok().flatten())
        })
}

/// After [`fill_from_catalog`]: the position at `index` keeps the
/// catalogue cost group as its own only where the title above does not
/// already give the same one (otherwise it inherits it); a position without
/// one gets that of its STLB-Bau key on the same terms. Leistungsbereich
/// likewise. Returns whether the position has an own cost group afterwards.
pub fn adopt_catalog_cost_group(lv: &mut Lv, index: usize) -> bool {
    let Some(position) = lv.rows().get(index).and_then(LvRow::as_position) else {
        return false;
    };
    let code = position
        .cost_group()
        .map(str::to_string)
        .or_else(|| catalog_cost_group(position));
    let work = position.work_category().map(str::to_string);
    // What the titles above give, without the position's own values.
    let (inherited, inherited_work) = {
        let Some(position) = lv.row_mut(index).and_then(LvRow::as_position_mut) else {
            return false;
        };
        position.set_cost_group(None);
        position.set_work_category(None);
        (
            lv.effective_cost_group(index).map(|(code, _)| code),
            lv.effective_work_category(index).map(|(code, _)| code),
        )
    };
    let own = code.filter(|code| inherited.as_deref() != Some(code.as_str()));
    let own_work = work.filter(|work| inherited_work.as_deref() != Some(work.as_str()));
    match lv.row_mut(index).and_then(LvRow::as_position_mut) {
        Some(position) => {
            let has_own = own.is_some();
            position.set_cost_group(own);
            position.set_work_category(own_work);
            has_own
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rich_text::RichText;

    fn reference() -> StlbReference {
        StlbReference {
            catalog: "STLB-Bau".into(),
            version: Some("2026-04".into()),
            work_category: "070".into(),
            group: 9001,
            cost_group: "481".into(),
            characteristics: vec![StlbCharacteristic {
                characteristic: 1,
                index: Some(0),
                value: Some(2),
                short_text_part: true,
            }],
            owner_complements: true,
            bidder_complements: false,
            text_fingerprint: None,
            legacy_fingerprint: None,
            complement_links: vec![("1".into(), 1)],
            complement_numbers: Vec::new(),
        }
    }

    #[test]
    fn print_line_as_in_the_gaeb_examples() {
        assert_eq!(reference().print_line(), "STLB-Bau 4/2026 070 TA");
        let mut both = reference();
        both.bidder_complements = true;
        both.version = Some("2025-10".into());
        assert_eq!(both.print_line(), "STLB-Bau 10/2025 070 TA TB");
        let bare = StlbReference {
            version: Some("04/2026".into()),
            ..StlbReference::default()
        };
        assert_eq!(bare.print_line(), "STLB-Bau");
        let number = StandardNumber {
            number: "12345678901234567".into(),
            catalog: "StLB".into(),
        };
        assert_eq!(number.print_line(), "StLB 12345678901234567");
        assert_eq!(
            reference().summary(),
            "STLB-Bau 4/2026 · LB 070 · Leistung 9001 · KG 481 · 1 Merkmale"
        );
    }

    #[test]
    fn edited_text_is_noticed_formatting_is_not() {
        let mut position = Position::new("", "Raumbediengerät", Decimal::ONE, "St", None);
        position.set_long_text(RichText::plain("Unterputz,\nFarbe weiß"));
        assert!(!position.stlb_text_changed());
        position.attach_stlb(reference());
        assert!(!position.stlb_text_changed());
        position.set_long_text(RichText::plain("Unterputz,  Farbe weiß"));
        assert!(!position.stlb_text_changed(), "nur Umbruch/Leerraum");
        position.set_short_text("Raumbediengerät AP");
        assert!(!position.stlb_text_changed(), "Kurztext ist frei");
        assert_eq!(position.marks(), vec!["STLB-Bau"]);
        position.set_long_text(RichText::plain("Unterputz, Farbe schwarz"));
        assert!(position.stlb_text_changed());
        assert_eq!(position.marks(), vec!["STLB-Bau*"]);
        let mut unknown = reference();
        unknown.text_fingerprint = None;
        position.set_stlb_reference(Some(unknown));
        assert!(
            !position.stlb_text_changed(),
            "ohne Fingerabdruck keine Aussage"
        );
        assert_eq!(position.marks(), vec!["STLB-Bau"]);
    }

    #[test]
    fn earlier_fingerprint_with_short_text_is_upgraded_on_loading() {
        // A file from before: fingerprint over Kurz- and Langtext.
        let mut position = Position::new("", "Raumbediengerät", Decimal::ONE, "St", None);
        position.set_long_text(RichText::plain("Unterputz, Farbe weiß"));
        let mut old = reference();
        old.text_fingerprint = None;
        old.legacy_fingerprint = Some(legacy_text_fingerprint(&position));
        position.set_stlb_reference(Some(old.clone()));
        position.upgrade_stlb_fingerprint();
        let stlb = position.stlb_reference().unwrap();
        assert_eq!(stlb.legacy_fingerprint, None);
        assert_eq!(stlb.text_fingerprint, Some(text_fingerprint(&position)));
        position.set_short_text("Raumbediengerät AP");
        assert!(
            !position.stlb_text_changed(),
            "nach dem Laden ist der Kurztext frei – kein falsches STLB-Bau*"
        );

        // Changed before saving with the earlier version: which text is
        // unknown, the mark stays.
        let mut changed = Position::new("", "Raumbediengerät AP", Decimal::ONE, "St", None);
        changed.set_long_text(RichText::plain("Unterputz, Farbe weiß"));
        changed.set_stlb_reference(Some(old));
        changed.upgrade_stlb_fingerprint();
        assert!(changed
            .stlb_reference()
            .unwrap()
            .legacy_fingerprint
            .is_some());
        assert!(changed.stlb_text_changed());
        assert_eq!(changed.marks(), vec!["STLB-Bau*"]);
    }

    #[test]
    fn locked_text_takes_only_the_values_of_its_complements() {
        let mut position = Position::new("", "Wandschrank", Decimal::ONE, "St", None);
        let field = crate::TextComplement {
            marker: "1".into(),
            kind: ComplementKind::Owner,
            caption: "Maße ".into(),
            ..crate::TextComplement::default()
        };
        position.set_complements(vec![field.clone()]);
        position.attach_stlb(reference());
        assert!(position.is_text_locked());
        let mut filled = field.clone();
        filled.body = "800/800/300".into();
        filled.anchor = None;
        let list = position.locked_complement_values(&[filled]).unwrap();
        assert_eq!(list[0].body, "800/800/300");
        assert_eq!(list[0].anchor, field.anchor, "Platz bleibt");
        position.set_complements(list);
        assert!(!position.stlb_text_changed(), "Werte ändern den Text nicht");
        position.set_complements(vec![field.clone()]);
        let mut caption = field.clone();
        caption.caption = "Größe ".into();
        assert!(position.locked_complement_values(&[caption]).is_err());
        assert!(
            position
                .locked_complement_values(&[field.clone(), field])
                .is_err(),
            "keine neue Angabe"
        );
        let mut placeholder = Position::stlb("Platzhalter");
        placeholder.set_stlb_reference(Some(reference()));
        assert!(!placeholder.is_text_locked(), "Platzhalter frei");
    }

    #[test]
    fn locked_text_and_conversion_to_a_free_position() {
        let mut lv = Lv::new("LV", crate::OzMask::parse("11.PPPP").unwrap());
        lv.push_group(crate::LvGroup::new(0, "", "GA"));
        let mut stlb = Position::new("", "Wandschrank", Decimal::ONE, "St", None);
        stlb.set_long_text(RichText::plain("Stahlblech"));
        stlb.attach_stlb(reference());
        let free = Position::new("", "Frei", Decimal::ONE, "St", None);
        let mut placeholder = Position::stlb("Platzhalter");
        placeholder.set_quantity(Decimal::ONE);
        let ids = [
            lv.push_position(stlb),
            lv.push_position(free),
            lv.push_position(placeholder),
        ];
        lv.renumber();
        assert!(
            lv.ensure_text_editable(&ids[1..]).is_ok(),
            "frei und Platzhalter"
        );
        let message = lv.ensure_text_editable(&ids).unwrap_err();
        assert!(message.contains("schreibgeschützt"), "{message}");
        assert!(message.contains("In freie Position umwandeln"), "{message}");
        assert_eq!(lv.convert_to_free(&ids, "18.11.2026"), 1);
        assert!(lv.ensure_text_editable(&ids).is_ok());
        let converted = lv.position_by_id(ids[0]).unwrap();
        assert!(converted.stlb_reference().is_none());
        let origin = converted.text_origin().unwrap();
        assert_eq!(origin.source, "STLB-Bau");
        assert!(
            origin
                .note
                .starts_with("in freie Position umgewandelt am 18.11.2026"),
            "{}",
            origin.note
        );
    }

    #[test]
    fn placeholder_takes_text_and_keys_keeps_its_quantity() {
        let mut placeholder = Position::stlb("Platzhalter");
        placeholder.set_quantity(Decimal::from(12));
        let mut source = Position::new("01.0010", "Fühler", Decimal::from(2), "St", None);
        source.set_lump_sum(false);
        source.set_long_text(RichText::plain("Kanalfühler"));
        source.attach_stlb(reference());
        source.set_standard_number(None);
        fill_from_catalog(&mut placeholder, &source);
        assert!(!placeholder.is_stlb_placeholder());
        assert_eq!(placeholder.quantity(), Decimal::from(12));
        assert_eq!(placeholder.unit(), "St");
        assert_eq!(placeholder.short_text(), "Fühler");
        assert_eq!(
            placeholder.stlb_reference().map(|stlb| stlb.group),
            Some(9001)
        );
        assert!(!placeholder.stlb_text_changed());

        let mut empty = Position::stlb("Platzhalter");
        fill_from_catalog(&mut empty, &source);
        assert_eq!(empty.quantity(), Decimal::from(2));
    }

    /// STLB-Bau Online gives the Kostengruppe as CtlgAssign (read into the
    /// position, the key has no CostGrp): the placeholder gets it unless the
    /// title gives the same; an own one of the placeholder stays.
    #[test]
    fn cost_group_of_the_file_is_taken_unless_the_title_gives_it() {
        use crate::{LvGroup, OzMask};
        let mut source = Position::new("0001", "Automationseinrichtung", Decimal::ZERO, "St", None);
        source.attach_stlb(StlbReference {
            cost_group: String::new(),
            ..reference()
        });
        source.set_cost_group(Some("481".into()));
        source.set_work_category(Some("070".into()));
        assert_eq!(catalog_cost_group(&source).as_deref(), Some("481"));

        let lv_with = |title: Option<&str>, own: Option<&str>| {
            let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
            let mut group = LvGroup::new(0, "", "GA");
            group.set_cost_group(title.map(str::to_string));
            lv.push_group(group);
            let mut placeholder = Position::stlb("folgt");
            placeholder.set_cost_group(own.map(str::to_string));
            lv.push_position(placeholder);
            lv.renumber();
            lv
        };
        let take = |mut lv: Lv| {
            if let Some(target) = lv.row_mut(1).and_then(LvRow::as_position_mut) {
                fill_from_catalog(target, &source);
            }
            let own = adopt_catalog_cost_group(&mut lv, 1);
            let position = lv.rows()[1].as_position().unwrap().clone();
            (own, position, lv)
        };
        let (own, position, lv) = take(lv_with(None, None));
        assert!(own);
        assert_eq!(position.cost_group(), Some("481"));
        assert_eq!(position.work_category(), Some("070"));
        assert_eq!(lv.effective_cost_group(1), Some(("481".into(), false)));

        let (own, position, lv) = take(lv_with(Some("481"), None));
        assert!(!own, "kommt schon vom Titel");
        assert_eq!(position.cost_group(), None);
        assert_eq!(lv.effective_cost_group(1), Some(("481".into(), true)));

        let (own, position, _) = take(lv_with(Some("482"), None));
        assert!(own);
        assert_eq!(position.cost_group(), Some("481"));

        let (_, position, _) = take(lv_with(None, Some("484")));
        assert_eq!(
            position.cost_group(),
            Some("484"),
            "eigene KG des Platzhalters bleibt"
        );
    }
}
