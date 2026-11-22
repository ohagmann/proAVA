//! Preisspiegel: all bids side by side, position by position, with frozen
//! header, frozen left columns and a frozen footer with the bid totals.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use eframe::egui::{
    self,
    text::{LayoutJob, TextWrapping},
    Align, Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, RichText, Sense, Stroke,
    StrokeKind, UiBuilder, Vec2,
};
use proava_application::PdfDocument;
use proava_domain::{
    format_decimal_de, BidId, BidSummary, ComparisonLine, ComparisonOptions, Decimal,
    DeviationReference, GroupLine, Money, PositionKind, PositionLine, PriceCell, PriceComparison,
    PriceMark, Project, RowId,
};

use crate::icons::Icon;
use crate::strings;
use crate::theme::Palette;
use crate::view::{ViewRequest, ViewResponse};
use crate::widgets;

const ROW_H: f32 = 26.0;
const HEAD_H: f32 = 52.0;
const FOOT_ROW_H: f32 = 21.0;
const FOOT_ROWS: usize = 7;
const PAD: f32 = 8.0;
const OZ_W: f32 = 104.0;
const MARK_W: f32 = 44.0;
const QTY_W: f32 = 70.0;
const UNIT_W: f32 = 48.0;
const EP_W: f32 = 98.0;
const GP_W: f32 = 114.0;
const STAT_W: f32 = 90.0;
/// Kept free at the right edge when the Kurztext takes the spare width.
const SCROLLBAR_ROOM: f32 = 16.0;
const SEARCH_HINT: &str = "OZ oder Kurztext suchen …";

/// Which price columns each bidder gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PriceColumns {
    #[default]
    Both,
    UnitOnly,
    TotalOnly,
}

impl PriceColumns {
    fn label(self) -> &'static str {
        match self {
            PriceColumns::Both => "EP + GP",
            PriceColumns::UnitOnly => "nur EP",
            PriceColumns::TotalOnly => "nur GP",
        }
    }

    fn block_width(self) -> f32 {
        match self {
            PriceColumns::Both => EP_W + GP_W,
            PriceColumns::UnitOnly => EP_W + 40.0,
            PriceColumns::TotalOnly => GP_W + 8.0,
        }
    }
}

/// Order of the bidder columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BidOrder {
    /// As read in.
    #[default]
    Input,
    Rank,
    Name,
}

impl BidOrder {
    fn label(self) -> &'static str {
        match self {
            BidOrder::Input => "wie eingelesen",
            BidOrder::Rank => "nach Rang",
            BidOrder::Name => "nach Name",
        }
    }
}

/// How anonymous bidders are numbered („Bieter 1 … n“).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnonymousNumbering {
    #[default]
    ByRank,
    ByInput,
}

/// Everything the user set up on screen that the PDF print should follow.
#[derive(Debug, Clone, PartialEq)]
pub struct ComparisonPrintOptions {
    pub comparison: ComparisonOptions,
    pub columns: PriceColumns,
    pub order: BidOrder,
    pub hidden_bids: Vec<BidId>,
    /// `Some` = names replaced by „Bieter 1 … n“.
    pub anonymous: Option<AnonymousNumbering>,
    pub totals_only: bool,
    pub undercut_percent: Decimal,
}

#[derive(Debug, Clone, Default)]
pub struct RowFilter {
    pub totals_only: bool,
    pub outliers_only: bool,
    pub search: String,
}

#[derive(Debug, Clone)]
struct EditCell {
    row: RowId,
    bid: BidId,
    text: String,
    started: bool,
    error: bool,
}

#[derive(Debug, Clone)]
enum Action {
    ShowInLv(RowId),
    Toggle(RowId),
    StartEdit(RowId, BidId, Option<Decimal>),
    CommitEdit,
    CancelEdit,
    CollapseAll,
    ExpandAll,
}

pub struct ComparisonView {
    options: ComparisonOptions,
    columns: PriceColumns,
    order: BidOrder,
    hidden: HashSet<BidId>,
    anonymous: bool,
    numbering: AnonymousNumbering,
    filter: RowFilter,
    collapsed: HashSet<RowId>,
    undercut_percent: u32,
    outlier_percent: u32,
    editing: Option<EditCell>,
    /// Comparison of the last frame; computed again only when the project
    /// (revision, active LV) or the options change.
    cache: Option<Arc<Derived>>,
}

/// What the table shows, computed from the project.
struct Derived {
    key: (u64, usize, ComparisonOptions),
    comparison: PriceComparison,
    corrections: HashMap<(BidId, RowId), (Money, Money)>,
    bidder_texts: HashMap<(BidId, RowId), Vec<String>>,
    /// The LV has planner prices: without, no column „Planerpreis“.
    planner_prices: bool,
}

impl Default for ComparisonView {
    fn default() -> Self {
        Self::new()
    }
}

/// Colours for good/bad marks that work on light and dark backgrounds.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Signal {
    pub good_bg: Color32,
    pub good_text: Color32,
    pub bad_bg: Color32,
    pub bad_text: Color32,
    /// Incomplete bids (missing prices).
    pub warn_bg: Color32,
    pub warn_text: Color32,
}

impl Signal {
    pub(crate) fn of(ui: &egui::Ui) -> Self {
        if ui.visuals().dark_mode {
            Self {
                good_bg: Color32::from_rgb(0x17, 0x33, 0x25),
                good_text: Color32::from_rgb(0x72, 0xD6, 0x9C),
                bad_bg: Color32::from_rgb(0x3B, 0x1F, 0x22),
                bad_text: Color32::from_rgb(0xFF, 0x8F, 0x86),
                warn_bg: Color32::from_rgb(0x3A, 0x2E, 0x14),
                warn_text: Color32::from_rgb(0xF0, 0xB4, 0x4A),
            }
        } else {
            Self {
                good_bg: Color32::from_rgb(0xE2, 0xF4, 0xE7),
                good_text: Color32::from_rgb(0x1B, 0x7A, 0x3B),
                bad_bg: Color32::from_rgb(0xFC, 0xE6, 0xE3),
                bad_text: Color32::from_rgb(0xB4, 0x26, 0x1E),
                warn_bg: Color32::from_rgb(0xFD, 0xF0, 0xD5),
                warn_text: Color32::from_rgb(0x8A, 0x52, 0x00),
            }
        }
    }
}

/// Horizontal layout of the table in content coordinates (0 = left edge).
struct Columns {
    text_w: f32,
    frozen_w: f32,
    /// Left edge of every block (planner, bids …, statistics).
    estimate_x: Option<f32>,
    bid_x: Vec<f32>,
    stats_x: f32,
    total_w: f32,
    block_w: f32,
    columns: PriceColumns,
}

impl Columns {
    fn new(inner_width: f32, bids: usize, with_estimate: bool, columns: PriceColumns) -> Self {
        let block_w = columns.block_width();
        let fixed = PAD + OZ_W + MARK_W + QTY_W + UNIT_W + PAD;
        let blocks = (bids + usize::from(with_estimate)) as f32 * block_w + 3.0 * STAT_W + PAD;
        // The Kurztext gets the width the price blocks leave free (they keep
        // theirs); room for the vertical scroll bar stays.
        let spare = (inner_width - SCROLLBAR_ROOM - fixed - blocks).max(0.0);
        let text_w = (inner_width * 0.24).clamp(170.0, 340.0).max(spare);
        let frozen_w = fixed + text_w;
        let mut x = frozen_w;
        let estimate_x = with_estimate.then(|| {
            let left = x;
            x += block_w;
            left
        });
        let bid_x = (0..bids)
            .map(|_| {
                let left = x;
                x += block_w;
                left
            })
            .collect();
        let stats_x = x;
        x += 3.0 * STAT_W + PAD;
        Self {
            text_w,
            frozen_w,
            estimate_x,
            bid_x,
            stats_x,
            total_w: x,
            block_w,
            columns,
        }
    }

    /// EP and GP cell of a block starting at `left` (screen coordinates).
    fn cells(&self, left: f32, top: f32, height: f32) -> (Option<Rect>, Option<Rect>) {
        let rect = |x: f32, w: f32| Rect::from_min_size(egui::pos2(x, top), Vec2::new(w, height));
        match self.columns {
            PriceColumns::Both => (Some(rect(left, EP_W)), Some(rect(left + EP_W, GP_W))),
            PriceColumns::UnitOnly => (Some(rect(left, self.block_w)), None),
            PriceColumns::TotalOnly => (None, Some(rect(left, self.block_w))),
        }
    }

    /// Where a group total goes: under GP, or under EP when only EP is shown.
    fn total_cell(&self, left: f32, top: f32, height: f32) -> Rect {
        let (unit, total) = self.cells(left, top, height);
        total.or(unit).expect("one column is always shown")
    }
}

impl ComparisonView {
    pub fn new() -> Self {
        let options = ComparisonOptions::default();
        Self {
            options,
            columns: PriceColumns::Both,
            order: BidOrder::Input,
            hidden: HashSet::new(),
            anonymous: false,
            numbering: AnonymousNumbering::ByRank,
            filter: RowFilter::default(),
            collapsed: HashSet::new(),
            undercut_percent: 10,
            outlier_percent: 20,
            editing: None,
            cache: None,
        }
    }

    fn derived(&mut self, project: &Project) -> Arc<Derived> {
        let key = (project.revision(), project.active_lv(), self.options);
        if let Some(derived) = self.cache.as_ref().filter(|derived| derived.key == key) {
            return derived.clone();
        }
        let derived = Arc::new(Derived {
            key,
            comparison: PriceComparison::build(project.lv(), project.bids(), self.options),
            corrections: corrections(project),
            bidder_texts: bidder_texts(project),
            planner_prices: project.lv().has_planner_prices(),
        });
        self.cache = Some(derived.clone());
        derived
    }

    // MERGE(projekt): Vorgaben aus Extras ▸ Einstellungen ▸ Preisspiegel.
    /// A new view with the defaults from the settings.
    pub fn with_defaults(
        outlier_percent: u32,
        reference: proava_domain::DeviationReference,
        undercut_percent: u32,
    ) -> Self {
        let mut view = Self::new();
        view.outlier_percent = outlier_percent;
        view.options.outlier_percent = Decimal::from(outlier_percent);
        view.options.reference = reference;
        view.undercut_percent = undercut_percent;
        view
    }

