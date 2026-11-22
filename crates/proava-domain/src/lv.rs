use rust_decimal::Decimal;

use crate::money::Money;
use crate::oz_mask::OzMask;
use crate::performance::PerformanceDescription;
use crate::position::{Position, TextComplement};
use crate::rich_text::RichText;
use crate::stlb::CatalogKeys;

/// Positions usually count 10, 20, 30 …; 1, 100 and 1000 are the other choices.
pub const DEFAULT_POSITION_STEP: u32 = 10;

/// Stable identity of a row inside one LV; survives renumbering and moves.
/// Bids and other references point to positions through it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct RowId(pub u64);

impl RowId {
    pub const UNSET: RowId = RowId(0);
}

/// A heading (Titel, Untertitel …). `level` 0 is the outermost one.
#[derive(Debug, Clone, PartialEq)]
pub struct LvGroup {
    id: RowId,
    level: usize,
    ordinal: String,
    short_text: String,
    /// Kostengruppe for everything below (positions with their own win).
    cost_group: Option<String>,
    /// Leistungsbereich for everything below (positions with their own win).
    work_category: Option<String>,
    /// Pauschalsumme of the title (GAEB TotalLSUM in BoQCtgy): replaces the
    /// sum of its block in the planner's totals above.
    lump_sum_total: Option<Money>,
    /// Datenpunkt-Abschnitt of a Kosten-LV (GU-Fall, Anwender 19.11.): the title
    /// is calculated over data points – the same building block as a
    /// GA-Datenpunktrechnung ([`crate::cost_estimate::CostEstimate`], its
    /// stage follows the LV). Its total counts as the title's own amount,
    /// split per Kostengruppe (KS: 480, KB: 481–489).
    points: Option<Box<crate::cost_estimate::CostEstimate>>,
}

impl LvGroup {
    pub fn new(level: usize, ordinal: impl Into<String>, short_text: impl Into<String>) -> Self {
        Self {
            id: RowId::UNSET,
            level,
            ordinal: ordinal.into(),
            short_text: short_text.into(),
            cost_group: None,
            work_category: None,
            lump_sum_total: None,
            points: None,
        }
    }

    pub fn id(&self) -> RowId {
        self.id
    }

    /// The Datenpunkt-Abschnitt of the title, if it is one.
    pub fn points(&self) -> Option<&crate::cost_estimate::CostEstimate> {
        self.points.as_deref()
    }

    pub fn points_mut(&mut self) -> Option<&mut crate::cost_estimate::CostEstimate> {
        self.points.as_deref_mut()
    }

    /// Makes the title a Datenpunkt-Abschnitt (Some) or an ordinary title (None).
    pub fn set_points(&mut self, points: Option<crate::cost_estimate::CostEstimate>) {
        self.points = points.map(Box::new);
    }

    /// The title is calculated over data points.
    pub fn is_point_section(&self) -> bool {
        self.points.is_some()
    }

    /// Own Kostengruppe, the default for everything below.
    pub fn cost_group(&self) -> Option<&str> {
        self.cost_group.as_deref()
    }

    pub fn set_cost_group(&mut self, code: Option<String>) {
        self.cost_group = code.filter(|code| !code.trim().is_empty());
    }

    /// Own Leistungsbereich, the default for everything below.
    pub fn work_category(&self) -> Option<&str> {
        self.work_category.as_deref()
    }

    pub fn set_work_category(&mut self, code: Option<String>) {
        self.work_category = code
            .map(|code| code.trim().to_string())
            .filter(|code| !code.is_empty());
    }

    /// Pauschalsumme of the title (GAEB TotalLSUM); None = the sum of its
    /// block.
    pub fn lump_sum_total(&self) -> Option<Money> {
        self.lump_sum_total
    }

    pub fn set_lump_sum_total(&mut self, total: Option<Money>) {
        self.lump_sum_total = total;
    }

    pub fn level(&self) -> usize {
        self.level
    }

    pub fn set_level(&mut self, level: usize) {
        self.level = level;
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
}

/// A free hint line without order number and price (Hinweistext).
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    id: RowId,
    text: RichText,
    /// Optional heading (Kurztext of a GAEB Remark), shown above the text.
    heading: String,
    /// Vorbemerkung (GAEB AddText) instead of a hint in the outline; GAEB
    /// DA XML carries it before the LV, wherever it stands here.
    preliminary: bool,
    /// Schlussbemerkung (GAEB AddText after the Award, e.g. a „Schlussblatt“
    /// of the planner): GAEB DA XML carries it after the LV, the print after
    /// the totals – wherever it stands here. Never together with
    /// `preliminary`.
    closing: bool,
    /// Textergänzungen in the text (STLB-Bau Standardbeschreibungen).
    complements: Vec<TextComplement>,
    /// STLB-Bau key or Standardleistungsnummer of a catalogue hint text.
    catalog: CatalogKeys,
    /// Herkunft of the text (STLB-Bau, AUSSCHREIBEN.DE …), for the planner
    /// only – also after a position became a hint.
    text_origin: Option<crate::TextOrigin>,
}

impl Note {
    pub fn new(text: impl Into<String>) -> Self {
        Self::rich(RichText::plain(&text.into()))
    }

    pub fn rich(text: RichText) -> Self {
        Self {
            id: RowId::UNSET,
            text,
            heading: String::new(),
            preliminary: false,
            closing: false,
            complements: Vec::new(),
            catalog: CatalogKeys::default(),
            text_origin: None,
        }
    }

    /// Where the text came from, if known.
    pub fn text_origin(&self) -> Option<&crate::TextOrigin> {
        self.text_origin.as_ref()
    }

    pub fn set_text_origin(&mut self, origin: Option<crate::TextOrigin>) {
        self.text_origin = origin.filter(|origin| !origin.is_empty());
    }

    pub fn complements(&self) -> &[TextComplement] {
        &self.complements
    }

    pub fn set_complements(&mut self, complements: Vec<TextComplement>) {
        self.complements = complements;
    }

    /// Inserts a block (image, PDF clip) into the text after its first `at`
    /// blocks; the Textergänzungen keep their place in the text.
    pub fn insert_text_block(&mut self, at: usize, block: crate::rich_text::TextBlock) {
        let at = at.min(self.text.blocks().len());
        self.text.blocks_mut().insert(at, block);
        crate::complement_text::shift_after_insert(&mut self.complements, at);
    }

    pub fn catalog(&self) -> &CatalogKeys {
        &self.catalog
    }

    pub fn set_catalog(&mut self, catalog: CatalogKeys) {
        self.catalog = catalog;
    }

    /// Catalogue line printed above the text, e.g. „STLB-Bau 4/2026 070 TA“.
    pub fn catalog_line(&self) -> Option<String> {
        self.catalog.print_line()
    }

    pub fn is_preliminary(&self) -> bool {
        self.preliminary
    }

    /// A Vorbemerkung is no Schlussbemerkung: setting one clears the other.
    pub fn set_preliminary(&mut self, preliminary: bool) {
        self.preliminary = preliminary;
        if preliminary {
            self.closing = false;
        }
    }

    pub fn is_closing(&self) -> bool {
        self.closing
    }

    pub fn set_closing(&mut self, closing: bool) {
        self.closing = closing;
        if closing {
            self.preliminary = false;
        }
    }

    pub fn heading(&self) -> &str {
        &self.heading
    }

    pub fn set_heading(&mut self, heading: impl Into<String>) {
        self.heading = heading.into();
    }

    pub fn id(&self) -> RowId {
        self.id
    }

    pub fn text(&self) -> &RichText {
        &self.text
    }

    pub fn text_mut(&mut self) -> &mut RichText {
        &mut self.text
    }

    pub fn set_text(&mut self, text: RichText) {
        self.text = text;
    }

    /// Neither heading nor text (also only spaces or empty paragraphs): a
    /// hint that says nothing – usually an empty line for the eye, which the
    /// [`Spacer`] does without a warning.
    pub fn is_blank(&self) -> bool {
        self.heading.trim().is_empty() && self.text.is_empty()
    }

    /// Heading, else the first line, for lists.
    pub fn title(&self) -> String {
        if !self.heading.trim().is_empty() {
            return self.heading.trim().to_string();
        }
        self.text
            .to_plain()
            .lines()
            .next()
            .unwrap_or_default()
            .to_string()
    }
}

/// Abstandshalter: an empty line between rows, only for editing (Anwender
/// 18.11.). No order number, text or price; it counts nowhere, and print,
/// GAEB and CSV leave it out.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Spacer {
    id: RowId,
}

impl Spacer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn id(&self) -> RowId {
        self.id
    }
}

/// One column of the unit price breakdown (EP-Anteil), e.g. Lohn.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PriceComponent {
    pub label: String,
    /// GAEB type: "Wages", "Materials", "Plant", "Miscellaneous", "Unknown" or
    /// empty (then the GAEB writer takes it from the label).
    pub kind: String,
}

/// A row of the bill of quantities in document order.
#[derive(Debug, Clone, PartialEq)]
pub enum LvRow {
    Group(LvGroup),
    Position(Position),
    Note(Note),
    /// Ausführungsbeschreibung, referenced by positions.
    Performance(PerformanceDescription),
    /// Abstandshalter (empty line for editing only).
    Spacer(Spacer),
}

impl LvRow {
    pub fn id(&self) -> RowId {
        match self {
            LvRow::Group(group) => group.id,
            LvRow::Position(position) => position.id(),
            LvRow::Note(note) => note.id,
            LvRow::Performance(performance) => performance.id(),
            LvRow::Spacer(spacer) => spacer.id,
        }
    }

    /// Same row asking for `id`; [`Lv::insert_row`] keeps it when it is free
    /// (used when loading a saved project).
    pub fn with_id(mut self, id: RowId) -> Self {
        self.set_id(id);
        self
    }

    fn set_id(&mut self, id: RowId) {
        match self {
            LvRow::Group(group) => group.id = id,
            LvRow::Position(position) => position.set_id(id),
            LvRow::Note(note) => note.id = id,
            LvRow::Performance(performance) => performance.set_id(id),
            LvRow::Spacer(spacer) => spacer.id = id,
        }
    }

    pub fn ordinal(&self) -> &str {
        match self {
            LvRow::Group(group) => group.ordinal(),
            LvRow::Position(position) => position.ordinal(),
            LvRow::Note(_) | LvRow::Performance(_) | LvRow::Spacer(_) => "",
        }
    }

    fn set_ordinal(&mut self, ordinal: String) {
        match self {
            LvRow::Group(group) => group.set_ordinal(ordinal),
            LvRow::Position(position) => position.set_ordinal(ordinal),
            LvRow::Note(_) | LvRow::Performance(_) | LvRow::Spacer(_) => {}
        }
    }

    pub fn short_text(&self) -> String {
        match self {
            LvRow::Group(group) => group.short_text().to_string(),
            LvRow::Position(position) => position.short_text().to_string(),
            LvRow::Note(note) => note.title(),
            LvRow::Performance(performance) => performance.heading(),
            LvRow::Spacer(_) => String::new(),
        }
    }

    pub fn as_group(&self) -> Option<&LvGroup> {
        match self {
            LvRow::Group(group) => Some(group),
            _ => None,
        }
    }

    pub fn as_position(&self) -> Option<&Position> {
        match self {
            LvRow::Position(position) => Some(position),
            _ => None,
        }
    }

    pub fn as_position_mut(&mut self) -> Option<&mut Position> {
        match self {
            LvRow::Position(position) => Some(position),
            _ => None,
        }
    }

    pub fn is_group(&self) -> bool {
        matches!(self, LvRow::Group(_))
    }

    pub fn is_position(&self) -> bool {
        matches!(self, LvRow::Position(_))
    }

    pub fn is_note(&self) -> bool {
        matches!(self, LvRow::Note(_))
    }

    pub fn is_spacer(&self) -> bool {
        matches!(self, LvRow::Spacer(_))
    }

    pub fn as_note(&self) -> Option<&Note> {
        match self {
            LvRow::Note(note) => Some(note),
            _ => None,
        }
    }

    /// Textergänzungen of the row (positions, hints, Ausführungsbeschreibungen).
    pub fn complements(&self) -> &[TextComplement] {
        match self {
            LvRow::Position(position) => position.complements(),
            LvRow::Note(note) => note.complements(),
            LvRow::Performance(performance) => performance.complements(),
            LvRow::Group(_) | LvRow::Spacer(_) => &[],
        }
    }

