//! What the print dialog asks for: document kind, options, range.

use proava_domain::{BidId, CostEstimateId, InvoiceId, RowId};

use crate::exchange::{ComparisonPrint, PdfLogo};

/// Kinds of printout the print dialog offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrintKind {
    /// Änderungsliste of the LV-Vergleich – printed from its window
    /// ([`PrintKind::FROM_WINDOWS`], not in [`PrintKind::ALL`]).
    ChangeList,
    /// Prüfung Nachtragsangebot of one Nachtrag (Reiter Nachträge).
    ChangeOrderCheck,
    /// Nachtragsübersicht of the LV (Reiter Nachträge).
    ChangeOrderOverview,
    /// Schreiben an Bieter oder Bauherrn (Schriftverkehr).
    Letters,
    /// LV ohne Preise (Angebotsaufforderung, Blanko-LV): fields to fill in.
    RequestForQuotation,
    /// LV mit Preisen with the planner's prices – the Kostenvoranschlag (DIN 276, 4.3.5).
    PricedLv,
    /// Kurztext-LV: priced, without long texts.
    ShortTextLv,
    /// Zusammenstellung / Summenblatt: titles with their sums only.
    Summary,
    /// Auftrags-LV of the awarded (or chosen) bid.
    Contract,
    /// Preisspiegel of all bids.
    PriceComparison,
    /// LV with the prices of one bid.
    BidPrices,
    /// Niederschrift über die Angebotsöffnung (Submissionsprotokoll).
    Submission,
    /// Kosten nach Kostengruppen (DIN 276), Planer und Bieter.
    CostGroups,
    /// Abrechnung: Aufmaßprüfung of one invoice (Rechnung gegen geprüft je Position).
    BillingCheck,
    /// Abrechnung: Rechnungsfreigabe of one invoice (Rechenweg, Unterschriften).
    Release,
    /// Kostenschätzung (2. Ebene) oder Kostenberechnung (3. Ebene) nach DIN 276.
    CostEstimate,
    /// Kostenverfolgung: Budget bis Kostenfeststellung je KG (DIN 276, 4.4–4.6).
    CostTracking,
    /// Kostenschätzung oder Kostenberechnung als Kurztext-LV (Kosten-LV, the
    /// active LV): Angaben nach DIN 276, Zusammenstellung nach Kostengruppen,
    /// Kurztext-Liste.
    CostLv,
}

impl PrintKind {
    pub const ALL: [PrintKind; 14] = [
        PrintKind::RequestForQuotation,
        PrintKind::PricedLv,
        PrintKind::ShortTextLv,
        PrintKind::Summary,
        PrintKind::Contract,
        PrintKind::PriceComparison,
        PrintKind::BidPrices,
        PrintKind::Submission,
        PrintKind::CostGroups,
        PrintKind::BillingCheck,
        PrintKind::Release,
        PrintKind::CostEstimate,
        PrintKind::CostLv,
        PrintKind::CostTracking,
    ];

