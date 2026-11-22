use rust_decimal::Decimal;

use crate::edit_status::EditStatus;
use crate::money::Money;
use crate::rich_text::RichText;
use crate::stlb::{self, StandardNumber, StlbReference};
use crate::text_origin::TextOrigin;
use crate::RowId;

/// What kind of position this is (Positionsart nach VOB/GAEB).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PositionKind {
    #[default]
    Normal,
    /// Bedarfsposition (Eventualposition). `with_total`: mit Gesamtbetrag,
    /// zählt dann in die Summe; ohne Gesamtbetrag nur Einheitspreis.
    Provisional { with_total: bool },
    /// Grundposition einer Wahlgruppe – zählt in die Summe.
    Base { group: u32 },
    /// Wahlposition (Alternative) zu einer Grundposition – zählt nicht.
    Alternative { group: u32, serial: u32 },
    /// Zulageposition – zählt in die Summe.
    Supplement,
}

impl PositionKind {
    pub fn label(self) -> &'static str {
        match self {
            PositionKind::Normal => "Normalposition",
            PositionKind::Provisional { with_total: true } => "Bedarfsposition mit GB",
            PositionKind::Provisional { with_total: false } => "Bedarfsposition ohne GB",
            PositionKind::Base { .. } => "Grundposition",
            PositionKind::Alternative { .. } => "Wahlposition",
            PositionKind::Supplement => "Zulageposition",
        }
    }

    /// Short mark for tables and print, empty for normal positions.
    pub fn mark(self) -> &'static str {
        match self {
            PositionKind::Normal => "",
            PositionKind::Provisional { with_total: true } => "B+GB",
            PositionKind::Provisional { with_total: false } => "B",
            PositionKind::Base { .. } => "G",
            PositionKind::Alternative { .. } => "W",
            PositionKind::Supplement => "Z",
        }
    }

    /// Whether the total price of such a position counts in the LV total.
    pub fn counts_in_total(self) -> bool {
        !matches!(
            self,
            PositionKind::Provisional { with_total: false } | PositionKind::Alternative { .. }
        )
    }
}

/// Who fills in a text complement (Textergänzung).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComplementKind {
    /// Auftraggeber: the planner's addition.
    Owner,
    /// Bieter: to be filled in by the bidder (Fabrikat, Typ …).
    #[default]
    Bidder,
}

/// Stands for the open gap of a Textergänzung in
/// [`TextComplement::print_text_gap`] (private use character).
pub const PRINT_GAP: char = '\u{E000}';

/// Dots of a printed gap: 52 like the STLB-Bau placeholder
/// (about 51 mm at 10 pt).
pub const PRINT_GAP_DOTS: usize = 52;

/// Textergänzung of a position (GAEB `TextComplement`): a marked gap with a
/// caption before it, the filled-in text and a tail after it. GAEB keeps
/// their place in the long text loosely; proAVA keeps them after it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextComplement {
    /// Marker (Kennung), e.g. "01".
    pub marker: String,
    pub kind: ComplementKind,
    /// Text before the gap, e.g. "Fabrikat:".
    pub caption: String,
    /// Filled-in text; empty = still open.
    pub body: String,
    /// Text after the gap.
    pub tail: String,
    /// Where it stood in the long text (from GAEB DA XML); None = after the
    /// text. Edits of the long text may leave it a little off – the display
    /// clamps it.
    pub anchor: Option<TextAnchor>,
}

/// Place of a Textergänzung in the long text: block index and character
/// offset in that block; `own_line` = on a line of its own before block
/// `block` (block may equal the block count: at the end).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextAnchor {
    pub block: usize,
    pub offset: usize,
    pub own_line: bool,
}