    /// A hint without heading and text ([`Note::is_blank`]).
    pub fn is_blank_note(&self) -> bool {
        self.as_note().is_some_and(Note::is_blank)
    }

    pub fn as_performance(&self) -> Option<&PerformanceDescription> {
        match self {
            LvRow::Performance(performance) => Some(performance),
            _ => None,
        }
    }

    pub fn as_performance_mut(&mut self) -> Option<&mut PerformanceDescription> {
        match self {
            LvRow::Performance(performance) => Some(performance),
            _ => None,
        }
    }

    pub fn is_performance(&self) -> bool {
        matches!(self, LvRow::Performance(_))
    }

    /// Headings and positions carry an order number; hints and
    /// Ausführungsbeschreibungen do not.
    pub fn has_ordinal(&self) -> bool {
        matches!(self, LvRow::Group(_) | LvRow::Position(_))
    }
}

/// Sum of all positions below one heading (including deeper headings).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupTotal {
    pub group_row: usize,
    pub total: Money,
}

/// A bill of quantities (Leistungsverzeichnis) as an outline: headings carry
/// their level, positions and hints belong to the heading above them.
#[derive(Debug, Clone, PartialEq)]
pub struct Lv {
    title: String,
    /// Ausschreibung (default) or Kostenermittlung nach DIN 276, see
    /// [`crate::lv_purpose`].
    purpose: crate::lv_purpose::LvPurpose,
    /// LV number or short name (GAEB BoQ Name), e.g. "480-V08".
    number: String,
    /// Columns of the unit price breakdown (EP-Anteile, GAEB LblUPComp).
    price_components: Vec<PriceComponent>,
    /// Label of the time per unit (Zeitansatz, GAEB LblTime), e.g. "Stunden".
    time_label: String,
    /// Positions numbers may hold letters (GAEB breakdown Num = No).
    alphanumeric_positions: bool,
    oz_mask: OzMask,
    /// Names of the Gliederungsstufen (Titel, Kostengruppe …), one per mask
    /// level; empty or missing ones fall back to the defaults.
    level_labels: Vec<String>,
    /// When the Angebotsaufforderung (X83) was last exported (ISO time);
    /// renumbering after that breaks the match with the bidders' files.
    request_exported: Option<String>,
    /// Fingerprint of what the bidders got ([`Lv::content_fingerprint`]) when
    /// the LV was sent (X83 export or Aufforderung); a different fingerprint
    /// now = changed since then, send again (Anwender 18.11.).
    sent_fingerprint: Option<String>,
    /// File the LV was sent as (the X83), for the list of changes.
    sent_file: Option<String>,
    /// Step between two position numbers: 1, 10, 100 or 1000 (usual: 10).
    position_step: u32,
    vat_percent: Decimal,
    currency: String,
    /// Pauschalsumme of the whole LV (GAEB BoQInfo Totals/TotalLSUM): replaces
    /// the sum of the positions as the LV total (net, VAT, gross).
    lump_sum_total: Option<Money>,
    rows: Vec<LvRow>,
    next_id: u64,
    /// Catalog of the cost groups (GAEB Ctlg), DIN 276-18 by default.
    cost_catalog: crate::cost_group::CostCatalog,
    /// Catalog of the Leistungsbereiche (GAEB Ctlg „work category“).
    work_catalog: crate::work_category::WorkCatalog,
}

impl Lv {
    pub fn new(title: impl Into<String>, oz_mask: OzMask) -> Self {
        Self {
            title: title.into(),
            purpose: crate::lv_purpose::LvPurpose::Tender,
            number: String::new(),
            price_components: Vec::new(),
            time_label: String::new(),
            alphanumeric_positions: false,
            oz_mask,
            level_labels: Vec::new(),
            request_exported: None,
            sent_fingerprint: None,
            sent_file: None,
            position_step: DEFAULT_POSITION_STEP,
            vat_percent: Decimal::from(19),
            currency: "EUR".to_string(),
            lump_sum_total: None,
            rows: Vec::new(),
            next_id: 1,
            cost_catalog: crate::cost_group::CostCatalog::default(),
            work_catalog: crate::work_category::WorkCatalog::default(),
        }
    }

    pub fn cost_catalog(&self) -> &crate::cost_group::CostCatalog {
        &self.cost_catalog
    }

    pub fn set_cost_catalog(&mut self, catalog: crate::cost_group::CostCatalog) {
        self.cost_catalog = catalog;
    }

    pub fn work_catalog(&self) -> &crate::work_category::WorkCatalog {
        &self.work_catalog
    }