    /// Name in the print dialog, the Drucken menu and the command palette –
    /// as a planner says it: first the two LVs without and with prices.
    pub fn label(self) -> &'static str {
        match self {
            PrintKind::ChangeList => "Änderungsliste (LV-Vergleich)",
            PrintKind::ChangeOrderCheck => "Prüfung Nachtragsangebot",
            PrintKind::ChangeOrderOverview => "Nachtragsübersicht",
            PrintKind::Letters => "Schreiben (Schriftverkehr)",
            PrintKind::RequestForQuotation => "LV ohne Preise (Angebotsaufforderung)",
            PrintKind::PricedLv => "LV mit Preisen (Kostenvoranschlag)",
            PrintKind::ShortTextLv => "Kurztext-LV",
            PrintKind::Summary => "Zusammenstellung (Summenblatt)",
            PrintKind::Contract => "Auftrags-LV",
            PrintKind::PriceComparison => "Preisspiegel",
            PrintKind::BidPrices => "LV mit den Preisen eines Bieters",
            PrintKind::Submission => "Niederschrift über die Angebotsöffnung",
            PrintKind::CostGroups => "Kosten nach Kostengruppen",
            PrintKind::BillingCheck => "Aufmaßprüfung einer Rechnung",
            PrintKind::Release => "Rechnungsfreigabe",
            PrintKind::CostEstimate => "Kostenschätzung / Kostenberechnung (DIN 276)",
            PrintKind::CostTracking => "Kostenverfolgung (DIN 276)",
            PrintKind::CostLv => "Kostenermittlung (Kosten-LV, DIN 276)",
        }
    }

    /// One line under the chosen kind in the print dialog: what it is for.
    pub fn description(self) -> &'static str {
        match self {
            PrintKind::ChangeList => "Unterschiede des LV gegenüber dem anderen Stand, mit Wortunterschieden.",
            PrintKind::ChangeOrderCheck => {
                "Ein Nachtrag: angeboten und geprüft je Position, Vermerke, Empfehlung, Unterschriften."
            }
            PrintKind::ChangeOrderOverview => "Alle Nachträge des LV mit Summen je Status und Prognose.",
            PrintKind::Letters => "Die Briefe aus dem Schriftverkehr, je Empfänger ein Schreiben.",
            PrintKind::RequestForQuotation => {
                "Für die Bieter: EP und GP leer zum Ausfüllen, Deckblatt mit Bieterfeld."
            }
            PrintKind::PricedLv => "Mit deinen Planerpreisen und Summen je Titel – der Kostenvoranschlag nach DIN 276.",
            PrintKind::ShortTextLv => "Nur Kurztexte, mit oder ohne Preise – kompakte Übersicht.",
            PrintKind::Summary => "Nur die Titel mit ihren Summen.",
            PrintKind::Contract => "Das LV mit den Preisen des beauftragten Bieters.",
            PrintKind::PriceComparison => "Alle Angebote nebeneinander, wie im Reiter Preisspiegel eingestellt.",
            PrintKind::BidPrices => "Das LV mit den Preisen eines gewählten Angebots.",
            PrintKind::Submission => "Submissionsprotokoll: Bieter, Eingang, Summen, Unterschriften.",
            PrintKind::CostGroups => "Planer- und Bietersummen nach DIN 276.",
            PrintKind::BillingCheck => "Rechnung gegen geprüfte Mengen je Position.",
            PrintKind::Release => "Rechenweg bis zum Freigabebetrag, mit Unterschriften.",
            PrintKind::CostEstimate => "Kostenermittlung der KG 480: Bauteile, Kostengruppen mit Prüfsumme, Erläuterungen.",
            PrintKind::CostTracking => {
                "Kostenvorgabe bis Kostenfeststellung je Kostengruppe, wie im Reiter eingestellt: Abweichungen, Diagramm, Erläuterungen."
            }
            PrintKind::CostLv => {
                "Das Kosten-LV: Angaben nach DIN 276, Zusammenstellung nach Kostengruppen, \
                 Kurztext-Liste mit Menge, Einheit, Kostenkennwert und Betrag."
            }
        }
    }

    /// Short key for settings, file names and the MCP server.
    pub fn key(self) -> &'static str {
        match self {
            PrintKind::ChangeList => "aenderungsliste",
            PrintKind::ChangeOrderCheck => "nachtragspruefung",
            PrintKind::ChangeOrderOverview => "nachtragsuebersicht",
            PrintKind::Letters => "schreiben",
            PrintKind::RequestForQuotation => "angebotsaufforderung",
            PrintKind::PricedLv => "bepreistes_lv",
            PrintKind::ShortTextLv => "kurztext_lv",
            PrintKind::Summary => "zusammenstellung",
            PrintKind::Contract => "auftrags_lv",
            PrintKind::PriceComparison => "preisspiegel",
            PrintKind::BidPrices => "bieterpreise",
            PrintKind::Submission => "niederschrift",
            PrintKind::CostGroups => "kostengruppen",
            PrintKind::BillingCheck => "aufmasspruefung",
            PrintKind::Release => "rechnungsfreigabe",
            PrintKind::CostEstimate => "kostenermittlung",
            PrintKind::CostTracking => "kostenverfolgung",
            PrintKind::CostLv => "kosten_lv",
        }
    }

    /// The kind of a key; also the names of the print dialog as keys
    /// („lv_ohne_preise“, „lv_mit_preisen“).
    pub fn from_key(key: &str) -> Option<Self> {
        let key = key.trim().to_lowercase();
        match key.as_str() {
            "lv_ohne_preise" => return Some(PrintKind::RequestForQuotation),
            "lv_mit_preisen" | "kostenvoranschlag" | "kostenanschlag" => {
                return Some(PrintKind::PricedLv)
            }
            _ => {}
        }
        PrintKind::ALL.into_iter().find(|kind| kind.key() == key)
    }

    /// Part of the suggested file name.
    pub fn file_label(self) -> &'static str {
        match self {
            PrintKind::ChangeList => "Aenderungsliste",
            PrintKind::ChangeOrderCheck => "Pruefung_Nachtrag",
            PrintKind::ChangeOrderOverview => "Nachtragsuebersicht",
            PrintKind::Letters => "Schreiben",
            PrintKind::RequestForQuotation => "LV_ohne_Preise",
            PrintKind::PricedLv => "LV_mit_Preisen",
            PrintKind::ShortTextLv => "Kurztext-LV",
            PrintKind::Summary => "Zusammenstellung",
            PrintKind::Contract => "Auftrags-LV",
            PrintKind::PriceComparison => "Preisspiegel",
            PrintKind::BidPrices => "Bieterpreise",
            PrintKind::Submission => "Niederschrift_Submission",
            PrintKind::CostGroups => "Kostengruppen",
            PrintKind::BillingCheck => "Aufmasspruefung",
            PrintKind::Release => "Rechnungsfreigabe",
            PrintKind::CostEstimate => "Kostenermittlung",
            PrintKind::CostTracking => "Kostenverfolgung",
            PrintKind::CostLv => "Kostenermittlung_LV",
        }
    }

    /// The kind prints one Kostenschätzung or Kostenberechnung.
    pub fn needs_cost_estimate(self) -> bool {
        self == PrintKind::CostEstimate
    }

    /// The kind prints the prices of one bid.
    pub fn needs_bid(self) -> bool {
        matches!(self, PrintKind::Contract | PrintKind::BidPrices)
    }

    /// The kind prints one invoice of the Abrechnung.
    pub fn needs_invoice(self) -> bool {
        matches!(self, PrintKind::BillingCheck | PrintKind::Release)
    }

    /// Kinds that can print the Mengenermittlung under the positions.
    pub fn offers_takeoff(self) -> bool {
        matches!(
            self,
            PrintKind::PricedLv | PrintKind::Contract | PrintKind::ShortTextLv
        )
    }

    /// The LV options (texts, range, layout) apply; the Preisspiegel has its own.
    pub fn is_lv(self) -> bool {
        !matches!(
            self,
            PrintKind::ChangeList
                | PrintKind::ChangeOrderCheck
                | PrintKind::ChangeOrderOverview
                | PrintKind::Letters
                | PrintKind::PriceComparison
                | PrintKind::Submission
                | PrintKind::CostGroups
                | PrintKind::BillingCheck
                | PrintKind::Release
                | PrintKind::CostEstimate
                | PrintKind::CostTracking
                | PrintKind::CostLv
        )
    }
}

