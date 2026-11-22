use rust_decimal::Decimal;

use crate::bid::{Address, Bid, BidId};
use crate::billing::Billing;
use crate::change_order::{ChangeOrder, ChangeOrderId};
use crate::contract::Contract;
use crate::german_date::{optional_date, optional_time, GermanDate};
use crate::invitation::{name_similarity, InviteStatus, InvitedBidder, InviteeId};
use crate::lv::{Lv, LvRow};
use crate::money::Money;
use crate::project_name::ProjectName;

/// Facts about the project that appear in headers, GAEB and print
/// (Projektstammdaten).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProjectInfo {
    /// Longer description (Projektbezeichnung), e.g. "Neubau Schulzentrum".
    pub label: String,
    /// Short name or project number of the client (GAEB NamePrj), e.g.
    /// "40651"; empty = the project name is used.
    pub short_name: String,
    /// What is built (Baumaßnahme / Kurzbeschreibung), several lines.
    pub description: String,
    /// Building owner (Bauherr): owner or sponsor of the construction.
    pub owner: Address,
    /// Contracting authority (Auftraggeber / Vergabestelle) when it is not
    /// the Bauherr; `None` = „wie Bauherr“. GAEB `OWN` and the printed
    /// „Auftraggeber“ use [`ProjectInfo::client`].
    pub own_client: Option<Address>,
    /// Planner who writes the LV (Aufsteller).
    pub planner: Address,
    /// Construction site (Baustelle / Bauort; GAEB CnstSite).
    pub site: Address,
    /// Kennnummer and Bezeichnung of the Baustelle (GAEB CnstSiteIDNo,
    /// CnstSiteName).
    pub site_number: String,
    pub site_name: String,
    /// Benachrichtigungsort of the Baustelle (GAEB NotifSite).
    pub notification_site: Option<Place>,
    /// Further Ausführungsorte of a GAEB file (CnstSite after the first).
    pub further_sites: Vec<ExecutionSite>,
    /// Bedarfsträger (GAEB Requester).
    pub requesters: Vec<Place>,
    /// Further addresses of the project (Architekt, Fachplaner, Bauleitung …).
    pub contacts: Vec<Contact>,
    /// Award number (Vergabenummer).
    pub award_number: String,
    /// Datenaustauschnummer of the Auftraggeber (GAEB OWN/DPNo).
    pub exchange_number: String,
    /// Debitorennummer of the Auftraggeber (GAEB OWN/AcctRecNo).
    pub client_account: String,
    /// State of the Auftraggeber: „EEA“ (EWR) or „Other“ (GAEB OWN/CntryType)
    /// and its Kfz-Kennzeichen (CntryName, e.g. „D“).
    pub client_country_kind: String,
    pub client_country_code: String,
    /// Kind of award procedure (Vergabeart), e.g. „Öffentliche Ausschreibung“.
    pub award_procedure: String,
    /// Deadline for bids, free text like "15.12.2026, 10:00 Uhr".
    pub submission_deadline: String,
    pub submission_place: String,
    /// End of the binding period (Zuschlags- und Bindefrist), TT.MM.JJJJ.
    pub binding_period_end: String,
    /// Execution period (Ausführungsfristen), TT.MM.JJJJ each.
    pub execution_start: String,
    pub execution_end: String,
    /// Further deadlines in words, e.g. „Fertigstellung ASP01 bis KW 12“.
    pub execution_note: String,
    /// Contract terms with retentions and deductions (Vertragsbedingungen).
    pub terms: ContractTerms,
    /// Own page header/footer of this project; None = the one from the
    /// settings (Extras ▸ Einstellungen ▸ Drucken).
    pub page_frame: Option<crate::page_frame::PageFrame>,
}

/// Roles offered for further addresses (frei änderbar).
pub const CONTACT_ROLES: [&str; 6] = [
    "Architekt",
    "TGA-Planer",
    "Elektroplaner",
    "Bauleitung",
    "Projektsteuerung",
    "Nutzer",
];

/// Award procedures offered as choice (Vergabeart); free text is allowed too.
pub const AWARD_PROCEDURES: [&str; 7] = [
    "Öffentliche Ausschreibung",
    "Beschränkte Ausschreibung mit Teilnahmewettbewerb",
    "Beschränkte Ausschreibung ohne Teilnahmewettbewerb",
    "Freihändige Vergabe",
    "Offenes Verfahren",
    "Nicht offenes Verfahren",
    "Verhandlungsverfahren",
];

impl ProjectInfo {
    /// The Auftraggeber: the own one, otherwise the Bauherr.
    pub fn client(&self) -> &Address {
        self.own_client.as_ref().unwrap_or(&self.owner)
    }

    /// Whether Auftraggeber and Bauherr are different parties (both are
    /// then printed separately).
    pub fn client_differs(&self) -> bool {
        self.own_client
            .as_ref()
            .is_some_and(|client| client != &self.owner)
    }

    /// Splits the stored deadline „15.12.2026, 10:00 Uhr“ into date and time text.
    pub fn deadline_parts(&self) -> (String, String) {
        split_deadline(&self.submission_deadline)
    }

    /// The date of the submission deadline, if it is one.
    pub fn submission_date(&self) -> Option<GermanDate> {
        GermanDate::parse(&self.deadline_parts().0).ok()
    }

    /// Errors that keep the master data from being taken over: dates that
    /// are none, an execution end before its start, wrong percentages.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let (deadline_date, deadline_time) = self.deadline_parts();
        let mut date = |label: &str, text: &str| match optional_date(text) {
            Ok(date) => date,
            Err(problem) => {
                problems.push(format!("{label}: {problem}"));
                None
            }
        };
        date("Abgabetermin", &deadline_date);
        date("Zuschlags- und Bindefrist", &self.binding_period_end);
        let start = date("Ausführungsbeginn", &self.execution_start);
        let end = date("Ausführungsende", &self.execution_end);
        if let Err(problem) = optional_time(&deadline_time) {
            problems.push(format!("Abgabetermin: {problem}"));
        }
        if let (Some(start), Some(end)) = (start, end) {
            if end < start {
                problems.push(format!(
                    "Das Ausführungsende ({end}) liegt vor dem Ausführungsbeginn ({start})."
                ));
            }
        }
        problems.extend(self.terms.problems());
        problems
    }

    /// [`ProjectInfo::problems`] as hints for data taken over as delivered
    /// (GAEB import): „… – wie aus der Datei übernommen“.
    pub fn problems_as_delivered(&self) -> Vec<String> {
        self.problems()
            .into_iter()
            .map(|problem| {
                format!(
                    "{} – wie aus der Datei übernommen (Projektangaben).",
                    problem.trim_end_matches('.')
                )
            })
            .collect()
    }

    /// Hints that do not block: binding period not after the deadline,
    /// execution starting before the deadline.
    pub fn warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        let deadline = self.submission_date();
        let binding = GermanDate::parse(&self.binding_period_end).ok();
        let start = GermanDate::parse(&self.execution_start).ok();
        if let (Some(deadline), Some(binding)) = (deadline, binding) {
            if binding <= deadline {
                warnings.push(format!(
                    "Die Bindefrist ({binding}) endet nicht nach dem Abgabetermin ({deadline})."
                ));
            }
        }
        if let (Some(deadline), Some(start)) = (deadline, start) {
            if start < deadline {
                warnings.push(format!(
                    "Der Ausführungsbeginn ({start}) liegt vor dem Abgabetermin ({deadline})."
                ));
            }
        }
        warnings
    }
}

/// „15.12.2026, 10:00 Uhr“ → („15.12.2026“, „10:00“); free text stays in the date part.
pub fn split_deadline(text: &str) -> (String, String) {
    let text = text.trim();
    if let Some((date, time)) = text.split_once(',') {
        let time = time.trim().trim_end_matches("Uhr").trim();
        if GermanDate::parse(date).is_ok() && optional_time(time).is_ok() {
            return (date.trim().to_string(), time.to_string());
        }
    }
    if let Some((date, time)) = text.split_once(' ') {
        let time = time.trim().trim_end_matches("Uhr").trim();
        if GermanDate::parse(date).is_ok() && !time.is_empty() && optional_time(time).is_ok() {
            return (date.trim().to_string(), time.to_string());
        }
    }
    (text.to_string(), String::new())
}

/// („15.12.2026“, „10:00“) → „15.12.2026, 10:00 Uhr“.
pub fn join_deadline(date: &str, time: &str) -> String {
    let (date, time) = (date.trim(), time.trim().trim_end_matches("Uhr").trim());
    match (date.is_empty(), time.is_empty()) {
        (_, true) => date.to_string(),
        (true, false) => format!("{time} Uhr"),
        (false, false) => format!("{date}, {time} Uhr"),
    }
}

/// A place of the award (GAEB Requester, CnstSite, NotifSite): address,
/// Kennnummer and Bezeichnung.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Place {
    pub address: Address,
    pub number: String,
    pub name: String,
}