    pub fn set_work_catalog(&mut self, catalog: crate::work_category::WorkCatalog) {
        self.work_catalog = catalog;
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    /// LV number (GAEB BoQ Name); empty when not set.
    pub fn number(&self) -> &str {
        &self.number
    }

    pub fn set_number(&mut self, number: impl Into<String>) {
        self.number = number.into();
    }

    /// Columns of the EP breakdown; the positions hold one value per column.
    pub fn price_components(&self) -> &[PriceComponent] {
        &self.price_components
    }

    pub fn set_price_components(&mut self, components: Vec<PriceComponent>) {
        self.price_components = components;
    }

    pub fn time_label(&self) -> &str {
        &self.time_label
    }

    pub fn set_time_label(&mut self, label: impl Into<String>) {
        self.time_label = label.into();
    }

    pub fn has_alphanumeric_positions(&self) -> bool {
        self.alphanumeric_positions
    }

    pub fn set_alphanumeric_positions(&mut self, alphanumeric: bool) {
        self.alphanumeric_positions = alphanumeric;
    }

    pub fn oz_mask(&self) -> &OzMask {
        &self.oz_mask
    }

    pub fn set_oz_mask(&mut self, oz_mask: OzMask) {
        self.oz_mask = oz_mask;
    }

    pub fn level_count(&self) -> usize {
        self.oz_mask.level_count()
    }

    /// The labels as stored (may be shorter than the mask or empty).
    pub fn level_labels(&self) -> &[String] {
        &self.level_labels
    }

    pub fn set_level_labels(&mut self, labels: Vec<String>) {
        let mut labels: Vec<String> = labels
            .into_iter()
            .map(|label| label.trim().to_string())
            .collect();
        while labels.last().is_some_and(String::is_empty) {
            labels.pop();
        }
        self.level_labels = labels;
    }

    /// Label of one level: the LV's own or the default (Titel, Untertitel …).
    pub fn level_label(&self, level: usize) -> String {
        self.level_labels
            .get(level)
            .filter(|label| !label.is_empty())
            .cloned()
            .unwrap_or_else(|| crate::mask_spec::default_level_label(level))
    }

    /// ISO time of the last X83 export, if any.
    pub fn request_exported(&self) -> Option<&str> {
        self.request_exported.as_deref()
    }

    pub fn set_request_exported(&mut self, iso: Option<String>) {
        self.request_exported = iso.filter(|iso| !iso.trim().is_empty());
    }

    /// The LV went to the bidders now: X83 time (`iso`), the state the
    /// bidders got (fingerprint) and the file.
    pub fn mark_sent(&mut self, iso: String, file: Option<String>) {
        self.set_request_exported(Some(iso));
        self.sent_fingerprint = Some(self.content_fingerprint());
        self.sent_file = file.filter(|file| !file.trim().is_empty());
    }

    /// Fingerprint of the state sent to the bidders (saved with the project).
    pub fn sent_fingerprint(&self) -> Option<&str> {
        self.sent_fingerprint.as_deref()
    }

    pub fn set_sent_fingerprint(&mut self, fingerprint: Option<String>) {
        self.sent_fingerprint = fingerprint.filter(|text| !text.trim().is_empty());
    }

    pub fn sent_file(&self) -> Option<&str> {
        self.sent_file.as_deref()
    }

    /// Forgets that the LV went out to bidders (a copy of it was never sent).
    pub fn clear_sent(&mut self) {
        self.request_exported = None;
        self.sent_fingerprint = None;
        self.sent_file = None;
    }

    pub fn set_sent_file(&mut self, file: Option<String>) {
        self.sent_file = file.filter(|text| !text.trim().is_empty());
    }

    /// Takes the facts of sending (time, fingerprint, file) from `other` –
    /// an undo step brings the content back, not the date of an export.
    pub fn keep_sent_state_of(&mut self, other: &Lv) {
        self.request_exported = other.request_exported.clone();
        self.sent_fingerprint = other.sent_fingerprint.clone();
        self.sent_file = other.sent_file.clone();
    }

    /// The content differs from what was sent to the bidders.
    pub fn changed_since_sent(&self) -> bool {
        self.sent_fingerprint
            .as_deref()
            .is_some_and(|sent| sent != self.content_fingerprint())
    }

    /// Fingerprint of what the bidders get: order numbers, texts (long texts
    /// as text), quantities, units and kinds of all rows – not the planner's
    /// prices, status, cost groups or other office data. FNV-1a, stable
    /// between program versions (saved in the project file).
    pub fn content_fingerprint(&self) -> String {
        let mut hash = Fnv::default();
        for row in &self.rows {
            match row {
                LvRow::Group(group) => {
                    hash.add("G");
                    hash.add(group.ordinal());
                    hash.add(group.short_text());
                    hash.add(&group.level().to_string());
                }
                LvRow::Position(position) => {
                    hash.add("P");
                    hash.add(position.ordinal());
                    hash.add(position.short_text());
                    hash.add(&position.long_text().to_plain());
                    hash.add(&position.long_text().blocks().len().to_string());
                    hash.add(&position.quantity().normalize().to_string());
                    hash.add(position.unit());
                    hash.add(&format!(
                        "{:?}|{}|{}|{:?}",
                        position.kind(),
                        position.is_lump_sum(),
                        position.is_hourly(),
                        position.performance().map(|id| self.index_of(id))
                    ));
                    for complement in position.complements() {
                        hash.add(&complement.marker);
                        hash.add(&complement.caption);
                        hash.add(&complement.tail);
                    }
                }
                LvRow::Note(note) => {
                    hash.add("N");
                    hash.add(&note.text().to_plain());
                }
                LvRow::Performance(description) => {
                    hash.add("A");
                    hash.add(description.number());
                    hash.add(&description.title());
                    hash.add(&description.long_text().to_plain());
                }
                LvRow::Spacer(_) => {}
            }
        }
        format!("{:016x}", hash.0)
    }

    /// The outermost level is a Los.
    pub fn has_lot(&self) -> bool {
        self.level_count() > 0 && crate::mask_spec::is_lot_label(&self.level_label(0))
    }

    pub fn position_step(&self) -> u32 {
        self.position_step
    }

    /// The step numbering really uses: the set step, but at most what the
    /// position digits allow (an old file with „PPP“ and step 1000 counts in
    /// hundreds instead of leaving numbers that do not fit the mask).
    pub fn effective_position_step(&self) -> u32 {
        self.position_step.max(1).min(self.oz_mask.largest_step())
    }

    pub fn set_position_step(&mut self, step: u32) {
        self.position_step = step.max(1);
    }

    pub fn vat_percent(&self) -> Decimal {
        self.vat_percent
    }

    pub fn set_vat_percent(&mut self, percent: Decimal) {
        self.vat_percent = percent;
    }

    pub fn currency(&self) -> &str {
        &self.currency
    }

    /// Ausschreibung or Kostenermittlung (see [`crate::lv_purpose`]).
    pub fn purpose(&self) -> crate::lv_purpose::LvPurpose {
        self.purpose
    }

    /// Datenpunkt-Abschnitte follow: they take the new stage, a tender LV
    /// keeps their titles without data points.
    pub fn set_purpose(&mut self, purpose: crate::lv_purpose::LvPurpose) {
        self.purpose = purpose;
        self.fit_point_sections();
    }

    pub fn set_currency(&mut self, currency: impl Into<String>) {
        self.currency = currency.into();
    }

    pub fn rows(&self) -> &[LvRow] {
        &self.rows
    }

    /// Direct access for editing a row's content; use the insert/remove/move
    /// methods to change the structure so ids stay unique.
    pub fn row_mut(&mut self, index: usize) -> Option<&mut LvRow> {
        self.rows.get_mut(index)
    }

    pub fn index_of(&self, id: RowId) -> Option<usize> {
        self.rows.iter().position(|row| row.id() == id)
    }

    pub fn row_by_id(&self, id: RowId) -> Option<&LvRow> {
        self.rows.iter().find(|row| row.id() == id)
    }

    pub fn position_by_id(&self, id: RowId) -> Option<&Position> {
        self.row_by_id(id).and_then(LvRow::as_position)
    }

    pub fn position_by_ordinal(&self, ordinal: &str) -> Option<&Position> {
        let wanted = normalize_ordinal(ordinal);
        self.positions()
            .find(|position| normalize_ordinal(position.ordinal()) == wanted)
    }

    pub fn positions(&self) -> impl Iterator<Item = &Position> {
        self.rows.iter().filter_map(LvRow::as_position)
    }

    /// Id the next new row will get.
    pub fn next_row_id(&self) -> RowId {
        RowId(self.next_id)
    }

    /// Makes sure new rows get ids from `next` on (ids of deleted rows are not
    /// handed out again after loading).
    pub fn reserve_row_ids(&mut self, next: RowId) {
        self.next_id = self.next_id.max(next.0);
    }

    fn take_id(&mut self) -> RowId {
        let id = RowId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Inserts a row at `index` (clamped) and returns its new id.
    pub fn insert_row(&mut self, index: usize, mut row: LvRow) -> RowId {
        let id = if row.id() == RowId::UNSET || self.index_of(row.id()).is_some() {
            self.take_id()
        } else {
            self.next_id = self.next_id.max(row.id().0 + 1);
            row.id()
        };
        row.set_id(id);
        if let LvRow::Performance(performance) = &mut row {
            let number = performance.number().to_string();
            if number.is_empty() || self.performance_by_number(&number).is_some() {
                performance.set_number(self.next_performance_number());
            }
        }
        let index = index.min(self.rows.len());
        self.rows.insert(index, row);
        id
    }

    pub fn push_row(&mut self, row: LvRow) -> RowId {
        self.insert_row(self.rows.len(), row)
    }

    pub fn push_group(&mut self, group: LvGroup) -> RowId {
        self.push_row(LvRow::Group(group))
    }

    pub fn push_position(&mut self, position: Position) -> RowId {
        self.push_row(LvRow::Position(position))
    }

    pub fn push_note(&mut self, note: Note) -> RowId {
        self.push_row(LvRow::Note(note))
    }

    pub fn push_spacer(&mut self) -> RowId {
        self.push_row(LvRow::Spacer(Spacer::new()))
    }

    /// Removes one row; positions referring to a removed Ausführungsbeschreibung
    /// lose the reference (see [`Lv::remove_performance`] for the count).
    pub fn remove_row(&mut self, index: usize) -> Option<LvRow> {
        self.keeping_performance_order(|lv| {
            let row = (index < lv.rows.len()).then(|| lv.rows.remove(index))?;
            if row.is_performance() || row.is_position() {
                lv.unlink_missing_references();
            }
            Some(row)
        })
    }

    /// The rows a heading spans: itself and everything until the next heading
    /// on the same or an outer level. Other rows span only themselves.
    pub fn block_range(&self, index: usize) -> std::ops::Range<usize> {
        let Some(LvRow::Group(group)) = self.rows.get(index) else {
            return index..(index + 1).min(self.rows.len());
        };
        let level = group.level;
        let end = self.rows[index + 1..]
            .iter()
            .position(|row| matches!(row, LvRow::Group(other) if other.level <= level))
            .map(|offset| index + 1 + offset)
            .unwrap_or(self.rows.len());
        index..end
    }

    /// Removes a heading together with everything below it.
    pub fn remove_block(&mut self, index: usize) -> Vec<LvRow> {
        self.keeping_performance_order(|lv| lv.remove_block_inner(index))
    }

    fn remove_block_inner(&mut self, index: usize) -> Vec<LvRow> {
        if index >= self.rows.len() {
            return Vec::new();
        }
        let range = self.block_range(index);
        let removed: Vec<LvRow> = self.rows.drain(range).collect();
        if removed
            .iter()
            .any(|row| row.is_performance() || row.is_position())
        {
            self.unlink_missing_references();
        }
        removed
    }

    /// Copies a row or block and inserts it right after the original block.
    /// Returns the index of the first copied row.
    pub fn duplicate_block(&mut self, index: usize) -> Option<usize> {
        self.keeping_performance_order(|lv| lv.duplicate_block_inner(index))
    }

    fn duplicate_block_inner(&mut self, index: usize) -> Option<usize> {
        if index >= self.rows.len() {
            return None;
        }
        let range = self.block_range(index);
        let copies: Vec<LvRow> = self.rows[range.clone()].to_vec();
        let mut target = range.end;
        // Ausführungsbeschreibungen copied along get a new number; positions
        // of the copy refer to the copied description.
        let mut renamed = std::collections::HashMap::new();
        for mut row in copies {
            let old = row.id();
            row.set_id(RowId::UNSET);
            // A copy is new work: it does not take over „fertig“ or „in Bearbeitung“.
            if let LvRow::Position(position) = &mut row {
                position.set_status(crate::edit_status::EditStatus::Open);
            }
            let new = self.insert_row(target, row);
            renamed.insert(old, new);
            target += 1;
        }
        for row in &mut self.rows[range.end..target] {
            if let LvRow::Position(position) = row {
                if let Some(copy) = position.performance().and_then(|old| renamed.get(&old)) {
                    let copy = *copy;
                    position.set_performance(Some(copy));
                }
                if let Some(copy) = position.repeats().and_then(|old| renamed.get(&old)) {
                    let copy = *copy;
                    position.set_repeats(Some(copy));
                }
            }
        }
        Some(range.end)
    }

    /// Moves a row (or a heading with its block) one step up or down past its
    /// neighbouring block. Returns the new index of the moved row.
    pub fn move_block(&mut self, index: usize, up: bool) -> Option<usize> {
        self.keeping_performance_order(|lv| lv.move_block_inner(index, up))
    }

    fn move_block_inner(&mut self, index: usize, up: bool) -> Option<usize> {
        if index >= self.rows.len() {
            return None;
        }
        let range = self.block_range(index);
        if up {
            if range.start == 0 {
                return None;
            }
            let neighbour_start = self.neighbour_above(range.start, &range);
            let block: Vec<LvRow> = self.rows.drain(range).collect();
            self.rows.splice(neighbour_start..neighbour_start, block);
            Some(neighbour_start)
        } else {
            if range.end >= self.rows.len() {
                return None;
            }
            let neighbour = self.block_range_like(range.end, &range);
            let block: Vec<LvRow> = self.rows.drain(range.clone()).collect();
            let insert_at = neighbour.end - block.len();
            self.rows.splice(insert_at..insert_at, block);
            Some(insert_at)
        }
    }

    /// Start of the block directly above `start`, measured like the moved block:
    /// a heading jumps over a sibling heading block, a single row over one row
    /// (or a whole heading block of a deeper level).
    fn neighbour_above(&self, start: usize, moved: &std::ops::Range<usize>) -> usize {
        let moved_level = match &self.rows[moved.start] {
            LvRow::Group(group) => Some(group.level),
            _ => None,
        };
        match moved_level {
            Some(level) => {
                let mut candidate = start - 1;
                loop {
                    if let LvRow::Group(group) = &self.rows[candidate] {
                        if group.level <= level {
                            return candidate;
                        }
                    }
                    if candidate == 0 {
                        return 0;
                    }
                    candidate -= 1;
                }
            }
            None => start - 1,
        }
    }

    fn block_range_like(
        &self,
        start: usize,
        moved: &std::ops::Range<usize>,
    ) -> std::ops::Range<usize> {
        match &self.rows[moved.start] {
            LvRow::Group(_) => self.block_range(start),
            _ => start..start + 1,
        }
    }

    /// Moves the rows `range` so that they start before row `target` (the
    /// row count = to the end). Ids and references stay. Returns the new
    /// index of the first moved row; None when `target` lies inside the range.
    pub fn move_rows(&mut self, range: std::ops::Range<usize>, target: usize) -> Option<usize> {
        self.keeping_performance_order(|lv| lv.move_rows_inner(range, target))
    }

    fn move_rows_inner(&mut self, range: std::ops::Range<usize>, target: usize) -> Option<usize> {
        if range.is_empty() || range.end > self.rows.len() || target > self.rows.len() {
            return None;
        }
        if target >= range.start && target <= range.end {
            return None;
        }
        let length = range.len();
        let block: Vec<LvRow> = self.rows.drain(range.clone()).collect();
        let start = if target > range.start {
            target - length
        } else {
            target
        };
        self.rows.splice(start..start, block);
        Some(start)
    }

    /// Changes the level of a heading and all headings inside its block by `delta`.
    /// Fails silently when a level would leave the mask.
    pub fn shift_level(&mut self, index: usize, deeper: bool) -> bool {
        if !matches!(self.rows.get(index), Some(LvRow::Group(_))) {
            return false;
        }
        let max_level = self.level_count().saturating_sub(1);
        let range = self.block_range(index);
        let ok = self.rows[range.clone()].iter().all(|row| match row {
            LvRow::Group(group) => {
                if deeper {
                    group.level < max_level
                } else {
                    group.level > 0
                }
            }
            _ => true,
        });
        if !ok {
            return false;
        }
        for row in &mut self.rows[range] {
            if let LvRow::Group(group) = row {
                if deeper {
                    group.level += 1;
                } else {
                    group.level -= 1;
                }
            }
        }
        true
    }

    /// Where a new row goes: right after `selected`, at the end without a selection.
    pub fn insert_index(&self, selected: Option<usize>) -> usize {
        match selected {
            Some(index) => (index + 1).min(self.rows.len()),
            None => self.rows.len(),
        }
    }

    /// Whether a position may stand directly under the title at `index`: a
    /// title of the deepest level, or a title without subtitles on a level
    /// where the LV already has positions directly under a title (GAEB 3.3
    /// „unbelegte Hierarchiestufen“ – imported LVs, the Positionsbibliothek).
    pub fn holds_positions_directly(&self, index: usize) -> bool {
        let Some(level) = self
            .rows
            .get(index)
            .and_then(LvRow::as_group)
            .map(LvGroup::level)
        else {
            return false;
        };
        if level + 1 >= self.level_count() {
            return true;
        }
        let subtitles = self.rows[index + 1..]
            .iter()
            .find_map(LvRow::as_group)
            .is_some_and(|next| next.level() > level);
        if subtitles {
            return false;
        }
        let mut parent = None;
        self.rows.iter().any(|row| match row {
            LvRow::Group(group) => {
                parent = Some(group.level());
                false
            }
            LvRow::Position(_) => parent == Some(level),
            _ => false,
        })
    }

    /// Level a new heading inserted after `selected` should get: the level of
    /// the heading the selection belongs to (a sibling), 0 in an empty LV.
    pub fn level_for_new_group(&self, selected: Option<usize>) -> usize {
        let Some(index) = selected else {
            return self
                .rows
                .iter()
                .rev()
                .find_map(|row| row.as_group().map(LvGroup::level))
                .unwrap_or(0);
        };
        self.rows[..=index.min(self.rows.len().saturating_sub(1))]
            .iter()
            .rev()
            .find_map(|row| row.as_group().map(LvGroup::level))
            .unwrap_or(0)
    }

    /// Writes all order numbers: each heading counts up on its level and
    /// resets the deeper levels; positions count in `position_step` and start
    /// again below every heading. Finished positions and the titles above
    /// them keep their numbers (see [`crate::numbering`]).
    pub fn renumber(&mut self) {
        self.renumber_with(crate::numbering::RenumberMode::ALL);
    }

    /// Sets the order number of a heading or position by hand.
    ///
    /// For a heading on level n, `text` is either its own piece („481“) or the
    /// whole number („01.481“); all rows in its block move along
    /// (01.481.10.0010 → 01.482.10.0010). For a position, `text` is the
    /// position part („0015“) or the whole number. The LV keeps its own
    /// numbering afterwards (no automatic renumbering).
    pub fn set_row_number(&mut self, index: usize, text: &str) -> Result<(), String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("Bitte eine Ordnungszahl eintragen.".to_string());
        }
        // Fertige Positionen (auch im Block eines Titels) behalten ihre OZ.
        self.ensure_editable_rows(&[index])?;
        let Some(row) = self.rows.get(index) else {
            return Err("Diese Zeile gibt es nicht.".to_string());
        };
        let new_ordinal = match row {
            LvRow::Group(group) => self.group_number(index, group.level, text)?,
            LvRow::Position(_) => self.position_number(index, text)?,
            _ => {
                return Err(
                    "Hinweise und Ausführungsbeschreibungen haben keine Ordnungszahl.".to_string(),
                )
            }
        };
        let old_ordinal = self.rows[index].ordinal().to_string();
        if normalize_ordinal(&new_ordinal) == normalize_ordinal(&old_ordinal) {
            return Ok(());
        }
        let key = normalize_ordinal(&new_ordinal);
        if self.rows.iter().enumerate().any(|(other, row)| {
            other != index && row.has_ordinal() && normalize_ordinal(row.ordinal()) == key
        }) {
            return Err(format!("Die Ordnungszahl {new_ordinal} gibt es schon."));
        }
        if self.rows[index].is_group() {
            let range = self.block_range(index);
            let old_prefix = format!("{}.", old_ordinal.trim());
            for row in &mut self.rows[range.clone()] {
                if !row.has_ordinal() {
                    continue;
                }
                let current = row.ordinal().trim().to_string();
                if let Some(rest) = current.strip_prefix(&old_prefix) {
                    row.set_ordinal(format!("{new_ordinal}.{rest}"));
                }
            }
            // A heading may share its prefix with the block only by
            // position; check that nothing collides with rows outside.
            let inside: std::collections::HashSet<String> = self.rows[range.clone()]
                .iter()
                .filter(|row| row.has_ordinal())
                .map(|row| normalize_ordinal(row.ordinal()))
                .collect();
            let clash = self.rows.iter().enumerate().any(|(other, row)| {
                !range.contains(&other)
                    && row.has_ordinal()
                    && inside.contains(&normalize_ordinal(row.ordinal()))
            });
            if clash {
                // undo
                let new_prefix = format!("{new_ordinal}.");
                for row in &mut self.rows[range.clone()] {
                    let current = row.ordinal().trim().to_string();
                    if let Some(rest) = current.strip_prefix(&new_prefix) {
                        row.set_ordinal(format!("{}{rest}", old_prefix));
                    }
                }
                return Err(format!(
                    "Mit {new_ordinal} entstünden doppelte Ordnungszahlen im LV."
                ));
            }
        }
        self.rows[index].set_ordinal(new_ordinal);
        Ok(())
    }