/// Form of the Deckblatt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CoverForm {
    /// Clear and left aligned, framed bidder field (Entwurf A).
    #[default]
    Standard,
    /// Like a letter: Vergabestelle top left, key facts in a box (Entwurf C).
    Letterhead,
}

impl CoverForm {
    pub const ALL: [CoverForm; 2] = [CoverForm::Standard, CoverForm::Letterhead];

    pub fn key(self) -> &'static str {
        match self {
            CoverForm::Standard => "standard",
            CoverForm::Letterhead => "briefkopf",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CoverForm::Standard => "Standard",
            CoverForm::Letterhead => "Briefkopf",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|form| form.key() == key.trim().to_lowercase())
    }
}

/// Which blocks the Deckblatt shows (Extras ▸ Einstellungen ▸ Drucken).
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

/// Druckstil of the printouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrintStyle {
    /// Plain like classic AVA printouts: black text, tinted title rows.
    #[default]
    Classic,
    /// proAVA's own look: headings in the accent colour.
    ProAva,
}

impl PrintStyle {
    pub const ALL: [PrintStyle; 2] = [PrintStyle::Classic, PrintStyle::ProAva];

    pub fn key(self) -> &'static str {
        match self {
            PrintStyle::Classic => "klassisch",
            PrintStyle::ProAva => "proava",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PrintStyle::Classic => "Klassisch",
            PrintStyle::ProAva => "proAVA",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|style| style.key() == key.trim().to_lowercase())
    }
}

/// Line width of the Blattrahmen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BorderWidth {
    #[default]
    Thin,
    Medium,
}

impl BorderWidth {
    pub const ALL: [BorderWidth; 2] = [BorderWidth::Thin, BorderWidth::Medium];

    pub fn key(self) -> &'static str {
        match self {
            BorderWidth::Thin => "duenn",
            BorderWidth::Medium => "mittel",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BorderWidth::Thin => "dünn",
            BorderWidth::Medium => "mittel",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        let key = key.trim().to_lowercase();
        Self::ALL
            .into_iter()
            .find(|width| width.key() == key || width.label() == key)
    }
}

