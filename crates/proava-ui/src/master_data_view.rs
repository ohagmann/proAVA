//! Projekt ▸ Projektstammdaten: everything basic about the project in four
//! tabs (Allgemein, Adressen, Vergabe & Termine, Vertragsbedingungen & Abzüge).
//! The form checks every field as you type; „Übernehmen“ only when valid.

use eframe::egui::{self, RichText};
use proava_domain::{
    format_decimal_de, join_deadline, optional_date, optional_time, parse_decimal_de, Address,
    Contact, ContractTerms, Decimal, Deduction, GermanDate, Project, ProjectInfo, AWARD_PROCEDURES,
    CONTACT_ROLES,
};

use crate::form_kit;
use crate::theme::Palette;
use crate::widgets;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasterTab {
    General,
    Addresses,
    Award,
    Terms,
    PageFrame,
}

const TABS: [(MasterTab, &str); 5] = [
    (MasterTab::General, "Allgemein"),
    (MasterTab::Addresses, "Adressen"),
    (MasterTab::Award, "Vergabe & Termine"),
    (MasterTab::Terms, "Vertragsbedingungen & Abzüge"),
    (MasterTab::PageFrame, "Kopf-/Fußzeile"),
];

/// One address of the Adressen tab: the fixed roles, then the further ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressSlot {
    Owner,
    Client,
    Planner,
    Site,
    Contact(usize),
}

impl AddressSlot {
    /// The role as shown in the list and as heading of the form.
    fn role(self, contacts: &[Contact]) -> String {
        match self {
            AddressSlot::Owner => "Bauherr".into(),
            AddressSlot::Client => "Auftraggeber".into(),
            AddressSlot::Planner => "Aufsteller / Planer".into(),
            AddressSlot::Site => "Baustelle".into(),
            AddressSlot::Contact(index) => contacts
                .get(index)
                .map(|contact| contact.role.trim())
                .filter(|role| !role.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("Weitere Adresse {}", index + 1)),
        }
    }

    /// What the role means and where it appears.
    fn explanation(self) -> &'static str {
        match self {
            AddressSlot::Owner => "Eigentümer bzw. Träger der Baumaßnahme. Steht auf dem Deckblatt.",
            AddressSlot::Client => {
                "Vergabestelle, schließt den Vertrag. Steht im GAEB-Export als Auftraggeber, auf dem \
                 Deckblatt, in der Kopfzeile der Ausdrucke, in der Niederschrift und im Auftrags-LV."
            }
            AddressSlot::Planner => "Stellt das LV auf. Steht auf dem Deckblatt und als Verfasser der Ausdrucke.",
            AddressSlot::Site => "Ort der Ausführung. Steht auf dem Deckblatt.",
            AddressSlot::Contact(_) => "Weitere Beteiligte; stehen auf dem Deckblatt unter ihrer Rolle.",
        }
    }
}

/// „Freie Eingabe“ in the award procedure choice.
const FREE_PROCEDURE: &str = "Freie Eingabe";

/// The form; strings as typed, checked live.
#[derive(Debug, Clone, PartialEq)]
pub struct MasterDataForm {
    pub tab: MasterTab,
    pub project_name: String,
    pub label: String,
    pub short_name: String,
    pub description: String,
    /// Bauherr.
    pub owner: Address,
    /// Auftraggeber when it is not the Bauherr (used unless `client_same`).
    pub client: Address,
    /// „Auftraggeber wie Bauherr“ (the default).
    pub client_same: bool,
    pub planner: Address,
    pub site: Address,
    /// Kennnummer and Bezeichnung of the Baustelle (GAEB CnstSite).
    pub site_number: String,
    pub site_name: String,
    /// From a GAEB file, kept as they are (shown, set via MCP):
    /// Benachrichtigungsort, further Ausführungsorte, Bedarfsträger.
    pub notification_site: Option<proava_domain::Place>,
    pub further_sites: Vec<proava_domain::ExecutionSite>,
    pub requesters: Vec<proava_domain::Place>,
    /// Debitorennummer and state of the Auftraggeber (GAEB OWN).
    pub client_account: String,
    pub client_country_kind: String,
    pub client_country_code: String,
    pub contacts: Vec<Contact>,
    /// The address shown in the Adressen tab.
    pub address: AddressSlot,
    /// One of `AWARD_PROCEDURES` or `FREE_PROCEDURE`.
    pub procedure_choice: String,
    pub procedure_free: String,
    pub award_number: String,
    /// Datenaustauschnummer of the Auftraggeber (GAEB OWN/DPNo).
    pub exchange_number: String,
    pub deadline_date: String,
    pub deadline_time: String,
    pub submission_place: String,
    pub binding_end: String,
    pub execution_start: String,
    pub execution_end: String,
    pub execution_note: String,
    pub performance_security: String,
    pub warranty_security: String,
    pub discount_percent: String,
    pub discount_days: String,
    /// (Bezeichnung, Prozent als Text)
    pub deductions: Vec<(String, String)>,
    pub payment_terms: String,
    pub notes: String,
    /// The project has its own Kopf-/Fußzeile (otherwise the settings' one).
    pub frame_own: bool,
    pub frame: proava_domain::PageFrame,
    /// Kopf-/Fußzeile from the settings (shown while the project has none).
    pub frame_default: proava_domain::PageFrame,
    /// Shown after a failed „Übernehmen“.
    pub error: Option<String>,
    /// The first field of the shown tab takes the keyboard focus (on opening
    /// and after switching the tab) – typing right away goes there.
    focus_first: bool,
    /// Today (for short dates like „1.12.“), refreshed while the dialog shows.
    pub today: GermanDate,
}