    /// The current screen settings, for the PDF print of the Preisspiegel.
    pub fn print_options(&self) -> ComparisonPrintOptions {
        let mut hidden_bids: Vec<BidId> = self.hidden.iter().copied().collect();
        hidden_bids.sort();
        ComparisonPrintOptions {
            comparison: self.options,
            columns: self.columns,
            order: self.order,
            hidden_bids,
            anonymous: self.anonymous.then_some(self.numbering),
            totals_only: self.filter.totals_only,
            undercut_percent: Decimal::from(self.undercut_percent),
        }
    }

    /// The on-screen settings as print order for the PDF export.
    pub fn print_settings(&self, project: &Project) -> proava_application::ComparisonPrint {
        let comparison = PriceComparison::build(project.lv(), project.bids(), self.options);
        let order = bid_order(&comparison.bids, &self.hidden, self.order);
        let names = if self.anonymous {
            let all = display_names(&comparison.bids, Some(self.numbering));
            order.iter().map(|&index| all[index].clone()).collect()
        } else {
            Vec::new()
        };
        proava_application::ComparisonPrint {
            options: self.options,
            bids: order
                .iter()
                .map(|&index| comparison.bids[index].bid)
                .collect(),
            names,
            totals_only: self.filter.totals_only,
        }
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
    ) -> ViewResponse {
        let mut response = ViewResponse::default();
        self.options.outlier_percent = Decimal::from(self.outlier_percent);

        if project.bids().is_empty() {
            empty_state(
                ui,
                palette,
                "Noch keine Angebote",
                "Der Preisspiegel stellt die Angebote der Bieter Position für Position nebeneinander. \
                 Lies dafür zuerst die Angebote ein (GAEB X84 oder D84).",
                &mut response,
            );
            return response;
        }

        let derived = self.derived(project);
        let comparison = &derived.comparison;
        if comparison.bids.is_empty() {
            empty_state(
                ui,
                palette,
                "Alle Angebote sind ausgeschlossen",
                "Unter „Angebote“ kannst du den Ausschluss wieder aufheben oder weitere Angebote einlesen.",
                &mut response,
            );
            return response;
        }

        let order = bid_order(&comparison.bids, &self.hidden, self.order);
        let names = display_names(&comparison.bids, self.anonymous.then_some(self.numbering));
        let rows = visible_rows(&comparison.lines, &self.filter, &self.collapsed);
        let vat_percent = project.lv().vat_percent();

        let mut actions: Vec<Action> = Vec::new();
        self.toolbar(
            ui,
            palette,
            comparison,
            derived.planner_prices,
            &mut actions,
            &mut response,
        );
        quantity_banner(
            ui,
            palette,
            comparison,
            project.awarded_bid().is_some(),
            &mut response,
        );
        ui.add_space(8.0);

        let table = Table {
            palette,
            signal: Signal::of(ui),
            comparison,
            order: &order,
            names: &names,
            rows: &rows,
            corrections: &derived.corrections,
            bidder_texts: &derived.bidder_texts,
            vat_percent,
            undercut_percent: Decimal::from(self.undercut_percent),
            with_estimate: self.options.with_estimate && derived.planner_prices,
        };
        table.show(ui, self, &mut actions);

        for action in actions {
            self.apply(action, project, comparison, &mut response);
        }
        response
    }

    fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        comparison: &PriceComparison,
        planner_prices: bool,
        actions: &mut Vec<Action>,
        response: &mut ViewResponse,
    ) {
        let positions = comparison
            .lines
            .iter()
            .filter(|line| matches!(line, ComparisonLine::Position(_)))
            .count();
        let outliers = comparison
            .lines
            .iter()
            .filter(|line| match line {
                ComparisonLine::Position(position) => {
                    position.cells.iter().any(|cell| cell.outlier)
                }
                ComparisonLine::Group(_) => false,
            })
            .count();
        let missing: usize = comparison.bids.iter().map(|bid| bid.missing_prices).sum();

        crate::widgets::row(ui, |ui| {
            ui.label(
                RichText::new("Preisspiegel")
                    .size(17.0)
                    .strong()
                    .color(palette.text),
            );
            ui.add_space(10.0);
            ui.label(
                RichText::new(format!(
                    "{} · {} · {} mit Ausreißern · {}",
                    crate::format::count(comparison.bids.len(), "Angebot", "Angebote"),
                    crate::format::count(positions, "Position", "Positionen"),
                    outliers,
                    crate::format::count(missing, "fehlender Preis", "fehlende Preise")
                ))
                .size(11.5)
                .color(palette.text_dim),
            );
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if accent_button(ui, palette, "Preisspiegel drucken …").clicked() {
                    response
                        .requests
                        .push(ViewRequest::ExportPdf(PdfDocument::PriceComparison));
                }
                ui.add_space(10.0);
                if !self.filter.search.is_empty()
                    && widgets::tool_button(ui, palette, Icon::Close, false, "Suche leeren")
                        .clicked()
                {
                    self.filter.search.clear();
                }
                // Fixed id: the clear button appears in front of the field
                // with the first letter and must not take the focus away.
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter.search)
                        .id(egui::Id::unique("comparison_search"))
                        .hint_text(SEARCH_HINT)
                        .desired_width(220.0),
                );
                widgets::search_mark(ui, palette);
            });
        });
        ui.add_space(8.0);

        crate::widgets::row_wrapped(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for columns in [
                PriceColumns::Both,
                PriceColumns::UnitOnly,
                PriceColumns::TotalOnly,
            ] {
                if segment(ui, palette, columns.label(), self.columns == columns).clicked() {
                    self.columns = columns;
                }
            }
            separator(ui, palette);

            ui.label(
                RichText::new("Abweichung zu")
                    .size(11.5)
                    .color(palette.text_dim),
            );
            egui::ComboBox::from_id_salt("cmp_reference")
                .width(118.0)
                .selected_text(self.options.reference.label())
                .show_ui(ui, |ui| {
                    for reference in [
                        DeviationReference::Mean,
                        DeviationReference::Lowest,
                        DeviationReference::Estimate,
                    ] {
                        ui.selectable_value(
                            &mut self.options.reference,
                            reference,
                            reference.label(),
                        );
                    }
                });
            ui.label(
                RichText::new("Ausreißer ab ±")
                    .size(11.5)
                    .color(palette.text_dim),
            );
            ui.add(
                egui::DragValue::new(&mut self.outlier_percent)
                    .range(1..=500)
                    .speed(0.5)
                    .suffix(" %"),
            );
            separator(ui, palette);

            // Without planner prices the column would only show „–“ and 0,00.
            ui.add_enabled(
                planner_prices,
                egui::Checkbox::new(&mut self.options.with_estimate, "Planerpreise"),
            )
            .on_hover_text("Kostenvoranschlag des Planers als eigene Spalte (wird nicht gerankt)")
            .on_disabled_hover_text("Das LV hat keine Planerpreise – keine Spalte Planerpreis.");
            ui.checkbox(&mut self.options.include_provisional, "Bedarf in Wertung")
                .on_hover_text(
                    "Variante: Bedarfspositionen ohne Gesamtbetrag mit ihrer Menge einrechnen",
                );
            ui.checkbox(&mut self.filter.totals_only, "nur Titelsummen");
            ui.checkbox(&mut self.filter.outliers_only, "nur Ausreißer");
            separator(ui, palette);

            self.bidder_menu(ui, comparison);
            if ui.button("Alle auf").clicked() {
                actions.push(Action::ExpandAll);
            }
            if ui.button("Alle zu").clicked() {
                actions.push(Action::CollapseAll);
            }
        });
    }

    fn bidder_menu(&mut self, ui: &mut egui::Ui, comparison: &PriceComparison) {
        let shown = comparison.bids.len() - self.hidden.len().min(comparison.bids.len());
        let label = if self.anonymous {
            format!("Bieter {shown}/{} · anonym …", comparison.bids.len())
        } else {
            format!("Bieter {shown}/{} …", comparison.bids.len())
        };
        let config = egui::containers::menu::MenuConfig::new()
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
        egui::containers::menu::MenuButton::new(label).config(config).ui(ui, |ui| {
            ui.set_min_width(260.0);
            ui.label(RichText::new("Anzeigen").size(11.0).weak());
            for summary in &comparison.bids {
                let mut visible = !self.hidden.contains(&summary.bid);
                if ui
                    .checkbox(
                        &mut visible,
                        format!(
                            "{} (Rang {}{})",
                            summary.name,
                            summary.rank,
                            if summary.complete { "" } else { ", unvollständig" }
                        ),
                    )
                    .changed()
                {
                    if visible {
                        self.hidden.remove(&summary.bid);
                    } else {
                        self.hidden.insert(summary.bid);
                    }
                }
            }
            if ui.button("Alle zeigen").clicked() {
                self.hidden.clear();
            }
            ui.separator();
            ui.label(RichText::new("Reihenfolge").size(11.0).weak());
            for order in [BidOrder::Input, BidOrder::Rank, BidOrder::Name] {
                ui.radio_value(&mut self.order, order, order.label());
            }
            ui.separator();
            ui.checkbox(&mut self.anonymous, "Bieter anonym („Bieter 1 … n“)");
            ui.add_enabled_ui(self.anonymous, |ui| {
                ui.radio_value(&mut self.numbering, AnonymousNumbering::ByRank, "nummeriert nach Rang");
                ui.radio_value(
                    &mut self.numbering,
                    AnonymousNumbering::ByInput,
                    "nummeriert nach Eingang",
                );
            });
            ui.separator();
            crate::widgets::row(ui, |ui| {
                ui.label("Aufgreifschwelle");
                ui.add(
                    egui::DragValue::new(&mut self.undercut_percent)
                        .range(1..=50)
                        .suffix(" %"),
                );
            })
            .response
            .on_hover_text(
                "Hinweis im Fuß, wenn der Günstigste um mehr als diesen Wert unter dem Zweiten liegt (§ 16d VOB/A)",
            );
        });
    }

    fn apply(
        &mut self,
        action: Action,
        project: &mut Project,
        comparison: &PriceComparison,
        response: &mut ViewResponse,
    ) {
        match action {
            Action::ShowInLv(row) => response.requests.push(ViewRequest::ShowInLv(row)),
            Action::Toggle(row) => {
                if !self.collapsed.remove(&row) {
                    self.collapsed.insert(row);
                }
            }
            Action::CollapseAll => {
                self.collapsed = comparison
                    .lines
                    .iter()
                    .filter_map(|line| match line {
                        ComparisonLine::Group(group) => Some(group.row),
                        ComparisonLine::Position(_) => None,
                    })
                    .collect();
            }
            Action::ExpandAll => self.collapsed.clear(),
            Action::StartEdit(row, bid, price) => {
                self.editing = Some(EditCell {
                    row,
                    bid,
                    text: price.map(edit_text).unwrap_or_default(),
                    started: false,
                    error: false,
                });
            }
            Action::CancelEdit => self.editing = None,
            Action::CommitEdit => {
                let Some(edit) = self.editing.as_mut() else {
                    return;
                };
                match parse_price_input(&edit.text) {
                    Ok(price) => {
                        let ordinal = project
                            .lv()
                            .position_by_id(edit.row)
                            .map(|position| position.ordinal().to_string())
                            .unwrap_or_default();
                        if let Some(bid) = project.bid_mut(edit.bid) {
                            let before = bid.unit_price(edit.row);
                            if before != price {
                                bid.set_unit_price(edit.row, price);
                                response.dirty = true;
                                response.toast = Some(format!(
                                    "EP {} bei {} geändert von {} auf {}",
                                    ordinal,
                                    bid.name(),
                                    price_text(before),
                                    price_text(price)
                                ));
                            }
                        }
                        self.editing = None;
                    }
                    Err(message) => {
                        edit.error = true;
                        edit.started = false;
                        response.toast = Some(message);
                    }
                }
            }
        }
    }
}