impl Place {
    pub fn is_empty(&self) -> bool {
        self.address == Address::default()
            && self.number.trim().is_empty()
            && self.name.trim().is_empty()
    }

    /// „Bezeichnung (Nr. …), Anschrift“ – what is filled.
    pub fn details(&self) -> String {
        let mut pieces = Vec::new();
        if !self.name.trim().is_empty() {
            pieces.push(self.name.trim().to_string());
        }
        if !self.number.trim().is_empty() {
            pieces.push(format!("Nr. {}", self.number.trim()));
        }
        let address = self.address.details();
        if !address.is_empty() {
            pieces.push(address);
        }
        pieces.join(", ")
    }
}

/// An Ausführungsort with its Benachrichtigungsort (GAEB CnstSite, NotifSite).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExecutionSite {
    pub site: Place,
    pub notification: Option<Place>,
}

/// A further address of the project with its role.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Contact {
    /// Role, e.g. „Architekt“, „TGA-Planer“, „Bauleitung“.
    pub role: String,
    pub address: Address,
}

/// Retentions, cash discount and deductions agreed for the contract
/// (Sicherheitseinbehalte, Skonto, Umlagen/Abzüge).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ContractTerms {
    /// Sicherheit für die Vertragserfüllung in % der Auftragssumme.
    pub performance_security_percent: Option<Decimal>,
    /// Sicherheit für Mängelansprüche in % der Abrechnungssumme.
    pub warranty_security_percent: Option<Decimal>,
    /// Skonto in % …
    pub cash_discount_percent: Option<Decimal>,
    /// … bei Zahlung innerhalb von n Tagen.
    pub cash_discount_days: Option<u32>,
    /// Umlagen/Abzüge in % der Abrechnungssumme (Bauwesenversicherung,
    /// Baustrom, Bauwasser, Baureinigung …).
    pub deductions: Vec<Deduction>,
    /// Zahlungsbedingungen, Abschlagszahlungen … in Worten.
    pub payment_terms: String,
    /// Further agreements in words.
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Deduction {
    pub label: String,
    pub percent: Decimal,
}

impl ContractTerms {
    /// The deductions an AVA usually starts with, all at 0 %.
    pub fn common_deductions() -> Vec<Deduction> {
        [
            "Bauwesenversicherung",
            "Baustrom",
            "Bauwasser",
            "Baureinigung",
        ]
        .into_iter()
        .map(|label| Deduction {
            label: label.to_string(),
            percent: Decimal::ZERO,
        })
        .collect()
    }

    /// Sum of all deductions in percent.
    pub fn deduction_percent(&self) -> Decimal {
        self.deductions
            .iter()
            .map(|deduction| deduction.percent)
            .sum()
    }

    /// Amount of every deduction for a net sum, rounded to cents.
    pub fn deduction_amounts(&self, net: Money) -> Vec<(String, Money)> {
        self.deductions
            .iter()
            .filter(|deduction| !deduction.percent.is_zero())
            .map(|deduction| (deduction.label.clone(), net.percent(deduction.percent)))
            .collect()
    }

    /// Plain problems for the user: percentages outside 0–100, empty labels,
    /// discount without days and the other way round.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let hundred = Decimal::ONE_HUNDRED;
        let check = |problems: &mut Vec<String>, value: Option<Decimal>, what: &str| {
            if let Some(value) = value {
                if value.is_sign_negative() || value > hundred {
                    problems.push(format!("{what}: {value} % liegt nicht zwischen 0 und 100."));
                }
            }
        };
        check(
            &mut problems,
            self.performance_security_percent,
            "Sicherheit Vertragserfüllung",
        );
        check(
            &mut problems,
            self.warranty_security_percent,
            "Sicherheit Mängelansprüche",
        );
        check(&mut problems, self.cash_discount_percent, "Skonto");
        match (self.cash_discount_percent, self.cash_discount_days) {
            (Some(percent), None) if !percent.is_zero() => {
                problems.push("Skonto ohne Zahlungsfrist in Tagen.".to_string())
            }
            (None, Some(_)) => problems.push("Skontofrist ohne Skontosatz.".to_string()),
            _ => {}
        }
        for deduction in &self.deductions {
            if deduction.label.trim().is_empty() {
                problems.push("Ein Abzug hat keine Bezeichnung.".to_string());
            }
            check(&mut problems, Some(deduction.percent), &deduction.label);
        }
        problems
    }
}

/// An image used in long texts; stored inside the project file.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageAsset {
    /// File name inside the project, unique, e.g. "kellerfenster.jpg".
    pub file: String,
    pub bytes: Vec<u8>,
    /// Ausschnitt aus einer PDF-Seite: `bytes` are the PDF (see
    /// [`crate::PdfClip`]); `None` = a picture (PNG/JPEG).
    pub clip: Option<crate::PdfClip>,
}

impl ImageAsset {
    /// Same picture or same PDF part (preview choices of a clip do not count).
    pub fn same_content(&self, other: &ImageAsset) -> bool {
        self.bytes == other.bytes
            && match (&self.clip, &other.clip) {
                (None, None) => true,
                (Some(a), Some(b)) => a.same_part(b),
                _ => false,
            }
    }

    pub fn is_pdf_clip(&self) -> bool {
        self.clip.is_some()
    }
}

/// One bill of quantities of a project with the bids received for it.
#[derive(Debug, Clone)]
struct LvEntry {
    /// Identity while the program runs (not stored): survives moving and
    /// is used by undo to find an LV again.
    key: u64,
    lv: Lv,
    bids: Vec<Bid>,
    next_bid_id: u64,
    /// Bieterverzeichnis: who was asked for a bid on this LV.
    invitees: Vec<InvitedBidder>,
    next_invitee_id: u64,
    /// Auftrag (snapshot of the awarded bid), Nachträge and Abrechnung.
    contract: Option<Contract>,
    change_orders: Vec<ChangeOrder>,
    next_change_order_id: u64,
    billing: Billing,
    /// Kostenverfolgung: the bid that counts as „Angebot“ before the award,
    /// with its reason (none = the lowest bid).
    offer_choice: Option<crate::cost_tracking::OfferChoice>,
}

/// Equal content, whatever the session key.
impl PartialEq for LvEntry {
    fn eq(&self, other: &Self) -> bool {
        self.lv == other.lv
            && self.bids == other.bids
            && self.next_bid_id == other.next_bid_id
            && self.invitees == other.invitees
            && self.contract == other.contract
            && self.change_orders == other.change_orders
            && self.billing == other.billing
            && self.offer_choice == other.offer_choice
    }
}

static NEXT_LV_KEY: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

static NEXT_REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// The LV list of a project with a revision stamp: every mutable access
/// draws a new, program-wide unique number (see [`Project::revision`]), so
/// no change can pass by the screens that keep computed results.
#[derive(Clone)]
struct Tracked<T> {
    value: T,
    revision: u64,
}

impl<T> Tracked<T> {
    fn new(value: T) -> Self {
        Self {
            value,
            revision: NEXT_REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        }
    }
}

impl<T> std::ops::Deref for Tracked<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> std::ops::DerefMut for Tracked<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.revision = NEXT_REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        &mut self.value
    }
}

/// Content only; the stamp is not part of it.
impl<T: PartialEq> PartialEq for Tracked<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Tracked<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(formatter)
    }
}

impl LvEntry {
    fn new(lv: Lv) -> Self {
        Self {
            key: NEXT_LV_KEY.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            lv,
            bids: Vec::new(),
            next_bid_id: 1,
            invitees: Vec::new(),
            next_invitee_id: 1,
            contract: None,
            change_orders: Vec::new(),
            next_change_order_id: 1,
            billing: Billing::default(),
            offer_choice: None,
        }
    }
}

/// Auftrag, Nachträge and Abrechnung of one LV at one moment (undo).
#[derive(Debug, Clone, PartialEq)]
pub struct ContractSnapshot {
    contract: Option<Contract>,
    change_orders: Vec<ChangeOrder>,
    next_change_order_id: u64,
    billing: Billing,
    /// The LV itself, for steps that also add or edit positions (Nachtrag).
    lv: Option<Lv>,
}

impl ContractSnapshot {
    /// Number of change orders and invoices: a change keeps the shape when
    /// it only edits fields (then typing may join one undo step).
    pub fn shape(&self) -> (usize, usize, bool) {
        (
            self.change_orders.len(),
            self.billing.invoices().len(),
            self.contract.is_some(),
        )
    }
}

/// Bids and invited bidders of one LV at one moment (undo).
#[derive(Debug, Clone, PartialEq)]
pub struct OffersSnapshot {
    bids: Vec<Bid>,
    next_bid_id: u64,
    invitees: Vec<InvitedBidder>,
    next_invitee_id: u64,
    offer_choice: Option<crate::cost_tracking::OfferChoice>,
}