/// Errors per field (None = fine) and errors between fields.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FormErrors {
    pub deadline_date: Option<String>,
    pub deadline_time: Option<String>,
    pub binding_end: Option<String>,
    pub execution_start: Option<String>,
    pub execution_end: Option<String>,
    pub performance_security: Option<String>,
    pub warranty_security: Option<String>,
    pub discount_percent: Option<String>,
    pub discount_days: Option<String>,
    pub deductions: Vec<Option<String>>,
}

impl FormErrors {
    pub fn is_empty(&self) -> bool {
        [
            &self.deadline_date,
            &self.deadline_time,
            &self.binding_end,
            &self.execution_start,
            &self.execution_end,
            &self.performance_security,
            &self.warranty_security,
            &self.discount_percent,
            &self.discount_days,
        ]
        .iter()
        .all(|error| error.is_none())
            && self.deductions.iter().all(Option::is_none)
    }

    /// The tab that holds the first error (to jump there on „Übernehmen“).
    pub fn first_tab(&self) -> Option<MasterTab> {
        if self.deadline_date.is_some()
            || self.deadline_time.is_some()
            || self.binding_end.is_some()
            || self.execution_start.is_some()
            || self.execution_end.is_some()
        {
            Some(MasterTab::Award)
        } else if !self.is_empty() {
            Some(MasterTab::Terms)
        } else {
            None
        }
    }
}

/// Empty = none; otherwise a percentage 0–100 (German comma, „%“ allowed).
pub fn parse_percent(text: &str) -> Result<Option<Decimal>, String> {
    let text = text.trim().trim_end_matches('%').trim();
    if text.is_empty() {
        return Ok(None);
    }
    let value = parse_decimal_de(text).map_err(|_| format!("„{text}“ ist keine Zahl."))?;
    if value < Decimal::ZERO || value > Decimal::ONE_HUNDRED {
        return Err("Zwischen 0 und 100 %.".to_string());
    }
    Ok(Some(value))
}

fn parse_days(text: &str) -> Result<Option<u32>, String> {
    let text = text.trim().trim_end_matches("Tage").trim();
    if text.is_empty() {
        return Ok(None);
    }
    match text.parse::<u32>() {
        Ok(days) if days <= 365 => Ok(Some(days)),
        _ => Err("Ganze Tage (0–365).".to_string()),
    }
}

fn percent_text(value: Option<Decimal>) -> String {
    value.map(percent_value).unwrap_or_default()
}

fn percent_value(value: Decimal) -> String {
    let text = format_decimal_de(value, 3);
    let text = text.trim_end_matches('0').trim_end_matches(',');
    text.to_string()
}

impl MasterDataForm {
    pub fn of(project: &Project) -> Self {
        let info = project.info();
        let (deadline_date, deadline_time) = info.deadline_parts();
        let procedure = info.award_procedure.trim();
        let (procedure_choice, procedure_free) = if procedure.is_empty() {
            (String::new(), String::new())
        } else if AWARD_PROCEDURES.contains(&procedure) {
            (procedure.to_string(), String::new())
        } else {
            (FREE_PROCEDURE.to_string(), procedure.to_string())
        };
        let terms = &info.terms;
        Self {
            tab: MasterTab::General,
            project_name: project.name().as_str().to_string(),
            label: info.label.clone(),
            short_name: info.short_name.clone(),
            description: info.description.clone(),
            owner: info.owner.clone(),
            client: info
                .own_client
                .clone()
                .unwrap_or_else(|| info.owner.clone()),
            client_same: info.own_client.is_none(),
            planner: info.planner.clone(),
            site: info.site.clone(),
            site_number: info.site_number.clone(),
            site_name: info.site_name.clone(),
            notification_site: info.notification_site.clone(),
            further_sites: info.further_sites.clone(),
            requesters: info.requesters.clone(),
            client_account: info.client_account.clone(),
            client_country_kind: info.client_country_kind.clone(),
            client_country_code: info.client_country_code.clone(),
            contacts: info.contacts.clone(),
            address: AddressSlot::Owner,
            procedure_choice,
            procedure_free,
            award_number: info.award_number.clone(),
            exchange_number: info.exchange_number.clone(),
            deadline_date,
            deadline_time,
            submission_place: info.submission_place.clone(),
            binding_end: info.binding_period_end.clone(),
            execution_start: info.execution_start.clone(),
            execution_end: info.execution_end.clone(),
            execution_note: info.execution_note.clone(),
            performance_security: percent_text(terms.performance_security_percent),
            warranty_security: percent_text(terms.warranty_security_percent),
            discount_percent: percent_text(terms.cash_discount_percent),
            discount_days: terms
                .cash_discount_days
                .map(|days| days.to_string())
                .unwrap_or_default(),
            deductions: terms
                .deductions
                .iter()
                .map(|deduction| (deduction.label.clone(), percent_value(deduction.percent)))
                .collect(),
            payment_terms: terms.payment_terms.clone(),
            notes: terms.notes.clone(),
            frame_own: info.page_frame.is_some(),
            frame: info.page_frame.clone().unwrap_or_default(),
            frame_default: proava_domain::PageFrame::default(),
            error: None,
            focus_first: true,
            today: GermanDate::new(1, 1, 2026).expect("valid date"),
        }
    }