/// Zeilenabstand of formatted texts (Langtext, Hinweise …) in the editor
/// and in print, as a factor of the font's own line height (Arial/Liberation
/// Sans: 1.15 × the font size, as AVA printouts set it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineSpacing {
    #[default]
    Single,
    OneFifteen,
    OneHalf,
}

impl LineSpacing {
    pub const ALL: [LineSpacing; 3] = [
        LineSpacing::Single,
        LineSpacing::OneFifteen,
        LineSpacing::OneHalf,
    ];

    /// Factor on the font's own line height.
    pub fn factor(self) -> f32 {
        match self {
            LineSpacing::Single => 1.0,
            LineSpacing::OneFifteen => 1.15,
            LineSpacing::OneHalf => 1.5,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            LineSpacing::Single => "einfach",
            LineSpacing::OneFifteen => "1,15",
            LineSpacing::OneHalf => "1,5",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        let key = key.trim().to_lowercase().replace('.', ",");
        Self::ALL.into_iter().find(|spacing| spacing.key() == key)
    }
}

/// Abstand nach Absatz of formatted texts (editor and print). AVA printouts
/// set none: empty paragraphs of the text itself make the blank lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParagraphGap {
    #[default]
    None,
    Points3,
    Points6,
    HalfLine,
}

impl ParagraphGap {
    pub const ALL: [ParagraphGap; 4] = [
        ParagraphGap::None,
        ParagraphGap::Points3,
        ParagraphGap::Points6,
        ParagraphGap::HalfLine,
    ];

    pub fn key(self) -> &'static str {
        match self {
            ParagraphGap::None => "0",
            ParagraphGap::Points3 => "3",
            ParagraphGap::Points6 => "6",
            ParagraphGap::HalfLine => "halbe-zeile",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ParagraphGap::None => "kein",
            ParagraphGap::Points3 => "3 pt",
            ParagraphGap::Points6 => "6 pt",
            ParagraphGap::HalfLine => "halbe Zeile",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        let key = key.trim().to_lowercase();
        Self::ALL
            .into_iter()
            .find(|gap| gap.key() == key || gap.label() == key)
    }
}

/// Zeilenabstand and Abstand nach Absatz of formatted texts – one setting
/// for the editor and the printouts, so that both look alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextSpacing {
    pub line: LineSpacing,
    pub gap: ParagraphGap,
}

impl TextSpacing {
    /// Line height of the fonts proAVA uses (Liberation Sans/Arial: ascent,
    /// descent and line gap) as a factor of the font size.
    pub const FONT_LINE_HEIGHT: f32 = 1.15;

    /// Distance of two lines of a paragraph in pt for text of `size_pt`.
    pub fn line_pitch_pt(&self, size_pt: f32) -> f32 {
        self.line.factor() * Self::FONT_LINE_HEIGHT * size_pt
    }

    /// Abstand nach Absatz in pt for text of `size_pt`.
    pub fn gap_pt(&self, size_pt: f32) -> f32 {
        match self.gap {
            ParagraphGap::None => 0.0,
            ParagraphGap::Points3 => 3.0,
            ParagraphGap::Points6 => 6.0,
            ParagraphGap::HalfLine => 0.5 * self.line_pitch_pt(size_pt),
        }
    }
}

/// Look of all printouts (Extras ▸ Einstellungen ▸ Drucken).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintLook {
    /// Default style; the print dialog may choose another per kind.
    pub style: PrintStyle,
    /// Accent colour „#RRGGBB“ for lines and fills.
    pub accent: String,
    /// Blattrahmen by default (the print dialog may switch it per kind).
    pub border: bool,
    pub border_width: BorderWidth,
    /// Tabellen voll umrandet (Gitter) by default (the print dialog may
    /// switch it per kind; the Preisspiegel has it from the start).
    pub table_grid: bool,
    /// Zeilenabstand and Abstand nach Absatz of the texts (Extras ▸
    /// Einstellungen ▸ Darstellung) – the text editor uses the same.
    pub text: TextSpacing,
}

impl PrintLook {
    /// Petrolblau (RGB 0/79/124).
    pub const DEFAULT_ACCENT: &'static str = "#004F7C";