impl OffersSnapshot {
    /// Number of bids and invited bidders: a change keeps the shape when it
    /// only edits fields (then typing may join one undo step).
    pub fn shape(&self) -> (usize, usize) {
        (self.bids.len(), self.invitees.len())
    }
}

/// The LV list of a project at one moment (for undo of LV-list changes).
#[derive(Debug, Clone)]
pub struct LvListSnapshot {
    entries: Vec<LvEntry>,
    active: usize,
    images: Vec<ImageAsset>,
}

impl LvListSnapshot {
    /// Session keys of the LVs in their order.
    pub fn keys(&self) -> Vec<u64> {
        self.entries.iter().map(|entry| entry.key).collect()
    }
}

/// A project: the unit the user opens, edits, backs up and hands over.
/// It holds one or more LVs (Leistungsverzeichnisse, e.g. Lose or trades);
/// `lv()`, `bids()` and friends work on the active one.
#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    name: ProjectName,
    created_iso: String,
    info: ProjectInfo,
    lvs: Tracked<Vec<LvEntry>>,
    active: usize,
    images: Vec<ImageAsset>,
    /// Letter templates changed by the user (Schriftverkehr); standard ones are not stored.
    letters: Vec<crate::letters::LetterTemplate>,
    /// Filed letters (Schriftverkehr-Archiv), see [`crate::correspondence`].
    correspondence: Vec<crate::correspondence::CorrespondenceEntry>,
    /// Kostenschätzungen und -berechnungen, see [`crate::cost_estimate`].
    cost_estimates: Vec<crate::cost_estimate::CostEstimate>,
    /// Kostenverfolgung: budget, notes, recorded states, see
    /// [`crate::cost_tracking`].
    cost_tracking: crate::cost_tracking::CostTracking,
}

impl Project {
    pub fn new(name: ProjectName, created_iso: impl Into<String>, lv: Lv) -> Self {
        Self {
            name,
            created_iso: created_iso.into(),
            info: ProjectInfo::default(),
            lvs: Tracked::new(vec![LvEntry::new(lv)]),
            active: 0,
            images: Vec::new(),
            letters: Vec::new(),
            correspondence: Vec::new(),
            cost_estimates: Vec::new(),
            cost_tracking: crate::cost_tracking::CostTracking::default(),
        }
    }

    /// Stamp of the LVs with their bids and invitees: a new number after
    /// every change (also after a mutable access that changed nothing),
    /// unique across all projects of the program run. Together with
    /// [`Project::active_lv`] it tells whether results computed from
    /// `lv()`/`bids()` are still valid.
    pub fn revision(&self) -> u64 {
        self.lvs.revision
    }

    /// The same project under another name (Kopie anlegen).
    pub fn with_name(mut self, name: ProjectName) -> Self {
        self.name = name;
        self
    }

    pub fn name(&self) -> &ProjectName {
        &self.name
    }

    pub fn created_iso(&self) -> &str {
        &self.created_iso
    }

    pub fn info(&self) -> &ProjectInfo {
        &self.info
    }

    pub fn info_mut(&mut self) -> &mut ProjectInfo {
        &mut self.info
    }

    /// The active LV.
    pub fn lv(&self) -> &Lv {
        &self.lvs[self.active].lv
    }

    pub fn lv_mut(&mut self) -> &mut Lv {
        &mut self.lvs[self.active].lv
    }

    pub fn lv_count(&self) -> usize {
        self.lvs.len()
    }

    /// All LVs in their order.
    pub fn lvs(&self) -> impl Iterator<Item = &Lv> {
        self.lvs.iter().map(|entry| &entry.lv)
    }

    pub fn lv_at(&self, index: usize) -> Option<&Lv> {
        self.lvs.get(index).map(|entry| &entry.lv)
    }

    /// Bids received for the LV at `index`.
    pub fn bids_of(&self, index: usize) -> &[Bid] {
        self.lvs
            .get(index)
            .map(|entry| entry.bids.as_slice())
            .unwrap_or(&[])
    }

    pub fn active_lv(&self) -> usize {
        self.active
    }

    /// Identity of the LV at `index` while the program runs (not stored).
    pub fn lv_key(&self, index: usize) -> Option<u64> {
        self.lvs.get(index).map(|entry| entry.key)
    }

    /// Index of the LV with this session key.
    pub fn lv_index_of_key(&self, key: u64) -> Option<usize> {
        self.lvs.iter().position(|entry| entry.key == key)
    }

    /// The LV at `index` for editing (LV properties of a not active LV).
    pub fn lv_at_mut(&mut self, index: usize) -> Option<&mut Lv> {
        self.lvs.get_mut(index).map(|entry| &mut entry.lv)
    }

    /// Bids and Bieterverzeichnis of the LV at `index`, for undo of changes
    /// to bids (Nachlass, Ausschluss, Auftrag, Namen) and invited bidders.
    pub fn offers_snapshot(&self, index: usize) -> Option<OffersSnapshot> {
        let entry = self.lvs.get(index)?;
        Some(OffersSnapshot {
            bids: entry.bids.clone(),
            next_bid_id: entry.next_bid_id,
            invitees: entry.invitees.clone(),
            next_invitee_id: entry.next_invitee_id,
            offer_choice: entry.offer_choice.clone(),
        })
    }

    /// Auftrag, Nachträge and Abrechnung of the LV at `index`, for undo.
    pub fn contract_snapshot(&self, index: usize) -> Option<ContractSnapshot> {
        let entry = self.lvs.get(index)?;
        Some(ContractSnapshot {
            contract: entry.contract.clone(),
            change_orders: entry.change_orders.clone(),
            next_change_order_id: entry.next_change_order_id,
            billing: entry.billing.clone(),
            lv: None,
        })
    }

    /// Like [`Project::contract_snapshot`], with the LV: for steps that also
    /// add or edit positions (Nachtragspositionen).
    pub fn contract_snapshot_with_lv(&self, index: usize) -> Option<ContractSnapshot> {
        let mut snapshot = self.contract_snapshot(index)?;
        snapshot.lv = Some(self.lvs[index].lv.clone());
        Some(snapshot)
    }

    /// Puts Auftrag, Nachträge and Abrechnung of an earlier moment back into
    /// the LV at `index`; returns what was there (for redo).
    pub fn restore_contract(
        &mut self,
        index: usize,
        snapshot: ContractSnapshot,
    ) -> Option<ContractSnapshot> {
        let before = if snapshot.lv.is_some() {
            self.contract_snapshot_with_lv(index)?
        } else {
            self.contract_snapshot(index)?
        };
        let entry = &mut self.lvs[index];
        if let Some(lv) = snapshot.lv {
            entry.lv = lv;
        }
        entry.contract = snapshot.contract;
        entry.change_orders = snapshot.change_orders;
        entry.next_change_order_id = entry
            .next_change_order_id
            .max(snapshot.next_change_order_id);
        let next_invoice = entry.billing.next_invoice_id();
        entry.billing = snapshot.billing;
        entry.billing.reserve_invoice_ids(next_invoice);
        Some(before)
    }

    /// Puts bids and invited bidders of an earlier moment back into the LV at
    /// `index`; returns what was there (for redo).
    pub fn restore_offers(
        &mut self,
        index: usize,
        snapshot: OffersSnapshot,
    ) -> Option<OffersSnapshot> {
        let before = self.offers_snapshot(index)?;
        let entry = &mut self.lvs[index];
        entry.bids = snapshot.bids;
        entry.next_bid_id = entry.next_bid_id.max(snapshot.next_bid_id);
        entry.invitees = snapshot.invitees;
        entry.next_invitee_id = entry.next_invitee_id.max(snapshot.next_invitee_id);
        entry.offer_choice = snapshot.offer_choice;
        Some(before)
    }

    /// Kostenverfolgung: the bid chosen as „Angebot“ of the LV at `index`
    /// (before the award; none = the lowest bid).
    pub fn offer_choice_of(&self, index: usize) -> Option<&crate::cost_tracking::OfferChoice> {
        self.lvs.get(index)?.offer_choice.as_ref()
    }

    /// Sets or clears the chosen bid of the LV at `index`; false when there
    /// is no such LV or bid.
    pub fn set_offer_choice_of(
        &mut self,
        index: usize,
        choice: Option<crate::cost_tracking::OfferChoice>,
    ) -> bool {
        let Some(entry) = self.lvs.get(index) else {
            return false;
        };
        if let Some(choice) = &choice {
            if !entry.bids.iter().any(|bid| bid.id() == choice.bid) {
                return false;
            }
        }
        self.lvs[index].offer_choice = choice;
        true
    }

    /// The LV list as it is now, for undo.
    pub fn lv_list_snapshot(&self) -> LvListSnapshot {
        LvListSnapshot {
            entries: self.lvs.value.clone(),
            active: self.active,
            images: self.images.clone(),
        }
    }