    fn group_number(&self, index: usize, level: usize, text: &str) -> Result<String, String> {
        let mask = &self.oz_mask;
        let width = mask.level_width(level);
        let piece = text.rsplit('.').next().unwrap_or(text).trim();
        let letters = mask.level_is_letters(level);
        let valid = if letters {
            !piece.is_empty()
                && piece
                    .chars()
                    .all(|character| character.is_ascii_alphabetic())
        } else {
            !piece.is_empty() && piece.chars().all(|character| character.is_ascii_digit())
        };
        if !valid {
            return Err(format!(
                "„{piece}“ passt nicht: die {}. Stufe der OZ-Maske {} erwartet {} mit höchstens {width} Stellen.",
                level + 1,
                mask.pattern(),
                if letters { "Buchstaben" } else { "Ziffern" }
            ));
        }
        if piece.len() > width {
            return Err(format!(
                "„{piece}“ ist zu lang: die {}. Stufe der OZ-Maske {} hat {width} Stellen. \
                 Mehr Stellen unter LV › Gliederung und OZ-Maske …",
                level + 1,
                mask.pattern()
            ));
        }
        let padded = if letters {
            piece.to_ascii_uppercase()
        } else {
            format!("{:0>width$}", piece, width = width)
        };
        if !letters && padded.chars().all(|character| character == '0') {
            return Err("Eine Titelnummer aus lauter Nullen ist nicht zulässig.".to_string());
        }
        let parent = self.rows[..index]
            .iter()
            .rev()
            .filter_map(LvRow::as_group)
            .find(|group| group.level < level)
            .map(|group| group.ordinal().trim().to_string());
        Ok(match parent {
            Some(parent) if !parent.is_empty() && level > 0 => format!("{parent}.{padded}"),
            _ => padded,
        })
    }

    fn position_number(&self, index: usize, text: &str) -> Result<String, String> {
        let mask = &self.oz_mask;
        let width = mask.position_width().max(1);
        let heading = self.rows[..index]
            .iter()
            .rev()
            .find_map(LvRow::as_group)
            .map(|group| group.ordinal().trim().to_string())
            .unwrap_or_default();
        let candidate = if !heading.is_empty() && text.starts_with(&format!("{heading}.")) {
            text[heading.len() + 1..].to_string()
        } else {
            text.to_string()
        };
        let mut pieces = candidate.split('.');
        let number = pieces.next().unwrap_or_default().trim();
        let index_part = pieces.next().map(str::trim).unwrap_or_default();
        if pieces.next().is_some()
            || number.is_empty()
            || !number.chars().all(|c| c.is_ascii_digit())
        {
            return Err(format!(
                "„{text}“ passt nicht: bitte die Positionsnummer ({width} Ziffern, z. B. {}) eintragen.",
                format!("{:0>width$}", 15, width = width)
            ));
        }
        if number.len() > width {
            return Err(format!(
                "„{number}“ ist zu lang: die Positionsnummer hat {width} Stellen. \
                 Mehr Stellen unter LV › Gliederung und OZ-Maske …"
            ));
        }
        let index_width = mask.index_width();
        if index_part.len() > index_width {
            return Err(if index_width == 0 {
                "Die OZ-Maske hat keine Indexstelle.".to_string()
            } else {
                format!("Der Index hat höchstens {index_width} Stelle(n).")
            });
        }
        let number = format!("{:0>width$}", number, width = width);
        if number.chars().all(|character| character == '0') {
            return Err("Die Positionsnummer 0 ist nicht zulässig.".to_string());
        }
        let mut ordinal = if heading.is_empty() {
            number
        } else {
            format!("{heading}.{number}")
        };
        if !index_part.is_empty() {
            ordinal = format!("{ordinal}.{}", index_part.to_ascii_uppercase());
        }
        Ok(ordinal)
    }

    /// Renumbers only the positions: headings keep the numbers the user gave
    /// them (481, 482 …), positions below every heading count again in steps;
    /// finished positions keep theirs.
    pub fn renumber_positions(&mut self) {
        self.renumber_with(crate::numbering::RenumberMode {
            positions_only: true,
            keep_done: true,
            scope: None,
        });
    }

    /// Whether every order number is the one [`Lv::renumber`] would write –
    /// then structural changes may renumber; imported LVs with their own
    /// numbers must keep them.
    pub fn is_auto_numbered(&self) -> bool {
        self.rows
            .iter()
            .zip(self.numbering_plan(crate::numbering::RenumberMode::ALL))
            .all(|(row, expected)| !row.has_ordinal() || row.ordinal() == expected)
    }

    /// Inserts a row at `index` and numbers it: an automatically numbered LV
    /// is renumbered; otherwise only the new row gets a free order number
    /// between its neighbours (e.g. 0015 between 0010 and 0020).
    pub fn insert_numbered(&mut self, index: usize, row: LvRow) -> Result<RowId, String> {
        self.keeping_performance_order(|lv| lv.insert_numbered_inner(index, row))
    }

    fn insert_numbered_inner(&mut self, index: usize, row: LvRow) -> Result<RowId, String> {
        let index = index.min(self.rows.len());
        if self.is_auto_numbered() {
            // Renumber only when no finished position would get a new number;
            // otherwise the new row takes a free number between its neighbours.
            let mut trial = self.clone();
            let id = trial.insert_row(index, row.clone());
            let free = crate::numbering::RenumberMode {
                keep_done: false,
                ..crate::numbering::RenumberMode::ALL
            };
            if trial.done_positions_affected(free).is_empty() {
                trial.renumber();
                // No number left for the new row (all steps used): refuse
                // below instead of leaving it without one.
                let unnumbered = |lv: &Lv| {
                    lv.rows
                        .iter()
                        .filter(|row| row.has_ordinal() && row.ordinal().trim().is_empty())
                        .count()
                };
                let numbered = !row.has_ordinal() || unnumbered(&trial) <= unnumbered(self);
                if numbered && trial.duplicate_ordinals().is_empty() {
                    *self = trial;
                    return Ok(id);
                }
            }
        }
        self.insert_with_free_ordinal(index, row)
    }

    /// Inserts a row at `index` with a free order number between its
    /// neighbours, without renumbering anything else (the id stays if set).
    pub fn insert_with_free_ordinal(
        &mut self,
        index: usize,
        mut row: LvRow,
    ) -> Result<RowId, String> {
        let index = index.min(self.rows.len());
        if row.has_ordinal() {
            let ordinal = self.free_ordinal_at(index, &row).ok_or_else(|| {
                "An dieser Stelle ist keine Ordnungszahl frei (das LV ist nicht fortlaufend \
                 nummeriert). Erst neu nummerieren oder an anderer Stelle einfügen."
                    .to_string()
            })?;
            row.set_ordinal(ordinal);
        }
        Ok(self.insert_row(index, row))
    }

    /// A free order number for `row` if it were inserted at `index`, between
    /// its neighbours and without touching other rows. None when there is none.
    pub fn free_ordinal_at(&self, index: usize, row: &LvRow) -> Option<String> {
        let index = index.min(self.rows.len());
        let candidate = match row {
            LvRow::Note(_) | LvRow::Performance(_) | LvRow::Spacer(_) => return Some(String::new()),
            LvRow::Position(_) => self.free_position_ordinal(index)?,
            LvRow::Group(group) => self.free_group_ordinal(index, group.level)?,
        };
        let key = normalize_ordinal(&candidate);
        let taken = self
            .rows
            .iter()
            .any(|other| !other.ordinal().is_empty() && normalize_ordinal(other.ordinal()) == key);
        (!taken).then_some(candidate)
    }

    fn free_position_ordinal(&self, index: usize) -> Option<String> {
        let heading = self.rows[..index].iter().rposition(LvRow::is_group);
        let start = heading.map_or(0, |heading| heading + 1);
        let previous = self.rows[start..index]
            .iter()
            .rev()
            .find_map(LvRow::as_position)
            .map(Position::ordinal);
        let next = self.rows[index..]
            .iter()
            .take_while(|row| !row.is_group())
            .find_map(LvRow::as_position)
            .map(Position::ordinal);

        let mask = &self.oz_mask;
        let read = |ordinal: &str| -> Option<(Slot, u32)> {
            match mask.read_position(ordinal) {
                Some((levels, number)) => Some((Slot::Position(levels), number)),
                None => split_last_number(ordinal)
                    .map(|(prefix, number, width)| (Slot::Plain { prefix, width }, number)),
            }
        };
        let low = match previous {
            Some(ordinal) => Some(read(ordinal)?),
            None => None,
        };
        let high = match next {
            Some(ordinal) => Some(read(ordinal)?),
            None => None,
        };
        let slot = match (&low, &high) {
            (Some((slot, _)), _) | (None, Some((slot, _))) => slot.clone(),
            (None, None) => {
                let heading_ordinal = heading
                    .map(|heading| self.rows[heading].ordinal())
                    .unwrap_or("");
                match mask.read_group(heading_ordinal) {
                    Some(levels) => Slot::Position(levels),
                    None if heading_ordinal.trim().is_empty() => Slot::Position(Vec::new()),
                    None => Slot::Plain {
                        prefix: format!("{}.", heading_ordinal.trim()),
                        width: mask.position_width(),
                    },
                }
            }
        };
        let max = match &slot {
            Slot::Plain { width, .. } => 10u32.saturating_pow(*width as u32).saturating_sub(1),
            _ => mask.position_max(),
        };
        let number = number_between(
            low.map_or(0, |(_, number)| number),
            high.map(|(_, number)| number),
            self.effective_position_step(),
        )?;
        (number <= max).then(|| slot.render(mask, number))
    }