struct Table<'a> {
    palette: &'a Palette,
    signal: Signal,
    comparison: &'a PriceComparison,
    order: &'a [usize],
    names: &'a [String],
    rows: &'a [usize],
    corrections: &'a HashMap<(BidId, RowId), (Money, Money)>,
    /// Fabrikat/Typ the bidder filled in (Textergänzungen), per cell.
    bidder_texts: &'a HashMap<(BidId, RowId), Vec<String>>,
    vat_percent: Decimal,
    undercut_percent: Decimal,
    /// Column „Planerpreis“: switched on and the LV has planner prices.
    with_estimate: bool,
}

impl Table<'_> {
    fn show(&self, ui: &mut egui::Ui, view: &mut ComparisonView, actions: &mut Vec<Action>) {
        let palette = self.palette;
        let full = ui.available_rect_before_wrap();
        ui.allocate_rect(full, Sense::hover());
        let undercut = undercut(&self.comparison.bids, self.undercut_percent);
        let foot_h =
            FOOT_ROWS as f32 * FOOT_ROW_H + 12.0 + if undercut.is_some() { 26.0 } else { 0.0 };

        let frame = full;
        ui.painter()
            .rect_filled(frame, CornerRadius::same(8), palette.panel);
        let head = Rect::from_min_size(frame.min, Vec2::new(frame.width(), HEAD_H));
        let foot = Rect::from_min_max(egui::pos2(frame.left(), frame.bottom() - foot_h), frame.max);
        let body = Rect::from_min_max(
            egui::pos2(frame.left(), head.bottom()),
            egui::pos2(frame.right(), foot.top()),
        );

        let columns = Columns::new(
            frame.width(),
            self.order.len(),
            self.with_estimate,
            view.columns,
        );

        let mut body_ui = ui.new_child(UiBuilder::new().max_rect(body).id_salt("cmp_body_ui"));
        let output = egui::ScrollArea::both()
            .id_salt("cmp_body")
            .auto_shrink([false, false])
            .content_margin(0)
            .show_viewport(&mut body_ui, |ui, viewport| {
                self.body(ui, viewport, &columns, view, actions);
            });
        let inner = output.inner_rect;
        let origin_x = inner.left() - output.state.clamped_offset().x;

        self.header(ui, head, inner, origin_x, &columns, view);
        self.footer(ui, foot, inner, origin_x, &columns, undercut);

        ui.painter().rect_stroke(
            frame,
            CornerRadius::same(8),
            Stroke::new(1.0, palette.line),
            StrokeKind::Inside,
        );
    }

    fn block_lefts(&self, columns: &Columns, origin_x: f32) -> Vec<f32> {
        columns.bid_x.iter().map(|x| origin_x + x).collect()
    }

    fn separators(
        &self,
        painter: &Painter,
        columns: &Columns,
        origin_x: f32,
        top: f32,
        bottom: f32,
    ) {
        let stroke = Stroke::new(1.0, self.palette.line);
        let mut lefts: Vec<f32> = columns.estimate_x.into_iter().collect();
        lefts.extend(columns.bid_x.iter().copied());
        lefts.push(columns.stats_x);
        for x in lefts {
            let x = origin_x + x;
            painter.line_segment([egui::pos2(x, top), egui::pos2(x, bottom)], stroke);
        }
    }

    fn header(
        &self,
        ui: &egui::Ui,
        head: Rect,
        inner: Rect,
        origin_x: f32,
        columns: &Columns,
        view: &ComparisonView,
    ) {
        let palette = self.palette;
        let painter = ui.painter().with_clip_rect(head);
        painter.rect_filled(
            head,
            CornerRadius {
                nw: 8,
                ne: 8,
                sw: 0,
                se: 0,
            },
            palette.panel_alt,
        );
        painter.line_segment(
            [head.left_bottom(), head.right_bottom()],
            Stroke::new(1.0, palette.line),
        );

        let top_line = head.top() + 17.0;
        let sub_line = head.top() + 38.0;
        let small = FontId::proportional(10.5);
        let left = inner.left();
        let mut x = left + PAD;
        painter.text(
            egui::pos2(x, top_line),
            Align2::LEFT_CENTER,
            format!(
                "Abweichung zu: {} · Ausreißer ab ±{} %",
                view.options.reference.label(),
                view.outlier_percent
            ),
            small.clone(),
            palette.text_dim,
        );
        painter.text(
            egui::pos2(x, sub_line),
            Align2::LEFT_CENTER,
            "OZ",
            small.clone(),
            palette.text_dim,
        );
        x += OZ_W + MARK_W;
        painter.text(
            egui::pos2(x, sub_line),
            Align2::LEFT_CENTER,
            "Kurztext",
            small.clone(),
            palette.text_dim,
        );
        x += columns.text_w + QTY_W;
        painter.text(
            egui::pos2(x - 6.0, sub_line),
            Align2::RIGHT_CENTER,
            "Menge",
            small.clone(),
            palette.text_dim,
        );
        painter.text(
            egui::pos2(x + 2.0, sub_line),
            Align2::LEFT_CENTER,
            "Einh.",
            small.clone(),
            palette.text_dim,
        );

        let cells_clip = Rect::from_min_max(
            egui::pos2(left + columns.frozen_w, head.top()),
            egui::pos2(inner.right(), head.bottom()),
        );
        let cells = painter.with_clip_rect(cells_clip);
        self.separators(&cells, columns, origin_x, head.top() + 4.0, head.bottom());

        // rank: (Rang, vollständig)
        let block_head =
            |left: f32, title: &str, rank: Option<(usize, bool)>, tooltip: Option<String>| {
                let width = columns.block_w;
                let narrow = width < 180.0;
                // „Rang 3“ or „Rang 3 · unvollständig“ – the number is the rank,
                // not a count of missing prices.
                // Narrow (only EP or GP): the badge shares its line with the
                // column label, „unvollständig“ then stays in the tooltip.
                let badge_text = rank.map(|(rank, complete)| {
                    if complete || narrow {
                        format!("Rang {rank}")
                    } else {
                        format!("Rang {rank} · unvollständig")
                    }
                });
                let badge_width = badge_text.as_ref().map_or(0.0, |text| {
                    cells
                        .layout_no_wrap(text.clone(), FontId::proportional(10.0), palette.text)
                        .size()
                        .x
                        + 12.0
                });
                let name_width = width
                    - if rank.is_some() && !narrow {
                        badge_width + 14.0
                    } else {
                        12.0
                    };
                text_fit(
                    &cells,
                    egui::pos2(left + 8.0, top_line),
                    Align2::LEFT_CENTER,
                    title,
                    FontId::proportional(12.0),
                    palette.text,
                    name_width,
                );
                if let Some((rank, complete)) = rank {
                    let badge = if narrow {
                        Rect::from_min_size(
                            egui::pos2(left + 8.0, sub_line - 8.5),
                            Vec2::new(badge_width, 17.0),
                        )
                    } else {
                        Rect::from_min_size(
                            egui::pos2(left + width - 6.0 - badge_width, top_line - 8.5),
                            Vec2::new(badge_width, 17.0),
                        )
                    };
                    let (fill, text) = if !complete {
                        (self.signal.warn_bg, self.signal.warn_text)
                    } else if rank == 1 {
                        (self.signal.good_bg, self.signal.good_text)
                    } else {
                        (palette.button, palette.text_dim)
                    };
                    cells.rect_filled(badge, CornerRadius::same(8), fill);
                    cells.text(
                        badge.center(),
                        Align2::CENTER_CENTER,
                        badge_text.clone().unwrap_or_default(),
                        FontId::proportional(10.0),
                        text,
                    );
                }
                let (unit, total) = columns.cells(left, head.top() + 28.0, 20.0);
                if let Some(unit) = unit {
                    cells.text(
                        egui::pos2(unit.right() - 8.0, sub_line),
                        Align2::RIGHT_CENTER,
                        "EP",
                        small.clone(),
                        palette.text_dim,
                    );
                }
                if let Some(total) = total {
                    cells.text(
                        egui::pos2(total.right() - 8.0, sub_line),
                        Align2::RIGHT_CENTER,
                        "GP",
                        small.clone(),
                        palette.text_dim,
                    );
                }
                if let Some(text) = tooltip {
                    let rect =
                        Rect::from_min_size(egui::pos2(left, head.top()), Vec2::new(width, HEAD_H))
                            .intersect(cells_clip);
                    if rect.is_positive() {
                        ui.interact(
                            rect,
                            ui.scope_id().with(("cmp_head", left as i32)),
                            Sense::hover(),
                        )
                        .on_hover_text(text);
                    }
                }
            };

        if let Some(x) = columns.estimate_x {
            block_head(origin_x + x, "Planer (Kostenvoranschlag)", None, None);
        }
        for (slot, left) in self.block_lefts(columns, origin_x).into_iter().enumerate() {
            let index = self.order[slot];
            let summary = &self.comparison.bids[index];
            let incomplete = (!summary.complete).then(|| incomplete_note(summary.missing_prices));
            let tooltip = match (view.anonymous, incomplete) {
                (true, None) => None,
                (true, Some(note)) => {
                    Some(format!("{note}. {}", crate::bids_view::INCOMPLETE_HINT))
                }
                (false, None) => Some(summary.name.clone()),
                (false, Some(note)) => Some(format!(
                    "{} – {note}. {}",
                    summary.name,
                    crate::bids_view::INCOMPLETE_HINT
                )),
            };
            block_head(
                left,
                &self.names[index],
                Some((summary.rank, summary.complete)),
                tooltip,
            );
        }
        let stats = origin_x + columns.stats_x;
        for (column, label) in ["Min EP", "Mittel EP", "Max EP"].into_iter().enumerate() {
            cells.text(
                egui::pos2(stats + (column + 1) as f32 * STAT_W - 8.0, sub_line),
                Align2::RIGHT_CENTER,
                label,
                small.clone(),
                palette.text_dim,
            );
        }
        cells.text(
            egui::pos2(stats + 8.0, top_line),
            Align2::LEFT_CENTER,
            "Alle Bieter",
            FontId::proportional(12.0),
            palette.text_dim,
        );
        frozen_edge(
            ui.painter(),
            palette,
            left + columns.frozen_w,
            head.top(),
            head.bottom(),
        );
    }

    fn body(
        &self,
        ui: &mut egui::Ui,
        viewport: Rect,
        columns: &Columns,
        view: &mut ComparisonView,
        actions: &mut Vec<Action>,
    ) {
        let palette = self.palette;
        let origin = ui.max_rect().min;
        let content_h = (self.rows.len() as f32 * ROW_H).max(ROW_H);
        ui.set_min_size(Vec2::new(columns.total_w, content_h));
        let clip = ui.clip_rect();
        let left = clip.left();
        let right = clip
            .right()
            .min(origin.x + columns.total_w)
            .max(left + columns.frozen_w);
        let cells_clip =
            Rect::from_min_max(egui::pos2(left + columns.frozen_w, clip.top()), clip.max);
        let painter = ui.painter().clone();
        let cells = painter.with_clip_rect(cells_clip);

        if self.rows.is_empty() {
            painter.text(
                egui::pos2(left + PAD, origin.y + ROW_H * 0.5 + 6.0),
                Align2::LEFT_CENTER,
                "Keine Zeile passt zum Filter.",
                FontId::proportional(12.0),
                palette.text_dim,
            );
            return;
        }

        let first = (viewport.min.y / ROW_H).floor().max(0.0) as usize;
        let last = ((viewport.max.y / ROW_H).ceil() as usize + 1).min(self.rows.len());
        let hover = ui
            .input(|input| input.pointer.hover_pos())
            .filter(|pos| clip.contains(*pos));
        let lefts = self.block_lefts(columns, origin.x);

        for slot in first..last {
            let line_index = self.rows[slot];
            let top = origin.y + slot as f32 * ROW_H;
            let row = Rect::from_min_max(egui::pos2(left, top), egui::pos2(right, top + ROW_H));
            let hovered = hover.is_some_and(|pos| row.contains(pos));
            match &self.comparison.lines[line_index] {
                ComparisonLine::Group(group) => {
                    let collapsed = view.collapsed.contains(&group.row);
                    // Yellow like the title rows of the LV (Einstellung),
                    // else the soft accent.
                    let fill = palette
                        .title_fill(group.level, hovered)
                        .unwrap_or(palette.accent_soft);
                    painter.rect_filled(row, CornerRadius::ZERO, fill);
                    self.group_frozen(&painter, row, columns, group, collapsed);
                    let frozen = Rect::from_min_size(row.min, Vec2::new(columns.frozen_w, ROW_H));
                    let response = ui
                        .interact(
                            frozen,
                            ui.scope_id().with(("cmp_group", group.row)),
                            Sense::click(),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if response.clicked() {
                        actions.push(Action::Toggle(group.row));
                    }
                    self.group_cells(&cells, top, origin.x, columns, &lefts, group);
                }
                ComparisonLine::Position(position) => {
                    let background = if hovered {
                        palette.row_hover
                    } else if slot % 2 == 1 {
                        palette.row_alt
                    } else {
                        palette.panel
                    };
                    painter.rect_filled(row, CornerRadius::ZERO, background);
                    self.position_frozen(ui, &painter, row, columns, position, actions);
                    self.position_cells(
                        ui, &cells, cells_clip, top, origin.x, columns, &lefts, position, view,
                        actions,
                    );
                }
            }
        }

        let bottom = (origin.y + content_h).min(clip.bottom());
        self.separators(&cells, columns, origin.x, clip.top(), bottom);
        frozen_edge(
            &painter,
            palette,
            left + columns.frozen_w,
            clip.top(),
            bottom,
        );
    }

    fn group_frozen(
        &self,
        painter: &Painter,
        row: Rect,
        columns: &Columns,
        group: &GroupLine,
        collapsed: bool,
    ) {
        let palette = self.palette.on_title_row(group.level);
        // With the title rows on: OZ and title text in the title colour
        // (light: on Hellgelb, dark: yellow on the usual title fill).
        let title_rows = palette.title_rows.is_some();
        let mark = if title_rows {
            palette.group_text
        } else {
            palette.accent
        };
        let middle = row.center().y;
        let indent = group.level as f32 * 10.0;
        let x = row.left() + PAD + indent;
        crate::icons::disclosure(painter, egui::pos2(x + 5.0, middle), collapsed, mark);
        painter.text(
            egui::pos2(x + 16.0, middle),
            Align2::LEFT_CENTER,
            &group.ordinal,
            FontId::monospace(11.5),
            mark,
        );
        let font = if group.level == 0 {
            FontId::proportional(12.5)
        } else {
            FontId::proportional(12.0)
        };
        let text_x = row.left() + PAD + OZ_W + indent.min(MARK_W);
        text_fit(
            painter,
            egui::pos2(text_x, middle),
            Align2::LEFT_CENTER,
            &group.short_text,
            font,
            if title_rows {
                palette.group_text
            } else {
                palette.text
            },
            columns.frozen_w - (text_x - row.left()) - PAD,
        );
    }

    fn group_cells(
        &self,
        painter: &Painter,
        top: f32,
        origin_x: f32,
        columns: &Columns,
        lefts: &[f32],
        group: &GroupLine,
    ) {
        let palette = self.palette.on_title_row(group.level);
        let font = FontId::proportional(12.0);
        if let Some(x) = columns.estimate_x {
            let cell = columns.total_cell(origin_x + x, top, ROW_H);
            painter.text(
                egui::pos2(cell.right() - 8.0, cell.center().y),
                Align2::RIGHT_CENTER,
                group.estimate_total.plain(),
                font.clone(),
                palette.text_dim,
            );
        }
        for (slot, left) in lefts.iter().enumerate() {
            let index = self.order[slot];
            let cell = columns.total_cell(*left, top, ROW_H);
            // Eine Titelsumme mit fehlenden Preisen ist nicht „die niedrigste“.
            let lowest = group.lowest == Some(index)
                && self.order.len() > 1
                && self.comparison.bids[index].complete;
            if lowest {
                painter.rect_filled(
                    cell.shrink2(Vec2::new(3.0, 3.0)),
                    CornerRadius::same(4),
                    self.signal.good_bg,
                );
            }
            painter.text(
                egui::pos2(cell.right() - 8.0, cell.center().y),
                Align2::RIGHT_CENTER,
                group.totals[index].plain(),
                font.clone(),
                if lowest {
                    self.signal.good_text
                } else {
                    palette.text
                },
            );
        }
    }

    fn position_frozen(
        &self,
        ui: &egui::Ui,
        painter: &Painter,
        row: Rect,
        columns: &Columns,
        position: &PositionLine,
        actions: &mut Vec<Action>,
    ) {
        let palette = self.palette;
        let middle = row.center().y;
        let counts = self.comparison.options.counts(position.kind);
        let text_color = if counts {
            palette.text
        } else {
            palette.text_dim
        };
        let mut x = row.left() + PAD;

        let oz_rect =
            Rect::from_min_size(egui::pos2(x - 4.0, row.top()), Vec2::new(OZ_W - 4.0, ROW_H));
        let response = ui
            .interact(
                oz_rect,
                ui.scope_id().with(("cmp_oz", position.row)),
                Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(format!(
                "{} im Leistungsverzeichnis zeigen",
                position.ordinal
            ));
        let ordinal_color = if response.hovered() {
            palette.accent_hover
        } else {
            palette.accent
        };
        // A long OZ ends with „…“ before the column of the marks.
        let mut job = LayoutJob::simple_singleline(
            position.ordinal.clone(),
            FontId::monospace(11.5),
            ordinal_color,
        );
        job.wrap = TextWrapping::truncate_at_width(OZ_W - 8.0);
        let galley = painter.layout_job(job);
        let text_rect = Align2::LEFT_CENTER.anchor_size(egui::pos2(x, middle), galley.size());
        if response.hovered() {
            painter.line_segment(
                [text_rect.left_bottom(), text_rect.right_bottom()],
                Stroke::new(1.0, ordinal_color),
            );
        }
        painter.galley(text_rect.min, galley, ordinal_color);
        if response.clicked() {
            actions.push(Action::ShowInLv(position.row));
        }
        x += OZ_W;

        let mark = kind_mark(position.kind);
        if !mark.is_empty() {
            let badge =
                Rect::from_min_size(egui::pos2(x, middle - 8.5), Vec2::new(MARK_W - 8.0, 17.0));
            painter.rect_filled(badge, CornerRadius::same(4), palette.button);
            painter.rect_stroke(
                badge,
                CornerRadius::same(4),
                Stroke::new(1.0, palette.line),
                StrokeKind::Inside,
            );
            painter.text(
                badge.center(),
                Align2::CENTER_CENTER,
                mark,
                FontId::proportional(10.0),
                palette.text_dim,
            );
            ui.interact(
                badge,
                ui.scope_id().with(("cmp_mark", position.row)),
                Sense::hover(),
            )
            .on_hover_text(position.kind.label());
        }
        x += MARK_W;

        let cut = text_fit(
            painter,
            egui::pos2(x, middle),
            Align2::LEFT_CENTER,
            &position.short_text,
            FontId::proportional(12.0),
            text_color,
            columns.text_w - 6.0,
        );
        if cut {
            let text_rect = Rect::from_min_size(
                egui::pos2(x, row.top()),
                Vec2::new(columns.text_w - 6.0, ROW_H),
            );
            ui.interact(
                text_rect,
                ui.scope_id().with(("cmp_text", position.row)),
                Sense::hover(),
            )
            .on_hover_text(&position.short_text);
        }
        x += columns.text_w + QTY_W;
        // Quantity changed after the bids came in (Massenmehrung/-minderung):
        // in the warning colour, the hint as tooltip (Anwender 18.11.).
        let quantity_rect = painter.text(
            egui::pos2(x - 6.0, middle),
            Align2::RIGHT_CENTER,
            quantity_text(position.quantity),
            FontId::proportional(11.5),
            if position.quantity_change.is_some() {
                palette.warn
            } else {
                palette.text_dim
            },
        );
        if let Some(change) = position.quantity_change {
            ui.interact(
                quantity_rect.expand(3.0),
                ui.scope_id().with(("cmp_qty", position.row)),
                Sense::hover(),
            )
            .on_hover_text(quantity_change_tooltip(&change, &position.unit));
        }
        text_fit(
            painter,
            egui::pos2(x + 2.0, middle),
            Align2::LEFT_CENTER,
            &position.unit,
            FontId::proportional(11.0),
            palette.text_dim,
            UNIT_W - 4.0,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn position_cells(
        &self,
        ui: &mut egui::Ui,
        painter: &Painter,
        cells_clip: Rect,
        top: f32,
        origin_x: f32,
        columns: &Columns,
        lefts: &[f32],
        position: &PositionLine,
        view: &mut ComparisonView,
        actions: &mut Vec<Action>,
    ) {
        let palette = self.palette;
        let counts = self.comparison.options.counts(position.kind);
        let font = FontId::proportional(12.0);
        let middle = top + ROW_H * 0.5;

        if let Some(x) = columns.estimate_x {
            let (unit, total) = columns.cells(origin_x + x, top, ROW_H);
            if let Some(unit) = unit {
                painter.text(
                    egui::pos2(unit.right() - 8.0, middle),
                    Align2::RIGHT_CENTER,
                    price_text(position.estimate_unit_price),
                    font.clone(),
                    palette.text_dim,
                );
            }
            if let Some(total) = total {
                painter.text(
                    egui::pos2(total.right() - 8.0, middle),
                    Align2::RIGHT_CENTER,
                    total_text(
                        position
                            .estimate_unit_price
                            .map(|_| position.estimate_total),
                        counts,
                    ),
                    font.clone(),
                    palette.text_dim,
                );
            }
        }

        for (slot, left) in lefts.iter().enumerate() {
            let index = self.order[slot];
            let summary = &self.comparison.bids[index];
            let cell = &position.cells[index];
            let block =
                Rect::from_min_size(egui::pos2(*left, top), Vec2::new(columns.block_w, ROW_H));
            if !block.intersects(cells_clip) {
                continue;
            }
            let (unit, total) = columns.cells(*left, top, ROW_H);
            let editing = view
                .editing
                .as_ref()
                .is_some_and(|edit| edit.row == position.row && edit.bid == summary.bid);

            let (fill, text_color) = match cell.mark {
                PriceMark::Lowest => (Some(self.signal.good_bg), self.signal.good_text),
                PriceMark::Highest => (Some(self.signal.bad_bg), self.signal.bad_text),
                PriceMark::None => (
                    None,
                    if counts {
                        palette.text
                    } else {
                        palette.text_dim
                    },
                ),
            };
            if let Some(fill) = fill {
                painter.rect_filled(
                    block.shrink2(Vec2::new(3.0, 3.0)),
                    CornerRadius::same(4),
                    fill,
                );
            }

            if editing {
                let rect = unit.or(total).expect("one column is always shown");
                self.editor(
                    ui,
                    cells_clip,
                    rect.shrink2(Vec2::new(3.0, 2.0)),
                    view,
                    actions,
                );
            } else if let Some(unit) = unit {
                match cell.unit_price {
                    Some(price) => {
                        painter.text(
                            egui::pos2(unit.right() - 8.0, middle),
                            Align2::RIGHT_CENTER,
                            format_decimal_de(price, 2),
                            font.clone(),
                            text_color,
                        );
                    }
                    None => missing_mark(painter, unit, palette.warn),
                }
            }
            if let Some(total) = total {
                if !(editing && unit.is_none()) {
                    match cell.unit_price {
                        Some(_) => {
                            painter.text(
                                egui::pos2(total.right() - 8.0, middle),
                                Align2::RIGHT_CENTER,
                                total_text(Some(cell.total), counts),
                                font.clone(),
                                if counts {
                                    palette.text
                                } else {
                                    palette.text_dim
                                },
                            );
                        }
                        None if unit.is_none() => missing_mark(painter, total, palette.warn),
                        None => {}
                    }
                }
            }

            let anchor = unit.or(total).expect("one column is always shown");
            let mut outlier_right = None;
            if cell.outlier {
                let up = cell
                    .deviation_percent
                    .is_some_and(|value| value > Decimal::ZERO);
                outlier_right = Some(
                    text_mark(
                        painter,
                        egui::pos2(anchor.left() + 5.0, middle),
                        if up { "hoch" } else { "tief" },
                        palette.warn,
                    )
                    .right(),
                );
            }
            let correction = self.corrections.get(&(summary.bid, position.row)).copied();
            if correction.is_some() {
                let target = total.unwrap_or(anchor);
                let left = match outlier_right {
                    Some(right) if total.is_none() => right + 4.0,
                    _ => target.left() + 5.0,
                };
                text_mark(painter, egui::pos2(left, middle), "RF", palette.warn);
            }

            if editing {
                continue;
            }
            let interact = block.intersect(cells_clip);
            if !interact.is_positive() {
                continue;
            }
            let response = ui.interact(
                interact,
                ui.scope_id().with(("cmp_cell", position.row, summary.bid)),
                Sense::click(),
            );
            if response.hovered() {
                painter.rect_stroke(
                    block.shrink2(Vec2::new(2.0, 2.0)),
                    CornerRadius::same(4),
                    Stroke::new(1.0, palette.accent),
                    StrokeKind::Inside,
                );
            }
            if response.double_clicked() {
                actions.push(Action::StartEdit(
                    position.row,
                    summary.bid,
                    cell.unit_price,
                ));
            }
            let name = &self.names[index];
            let reference = self.comparison.options.reference;
            let unit_name = position.unit.clone();
            let texts = self.bidder_texts.get(&(summary.bid, position.row));
            response.on_hover_ui(|ui| {
                cell_tooltip(ui, name, cell, reference, &unit_name, correction);
                for text in texts.into_iter().flatten() {
                    ui.label(format!("Bieterangabe: {text}"));
                }
            });
        }

        let stats = origin_x + columns.stats_x;
        for (column, value) in [
            position.min_unit_price,
            position.mean_unit_price,
            position.max_unit_price,
        ]
        .into_iter()
        .enumerate()
        {
            painter.text(
                egui::pos2(stats + (column + 1) as f32 * STAT_W - 8.0, middle),
                Align2::RIGHT_CENTER,
                value
                    .map(|price| format_decimal_de(price, 2))
                    .unwrap_or_default(),
                FontId::proportional(11.5),
                palette.text_dim,
            );
        }
    }

    fn editor(
        &self,
        ui: &mut egui::Ui,
        clip: Rect,
        rect: Rect,
        view: &mut ComparisonView,
        actions: &mut Vec<Action>,
    ) {
        let Some(edit) = view.editing.as_mut() else {
            return;
        };
        let palette = self.palette;
        let id = ui.scope_id().with(("cmp_edit", edit.row, edit.bid));
        let mut child = ui.new_child(UiBuilder::new().max_rect(rect).id_salt((
            "cmp_edit_ui",
            edit.row,
            edit.bid,
        )));
        child.set_clip_rect(clip);
        let output = egui::TextEdit::singleline(&mut edit.text)
            .id(id)
            .align(Align2::RIGHT_CENTER)
            .font(FontId::proportional(12.0))
            .margin(egui::Margin::symmetric(4, 2))
            .desired_width(rect.width())
            .show(&mut child);
        if edit.error {
            child.painter().rect_stroke(
                rect,
                CornerRadius::same(4),
                Stroke::new(1.5, palette.warn),
                StrokeKind::Outside,
            );
        }
        if !edit.started {
            output.response.request_focus();
            let mut state = output.state.clone();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::select_all_str(&edit.text)));
            state.store(ui.ctx(), id);
            edit.started = true;
            return;
        }
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            actions.push(Action::CancelEdit);
        } else if output.response.lost_focus() {
            actions.push(Action::CommitEdit);
        }
    }

    fn footer(
        &self,
        ui: &egui::Ui,
        foot: Rect,
        inner: Rect,
        origin_x: f32,
        columns: &Columns,
        undercut: Option<Undercut>,
    ) {
        let palette = self.palette;
        let painter = ui.painter().with_clip_rect(foot);
        painter.rect_filled(
            foot,
            CornerRadius {
                nw: 0,
                ne: 0,
                sw: 8,
                se: 8,
            },
            palette.panel_alt,
        );
        painter.line_segment(
            [foot.left_top(), foot.right_top()],
            Stroke::new(1.0, palette.line),
        );

        let left = inner.left();
        let labels = [
            "Summe netto",
            "Nachlass",
            "Netto nach Nachlass",
            &format!("USt {} %", format_decimal_de(self.vat_percent, 0)),
            "Brutto",
            "Rang",
            "über Günstigstem",
        ];
        let row_top = |row: usize| foot.top() + 6.0 + row as f32 * FOOT_ROW_H;
        for (row, label) in labels.iter().enumerate() {
            let strong = row == 2 || row == 5;
            painter.text(
                egui::pos2(left + PAD + OZ_W + MARK_W, row_top(row) + FOOT_ROW_H * 0.5),
                Align2::LEFT_CENTER,
                *label,
                if strong {
                    FontId::proportional(12.0)
                } else {
                    FontId::proportional(11.5)
                },
                if strong {
                    palette.text
                } else {
                    palette.text_dim
                },
            );
        }
        painter.line_segment(
            [
                egui::pos2(left + PAD, row_top(5)),
                egui::pos2(inner.right(), row_top(5)),
            ],
            Stroke::new(1.0, palette.line),
        );

        let cells_clip = Rect::from_min_max(
            egui::pos2(left + columns.frozen_w, foot.top()),
            egui::pos2(inner.right(), foot.bottom()),
        );
        let cells = painter.with_clip_rect(cells_clip);
        self.separators(&cells, columns, origin_x, foot.top(), row_top(FOOT_ROWS));

        // One line within the block („unvollständig – 3 Preise fehlen“ ends
        // with „…“ when only EP is shown; the head's tooltip has it in full).
        let value = |block_left: f32, row: usize, text: String, color: Color32, strong: bool| {
            let cell = columns.total_cell(block_left, row_top(row), FOOT_ROW_H);
            text_fit(
                &cells,
                egui::pos2(cell.right() - 8.0, cell.center().y),
                Align2::RIGHT_CENTER,
                &text,
                if strong {
                    FontId::proportional(12.5)
                } else {
                    FontId::proportional(11.5)
                },
                color,
                cell.right() - 8.0 - (block_left + 6.0),
            );
        };

        if let Some(x) = columns.estimate_x {
            let net = self.comparison.estimate_net;
            let vat = net.percent(self.vat_percent);
            let block = origin_x + x;
            value(block, 0, net.plain(), palette.text_dim, false);
            value(block, 2, net.plain(), palette.text_dim, true);
            value(block, 3, vat.plain(), palette.text_dim, false);
            value(block, 4, (net + vat).plain(), palette.text_dim, false);
            value(
                block,
                5,
                "nicht gewertet".to_string(),
                palette.text_dim,
                false,
            );
        }

        for (slot, block) in self.block_lefts(columns, origin_x).into_iter().enumerate() {
            let summary = &self.comparison.bids[self.order[slot]];
            let first = summary.rank == 1 && summary.complete;
            let strong_color = if first {
                self.signal.good_text
            } else {
                palette.text
            };
            value(block, 0, summary.net.plain(), palette.text, false);
            value(
                block,
                1,
                if summary.discount.is_zero() {
                    "—".to_string()
                } else {
                    (-summary.discount).plain()
                },
                palette.text_dim,
                false,
            );
            value(
                block,
                2,
                summary.net_after_discount.plain(),
                strong_color,
                true,
            );
            value(block, 3, summary.vat.plain(), palette.text_dim, false);
            value(block, 4, summary.gross.plain(), palette.text, false);
            value(block, 5, format!("{}.", summary.rank), strong_color, true);
            value(
                block,
                6,
                if summary.net_after_discount.is_zero() {
                    "keine Preise".to_string()
                } else if !summary.complete {
                    incomplete_note(summary.missing_prices)
                } else if first {
                    "günstigster".to_string()
                } else {
                    format!("+{} %", format_decimal_de(summary.above_lowest_percent, 2))
                },
                if !summary.complete {
                    self.signal.warn_text
                } else if first {
                    self.signal.good_text
                } else {
                    palette.text_dim
                },
                false,
            );
            if summary.quantity_changes > 0 {
                let cell = columns.total_cell(block, row_top(2), FOOT_ROW_H);
                let hint = text_mark(
                    &cells,
                    egui::pos2(block + 6.0, cell.center().y),
                    "Menge",
                    palette.warn,
                );
                let rect = hint.expand(2.0).intersect(cells_clip);
                if rect.is_positive() {
                    ui.interact(
                        rect,
                        ui.scope_id().with(("cmp_foot_qty", summary.bid)),
                        Sense::hover(),
                    )
                    .on_hover_text(at_lv_quantities_note(summary));
                }
            }
            if summary.missing_prices > 0 || summary.arithmetic_errors > 0 {
                let cell = columns.total_cell(block, row_top(0), FOOT_ROW_H);
                let hint = text_mark(
                    &cells,
                    egui::pos2(block + 6.0, cell.top() + 10.5),
                    "prüfen",
                    palette.warn,
                );
                let rect = hint.expand(2.0).intersect(cells_clip);
                if rect.is_positive() {
                    ui.interact(rect, ui.scope_id().with(("cmp_foot_hint", summary.bid)), Sense::hover())
                        .on_hover_text(format!(
                            "{} fehlende Preise, {} Rechenfehler – Summe ist unvollständig bzw. aus den EP neu gerechnet.",
                            summary.missing_prices, summary.arithmetic_errors
                        ));
                }
            }
        }
        frozen_edge(
            ui.painter(),
            palette,
            left + columns.frozen_w,
            foot.top(),
            row_top(FOOT_ROWS),
        );

        if let Some(undercut) = undercut {
            let band = Rect::from_min_max(
                egui::pos2(foot.left() + 8.0, row_top(FOOT_ROWS) + 2.0),
                egui::pos2(foot.right() - 8.0, foot.bottom() - 4.0),
            );
            painter.rect_filled(band, CornerRadius::same(5), self.signal.bad_bg);
            painter.text(
                egui::pos2(band.left() + 12.0, band.center().y),
                Align2::LEFT_CENTER,
                format!(
                    "Aufgreifschwelle prüfen: {} liegt {} % unter {} (Schwelle {} %, § 16d Abs. 1 VOB/A).",
                    self.names[undercut.lowest],
                    format_decimal_de(undercut.percent, 1),
                    self.names[undercut.second],
                    format_decimal_de(self.undercut_percent, 0)
                ),
                FontId::proportional(12.0),
                self.signal.bad_text,
            );
        }
    }
}