    /// Back to an earlier LV list (undo). LVs that still exist keep their
    /// current bids – bids are read in later and are not part of undo –,
    /// images that were dropped come back. Returns the snapshot of the list
    /// before (for redo).
    pub fn restore_lv_list(&mut self, snapshot: LvListSnapshot) -> LvListSnapshot {
        let before = self.lv_list_snapshot();
        let mut entries = snapshot.entries;
        for entry in &mut entries {
            if let Some(current) = self.lvs.iter().find(|current| current.key == entry.key) {
                entry.bids = current.bids.clone();
                entry.next_bid_id = current.next_bid_id;
                entry.invitees = current.invitees.clone();
                entry.next_invitee_id = current.next_invitee_id;
            }
        }
        if entries.is_empty() {
            return before;
        }
        self.active = snapshot.active.min(entries.len() - 1);
        *self.lvs = entries;
        for image in snapshot.images {
            if self.image(&image.file).is_none() {
                self.images.push(image);
            }
        }
        before
    }

    /// Takes over the session keys of `from` for the LVs at the same place
    /// (a state of the same project read back from a file, e.g. after an
    /// assistant changed it): undo steps, views and answers to questions of
    /// these LVs stay theirs. LVs beyond keep their own keys.
    pub fn adopt_session_keys(&mut self, from: &Project) {
        for (entry, own) in self.lvs.value.iter_mut().zip(from.lvs.iter()) {
            entry.key = own.key;
        }
    }

    /// Puts in `other` as the whole state of the project and returns the
    /// state before – for one undo step around a change an assistant made
    /// on a copy (Fenster-Brücke). The active LV stays (as far as it
    /// exists). Images only add up: the project keeps every image of both
    /// states (unused ones go when saving), so the state returned carries
    /// none – undo steps stay small.
    pub fn swap_state(&mut self, mut other: Project) -> Project {
        other.active = self.active.min(other.lvs.len().saturating_sub(1));
        let mut images = std::mem::take(&mut other.images);
        for image in std::mem::take(&mut self.images) {
            if !images.iter().any(|known| known.file == image.file) {
                images.push(image);
            }
        }
        let before = std::mem::replace(self, other);
        self.images = images;
        // A new stamp: results computed for any earlier state stay apart.
        let _ = &mut *self.lvs;
        before
    }

    /// Same content, whatever LV is shown, whatever the session keys and
    /// the order of the images.
    pub fn same_content(&self, other: &Project) -> bool {
        self.name == other.name
            && self.created_iso == other.created_iso
            && self.info == other.info
            && self.lvs == other.lvs
            && self.images.len() == other.images.len()
            && self.images.iter().all(|image| other.images.contains(image))
            && self.letters == other.letters
            && self.correspondence == other.correspondence
            && self.cost_estimates == other.cost_estimates
            && self.cost_tracking == other.cost_tracking
    }

    /// Whether the LV at `index` with its bids, Auftrag, Nachträge and
    /// Abrechnung equals the one at `other_index` of `other`.
    pub fn same_lv_entry(&self, index: usize, other: &Project, other_index: usize) -> bool {
        match (self.lvs.get(index), other.lvs.get(other_index)) {
            (Some(own), Some(theirs)) => own == theirs,
            (None, None) => true,
            _ => false,
        }
    }

    /// Makes another LV the active one; false when there is none at `index`.
    pub fn set_active_lv(&mut self, index: usize) -> bool {
        if index < self.lvs.len() {
            self.active = index;
            true
        } else {
            false
        }
    }

    /// Adds an LV after the others and returns its index (it does not become active).
    pub fn add_lv(&mut self, lv: Lv) -> usize {
        self.lvs.push(LvEntry::new(lv));
        self.lvs.len() - 1
    }

    /// Removes an LV with its bids. The last LV of a project stays.
    pub fn remove_lv(&mut self, index: usize) -> Result<(Lv, Vec<Bid>), String> {
        if self.lvs.len() <= 1 {
            return Err("Ein Projekt braucht mindestens ein Leistungsverzeichnis.".to_string());
        }
        if index >= self.lvs.len() {
            return Err("Dieses Leistungsverzeichnis gibt es nicht.".to_string());
        }
        let entry = self.lvs.remove(index);
        if self.active > index || self.active >= self.lvs.len() {
            self.active = self.active.saturating_sub(1);
        }
        self.remove_unused_images();
        Ok((entry.lv, entry.bids))
    }

    /// Moves an LV one place up or down; returns its new index.
    pub fn move_lv(&mut self, index: usize, up: bool) -> Option<usize> {
        let target = if up { index.checked_sub(1)? } else { index + 1 };
        if index >= self.lvs.len() || target >= self.lvs.len() {
            return None;
        }
        self.lvs.swap(index, target);
        if self.active == index {
            self.active = target;
        } else if self.active == target {
            self.active = index;
        }
        Some(target)
    }

    /// A copy of the LV at `index` without bids, added at the end.
    pub fn duplicate_lv(&mut self, index: usize) -> Option<usize> {
        let mut copy = self.lvs.get(index)?.lv.clone();
        copy.set_title(format!("{} (Kopie)", copy.title()));
        // The copy did not go out to bidders: no question before renumbering.
        copy.clear_sent();
        Some(self.add_lv(copy))
    }

    /// LV numbers used twice in the project (an LV number should be unique).
    pub fn duplicate_lv_numbers(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut twice = Vec::new();
        for lv in self.lvs() {
            let number = lv.number().trim();
            if !number.is_empty()
                && !seen.insert(number.to_string())
                && !twice.contains(&number.to_string())
            {
                twice.push(number.to_string());
            }
        }
        twice
    }

    /// Letter templates the user changed (see [`Project::letter_template`]).
    pub fn letter_templates(&self) -> &[crate::letters::LetterTemplate] {
        &self.letters
    }

    pub(crate) fn letter_templates_mut(&mut self) -> &mut Vec<crate::letters::LetterTemplate> {
        &mut self.letters
    }

    pub fn images(&self) -> &[ImageAsset] {
        &self.images
    }

    pub(crate) fn correspondence_list(&self) -> &[crate::correspondence::CorrespondenceEntry] {
        &self.correspondence
    }

    pub(crate) fn correspondence_list_mut(
        &mut self,
    ) -> &mut Vec<crate::correspondence::CorrespondenceEntry> {
        &mut self.correspondence
    }

    pub(crate) fn cost_estimate_list(&self) -> &[crate::cost_estimate::CostEstimate] {
        &self.cost_estimates
    }

    pub(crate) fn cost_estimate_list_mut(
        &mut self,
    ) -> &mut Vec<crate::cost_estimate::CostEstimate> {
        &mut self.cost_estimates
    }

    /// Kostenverfolgung: budget, notes per KG, recorded states.
    pub fn cost_tracking(&self) -> &crate::cost_tracking::CostTracking {
        &self.cost_tracking
    }

    pub fn cost_tracking_mut(&mut self) -> &mut crate::cost_tracking::CostTracking {
        &mut self.cost_tracking
    }

    /// Replaces the Kostenverfolgung data (undo); returns the old one.
    pub fn replace_cost_tracking(
        &mut self,
        tracking: crate::cost_tracking::CostTracking,
    ) -> crate::cost_tracking::CostTracking {
        std::mem::replace(&mut self.cost_tracking, tracking)
    }

    pub fn image(&self, file: &str) -> Option<&ImageAsset> {
        self.images.iter().find(|image| image.file == file)
    }

    /// Adds an image under a unique file name (suffix `_2`, `_3` … if taken)
    /// and returns the name it got.
    pub fn add_image(&mut self, wanted_name: &str, bytes: Vec<u8>) -> String {
        self.add_asset(wanted_name, bytes, None)
    }

    /// Adds an Ausschnitt aus einer PDF-Seite (`pdf` = the PDF file) under a
    /// unique file name and returns the name it got.
    pub fn add_pdf_clip(
        &mut self,
        wanted_name: &str,
        pdf: Vec<u8>,
        clip: crate::PdfClip,
    ) -> String {
        self.add_asset(wanted_name, pdf, Some(clip))
    }

    /// A copy of `asset` from elsewhere (other project, clipboard) under a
    /// unique name; picture or PDF clip.
    pub fn add_asset_copy(&mut self, asset: &ImageAsset) -> String {
        self.add_asset(&asset.file, asset.bytes.clone(), asset.clip.clone())
    }

    fn add_asset(
        &mut self,
        wanted_name: &str,
        bytes: Vec<u8>,
        clip: Option<crate::PdfClip>,
    ) -> String {
        let cleaned = clean_file_name(wanted_name);
        let (stem, extension) = match cleaned.rsplit_once('.') {
            Some((stem, extension)) => (stem.to_string(), format!(".{extension}")),
            None => (cleaned.clone(), String::new()),
        };
        let mut name = cleaned;
        let mut counter = 2;
        while self.image(&name).is_some() {
            name = format!("{stem}_{counter}{extension}");
            counter += 1;
        }
        self.images.push(ImageAsset {
            file: name.clone(),
            bytes,
            clip,
        });
        name
    }