impl TextComplement {
    /// One line for lists and print, e.g. "Fabrikat: ………… (Bieter)".
    pub fn display(&self) -> String {
        let body = if self.body.trim().is_empty() {
            "…………"
        } else {
            self.body.trim()
        };
        let text = [self.caption.trim(), body, self.tail.trim()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        match self.kind {
            ComplementKind::Bidder => format!("{text} (vom Bieter)"),
            ComplementKind::Owner => text,
        }
    }

    /// Form for the printed long text and for text that leaves proAVA
    /// (copy as plain text, CSV): caption, gap and tail as delivered, with
    /// their line breaks (STLB-Bau puts „vom Bieter einzutragen“ on a line of
    /// its own); a space only where a part brings none. No „(vom Bieter)“:
    /// the line of dots marks a Bieterangabe, and the text often says
    /// „vom Bieter einzutragen“ itself.
    pub fn print_text(&self) -> String {
        self.print_with_gap("…………", "")
    }

    /// Like [`Self::print_text`], an open gap as [`PRINT_GAP`]: the PDF
    /// draws it as a line of dots as long as in other AVA programs, in one
    /// piece.
    pub fn print_text_gap(&self) -> String {
        self.print_with_gap(&PRINT_GAP.to_string(), "")
    }

    /// The form the editor shows: like [`Self::print_text`], a Bieterangabe
    /// with „(vom Bieter)“ unless its text says so – a hint for editing
    /// (it tells a Bieterangabe from an Angabe des Auftraggebers), never
    /// printed.
    pub fn editor_text(&self) -> String {
        self.print_with_gap("…………", " (vom Bieter)")
    }

    fn print_with_gap(&self, gap: &str, bidder_note: &str) -> String {
        let body = if self.body.trim().is_empty() {
            gap
        } else {
            self.body.trim()
        };
        let mut text = String::new();
        for part in [self.caption.as_str(), body, self.tail.as_str()] {
            if part.trim().is_empty() {
                continue;
            }
            let needs_space = !text.is_empty()
                && !text.ends_with(char::is_whitespace)
                && !part.starts_with(char::is_whitespace);
            if needs_space {
                text.push(' ');
            }
            text.push_str(part);
        }
        // Spaces at both ends go; a final line break stays (one), so that
        // the text after an anchored gap still starts on a new line.
        let ends_with_break = text.trim_end_matches([' ', '\t']).ends_with('\n');
        let mut text = text.trim().to_string();
        let says_bidder = text.to_lowercase().contains("bieter");
        if self.kind == ComplementKind::Bidder && !says_bidder && !bidder_note.is_empty() {
            text.push_str(bidder_note);
        }
        if ends_with_break {
            text.push('\n');
        }
        text
    }
}

impl TextComplement {
    /// One line for the long-text editor: [`Self::editor_text`] with its
    /// line breaks as spaces, e.g. „Fabrikat: ………… (vom Bieter)“.
    pub fn field_label(&self) -> String {
        self.editor_text()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// With the blanks GAEB programs show between the parts: Vortext with a
    /// space at its end, Nachtext with one at its start (unless it starts
    /// with a punctuation mark or line break) – as STLB-Bau writes them
    /// („Hardwareliste Beiblatt 70-4 '……'“, not „…70-4'……'“).
    pub fn with_spaces(mut self) -> Self {
        if !self.caption.trim().is_empty() && !self.caption.ends_with(char::is_whitespace) {
            self.caption.push(' ');
        }
        if self.tail.starts_with(char::is_alphanumeric) {
            self.tail.insert(0, ' ');
        }
        self
    }

    /// „Bieterangabe“ or „Angabe des Auftraggebers“.
    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            ComplementKind::Bidder => "Bieterangabe",
            ComplementKind::Owner => "Angabe des Auftraggebers",
        }
    }
}

/// One line of a bill of quantities.
#[derive(Debug, Clone, PartialEq)]
pub struct Position {
    id: RowId,
    ordinal: String,
    short_text: String,
    long_text: RichText,
    quantity: Decimal,
    /// Rechenansatz the quantity came from („3*12,5+4“); GAEB gets only the result.
    quantity_formula: Option<String>,
    unit: String,
    /// Planner's unit price (Kostenvoranschlag); None = not priced.
    unit_price: Option<Decimal>,
    kind: PositionKind,
    lump_sum: bool,
    hourly: bool,
    /// Placeholder waiting for a text from STLB-Bau (text and price still open).
    stlb_placeholder: bool,
    /// Key of the text in STLB-Bau, if it came from there.
    stlb: Option<StlbReference>,
    /// Older Standardleistungsnummer (StLB/STLK) of the text.
    standard_number: Option<StandardNumber>,
    /// Ausführungsbeschreibung this position refers to ("gem. A0003").
    performance: Option<RowId>,
    /// Wiederholungsbeschreibung: the position whose text this one repeats.
    repeats: Option<RowId>,
    complements: Vec<TextComplement>,
    /// Quantity to be filled in by the bidder (GAEB QtyTBD).
    quantity_open: bool,
    /// Bidder shall break the unit price down (GAEB UPBkdn).
    price_breakdown: bool,
    /// Position entfällt: stays in the LV, counts in no sum (GAEB NotAppl).
    not_applicable: bool,
    /// Schwerpunktposition (GAEB KeyIt).
    key_item: bool,
    /// EP-Anteile, one per column of [`crate::Lv::price_components`]; for
    /// information only, the unit price stays what counts.
    price_components: Vec<Option<Decimal>>,
    /// Zeitansatz per unit (GAEB TimeQu).
    time_quantity: Option<Decimal>,
    /// Unterbeschreibungen of a Leitposition (GAEB SubDescr).
    sub_descriptions: Vec<SubDescription>,
    /// Bearbeitungsstatus; „fertig“ locks the position (see [`crate::edit_status`]).
    status: EditStatus,
    /// Kostengruppe nach DIN 276 (three digits); None = that of the title above.
    cost_group: Option<String>,
    /// Taken from a library: which one and which entry (Mit Bibliothek abgleichen).
    origin: Option<crate::library::LibraryOrigin>,
    /// As a library entry: Stichwörter, Stand, date of the price.
    library_info: crate::library::EntryInfo,
    /// Herkunft of the text (AUSSCHREIBEN.DE, STLB-Bau …), for the planner only.
    text_origin: Option<TextOrigin>,
    /// Leistungsbereich (Gewerk), e.g. „070“ (see [`crate::work_category`]).
    work_category: Option<String>,
    /// GP of an imported file did not match Menge × EP: marked until the
    /// planner confirms it or changes Menge or EP.
    price_check: Option<crate::price_check::PriceCheck>,
    /// Mengenermittlung of the planner (Planungsmenge): lines with place and
    /// Rechenansatz, the same kind of lines as the Aufmaß of the Abrechnung
    /// ([`crate::Billing::takeoff_of`]); here without invoice.
    takeoff: Vec<crate::billing::TakeoffLine>,
    /// Zuschlagsposition (GAEB MarkupItem): the unit price is the percentage,
    /// the total comes from the positions it applies to (see [`crate::surcharge`]).
    surcharge: Option<crate::surcharge::Surcharge>,
    /// Marked „zu bezuschlagen“ for Zuschläge on the marked positions (GAEB MarkupIt).
    marked_for_surcharge: bool,
}