/// Tooltip of a quantity changed after the bids came in.
pub fn quantity_change_tooltip(change: &proava_domain::QuantityChange, unit: &str) -> String {
    format!(
        "{} – nach dem Angebotseingang im LV geändert. Gewertet wird mit der Angebotsmenge; \
         die Differenz ist eine Mehr-/Mindermenge (Nachtrag).",
        change.hint(unit)
    )
}

/// Tooltip of the footer mark „Menge“: the sum with the current LV quantities.
pub fn at_lv_quantities_note(summary: &BidSummary) -> String {
    format!(
        "{}: netto nach Nachlass mit den aktuellen LV-Mengen {} (gewertet: {} mit den Angebotsmengen).",
        crate::format::count(summary.quantity_changes, "Menge nach Angebotseingang geändert", "Mengen nach Angebotseingang geändert"),
        summary.after_discount_at_lv_quantities,
        summary.net_after_discount
    )
}

/// Title of the hint band: Massenmehrung, -minderung or both.
pub fn quantity_banner_text(comparison: &PriceComparison) -> Option<String> {
    let changes = comparison.quantity_changes();
    let first = changes.first()?;
    let increase = changes.iter().any(|line| {
        line.quantity_change
            .is_some_and(|change| change.is_increase())
    });
    let decrease = changes.iter().any(|line| {
        line.quantity_change
            .is_some_and(|change| !change.is_increase())
    });
    let what = match (increase, decrease) {
        (true, false) => "Massenmehrung",
        (false, true) => "Massenminderung",
        _ => "Massenmehrung/-minderung",
    };
    let example = first
        .quantity_change
        .map(|change| {
            format!(
                "{} {}",
                first.ordinal,
                change.hint(&first.unit).trim_start_matches("Achtung ")
            )
        })
        .unwrap_or_default();
    let more = if changes.len() > 1 {
        format!(" und {} weitere", changes.len() - 1)
    } else {
        String::new()
    };
    Some(format!(
        "Achtung {what} nach Angebotseingang: {example}{more}. Gewertet wird mit den Angebotsmengen; \
         die Summen mit LV-Menge stehen am „Menge“ unter den Summen."
    ))
}

