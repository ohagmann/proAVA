//! Exchange model shared by the GAEB DA XML and GAEB 90 (DATAN) codecs.
//!
//! It mirrors what GAEB transports, not how proAVA stores a project. The
//! infrastructure layer maps between this model and the domain. Both codecs
//! read into and write from exactly these types, so a D84 and an X84 of the
//! same bid end up identical.

use rust_decimal::Decimal;

/// Data exchange phase (Datenaustauschphase, DP).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    /// 81 – Leistungsbeschreibung (LV ohne Mengen/Preise oder Leseexemplar)
    Dp81,
    /// 82 – Kostenanschlag / bepreistes LV (Preisanschlag)
    Dp82,
    /// 83 – Angebotsaufforderung
    Dp83,
    /// 84 – Angebotsabgabe (Rückläufer der Bieter)
    Dp84,
    /// 85 – Nebenangebot
    Dp85,
    /// 86 – Auftragserteilung
    Dp86,
}

impl Phase {
    pub fn number(self) -> u8 {
        match self {
            Phase::Dp81 => 81,
            Phase::Dp82 => 82,
            Phase::Dp83 => 83,
            Phase::Dp84 => 84,
            Phase::Dp85 => 85,
            Phase::Dp86 => 86,
        }
    }

    pub fn from_number(number: u8) -> Option<Self> {
        Some(match number {
            81 => Phase::Dp81,
            82 => Phase::Dp82,
            83 => Phase::Dp83,
            84 => Phase::Dp84,
            85 => Phase::Dp85,
            86 => Phase::Dp86,
            _ => return None,
        })
    }

    /// Phases that carry unit prices.
    pub fn has_prices(self) -> bool {
        matches!(self, Phase::Dp82 | Phase::Dp84 | Phase::Dp85 | Phase::Dp86)
    }
}

/// One GAEB file, whatever the format on disk.
#[derive(Debug, Clone, PartialEq)]
pub struct GaebDocument {
    pub phase: Phase,
    /// Program that wrote the file (GAEBInfo/ProgName, DATAN record 00).
    pub program: String,
    /// Creation date as written in the file, ISO `YYYY-MM-DD` when known.
    pub date: Option<String>,
    pub project: ProjectInfo,
    pub award: Award,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProjectInfo {
    /// Short project name (NamePrj).
    pub name: String,
    /// Project label / description (LblPrj).
    pub label: String,
    /// Currency code, normally "EUR".
    pub currency: String,
    /// Currency label (CurLbl), e.g. "Euro".
    pub currency_label: String,
    /// Number of decimal places for unit prices (UPFracDig), when given.
    pub up_fraction_digits: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Award {
    /// Client / owner (OWN), when given.
    pub owner: Option<Address>,
    /// Bidder (CTR) – set in phase 84/85.
    pub bidder: Option<Address>,
    /// Date of the bid (AwardInfo/BidDate), ISO.
    pub bid_date: Option<String>,
    /// Submission place/date/time for phase 83 (SubmLoc, OpenDate, OpenTime …).
    pub submission: Submission,
    pub boq: Boq,
    /// Award number of the client (Vergabenummer, DATAN 00 VERGAG).
    pub award_no: String,
    /// Datenaustauschnummer of the client (OWN/DPNo).
    pub owner_dp_no: String,
    /// Debitorennummer of the client (OWN/AcctRecNo).
    pub owner_account: String,
    /// State of the client (OWN/CntryType „EEA“/„Other“, CntryName = Kfz-Kennzeichen).
    pub owner_country: Country,
    /// Datenaustauschnummer of the bidder/contractor (CTR/DPNo).
    pub bidder_dp_no: String,
    /// Bedarfsträger (Requester), in file order.
    pub requesters: Vec<Place>,
    /// Ausführungsorte (CnstSite), each with its Benachrichtigungsort (NotifSite).
    pub sites: Vec<Site>,
    /// Contract terms as plain lines (DATAN T1, "vertragliche Regelungen").
    pub contract_text: Vec<String>,
    /// Award procedure (AwardInfo/Cat), e.g. "OpenCall", "NegCont".
    pub category: String,
    /// Preliminary texts (Vorbemerkungen, AddText) in document order.
    pub add_texts: Vec<AddText>,
    /// Closing texts (Schlussbemerkungen): AddText at the root after the
    /// Award, e.g. a „Schlussblatt“ for the bidder – DA XML only, not in 84.
    pub closing_texts: Vec<AddText>,
}

/// Additional text such as Vorbemerkungen (AddText).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AddText {
    /// Heading (OutlineAddText).
    pub outline: String,
    /// Body (DetailAddText).
    pub detail: RichText,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Address {
    pub name1: String,
    pub name2: String,
    pub name3: String,
    pub name4: String,
    pub street: String,
    pub postal_code: String,
    pub city: String,
    pub phone: String,
    pub email: String,
    pub country: String,
    /// International Location Number (ILN/GLN).
    pub iln: String,
    pub contact: String,
    pub fax: String,
    /// Umsatzsteuer-Identifikationsnummer (VATID).
    pub vat_id: String,
}

/// Requester, CnstSite or NotifSite: address, Kennnummer (…IDNo), Bezeichnung (…Name).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Place {
    pub address: Option<Address>,
    pub id_no: String,
    pub name: String,
}

/// An Ausführungsort (CnstSite) with its Benachrichtigungsort (NotifSite).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Site {
    pub site: Place,
    pub notification: Option<Place>,
}