    /// A date field: empty, a full date or a short one („1.12.26“, „1.12.“).
    fn read_date(&self, text: &str) -> Result<Option<GermanDate>, String> {
        if let Some(date) = GermanDate::complete(text, self.today) {
            return Ok(Some(date));
        }
        optional_date(text)
    }

    pub fn errors(&self) -> FormErrors {
        let date = |text: &str| {
            self.read_date(text)
                .err()
                .map(|error| shorten_date_error(&error))
        };
        let mut errors = FormErrors {
            deadline_date: date(&self.deadline_date),
            deadline_time: optional_time(&self.deadline_time).err(),
            binding_end: date(&self.binding_end),
            execution_start: date(&self.execution_start),
            execution_end: date(&self.execution_end),
            performance_security: parse_percent(&self.performance_security).err(),
            warranty_security: parse_percent(&self.warranty_security).err(),
            discount_percent: parse_percent(&self.discount_percent).err(),
            discount_days: parse_days(&self.discount_days).err(),
            deductions: self
                .deductions
                .iter()
                .map(|(label, percent)| {
                    if label.trim().is_empty() && !percent.trim().is_empty() {
                        Some("Bezeichnung fehlt.".to_string())
                    } else if percent.trim().is_empty() && !label.trim().is_empty() {
                        Some("Prozentsatz fehlt.".to_string())
                    } else {
                        parse_percent(percent).err()
                    }
                })
                .collect(),
        };
        if errors.execution_end.is_none() {
            if let (Ok(Some(start)), Ok(Some(end))) = (
                self.read_date(&self.execution_start),
                self.read_date(&self.execution_end),
            ) {
                if end < start {
                    errors.execution_end = Some("Ende liegt vor dem Beginn.".to_string());
                }
            }
        }
        if errors.discount_days.is_none() && errors.discount_percent.is_none() {
            let percent = parse_percent(&self.discount_percent).ok().flatten();
            let days = parse_days(&self.discount_days).ok().flatten();
            match (percent, days) {
                (Some(percent), None) if !percent.is_zero() => {
                    errors.discount_days = Some("Skontofrist fehlt.".to_string())
                }
                (None, Some(_)) => errors.discount_percent = Some("Skontosatz fehlt.".to_string()),
                _ => {}
            }
        }
        errors
    }

    /// The award procedure as it is stored.
    fn procedure(&self) -> String {
        if self.procedure_choice == FREE_PROCEDURE {
            self.procedure_free.trim().to_string()
        } else {
            self.procedure_choice.trim().to_string()
        }
    }

    /// The master data as typed – only when every field is valid.
    pub fn to_info(&self) -> Result<ProjectInfo, String> {
        let errors = self.errors();
        if !errors.is_empty() {
            return Err("Bitte die rot markierten Felder berichtigen.".to_string());
        }
        let date_text = |text: &str| {
            self.read_date(text)
                .ok()
                .flatten()
                .map(|date: GermanDate| date.to_string())
                .unwrap_or_default()
        };
        let deadline_time = optional_time(&self.deadline_time)
            .ok()
            .flatten()
            .map(|(hours, minutes)| format!("{hours:02}:{minutes:02}"))
            .unwrap_or_default();
        let info = ProjectInfo {
            label: self.label.trim().to_string(),
            short_name: self.short_name.trim().to_string(),
            description: self.description.trim_end().to_string(),
            owner: form_kit::trimmed(&self.owner),
            own_client: Some(form_kit::trimmed(&self.client))
                .filter(|client| !self.client_same && *client != Address::default()),
            planner: form_kit::trimmed(&self.planner),
            site: form_kit::trimmed(&self.site),
            site_number: self.site_number.trim().to_string(),
            site_name: self.site_name.trim().to_string(),
            notification_site: self.notification_site.clone(),
            further_sites: self.further_sites.clone(),
            requesters: self.requesters.clone(),
            client_account: self.client_account.trim().to_string(),
            client_country_kind: self.client_country_kind.trim().to_string(),
            client_country_code: self.client_country_code.trim().to_string(),
            contacts: self
                .contacts
                .iter()
                .map(|contact| Contact {
                    role: contact.role.trim().to_string(),
                    address: form_kit::trimmed(&contact.address),
                })
                .filter(|contact| !contact.role.is_empty() || contact.address != Address::default())
                .collect(),
            award_number: self.award_number.trim().to_string(),
            exchange_number: self.exchange_number.trim().to_string(),
            award_procedure: self.procedure(),
            submission_deadline: join_deadline(&date_text(&self.deadline_date), &deadline_time),
            submission_place: self.submission_place.trim().to_string(),
            binding_period_end: date_text(&self.binding_end),
            execution_start: date_text(&self.execution_start),
            execution_end: date_text(&self.execution_end),
            execution_note: self.execution_note.trim_end().to_string(),
            terms: ContractTerms {
                performance_security_percent: parse_percent(&self.performance_security)
                    .ok()
                    .flatten(),
                warranty_security_percent: parse_percent(&self.warranty_security).ok().flatten(),
                cash_discount_percent: parse_percent(&self.discount_percent).ok().flatten(),
                cash_discount_days: parse_days(&self.discount_days).ok().flatten(),
                deductions: self
                    .deductions
                    .iter()
                    .filter(|(label, _)| !label.trim().is_empty())
                    .map(|(label, percent)| Deduction {
                        label: label.trim().to_string(),
                        percent: parse_percent(percent).ok().flatten().unwrap_or_default(),
                    })
                    .collect(),
                payment_terms: self.payment_terms.trim_end().to_string(),
                notes: self.notes.trim_end().to_string(),
            },
            page_frame: self.frame_own.then(|| self.frame.normalized()),
        };
        let problems = info.problems();
        if let Some(problem) = problems.first() {
            return Err(problem.clone());
        }
        Ok(info)
    }