    /// Preview choices of a PDF clip (frozen with its picture, hidden);
    /// false when `file` is no clip of this project or nothing changed.
    pub fn set_clip_preview(
        &mut self,
        file: &str,
        frozen: bool,
        hidden: bool,
        frozen_png: Option<std::sync::Arc<Vec<u8>>>,
    ) -> bool {
        let Some(clip) = self
            .images
            .iter_mut()
            .find(|image| image.file == file)
            .and_then(|image| image.clip.as_mut())
        else {
            return false;
        };
        // Frozen needs a picture; without one the preview stays live.
        let frozen_png = if frozen {
            frozen_png.or_else(|| clip.frozen_png.clone())
        } else {
            None
        };
        let frozen = frozen && frozen_png.is_some();
        if clip.preview_frozen == frozen
            && clip.preview_hidden == hidden
            && clip.frozen_png == frozen_png
        {
            return false;
        }
        clip.preview_frozen = frozen;
        clip.preview_hidden = hidden;
        clip.frozen_png = frozen_png;
        true
    }

    /// Images in the long texts of the active LV (positions, hints,
    /// Ausführungsbeschreibungen): how many and their size in bytes – what a
    /// GAEB DA XML export with images carries (Base64 makes it about a third
    /// more in the file). PDF clips count, but not with their PDF: GAEB gets
    /// a PNG of them whose size only the export adapter knows.
    pub fn lv_images(&self) -> (usize, usize) {
        let mut count = 0;
        let mut bytes = 0;
        for (image, _) in self.lv_image_uses() {
            count += 1;
            bytes += image
                .filter(|image| image.clip.is_none())
                .map_or(0, |image| image.bytes.len());
        }
        (count, bytes)
    }

    /// Every image block in the long texts of the active LV: the asset (None
    /// when missing) and the width in the text (mm).
    pub fn lv_image_uses(&self) -> Vec<(Option<&ImageAsset>, Option<f32>)> {
        let mut uses = Vec::new();
        for row in self.lv().rows() {
            let text = match row {
                LvRow::Position(position) => position.long_text(),
                LvRow::Note(note) => note.text(),
                LvRow::Performance(performance) => performance.long_text(),
                LvRow::Group(_) | LvRow::Spacer(_) => continue,
            };
            for block in text.blocks() {
                if let crate::TextBlock::Image { file, width_mm } = block {
                    uses.push((self.image(file), *width_mm));
                }
            }
        }
        uses
    }