/// Unterbeschreibung: a numbered part of a Leitposition with its own text
/// and partial quantity.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SubDescription {
    pub number: String,
    pub short_text: String,
    pub long_text: RichText,
    pub quantity: Option<Decimal>,
    pub unit: String,
    /// STLB-Bau key or Standardleistungsnummer of this text.
    pub catalog: stlb::CatalogKeys,
}

impl Position {
    pub fn new(
        ordinal: impl Into<String>,
        short_text: impl Into<String>,
        quantity: Decimal,
        unit: impl Into<String>,
        unit_price: Option<Decimal>,
    ) -> Self {
        Self {
            id: RowId::UNSET,
            ordinal: ordinal.into(),
            short_text: short_text.into(),
            long_text: RichText::default(),
            quantity,
            quantity_formula: None,
            unit: unit.into(),
            unit_price,
            kind: PositionKind::Normal,
            lump_sum: false,
            hourly: false,
            stlb_placeholder: false,
            stlb: None,
            standard_number: None,
            performance: None,
            repeats: None,
            complements: Vec::new(),
            quantity_open: false,
            price_breakdown: false,
            not_applicable: false,
            key_item: false,
            price_components: Vec::new(),
            time_quantity: None,
            sub_descriptions: Vec::new(),
            status: EditStatus::Open,
            cost_group: None,
            origin: None,
            library_info: crate::library::EntryInfo::default(),
            text_origin: None,
            work_category: None,
            takeoff: Vec::new(),
            price_check: None,
            surcharge: None,
            marked_for_surcharge: false,
        }
    }

    /// A Zuschlagsposition: `percent` on the positions `surcharge` names
    /// (no quantity, no unit; the percentage stands in the unit price).
    pub fn surcharge_position(
        short_text: impl Into<String>,
        surcharge: crate::surcharge::Surcharge,
        percent: Option<Decimal>,
    ) -> Self {
        let mut position = Self::new("", short_text, Decimal::ZERO, "", percent);
        position.surcharge = Some(surcharge);
        position
    }

    /// Zuschlagsposition (GAEB MarkupItem)?
    pub fn is_surcharge(&self) -> bool {
        self.surcharge.is_some()
    }

    pub fn surcharge(&self) -> Option<&crate::surcharge::Surcharge> {
        self.surcharge.as_ref()
    }

    /// Makes the position a Zuschlagsposition (or an ordinary one with None).
    /// A Zuschlag has no quantity and unit of its own.
    pub fn set_surcharge(&mut self, surcharge: Option<crate::surcharge::Surcharge>) {
        if surcharge.is_some() {
            self.quantity = Decimal::ZERO;
            self.quantity_formula = None;
            self.unit.clear();
            self.lump_sum = false;
            self.hourly = false;
        }
        self.surcharge = surcharge;
    }

    /// Percentage of a Zuschlagsposition (its unit price field).
    pub fn surcharge_percent(&self) -> Option<Decimal> {
        self.surcharge.as_ref().and(self.unit_price)
    }

    /// Marked „zu bezuschlagen“ (GAEB MarkupIt).
    pub fn is_marked_for_surcharge(&self) -> bool {
        self.marked_for_surcharge
    }

    pub fn set_marked_for_surcharge(&mut self, marked: bool) {
        self.marked_for_surcharge = marked;
    }