    fn free_group_ordinal(&self, index: usize, level: usize) -> Option<String> {
        let mut previous = None;
        let mut parent = None;
        for row in self.rows[..index].iter().rev() {
            if let LvRow::Group(group) = row {
                if group.level < level {
                    parent = Some(group);
                    break;
                }
                if group.level == level && previous.is_none() {
                    previous = Some(group);
                }
            }
        }
        let next = self.rows[index..]
            .iter()
            .filter_map(LvRow::as_group)
            .find(|group| group.level <= level)
            .filter(|group| group.level == level);

        let mask = &self.oz_mask;
        let read = |ordinal: &str| -> Option<(Slot, u32)> {
            match mask.read_group(ordinal) {
                Some(levels) if levels.len() == level + 1 => {
                    Some((Slot::Group(levels[..level].to_vec()), levels[level]))
                }
                _ => split_last_number(ordinal)
                    .map(|(prefix, number, width)| (Slot::Plain { prefix, width }, number)),
            }
        };
        let low = match previous {
            Some(group) => Some(read(group.ordinal())?),
            None => None,
        };
        let high = match next {
            Some(group) => Some(read(group.ordinal())?),
            None => None,
        };
        let slot = match (&low, &high) {
            (Some((slot, _)), _) | (None, Some((slot, _))) => slot.clone(),
            (None, None) => match parent {
                None if level == 0 => Slot::Group(Vec::new()),
                None => return None,
                Some(parent) => match mask.read_group(parent.ordinal()) {
                    Some(levels) if levels.len() == level => Slot::Group(levels),
                    _ => Slot::Plain {
                        prefix: format!("{}.", parent.ordinal().trim()),
                        width: mask.level_width(level),
                    },
                },
            },
        };
        let max = match &slot {
            Slot::Plain { width, .. } => 10u32.saturating_pow(*width as u32).saturating_sub(1),
            _ => mask.level_max(level),
        };
        let number = number_between(
            low.map_or(0, |(_, number)| number),
            high.map(|(_, number)| number),
            1,
        )?;
        (number <= max).then(|| slot.render(mask, number))
    }

    pub fn position_count(&self) -> usize {
        self.rows.iter().filter(|row| row.is_position()).count()
    }

    pub fn note_count(&self) -> usize {
        self.rows.iter().filter(|row| row.is_note()).count()
    }

    pub fn spacer_count(&self) -> usize {
        self.rows.iter().filter(|row| row.is_spacer()).count()
    }

    /// Hints without heading and text, in document order.
    pub fn blank_notes(&self) -> Vec<RowId> {
        self.rows
            .iter()
            .filter(|row| row.is_blank_note())
            .map(LvRow::id)
            .collect()
    }

    /// Turns the blank hints among `ids` into Abstandshalter (same id, same
    /// place); returns how many. Hints with text stay untouched.
    pub fn blank_notes_to_spacers(&mut self, ids: &[RowId]) -> usize {
        let mut count = 0;
        for row in &mut self.rows {
            if ids.contains(&row.id()) && row.is_blank_note() {
                let id = row.id();
                *row = LvRow::Spacer(Spacer { id });
                count += 1;
            }
        }
        count
    }

    pub fn group_count(&self) -> usize {
        self.rows.iter().filter(|row| row.is_group()).count()
    }

    pub fn performance_count(&self) -> usize {
        self.rows.iter().filter(|row| row.is_performance()).count()
    }

    // ---------- Ausführungsbeschreibungen ----------

    /// All Ausführungsbeschreibungen in document order.
    pub fn performances(&self) -> impl Iterator<Item = &PerformanceDescription> {
        self.rows.iter().filter_map(LvRow::as_performance)
    }

    pub fn performance_by_id(&self, id: RowId) -> Option<&PerformanceDescription> {
        self.row_by_id(id).and_then(LvRow::as_performance)
    }

    pub fn performance_mut(&mut self, id: RowId) -> Option<&mut PerformanceDescription> {
        self.rows
            .iter_mut()
            .find(|row| row.id() == id)
            .and_then(LvRow::as_performance_mut)
    }

    /// The description with this number ("3" and "003" are the same).
    pub fn performance_by_number(&self, number: &str) -> Option<&PerformanceDescription> {
        let wanted = performance_key(number);
        self.performances()
            .find(|performance| performance_key(performance.number()) == wanted)
    }

    /// The description a position refers to.
    pub fn performance_of(&self, position: &Position) -> Option<&PerformanceDescription> {
        position
            .performance()
            .and_then(|id| self.performance_by_id(id))
    }

    /// Highest numeric number + 1 ("1" in an LV without descriptions).
    pub fn next_performance_number(&self) -> String {
        let highest = self
            .performances()
            .filter_map(|performance| performance.number().trim().parse::<u32>().ok())
            .max()
            .unwrap_or(0);
        (highest + 1).to_string()
    }

    /// Whether the Ausführungsbeschreibungen are numbered 1, 2, 3 … in the
    /// order of the LV (true without any). While they are, proAVA keeps it
    /// that way on every insert, copy, move and delete; numbers from a GAEB
    /// file or given by hand stay until „Neu nummerieren“.
    pub fn performances_in_order(&self) -> bool {
        self.performances().enumerate().all(|(index, performance)| {
            performance_key(performance.number()) == (index + 1).to_string()
        })
    }

    /// Numbers all Ausführungsbeschreibungen 1, 2, 3 … in LV order (the
    /// positions refer to them by id and keep their reference). Returns how
    /// many numbers changed.
    pub fn renumber_performances(&mut self) -> usize {
        let mut changed = 0;
        let mut next = 1;
        for row in &mut self.rows {
            if let LvRow::Performance(performance) = row {
                let number = next.to_string();
                if performance_key(performance.number()) != number {
                    performance.set_number(number);
                    changed += 1;
                }
                next += 1;
            }
        }
        changed
    }

    /// Runs a change and renumbers the Ausführungsbeschreibungen afterwards
    /// when they were numbered 1, 2, 3 … before.
    fn keeping_performance_order<T>(&mut self, change: impl FnOnce(&mut Self) -> T) -> T {
        let in_order = self.performances_in_order();
        let result = change(self);
        if in_order && !self.performances_in_order() {
            self.renumber_performances();
        }
        result
    }

    /// Number problems the GAEB export refuses: empty, not 1–9999, or the
    /// same number twice (GAEB PerfNo must be unique).
    pub fn performance_number_problems(&self) -> Vec<(RowId, String)> {
        let mut problems = Vec::new();
        let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for performance in self.performances() {
            let number = performance.number().trim();
            let valid = number
                .parse::<u32>()
                .is_ok_and(|value| (1..=9999).contains(&value))
                && number.bytes().all(|byte| byte.is_ascii_digit());
            if number.is_empty() {
                problems.push((
                    performance.id(),
                    format!("„{}“ hat keine Nummer.", performance.title()),
                ));
            } else if !valid {
                problems.push((
                    performance.id(),
                    format!("Ausführungsbeschreibung „{number}“: die Nummer ist eine Zahl von 1 bis 9999."),
                ));
            }
            let count = seen.entry(performance_key(number)).or_insert(0);
            *count += 1;
            if *count == 2 && !number.is_empty() {
                problems.push((
                    performance.id(),
                    format!(
                        "Die Nummer {} haben mehrere Ausführungsbeschreibungen.",
                        crate::performance::performance_mark(number)
                    ),
                ));
            }
        }
        problems
    }

    /// Renumbers one description; the number must not be empty or taken. A
    /// number is stored as GAEB has it („0005“ → „5“; shown as „0005“).
    pub fn set_performance_number(&mut self, id: RowId, number: &str) -> Result<(), String> {
        let number = crate::performance::performance_number_input(number);
        let key = performance_key(number);
        let number = if number.bytes().all(|byte| byte.is_ascii_digit()) {
            key.as_str()
        } else {
            number
        };
        if number.is_empty() {
            return Err("Die Ausführungsbeschreibung braucht eine Nummer.".to_string());
        }
        if self
            .performance_by_number(number)
            .is_some_and(|other| other.id() != id)
        {
            return Err(format!(
                "Die Nummer {} hat schon eine andere Ausführungsbeschreibung.",
                crate::performance::performance_mark(number)
            ));
        }
        let performance = self
            .performance_mut(id)
            .ok_or("Die Ausführungsbeschreibung gibt es nicht (mehr).")?;
        performance.set_number(number);
        Ok(())
    }

    /// Ids of the positions referring to a description, in document order.
    pub fn positions_using(&self, performance: RowId) -> Vec<RowId> {
        self.positions()
            .filter(|position| position.performance() == Some(performance))
            .map(Position::id)
            .collect()
    }

    /// Sets or clears the reference of a position; the description must exist.
    pub fn link_performance(
        &mut self,
        position: RowId,
        performance: Option<RowId>,
    ) -> Result<(), String> {
        if let Some(id) = performance {
            if self.performance_by_id(id).is_none() {
                return Err("Die Ausführungsbeschreibung gibt es nicht (mehr).".to_string());
            }
        }
        let target = self
            .rows
            .iter_mut()
            .find(|row| row.id() == position)
            .and_then(LvRow::as_position_mut)
            .ok_or("Die Position gibt es nicht (mehr).")?;
        target.set_performance(performance);
        Ok(())
    }

    /// Removes a description and clears the references to it. Returns the
    /// description and how many positions lost their reference.
    pub fn remove_performance(&mut self, id: RowId) -> Option<(PerformanceDescription, usize)> {
        let index = self
            .index_of(id)
            .filter(|index| self.rows[*index].is_performance())?;
        let LvRow::Performance(removed) = self.rows.remove(index) else {
            unreachable!("checked above");
        };
        Some((removed, self.unlink_missing_performances()))
    }

    /// Base positions of Wiederholungsbeschreibungen: sets or clears the
    /// reference; the base must be another position of this LV.
    pub fn link_repetition(&mut self, position: RowId, base: Option<RowId>) -> Result<(), String> {
        if let Some(base) = base {
            if base == position || self.position_by_id(base).is_none() {
                return Err("Die Bezugsposition gibt es nicht (oder es ist dieselbe).".to_string());
            }
        }
        let target = self
            .rows
            .iter_mut()
            .find(|row| row.id() == position)
            .and_then(LvRow::as_position_mut)
            .ok_or("Die Position gibt es nicht (mehr).")?;
        target.set_repeats(base);
        Ok(())
    }

    /// Ids of the positions repeating `base` (Wiederholungsbeschreibungen).
    pub fn positions_repeating(&self, base: RowId) -> Vec<RowId> {
        self.positions()
            .filter(|position| position.repeats() == Some(base))
            .map(Position::id)
            .collect()
    }

    /// Clears references to Ausführungsbeschreibungen and base positions
    /// that do not exist; returns how many. Loading and importing call it
    /// once all rows are in place; removing rows calls it itself.
    pub fn unlink_missing_references(&mut self) -> usize {
        let positions: std::collections::HashSet<RowId> =
            self.positions().map(Position::id).collect();
        let mut cleared = self.unlink_missing_performances();
        for row in &mut self.rows {
            if let LvRow::Position(position) = row {
                if position
                    .repeats()
                    .is_some_and(|id| id == position.id() || !positions.contains(&id))
                {
                    position.set_repeats(None);
                    cleared += 1;
                }
                // Zuschlag on listed positions: drop the ones that are gone.
                if let Some(surcharge) = position.surcharge().filter(|surcharge| {
                    surcharge
                        .references
                        .iter()
                        .any(|reference| !positions.contains(&reference.position))
                }) {
                    let mut surcharge = surcharge.clone();
                    let before = surcharge.references.len();
                    surcharge
                        .references
                        .retain(|reference| positions.contains(&reference.position));
                    cleared += before - surcharge.references.len();
                    position.set_surcharge(Some(surcharge));
                }
            }
        }
        cleared
    }

    /// Clears references to descriptions that do not exist; returns how many.
    pub fn unlink_missing_performances(&mut self) -> usize {
        let existing: std::collections::HashSet<RowId> = self
            .performances()
            .map(PerformanceDescription::id)
            .collect();
        let mut cleared = 0;
        for row in &mut self.rows {
            if let LvRow::Position(position) = row {
                if position
                    .performance()
                    .is_some_and(|id| !existing.contains(&id))
                {
                    position.set_performance(None);
                    cleared += 1;
                }
            }
        }
        cleared
    }

