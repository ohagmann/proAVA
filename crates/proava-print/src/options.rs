//! Layout options of the LV printouts (what the print dialog sets). The
//! content (which texts, images, titles) is chosen when the print model is
//! built; these options only steer the layout of the sections.

/// Layout options for [`crate::render_lv_pdf_with`].
#[derive(Debug, Clone, PartialEq)]
pub struct PrintOptions {
    /// A4 landscape instead of portrait.
    pub landscape: bool,
    /// Base font size in pt (9, 10 or 11).
    pub font_size_pt: u8,
    /// Every top-level title starts on a new page.
    pub title_page_break: bool,
    /// Zusammenstellung (titles with sums) at the end.
    pub summary: bool,
    /// Only the Zusammenstellung with the totals, no positions (Summenblatt).
    pub summary_only: bool,
    /// Zusammenstellung with the share of each title sum in the net total
    /// (column „Anteil“, priced printouts only).
    pub summary_share: bool,
    /// Free text in the page header of every page, e.g. „Vorabzug“.
    pub header_text: String,
    /// Free text in the page footer of every page.
    pub footer_text: String,
    /// Inhaltsverzeichnis with page numbers before the LV (and page numbers
    /// in the Zusammenstellung).
    pub contents: bool,
    /// „Übertrag“ of the running title sum at the bottom and top of the pages.
    pub carry: bool,
    /// PDF-Lesezeichen for every position under its title. Titles, Hinweise
    /// with a heading, Ausführungsbeschreibungen, Deckblatt, Zusammenstellung
    /// … always have one.
    pub bookmark_positions: bool,
}

impl Default for PrintOptions {
    fn default() -> Self {
        Self {
            landscape: false,
            font_size_pt: 9,
            title_page_break: false,
            summary: true,
            summary_only: false,
            summary_share: true,
            header_text: String::new(),
            footer_text: String::new(),
            contents: false,
            carry: false,
            bookmark_positions: true,
        }
    }
}

impl PrintOptions {
    /// Font size limited to what the templates are made for.
    pub fn font_size(&self) -> u8 {
        self.font_size_pt.clamp(8, 12)
    }
}