/// State of a party (CntryType „EEA“ or „Other“, CntryName = Kfz-Kennzeichen, max. 3).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Country {
    pub kind: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Submission {
    pub location: String,
    pub open_date: Option<String>,
    pub open_time: Option<String>,
    pub construction_start: Option<String>,
    pub construction_end: Option<String>,
    /// End of the award period (Zuschlagsfrist, DATAN 01 DATZUS), ISO.
    pub award_deadline: Option<String>,
}

/// Leistungsverzeichnis (BoQ).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Boq {
    /// LV name / number (BoQInfo/Name).
    pub name: String,
    /// LV label (BoQInfo/LblBoQ).
    pub label: String,
    /// OZ breakdown, outermost level first, then item and index.
    pub breakdown: Vec<BreakdownLevel>,
    pub body: Vec<BoqNode>,
    pub totals: Option<Totals>,
    /// Labels of the up to six unit price components (LblUPComp1…6).
    pub up_component_labels: Vec<String>,
    /// Label of the time quantity (LblTime / DATAN 06 BEZEITAN), e.g. "Stunden".
    pub time_label: String,
    /// Kind of each unit price component (attribute Type of LblUPComp1…6:
    /// "Wages", "Materials", "Plant", "Miscellaneous"), parallel to the labels.
    pub up_component_kinds: Vec<String>,
    /// LV date (BoQInfo/Date), ISO.
    pub date: Option<String>,
    /// Catalogs the assignments refer to (BoQInfo/Ctlg), e.g. DIN 276.
    pub catalogs: Vec<Catalog>,
    /// Nachtrag the whole LV belongs to (BoQInfo/CONo, COStatus).
    pub change_order: Option<ChangeOrderTag>,
}

/// Nachtragsnummer and status (CONo, COStatus, DA XML 3.3): number 0 = main
/// contract, 1, 2 … = Nachträge; status e.g. „Filed“, „Offered“, „Approved“.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChangeOrderTag {
    pub number: u32,
    pub status: String,
}

/// A catalog (BoQInfo/Ctlg): DIN 276 cost groups, work categories …
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Catalog {
    /// CtlgID, referred to by [`CatalogAssign::catalog`].
    pub id: String,
    /// CtlgType, e.g. "cost group DIN 276 2018-12".
    pub kind: String,
    /// CtlgName, e.g. "DIN 276-18".
    pub name: String,
}

impl Catalog {
    /// Whether this catalog holds DIN 276 cost groups.
    pub fn is_cost_groups(&self) -> bool {
        self.kind.to_lowercase().starts_with("cost group din 276")
    }

    /// Whether this catalog holds work categories (Leistungsbereiche, Gewerke).
    pub fn is_work_categories(&self) -> bool {
        self.kind.trim().eq_ignore_ascii_case("work category")
    }
}

/// Assignment of an item or category to a catalog entry (CtlgAssign).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CatalogAssign {
    /// CtlgID of the catalog.
    pub catalog: String,
    /// CtlgCode, e.g. "481".
    pub code: String,
}