    /// „#RRGGBB“ (also without „#“, any case) → „#RRGGBB“; else None.
    pub fn parse_accent(text: &str) -> Option<String> {
        let text = text.trim();
        let digits = text.strip_prefix('#').unwrap_or(text);
        (digits.len() == 6 && digits.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| format!("#{}", digits.to_ascii_uppercase()))
    }
}

impl Default for PrintLook {
    fn default() -> Self {
        Self {
            style: PrintStyle::Classic,
            accent: Self::DEFAULT_ACCENT.to_string(),
            border: false,
            border_width: BorderWidth::Thin,
            table_grid: false,
            text: TextSpacing::default(),
        }
    }
}

/// Options of the print dialog, remembered per kind.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintSettings {
    pub long_texts: bool,
    pub images: bool,
    /// Hinweise and Vorbemerkungen.
    pub notes: bool,
    pub performance: bool,
    /// Zusammenstellung at the end.
    pub summary: bool,
    pub title_page_break: bool,
    pub landscape: bool,
    /// 9, 10 or 11.
    pub font_size_pt: u8,
    /// Printed in the page header / footer of every page.
    pub header_text: String,
    pub footer_text: String,
    /// Deckblatt with the project master data before the LV.
    pub cover: bool,
    /// Its form (Standard or Briefkopf).
    pub cover_form: CoverForm,
    /// Inhaltsverzeichnis with page numbers (also in the Zusammenstellung).
    pub contents: bool,
    /// „Übertrag“ of the running title sum at page end and start.
    pub carry: bool,
    /// Kurztext-LV with prices (false = blank EP/GP fields to fill in).
    pub prices: bool,
    /// Druckstil of this kind; None = the default from the settings.
    pub style: Option<PrintStyle>,
    /// Mengenermittlung under the positions (bepreistes LV, Auftrags-LV,
    /// Kurztext-LV; see [`PrintKind::offers_takeoff`]).
    pub takeoff: bool,
    /// Blattrahmen for this kind; None = the default from the settings.
    pub border: Option<bool>,
    /// Tabellen voll umrandet (Gitter) for this kind; None = the default
    /// from the settings.
    pub grid: Option<bool>,
    /// Kostengruppe at every position („KG 481“, own or from the title).
    pub cost_groups: bool,
    /// Leistungsbereich at every position („LB 070“).
    pub work_categories: bool,
    /// Kostenermittlung: bar chart of the cost groups / Bauteile.
    pub chart: bool,
    /// Kostenermittlung: plant list under each Bauteil.
    pub plants: bool,
    /// Kostenermittlung: Kostenkontrolle against the LVs and the contract.
    pub cost_control: bool,
    /// PDF-Lesezeichen for every position (titles and sections always).
    pub bookmark_positions: bool,
}

impl Default for PrintSettings {
    fn default() -> Self {
        Self {
            long_texts: true,
            images: true,
            notes: true,
            performance: true,
            summary: true,
            title_page_break: false,
            landscape: false,
            font_size_pt: 9,
            header_text: String::new(),
            footer_text: String::new(),
            cover: false,
            cover_form: CoverForm::Standard,
            contents: true,
            carry: true,
            prices: true,
            style: None,
            takeoff: false,
            border: None,
            grid: None,
            cost_groups: true,
            work_categories: false,
            chart: true,
            plants: true,
            cost_control: true,
            bookmark_positions: true,
        }
    }
}

impl PrintSettings {
    /// What a kind prints without changes by the user.
    pub fn defaults_for(kind: PrintKind) -> Self {
        let mut settings = Self {
            cover: kind == PrintKind::RequestForQuotation,
            // Kostenermittlung: A4 quer wie die Tabellen der Planer.
            landscape: matches!(kind, PrintKind::CostEstimate | PrintKind::CostTracking),
            ..Self::default()
        };
        if kind == PrintKind::PriceComparison {
            // Gitter-Tabelle from the start (Anwender 20.11.).
            settings.grid = Some(true);
        }
        if kind == PrintKind::CostLv {
            // Kurztext-Liste: the Langtext (source of the Kennwert) on demand.
            settings.long_texts = false;
        }
        if kind == PrintKind::ShortTextLv {
            settings.long_texts = false;
            settings.images = false;
            settings.notes = false;
            // Ausführungsbeschreibungen stay with mark and heading.
        }
        settings
    }