    /// Placeholder for a position whose text will come from STLB-Bau later.
    pub fn stlb(short_text: impl Into<String>) -> Self {
        let mut position = Self::new("", short_text, Decimal::ZERO, "", None);
        position.stlb_placeholder = true;
        position
    }

    pub fn id(&self) -> RowId {
        self.id
    }

    pub(crate) fn set_id(&mut self, id: RowId) {
        self.id = id;
    }

    pub fn ordinal(&self) -> &str {
        &self.ordinal
    }

    pub fn set_ordinal(&mut self, ordinal: impl Into<String>) {
        self.ordinal = ordinal.into();
    }

    pub fn short_text(&self) -> &str {
        &self.short_text
    }

    pub fn set_short_text(&mut self, text: impl Into<String>) {
        self.short_text = text.into();
    }

    pub fn long_text(&self) -> &RichText {
        &self.long_text
    }

    pub fn long_text_mut(&mut self) -> &mut RichText {
        &mut self.long_text
    }

    pub fn set_long_text(&mut self, text: RichText) {
        self.long_text = text;
    }

    pub fn quantity(&self) -> Decimal {
        self.quantity
    }

    /// Sets the quantity as a plain number; a Rechenansatz is dropped (set it
    /// again afterwards with [`Self::set_quantity_formula`]).
    pub fn set_quantity(&mut self, quantity: Decimal) {
        if quantity != self.quantity {
            // A new Menge answers the GP-Prüfung of the import.
            self.price_check = None;
        }
        self.quantity = quantity;
        self.quantity_formula = None;
    }

    pub fn quantity_formula(&self) -> Option<&str> {
        self.quantity_formula.as_deref()
    }

