//! PDF printouts for proAVA (Angebotsaufforderung, bepreistes LV, Preisspiegel),
//! typeset with an embedded Typst compiler and embedded Liberation Sans fonts.

mod billing;
mod change_order;
mod cost_estimate;
mod cost_lv;
mod cost_tracking;
mod estimate;
mod format;
mod frame;
mod keep;
mod letter;
mod model;
mod options;
mod pdf_clip;
mod submission;
mod view;
mod world;

use std::fmt;

pub use billing::{
    render_billing_check_pdf_counted, render_release_pdf_counted, PrintAmountLine,
    PrintBillingCheck, PrintBillingPosition, PrintBillingRow, PrintEarlierRelease, PrintMark,
    PrintRelease,
};
pub use change_order::{
    render_change_order_check_pdf, render_change_order_overview_pdf, ChangeOrderCheckLine,
    ChangeOrderOverviewLine, PrintChangeOrderCheck, PrintChangeOrderOverview,
};
pub use cost_estimate::{
    render_cost_estimate_pdf, render_cost_estimate_pdf_counted, PrintControlLine, PrintCostControl,
    PrintCostEstimate, PrintCostOptions, PrintCostSection, PrintKindSum, PrintPointKind,
    PrintPointRow, PrintShare,
};
pub use cost_lv::{
    render_cost_lv_pdf, render_cost_lv_pdf_counted, PrintCostLv, PrintCostLvGroup, PrintCostLvItem,
    PrintCostLvPoints,
};
pub use cost_tracking::{
    render_cost_tracking_pdf, render_cost_tracking_pdf_counted, PrintCostTracking, PrintKeyFigure,
    PrintTrackCell, PrintTrackChart, PrintTrackColumn, PrintTrackMark, PrintTrackNote,
    PrintTrackOptions, PrintTrackRow, PrintTrackRowKind, PrintTrackWithout,
};
pub use estimate::{estimate_lv_print, PrintEstimate};
pub use frame::{FrameLogo, PrintFrame, DEFAULT_FOOTER, DEFAULT_HEADER, PLACEHOLDER_NAMES};
pub use letter::{render_letters_pdf, PrintLetter, PrintLetters};
pub use model::*;
pub use options::PrintOptions;
pub use pdf_clip::{
    pdf_area, pdf_content, pdf_pages, pdf_single_page, pixel_size, render_pdf_area,
    sample_vector_pdf, ClipPixels, PdfArea, MAX_PIXELS, MAX_SIDE,
};
pub use submission::{
    render_submission_pdf, render_submission_pdf_counted, PrintSubmission, SubmissionBidder,
};

const LV_TEMPLATE: &str = include_str!("../templates/lv.typ");
const PRICE_COMPARISON_TEMPLATE: &str = include_str!("../templates/price_comparison.typ");
const COST_GROUPS_TEMPLATE: &str = include_str!("../templates/cost_groups.typ");
const CHANGES_TEMPLATE: &str = include_str!("../templates/changes.typ");

#[derive(Debug)]
pub enum PrintError {
    /// The Typst template could not be laid out (includes broken images).
    Layout(String),
    Pdf(String),
    Data(String),
}

impl fmt::Display for PrintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Layout(msg) => write!(f, "Satzfehler: {msg}"),
            Self::Pdf(msg) => write!(f, "PDF-Fehler: {msg}"),
            Self::Data(msg) => write!(f, "Datenfehler: {msg}"),
        }
    }
}

impl std::error::Error for PrintError {}

/// Angebotsaufforderung (`LvPrintMode::Blank`) or bepreistes LV (`LvPrintMode::Priced`).
pub fn render_lv_pdf(lv: &PrintLv, mode: LvPrintMode) -> Result<Vec<u8>, PrintError> {
    render_lv_pdf_with(lv, mode, &PrintOptions::default())
}

/// LV printout with layout options (paper, font size, page breaks, sections).
pub fn render_lv_pdf_with(
    lv: &PrintLv,
    mode: LvPrintMode,
    options: &PrintOptions,
) -> Result<Vec<u8>, PrintError> {
    let world = lv_world(lv, mode, options)?;
    world.pdf(&world.layout()?)
}

/// LV printout with options, together with its page count (one layout run).
pub fn render_lv_pdf_counted(
    lv: &PrintLv,
    mode: LvPrintMode,
    options: &PrintOptions,
) -> Result<(Vec<u8>, usize), PrintError> {
    let world = lv_world(lv, mode, options)?;
    let document = world.layout()?;
    Ok((world.pdf(&document)?, document.pages().len()))
}