    /// One line for the settings file, e.g. `lang=1;bild=0;…;kopf=Vorabzug`.
    pub fn to_line(&self) -> String {
        let flag = |value: bool| if value { "1" } else { "0" };
        let clean = |text: &str| text.replace([';', '\n', '\r'], " ");
        format!(
            "lang={};bild={};hinw={};ab={};zus={};seite={};quer={};deck={};deckform={};inh={};uebertrag={};preise={};stil={};me={};rahmen={};gitter={};kg={};lb={};diagramm={};anlagen={};kontrolle={};lzpos={};pt={};kopf={};fuss={}",
            flag(self.long_texts),
            flag(self.images),
            flag(self.notes),
            flag(self.performance),
            flag(self.summary),
            flag(self.title_page_break),
            flag(self.landscape),
            flag(self.cover),
            self.cover_form.key(),
            flag(self.contents),
            flag(self.carry),
            flag(self.prices),
            self.style.map(PrintStyle::key).unwrap_or_default(),
            flag(self.takeoff),
            self.border.map(flag).unwrap_or_default(),
            self.grid.map(flag).unwrap_or_default(),
            flag(self.cost_groups),
            flag(self.work_categories),
            flag(self.chart),
            flag(self.plants),
            flag(self.cost_control),
            flag(self.bookmark_positions),
            self.font_size_pt,
            clean(&self.header_text),
            clean(&self.footer_text),
        )
    }

    /// Reads [`Self::to_line`]; unknown or missing keys keep the defaults of `kind`.
    pub fn from_line(kind: PrintKind, line: &str) -> Self {
        let mut settings = Self::defaults_for(kind);
        for part in line.split(';') {
            let Some((key, value)) = part.split_once('=') else {
                continue;
            };
            let on = value.trim() == "1";
            match key.trim() {
                "lang" => settings.long_texts = on,
                "bild" => settings.images = on,
                "hinw" => settings.notes = on,
                "ab" => settings.performance = on,
                "zus" => settings.summary = on,
                "seite" => settings.title_page_break = on,
                "quer" => settings.landscape = on,
                "deck" => settings.cover = on,
                "inh" => settings.contents = on,
                "uebertrag" => settings.carry = on,
                "preise" => settings.prices = on,
                "stil" => settings.style = PrintStyle::from_key(value),
                "rahmen" => {
                    settings.border = match value.trim() {
                        "1" => Some(true),
                        "0" => Some(false),
                        _ => None,
                    }
                }
                "gitter" => {
                    settings.grid = match value.trim() {
                        "1" => Some(true),
                        "0" => Some(false),
                        _ => None,
                    }
                }
                "me" => settings.takeoff = on,
                "kg" => settings.cost_groups = on,
                "lb" => settings.work_categories = on,
                "diagramm" => settings.chart = on,
                "anlagen" => settings.plants = on,
                "kontrolle" => settings.cost_control = on,
                "lzpos" => settings.bookmark_positions = on,
                "deckform" => {
                    if let Some(form) = CoverForm::from_key(value) {
                        settings.cover_form = form;
                    }
                }
                "pt" => {
                    if let Ok(size) = value.trim().parse::<u8>() {
                        settings.font_size_pt = size.clamp(8, 12);
                    }
                }
                "kopf" => settings.header_text = value.to_string(),
                "fuss" => settings.footer_text = value.to_string(),
                _ => {}
            }
        }
        settings
    }
}

/// One printout as the dialog (or the MCP server) asks for it.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintJob {
    pub kind: PrintKind,
    pub settings: PrintSettings,
    /// For [`PrintKind::needs_bid`]; Auftrags-LV without it = the awarded bid.
    pub bid: Option<BidId>,
    /// Titles to print (with everything below them); empty = whole LV.
    pub titles: Vec<RowId>,
    /// Settings of the Preisspiegel (order, anonymous …).
    pub comparison: ComparisonPrint,
    /// Logo for the title block (from Einstellungen ▸ Drucken).
    pub logo: Option<PdfLogo>,
    /// Kopf-/Fußzeile from the settings; a project's own one wins.
    pub frame: proava_domain::PageFrame,
    /// Name of the PDF for the placeholder `{datei}`; empty = the project name.
    pub file_name: String,
    /// Blocks of the Deckblatt from the settings.
    pub cover_blocks: CoverBlocks,
    /// For [`PrintKind::needs_invoice`]; None = the last invoice of the LV.
    pub invoice: Option<InvoiceId>,
    /// Default style and accent colour from the settings.
    pub look: PrintLook,
    /// Bearbeiter from the settings for `{ersteller}`/`{bearbeiter}` and the
    /// title block; empty = the planner of the project.
    pub author: String,
    /// For [`PrintKind::needs_cost_estimate`]; None = the newest one.
    pub cost_estimate: Option<CostEstimateId>,
    /// Kostenverfolgung: the view of the tab (Ebene, Ansicht, netto/brutto,
    /// Abweichung, Vergleich).
    pub tracking: proava_domain::TrackOptions,
}