/// Hint band above the table when LV quantities changed after the bids
/// came in; with an awarded bid the step into a Nachtrag.
fn quantity_banner(
    ui: &mut egui::Ui,
    palette: &Palette,
    comparison: &PriceComparison,
    awarded: bool,
    response: &mut ViewResponse,
) {
    let Some(text) = quantity_banner_text(comparison) else {
        return;
    };
    ui.add_space(6.0);
    egui::Frame::new()
        .fill(palette.warn.gamma_multiply(0.12))
        .corner_radius(CornerRadius::same(5))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(text).size(12.0).color(palette.text));
                if awarded
                    && ui
                        .button(strings::QUANTITY_CHANGE_ORDER)
                        .on_hover_text(strings::QUANTITY_CHANGE_ORDER_HINT)
                        .clicked()
                {
                    response.requests.push(ViewRequest::QuantityChangeOrder);
                }
            });
        });
}

fn cell_tooltip(
    ui: &mut egui::Ui,
    name: &str,
    cell: &PriceCell,
    reference: DeviationReference,
    unit: &str,
    correction: Option<(Money, Money)>,
) {
    let strong = ui.visuals().text_color();
    ui.label(RichText::new(name).color(strong).size(13.0));
    match cell.unit_price {
        Some(price) => {
            ui.label(format!(
                "EP {} € · GP {}",
                format_decimal_de(price, 2),
                cell.total
            ));
        }
        None => {
            ui.label("Kein Preis angeboten.");
        }
    }
    ui.label(deviation_text(cell, reference, unit));
    match cell.mark {
        PriceMark::Lowest => {
            ui.label("Günstigster EP der Zeile");
        }
        PriceMark::Highest => {
            ui.label("Teuerster EP der Zeile");
        }
        PriceMark::None => {}
    }
    if cell.outlier {
        ui.label(RichText::new("Ausreißer").color(ui.visuals().warn_fg_color));
    }
    if let Some((stated, computed)) = correction {
        ui.label(format!(
            "Rechenfehler: GP laut Angebot {stated}, aus EP gerechnet {computed} (gewertet wird der EP)"
        ));
    }
    ui.label(RichText::new("Doppelklick: EP ändern").weak());
}