    /// Hints that do not block (Bindefrist vor Abgabetermin …) – also while
    /// other fields still have errors: only the valid dates count.
    pub fn warnings(&self) -> Vec<String> {
        let valid = |text: &str| {
            self.read_date(text)
                .ok()
                .flatten()
                .map(|date| date.to_string())
                .unwrap_or_default()
        };
        let info = ProjectInfo {
            submission_deadline: valid(&self.deadline_date),
            binding_period_end: valid(&self.binding_end),
            execution_start: valid(&self.execution_start),
            execution_end: valid(&self.execution_end),
            ..ProjectInfo::default()
        };
        info.warnings()
    }

    /// Sum of the typed deductions in percent (invalid ones count 0).
    pub fn deduction_sum(&self) -> Decimal {
        self.deductions
            .iter()
            .filter_map(|(_, percent)| parse_percent(percent).ok().flatten())
            .sum()
    }

    /// Adds the usual deductions that are not there yet.
    pub fn insert_common_deductions(&mut self) {
        for deduction in ContractTerms::common_deductions() {
            let known = self
                .deductions
                .iter()
                .any(|(label, _)| label.trim().eq_ignore_ascii_case(&deduction.label));
            if !known {
                self.deductions
                    .push((deduction.label, percent_value(deduction.percent)));
            }
        }
    }