    /// Planner's net total: the Pauschalsumme of the LV when it has one,
    /// otherwise the sum of the positions ([`Lv::positions_total`]).
    pub fn net_total(&self) -> Money {
        self.lump_sum_total
            .unwrap_or_else(|| self.positions_total())
    }

    /// Sum of the counted positions (Zuschläge included; a title with a
    /// Pauschalsumme counts with it), also when a Pauschalsumme of the LV
    /// replaces it as the LV total.
    pub fn positions_total(&self) -> Money {
        self.planner_row_totals().into_iter().sum()
    }

    /// Planner's counted total of every row with the Pauschalsummen of the
    /// titles: such a title carries its Pauschalsumme, the rows of its block
    /// count nothing above it.
    fn planner_row_totals(&self) -> Vec<Money> {
        let mut totals = self.planner_raw_totals();
        let mut index = 0;
        while index < self.rows.len() {
            if let LvRow::Group(group) = &self.rows[index] {
                if let Some(lump) = group.lump_sum_total {
                    let range = self.block_range(index);
                    for total in &mut totals[range.clone()] {
                        *total = Money::ZERO;
                    }
                    totals[index] = lump;
                    index = range.end;
                    continue;
                }
            }
            index += 1;
        }
        totals
    }

    /// Whether the planner priced the LV (Kostenvoranschlag): a position with
    /// a unit price other than 0 or a sum other than 0. Without, the
    /// Preisspiegel leaves the column „Planerpreis“ out.
    pub fn has_planner_prices(&self) -> bool {
        self.positions()
            .any(|position| position.unit_price().is_some_and(|price| !price.is_zero()))
            || !self.net_total().is_zero()
    }

    /// Whether any title has a Pauschalsumme.
    pub fn has_group_lump_sums(&self) -> bool {
        self.rows
            .iter()
            .any(|row| matches!(row, LvRow::Group(group) if group.lump_sum_total.is_some()))
    }

    /// Pauschalsumme of the whole LV (GAEB TotalLSUM); None = the sum of
    /// the positions is the total.
    pub fn lump_sum_total(&self) -> Option<Money> {
        self.lump_sum_total
    }

    pub fn set_lump_sum_total(&mut self, total: Option<Money>) {
        self.lump_sum_total = total;
    }

    /// Counted total of every row for a price source: `total_of` gives the
    /// counted total of an ordinary position, `price_of` the unit price of a
    /// position – for a Zuschlagsposition its percentage. Zuschläge count
    /// with base × percentage when the position counts; the Rechengrundlage
    /// of Aufteilungszeilen counts nothing (the Aufteilung carries it); other
    /// rows zero.
    pub fn row_totals_with(
        &self,
        total_of: impl Fn(&Position) -> Money,
        price_of: impl Fn(&Position) -> Option<Decimal>,
    ) -> Vec<Money> {
        let computed = crate::surcharge::compute_with(self, &total_of, &price_of);
        self.rows
            .iter()
            .map(|row| match row.as_position() {
                Some(position) if position.is_surcharge() => {
                    if position.counts_in_total() {
                        computed
                            .values
                            .get(&position.id())
                            .map(|value| value.amount)
                            .unwrap_or_default()
                    } else {
                        Money::ZERO
                    }
                }
                Some(position) if computed.bases.contains(&position.id()) => Money::ZERO,
                Some(position) => total_of(position),
                None => Money::ZERO,
            })
            .collect()
    }

    /// Rechengrundlage: the positions Aufteilungszeilen refer to – they count
    /// nothing themselves, their GP is shown in brackets.
    pub fn split_bases(&self) -> std::collections::HashSet<RowId> {
        self.split_computation().bases
    }

    /// The blocks of Aufteilungszeilen (planner's prices) with base,
    /// percentages and amount.
    pub fn split_blocks(&self) -> Vec<crate::surcharge::SplitBlock> {
        self.split_computation().blocks
    }

    /// Zuschläge, Rechengrundlage and Aufteilungsblöcke for the planner's
    /// prices at once (for drawing many rows).
    pub fn split_computation(&self) -> crate::surcharge::SurchargeComputation {
        crate::surcharge::compute_with(self, &Position::counted_total, &Position::unit_price)
    }

    /// Bezugssumme and Zuschlag of every Zuschlagsposition (planner's prices).
    pub fn surcharge_values(
        &self,
    ) -> std::collections::HashMap<RowId, crate::surcharge::SurchargeValue> {
        crate::surcharge::planner_values(self)
    }

    /// GP of a position as shown (planner's prices): Menge × EP, for a
    /// Zuschlagsposition base × percentage. Computes the LV's Zuschläge –
    /// for many rows take [`Lv::surcharge_values`] once.
    pub fn shown_total(&self, position: &Position) -> Money {
        if position.is_surcharge() {
            self.surcharge_values()
                .get(&position.id())
                .map(|value| value.amount)
                .unwrap_or_default()
        } else {
            position.total_price()
        }
    }

    pub fn vat(&self, net: Money) -> Money {
        net.percent(self.vat_percent)
    }

    /// Every heading with the sum of the counted positions in its block;
    /// a title with a Pauschalsumme has that, and titles above count it
    /// instead of its positions.
    pub fn group_totals(&self) -> Vec<GroupTotal> {
        let raw = self.planner_raw_totals();
        self.rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| row.as_group().map(|group| (index, group)))
            .map(|(index, group)| GroupTotal {
                group_row: index,
                // A Datenpunkt-Abschnitt carries its own amount on its row.
                total: group
                    .lump_sum_total
                    .unwrap_or_else(|| raw[index] + self.block_total(index, &raw)),
            })
            .collect()
    }

    /// Sum of the positions in the block of the title at `index` (planner's
    /// prices), ignoring the title's own Pauschalsumme – inner titles with
    /// one count with it; a Datenpunkt-Abschnitt counts with its amount.
    pub fn block_positions_total(&self, index: usize) -> Money {
        let raw = self.planner_raw_totals();
        raw.get(index).copied().unwrap_or_default() + self.block_total(index, &raw)
    }

    /// Planner's counted total of every row, the amount of a
    /// Datenpunkt-Abschnitt on its title row (Pauschalsummen not applied).
    fn planner_raw_totals(&self) -> Vec<Money> {
        let mut raw = self.row_totals_with(Position::counted_total, Position::unit_price);
        for (index, row) in self.rows.iter().enumerate() {
            if let LvRow::Group(group) = row {
                if let Some(points) = group.points() {
                    raw[index] += points.result().total;
                }
            }
        }
        raw
    }

    /// Indices of the Datenpunkt-Abschnitte, in their order.
    pub fn point_sections(&self) -> Vec<usize> {
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, row)| matches!(row, LvRow::Group(group) if group.is_point_section()))
            .map(|(index, _)| index)
            .collect()
    }

    /// Sum of the block of the title at `index` from the row totals `raw`;
    /// a title inside with a Pauschalsumme counts with it.
    fn block_total(&self, index: usize, raw: &[Money]) -> Money {
        let range = self.block_range(index);
        let mut total = Money::ZERO;
        let mut row = range.start + 1;
        while row < range.end {
            if let LvRow::Group(group) = &self.rows[row] {
                if let Some(lump) = group.lump_sum_total {
                    total += lump;
                    row = self.block_range(row).end;
                    continue;
                }
            }
            total += raw[row];
            row += 1;
        }
        total
    }

    /// Group sums for any price source (e.g. a bidder's prices): `total_of`
    /// as in [`Lv::row_totals_with`], `price_of` the unit price or, for a
    /// Zuschlagsposition, the percentage.
    pub fn group_totals_with(
        &self,
        total_of: impl Fn(&Position) -> Money,
        price_of: impl Fn(&Position) -> Option<Decimal>,
    ) -> Vec<GroupTotal> {
        let totals_by_row = self.row_totals_with(total_of, price_of);
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.is_group())
            .map(|(index, _)| {
                let range = self.block_range(index);
                GroupTotal {
                    group_row: index,
                    total: totals_by_row[range].iter().copied().sum(),
                }
            })
            .collect()
    }

    /// Order number a new position after `selected` would get with renumbering.
    pub fn preview_position_ordinal(&self, selected: Option<usize>) -> String {
        let mut copy = self.clone();
        let index = copy.insert_index(selected);
        copy.insert_row(
            index,
            LvRow::Position(Position::new("", "", Decimal::ZERO, "", None)),
        );
        copy.renumber();
        copy.rows[index].ordinal().to_string()
    }

    /// Order number a new heading after `selected` would get with renumbering.
    pub fn preview_group_ordinal(&self, selected: Option<usize>, level: usize) -> String {
        let mut copy = self.clone();
        let index = copy.insert_group_index(selected, level);
        copy.insert_row(index, LvRow::Group(LvGroup::new(level, "", "")));
        copy.renumber();
        copy.rows[index].ordinal().to_string()
    }

    /// A new heading on `level` goes after the whole block of the selected
    /// heading when that heading is on the same or a deeper level.
    pub fn insert_group_index(&self, selected: Option<usize>, level: usize) -> usize {
        let Some(index) = selected else {
            return self.rows.len();
        };
        if index >= self.rows.len() {
            return self.rows.len();
        }
        let after_selection = self.insert_index(Some(index));
        match self.rows[..=index]
            .iter()
            .rposition(|row| matches!(row, LvRow::Group(group) if group.level <= level))
        {
            Some(start) if self.rows[start].as_group().map(LvGroup::level) == Some(level) => {
                self.block_range(start).end.max(after_selection)
            }
            _ => after_selection,
        }
    }

    /// Positions with the same order number – a common export problem.
    pub fn duplicate_ordinals(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut duplicates = Vec::new();
        for row in &self.rows {
            if !row.has_ordinal() || row.ordinal().is_empty() {
                continue;
            }
            let key = normalize_ordinal(row.ordinal());
            if !seen.insert(key) && !duplicates.contains(&row.ordinal().to_string()) {
                duplicates.push(row.ordinal().to_string());
            }
        }
        duplicates
    }
}

/// How a free order number is written: by the mask (levels known) or by
/// copying the neighbour's prefix when its number does not follow the mask.
#[derive(Clone)]
enum Slot {
    Position(Vec<u32>),
    Group(Vec<u32>),
    Plain { prefix: String, width: usize },
}

impl Slot {
    fn render(&self, mask: &OzMask, number: u32) -> String {
        match self {
            Slot::Position(levels) => mask.format_position(levels, number, ""),
            Slot::Group(prefix) => {
                let mut levels = prefix.clone();
                levels.push(number);
                mask.format_group(&levels)
            }
            Slot::Plain { prefix, width } => format!("{prefix}{number:0width$}"),
        }
    }
}

/// A number above `low` and below `high`: the next multiple of `step` if it
/// fits, else the middle; None when the two are neighbours.
fn number_between(low: u32, high: Option<u32>, step: u32) -> Option<u32> {
    let next_multiple = (low / step + 1).checked_mul(step)?;
    match high {
        None => Some(next_multiple),
        Some(high) if high <= low.saturating_add(1) => None,
        Some(high) if next_multiple < high => Some(next_multiple),
        Some(high) => Some(low + (high - low) / 2),
    }
}

/// "1.2.10" → ("1.2.", 10, 2).
fn split_last_number(ordinal: &str) -> Option<(String, u32, usize)> {
    let trimmed = ordinal.trim();
    let (prefix, last) = match trimmed.rfind('.') {
        Some(dot) => (&trimmed[..=dot], &trimmed[dot + 1..]),
        None => ("", trimmed),
    };
    if last.is_empty() || !last.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((prefix.to_string(), last.parse().ok()?, last.len()))
}

/// "003" and "3" are the same description number.
fn performance_key(number: &str) -> String {
    let number = crate::performance::performance_number_input(number);
    if !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()) {
        let trimmed = number.trim_start_matches('0');
        if trimmed.is_empty() { "0" } else { trimmed }.to_string()
    } else {
        number.to_uppercase()
    }
}

/// Compares order numbers without dots, spaces and trailing index separators.
pub fn normalize_ordinal(ordinal: &str) -> String {
    ordinal
        .split(['.', ' '])
        .filter(|piece| !piece.is_empty())
        .collect::<Vec<_>>()
        .join(".")
}