impl PrintJob {
    pub fn new(kind: PrintKind) -> Self {
        Self {
            kind,
            settings: PrintSettings::defaults_for(kind),
            bid: None,
            titles: Vec::new(),
            comparison: ComparisonPrint::default(),
            logo: None,
            frame: proava_domain::PageFrame::default(),
            file_name: String::new(),
            cover_blocks: CoverBlocks::default(),
            invoice: None,
            look: PrintLook::default(),
            author: String::new(),
            cost_estimate: None,
            tracking: proava_domain::TrackOptions::default(),
        }
    }
}

/// Result of a printout.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PrintOutcome {
    pub pages: usize,
    pub warnings: Vec<String>,
}

/// Beginning of the message for a printout refused because it would not fit
/// into the free memory (the exchange layer writes it, the program shows it in
/// a window, the MCP server adds its own way to print a part).
pub const MEMORY_REFUSAL: &str = "Der Druck ist sehr groß";

/// The message of a printout refused for lack of memory.
pub fn is_memory_refusal(message: &str) -> bool {
    message.contains(MEMORY_REFUSAL)
}

/// Documents printed from a window of their own: the print dialog shows only
/// that kind with its options (Stil, Blattrahmen, Seite, Briefe: Ablage).
impl PrintKind {
    pub const FROM_WINDOWS: [PrintKind; 4] = [
        PrintKind::ChangeList,
        PrintKind::ChangeOrderCheck,
        PrintKind::ChangeOrderOverview,
        PrintKind::Letters,
    ];

    /// The kind belongs to a window (LV-Vergleich, Nachträge, Schriftverkehr).
    pub fn from_window(self) -> bool {
        Self::FROM_WINDOWS.contains(&self)
    }

    /// Whether the print offers a Blattrahmen (letters never have one).
    pub fn offers_border(self) -> bool {
        self != PrintKind::Letters
    }

    /// Whether the print has tables that can be fully ruled (Gitter): all
    /// but letters.
    pub fn offers_grid(self) -> bool {
        self != PrintKind::Letters
    }

    /// Whether the print can be laid out landscape in the dialog.
    pub fn offers_landscape(self) -> bool {
        self == PrintKind::ChangeList
    }
}

#[cfg(test)]
mod tests {
    use super::{PrintKind, PrintSettings};

    /// DIN 276:2018, 4.3.5: the LV with the planner's prices is the
    /// Kostenvoranschlag; the old MCP values stay valid.
    #[test]
    fn priced_lv_is_the_kostenvoranschlag() {
        assert_eq!(
            PrintKind::PricedLv.label(),
            "LV mit Preisen (Kostenvoranschlag)"
        );
        for key in [
            "bepreistes_lv",
            "lv_mit_preisen",
            "kostenvoranschlag",
            "kostenanschlag",
        ] {
            assert_eq!(PrintKind::from_key(key), Some(PrintKind::PricedLv), "{key}");
        }
    }

