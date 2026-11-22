//! Domain model of proAVA.
//!
//! Pure Rust without I/O: no files, no UI, no network.
//! Everything that is true about a project lives here.

pub mod award;
pub mod bid;
pub mod bid_structure;
pub mod bids_or_order;
pub mod billing;
pub mod billing_check;
pub mod change_order;
pub mod change_order_work;
pub mod complement_text;
pub mod contract;
pub mod correspondence;
pub mod cost_demo;
pub mod cost_estimate;
pub mod cost_group;
pub mod cost_lv;
pub mod cost_points;
pub mod cost_split;
pub mod cost_sums;
pub mod cost_tracking;
pub mod edit_status;
pub mod error;
pub mod german_date;
pub mod input_limits;
pub mod invitation;
pub mod letters;
pub mod library;
pub mod lv;
pub mod lv_diff;
pub mod lv_purpose;
pub mod lv_search;
pub mod mask_spec;
pub mod money;
pub mod numbering;
pub mod oz_mask;
pub mod page_frame;
pub mod pdf_clip;
pub mod performance;
pub mod position;
pub mod price_check;
pub mod price_comparison;
pub mod project;
pub mod project_name;
pub mod quantity_formula;
pub mod rich_text;
pub mod row_kind;
pub mod stlb;
pub mod stlb_update;
pub mod surcharge;
pub mod template;
pub mod text_origin;
mod title_default;
pub mod transfer;
pub mod wording;
pub mod work_category;

