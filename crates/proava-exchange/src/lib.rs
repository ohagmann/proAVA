//! Adapter between the proAVA domain and the outside world of GAEB and PDF.
//!
//! Maps a [`proava_domain::Project`] to the GAEB exchange model and back,
//! matches bidders' files against the LV and implements the application
//! port [`proava_application::ExchangeFiles`].

mod bid_match;
mod billing_import;
mod billing_print;
mod change_order;
mod clip;
mod contract;
mod cost_lv_print;
mod cost_print;
mod cover;
pub mod csv_export;
pub mod csv_import;
mod files;
mod from_gaeb;
mod letters;
pub mod memory;
mod oz;
mod print_job;
mod rule_check;
mod stlb;
mod submission;
mod text;
mod to_gaeb;
mod to_print;
pub mod tracking_csv;
mod tracking_print;

pub use bid_match::{match_bid, match_bid_with};
pub use billing_import::{contract_from_gaeb, invoice_from_gaeb};
pub use billing_print::{billing_check_to_print, release_to_print};
pub use change_order::{
    change_order_check_to_print, change_order_offer_from_gaeb, change_order_overview_to_print,
    change_order_print_kind, change_order_to_gaeb,
};
pub use clip::GAEB_RENDERS;
pub use contract::{bid_prices_to_print, contract_to_gaeb, contract_to_print};
pub use cost_lv_print::{cost_lv_to_print, CostLvPrintOptions};
pub use cost_print::cost_estimate_to_print;
pub use cover::project_cover;
pub use files::{GaebFormat, GaebPdfExchange};
pub use from_gaeb::gaeb_to_project;
pub use letters::{letters_to_print, release_letter_to_print};
pub use oz::{
    breakdown_from_mask, breakdown_with_labels, labels_from_breakdown, mask_from_breakdown,
};
pub use print_job::{print_selection, render_print, render_print_within};
pub use rule_check::{export_phase, export_rule_findings, rule_line, RuleFinding, RuleSeverity};
pub use submission::submission_to_print;
pub use to_gaeb::lv_to_gaeb;
pub use to_print::{
    cost_groups_to_print, lv_to_print, page_frame_to_print, price_comparison_to_print,
    price_comparison_to_print_with,
};
pub use tracking_print::cost_tracking_to_print;

#[cfg(test)]
mod tests;