    /// Sets a Rechenansatz and its result; an empty text removes it.
    pub fn set_quantity_formula(&mut self, formula: Option<String>) {
        self.quantity_formula = formula
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty());
    }

    pub fn unit(&self) -> &str {
        &self.unit
    }

    pub fn set_unit(&mut self, unit: impl Into<String>) {
        self.unit = unit.into();
    }

    pub fn unit_price(&self) -> Option<Decimal> {
        self.unit_price
    }

    pub fn set_unit_price(&mut self, price: Option<Decimal>) {
        if price != self.unit_price {
            // A new EP answers the GP-Prüfung of the import.
            self.price_check = None;
        }
        self.unit_price = price;
    }

    /// The GP of the imported file differed from Menge × EP (see
    /// [`crate::price_check`]); None once confirmed or Menge/EP changed.
    pub fn price_check(&self) -> Option<&crate::price_check::PriceCheck> {
        self.price_check.as_ref()
    }

    /// Sets the mark (import) or removes it (confirmed).
    pub fn set_price_check(&mut self, check: Option<crate::price_check::PriceCheck>) {
        self.price_check = check;
    }

    pub fn kind(&self) -> PositionKind {
        self.kind
    }

    pub fn set_kind(&mut self, kind: PositionKind) {
        self.kind = kind;
    }

    pub fn is_lump_sum(&self) -> bool {
        self.lump_sum
    }

    pub fn set_lump_sum(&mut self, lump_sum: bool) {
        self.lump_sum = lump_sum;
    }

    pub fn is_hourly(&self) -> bool {
        self.hourly
    }

    pub fn set_hourly(&mut self, hourly: bool) {
        self.hourly = hourly;
    }

    pub fn is_stlb_placeholder(&self) -> bool {
        self.stlb_placeholder
    }

    pub fn set_stlb_placeholder(&mut self, placeholder: bool) {
        self.stlb_placeholder = placeholder;
    }

    /// Row id of the Ausführungsbeschreibung this position refers to.
    pub fn performance(&self) -> Option<RowId> {
        self.performance
    }

    /// Sets the reference without checking it; inside an LV use
    /// [`crate::Lv::link_performance`], which checks that it exists.
    pub fn set_performance(&mut self, performance: Option<RowId>) {
        self.performance = performance;
    }

    /// Base position of a Wiederholungsbeschreibung.
    pub fn repeats(&self) -> Option<RowId> {
        self.repeats
    }

    /// Sets the reference without checking it; inside an LV use
    /// [`crate::Lv::link_repetition`].
    pub fn set_repeats(&mut self, base: Option<RowId>) {
        self.repeats = base;
    }

    pub fn complements(&self) -> &[TextComplement] {
        &self.complements
    }

    pub fn complements_mut(&mut self) -> &mut Vec<TextComplement> {
        &mut self.complements
    }

    pub fn set_complements(&mut self, complements: Vec<TextComplement>) {
        self.complements = complements;
    }

    /// Inserts a block (image, PDF clip) into the long text after its first
    /// `at` blocks; the Textergänzungen keep their place in the text.
    pub fn insert_long_text_block(&mut self, at: usize, block: crate::rich_text::TextBlock) {
        let at = at.min(self.long_text.blocks().len());
        self.long_text.blocks_mut().insert(at, block);
        crate::complement_text::shift_after_insert(&mut self.complements, at);
    }

    pub fn is_quantity_open(&self) -> bool {
        self.quantity_open
    }

    pub fn set_quantity_open(&mut self, open: bool) {
        self.quantity_open = open;
    }

    pub fn has_price_breakdown(&self) -> bool {
        self.price_breakdown
    }

    pub fn set_price_breakdown(&mut self, breakdown: bool) {
        self.price_breakdown = breakdown;
    }

    pub fn is_not_applicable(&self) -> bool {
        self.not_applicable
    }

    pub fn set_not_applicable(&mut self, not_applicable: bool) {
        self.not_applicable = not_applicable;
    }

    pub fn is_key_item(&self) -> bool {
        self.key_item
    }

    pub fn set_key_item(&mut self, key_item: bool) {
        self.key_item = key_item;
    }

    pub fn price_components(&self) -> &[Option<Decimal>] {
        &self.price_components
    }

    pub fn set_price_components(&mut self, components: Vec<Option<Decimal>>) {
        self.price_components = components;
    }

    pub fn time_quantity(&self) -> Option<Decimal> {
        self.time_quantity
    }

    pub fn set_time_quantity(&mut self, hours: Option<Decimal>) {
        self.time_quantity = hours;
    }

    pub fn sub_descriptions(&self) -> &[SubDescription] {
        &self.sub_descriptions
    }

    pub fn sub_descriptions_mut(&mut self) -> &mut Vec<SubDescription> {
        &mut self.sub_descriptions
    }

    pub fn set_sub_descriptions(&mut self, sub_descriptions: Vec<SubDescription>) {
        self.sub_descriptions = sub_descriptions;
    }

    /// Own Kostengruppe (see [`crate::Lv::effective_cost_group`] for the one that applies).
    pub fn cost_group(&self) -> Option<&str> {
        self.cost_group.as_deref()
    }

    pub fn set_cost_group(&mut self, code: Option<String>) {
        self.cost_group = code.filter(|code| !code.trim().is_empty());
    }

    /// The library entry this position was taken from, if any.
    pub fn origin(&self) -> Option<&crate::library::LibraryOrigin> {
        self.origin.as_ref()
    }

    pub fn set_origin(&mut self, origin: Option<crate::library::LibraryOrigin>) {
        self.origin = origin;
    }

    /// Stichwörter, Stand and price date of a library entry.
    pub fn library_info(&self) -> &crate::library::EntryInfo {
        &self.library_info
    }

    pub fn set_library_info(&mut self, info: crate::library::EntryInfo) {
        self.library_info = info;
    }

    /// Leistungsbereich (Gewerk), e.g. „070“; None = none.
    pub fn work_category(&self) -> Option<&str> {
        self.work_category.as_deref()
    }

    /// Lines of the Mengenermittlung (Planungsmenge).
    pub fn takeoff(&self) -> &[crate::billing::TakeoffLine] {
        &self.takeoff
    }

    /// Sets the lines of the Mengenermittlung; an invoice has no meaning
    /// here and is dropped.
    pub fn set_takeoff(&mut self, lines: Vec<crate::billing::TakeoffLine>) {
        self.takeoff = lines
            .into_iter()
            .map(|line| crate::billing::TakeoffLine {
                invoice: None,
                ..line
            })
            .collect();
    }

    /// Sum of the valid lines of the Mengenermittlung and the errors of the
    /// others („Zeile 2 „EG“: …“).
    pub fn takeoff_total(&self) -> (Decimal, Vec<String>) {
        let mut sum = Decimal::ZERO;
        let mut errors = Vec::new();
        for (number, line) in self.takeoff.iter().enumerate() {
            match line.result() {
                Ok(value) => sum += value,
                Err(error) => {
                    let label = line.label.trim();
                    errors.push(if label.is_empty() {
                        format!("Zeile {}: {error}", number + 1)
                    } else {
                        format!("Zeile {} „{label}“: {error}", number + 1)
                    })
                }
            }
        }
        (sum.normalize(), errors)
    }

    /// The quantity differs from the sum of a Mengenermittlung without errors.
    pub fn takeoff_differs(&self) -> Option<Decimal> {
        if self.takeoff.is_empty() {
            return None;
        }
        let (sum, errors) = self.takeoff_total();
        (errors.is_empty() && sum != self.quantity.normalize()).then_some(sum)
    }

    pub fn set_work_category(&mut self, code: Option<String>) {
        self.work_category = code
            .map(|code| code.trim().to_string())
            .filter(|code| !code.is_empty());
    }

    pub fn status(&self) -> EditStatus {
        self.status
    }

    pub fn set_status(&mut self, status: EditStatus) {
        self.status = status;
    }

    /// Finished positions are read-only until their status is set back.
    pub fn is_locked(&self) -> bool {
        self.status.is_locked()
    }

    /// Whether its total counts in sums: not for Wahl-, Bedarfspositionen
    /// ohne GB and positions that entfallen.
    pub fn counts_in_total(&self) -> bool {
        self.kind.counts_in_total() && !self.not_applicable
    }

    /// Planner's total price (GP), rounded to cents; zero when unpriced.
    pub fn total_price(&self) -> Money {
        self.total_for(self.unit_price)
    }

    /// GP for any unit price (e.g. a bidder's), rounded to cents. Zero for a
    /// Zuschlagsposition – its total comes from the LV
    /// ([`crate::Lv::surcharge_values`], [`crate::Lv::row_totals_with`]).
    pub fn total_for(&self, unit_price: Option<Decimal>) -> Money {
        if self.surcharge.is_some() {
            return Money::ZERO;
        }
        unit_price
            .map(|price| Money::total(price, self.quantity))
            .unwrap_or(Money::ZERO)
    }

    /// Total that counts in sums (zero for Wahl- and Bedarfspositionen ohne GB).
    pub fn counted_total(&self) -> Money {
        if self.counts_in_total() {
            self.total_price()
        } else {
            Money::ZERO
        }
    }

    /// Labels of all marks for display: kind, Pauschal, Stundenlohn, STLB.
    pub fn marks(&self) -> Vec<&'static str> {
        let mut marks = Vec::new();
        if !self.kind.mark().is_empty() {
            marks.push(self.kind.mark());
        }
        if self.lump_sum {
            marks.push("Psch");
        }
        if self.hourly {
            marks.push("Std");
        }
        if self.stlb_placeholder {
            marks.push("STLB");
        }
        if self.not_applicable {
            marks.push("entf");
        }
        match &self.surcharge {
            Some(surcharge) if surcharge.split => marks.push("Aufteilung"),
            Some(_) => marks.push("Zuschlag"),
            None => {}
        }
        if self.stlb.is_some() {
            marks.push(if self.stlb_text_changed() {
                "STLB-Bau*"
            } else {
                "STLB-Bau"
            });
        }
        marks
    }

    /// Key of the text in STLB-Bau.
    pub fn stlb_reference(&self) -> Option<&StlbReference> {
        self.stlb.as_ref()
    }

    /// Sets the key as it is (e.g. read from the project file).
    pub fn set_stlb_reference(&mut self, stlb: Option<StlbReference>) {
        self.stlb = stlb;
    }

    /// Sets the key for the text the position has now (taken from STLB-Bau):
    /// later edits of the text show up in [`Self::stlb_text_changed`].
    pub fn attach_stlb(&mut self, mut stlb: StlbReference) {
        stlb.text_fingerprint = Some(stlb::text_fingerprint(self));
        stlb.legacy_fingerprint = None;
        self.stlb = Some(stlb);
    }

    /// Whether the Langtext differs from what STLB-Bau delivered (the
    /// Kurztext is free). For files with the earlier fingerprint that no
    /// longer matched on loading: Kurz- or Langtext differ.
    pub fn stlb_text_changed(&self) -> bool {
        let Some(stlb) = self.stlb.as_ref() else {
            return false;
        };
        match (stlb.text_fingerprint, stlb.legacy_fingerprint) {
            (Some(fingerprint), _) => fingerprint != stlb::text_fingerprint(self),
            (None, Some(legacy)) => legacy != stlb::legacy_text_fingerprint(self),
            (None, None) => false,
        }
    }

    /// After loading a file with the earlier fingerprint (Kurz- and
    /// Langtext): when it still matches, the text is as delivered – the new
    /// fingerprint (Langtext only) replaces it, so a later change of the
    /// Kurztext is no change of the STLB-Bau text. When it does not match,
    /// it stays (Kurz- or Langtext were changed; which one is unknown).
    pub fn upgrade_stlb_fingerprint(&mut self) {
        let current = stlb::text_fingerprint(self);
        let legacy_now = stlb::legacy_text_fingerprint(self);
        if let Some(stlb) = self.stlb.as_mut() {
            if stlb.text_fingerprint.is_none() && stlb.legacy_fingerprint == Some(legacy_now) {
                stlb.text_fingerprint = Some(current);
                stlb.legacy_fingerprint = None;
            }
        }
    }

    /// The Langtext and the unit come from STLB-Bau and are write-protected
    /// (Kurztext, quantity, prices, KG, LB, status, kind and the values of
    /// the Textergänzungen stay free). Placeholders are not locked.
    pub fn is_text_locked(&self) -> bool {
        self.stlb.is_some() && !self.stlb_placeholder
    }

    /// For a locked STLB-Bau text only the values of the Textergänzungen
    /// change (the planner fills the TA, the bidder the TB): the list as it
    /// is with the values (`body`) of `edited`, matched by marker. Err when
    /// `edited` adds or removes a field or changes its kind, Vortext or
    /// Nachtext – that is part of the STLB-Bau text.
    pub fn locked_complement_values(
        &self,
        edited: &[TextComplement],
    ) -> Result<Vec<TextComplement>, String> {
        let same =
            |a: &str, b: &str| a.trim().trim_start_matches('0') == b.trim().trim_start_matches('0');
        let structure =
            "Im STLB-Bau-Text sind nur die Werte der Textergänzungen änderbar – Art, Vortext und \
                         Nachtext gehören zum Text, neue Angaben gehen nicht.";
        if edited.len() != self.complements.len() {
            return Err(structure.to_string());
        }
        self.complements
            .iter()
            .map(|current| {
                let new = edited
                    .iter()
                    .find(|new| same(&new.marker, &current.marker))
                    .ok_or_else(|| structure.to_string())?;
                if new.kind != current.kind
                    || new.caption.trim() != current.caption.trim()
                    || new.tail.trim() != current.tail.trim()
                {
                    return Err(structure.to_string());
                }
                Ok(TextComplement {
                    body: new.body.clone(),
                    ..current.clone()
                })
            })
            .collect()
    }

    /// „In freie Position umwandeln“: the STLB-Bau key with its fingerprint
    /// goes, the text becomes free. Where the text came from stays as its
    /// Herkunft, with a note of the conversion (`today`: TT.MM.JJJJ).
    /// False when the position had no key.
    pub fn convert_to_free(&mut self, today: &str) -> bool {
        let Some(key) = crate::stlb::key_line(self) else {
            return false;
        };
        let Some(stlb) = self.stlb.take() else {
            return false;
        };
        let note = format!("in freie Position umgewandelt am {today} ({key})");
        let mut origin = self.text_origin.clone().unwrap_or_else(|| TextOrigin {
            source: if stlb.catalog.trim().is_empty() {
                crate::text_origin::STLB_BAU.to_string()
            } else {
                stlb.catalog.trim().to_string()
            },
            ..TextOrigin::default()
        });
        origin.note = note;
        self.text_origin = Some(origin);
        true
    }

    pub fn standard_number(&self) -> Option<&StandardNumber> {
        self.standard_number.as_ref()
    }

    pub fn set_standard_number(&mut self, number: Option<StandardNumber>) {
        self.standard_number = number;
    }

    /// Takes STLB-Bau key, Standardleistungsnummer and Herkunft of the text of
    /// `other` (copies, paste, take-over from a catalogue).
    pub fn copy_catalog_keys_from(&mut self, other: &Position) {
        self.stlb = other.stlb.clone();
        self.standard_number = other.standard_number.clone();
        self.text_origin = other.text_origin.clone();
    }

    /// Where the text came from (text source AUSSCHREIBEN.DE, STLB-Bau …), if
    /// known. Not the library entry – that is [`Self::origin`].
    pub fn text_origin(&self) -> Option<&TextOrigin> {
        self.text_origin.as_ref()
    }

    /// Sets the Herkunft of the text; an empty one counts as none.
    pub fn set_text_origin(&mut self, origin: Option<TextOrigin>) {
        self.text_origin = origin.filter(|origin| !origin.is_empty());
    }

    /// Catalogue line to print above the Langtext (GAEB 4.6.10), if any.
    /// TA/TB follow the Textergänzungen the position has now.
    pub fn catalog_line(&self) -> Option<String> {
        if let Some(stlb) = &self.stlb {
            let mut line = stlb.clone();
            if !self.complements.is_empty() {
                line.owner_complements = self
                    .complements
                    .iter()
                    .any(|c| c.kind == ComplementKind::Owner);
                line.bidder_complements = self
                    .complements
                    .iter()
                    .any(|c| c.kind == ComplementKind::Bidder);
            }
            return Some(line.print_line());
        }
        self.standard_number
            .as_ref()
            .map(StandardNumber::print_line)
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::{Position, PositionKind};
    use crate::money::Money;

    #[test]
    fn provisional_and_alternative_positions_do_not_count() {
        let mut position = Position::new("", "x", Decimal::from(2), "St", Some(Decimal::from(10)));
        assert_eq!(position.counted_total(), Money::from_cents(2000));
        position.set_kind(PositionKind::Provisional { with_total: false });
        assert_eq!(position.counted_total(), Money::ZERO);
        assert_eq!(position.total_price(), Money::from_cents(2000));
        position.set_kind(PositionKind::Provisional { with_total: true });
        assert_eq!(position.counted_total(), Money::from_cents(2000));
        position.set_kind(PositionKind::Alternative {
            group: 1,
            serial: 1,
        });
        assert_eq!(position.counted_total(), Money::ZERO);
        position.set_kind(PositionKind::Base { group: 1 });
        assert_eq!(position.counted_total(), Money::from_cents(2000));
        position.set_not_applicable(true);
        assert_eq!(position.counted_total(), Money::ZERO);
        assert!(position.marks().contains(&"entf"));
    }

    /// Print form of a Textergänzung: STLB-Bau's line break before the tail
    /// stays; „(vom Bieter)“ only in the editor's form.
    #[test]
    fn complements_print_as_delivered() {
        use super::{ComplementKind, TextComplement};
        let stlb = TextComplement {
            kind: ComplementKind::Bidder,
            caption: "Gerätetyp gemäß Geräteliste ".into(),
            tail: "\nvom Bieter einzutragen".into(),
            ..TextComplement::default()
        };
        assert_eq!(
            stlb.print_text(),
            "Gerätetyp gemäß Geräteliste …………\nvom Bieter einzutragen"
        );
        assert_eq!(
            stlb.display(),
            "Gerätetyp gemäß Geräteliste ………… vom Bieter einzutragen (vom Bieter)"
        );

        let plain = TextComplement {
            kind: ComplementKind::Bidder,
            caption: "Fabrikat:".into(),
            ..TextComplement::default()
        };
        // Print and outward text: no „(vom Bieter)“ – the dots mark it.
        assert_eq!(plain.print_text(), "Fabrikat: …………");
        // Editor and lists: with the hint, not printed.
        assert_eq!(plain.editor_text(), plain.display());
        assert_eq!(plain.field_label(), "Fabrikat: ………… (vom Bieter)");
        assert_eq!(
            stlb.field_label(),
            "Gerätetyp gemäß Geräteliste ………… vom Bieter einzutragen",
            "sagt es schon selbst"
        );

        let owner = TextComplement {
            kind: ComplementKind::Owner,
            caption: "Gerätetyp gemäß Anlage ".into(),
            body: "A3".into(),
            tail: "\nmit Zubehör\n".into(),
            ..TextComplement::default()
        };
        assert_eq!(
            owner.print_text(),
            "Gerätetyp gemäß Anlage A3\nmit Zubehör\n",
            "Umbruch am Ende bleibt"
        );
        // For the PDF: the gap as placeholder, nothing after it.
        assert_eq!(
            plain.print_text_gap(),
            format!("Fabrikat: {}", super::PRINT_GAP)
        );
        assert_eq!(
            owner.print_text_gap(),
            owner.print_text(),
            "ausgefüllt: keine Lücke"
        );
        assert_eq!(
            stlb.print_text_gap(),
            format!(
                "Gerätetyp gemäß Geräteliste {}\nvom Bieter einzutragen",
                super::PRINT_GAP
            )
        );
    }
}