pub use award::{PriceAdoption, PriceSource};
pub use bid::{Address, Award, Bid, BidComplement, BidId, BidPrice, Discount, QuantityChange};
pub use bid_structure::{
    align, check_pair, kind_label, normalized, position_kind_label, same_unit, text_similarity,
    Alignment, AssignedBy, Assignment, BidItem, DeviationLine, PairCheck, StoredSuggestion,
    StructureReport, Suggestion, TotalBreakdown,
};
pub use bids_or_order::{position_ids, BidsOrOrder, UncoveredPositions};
pub use billing::{
    Billing, CheckBasis, Correction, Invoice, InvoiceId, InvoiceItem, InvoiceKind, Mode, Progress,
    TakeoffLine, VatRule,
};
pub use billing_check::{
    BillingContext, EarlierRelease, Finding, InvoiceCheck, PositionCheck, Release, Severity,
};
pub use change_order::{
    billable_items, change_order_overview, pending_change_order, BillableItem, BillableSource,
    ChangeBasis, ChangeOrder, ChangeOrderId, ChangeOrderItem, ChangeOrderLine, ChangeOrderOverview,
    ChangeOrderStatus, ItemChange, StatusChange,
};
pub use change_order_work::{
    area_title, deviation_percent, price_hints, sums_by_status, AreaProposal, ChangeOrderOffer,
    OfferResult, PriceHint, StatusSum, COLLECTOR_TITLE,
};
pub use contract::{discount_percent, Contract, ContractItem};
pub use correspondence::{
    bidder_letter_entry, release_letter_entry, size_label, CorrespondenceEntry, CorrespondenceId,
    CorrespondenceKind, ARCHIVE_HINT_BYTES,
};
pub use cost_estimate::{
    cost_control, default_point_kinds, default_shares, ControlLine, CostControl, CostEstimate,
    CostEstimateId, CostResult, CostSection, CostShare, CostStage, KindSum, PointKind, PointRow,
    SectionResult, ShareLine, VIRTUAL_KEYS,
};
pub use cost_group::{
    cost_group_heirs_question, cost_group_label, cost_group_report, parse_cost_group, CostCatalog,
    CostGroupLine, CostGroupReport, DIN276_KG400,
};
pub use cost_lv::{
    calculation_from, cost_lv_findings, cost_lv_mask, cost_lv_note, cost_lv_template,
    din_group_label, din_level, kg400_groups, kg400_subgroups,
};
pub use cost_points::{
    new_point_section, point_section_as, point_section_findings, POINTS_GROUP, POINTS_TITLE,
};
pub use cost_split::{point_rows, split_findings, split_rows, POINTS_TEMPLATE_TITLE};
pub use cost_sums::{cost_group_at_level, CostSource, CostSourceRef};
pub use cost_tracking::{
    code_level, cost_tracking, group_label, offer_of, record_snapshot, source_key, stage_parts,
    BudgetKind, CostSnapshot, CostTracking, Deviation, LvTrack, Mark, OfferChoice, OfferSource,
    PreviousDeviation, SnapshotCell, StagePart, TrackDeviation, TrackOptions, TrackReport,
    TrackRow, TrackRowKind, TrackStage, TrackView, WithoutGroup,
};
pub use edit_status::{locked_message, EditStatus, StatusCounts, LOCKED_HINT};
pub use error::DomainError;
pub use german_date::{optional_date, optional_time, GermanDate};
pub use input_limits::NumberKind;
pub use invitation::{name_similarity, InviteStatus, InvitedBidder, InviteeId};
pub use letters::{
    default_recipients, letter_values, placeholders_in, release_letter_values, LetterKind,
    LetterTemplate, PLACEHOLDERS,
};
pub use lv::{
    normalize_ordinal, GroupTotal, Lv, LvGroup, LvRow, Note, PriceComponent, RowId, Spacer,
    DEFAULT_POSITION_STEP,
};
pub use lv_purpose::LvPurpose;
pub use mask_spec::{default_level_label, LevelSpec, MaskSpec};
pub use money::{format_decimal_de, parse_decimal_de, Money};
pub use numbering::{OrdinalChange, RenumberMode};
pub use oz_mask::OzMask;
pub use page_frame::{
    FrameLogoPlace, PageFrame, DEFAULT_PAGE_FOOTER, DEFAULT_PAGE_HEADER, FRAME_FONT_SIZES,
    PAGE_PLACEHOLDERS,
};
pub use pdf_clip::{is_pdf, paper_name, ClipRect, PdfClip, PT_PER_MM};
pub use performance::{
    is_performance_mark, performance_mark, performance_number_input, shown_performance_number,
    PerformanceDescription,
};
pub use position::{
    ComplementKind, Position, PositionKind, SubDescription, TextAnchor, TextComplement, PRINT_GAP,
    PRINT_GAP_DOTS,
};
pub use price_check::PriceCheck;
pub use price_comparison::{
    BidSummary, ComparisonLine, ComparisonOptions, DeviationReference, GroupLine, PositionLine,
    PriceCell, PriceComparison, PriceMark,
};
pub use project::{
    join_deadline, size_text, split_deadline, Contact, ContractSnapshot, ContractTerms, Deduction,
    ExecutionSite, ImageAsset, LvListSnapshot, OffersSnapshot, Place, Project, ProjectInfo,
    AWARD_PROCEDURES, CONTACT_ROLES,
};
pub use project_name::ProjectName;
pub use rich_text::{
    apply_format, font_class, is_stray_control, list_indent, list_marker, list_markers,
    list_numbering, normalize_runs, runs_text, strip_control_chars, Align, Cell, FontClass,
    RichText, Run, Table, TextBlock, DEFAULT_SIZE_PT, MAX_LIST_LEVEL,
};
pub use row_kind::{Conversion, RowKind};
pub use rust_decimal::Decimal;
pub use stlb::{CatalogKeys, ComplementNumber, StandardNumber, StlbCharacteristic, StlbReference};
pub use surcharge::{
    SplitBlock, Surcharge, SurchargeComputation, SurchargeKind, SurchargeReference, SurchargeValue,
};
pub use text_origin::TextOrigin;
pub use work_category::{
    parse_work_category, work_category_heirs_question, work_category_label, work_category_text,
    WorkCatalog, STLB_WORK_CATEGORIES, WORK_CATEGORY_TYPE,
};