    /// Drops images no long text of any LV and no letter template uses any more.
    pub fn remove_unused_images(&mut self) {
        let letters: Vec<String> = self
            .letters
            .iter()
            .flat_map(|template| {
                template
                    .body
                    .images()
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .collect();
        let used: std::collections::HashSet<String> = self
            .lvs
            .iter()
            .flat_map(|entry| entry.lv.rows())
            .flat_map(|row| match row {
                crate::LvRow::Position(position) => position
                    .long_text()
                    .images()
                    .map(str::to_string)
                    .collect::<Vec<_>>(),
                crate::LvRow::Note(note) => note.text().images().map(str::to_string).collect(),
                crate::LvRow::Performance(performance) => performance
                    .long_text()
                    .images()
                    .map(str::to_string)
                    .collect(),
                crate::LvRow::Group(_) | crate::LvRow::Spacer(_) => Vec::new(),
            })
            .chain(letters)
            .collect();
        self.images.retain(|image| used.contains(&image.file));
    }

    /// Bids for the active LV.
    pub fn bids(&self) -> &[Bid] {
        &self.lvs[self.active].bids
    }

    pub fn bid_mut(&mut self, id: BidId) -> Option<&mut Bid> {
        self.lvs[self.active]
            .bids
            .iter_mut()
            .find(|bid| bid.id() == id)
    }

    /// Adds a bid to the active LV; the current LV quantities become its
    /// Angebotsmengen (see [`Bid::take_offered_quantities`]).
    pub fn add_bid(&mut self, mut bid: Bid) -> BidId {
        bid.take_offered_quantities(self.lv());
        let index = self.active;
        self.add_bid_to(index, bid)
    }

    /// Adds a bid to the LV at `index` (used when loading).
    pub fn add_bid_to(&mut self, index: usize, mut bid: Bid) -> BidId {
        let last = self.lvs.len() - 1;
        let entry = &mut self.lvs[index.min(last)];
        let id = if bid.id() == BidId::default()
            || entry.bids.iter().any(|other| other.id() == bid.id())
        {
            let id = BidId(entry.next_bid_id);
            entry.next_bid_id += 1;
            id
        } else {
            entry.next_bid_id = entry.next_bid_id.max(bid.id().0 + 1);
            bid.id()
        };
        bid.set_id(id);
        entry.bids.push(bid);
        id
    }

    /// Id the next new bid of the active LV will get.
    pub fn next_bid_id(&self) -> BidId {
        BidId(self.lvs[self.active].next_bid_id)
    }

    pub fn next_bid_id_of(&self, index: usize) -> BidId {
        BidId(
            self.lvs
                .get(index)
                .map(|entry| entry.next_bid_id)
                .unwrap_or(1),
        )
    }

    /// Makes sure new bids of the active LV get ids from `next` on.
    pub fn reserve_bid_ids(&mut self, next: BidId) {
        let index = self.active;
        self.reserve_bid_ids_of(index, next);
    }

    pub fn reserve_bid_ids_of(&mut self, index: usize, next: BidId) {
        if let Some(entry) = self.lvs.get_mut(index) {
            entry.next_bid_id = entry.next_bid_id.max(next.0);
        }
    }

    pub fn remove_bid(&mut self, id: BidId) -> Option<Bid> {
        let entry = &mut self.lvs[self.active];
        let index = entry.bids.iter().position(|bid| bid.id() == id)?;
        for invitee in &mut entry.invitees {
            if invitee.bid == Some(id) {
                invitee.bid = None;
                if invitee.status == InviteStatus::Submitted {
                    invitee.status = InviteStatus::Requested;
                }
            }
        }
        Some(entry.bids.remove(index))
    }

    // ----- Bieterverzeichnis (je LV) -----

    /// Invited bidders of the active LV.
    /// Auftrag of the active LV (snapshot taken over from the awarded bid).
    pub fn contract(&self) -> Option<&Contract> {
        self.contract_of(self.active)
    }

    pub fn contract_of(&self, index: usize) -> Option<&Contract> {
        self.lvs
            .get(index)
            .and_then(|entry| entry.contract.as_ref())
    }

    pub fn contract_mut(&mut self) -> Option<&mut Contract> {
        self.lvs[self.active].contract.as_mut()
    }

    pub fn set_contract(&mut self, contract: Option<Contract>) {
        self.set_contract_of(self.active, contract);
    }

    pub fn set_contract_of(&mut self, index: usize, contract: Option<Contract>) {
        if let Some(entry) = self.lvs.get_mut(index) {
            entry.contract = contract;
        }
    }

    /// Nachträge of the active LV.
    pub fn change_orders(&self) -> &[ChangeOrder] {
        self.change_orders_of(self.active)
    }

    pub fn change_orders_of(&self, index: usize) -> &[ChangeOrder] {
        self.lvs
            .get(index)
            .map(|entry| entry.change_orders.as_slice())
            .unwrap_or(&[])
    }

    pub fn change_order_mut(&mut self, id: ChangeOrderId) -> Option<&mut ChangeOrder> {
        self.lvs[self.active]
            .change_orders
            .iter_mut()
            .find(|order| order.id == id)
    }

    /// Adds a change order to the active LV. A fresh id is handed out unless
    /// the order brings a free one (project file); number 0 becomes the next
    /// free number.
    pub fn add_change_order(&mut self, order: ChangeOrder) -> ChangeOrderId {
        self.add_change_order_to(self.active, order)
    }

    pub fn add_change_order_to(&mut self, index: usize, mut order: ChangeOrder) -> ChangeOrderId {
        let entry = &mut self.lvs[index];
        if order.id.0 == 0 || entry.change_orders.iter().any(|other| other.id == order.id) {
            order.id = ChangeOrderId(entry.next_change_order_id.max(1));
        }
        entry.next_change_order_id = entry.next_change_order_id.max(order.id.0 + 1);
        if order.number == 0 {
            order.number = entry
                .change_orders
                .iter()
                .map(|other| other.number)
                .max()
                .unwrap_or(0)
                + 1;
        }
        let id = order.id;
        entry.change_orders.push(order);
        id
    }

    pub fn remove_change_order(&mut self, id: ChangeOrderId) -> Option<ChangeOrder> {
        let orders = &mut self.lvs[self.active].change_orders;
        let index = orders.iter().position(|order| order.id == id)?;
        Some(orders.remove(index))
    }

    /// A change order of the active LV.
    pub fn change_order(&self, id: ChangeOrderId) -> Option<&ChangeOrder> {
        self.change_orders().iter().find(|order| order.id == id)
    }

    /// Abrechnung of the active LV.
    pub fn billing(&self) -> &Billing {
        &self.lvs[self.active].billing
    }

    pub fn billing_of(&self, index: usize) -> Option<&Billing> {
        self.lvs.get(index).map(|entry| &entry.billing)
    }

    pub fn billing_mut(&mut self) -> &mut Billing {
        &mut self.lvs[self.active].billing
    }

    pub fn billing_of_mut(&mut self, index: usize) -> Option<&mut Billing> {
        self.lvs.get_mut(index).map(|entry| &mut entry.billing)
    }

    /// Context for checking invoices of the active LV; None without contract.
    pub fn billing_context(&self) -> Option<crate::billing_check::BillingContext<'_>> {
        Some(crate::billing_check::BillingContext {
            contract: self.contract()?,
            change_orders: self.change_orders(),
            lv: self.lv(),
        })
    }

    pub fn invitees(&self) -> &[InvitedBidder] {
        &self.lvs[self.active].invitees
    }

    pub fn invitees_of(&self, index: usize) -> &[InvitedBidder] {
        self.lvs
            .get(index)
            .map(|entry| entry.invitees.as_slice())
            .unwrap_or(&[])
    }

    pub fn invitee_mut(&mut self, id: InviteeId) -> Option<&mut InvitedBidder> {
        self.lvs[self.active]
            .invitees
            .iter_mut()
            .find(|invitee| invitee.id == id)
    }

    /// Adds an invited bidder to the active LV; keeps a given id if free.
    pub fn add_invitee(&mut self, invitee: InvitedBidder) -> InviteeId {
        let index = self.active;
        self.add_invitee_to(index, invitee)
    }

    /// Adds an invited bidder to the LV at `index` (used when loading).
    pub fn add_invitee_to(&mut self, index: usize, mut invitee: InvitedBidder) -> InviteeId {
        let last = self.lvs.len() - 1;
        let entry = &mut self.lvs[index.min(last)];
        let taken = entry.invitees.iter().any(|other| other.id == invitee.id);
        if invitee.id == InviteeId::default() || taken {
            invitee.id = InviteeId(entry.next_invitee_id);
        }
        entry.next_invitee_id = entry.next_invitee_id.max(invitee.id.0 + 1);
        let id = invitee.id;
        entry.invitees.push(invitee);
        id
    }

    pub fn remove_invitee(&mut self, id: InviteeId) -> Option<InvitedBidder> {
        let invitees = &mut self.lvs[self.active].invitees;
        let index = invitees.iter().position(|invitee| invitee.id == id)?;
        Some(invitees.remove(index))
    }

    /// Moves an invited bidder one place up or down in the list.
    pub fn move_invitee(&mut self, id: InviteeId, up: bool) -> bool {
        let invitees = &mut self.lvs[self.active].invitees;
        let Some(index) = invitees.iter().position(|invitee| invitee.id == id) else {
            return false;
        };
        let target = if up {
            index.checked_sub(1)
        } else {
            Some(index + 1)
        };
        match target {
            Some(target) if target < invitees.len() => {
                invitees.swap(index, target);
                true
            }
            _ => false,
        }
    }

    /// Puts the bidder of `bid` into the Bieterverzeichnis of the active LV,
    /// when an offer is read (Anwender, 18.11.): an entry with the
    /// same firm name (case, spaces and punctuation ignored) or the same
    /// street and postcode is linked to the bid; otherwise a new entry with
    /// the bid's address is made („Angebot abgegeben“, Eingang = Angebotsdatum).
    /// Nothing happens when the bid is linked already, unknown, or its firm is
    /// listed but linked to another bid (no second entry for the same firm).
    /// Returns the entry and whether it was created.
    pub fn adopt_bidder(&mut self, bid: BidId) -> Option<(InviteeId, bool)> {
        let address = self
            .bids()
            .iter()
            .find(|known| known.id() == bid)?
            .bidder()
            .clone();
        if self.invitee_of_bid(bid).is_some() {
            return None;
        }
        let key = |text: &str| -> String {
            text.chars()
                .filter(|character| character.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect()
        };
        let name = key(&address.name1);
        let place = (key(&address.street), key(&address.postal_code));
        let same_firm = |known: &InvitedBidder| {
            (!name.is_empty() && key(&known.address.name1) == name)
                || (!place.0.is_empty()
                    && !place.1.is_empty()
                    && (key(&known.address.street), key(&known.address.postal_code)) == place)
        };
        let listed: Vec<(InviteeId, bool)> = self
            .invitees()
            .iter()
            .filter(|known| same_firm(known))
            .map(|known| (known.id, known.bid.is_none()))
            .collect();
        if let Some(&(id, _)) = listed.iter().find(|(_, free)| *free) {
            self.link_bid(bid, Some(id)).ok()?;
            return Some((id, false));
        }
        if !listed.is_empty() || name.is_empty() {
            return None;
        }
        // Same firm in other spelling (legal form, hyphen): as the dialog
        // proposes it (name similarity, free entries only).
        if let Some(id) = self.suggest_invitee(&address.name1) {
            self.link_bid(bid, Some(id)).ok()?;
            return Some((id, false));
        }
        let id = self.add_invitee(InvitedBidder::new(address));
        self.link_bid(bid, Some(id)).ok()?;
        Some((id, true))
    }

    /// Links a bid of the active LV with an invited bidder (Angebot abgegeben);
    /// another invitee linked to the same bid lets go. `None` unlinks the bid.
    /// A receipt date that came from a bid (same date) goes with the link.
    pub fn link_bid(&mut self, bid: BidId, invitee: Option<InviteeId>) -> Result<(), String> {
        let entry = &mut self.lvs[self.active];
        if !entry.bids.iter().any(|known| known.id() == bid) {
            return Err("Dieses Angebot gibt es im LV nicht.".to_string());
        }
        let receipt_of = |id: BidId| -> Option<String> {
            entry
                .bids
                .iter()
                .find(|known| known.id() == id)?
                .bid_date()
                .map(|date| {
                    GermanDate::parse_iso(date)
                        .map(|date| date.to_string())
                        .unwrap_or_else(|| date.to_string())
                })
        };
        let received = receipt_of(bid);
        if let Some(id) = invitee {
            if !entry.invitees.iter().any(|known| known.id == id) {
                return Err("Diesen angefragten Bieter gibt es im LV nicht.".to_string());
            }
        }
        // Receipt of the bid the new invitee had before (replaced by this one).
        let previous = invitee
            .and_then(|id| entry.invitees.iter().find(|known| known.id == id))
            .and_then(|known| known.bid)
            .filter(|old| *old != bid)
            .and_then(receipt_of);
        let came_from = |date: &str, from: &Option<String>| {
            from.as_deref().is_some_and(|from| from == date.trim())
        };
        for known in &mut entry.invitees {
            if known.bid == Some(bid) && Some(known.id) != invitee {
                known.bid = None;
                if known.status == InviteStatus::Submitted {
                    known.status = InviteStatus::Requested;
                }
                if came_from(&known.received, &received) {
                    known.received.clear();
                }
            }
            if Some(known.id) == invitee {
                known.bid = Some(bid);
                if matches!(known.status, InviteStatus::Requested | InviteStatus::NoBid) {
                    known.status = InviteStatus::Submitted;
                }
                if came_from(&known.received, &previous) {
                    known.received.clear();
                }
                if known.received.trim().is_empty() {
                    if let Some(date) = &received {
                        known.received = date.clone();
                    }
                }
            }
        }
        Ok(())
    }

    /// The invited bidder linked with a bid of the active LV.
    pub fn invitee_of_bid(&self, bid: BidId) -> Option<&InvitedBidder> {
        self.invitees()
            .iter()
            .find(|invitee| invitee.bid == Some(bid))
    }

    /// The invited bidder of the active LV whose name fits a bidder's name
    /// best (not yet linked to another bid); None below a clear match.
    pub fn suggest_invitee(&self, bidder_name: &str) -> Option<InviteeId> {
        self.invitees()
            .iter()
            .filter(|invitee| invitee.bid.is_none())
            .map(|invitee| (name_similarity(invitee.name(), bidder_name), invitee.id))
            .filter(|(score, _)| *score >= 60)
            .max_by_key(|(score, id)| (*score, std::cmp::Reverse(*id)))
            .map(|(_, id)| id)
    }
}

fn clean_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = base
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.trim_matches(['.', '_']).is_empty() {
        "bild.png".to_string()
    } else {
        cleaned
    }
}

/// A file size for messages: „850 KB“, „1,4 MB“ (at least 1 KB).
pub fn size_text(bytes: usize) -> String {
    if bytes < 1024 * 1024 {
        format!("{} KB", bytes.div_ceil(1024).max(1))
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)).replace('.', ",")
    }
}

#[cfg(test)]
mod tests {
    use super::Project;
    use crate::bid::{Address, Bid, BidId};
    use crate::invitation::{InviteStatus, InvitedBidder};
    use crate::{Lv, OzMask, ProjectName};