/// One piece of the OZ mask (BoQBkdn).
#[derive(Debug, Clone, PartialEq)]
pub struct BreakdownLevel {
    pub kind: BreakdownKind,
    pub label: String,
    pub length: u8,
    /// `Num = Yes`: digits only; otherwise alphanumeric.
    pub numeric: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakdownKind {
    /// Lot (Los) – only as the outermost level.
    Lot,
    /// Titel / Untertitel / Abschnitt.
    Level,
    /// Position.
    Item,
    /// Index digit(s) after the position.
    Index,
}

/// A node of the LV tree in document order.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum BoqNode {
    Category(Category),
    Item(Item),
    /// Free text without ordinal number (Remark / Hinweistext).
    Remark(Remark),
    /// Ausführungsbeschreibung (PerfDescr), referenced by items via
    /// [`ItemKind::reference_description`].
    PerfDescr(PerfDescr),
}

/// Ausführungsbeschreibung (PerfDescr / DATAN 41).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PerfDescr {
    /// Number (PerfNo / AUSNR), e.g. "1".
    pub number: String,
    pub label: String,
    pub short_text: String,
    pub long_text: RichText,
    /// STLB-Bau key of the text (DA XML only).
    pub stlb: Option<StlbReference>,
    /// Standardleistungsnummer of the text.
    pub standard_number: Option<StandardNumber>,
    /// Textergänzungen in the text (DA XML only).
    pub complements: Vec<TextComplement>,
}

/// Titel/Untertitel (BoQCtgy).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Category {
    /// This level's part of the ordinal number, e.g. "01".
    pub rno_part: String,
    pub label: String,
    pub body: Vec<BoqNode>,
    pub totals: Option<Totals>,
    /// Grund-/Wahlbereich (BoQCtgy/ALNBGroupNo, ALNBSerNo): serial "0" =
    /// Grundbereich, > 0 = Wahlbereich (not part of the totals).
    pub alternative: Option<Alternative>,
    /// Catalog assignments (CtlgAssign), e.g. the DIN 276 cost group.
    pub catalog_assigns: Vec<CatalogAssign>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Remark {
    pub text: RichText,
    /// Heading of the remark (OutlineText), may be empty.
    pub short_text: String,
    /// Textergänzungen in the text (STLB-Bau Standardbeschreibungen often
    /// have some), DA XML only.
    pub complements: Vec<TextComplement>,
    /// STLB-Bau key of a Standardbeschreibung („STLB-Bau Hinweis“).
    pub stlb: Option<StlbReference>,
    /// Older Standardleistungsnummer (StLNo).
    pub standard_number: Option<StandardNumber>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Totals {
    pub total: Option<Decimal>,
    pub discount_percent: Option<Decimal>,
    pub total_after_discount: Option<Decimal>,
    pub vat_percent: Option<Decimal>,
    pub total_gross: Option<Decimal>,
    /// Discount as an amount, positive = Nachlass (DATAN AUFABDM with sign inverted).
    pub discount_amount: Option<Decimal>,
    /// Lump sum replacing the computed total (Pauschalsumme, DATAN PAUSCH).
    pub lump_sum: Option<Decimal>,
    pub total_net: Option<Decimal>,
    pub vat_amount: Option<Decimal>,
}

/// Leistungsposition.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Item {
    /// Position part of the ordinal number, e.g. "0010".
    pub rno_part: String,
    /// Index part (RNoIndex), e.g. "1" – empty when the mask has none.
    pub rno_index: String,
    pub kind: ItemKind,
    pub qty: Option<Decimal>,
    /// Quantity to be filled in by the bidder (QtyTBD).
    pub qty_to_be_determined: bool,
    /// Unit (QU), e.g. "m", "St", "psch".
    pub unit: String,
    /// Unit price (UP), phases with prices only.
    pub unit_price: Option<Decimal>,
    /// Unit price components (UPComp1…6).
    pub up_components: Vec<Option<Decimal>>,
    /// Total price (IT) as written in the file.
    pub total: Option<Decimal>,
    /// Bidder marked the item as not offered.
    pub not_offered: bool,
    /// Kurztext (OutlineText), plain.
    pub short_text: String,
    /// Langtext (DetailTxt).
    pub long_text: RichText,
    /// Text supplements (Textergänzungen, TextComplement) by marker.
    pub complements: Vec<TextComplement>,
    /// Discount on this item in percent, positive = Nachlass (DATAN 23 AUFABFAK).
    pub discount_percent: Option<Decimal>,
    /// Markup percentage of a Zuschlagsposition (DATAN 23 ZUSCHPR).
    pub markup_percent: Option<Decimal>,
    /// Sum of the items the markup applies to (DATAN 23 ZUSCHSU).
    pub markup_base: Option<Decimal>,
    /// Time quantity per unit (TimeQu / DATAN 30 ZEITANS).
    pub time_quantity: Option<Decimal>,
    /// Unterbeschreibungen of a Leitposition (SubDescr / DATAN 24).
    pub sub_descriptions: Vec<SubDescription>,
    /// Position entfällt (NotAppl): kept in the LV, but without total.
    pub not_applicable: bool,
    /// Schwerpunktposition (KeyIt).
    pub key_item: bool,
    /// Bidder shall break the unit price down into components (UPBkdn).
    pub up_breakdown: bool,
    /// Set when the node is a Zuschlagsposition (MarkupItem in DA XML); its
    /// percentage and base are `markup_percent` / `markup_base`.
    pub surcharge: Option<Surcharge>,
    /// STLB-Bau key of the text (Description/STLBBau, DA XML only). Takes
    /// precedence over `standard_number`: a description carries only one.
    pub stlb: Option<StlbReference>,
    /// Standardleistungsnummer of the text (Description/StLNo, GAEB 90
    /// record 21 TEXTSYST/STLNR).
    pub standard_number: Option<StandardNumber>,
    /// Catalog assignments (CtlgAssign), e.g. the DIN 276 cost group.
    pub catalog_assigns: Vec<CatalogAssign>,
    /// Nachtrag of this item (CONo, COStatus); its status has priority over
    /// the one of the BoQ.
    pub change_order: Option<ChangeOrderTag>,
}

