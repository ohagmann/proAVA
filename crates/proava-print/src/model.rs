use rust_decimal::Decimal;

/// Document header shared by all printouts.
#[derive(Debug, Clone, Default)]
pub struct PrintHeader {
    pub project_name: String,
    pub project_title: String,
    pub client: String,
    pub lv_name: String,
    /// Already formatted, e.g. "17.11.2026".
    pub date: String,
    pub author: String,
    /// Replaces the default document type in the page header and title block.
    pub document_kind: Option<String>,
    pub logo: Option<PrintImage>,
    /// Kopf- und Fußzeile (fields, placeholders, rules); default = standard.
    pub frame: crate::frame::PrintFrame,
    /// Druckstil and accent colour.
    pub look: PrintLook,
    /// Währung of the LV as written behind amounts and in the column heads
    /// („€“, „USD“, „CHF“); empty = „€“.
    pub currency: String,
}

impl PrintHeader {
    /// [`Self::currency`] with „€“ for an empty one.
    pub fn currency_sign(&self) -> &str {
        match self.currency.trim() {
            "" => "€",
            sign => sign,
        }
    }
}

/// Druckstil; the colours of each are set in `templates/common.typ`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrintStyle {
    /// Plain: black text, title rows tinted in the accent colour.
    #[default]
    Classic,
    /// proAVA's own look: headings in the accent colour.
    ProAva,
}

/// Blattrahmen: a line around header, content and footer of every page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageBorder {
    #[default]
    None,
    Thin,
    Medium,
}

/// Abstand nach Absatz of formatted texts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParagraphGap {
    /// In points (0 = none, as AVA printouts).
    Points(u8),
    /// Half a line of the paragraph's text.
    HalfLine,
}

impl Default for ParagraphGap {
    fn default() -> Self {
        ParagraphGap::Points(0)
    }
}

/// Druckstil, accent colour, Blattrahmen and text spacing of a printout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintLook {
    pub style: PrintStyle,
    /// „#RRGGBB“; anything else prints in [`PrintLook::DEFAULT_ACCENT`].
    pub accent: String,
    pub border: PageBorder,
    /// Zeilenabstand of formatted texts in percent of the font's own line
    /// height (100 = einfach: 1.15 × the font size, as AVA printouts).
    pub line_spacing_percent: u16,
    /// Abstand nach Absatz of formatted texts.
    pub paragraph_gap: ParagraphGap,
    /// Tabellen voll umrandet (Gitter): every cell ruled; long texts of the
    /// LV in one frame per position.
    pub table_grid: bool,
}

impl PrintLook {
    /// Petrolblau (RGB 0/79/124).
    pub const DEFAULT_ACCENT: &'static str = "#004F7C";

    /// The accent as „#RRGGBB“ if it is one, else the default.
    pub fn accent_hex(&self) -> String {
        let text = self.accent.trim();
        let digits = text.strip_prefix('#').unwrap_or(text);
        if digits.len() == 6 && digits.chars().all(|c| c.is_ascii_hexdigit()) {
            format!("#{}", digits.to_ascii_uppercase())
        } else {
            Self::DEFAULT_ACCENT.to_string()
        }
    }
}