    fn project() -> Project {
        Project::new(
            ProjectName::parse("Test").unwrap(),
            "2026-11-17T18:00:00",
            Lv::new("LV", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap()),
        )
    }

    #[test]
    fn images_get_unique_clean_names() {
        let mut project = project();
        assert_eq!(
            project.add_image("/tmp/Keller fenster.jpg", vec![1]),
            "Keller_fenster.jpg"
        );
        assert_eq!(
            project.add_image("Keller fenster.jpg", vec![2]),
            "Keller_fenster_2.jpg"
        );
        assert_eq!(project.images().len(), 2);
        project.remove_unused_images();
        assert!(project.images().is_empty());
    }

    #[test]
    fn images_of_letter_templates_are_kept() {
        use crate::{LetterKind, LetterTemplate, TextBlock};
        let mut project = project();
        let file = project.add_image("lageplan.png", vec![1]);
        let mut template = LetterTemplate::standard(LetterKind::Invitation);
        template.body.blocks_mut().push(TextBlock::Image {
            file: file.clone(),
            width_mm: Some(80.0),
        });
        project.set_letter_template(template);
        project.remove_unused_images();
        assert_eq!(project.images().len(), 1, "Bild der Briefvorlage bleibt");
        project.set_letter_template(LetterTemplate::standard(LetterKind::Invitation));
        project.remove_unused_images();
        assert!(project.images().is_empty());
    }

    #[test]
    fn revision_follows_every_change_of_lvs_and_bids() {
        let mut project = project();
        let start = project.revision();
        let _ = (
            project.lv().title(),
            project.bids().len(),
            project.lvs().count(),
        );
        assert_eq!(project.revision(), start, "Lesen ändert nichts");
        project.lv_mut().set_title("Neu");
        let titled = project.revision();
        assert_ne!(titled, start);
        let bid = project.add_bid(Bid::new(Address::named("A"), "a.x84"));
        let with_bid = project.revision();
        assert_ne!(with_bid, titled);
        project.bid_mut(bid).unwrap().set_excluded(true);
        assert_ne!(project.revision(), with_bid);
        let copy = project.clone();
        assert_eq!(copy.revision(), project.revision());
        assert_eq!(copy, project);
        assert_ne!(
            self::project().revision(),
            project.revision(),
            "eindeutig über Projekte"
        );
        let other = project.add_lv(Lv::new(
            "LV 2",
            OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap(),
        ));
        let before = project.revision();
        project.set_active_lv(other);
        assert_eq!(
            project.revision(),
            before,
            "das aktive LV gehört zum Schlüssel (active_lv)"
        );
    }

    #[test]
    fn reading_a_bid_puts_its_firm_into_the_bidder_list_once() {
        // Anwender 18.11.: beim Angebotsimport Bieter anlegen,
        // vorhandene (gleicher Name/Anschrift) zuordnen statt doppelt.
        let mut project = project();
        let mut address = Address::named("Müller Regeltechnik GmbH");
        address.city = "Musterstadt".into();
        let mut bid = Bid::new(address, "m.x84");
        bid.set_bid_date(Some("2026-11-18".into()));
        let mueller = project.add_bid(bid);
        let (id, created) = project.adopt_bidder(mueller).unwrap();
        assert!(created);
        let entry = project
            .invitees()
            .iter()
            .find(|known| known.id == id)
            .unwrap();
        assert_eq!(entry.address.city, "Musterstadt");
        assert_eq!(entry.status, InviteStatus::Submitted);
        assert_eq!(entry.bid, Some(mueller));
        assert_eq!(entry.received, "18.11.2026");
        assert_eq!(project.adopt_bidder(mueller), None, "schon verknüpft");

        // Schon angefragt, Schreibweise anders: zuordnen, nicht doppelt.
        let schulze = project.add_invitee(InvitedBidder::new(Address::named("Schulze GA KG")));
        let bid = project.add_bid(Bid::new(Address::named("SCHULZE G.A. KG"), "s.x84"));
        assert_eq!(project.adopt_bidder(bid), Some((schulze, false)));
        // Gleiche Anschrift, anderer Name (Firmenname gekürzt): zuordnen.
        let mut weber = Address::named("Weber Automation AG");
        weber.street = "Hauptstraße 5".into();
        weber.postal_code = "12345".into();
        let listed = project.add_invitee(InvitedBidder::new(weber.clone()));
        weber.name1 = "Weber AG".into();
        let bid = project.add_bid(Bid::new(weber, "w.x84"));
        assert_eq!(project.adopt_bidder(bid), Some((listed, false)));
        // Andere Schreibweise mit Rechtsform: „Müller-Elektro“ = „Müller Elektro GmbH“.
        let elektro =
            project.add_invitee(InvitedBidder::new(Address::named("Müller Elektro GmbH")));
        let bid = project.add_bid(Bid::new(Address::named("Müller-Elektro"), "e.d84"));
        assert_eq!(project.adopt_bidder(bid), Some((elektro, false)));
        // Zweites Angebot derselben Firma (Nebenangebot): kein zweiter Eintrag.
        let again = project.add_bid(Bid::new(
            Address::named("Müller Regeltechnik GmbH"),
            "m2.x84",
        ));
        assert_eq!(project.adopt_bidder(again), None);
        assert_eq!(project.invitees().len(), 4);
    }

    #[test]
    fn bids_get_ids() {
        let mut project = project();
        let first = project.add_bid(Bid::new(Address::named("A"), "a.x84"));
        let second = project.add_bid(Bid::new(Address::named("B"), "b.x84"));
        assert_ne!(first, second);
        assert!(project.remove_bid(first).is_some());
        assert_eq!(project.bids().len(), 1);
    }