/// Key of a text built with STLB-Bau – Dynamische BauDaten (STLBBau).
/// With it, STLB-Bau can open, check and update the item later.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StlbReference {
    /// Catalog name (CtlgName, at most 10 characters), normally "STLB-Bau".
    pub catalog: String,
    /// Catalog version (VersDate) as `YYYY-MM`, e.g. "2023-04".
    pub version: Option<String>,
    /// Leistungsbereich (WCtg, at most 3 characters), e.g. "012".
    pub work_category: String,
    /// Identifier of the group (Group), i.e. the Leistung within the LB.
    pub group: i64,
    /// Cost group (CostGrp, at most 20 characters), e.g. DIN 276 "331".
    pub cost_group: String,
    /// Chosen characteristics (STLBBauID) in file order; the order has no
    /// meaning (GAEB rule STLBBauID 1).
    pub characteristics: Vec<StlbCharacteristic>,
}

/// One chosen characteristic of an STLB-Bau text (STLBBauID).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StlbCharacteristic {
    /// Beschreibungsmerkmal (ArtChrIdent).
    pub characteristic: i64,
    /// Index to the characteristic (ArtChIdx); none for text complements.
    pub index: Option<i64>,
    /// Chosen value, Ausprägung (ChVIdent); none for text complements.
    pub value: Option<i64>,
    /// The characteristic contributes to the Kurztext (OutlTextPart).
    pub short_text_part: bool,
}

/// Standardleistungsnummer of an older catalog text (StLNo).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StandardNumber {
    /// The number, digits only, at most 17.
    pub number: String,
    /// Catalog: "StLB", "StLK", "STLB-BauZ" or empty when the file does not say.
    pub catalog: String,
}

/// What DA XML knows about a Zuschlagsposition beyond percentage and base.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Surcharge {
    /// MarkupType: "IdentAsMark", "AllInCat" or "ListInSubQty".
    pub markup_type: String,
    /// Items for "ListInSubQty" (MarkupSubQty).
    pub items: Vec<SurchargeItem>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SurchargeItem {
    /// Full ordinal number pieces joined with ".", index included.
    pub ordinal: String,
    /// Partial quantity (SubQty); None = quantity of the item.
    pub sub_qty: Option<Decimal>,
}

/// Unterbeschreibung (SubDescr / DATAN 24).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SubDescription {
    pub number: String,
    pub qty: Option<Decimal>,
    pub unit: String,
    pub short_text: String,
    pub long_text: RichText,
    /// STLB-Bau key of the text (DA XML only).
    pub stlb: Option<StlbReference>,
    /// Standardleistungsnummer (DA XML StLNo, GAEB 90 record 24 TEXTSYST/STLNR).
    pub standard_number: Option<StandardNumber>,
}