/// Preisspiegel together with its page count.
pub fn render_price_comparison_pdf_counted(
    pc: &PrintPriceComparison,
) -> Result<(Vec<u8>, usize), PrintError> {
    let world = price_comparison_world(pc)?;
    let document = world.layout()?;
    Ok((world.pdf(&document)?, document.pages().len()))
}

/// Number of pages the LV printout would have, without writing the PDF.
pub fn lv_page_count(
    lv: &PrintLv,
    mode: LvPrintMode,
    options: &PrintOptions,
) -> Result<usize, PrintError> {
    let pages = lv_world(lv, mode, options)?.layout()?.pages().len();
    world::release_cache();
    Ok(pages)
}

/// Number of pages of the Preisspiegel.
pub fn price_comparison_page_count(pc: &PrintPriceComparison) -> Result<usize, PrintError> {
    let pages = price_comparison_world(pc)?.layout()?.pages().len();
    world::release_cache();
    Ok(pages)
}

/// Preisspiegel, landscape A4 or A3.
pub fn render_price_comparison_pdf(pc: &PrintPriceComparison) -> Result<Vec<u8>, PrintError> {
    let world = price_comparison_world(pc)?;
    world.pdf(&world.layout()?)
}

/// Kosten nach Kostengruppen (portrait, landscape with more than two bidders).
pub fn render_cost_groups_pdf(report: &PrintCostGroups) -> Result<Vec<u8>, PrintError> {
    render_cost_groups_pdf_counted(report).map(|(bytes, _)| bytes)
}

/// [`render_cost_groups_pdf`] with the number of pages.
pub fn render_cost_groups_pdf_counted(
    report: &PrintCostGroups,
) -> Result<(Vec<u8>, usize), PrintError> {
    let mut files = view::Files::default();
    let data = view::cost_groups(report, &mut files);
    let world = build_world(COST_GROUPS_TEMPLATE, files, &data)?;
    let document = world.layout()?;
    Ok((world.pdf(&document)?, document.pages().len()))
}

/// Änderungsliste (LV-Vergleich), A4 hoch.
pub fn render_change_list_pdf(list: &PrintChangeList) -> Result<Vec<u8>, PrintError> {
    let mut files = view::Files::default();
    let data = view::change_list(list, &mut files);
    let world = build_world(CHANGES_TEMPLATE, files, &data)?;
    world.pdf(&world.layout()?)
}

fn lv_world(
    lv: &PrintLv,
    mode: LvPrintMode,
    options: &PrintOptions,
) -> Result<world::PrintWorld, PrintError> {
    let mut files = view::Files::default();
    let data = view::lv(lv, mode, options, &mut files);
    build_world(LV_TEMPLATE, files, &data)
}

fn price_comparison_world(pc: &PrintPriceComparison) -> Result<world::PrintWorld, PrintError> {
    let mut files = view::Files::default();
    let data = view::price_comparison(pc, &mut files);
    build_world(PRICE_COMPARISON_TEMPLATE, files, &data)
}

fn build_world(
    template: &str,
    files: view::Files,
    data: &serde_json::Value,
) -> Result<world::PrintWorld, PrintError> {
    let mut world = world::PrintWorld::new(template);
    let json = serde_json::to_vec(data).map_err(|e| PrintError::Data(e.to_string()))?;
    world.add_file("/data.json", json);
    for (path, bytes) in files.into_inner() {
        world.add_file(&path, bytes);
    }
    stage_done(Stage::Data);
    Ok(world)
}

/// Stages of a printout, for measurements ([`observe_stages`]).
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Print model turned into the data and files of the Typst world.
    Data,
    /// Typst laid the document out.
    Layout,
    /// The PDF is written.
    Pdf,
}

thread_local! {
    static OBSERVER: std::cell::RefCell<Option<Box<dyn FnMut(Stage)>>> =
        const { std::cell::RefCell::new(None) };
}

/// Measurement: frees Typst's cache now and waits for a release still
/// running on the helper thread (what a printout keeps afterwards).
#[doc(hidden)]
pub fn release_cache_now() {
    typst::comemo::evict(0);
}

/// Measurement (example `druck_speicher`): `observer` hears of every stage
/// finished on this thread; None switches it off.
#[doc(hidden)]
pub fn observe_stages(observer: Option<Box<dyn FnMut(Stage)>>) {
    OBSERVER.with(|slot| *slot.borrow_mut() = observer);
}

fn stage_done(stage: Stage) {
    OBSERVER.with(|slot| {
        if let Some(observer) = slot.borrow_mut().as_mut() {
            observer(stage);
        }
    });
}

#[cfg(test)]
mod tests;