#[cfg(test)]
mod takeoff_tests {
    use rust_decimal::Decimal;

    use super::Position;
    use crate::billing::{InvoiceId, TakeoffLine};

    fn line(label: &str, formula: &str) -> TakeoffLine {
        TakeoffLine {
            label: label.into(),
            formula: formula.into(),
            invoice: Some(InvoiceId(3)),
        }
    }

    #[test]
    fn takeoff_sums_valid_lines_and_names_wrong_ones() {
        let mut position = Position::new("", "Kabel", Decimal::from(10), "m", None);
        assert_eq!(
            position.takeoff_differs(),
            None,
            "ohne Mengenermittlung kein Vergleich"
        );
        position.set_takeoff(vec![
            line("EG Strang 1", "2*12,5"),
            line("OG", "4,5+3"),
            line("", ""),
        ]);
        assert!(
            position.takeoff().iter().all(|line| line.invoice.is_none()),
            "keine Rechnung im LV"
        );
        assert_eq!(
            position.takeoff_total(),
            ("32.5".parse().unwrap(), Vec::new())
        );
        assert_eq!(position.takeoff_differs(), Some("32.5".parse().unwrap()));
        position.set_quantity("32.5".parse().unwrap());
        assert_eq!(position.takeoff_differs(), None);
        position.set_takeoff(vec![line("EG", "2*"), line("", "3+")]);
        let (sum, errors) = position.takeoff_total();
        assert_eq!(sum, Decimal::ZERO);
        assert_eq!(errors.len(), 2);
        assert!(errors[0].starts_with("Zeile 1 „EG“: "), "{errors:?}");
        assert!(errors[1].starts_with("Zeile 2: "), "{errors:?}");
        assert_eq!(
            position.takeoff_differs(),
            None,
            "bei Fehlern kein Vergleich"
        );
    }
}