fn empty_state(
    ui: &mut egui::Ui,
    palette: &Palette,
    title: &str,
    hint: &str,
    response: &mut ViewResponse,
) {
    ui.add_space(70.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(title).size(18.0).strong().color(palette.text));
        ui.add_space(8.0);
        centered_paragraph(ui, hint, 520.0, palette.text_dim);
        ui.add_space(18.0);
        if big_accent_button(ui, palette, "Angebote einlesen (X84/D84) …").clicked() {
            response.requests.push(ViewRequest::ImportBids);
        }
    });
}

// ---------------------------------------------------------------------------
// Small painted pieces, shared with the bids view.

/// A wrapped paragraph of `width`, centred in the available space.
pub(crate) fn centered_paragraph(ui: &mut egui::Ui, text: &str, width: f32, color: Color32) {
    let width = width.min(ui.available_width());
    let left = ui.max_rect().center().x - width * 0.5;
    let rect = Rect::from_min_size(egui::pos2(left, ui.cursor().top()), Vec2::new(width, 400.0));
    let response = ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(Align::Center)),
        |ui| {
            ui.add(egui::Label::new(RichText::new(text).size(12.0).color(color)).wrap());
        },
    );
    let used = response.response.rect;
    ui.allocate_space(Vec2::new(0.0, used.height()));
}