/// FNV-1a 64 over text pieces (with a separator, so „ab“+„c“ ≠ „a“+„bc“).
struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Fnv {
    fn add(&mut self, text: &str) {
        for byte in text.bytes().chain(std::iter::once(0x1f)) {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::{Lv, LvGroup, LvRow, Note, Position};
    use crate::money::Money;
    use crate::oz_mask::OzMask;
    use crate::position::PositionKind;

    fn dec(value: i64) -> Decimal {
        Decimal::from(value)
    }

    /// Kostenvoranschlag 0,00 € (EP leer oder 0): no planner prices – the
    /// Preisspiegel leaves the column „Planerpreis“ out.
    #[test]
    fn planner_prices_are_a_price_other_than_zero() {
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "01", "Technik"));
        lv.push_position(Position::new("01.0010", "A", dec(2), "St", None));
        lv.push_position(Position::new("01.0020", "B", dec(1), "St", Some(dec(0))));
        assert!(!lv.has_planner_prices());
        lv.push_position(Position::new("01.0030", "C", dec(1), "St", Some(dec(5))));
        assert!(lv.has_planner_prices());
        // A Pauschalsumme of a title alone is a planner price too.
        let mut lump = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        lump.push_group(LvGroup::new(0, "01", "Technik"));
        lump.push_position(Position::new("01.0010", "A", dec(1), "St", None));
        if let Some(LvRow::Group(group)) = lump.row_mut(0) {
            group.set_lump_sum_total(Some(Money::from_cents(10_000)));
        }
        assert!(lump.has_planner_prices());
    }

    #[test]
    fn a_title_lump_sum_counts_above_and_keeps_its_inner_sums() {
        let mut lv = Lv::new("LV", OzMask::parse("11.22.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "01", "Rohbau"));
        lv.push_group(LvGroup::new(1, "01.01", "Erdarbeiten"));
        lv.push_position(Position::new(
            "01.01.0010",
            "A",
            dec(2),
            "St",
            Some(dec(100)),
        ));
        lv.push_group(LvGroup::new(1, "01.02", "Mauerwerk"));
        lv.push_position(Position::new(
            "01.02.0010",
            "B",
            dec(1),
            "St",
            Some(dec(300)),
        ));
        lv.push_group(LvGroup::new(0, "02", "Technik"));
        lv.push_position(Position::new("02.0010", "C", dec(1), "St", Some(dec(50))));
        assert_eq!(lv.net_total(), Money::from_cents(55_000));
        let lump = |lv: &mut Lv, index: usize, cents: i64| {
            if let Some(LvRow::Group(group)) = lv.row_mut(index) {
                group.set_lump_sum_total(Some(Money::from_cents(cents)));
            }
        };
        // 01.01 pauschal 150 instead of 200
        lump(&mut lv, 1, 15_000);
        let totals: Vec<i64> = lv.group_totals().iter().map(|t| t.total.cents()).collect();
        assert_eq!(totals, vec![45_000, 15_000, 30_000, 5_000]);
        assert_eq!(lv.net_total(), Money::from_cents(50_000));
        // 01 pauschal 400: 01.01 and 01.02 keep their own sums
        lump(&mut lv, 0, 40_000);
        let totals: Vec<i64> = lv.group_totals().iter().map(|t| t.total.cents()).collect();
        assert_eq!(totals, vec![40_000, 15_000, 30_000, 5_000]);
        assert_eq!(lv.positions_total(), Money::from_cents(45_000));
        assert!(lv.has_group_lump_sums());
    }

    #[test]
    fn a_lump_sum_replaces_the_sum_of_the_positions() {
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "01", "Titel"));
        lv.push_position(Position::new("01.0010", "A", dec(2), "St", Some(dec(100))));
        assert_eq!(lv.net_total(), Money::from_cents(20_000));
        lv.set_lump_sum_total(Some(Money::from_cents(15_000)));
        assert_eq!(lv.net_total(), Money::from_cents(15_000));
        assert_eq!(lv.positions_total(), Money::from_cents(20_000));
        // the title keeps the sum of its positions
        assert_eq!(lv.group_totals()[0].total, Money::from_cents(20_000));
    }

    fn position(text: &str, qty: i64, cents: i64) -> Position {
        Position::new("", text, dec(qty), "St", Some(Decimal::new(cents, 2)))
    }

    /// Anwender 18.11.: nach dem Versand geändert → neu versenden; Planerpreise
    /// und Bearbeitungsstand gehen die Bieter nichts an.
    #[test]
    fn changes_since_sending_count_only_what_the_bidders_got() {
        let mut lv = demo_lv();
        lv.renumber();
        assert!(!lv.changed_since_sent(), "nie versandt");
        lv.mark_sent("2026-11-18T09:00:00".into(), Some("/p/LV.X83".into()));
        assert_eq!(lv.request_exported(), Some("2026-11-18T09:00:00"));
        assert_eq!(lv.sent_file(), Some("/p/LV.X83"));
        assert!(!lv.changed_since_sent());

        let index = lv
            .positions()
            .next()
            .map(|position| lv.index_of(position.id()).unwrap())
            .unwrap();
        let position = lv.row_mut(index).unwrap().as_position_mut().unwrap();
        position.set_unit_price(Some(Decimal::new(99, 0)));
        position.set_status(crate::EditStatus::Done);
        assert!(
            !lv.changed_since_sent(),
            "Planerpreis und Status zählen nicht"
        );

        let position = lv.row_mut(index).unwrap().as_position_mut().unwrap();
        position.set_quantity(dec(2));
        assert!(lv.changed_since_sent(), "Menge geändert");
        let position = lv.row_mut(index).unwrap().as_position_mut().unwrap();
        position.set_quantity(dec(1));
        assert!(!lv.changed_since_sent(), "zurück auf den versandten Stand");

        let mut renumbered = lv.clone();
        renumbered.insert_row(index, LvRow::Note(Note::new("Neu")));
        assert!(renumbered.changed_since_sent(), "Hinweis eingefügt");
        let mut undone = lv.clone();
        undone.set_sent_fingerprint(None);
        undone.keep_sent_state_of(&lv);
        assert_eq!(undone.sent_fingerprint(), lv.sent_fingerprint());
    }

    fn demo_lv() -> Lv {
        let mask = OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap();
        let mut lv = Lv::new("GA", mask);
        lv.push_group(LvGroup::new(0, "", "Automationsstationen"));
        lv.push_group(LvGroup::new(1, "", "ASP01"));
        lv.push_position(position("DDC-Station", 1, 250000));
        lv.push_position(position("E/A-Modul", 4, 35000));
        lv.push_group(LvGroup::new(1, "", "ASP02"));
        lv.push_position(position("DDC-Station", 1, 250000));
        lv.push_group(LvGroup::new(0, "", "Feldgeräte"));
        lv.push_group(LvGroup::new(1, "", "Fühler"));
        lv.push_position(position("Temperaturfühler", 10, 8000));
        lv.renumber();
        lv
    }

    fn ordinals(lv: &Lv) -> Vec<String> {
        lv.rows()
            .iter()
            .map(|row| row.ordinal().to_string())
            .collect()
    }

    #[test]
    fn renumber_per_level_and_restarts_positions_per_heading() {
        let lv = demo_lv();
        assert_eq!(
            ordinals(&lv),
            vec![
                "01",
                "01.01",
                "01.01.0010",
                "01.01.0020",
                "01.02",
                "01.02.0010",
                "02",
                "02.01",
                "02.01.0010"
            ]
        );
    }

    #[test]
    fn step_changes_position_numbers() {
        let mut lv = demo_lv();
        lv.set_position_step(100);
        lv.renumber();
        assert_eq!(lv.rows()[3].ordinal(), "01.01.0200");
        lv.set_position_step(1);
        lv.renumber();
        assert_eq!(lv.rows()[3].ordinal(), "01.01.0002");
    }

    #[test]
    fn group_totals_include_sub_headings() {
        let lv = demo_lv();
        let totals = lv.group_totals();
        assert_eq!(totals.len(), 5);
        assert_eq!(totals[0].total, Money::from_cents(250000 + 140000 + 250000));
        assert_eq!(totals[1].total, Money::from_cents(390000));
        assert_eq!(totals[3].total, Money::from_cents(80000));
        assert_eq!(lv.net_total(), Money::from_cents(720000));
    }

    #[test]
    fn provisional_position_is_left_out_of_sums() {
        let mut lv = demo_lv();
        let index = 2;
        lv.row_mut(index)
            .and_then(LvRow::as_position_mut)
            .unwrap()
            .set_kind(PositionKind::Provisional { with_total: false });
        assert_eq!(lv.net_total(), Money::from_cents(470000));
    }

    #[test]
    fn notes_have_no_number_and_ids_are_unique() {
        let mut lv = demo_lv();
        let id = lv.insert_row(3, LvRow::Note(Note::new("Hinweis")));
        lv.renumber();
        assert_eq!(lv.rows()[3].ordinal(), "");
        assert_eq!(lv.rows()[4].ordinal(), "01.01.0020");
        let mut ids: Vec<_> = lv.rows().iter().map(LvRow::id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), lv.rows().len());
        assert_eq!(lv.index_of(id), Some(3));
    }

    #[test]
    fn loaded_ids_are_kept_and_reserved() {
        let mut lv = Lv::new("GA", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        let id = lv.push_row(LvRow::Note(Note::new("a")).with_id(super::RowId(7)));
        assert_eq!(id, super::RowId(7));
        let taken = lv.push_row(LvRow::Note(Note::new("b")).with_id(super::RowId(7)));
        assert_eq!(taken, super::RowId(8));
        lv.reserve_row_ids(super::RowId(20));
        assert_eq!(lv.next_row_id(), super::RowId(20));
        lv.reserve_row_ids(super::RowId(3));
        assert_eq!(lv.push_note(Note::new("c")), super::RowId(20));
    }

    #[test]
    fn blocks_move_duplicate_and_remove() {
        let mut lv = demo_lv();
        assert_eq!(lv.block_range(1), 1..4);
        assert_eq!(lv.block_range(0), 0..6);
        assert_eq!(lv.block_range(2), 2..3);

        let moved = lv.move_block(4, true).unwrap();
        lv.renumber();
        assert_eq!(moved, 1);
        assert_eq!(lv.rows()[1].short_text(), "ASP02");
        assert_eq!(lv.rows()[3].short_text(), "ASP01");

        let moved = lv.move_block(0, false).unwrap();
        lv.renumber();
        assert_eq!(moved, 3);
        assert_eq!(lv.rows()[0].short_text(), "Feldgeräte");
        assert_eq!(lv.rows()[0].ordinal(), "01");

        let before = lv.rows().len();
        let first_copy = lv.duplicate_block(0).unwrap();
        assert_eq!(first_copy, 3);
        assert_eq!(lv.rows().len(), before + 3);
        let removed = lv.remove_block(first_copy);
        assert_eq!(removed.len(), 3);
        assert_eq!(lv.rows().len(), before);
    }

    #[test]
    fn single_positions_move_one_row() {
        let mut lv = demo_lv();
        let moved = lv.move_block(3, true).unwrap();
        assert_eq!(moved, 2);
        assert_eq!(lv.rows()[2].short_text(), "E/A-Modul");
        assert!(lv.move_block(lv.rows().len() - 1, false).is_none());
    }

    #[test]
    fn shift_level_keeps_inside_the_mask() {
        let mut lv = demo_lv();
        assert!(!lv.shift_level(1, true));
        assert!(lv.shift_level(1, false));
        lv.renumber();
        assert_eq!(lv.rows()[1].ordinal(), "02");
        assert!(!lv.shift_level(0, false));
    }

    fn imported_lv(ordinals: &[(&str, &str)]) -> Lv {
        let mut lv = Lv::new("GA", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        for (ordinal, text) in ordinals {
            if ordinal.len() <= 5 && !text.starts_with('P') {
                let level = ordinal.matches('.').count();
                lv.push_group(LvGroup::new(level, *ordinal, *text));
            } else {
                lv.push_position(Position::new(*ordinal, *text, dec(1), "St", None));
            }
        }
        lv
    }

    fn new_position() -> LvRow {
        LvRow::Position(Position::new("", "Neu", dec(1), "St", None))
    }

    #[test]
    fn auto_numbering_is_recognised() {
        let mut lv = demo_lv();
        assert!(lv.is_auto_numbered());
        lv.insert_row(3, LvRow::Note(Note::new("Hinweis")));
        assert!(lv.is_auto_numbered());
        lv.row_mut(2)
            .and_then(LvRow::as_position_mut)
            .unwrap()
            .set_ordinal("01.01.0015");
        assert!(!lv.is_auto_numbered());
        assert!(Lv::new("leer", OzMask::parse("11.PPPP").unwrap()).is_auto_numbered());
    }

    #[test]
    fn insert_numbered_renumbers_an_automatic_lv() {
        let mut lv = demo_lv();
        let id = lv.insert_numbered(3, new_position()).unwrap();
        let index = lv.index_of(id).unwrap();
        assert_eq!(lv.rows()[index].ordinal(), "01.01.0020");
        assert_eq!(lv.rows()[index + 1].ordinal(), "01.01.0030");
        assert!(lv.is_auto_numbered());
    }

    /// Gefunden mit den Eigenschaftstests: waren alle Nummern einer Stufe
    /// belegt, kam die neue Zeile ohne OZ ins LV.
    #[test]
    fn insert_numbered_refuses_when_no_number_is_left() {
        let mut lv = Lv::new("LV", OzMask::parse("1.P").unwrap());
        lv.set_position_step(1);
        lv.push_group(LvGroup::new(0, "", "Titel"));
        for _ in 0..9 {
            lv.push_position(Position::new("", "P", Decimal::ONE, "St", None));
        }
        lv.renumber();
        assert_eq!(lv.rows().last().unwrap().ordinal(), "1.9");
        let before = lv.clone();
        assert!(lv.insert_numbered(10, new_position()).is_err());
        assert_eq!(lv, before, "nichts geändert");
        // Titles too: one digit holds nine.
        for _ in 0..8 {
            lv.insert_numbered(lv.rows().len(), LvRow::Group(LvGroup::new(0, "", "T")))
                .unwrap();
        }
        assert!(lv
            .insert_numbered(lv.rows().len(), LvRow::Group(LvGroup::new(0, "", "T")))
            .is_err());
        assert!(lv.rows().iter().all(|row| !row.ordinal().is_empty()));
    }

    #[test]
    fn insert_numbered_finds_free_numbers_in_an_imported_lv() {
        let mut lv = imported_lv(&[
            ("01", "Titel"),
            ("01.01", "Untertitel"),
            ("01.01.0010", "P1"),
            ("01.01.0020", "P2"),
            ("01.01.0040", "P3"),
            ("01.01.0041", "P4"),
            ("03", "Titel 3"),
        ]);
        assert!(!lv.is_auto_numbered());

        let id = lv.insert_numbered(3, new_position()).unwrap();
        assert_eq!(lv.row_by_id(id).unwrap().ordinal(), "01.01.0015");
        let id = lv.insert_numbered(4, new_position()).unwrap();
        assert_eq!(lv.row_by_id(id).unwrap().ordinal(), "01.01.0017");
        let id = lv.insert_numbered(2, new_position()).unwrap();
        assert_eq!(lv.row_by_id(id).unwrap().ordinal(), "01.01.0005");
        let end_of_block = lv
            .index_of(
                lv.rows()
                    .iter()
                    .find(|row| row.ordinal() == "03")
                    .unwrap()
                    .id(),
            )
            .unwrap();
        let id = lv.insert_numbered(end_of_block, new_position()).unwrap();
        assert_eq!(lv.row_by_id(id).unwrap().ordinal(), "01.01.0050");
        let between_neighbours = lv
            .index_of(
                lv.rows()
                    .iter()
                    .find(|row| row.ordinal() == "01.01.0041")
                    .unwrap()
                    .id(),
            )
            .unwrap();
        assert!(lv
            .insert_numbered(between_neighbours, new_position())
            .is_err());

        let id = lv
            .insert_numbered(
                end_of_block + 1,
                LvRow::Group(LvGroup::new(0, "", "Titel 2")),
            )
            .unwrap();
        assert_eq!(lv.row_by_id(id).unwrap().ordinal(), "02");
        let titel_3 = lv
            .rows()
            .iter()
            .position(|row| row.ordinal() == "03")
            .unwrap();
        assert!(lv
            .insert_numbered(titel_3, LvRow::Group(LvGroup::new(0, "", "zu eng")))
            .is_err());
        let id = lv.insert_numbered(lv.rows().len(), new_position()).unwrap();
        assert_eq!(lv.row_by_id(id).unwrap().ordinal(), "03.00.0010");
        assert!(lv.duplicate_ordinals().is_empty());
    }

    #[test]
    fn numbers_outside_the_mask_follow_the_neighbours() {
        let mut lv = imported_lv(&[("1", "Titel"), ("1.10", "P1"), ("1.20", "P2")]);
        let id = lv.insert_numbered(2, new_position()).unwrap();
        assert_eq!(lv.row_by_id(id).unwrap().ordinal(), "1.15");
    }

    #[test]
    fn previews_and_duplicates() {
        let lv = demo_lv();
        assert_eq!(lv.preview_position_ordinal(Some(3)), "01.01.0030");
        assert_eq!(lv.preview_group_ordinal(Some(2), 1), "01.02");
        assert_eq!(lv.insert_group_index(Some(2), 1), 4);
        assert_eq!(lv.insert_group_index(Some(2), 0), 6);
        assert!(lv.duplicate_ordinals().is_empty());
        assert!(lv.position_by_ordinal("01.02.0010").is_some());
        assert!(lv.position_by_ordinal("1.2.10").is_none());
    }

    #[test]
    fn kg_numbers_on_titles_move_their_block() {
        let mask = OzMask::parse("11.222.33.PPPP").unwrap();
        let mut lv = Lv::new("GA", mask);
        lv.push_group(LvGroup::new(0, "", "Raumautomation"));
        lv.push_group(LvGroup::new(1, "", "Automationseinrichtungen"));
        lv.push_group(LvGroup::new(2, "", "ASP"));
        lv.push_position(position("DDC", 1, 100));
        lv.push_position(position("Modul", 2, 100));
        lv.push_group(LvGroup::new(1, "", "Schaltschränke"));
        lv.push_group(LvGroup::new(2, "", "SSK"));
        lv.push_position(position("Schrank", 1, 100));
        lv.renumber();
        assert_eq!(lv.rows()[3].ordinal(), "01.001.01.0010");

        lv.set_row_number(1, "481").unwrap();
        assert_eq!(lv.rows()[1].ordinal(), "01.481");
        assert_eq!(lv.rows()[2].ordinal(), "01.481.01");
        assert_eq!(lv.rows()[4].ordinal(), "01.481.01.0020");
        lv.set_row_number(5, "01.482").unwrap();
        assert_eq!(lv.rows()[7].ordinal(), "01.482.01.0010");
        lv.set_row_number(2, "10").unwrap();
        assert_eq!(lv.rows()[3].ordinal(), "01.481.10.0010");
        assert!(!lv.is_auto_numbered());

        assert!(lv
            .set_row_number(5, "481")
            .unwrap_err()
            .contains("gibt es schon"));
        let too_long = lv.set_row_number(5, "4810").unwrap_err();
        assert!(too_long.contains("zu lang"), "{too_long}");
        assert!(
            too_long.ends_with("Mehr Stellen unter LV › Gliederung und OZ-Maske …"),
            "{too_long}"
        );
        assert!(lv.set_row_number(5, "48a").is_err());
        assert!(lv.set_row_number(5, "000").is_err());

        lv.set_row_number(4, "0015").unwrap();
        assert_eq!(lv.rows()[4].ordinal(), "01.481.10.0015");
        assert!(lv
            .set_row_number(4, "0010")
            .unwrap_err()
            .contains("gibt es schon"));
        assert!(lv.set_row_number(4, "12345").is_err());

        // A free number for a new title after 481 is 482 … 483 here (482 is taken).
        let new_title = LvRow::Group(LvGroup::new(1, "", "Management"));
        let index = lv.rows().len();
        assert_eq!(
            lv.free_ordinal_at(index, &new_title).as_deref(),
            Some("01.483")
        );

        lv.set_position_step(10);
        lv.renumber_positions();
        assert_eq!(lv.rows()[1].ordinal(), "01.481");
        assert_eq!(lv.rows()[4].ordinal(), "01.481.10.0020");
        assert_eq!(lv.rows()[7].ordinal(), "01.482.01.0010");
    }

    #[test]
    fn level_labels_fall_back_to_the_defaults() {
        let mut lv = demo_lv();
        assert_eq!(lv.level_label(0), "Titel");
        assert_eq!(lv.level_label(1), "Untertitel");
        assert!(!lv.has_lot());
        lv.set_level_labels(vec![" Los ".into(), String::new(), String::new()]);
        assert_eq!(lv.level_labels(), ["Los".to_string()]);
        assert_eq!(lv.level_label(1), "Untertitel");
        assert!(lv.has_lot());
        lv.set_level_labels(Vec::new());
        assert_eq!(lv.level_label(0), "Titel");
    }

    #[test]
    fn spacer_has_no_number_and_counts_nowhere() {
        let mut lv = demo_lv();
        let total = lv.net_total();
        let numbers = ordinals(&lv);
        // Between the two positions of ASP01 and at the very end.
        lv.insert_numbered(3, LvRow::Spacer(super::Spacer::new()))
            .unwrap();
        lv.push_spacer();
        lv.renumber();
        assert_eq!(lv.spacer_count(), 2);
        assert_eq!(lv.position_count(), 4);
        assert_eq!(lv.note_count(), 0);
        assert_eq!(lv.net_total(), total);
        let without: Vec<String> = lv
            .rows()
            .iter()
            .filter(|row| !row.is_spacer())
            .map(|row| row.ordinal().to_string())
            .collect();
        assert_eq!(without, numbers, "Abstandshalter ändern keine OZ");
        let spacer = &lv.rows()[3];
        assert!(
            spacer.is_spacer() && spacer.ordinal().is_empty() && spacer.short_text().is_empty()
        );
        assert!(!spacer.has_ordinal());
        // Ids stay unique; a moved spacer keeps its id.
        let id = spacer.id();
        let moved = lv.move_block(3, true).unwrap();
        assert_eq!(lv.rows()[moved].id(), id);
        // An LV with its own numbers: the spacer takes no number.
        let mut own = demo_lv();
        own.set_row_number(2, "0015").unwrap();
        assert!(!own.is_auto_numbered());
        let id = own
            .insert_numbered(3, LvRow::Spacer(super::Spacer::new()))
            .unwrap();
        assert_eq!(own.row_by_id(id).unwrap().ordinal(), "");
    }

    #[test]
    fn blank_hints_and_their_conversion() {
        use crate::rich_text::{RichText, TextBlock};
        let mut lv = demo_lv();
        let empty = lv.push_note(Note::new(""));
        let spaces = lv.push_note(Note::new("  \u{a0} "));
        let paragraphs = lv.push_note(Note::rich(RichText::new(vec![
            TextBlock::text(""),
            TextBlock::text("   "),
        ])));
        let mut headed = Note::new("");
        headed.set_heading("Vorbemerkung");
        let headed = lv.push_note(headed);
        let image = lv.push_note(Note::rich(RichText::new(vec![TextBlock::Image {
            file: "bild.png".into(),
            width_mm: None,
        }])));
        let text = lv.push_note(Note::new("Gilt für alle Kabel."));
        assert_eq!(lv.blank_notes(), vec![empty, spaces, paragraphs]);
        assert!(
            !lv.row_by_id(headed).unwrap().is_blank_note(),
            "Überschrift ist Inhalt"
        );
        assert!(
            !lv.row_by_id(image).unwrap().is_blank_note(),
            "Bild ist Inhalt"
        );
        let place = lv.index_of(spaces).unwrap();
        // Only blank hints change – the one with text stays.
        assert_eq!(lv.blank_notes_to_spacers(&[spaces, text]), 1);
        assert_eq!(
            lv.index_of(spaces),
            Some(place),
            "gleiche Stelle, gleiche Id"
        );
        assert!(lv.rows()[place].is_spacer());
        assert!(lv.row_by_id(text).unwrap().is_note());
        let all = lv.blank_notes();
        assert_eq!(lv.blank_notes_to_spacers(&all), 2);
        assert!(lv.blank_notes().is_empty());
        assert_eq!(lv.spacer_count(), 3);
        assert_eq!(lv.note_count(), 3);
    }
}