/// Positionsart.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemKind {
    /// Bedarfsposition (Provis): None = Normalposition.
    pub provisional: Option<Provisional>,
    /// Wahlposition / Grundposition: alternative group and serial number
    /// (ALNGroupNo / ALNSerNo). Serial 0 = Grundposition, > 0 = Wahlposition.
    pub alternative: Option<Alternative>,
    /// Pauschalposition (LumpSumItem).
    pub lump_sum: bool,
    /// Stundenlohnarbeiten (HourIt).
    pub hourly: bool,
    /// Zulageposition (MarkupIt) referencing base items.
    pub markup: bool,
    /// Leitbeschreibung / Ausführungsbeschreibung referenced by others.
    pub reference_description: Option<String>,
    /// Wiederholungsposition: ordinal number of the base item, dotted (DATAN 28 OZ).
    pub repetition_of: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provisional {
    /// Bedarfsposition ohne Gesamtbetrag.
    WithoutTotal,
    /// Bedarfsposition mit Gesamtbetrag.
    WithTotal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alternative {
    pub group: String,
    pub serial: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextComplement {
    pub marker: String,
    pub kind: String,
    /// Text before the body (ComplCaption). DA XML keeps it as written,
    /// with leading/trailing spaces and line breaks (STLB-Bau Check compares
    /// it character by character); GAEB 90 gives it trimmed.
    pub caption: String,
    /// Filled-in text without the enclosing apostrophes; empty = open.
    pub body: String,
    /// Text after the body (ComplTail), kept as written like the caption.
    pub tail: String,
    /// STLB-Bau characteristic the complement belongs to (attribute
    /// ArtChrIdent, DA XML only).
    pub stlb_characteristic: Option<i64>,
    /// The body is the value of a numeric characteristic (ComplBodyDec /
    /// ComplBodyInt, DA XML only); the writer takes the value from the body.
    pub number: Option<ComplementNumber>,
    /// Where the complement stood in the long text (DA XML only); the
    /// writer still puts all complements after the text.
    pub anchor: Option<TextAnchor>,
}

/// Value type of a numeric STLB-Bau characteristic filled in by a text
/// complement (GAEB `ComplBodyDec` / `ComplBodyInt`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComplementNumber {
    Decimal,
    Integer,
}

impl ComplementNumber {
    /// Value for the `Value` attribute from the body text („1,5“, „1.5“,
    /// „1.000,5“, „220“); None when the body is no number of this kind.
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

/// Place of a text complement in the long text: block index and character
/// offset in that block's text; `own_line` = on a line of its own before
/// block `block` (which may equal the block count: at the end).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextAnchor {
    pub block: usize,
    pub offset: usize,
    pub own_line: bool,
}

/// Deepest list level (0 = outermost), as in the domain.
pub const MAX_LIST_LEVEL: u8 = 7;

/// Formatted text as GAEB carries it (a subset of XHTML in DA XML,
/// plain lines in GAEB 90). Images are never part of GAEB output.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RichText {
    pub blocks: Vec<TextBlock>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TextBlock {
    Paragraph {
        runs: Vec<Run>,
        align: Align,
    },
    /// One bullet point; `ordered` for numbered lists. `level` 0 is the
    /// outermost list, deeper levels are lists nested in a list item
    /// (`<ul>` in `<li>`), up to [`MAX_LIST_LEVEL`].
    ListItem {
        ordered: bool,
        level: u8,
        runs: Vec<Run>,
        align: Align,
    },
    /// Table with formatted cells.
    Table(Table),
    /// Image (DA XML `<image>`): the file content (JPEG, PNG, GIF …), a file
    /// name (from the file or made up from the content) and the width in mm
    /// when the file gives one. Written to GAEB only on request.
    Image {
        file_name: String,
        data: Vec<u8>,
        width_mm: Option<f32>,
    },
}

/// Horizontal alignment (`text-align`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    pub rows: Vec<Vec<Cell>>,
    /// Leading rows written as `<th>` (head of the table).
    pub header_rows: usize,
    /// Column widths in mm (`width` of the cells), `None` = automatic.
    pub column_widths_mm: Vec<Option<f32>>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Cell {
    pub runs: Vec<Run>,
    pub align: Align,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// `font-family` as written in the file.
    pub font: Option<String>,
    /// `font-size` in points.
    pub size_pt: Option<f32>,
}

impl Run {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    pub fn same_format(&self, other: &Run) -> bool {
        self.bold == other.bold
            && self.italic == other.italic
            && self.underline == other.underline
            && self.font == other.font
            && self.size_pt == other.size_pt
    }
}

impl Cell {
    pub fn plain(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            runs: if text.is_empty() {
                Vec::new()
            } else {
                vec![Run::plain(text)]
            },
            align: Align::Left,
        }
    }

    pub fn text(&self) -> String {
        self.runs.iter().map(|run| run.text.as_str()).collect()
    }
}