    /// Draws the form; returns `(cancelled, confirmed)`.
    pub fn show(&mut self, ui: &mut egui::Ui, palette: &Palette) -> (bool, bool) {
        self.today = crate::date_picker::today(ui.ctx());
        widgets::title(
            ui,
            palette,
            "Projektstammdaten",
            "Erscheinen im Deckblatt der Ausdrucke, in Kopfzeilen und im GAEB-Export.",
        );
        let shown_tab = self.tab;
        form_kit::tab_bar(ui, palette, &TABS, &mut self.tab);
        if self.tab != shown_tab {
            self.focus_first = true;
        }
        let errors = self.errors();
        egui::ScrollArea::vertical()
            .id_salt("master_data_scroll")
            .max_height((ui.ctx().content_rect().height() * 0.62).clamp(260.0, 620.0))
            .min_scrolled_height((ui.ctx().content_rect().height() * 0.62).clamp(260.0, 620.0))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Feste Höhe: der Dialog springt beim Reiterwechsel nicht.
                ui.set_min_height(
                    (ui.ctx().content_rect().height() * 0.62).clamp(260.0, 620.0) - 4.0,
                );
                match self.tab {
                    MasterTab::General => self.general(ui, palette),
                    MasterTab::Addresses => self.addresses(ui, palette),
                    MasterTab::Award => self.award(ui, palette, &errors),
                    MasterTab::Terms => self.terms(ui, palette, &errors),
                    MasterTab::PageFrame => self.page_frame(ui, palette),
                }
            });
        ui.add_space(8.0);
        for warning in self.warnings() {
            ui.label(
                RichText::new(format!("Hinweis: {warning}"))
                    .size(12.0)
                    .color(palette.warn),
            );
        }
        if !errors.is_empty() {
            let tabs: Vec<&str> = [MasterTab::Award, MasterTab::Terms]
                .into_iter()
                .filter(|tab| {
                    *tab == MasterTab::Award && errors.first_tab() == Some(MasterTab::Award)
                        || *tab == MasterTab::Terms && terms_have_errors(&errors)
                })
                .map(|tab| {
                    TABS.iter()
                        .find(|(known, _)| *known == tab)
                        .map(|(_, label)| *label)
                        .unwrap_or("")
                })
                .collect();
            ui.label(
                RichText::new(format!("Fehler im Reiter {}.", tabs.join(" und ")))
                    .size(12.0)
                    .color(palette.error),
            );
        }
        ui.add_space(4.0);
        widgets::error_line(ui, palette, self.error.as_deref());
        widgets::dialog_buttons(ui, palette, "Übernehmen")
    }

    fn general(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        ui.label(RichText::new("Projekt").size(12.0).color(palette.text_dim));
        ui.label(
            RichText::new(&self.project_name)
                .size(14.0)
                .strong()
                .color(palette.text),
        );
        form_kit::hint(
            ui,
            palette,
            "Der Projektname ist der Name der Projektdatei.",
        );
        ui.add_space(10.0);
        let first = form_kit::field(ui, palette, "Projektbezeichnung", &mut self.label, None);
        if std::mem::take(&mut self.focus_first) {
            first.request_focus();
        }
        form_kit::field_with(
            ui,
            palette,
            "Kurzname / Projektnummer des Auftraggebers",
            &mut self.short_name,
            None,
            "leer = Projektname",
        );
        form_kit::multiline(
            ui,
            palette,
            "Baumaßnahme (Kurzbeschreibung)",
            &mut self.description,
            4,
        );
    }

    fn addresses(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        if let AddressSlot::Contact(index) = self.address {
            if index >= self.contacts.len() {
                self.address = AddressSlot::Owner;
            }
        }
        let height = ui.available_height().max(300.0);
        ui.horizontal_top(|ui| {
            // Left: every address with its role (address list).
            ui.allocate_ui_with_layout(
                egui::vec2(240.0, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(240.0);
                    let shown = self.address;
                    self.address_list(ui, palette);
                    // Another address chosen: its first field takes the cursor.
                    if self.address != shown {
                        self.focus_first = true;
                    }
                },
            );
            ui.add_space(14.0);
            // Right: the chosen address under its role.
            ui.vertical(|ui| self.address_form(ui, palette));
        });
    }

    /// The fixed roles, then the further addresses; a click shows one.
    fn address_list(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        let mut slots = vec![
            AddressSlot::Owner,
            AddressSlot::Client,
            AddressSlot::Planner,
            AddressSlot::Site,
        ];
        slots.extend((0..self.contacts.len()).map(AddressSlot::Contact));
        egui::Frame::new()
            .stroke(egui::Stroke::new(1.0, palette.line))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::same(4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Rolle")
                            .size(11.0)
                            .strong()
                            .color(palette.text_dim),
                    );
                });
                for slot in slots {
                    let role = slot.role(&self.contacts);
                    let name = self.address_name(slot);
                    let selected = self.address == slot;
                    let response = address_row(ui, palette, &role, &name, selected);
                    if response.clicked() {
                        self.address = slot;
                    }
                    if slot == AddressSlot::Site && !self.contacts.is_empty() {
                        ui.separator();
                    }
                }
            });
        ui.add_space(6.0);
        crate::menus::menu(ui, "Adresse hinzufügen …", |ui| {
            for role in CONTACT_ROLES {
                if ui.button(role).clicked() {
                    self.contacts.push(Contact {
                        role: role.to_string(),
                        address: Address::default(),
                    });
                    self.address = AddressSlot::Contact(self.contacts.len() - 1);
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("Andere Rolle").clicked() {
                self.contacts.push(Contact::default());
                self.address = AddressSlot::Contact(self.contacts.len() - 1);
                ui.close();
            }
        });
    }

    /// Name shown in the list next to the role („wie Bauherr“, „– leer –“).
    fn address_name(&self, slot: AddressSlot) -> String {
        let address = match slot {
            AddressSlot::Owner => &self.owner,
            AddressSlot::Client if self.client_same => return "wie Bauherr".into(),
            AddressSlot::Client => &self.client,
            AddressSlot::Planner => &self.planner,
            AddressSlot::Site => &self.site,
            AddressSlot::Contact(index) => match self.contacts.get(index) {
                Some(contact) => &contact.address,
                None => return String::new(),
            },
        };
        let name = address.name1.trim();
        if name.is_empty() {
            let line = address.one_line();
            if line.trim().is_empty() {
                "– leer –".into()
            } else {
                line
            }
        } else {
            name.to_string()
        }
    }

    fn address_form(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        let slot = self.address;
        ui.label(
            RichText::new(slot.role(&self.contacts))
                .size(17.0)
                .strong()
                .color(palette.text),
        );
        ui.add_space(2.0);
        form_kit::hint(ui, palette, slot.explanation());
        ui.add_space(8.0);
        match slot {
            AddressSlot::Owner => {
                form_kit::address_with_focus(ui, palette, &mut self.owner, &mut self.focus_first)
            }
            AddressSlot::Client => {
                let before = self.client_same;
                ui.checkbox(&mut self.client_same, "Auftraggeber wie Bauherr");
                if before && !self.client_same && self.client == Address::default() {
                    // Start from the Bauherr: usually only a few lines differ.
                    self.client = self.owner.clone();
                }
                ui.add_space(6.0);
                if self.client_same {
                    self.focus_first = false;
                    let mut shown = self.owner.clone();
                    ui.add_enabled_ui(false, |ui| form_kit::address(ui, palette, &mut shown));
                } else {
                    form_kit::address_with_focus(
                        ui,
                        palette,
                        &mut self.client,
                        &mut self.focus_first,
                    );
                }
                self.client_extra(ui, palette);
            }
            AddressSlot::Planner => {
                form_kit::address_with_focus(ui, palette, &mut self.planner, &mut self.focus_first)
            }
            AddressSlot::Site => {
                form_kit::address_with_focus(ui, palette, &mut self.site, &mut self.focus_first);
                form_kit::split(
                    ui,
                    0.4,
                    |ui| {
                        form_kit::field(ui, palette, "Kennnummer", &mut self.site_number, None);
                    },
                    |ui| {
                        form_kit::field(ui, palette, "Bezeichnung", &mut self.site_name, None);
                    },
                );
                let mut lines = Vec::new();
                if let Some(place) = &self.notification_site {
                    lines.push(format!("Benachrichtigungsort: {}", place.details()));
                }
                for site in &self.further_sites {
                    lines.push(format!("Weiterer Ausführungsort: {}", site.site.details()));
                    if let Some(place) = &site.notification {
                        lines.push(format!("  Benachrichtigungsort: {}", place.details()));
                    }
                }
                gaeb_lines(ui, palette, &lines);
            }
            AddressSlot::Contact(index) => {
                let mut remove = false;
                if let Some(contact) = self.contacts.get_mut(index) {
                    crate::widgets::row(ui, |ui| {
                        ui.label(RichText::new("Rolle").size(12.0).color(palette.text_dim));
                        egui::ComboBox::from_id_salt(("md_role", index))
                            .selected_text("Rolle wählen")
                            .show_ui(ui, |ui| {
                                for role in CONTACT_ROLES {
                                    if ui.selectable_label(contact.role == role, role).clicked() {
                                        contact.role = role.to_string();
                                    }
                                }
                            });
                        if ui.button("Adresse entfernen").clicked() {
                            remove = true;
                        }
                    });
                    ui.add_space(4.0);
                    let role =
                        form_kit::field(ui, palette, "Rolle (frei)", &mut contact.role, None);
                    if std::mem::take(&mut self.focus_first) {
                        role.request_focus();
                    }
                    form_kit::address(ui, palette, &mut contact.address);
                }
                if remove {
                    self.contacts.remove(index);
                    self.address = if index > 0 {
                        AddressSlot::Contact(index - 1)
                    } else {
                        AddressSlot::Site
                    };
                }
            }
        }
    }

    /// Auftraggeber: Debitorennummer and state (GAEB OWN); Bedarfsträger.
    fn client_extra(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        ui.add_space(6.0);
        form_kit::split(
            ui,
            0.4,
            |ui| {
                form_kit::field(
                    ui,
                    palette,
                    "Debitorennummer",
                    &mut self.client_account,
                    None,
                );
            },
            |ui| {
                ui.label(RichText::new("Staat").size(12.0).color(palette.text_dim));
                crate::widgets::row(ui, |ui| {
                    for (kind, label) in [("", "–"), ("EEA", "EWR"), ("Other", "anderer Staat")] {
                        if ui
                            .selectable_label(self.client_country_kind == kind, label)
                            .clicked()
                        {
                            self.client_country_kind = kind.to_string();
                        }
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut self.client_country_code)
                            .hint_text("Kfz-Kennz.")
                            .char_limit(3)
                            .desired_width(70.0),
                    );
                });
            },
        );
        let lines: Vec<String> = self
            .requesters
            .iter()
            .map(|place| format!("Bedarfsträger: {}", place.details()))
            .collect();
        gaeb_lines(ui, palette, &lines);
    }

    fn award(&mut self, ui: &mut egui::Ui, palette: &Palette, errors: &FormErrors) {
        form_kit::section(ui, palette, "Vergabe");
        ui.label(
            RichText::new("Vergabeart")
                .size(12.0)
                .color(palette.text_dim),
        );
        ui.add_space(3.0);
        let shown = if self.procedure_choice.is_empty() {
            "– bitte wählen –".to_string()
        } else {
            self.procedure_choice.clone()
        };
        egui::ComboBox::from_id_salt("md_procedure")
            .selected_text(shown)
            .width(ui.available_width().min(420.0))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.procedure_choice,
                    String::new(),
                    "– keine Angabe –",
                );
                for procedure in AWARD_PROCEDURES.iter().chain([&FREE_PROCEDURE]) {
                    ui.selectable_value(
                        &mut self.procedure_choice,
                        procedure.to_string(),
                        *procedure,
                    );
                }
            });
        ui.add_space(8.0);
        if self.procedure_choice == FREE_PROCEDURE {
            form_kit::field(
                ui,
                palette,
                "Vergabeart (frei)",
                &mut self.procedure_free,
                None,
            );
        }
        let mut number = None;
        form_kit::split(
            ui,
            0.5,
            |ui| {
                number = Some(form_kit::field(
                    ui,
                    palette,
                    "Vergabenummer",
                    &mut self.award_number,
                    None,
                ));
            },
            |ui| {
                form_kit::field(
                    ui,
                    palette,
                    "Datenaustauschnummer (GAEB DPNo)",
                    &mut self.exchange_number,
                    None,
                );
            },
        );
        if std::mem::take(&mut self.focus_first) {
            if let Some(number) = number {
                number.request_focus();
            }
        }
        form_kit::section(ui, palette, "Angebotsabgabe");
        form_kit::split(
            ui,
            0.6,
            |ui| {
                form_kit::date_field(
                    ui,
                    palette,
                    "Abgabetermin",
                    &mut self.deadline_date,
                    errors.deadline_date.as_deref(),
                );
            },
            |ui| {
                form_kit::field_with(
                    ui,
                    palette,
                    "Uhrzeit",
                    &mut self.deadline_time,
                    errors.deadline_time.as_deref(),
                    "HH:MM",
                );
            },
        );
        form_kit::field(ui, palette, "Abgabeort", &mut self.submission_place, None);
        form_kit::date_field(
            ui,
            palette,
            "Zuschlags- und Bindefrist bis",
            &mut self.binding_end,
            errors.binding_end.as_deref(),
        );
        form_kit::section(ui, palette, "Ausführungsfristen");
        form_kit::split(
            ui,
            0.5,
            |ui| {
                form_kit::date_field(
                    ui,
                    palette,
                    "Ausführungsbeginn",
                    &mut self.execution_start,
                    errors.execution_start.as_deref(),
                );
            },
            |ui| {
                form_kit::date_field(
                    ui,
                    palette,
                    "Ausführungsende",
                    &mut self.execution_end,
                    errors.execution_end.as_deref(),
                );
            },
        );
        form_kit::multiline(
            ui,
            palette,
            "Weitere Fristen (z. B. Zwischentermine, Fertigstellung einzelner Bauteile)",
            &mut self.execution_note,
            3,
        );
    }

    /// Value of a placeholder for the preview: from the form, otherwise an example.
    fn frame_value(&self, name: &str) -> String {
        let text = match name {
            "projekt" => self.project_name.clone(),
            "projektnummer" => self.short_name.clone(),
            "projektbezeichnung" => self.label.clone(),
            "baumassnahme" => self
                .description
                .lines()
                .next()
                .unwrap_or_default()
                .to_string(),
            "bauherr" | "auftraggeber" => self.owner.name1.clone(),
            "planer" | "bearbeiter" => self.planner.name1.clone(),
            "vergabenummer" => self.award_number.clone(),
            "kopftext" | "fusstext" => String::new(),
            other => return crate::frame_form::sample_value(other),
        };
        text.trim().to_string()
    }

    fn page_frame(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        let own = ui
            .checkbox(
                &mut self.frame_own,
                "Eigene Kopf-/Fußzeile für dieses Projekt",
            )
            .on_hover_text("Sonst gilt die aus Extras, Einstellungen, Reiter Kopf-/Fußzeile.");
        if own.changed() && self.frame_own && self.frame == proava_domain::PageFrame::default() {
            self.frame = self.frame_default.clone();
        }
        ui.add_space(6.0);
        let form = self.clone();
        let value = move |name: &str| form.frame_value(name);
        if self.frame_own {
            crate::frame_form::editor(ui, palette, &mut self.frame, &value);
        } else {
            ui.label(
                RichText::new(
                    "Es gilt die Kopf-/Fußzeile aus den Einstellungen (Extras, Einstellungen):",
                )
                .size(12.0)
                .color(palette.text_dim),
            );
            ui.add_space(4.0);
            crate::frame_form::preview(ui, palette, &self.frame_default, &value);
        }
    }

    fn terms(&mut self, ui: &mut egui::Ui, palette: &Palette, errors: &FormErrors) {
        form_kit::section(ui, palette, "Sicherheiten");
        let focus_first = std::mem::take(&mut self.focus_first);
        form_kit::split(
            ui,
            0.5,
            |ui| {
                let first = form_kit::field_with(
                    ui,
                    palette,
                    "Vertragserfüllung in % der Auftragssumme",
                    &mut self.performance_security,
                    errors.performance_security.as_deref(),
                    "z. B. 5",
                );
                if focus_first {
                    first.request_focus();
                }
            },
            |ui| {
                form_kit::field_with(
                    ui,
                    palette,
                    "Mängelansprüche in % der Abrechnungssumme",
                    &mut self.warranty_security,
                    errors.warranty_security.as_deref(),
                    "z. B. 3",
                );
            },
        );
        form_kit::section(ui, palette, "Skonto");
        form_kit::split(
            ui,
            0.5,
            |ui| {
                form_kit::field_with(
                    ui,
                    palette,
                    "Skonto in %",
                    &mut self.discount_percent,
                    errors.discount_percent.as_deref(),
                    "z. B. 2",
                );
            },
            |ui| {
                form_kit::field_with(
                    ui,
                    palette,
                    "bei Zahlung innerhalb von … Tagen",
                    &mut self.discount_days,
                    errors.discount_days.as_deref(),
                    "z. B. 14",
                );
            },
        );
        form_kit::section(ui, palette, "Abzüge / Umlagen (in % der Abrechnungssumme)");
        let mut remove = None;
        let width = ui.available_width();
        for (index, (label, percent)) in self.deductions.iter_mut().enumerate() {
            let error = errors.deductions.get(index).cloned().flatten();
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(label)
                        .hint_text("Bezeichnung")
                        .desired_width(width - 230.0),
                );
                let mut edit = egui::TextEdit::singleline(percent)
                    .hint_text("%")
                    .align(egui::Align2::RIGHT_CENTER)
                    .desired_width(80.0);
                if error.is_some() {
                    edit = edit.text_color(palette.error);
                }
                ui.add(edit);
                ui.label("%");
                if ui.small_button("Entfernen").clicked() {
                    remove = Some(index);
                }
            });
            if let Some(error) = error {
                ui.label(RichText::new(error).size(11.5).color(palette.error));
            }
        }
        if let Some(index) = remove {
            self.deductions.remove(index);
        }
        ui.horizontal(|ui| {
            if ui.button("Abzug hinzufügen").clicked() {
                self.deductions.push((String::new(), String::new()));
            }
            if ui.button("Übliche Abzüge einfügen").on_hover_text(
                "Bauwesenversicherung, Baustrom, Bauwasser, Baureinigung – Prozentsätze danach eintragen",
            ).clicked() {
                self.insert_common_deductions();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("Summe der Abzüge: {} %", percent_value(self.deduction_sum())))
                        .strong()
                        .color(palette.text),
                );
            });
        });
        ui.add_space(10.0);
        form_kit::section(ui, palette, "Zahlungsbedingungen und Weiteres");
        form_kit::multiline(
            ui,
            palette,
            "Zahlungsbedingungen",
            &mut self.payment_terms,
            3,
        );
        form_kit::multiline(ui, palette, "Weitere Vereinbarungen", &mut self.notes, 3);
    }
}