    #[test]
    fn several_lvs_with_their_own_bids() {
        let mut project = project();
        project.lv_mut().set_number("1");
        project.add_bid(Bid::new(Address::named("A"), "a.x84"));
        let mut second = Lv::new("Los 2", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        second.set_number("2");
        let index = project.add_lv(second);
        assert_eq!(index, 1);
        assert_eq!(project.active_lv(), 0);
        assert!(project.set_active_lv(1));
        assert_eq!(project.lv().title(), "Los 2");
        assert!(project.bids().is_empty());
        assert_eq!(
            project.add_bid(Bid::new(Address::named("B"), "b.x84")),
            BidId(1)
        );
        assert_eq!(project.bids_of(0).len(), 1);
        assert_eq!(project.move_lv(1, true), Some(0));
        assert_eq!(project.active_lv(), 0);
        assert_eq!(project.lv().title(), "Los 2");
        project.lv_mut().set_number("1");
        assert_eq!(project.duplicate_lv_numbers(), vec!["1".to_string()]);
        project
            .lv_mut()
            .mark_sent("2026-11-18T09:00:00".into(), Some("a.X83".into()));
        let copy = project.duplicate_lv(0).unwrap();
        assert_eq!(project.lv_at(copy).unwrap().title(), "Los 2 (Kopie)");
        assert!(project.bids_of(copy).is_empty());
        // The copy was never sent: no question before renumbering.
        let copied = project.lv_at(copy).unwrap();
        assert!(copied.sent_fingerprint().is_none() && copied.request_exported().is_none());
        assert!(project.lv_at(0).unwrap().sent_fingerprint().is_some());
        project.remove_lv(0).unwrap();
        project.remove_lv(0).unwrap();
        assert!(project.remove_lv(0).is_err());
        assert_eq!(project.lv_count(), 1);
        assert_eq!(project.active_lv(), 0);
    }

    #[test]
    fn contract_terms_check_and_compute() {
        use super::{ContractTerms, Deduction};
        use crate::Money;
        let dec = |text: &str| text.parse::<rust_decimal::Decimal>().unwrap();
        let mut terms = ContractTerms {
            deductions: ContractTerms::common_deductions(),
            ..ContractTerms::default()
        };
        assert!(terms.problems().is_empty());
        terms.deductions[0].percent = dec("0.3");
        terms.deductions[1].percent = dec("0.5");
        assert_eq!(terms.deduction_percent(), dec("0.8"));
        assert_eq!(
            terms.deduction_amounts(Money::from_cents(10_000_000)),
            vec![
                (
                    "Bauwesenversicherung".to_string(),
                    Money::from_cents(30_000)
                ),
                ("Baustrom".to_string(), Money::from_cents(50_000)),
            ]
        );
        terms.cash_discount_percent = Some(dec("2"));
        terms.performance_security_percent = Some(dec("120"));
        terms.deductions.push(Deduction {
            label: " ".into(),
            percent: dec("-1"),
        });
        let problems = terms.problems();
        assert_eq!(problems.len(), 4, "{problems:?}");
    }

    #[test]
    fn lv_list_undo_keeps_newer_bids() {
        let mut project = project();
        let first_key = project.lv_key(0).unwrap();
        let before = project.lv_list_snapshot();
        let second = project.add_lv(Lv::new(
            "Los 2",
            OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap(),
        ));
        assert_ne!(project.lv_key(second), Some(first_key));
        project.add_bid(Bid::new(Address::named("A"), "a.x84"));
        let after = project.restore_lv_list(before);
        assert_eq!(project.lv_count(), 1);
        assert_eq!(
            project.bids().len(),
            1,
            "Angebot von nach dem Schnappschuss bleibt"
        );
        project.restore_lv_list(after);
        assert_eq!(project.lv_count(), 2);
        assert_eq!(project.lv_index_of_key(first_key), Some(0));
        let mut copy = project.clone();
        copy.remove_lv(1).unwrap();
        copy.add_lv(project.lv_at(1).unwrap().clone());
        assert_eq!(copy, project, "Schlüssel zählen beim Vergleich nicht");
    }

    #[test]
    fn invited_bidders_are_linked_with_bids() {
        use crate::invitation::{InviteStatus, InvitedBidder};
        let mut project = project();
        let mueller =
            project.add_invitee(InvitedBidder::new(Address::named("Müller Elektro GmbH")));
        let schmidt =
            project.add_invitee(InvitedBidder::new(Address::named("Schmidt Regeltechnik")));
        assert_ne!(mueller, schmidt);
        let mut bid = Bid::new(Address::named("Elektro Müller"), "m.x84");
        bid.set_bid_date(Some("14.12.2026".into()));
        let bid = project.add_bid(bid);
        assert_eq!(
            project.suggest_invitee("Müller-Elektro GmbH"),
            Some(mueller)
        );
        assert_eq!(project.suggest_invitee("Meier Bau"), None);
        project.link_bid(bid, Some(mueller)).unwrap();
        let linked = project.invitee_of_bid(bid).unwrap();
        assert_eq!(linked.status, InviteStatus::Submitted);
        assert_eq!(linked.received, "14.12.2026");
        assert_eq!(
            project.suggest_invitee("Müller Elektro"),
            None,
            "schon verknüpft"
        );
        project.link_bid(bid, Some(schmidt)).unwrap();
        assert_eq!(project.invitees()[0].status, InviteStatus::Requested);
        assert_eq!(
            project.invitees()[0].received,
            "",
            "Eingang kam aus dem Angebot und geht mit"
        );
        assert_eq!(project.invitee_of_bid(bid).unwrap().id, schmidt);
        let mut other = Bid::new(Address::named("Schmidt Regeltechnik"), "s.x84");
        other.set_bid_date(Some("15.12.2026".into()));
        let other = project.add_bid(other);
        project.link_bid(other, Some(schmidt)).unwrap();
        assert_eq!(
            project.invitees()[1].received,
            "15.12.2026",
            "Eingang des neuen Angebots"
        );
        project.invitee_mut(schmidt).unwrap().received = "15.12.2026, 09:30 Uhr".into();
        project.link_bid(other, None).unwrap();
        assert_eq!(
            project.invitees()[1].received,
            "15.12.2026, 09:30 Uhr",
            "eigene Angabe bleibt"
        );
        project.remove_bid(other);
        project.link_bid(bid, Some(schmidt)).unwrap();
        project.remove_bid(bid);
        assert!(project
            .invitees()
            .iter()
            .all(|invitee| invitee.bid.is_none()));
        assert!(project.move_invitee(schmidt, true));
        assert_eq!(project.invitees()[0].id, schmidt);
        assert!(project.remove_invitee(mueller).is_some());
        let mut second = Lv::new("Los 2", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        second.set_number("2");
        let index = project.add_lv(second);
        assert!(project.invitees_of(index).is_empty());
    }

    #[test]
    fn offers_come_back_with_undo() {
        use crate::invitation::InvitedBidder;
        let mut project = project();
        let before = project.offers_snapshot(0).unwrap();
        project.add_bid(Bid::new(Address::named("A"), "a.x84"));
        project.add_invitee(InvitedBidder::new(Address::named("B")));
        let after = project.restore_offers(0, before).unwrap();
        assert!(project.bids().is_empty() && project.invitees().is_empty());
        project.restore_offers(0, after);
        assert_eq!(project.bids().len(), 1);
        assert_eq!(project.invitees().len(), 1);
        assert!(project.offers_snapshot(3).is_none());
    }

    #[test]
    fn master_data_problems_and_warnings() {
        use super::{join_deadline, split_deadline, ProjectInfo};
        let mut info = ProjectInfo {
            submission_deadline: join_deadline("15.12.2026", "10:00"),
            binding_period_end: "10.12.2026".into(),
            execution_start: "01.03.2027".into(),
            execution_end: "28.02.2027".into(),
            ..ProjectInfo::default()
        };
        assert_eq!(info.submission_deadline, "15.12.2026, 10:00 Uhr");
        assert_eq!(
            split_deadline(&info.submission_deadline),
            ("15.12.2026".into(), "10:00".into())
        );
        assert_eq!(
            split_deadline("nach Absprache"),
            ("nach Absprache".into(), String::new())
        );
        assert_eq!(info.problems().len(), 1, "{:?}", info.problems());
        assert_eq!(info.warnings().len(), 1, "{:?}", info.warnings());
        info.execution_end = "30.02.2027".into();
        assert!(info.problems()[0].contains("Ausführungsende"));
        info.execution_end = "31.12.2027".into();
        info.binding_period_end = "31.01.2027".into();
        assert!(info.problems().is_empty());
        assert!(info.warnings().is_empty());
        info.execution_start = "01.12.2026".into();
        assert_eq!(info.warnings().len(), 1);
    }

    #[test]
    fn client_is_the_own_one_or_the_building_owner() {
        use super::ProjectInfo;
        let mut info = ProjectInfo {
            owner: Address::named("Stadt Musterstadt"),
            ..ProjectInfo::default()
        };
        assert_eq!(info.client().name1, "Stadt Musterstadt", "wie Bauherr");
        assert!(!info.client_differs());
        info.own_client = Some(Address::named("Stadt Musterstadt"));
        assert!(!info.client_differs(), "gleiche Adresse zählt als gleich");
        info.own_client = Some(Address::named("Zentrale Vergabestelle"));
        assert_eq!(info.client().name1, "Zentrale Vergabestelle");
        assert!(info.client_differs());
    }

    #[test]
    fn a_whole_state_swaps_in_with_keys_active_lv_and_images_kept() {
        let mut project = project();
        project.add_lv(Lv::new("Zweites", OzMask::parse("11.PPPPI").unwrap()));
        project.set_active_lv(1);
        let photo = project.add_image("foto.png", vec![1, 2, 3]);
        let keys = [project.lv_key(0), project.lv_key(1)];

        // Read back from a file elsewhere: new keys, LV 1 active there.
        let mut first = project.lv_at(0).unwrap().clone();
        first.set_title("Geändert");
        let mut changed = Project::new(project.name().clone(), "2026-11-17T18:00:00", first);
        changed.add_lv(project.lv_at(1).unwrap().clone());
        changed.add_lv(Lv::new("Drittes", OzMask::parse("11.PPPPI").unwrap()));
        let plan = changed.add_image("plan.png", vec![4]);
        assert_ne!(changed.lv_key(0), keys[0]);
        changed.adopt_session_keys(&project);
        assert_eq!([changed.lv_key(0), changed.lv_key(1)], keys);
        let third = changed.lv_key(2);
        assert!(third.is_some() && !keys.contains(&third));

        let before = project.swap_state(changed);
        assert_eq!(project.active_lv(), 1, "das gezeigte LV bleibt");
        assert_eq!(project.lv_at(0).unwrap().title(), "Geändert");
        assert!(project.image(&photo).is_some() && project.image(&plan).is_some());
        assert!(before.images().is_empty(), "der Schritt trägt keine Bilder");
        assert!(!project.same_lv_entry(0, &before, 0));
        assert!(project.same_lv_entry(1, &before, 1));

        // Back (Strg+Z): the earlier state, the images stay.
        let after = project.swap_state(before);
        assert_eq!(project.lv_count(), 2);
        assert_eq!([project.lv_key(0), project.lv_key(1)], keys);
        assert_ne!(project.lv_at(0).unwrap().title(), "Geändert");
        assert!(
            project.image(&plan).is_some(),
            "Bilder gehen nicht verloren"
        );
        assert_eq!(after.lv_count(), 3);
        let mut again = project.clone();
        let _ = again.swap_state(after);
        assert!(!again.same_content(&project));
        assert!(project.same_content(&project.clone()));
    }

    #[test]
    fn loaded_bid_ids_are_kept_and_reserved() {
        let mut project = project();
        let kept = project.add_bid(Bid::new(Address::named("A"), "a").with_id(BidId(5)));
        assert_eq!(kept, BidId(5));
        project.reserve_bid_ids(BidId(9));
        assert_eq!(project.next_bid_id(), BidId(9));
        assert_eq!(
            project.add_bid(Bid::new(Address::named("B"), "b")),
            BidId(9)
        );
    }
}