    #[test]
    fn settings_line_round_trips_and_keys_are_unique() {
        let mut settings = PrintSettings::defaults_for(PrintKind::ShortTextLv);
        assert!(!settings.long_texts);
        settings.landscape = true;
        settings.font_size_pt = 11;
        settings.header_text = "Vorabzug; nicht\nverbindlich".into();
        let back = PrintSettings::from_line(PrintKind::ShortTextLv, &settings.to_line());
        assert_eq!(back.header_text, "Vorabzug  nicht verbindlich");
        assert!(back.landscape && !back.long_texts);
        let mut blank = PrintSettings::defaults_for(PrintKind::ShortTextLv);
        assert!(blank.prices && blank.contents && blank.carry);
        blank.prices = false;
        blank.cover = true;
        blank.cover_form = super::CoverForm::Letterhead;
        let blank_back = PrintSettings::from_line(PrintKind::ShortTextLv, &blank.to_line());
        assert!(!blank_back.prices && blank_back.cover);
        assert_eq!(blank_back.style, None, "ohne Wahl gilt die Vorgabe");
        blank.style = Some(super::PrintStyle::ProAva);
        assert_eq!(
            PrintSettings::from_line(PrintKind::ShortTextLv, &blank.to_line()).style,
            Some(super::PrintStyle::ProAva)
        );
        assert_eq!(
            super::PrintLook::parse_accent(" 004f7c"),
            Some("#004F7C".to_string())
        );
        assert_eq!(super::PrintLook::parse_accent("#04F7C"), None);
        assert_eq!(blank_back.border, None, "ohne Wahl gilt die Vorgabe");
        blank.border = Some(false);
        assert_eq!(
            PrintSettings::from_line(PrintKind::ShortTextLv, &blank.to_line()).border,
            Some(false)
        );
        blank.border = Some(true);
        assert_eq!(
            PrintSettings::from_line(PrintKind::ShortTextLv, &blank.to_line()).border,
            Some(true)
        );
        assert_eq!(
            super::BorderWidth::from_key("Mittel"),
            Some(super::BorderWidth::Medium)
        );
        // Gitter: the Preisspiegel from the start, the others follow the
        // settings; the choice per kind is remembered.
        assert_eq!(
            PrintSettings::defaults_for(PrintKind::PriceComparison).grid,
            Some(true)
        );
        assert_eq!(blank_back.grid, None);
        blank.grid = Some(true);
        assert_eq!(
            PrintSettings::from_line(PrintKind::ShortTextLv, &blank.to_line()).grid,
            Some(true)
        );
        let mut comparison = PrintSettings::defaults_for(PrintKind::PriceComparison);
        comparison.grid = Some(false);
        assert_eq!(
            PrintSettings::from_line(PrintKind::PriceComparison, &comparison.to_line()).grid,
            Some(false)
        );
        // A line saved before the option keeps the default of the kind.
        assert_eq!(
            PrintSettings::from_line(PrintKind::PriceComparison, "lang=1").grid,
            Some(true)
        );
        assert!(PrintKind::PricedLv.offers_grid() && !PrintKind::Letters.offers_grid());
        assert!(
            blank_back.cost_groups && !blank_back.work_categories,
            "Vorgabe: KG an, LB aus"
        );
        blank.cost_groups = false;
        blank.work_categories = true;
        let codes = PrintSettings::from_line(PrintKind::ShortTextLv, &blank.to_line());
        assert!(!codes.cost_groups && codes.work_categories);
        assert_eq!(blank_back.cover_form, super::CoverForm::Letterhead);
        assert_eq!(back.font_size_pt, 11);
        assert_eq!(
            PrintSettings::from_line(PrintKind::PricedLv, ""),
            PrintSettings::default()
        );
        for kind in PrintKind::ALL {
            assert_eq!(PrintKind::from_key(kind.key()), Some(kind));
        }
        let mut costs = PrintSettings::defaults_for(PrintKind::CostEstimate);
        assert!(
            costs.landscape && costs.chart && costs.plants && costs.cost_control,
            "Kosten: quer, alles an"
        );
        costs.chart = false;
        costs.cost_control = false;
        let back = PrintSettings::from_line(PrintKind::CostEstimate, &costs.to_line());
        assert!(!back.chart && back.plants && !back.cost_control && back.landscape);
    }

    #[test]
    fn position_bookmarks_are_on_and_round_trip() {
        let mut settings = PrintSettings::defaults_for(PrintKind::RequestForQuotation);
        assert!(
            settings.bookmark_positions,
            "Vorgabe: Lesezeichen für Positionen"
        );
        settings.bookmark_positions = false;
        let line = settings.to_line();
        assert!(line.contains("lzpos=0"), "{line}");
        assert!(
            !PrintSettings::from_line(PrintKind::RequestForQuotation, &line).bookmark_positions
        );
        // Older lines without the key keep the default.
        assert!(PrintSettings::from_line(PrintKind::PricedLv, "lang=1").bookmark_positions);
    }

    #[test]
    fn takeoff_option_round_trips_for_the_lv_kinds() {
        let mut settings = PrintSettings::defaults_for(PrintKind::PricedLv);
        assert!(!settings.takeoff, "Vorgabe: ohne Mengenermittlung");
        settings.takeoff = true;
        assert!(PrintSettings::from_line(PrintKind::PricedLv, &settings.to_line()).takeoff);
        let offered: Vec<PrintKind> = PrintKind::ALL
            .into_iter()
            .filter(|kind| kind.offers_takeoff())
            .collect();
        assert_eq!(
            offered,
            [
                PrintKind::PricedLv,
                PrintKind::ShortTextLv,
                PrintKind::Contract
            ]
        );
    }
}