impl Default for PrintLook {
    fn default() -> Self {
        Self {
            style: PrintStyle::Classic,
            accent: Self::DEFAULT_ACCENT.to_string(),
            border: PageBorder::None,
            line_spacing_percent: 100,
            paragraph_gap: ParagraphGap::default(),
            table_grid: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PrintImage {
    pub data: Vec<u8>,
    pub file_name: String,
    pub width_mm: Option<f32>,
    pub caption: Option<String>,
    /// `data` is a PDF: print page `page` cut to `area` – as vector graphics.
    pub clip: Option<PrintClip>,
}

/// Ausschnitt aus einer PDF-Seite in a printout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrintClip {
    /// Page from 1.
    pub page: u32,
    /// Size of the page in points (the rectangle is relative to it).
    pub page_width: f32,
    pub page_height: f32,
    pub area: crate::PdfArea,
}

impl PrintImage {
    pub fn new(data: Vec<u8>, file_name: impl Into<String>) -> Self {
        Self {
            data,
            file_name: file_name.into(),
            width_mm: None,
            caption: None,
            clip: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TextRun {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// Embedded family to print with: [`FONT_SANS`] (default), [`FONT_SERIF`]
    /// or [`FONT_MONO`]; other names fall back to Sans.
    pub font: Option<String>,
    /// Size in points; `None` = size of the surrounding text.
    pub size_pt: Option<f32>,
    /// The gap of a Bieterangabe (a line of dots): printed in one piece,
    /// never broken across lines.
    pub gap: bool,
    /// Part of a Textergänzung on a line of its own: stays in the
    /// Kurztext-LV under the short text (the bidder fills it in).
    pub entry: bool,
}

pub const FONT_SANS: &str = "Liberation Sans";
pub const FONT_SERIF: &str = "Liberation Serif";
pub const FONT_MONO: &str = "Liberation Mono";

impl TextRun {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    pub fn bold(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bold: true,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

impl TextAlign {
    pub(crate) fn name(self) -> &'static str {
        match self {
            TextAlign::Left => "left",
            TextAlign::Center => "center",
            TextAlign::Right => "right",
            TextAlign::Justify => "justify",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TableCell {
    pub runs: Vec<TextRun>,
    pub align: TextAlign,
}

impl TableCell {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            runs: vec![TextRun::plain(text)],
            align: TextAlign::Left,
        }
    }
}

#[derive(Debug, Clone)]
pub enum TextBlock {
    Paragraph {
        runs: Vec<TextRun>,
        align: TextAlign,
    },
    /// Bullet point; level 0 is the outermost. `marker`: „1.“, „a)“ …
    /// for numbered items; None = bullet „•“ (every level).
    ListItem {
        level: u8,
        marker: Option<String>,
        runs: Vec<TextRun>,
        align: TextAlign,
    },
    /// Table with framed cells; the first `header_rows` rows are printed
    /// bold on a tinted background and repeated after a page break.
    Table {
        header_rows: usize,
        rows: Vec<Vec<TableCell>>,
        /// Column widths in mm, `None` = share of the remaining width.
        widths_mm: Vec<Option<f32>>,
    },
    Image(PrintImage),
}

impl TextBlock {
    pub fn paragraph(runs: Vec<TextRun>) -> Self {
        Self::Paragraph {
            runs,
            align: TextAlign::Left,
        }
    }

    pub fn text(text: impl Into<String>) -> Self {
        Self::paragraph(vec![TextRun::plain(text)])
    }

    pub fn bullet(text: impl Into<String>) -> Self {
        Self::ListItem {
            level: 0,
            marker: None,
            runs: vec![TextRun::plain(text)],
            align: TextAlign::Left,
        }
    }

    /// Table of plain texts.
    pub fn table(header: bool, rows: Vec<Vec<String>>) -> Self {
        Self::Table {
            header_rows: usize::from(header && !rows.is_empty()),
            rows: rows
                .into_iter()
                .map(|cells| cells.into_iter().map(TableCell::plain).collect())
                .collect(),
            widths_mm: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PositionKind {
    /// Bedarfsposition
    Optional,
    /// Wahlposition
    Alternative,
    /// Grundposition
    Base,
    /// Pauschal
    LumpSum,
    /// Stundenlohn
    Hourly,
    /// Zulage
    Surcharge,
    Other(String),
}

impl PositionKind {
    pub fn label(&self) -> &str {
        match self {
            Self::Optional => "Bedarfsposition",
            Self::Alternative => "Wahlposition",
            Self::Base => "Grundposition",
            Self::LumpSum => "Pauschal",
            Self::Hourly => "Stundenlohn",
            Self::Surcharge => "Zulage",
            Self::Other(text) => text,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PrintPosition {
    pub oz: String,
    pub short_text: String,
    pub long_text: Vec<TextBlock>,
    /// Mengenermittlung under the position (print option „mit
    /// Mengenermittlung“); empty = none or not printed.
    pub takeoff: Vec<PrintTakeoffLine>,
    pub quantity: Option<Decimal>,
    pub unit: String,
    pub unit_price: Option<Decimal>,
    pub total_price: Option<Decimal>,
    pub kinds: Vec<PositionKind>,
    /// False for positions whose total is not part of the title sum
    /// (e.g. Bedarfs- or Wahlposition); their GP is printed in brackets.
    pub in_total: bool,
    /// EP-Aufgliederung: (label, value) per part, e.g. („Lohn“, 120). Empty =
    /// the LV asks for no breakdown. The bidder fills in the blank ones.
    pub price_parts: Vec<(String, Option<Decimal>)>,
    /// Lines under the short text, also without long texts: „Ausführung nach
    /// Ausführungsbeschreibung 3 – Kabeltrassen“, „Position entfällt“ …
    pub remarks: Vec<String>,
    /// Kostengruppe (DIN 276) that applies, own or from the title, e.g. „481“;
    /// None = not printed.
    pub cost_group: Option<String>,
    /// Leistungsbereich that applies, e.g. „070“; None = not printed.
    pub work_category: Option<String>,
    /// Zuschlagsposition: Bezugssumme in the quantity column, the percentage
    /// in the unit price column (both None in an LV without prices: the
    /// bidder fills in the percentage).
    pub surcharge: Option<Box<PrintSurcharge>>,
}

/// Bezugssumme and percentage of a Zuschlagsposition.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PrintSurcharge {
    pub base: Option<Decimal>,
    pub percent: Option<Decimal>,
}

impl Default for PrintPosition {
    fn default() -> Self {
        Self {
            oz: String::new(),
            short_text: String::new(),
            long_text: Vec::new(),
            takeoff: Vec::new(),
            quantity: None,
            unit: String::new(),
            unit_price: None,
            total_price: None,
            kinds: Vec::new(),
            in_total: true,
            price_parts: Vec::new(),
            remarks: Vec::new(),
            cost_group: None,
            work_category: None,
            surcharge: None,
        }
    }
}

impl PrintPosition {
    /// GP as given, otherwise quantity × EP rounded to cents.
    pub fn effective_total(&self) -> Option<Decimal> {
        self.total_price.or_else(|| {
            let total = self.quantity? * self.unit_price?;
            Some(crate::format::round_money(total))
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct PrintTitle {
    pub oz: String,
    /// Stufenbezeichnung (GAEB), e.g. „Los“, „Titel“, „Abschnitt“; may be empty.
    pub label: String,
    pub text: String,
    /// Title sum; computed from the contained items if `None`.
    pub total: Option<Decimal>,
    /// The sum is the title's Pauschalsumme (GAEB TotalLSUM).
    pub lump: bool,
    pub items: Vec<PrintItem>,
}

#[derive(Debug, Clone)]
pub enum PrintItem {
    Title(PrintTitle),
    Position(PrintPosition),
    /// Free text without OZ (Hinweistext).
    Note(Vec<TextBlock>),
    /// Vorbemerkung (GAEB `AddText`): over the full width, without the mark
    /// „Hinweis“ – its own heading says what it is.
    Preliminary(Vec<TextBlock>),
    /// Schlussbemerkung (GAEB `AddText` after the LV, e.g. a „Schlussblatt“):
    /// printed after the totals, on a page of its own, wherever it stands.
    Closing(Vec<TextBlock>),
    /// Ausführungsbeschreibung: its mark for the OZ column („A0003“) and its
    /// text, beginning with the heading.
    Performance(String, Vec<TextBlock>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LvPrintMode {
    /// Angebotsaufforderung: no prices, blank lines to fill in.
    Blank,
    /// Bepreistes LV (Kostenvoranschlag).
    Priced,
}

#[derive(Debug, Clone)]
pub struct PrintLv {
    pub header: PrintHeader,
    pub items: Vec<PrintItem>,
    /// Computed from the items if `None`.
    pub net_total: Option<Decimal>,
    pub vat_percent: Decimal,
    /// Parties printed under the title block, e.g. („Auftraggeber“, „Stadt …“);
    /// empty for an ordinary LV.
    pub parties: Vec<(String, String)>,
    /// Discount on the net sum (label, positive amount), e.g. („Nachlass 3 %“, 1200).
    pub discount: Option<(String, Decimal)>,
    /// Cover sheet with the project's master data (Deckblatt); the LV starts on the next page.
    pub cover: Option<PrintCover>,
    /// Pauschalsumme of the LV (GAEB TotalLSUM): printed under the sum of
    /// the positions; `net_total` is then the Pauschalsumme.
    pub lump_sum: Option<Decimal>,
}

/// Form of the Deckblatt (layout in `templates/cover.typ`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CoverStyle {
    /// Left aligned from top to bottom, framed bidder field at the bottom.
    #[default]
    Standard,
    /// Like a letter: Vergabestelle top left, key facts in a box on the right.
    Letterhead,
    /// Addresses (one block per party) and facts in fully ruled tables.
    Table,
    /// Like [`CoverStyle::Table`], each address field in a row of its own.
    FieldTable,
}

/// Which blocks the Deckblatt shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoverBlocks {
    pub logo: bool,
    pub addresses: bool,
    pub facts: bool,
    /// Fields for the bidder (Angebotsaufforderung only).
    pub bidder: bool,
}

impl Default for CoverBlocks {
    fn default() -> Self {
        Self {
            logo: true,
            addresses: true,
            facts: true,
            bidder: true,
        }
    }
}

/// Deckblatt and the following page „Vertragsbedingungen“. Order and labels
/// of facts and addresses are set in `templates/cover.typ`; here only the
/// values by key. Empty values are left out.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PrintCover {
    pub style: CoverStyle,
    pub blocks: CoverBlocks,
    /// e.g. „LV 480 Gebäudeautomation“.
    pub lv: String,
    /// Projektbezeichnung (or the project name).
    pub project: String,
    /// Baumaßnahme, may have several lines.
    pub measure: String,
    /// Kenndaten by key: projektnummer, vergabeart, vergabenummer,
    /// abgabetermin, abgabeort, bindefrist, ausfuehrung, weitere_fristen, stand.
    pub facts: Vec<(String, String)>,
    /// Address blocks by key (bauherr, auftraggeber, planer) with their lines.
    pub addresses: Vec<(String, Vec<String>)>,
    /// The same addresses field by field for [`CoverStyle::FieldTable`]:
    /// name, zusatz, strasse, ort, telefon, email.
    pub address_fields: Vec<(String, Vec<(String, String)>)>,
    /// Folgeseite „Vertragsbedingungen“: sections of label/value lines.
    pub sections: Vec<CoverSection>,
    /// Free text under the sections (Zahlungsbedingungen, weitere Vereinbarungen …).
    pub notes: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CoverSection {
    pub heading: String,
    pub rows: Vec<(String, String)>,
}

impl Default for PrintLv {
    fn default() -> Self {
        Self {
            header: PrintHeader::default(),
            items: Vec::new(),
            net_total: None,
            vat_percent: Decimal::new(19, 0),
            parties: Vec::new(),
            discount: None,
            cover: None,
            lump_sum: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PrintBidder {
    pub name: String,
    /// Free line behind the name (e.g. „Nebenangebot“).
    pub note: String,
    /// Date of the bid (TT.MM.JJJJ), column „Angebot vom“; empty = unknown.
    pub date: String,
    /// Net offer sum before discount; computed from the positions if `None`.
    pub net_total: Option<Decimal>,
    pub discount_percent: Option<Decimal>,
    /// Absolute discount; used instead of the percentage if set.
    pub discount_amount: Option<Decimal>,
    /// Net after discount with the current LV quantities, when quantities
    /// changed after the bids came in (information, not ranked).
    pub after_discount_at_lv_quantities: Option<Decimal>,
    /// Order numbers of the bid that are not in the LV (not ranked).
    pub unknown_ordinals: Vec<String>,
    /// Positions whose stated GP is not quantity × EP („!“; ranked with
    /// quantity × EP).
    pub arithmetic_errors: Vec<PcArithmeticError>,
    /// Strukturabweichungen of the file: summary („Strukturabweichungen: 6
    /// umnummeriert …“), the file sum against the ranked one, and every
    /// deviation as a line (appendix when there are many).
    pub structure: Option<PcStructure>,
}

/// The Abweichungsprotokoll of a bid for the print.
#[derive(Debug, Clone, Default)]
pub struct PcStructure {
    pub summary: String,
    pub sum_note: Option<String>,
    pub lines: Vec<String>,
}

/// A Rechenfehler of a bidder: the GP of the bid and quantity × EP.
#[derive(Debug, Clone, Default)]
pub struct PcArithmeticError {
    pub oz: String,
    pub stated: Decimal,
    pub computed: Decimal,
}

#[derive(Debug, Clone, Default)]
pub struct BidPrice {
    pub unit_price: Option<Decimal>,
    pub total_price: Option<Decimal>,
    /// The unit price deviates more than allowed from the reference („?“).
    pub outlier: bool,
    /// The bidder's GP is not quantity × unit price („!“).
    pub error: bool,
}

#[derive(Debug, Clone)]
pub struct PcPosition {
    pub oz: String,
    pub short_text: String,
    pub quantity: Option<Decimal>,
    pub unit: String,
    pub kinds: Vec<PositionKind>,
    pub in_total: bool,
    /// One entry per bidder, same order as `PrintPriceComparison::bidders`.
    pub prices: Vec<BidPrice>,
    /// The planner's price (Kostenvoranschlag), shown when
    /// [`PrintPriceComparison::estimate`] is set.
    pub estimate: BidPrice,
    /// LV quantity changed after the bids came in: „Achtung Massenmehrung:
    /// LV-Menge 12 statt 10 St (+2)“; empty = none.
    pub quantity_note: String,
}

impl Default for PcPosition {
    fn default() -> Self {
        Self {
            oz: String::new(),
            short_text: String::new(),
            quantity: None,
            unit: String::new(),
            kinds: Vec::new(),
            in_total: true,
            prices: Vec::new(),
            estimate: BidPrice::default(),
            quantity_note: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PcTitle {
    pub oz: String,
    pub text: String,
    /// Per bidder; computed from the contained items where missing.
    pub totals: Vec<Option<Decimal>>,
    /// The planner's sum of the title.
    pub estimate: Option<Decimal>,
    pub items: Vec<PcItem>,
}

#[derive(Debug, Clone)]
pub enum PcItem {
    Title(PcTitle),
    Position(PcPosition),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaperSize {
    /// A4 up to three bidders, A3 above.
    #[default]
    Auto,
    A4,
    A3,
}

#[derive(Debug, Clone)]
pub struct PrintPriceComparison {
    pub header: PrintHeader,
    pub bidders: Vec<PrintBidder>,
    pub items: Vec<PcItem>,
    pub vat_percent: Decimal,
    pub paper: PaperSize,
    /// Column „Planerpreis“ (LV-Schätzpreis) with the planner's prices and sums.
    pub estimate: bool,
    /// The planner's net total of the LV.
    pub estimate_net: Option<Decimal>,
    /// What „?“ means, e.g. „EP weicht mehr als 20 % vom Mittelwert ab“; empty = no outliers marked.
    pub outlier_note: String,
    /// Skonto of the contract terms, shown for information only (not ranked).
    pub cash_discount_percent: Option<Decimal>,
    /// e.g. „bei Zahlung innerhalb von 14 Tagen“.
    pub cash_discount_note: String,
}

impl Default for PrintPriceComparison {
    fn default() -> Self {
        Self {
            header: PrintHeader::default(),
            bidders: Vec::new(),
            items: Vec::new(),
            vat_percent: Decimal::new(19, 0),
            paper: PaperSize::Auto,
            estimate: false,
            estimate_net: None,
            outlier_note: String::new(),
            cash_discount_percent: None,
            cash_discount_note: String::new(),
        }
    }
}

/// Kosten nach Kostengruppen (DIN 276): sums per cost group for the
/// planner's prices and per bidder.
#[derive(Debug, Clone, Default)]
pub struct PrintCostGroups {
    pub header: PrintHeader,
    /// Catalog name, e.g. "DIN 276-18".
    pub catalog: String,
    pub bidders: Vec<String>,
    pub lines: Vec<PrintCostGroupLine>,
    pub planner_total: Decimal,
    pub bidder_totals: Vec<Decimal>,
}

#[derive(Debug, Clone, Default)]
pub struct PrintCostGroupLine {
    /// Three digits; empty = ohne Kostengruppe.
    pub code: String,
    pub label: String,
    /// Sum of a group of ten (e.g. 480 over 481–489).
    pub subtotal: bool,
    pub positions: usize,
    pub planner: Decimal,
    pub bidders: Vec<Decimal>,
}

/// Änderungsliste: differences of an LV against another state.
#[derive(Debug, Clone, Default)]
pub struct PrintChangeList {
    pub header: PrintHeader,
    /// A4 quer (the dialog's „Seite“).
    pub landscape: bool,
    /// What was compared against, e.g. „DLZ_…_F.X83 (Stand 12.11.2026)“.
    pub against: String,
    /// „1 neu, 2 entfallen …“
    pub summary: String,
    pub entries: Vec<PrintChange>,
}

#[derive(Debug, Clone, Default)]
pub struct PrintChange {
    /// neu, entfallen, verschoben, geändert
    pub kind: String,
    /// Position, Titel, Hinweis, Ausführungsbeschreibung
    pub row_kind: String,
    pub ordinal: String,
    /// Old OZ when it changed.
    pub old_ordinal: String,
    pub text: String,
    /// „Menge: 652 → 150“ …
    pub details: Vec<String>,
}

/// One line of the Mengenermittlung of a position: place, Rechenansatz and
/// its result (None = the Rechenansatz is wrong; printed as „Fehler“).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PrintTakeoffLine {
    pub label: String,
    pub formula: String,
    pub result: Option<Decimal>,
}

impl PrintPosition {
    /// Sum of the valid lines of the Mengenermittlung (None without lines).
    pub fn takeoff_sum(&self) -> Option<Decimal> {
        (!self.takeoff.is_empty()).then(|| {
            self.takeoff
                .iter()
                .filter_map(|line| line.result)
                .sum::<Decimal>()
                .normalize()
        })
    }
}