fn terms_have_errors(errors: &FormErrors) -> bool {
    errors.performance_security.is_some()
        || errors.warranty_security.is_some()
        || errors.discount_percent.is_some()
        || errors.discount_days.is_some()
        || errors.deductions.iter().any(Option::is_some)
}

/// „Den 31.04.2027 gibt es nicht.“ stays; the long format hint gets shorter under a field.
fn shorten_date_error(error: &str) -> String {
    if error.contains("Format") {
        "Datum als TT.MM.JJJJ.".to_string()
    } else {
        error.to_string()
    }
}

/// One line of the address list: role in bold, name below it (dim).
fn address_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    role: &str,
    name: &str,
    selected: bool,
) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 40.0), egui::Sense::click());
    widgets::name_button(ui, &response, role);
    let fill = if selected {
        palette.selected
    } else if response.hovered() {
        palette.row_hover
    } else {
        egui::Color32::TRANSPARENT
    };
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::CornerRadius::same(4), fill);
    if selected {
        painter.rect_filled(
            egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())),
            egui::CornerRadius::ZERO,
            palette.accent,
        );
    }
    widgets::bold_text(
        &painter,
        egui::pos2(rect.left() + 10.0, rect.top() + 12.0),
        egui::Align2::LEFT_CENTER,
        role,
        egui::FontId::proportional(13.0),
        palette.text,
    );
    painter.text(
        egui::pos2(rect.left() + 10.0, rect.bottom() - 11.0),
        egui::Align2::LEFT_CENTER,
        crate::format::truncate_middle(name, 34),
        egui::FontId::proportional(12.0),
        palette.text_dim,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Places of a GAEB file the form keeps as they are (set via MCP).
fn gaeb_lines(ui: &mut egui::Ui, palette: &Palette, lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    ui.add_space(6.0);
    for line in lines {
        ui.label(RichText::new(line).size(12.0).color(palette.text_dim));
    }
    form_kit::hint(
        ui,
        palette,
        "Aus der GAEB-Datei; bleiben erhalten und gehen mit dem Export wieder hinaus.",
    );
}

#[cfg(test)]
mod tests {
    use proava_domain::{Decimal, Lv, OzMask, Project, ProjectName, AWARD_PROCEDURES};

    use super::{parse_percent, MasterDataForm, MasterTab, FREE_PROCEDURE};

    fn project() -> Project {
        Project::new(
            ProjectName::parse("Schule").unwrap(),
            "2026-11-17",
            Lv::new("LV", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap()),
        )
    }

    #[test]
    fn empty_form_round_trips() {
        let project = project();
        let form = MasterDataForm::of(&project);
        assert!(form.errors().is_empty());
        assert_eq!(&form.to_info().unwrap(), project.info());
    }

    #[test]
    fn filled_form_becomes_master_data_and_back() {
        let mut form = MasterDataForm::of(&project());
        form.label = " Neubau Schulzentrum ".into();
        form.owner.name1 = "Stadt Musterstadt".into();
        form.procedure_choice = AWARD_PROCEDURES[0].into();
        form.deadline_date = "1.2.2027".into();
        form.deadline_time = "10:00".into();
        form.binding_end = "31.3.2027".into();
        form.execution_start = "01.05.2027".into();
        form.execution_end = "30.11.2027".into();
        form.performance_security = "5".into();
        form.discount_percent = "2,5 %".into();
        form.discount_days = "14".into();
        form.insert_common_deductions();
        form.insert_common_deductions();
        assert_eq!(form.deductions.len(), 4);
        form.deductions[0].1 = "0,3".into();
        form.deductions[1].1 = "0,5".into();
        form.deductions[2].1 = "0".into();
        form.deductions[3].1 = "0".into();
        let info = form.to_info().unwrap();
        assert_eq!(info.label, "Neubau Schulzentrum");
        assert_eq!(info.submission_deadline, "01.02.2027, 10:00 Uhr");
        assert_eq!(info.execution_start, "01.05.2027");
        assert_eq!(
            info.terms.deduction_percent(),
            "0.8".parse::<Decimal>().unwrap()
        );
        assert_eq!(info.terms.cash_discount_days, Some(14));
        assert!(info.warnings().is_empty());

        let mut project = project();
        *project.info_mut() = info.clone();
        let again = MasterDataForm::of(&project);
        assert_eq!(again.to_info().unwrap(), info);
        assert_eq!(again.discount_percent, "2,5");
    }

    #[test]
    fn errors_sit_at_the_fields() {
        let mut form = MasterDataForm::of(&project());
        form.deadline_date = "31.04.2027".into();
        form.execution_start = "01.06.2027".into();
        form.execution_end = "01.05.2027".into();
        form.warranty_security = "120".into();
        form.discount_percent = "2".into();
        form.deductions.push(("Baustrom".into(), "x".into()));
        form.deductions.push((String::new(), "1".into()));
        let errors = form.errors();
        assert!(errors.deadline_date.unwrap().contains("gibt es nicht"));
        assert!(errors.execution_end.is_some());
        assert!(errors.warranty_security.is_some());
        assert!(errors.discount_days.is_some());
        assert_eq!(
            errors
                .deductions
                .iter()
                .filter(|error| error.is_some())
                .count(),
            2
        );
        assert!(form.to_info().is_err());
        assert_eq!(form.errors().first_tab(), Some(MasterTab::Award));
    }

    #[test]
    fn warnings_show_also_with_errors_elsewhere() {
        let mut form = MasterDataForm::of(&project());
        form.deadline_date = "15.12.2026".into();
        form.binding_end = "01.12.2026".into();
        form.execution_end = "31.04.2027".into();
        assert!(form.to_info().is_err());
        assert_eq!(form.warnings().len(), 1);
    }

    #[test]
    fn binding_period_before_deadline_is_a_warning() {
        let mut form = MasterDataForm::of(&project());
        form.deadline_date = "15.12.2026".into();
        form.binding_end = "01.12.2026".into();
        assert!(form.to_info().is_ok());
        assert_eq!(form.warnings().len(), 1);
    }

    #[test]
    fn free_procedure_and_percent_parsing() {
        let mut project = project();
        project.info_mut().award_procedure = "Direktauftrag".into();
        let form = MasterDataForm::of(&project);
        assert_eq!(form.procedure_choice, FREE_PROCEDURE);
        assert_eq!(form.to_info().unwrap().award_procedure, "Direktauftrag");
        assert_eq!(parse_percent("").unwrap(), None);
        assert_eq!(parse_percent("0,3%").unwrap(), Some("0.3".parse().unwrap()));
        assert!(parse_percent("-1").is_err());
    }
}