/// One line of a table cell, cut with „…“ at `max_width`; true when cut.
pub(crate) fn text_fit(
    painter: &Painter,
    pos: Pos2,
    align: Align2,
    text: &str,
    font: FontId,
    color: Color32,
    max_width: f32,
) -> bool {
    // One line: line breaks of a GAEB-Kurztext become spaces, the rest „…“.
    let mut job =
        LayoutJob::simple_singleline(crate::format::one_line(text).into_owned(), font, color);
    job.wrap = TextWrapping::truncate_at_width(max_width.max(8.0));
    let galley = painter.layout_job(job);
    let cut = galley.elided;
    let rect = align.anchor_size(pos, galley.size());
    painter.galley(rect.min, galley, color);
    cut
}

fn frozen_edge(painter: &Painter, palette: &Palette, x: f32, top: f32, bottom: f32) {
    painter.line_segment(
        [egui::pos2(x, top), egui::pos2(x, bottom)],
        Stroke::new(1.0, palette.line),
    );
    painter.line_segment(
        [egui::pos2(x + 1.0, top), egui::pos2(x + 1.0, bottom)],
        Stroke::new(2.0, palette.line.gamma_multiply(0.45)),
    );
}

/// A short warning word in a price cell („hoch“, „tief“, „RF“, „prüfen“):
/// text instead of drawn symbols. Returns the rect of the text.
fn text_mark(painter: &Painter, left_center: Pos2, text: &str, color: Color32) -> Rect {
    painter.text(
        left_center,
        Align2::LEFT_CENTER,
        text,
        crate::fonts::bold(10.5),
        color,
    )
}

fn missing_mark(painter: &Painter, cell: Rect, color: Color32) {
    painter.text(
        egui::pos2(cell.right() - 8.0, cell.center().y),
        Align2::RIGHT_CENTER,
        "—",
        FontId::proportional(12.0),
        color,
    );
}

fn separator(ui: &mut egui::Ui, palette: &Palette) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(9.0, 22.0), Sense::hover());
    ui.painter().line_segment(
        [
            rect.center_top() + Vec2::new(0.0, 3.0),
            rect.center_bottom() - Vec2::new(0.0, 3.0),
        ],
        Stroke::new(1.0, palette.line),
    );
}

pub(crate) fn segment(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    active: bool,
) -> egui::Response {
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_string(), FontId::proportional(12.0), palette.text);
    let size = Vec2::new(galley.size().x + 22.0, 26.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let (fill, text) = if active {
        (palette.accent, palette.on_accent)
    } else if response.hovered() {
        (palette.button_hover, palette.text)
    } else {
        (palette.button, palette.text)
    };
    ui.painter().rect_filled(rect, CornerRadius::same(6), fill);
    if !active {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(6),
            Stroke::new(1.0, palette.line),
            StrokeKind::Inside,
        );
    }
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(12.0),
        text,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub(crate) fn accent_button(ui: &mut egui::Ui, palette: &Palette, label: &str) -> egui::Response {
    sized_button(ui, palette, label, 30.0, 12.0, true)
}

pub(crate) fn plain_button(ui: &mut egui::Ui, palette: &Palette, label: &str) -> egui::Response {
    sized_button(ui, palette, label, 30.0, 12.0, false)
}

pub(crate) fn big_accent_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
) -> egui::Response {
    sized_button(ui, palette, label, 40.0, 13.5, true)
}

fn sized_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    height: f32,
    size: f32,
    primary: bool,
) -> egui::Response {
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_string(), FontId::proportional(size), palette.text);
    let width = galley.size().x + if height > 34.0 { 44.0 } else { 28.0 };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    // Name for screen readers and UI tests.
    crate::widgets::name_button(ui, &response, label);
    let (fill, text) = if primary {
        (
            if response.hovered() {
                palette.accent_hover
            } else {
                palette.accent
            },
            palette.on_accent,
        )
    } else if response.hovered() {
        (palette.button_hover, palette.text)
    } else {
        (palette.button, palette.text)
    };
    ui.painter().rect_filled(rect, CornerRadius::same(7), fill);
    if !primary {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(7),
            Stroke::new(1.0, palette.line),
            StrokeKind::Inside,
        );
    }
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(size),
        text,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

// ---------------------------------------------------------------------------
// Logic without egui.

/// Arithmetic errors of all bids: (stated GP, recomputed GP).
fn bidder_texts(project: &Project) -> HashMap<(BidId, RowId), Vec<String>> {
    let lv = project.lv();
    project
        .bids()
        .iter()
        .flat_map(|bid| {
            bid.complements().keys().filter_map(move |row| {
                let position = lv.position_by_id(*row)?;
                Some(((bid.id(), *row), bid.complement_lines(position)))
            })
        })
        .collect()
}

fn corrections(project: &Project) -> HashMap<(BidId, RowId), (Money, Money)> {
    let lv = project.lv();
    project
        .bids()
        .iter()
        .filter(|bid| !bid.is_excluded())
        .flat_map(|bid| {
            bid.arithmetic_errors(lv)
                .into_iter()
                .map(move |(position, stated, computed)| {
                    ((bid.id(), position.id()), (stated, computed))
                })
        })
        .collect()
}

/// Short mark of a position kind including Pauschal/Stundenlohn is not
/// available on the line, so only the kind mark is shown.
fn kind_mark(kind: PositionKind) -> &'static str {
    kind.mark()
}

/// Indices into the comparison lines that are visible with this filter.
/// Filtering (search, outliers) ignores collapsed titles so hits are never hidden.
pub fn visible_rows(
    lines: &[ComparisonLine],
    filter: &RowFilter,
    collapsed: &HashSet<RowId>,
) -> Vec<usize> {
    let query = filter.search.trim().to_lowercase();
    let filtering = filter.outliers_only || !query.is_empty();
    let text_hit = |ordinal: &str, text: &str| {
        !query.is_empty()
            && (ordinal.to_lowercase().contains(&query) || text.to_lowercase().contains(&query))
    };
    let position_hit = |position: &PositionLine| {
        (!filter.outliers_only || position.cells.iter().any(|cell| cell.outlier))
            && (query.is_empty() || text_hit(&position.ordinal, &position.short_text))
    };

    let mut has_hit = vec![false; lines.len()];
    let mut open: Vec<(usize, usize)> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        match line {
            ComparisonLine::Group(group) => {
                while open.last().is_some_and(|(level, _)| *level >= group.level) {
                    open.pop();
                }
                open.push((group.level, index));
            }
            ComparisonLine::Position(position) => {
                if position_hit(position) {
                    for (_, group) in &open {
                        has_hit[*group] = true;
                    }
                }
            }
        }
    }

    struct Open {
        level: usize,
        hides_children: bool,
        text_hit: bool,
    }
    let mut stack: Vec<Open> = Vec::new();
    let mut visible = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        match line {
            ComparisonLine::Group(group) => {
                while stack.last().is_some_and(|open| open.level >= group.level) {
                    stack.pop();
                }
                let parent_hidden = stack.iter().any(|open| open.hides_children);
                let inherited_hit = stack.iter().any(|open| open.text_hit);
                let own_hit = !filter.outliers_only && text_hit(&group.ordinal, &group.short_text);
                let show =
                    !parent_hidden && (!filtering || has_hit[index] || own_hit || inherited_hit);
                if show {
                    visible.push(index);
                }
                stack.push(Open {
                    level: group.level,
                    hides_children: !show || (!filtering && collapsed.contains(&group.row)),
                    text_hit: own_hit || inherited_hit,
                });
            }
            ComparisonLine::Position(position) => {
                if filter.totals_only || stack.iter().any(|open| open.hides_children) {
                    continue;
                }
                let inherited_hit = stack.iter().any(|open| open.text_hit);
                if !filtering || position_hit(position) || inherited_hit {
                    visible.push(index);
                }
            }
        }
    }
    visible
}

/// Indices into `bids` in the chosen order, without hidden bids.
pub fn bid_order(bids: &[BidSummary], hidden: &HashSet<BidId>, order: BidOrder) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..bids.len())
        .filter(|index| !hidden.contains(&bids[*index].bid))
        .collect();
    match order {
        BidOrder::Input => {}
        BidOrder::Rank => indices.sort_by_key(|index| bids[*index].rank),
        BidOrder::Name => indices.sort_by_key(|index| bids[*index].name.to_lowercase()),
    }
    indices
}