impl Table {
    pub fn from_texts(rows: Vec<Vec<String>>) -> Self {
        Self {
            rows: rows
                .into_iter()
                .map(|cells| cells.into_iter().map(Cell::plain).collect())
                .collect(),
            ..Self::default()
        }
    }

    pub fn texts(&self) -> Vec<Vec<String>> {
        self.rows
            .iter()
            .map(|cells| cells.iter().map(Cell::text).collect())
            .collect()
    }

    pub fn column_count(&self) -> usize {
        self.rows.iter().map(Vec::len).max().unwrap_or(0)
    }
}

impl TextBlock {
    pub fn paragraph(runs: Vec<Run>) -> Self {
        TextBlock::Paragraph {
            runs,
            align: Align::Left,
        }
    }

    pub fn list_item(ordered: bool, runs: Vec<Run>) -> Self {
        Self::list_item_at(ordered, 0, runs)
    }

    /// List item on `level` (capped at [`MAX_LIST_LEVEL`]).
    pub fn list_item_at(ordered: bool, level: u8, runs: Vec<Run>) -> Self {
        TextBlock::ListItem {
            ordered,
            level: level.min(MAX_LIST_LEVEL),
            runs,
            align: Align::Left,
        }
    }
}

impl RichText {
    pub fn plain(text: &str) -> Self {
        Self {
            blocks: text
                .split('\n')
                .map(|line| TextBlock::paragraph(vec![Run::plain(line)]))
                .collect(),
        }
    }

    /// Text without formatting, one line per block.
    pub fn to_plain(&self) -> String {
        let lines: Vec<String> = self
            .blocks
            .iter()
            .filter_map(|block| match block {
                TextBlock::Paragraph { runs, .. } => {
                    Some(runs.iter().map(|run| run.text.as_str()).collect())
                }
                TextBlock::ListItem { runs, level, .. } => {
                    let text: String = runs.iter().map(|run| run.text.as_str()).collect();
                    Some(format!(
                        "{}- {text}",
                        "  ".repeat(usize::from((*level).min(MAX_LIST_LEVEL)))
                    ))
                }
                TextBlock::Table(table) => Some(
                    table
                        .texts()
                        .iter()
                        .map(|cells| cells.join("\t"))
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                TextBlock::Image { .. } => None,
            })
            .collect();
        lines.join("\n")
    }

    pub fn is_empty(&self) -> bool {
        self.to_plain().trim().is_empty()
            && !self
                .blocks
                .iter()
                .any(|block| matches!(block, TextBlock::Image { .. }))
    }
}

impl Boq {
    /// All items in document order with their full ordinal number pieces.
    pub fn items(&self) -> Vec<(Vec<String>, &Item)> {
        let mut out = Vec::new();
        collect_items(&self.body, &mut Vec::new(), &mut out);
        out
    }
}

fn collect_items<'a>(
    nodes: &'a [BoqNode],
    path: &mut Vec<String>,
    out: &mut Vec<(Vec<String>, &'a Item)>,
) {
    for node in nodes {
        match node {
            BoqNode::Category(category) => {
                path.push(category.rno_part.clone());
                collect_items(&category.body, path, out);
                path.pop();
            }
            BoqNode::Item(item) => {
                let mut pieces = path.clone();
                pieces.push(item.rno_part.clone());
                if !item.rno_index.is_empty() {
                    pieces.push(item.rno_index.clone());
                }
                out.push((pieces, item));
            }
            BoqNode::Remark(_) | BoqNode::PerfDescr(_) => {}
        }
    }
}