/// Names to show, one per bid: real names or „Bieter n“.
pub fn display_names(bids: &[BidSummary], anonymous: Option<AnonymousNumbering>) -> Vec<String> {
    match anonymous {
        None => bids.iter().map(|bid| bid.name.clone()).collect(),
        Some(AnonymousNumbering::ByInput) => (1..=bids.len())
            .map(|number| format!("Bieter {number}"))
            .collect(),
        Some(AnonymousNumbering::ByRank) => bids
            .iter()
            .map(|bid| format!("Bieter {}", bid.rank))
            .collect(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Undercut {
    /// Index of the cheapest bid.
    pub lowest: usize,
    pub second: usize,
    /// How far the cheapest lies below the second, in percent of the second.
    pub percent: Decimal,
}

/// The cheapest bid lies more than `threshold` percent below the second one
/// (Aufgreifschwelle, § 16d Abs. 1 VOB/A).
pub fn undercut(bids: &[BidSummary], threshold: Decimal) -> Option<Undercut> {
    let lowest = bids.iter().position(|bid| bid.rank == 1 && bid.complete)?;
    let second = bids.iter().position(|bid| bid.rank == 2 && bid.complete)?;
    let first_total = bids[lowest].net_after_discount.to_decimal();
    let second_total = bids[second].net_after_discount.to_decimal();
    if second_total <= Decimal::ZERO {
        return None;
    }
    let percent = ((second_total - first_total) / second_total * Decimal::ONE_HUNDRED).round_dp(1);
    (percent > threshold).then_some(Undercut {
        lowest,
        second,
        percent,
    })
}

/// „unvollständig – n Preise fehlen“ for the head and footer of a bid.
pub fn incomplete_note(missing: usize) -> String {
    match missing {
        1 => "unvollständig – 1 Preis fehlt".to_string(),
        count => format!("unvollständig – {count} Preise fehlen"),
    }
}

/// Reads a unit price typed in German format; empty clears the price.
pub fn parse_price_input(text: &str) -> Result<Option<Decimal>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    proava_domain::NumberKind::UnitPrice
        .parse(text)
        .map(Some)
        .map_err(|error| format!("Einheitspreis: {error}"))
}

/// Text for the editor: German, without thousands dots.
fn edit_text(price: Decimal) -> String {
    format_decimal_de(price, 2).replace('.', "")
}

pub fn price_text(price: Option<Decimal>) -> String {
    price
        .map(|value| format_decimal_de(value, 2))
        .unwrap_or_else(|| "—".to_string())
}

/// GP for the table; positions that do not count are shown in brackets.
pub fn total_text(total: Option<Money>, counts: bool) -> String {
    match total {
        None => "—".to_string(),
        Some(total) if counts => total.plain(),
        Some(total) => format!("({})", total.plain()),
    }
}

fn quantity_text(value: Decimal) -> String {
    let text = format_decimal_de(value, 3);
    text.trim_end_matches('0').trim_end_matches(',').to_string()
}

pub fn signed_percent(value: Decimal) -> String {
    let sign = if value > Decimal::ZERO { "+" } else { "" };
    format!("{sign}{} %", format_decimal_de(value, 1))
}

pub fn signed_amount(value: Decimal) -> String {
    let sign = if value > Decimal::ZERO { "+" } else { "" };
    format!("{sign}{} €", format_decimal_de(value, 2))
}

/// „+12,5 % (+3,40 € je St) zum Mittelwert“
pub fn deviation_text(cell: &PriceCell, reference: DeviationReference, unit: &str) -> String {
    let base = match reference {
        DeviationReference::Mean => "zum Mittelwert",
        DeviationReference::Lowest => "zum Günstigsten",
        DeviationReference::Estimate => "zum Planerpreis",
    };
    match (cell.deviation_percent, cell.deviation_amount) {
        (Some(percent), Some(amount)) => {
            let per = if unit.is_empty() {
                String::new()
            } else {
                format!(" je {unit}")
            };
            format!(
                "Abweichung {base}: {} ({}{per})",
                signed_percent(percent),
                signed_amount(amount)
            )
        }
        (None, Some(amount)) => format!("Abweichung {base}: {}", signed_amount(amount)),
        _ => format!("Keine Abweichung {base} berechenbar"),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use proava_domain::{
        BidId, BidSummary, ComparisonLine, ComparisonOptions, Decimal, DeviationReference, Money,
        PriceComparison, RowId,
    };

    use super::{
        bid_order, deviation_text, display_names, parse_price_input, price_text, signed_percent,
        total_text, undercut, visible_rows, AnonymousNumbering, BidOrder, RowFilter,
    };
    use crate::demo::demo_project;

    fn dec(text: &str) -> Decimal {
        text.parse().unwrap()
    }

    fn summary(id: u64, name: &str, rank: usize, cents: i64) -> BidSummary {
        BidSummary {
            bid: BidId(id),
            name: name.to_string(),
            net: Money::from_cents(cents),
            discount: Money::ZERO,
            net_after_discount: Money::from_cents(cents),
            vat: Money::ZERO,
            gross: Money::from_cents(cents),
            rank,
            above_lowest_percent: Decimal::ZERO,
            missing_prices: 0,
            arithmetic_errors: 0,
            quantity_changes: 0,
            after_discount_at_lv_quantities: Money::from_cents(cents),
            complete: true,
        }
    }

    #[test]
    fn price_input_is_german() {
        assert_eq!(parse_price_input("1.234,5").unwrap(), Some(dec("1234.5")));
        assert_eq!(parse_price_input(" 12 € ").unwrap(), Some(dec("12")));
        assert_eq!(parse_price_input("").unwrap(), None);
        assert!(parse_price_input("zwölf").is_err());
        assert!(parse_price_input("1,23456").is_err());
        assert_eq!(parse_price_input("1,125").unwrap(), Some(dec("1.125")));
        assert_eq!(parse_price_input("1.150").unwrap(), Some(dec("1150")));
    }

    #[test]
    fn texts() {
        assert_eq!(price_text(None), "—");
        assert_eq!(price_text(Some(dec("1234.5"))), "1.234,50");
        assert_eq!(total_text(Some(Money::from_cents(150)), false), "(1,50)");
        assert_eq!(signed_percent(dec("12.34")), "+12,3 %");
        assert_eq!(signed_percent(dec("-4")), "-4,0 %");
    }

    #[test]
    fn order_names_and_undercut() {
        let bids = vec![
            summary(1, "Zeta", 2, 11_000),
            summary(2, "Alpha", 3, 13_000),
            summary(3, "Mitte", 1, 9_500),
        ];
        let mut hidden = HashSet::new();
        assert_eq!(bid_order(&bids, &hidden, BidOrder::Rank), vec![2, 0, 1]);
        assert_eq!(bid_order(&bids, &hidden, BidOrder::Name), vec![1, 2, 0]);
        hidden.insert(BidId(2));
        assert_eq!(bid_order(&bids, &hidden, BidOrder::Input), vec![0, 2]);

        assert_eq!(display_names(&bids, None)[1], "Alpha");
        assert_eq!(
            display_names(&bids, Some(AnonymousNumbering::ByRank)),
            vec!["Bieter 2", "Bieter 3", "Bieter 1"]
        );
        assert_eq!(
            display_names(&bids, Some(AnonymousNumbering::ByInput))[2],
            "Bieter 3"
        );

        let found = undercut(&bids, dec("10")).unwrap();
        assert_eq!((found.lowest, found.second), (2, 0));
        assert_eq!(found.percent, dec("13.6"));
        assert!(undercut(&bids, dec("15")).is_none());
    }

    #[test]
    fn filters_collapse_and_search() {
        let project = demo_project();
        let comparison =
            PriceComparison::build(project.lv(), project.bids(), ComparisonOptions::default());
        let lines = &comparison.lines;
        let all = visible_rows(lines, &RowFilter::default(), &HashSet::new());
        assert_eq!(all.len(), lines.len());

        let totals = visible_rows(
            lines,
            &RowFilter {
                totals_only: true,
                ..RowFilter::default()
            },
            &HashSet::new(),
        );
        assert!(totals
            .iter()
            .all(|index| matches!(lines[*index], ComparisonLine::Group(_))));

        let first_title: RowId = match &lines[0] {
            ComparisonLine::Group(group) => group.row,
            ComparisonLine::Position(_) => unreachable!(),
        };
        let collapsed: HashSet<RowId> = [first_title].into_iter().collect();
        let folded = visible_rows(lines, &RowFilter::default(), &collapsed);
        assert_eq!(folded[0], 0);
        assert!(matches!(&lines[folded[1]], ComparisonLine::Group(group) if group.level == 0));

        let search = visible_rows(
            lines,
            &RowFilter {
                search: "dreiwege".to_string(),
                ..RowFilter::default()
            },
            &collapsed,
        );
        assert_eq!(search.len(), 3);
        assert!(
            matches!(&lines[search[2]], ComparisonLine::Position(position) if position.short_text.starts_with("Dreiwegeventil"))
        );

        let outliers = visible_rows(
            lines,
            &RowFilter {
                outliers_only: true,
                ..RowFilter::default()
            },
            &HashSet::new(),
        );
        assert!(outliers.iter().any(|index| matches!(
            &lines[*index],
            ComparisonLine::Position(position) if position.cells.iter().any(|cell| cell.outlier)
        )));
        assert!(outliers.iter().all(|index| match &lines[*index] {
            ComparisonLine::Position(position) => position.cells.iter().any(|cell| cell.outlier),
            ComparisonLine::Group(_) => true,
        }));
    }

    #[test]
    fn deviation_tooltip_names_reference_and_euros() {
        let project = demo_project();
        let comparison = PriceComparison::build(
            project.lv(),
            project.bids(),
            ComparisonOptions {
                reference: DeviationReference::Estimate,
                ..ComparisonOptions::default()
            },
        );
        let ComparisonLine::Position(position) = &comparison.lines[2] else {
            panic!("expected position");
        };
        let text = deviation_text(&position.cells[0], DeviationReference::Estimate, "St");
        assert!(text.starts_with("Abweichung zum Planerpreis: "));
        assert!(text.contains("€ je St"));
    }
}
