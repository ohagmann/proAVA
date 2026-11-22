//! Bieterverzeichnis of the active LV (Reiter „Angebote“, oberer Teil):
//! who was asked for a bid, status, link to the bid read in, Eingang,
//! Nebenangebote, Bemerkungen – the base of the Niederschrift.

use eframe::egui::{self, RichText};
use proava_domain::{Address, BidId, InviteStatus, InvitedBidder, InviteeId, Project};

use crate::form_kit;
use crate::icons::Icon;
use crate::list_keys::ListKey;
use crate::theme::Palette;
use crate::view::{ViewRequest, ViewResponse};
use crate::widgets;

/// An address the user can take for a new invited bidder.
#[derive(Debug, Clone, PartialEq)]
pub struct AddressChoice {
    pub label: String,
    pub address: Address,
}

/// Addresses known in the project: further addresses of the master data and
/// bidders invited for other LVs (each firm once, not those already on the list).
pub fn address_choices(project: &Project) -> Vec<AddressChoice> {
    let mut choices: Vec<AddressChoice> = Vec::new();
    let listed: Vec<String> = project
        .invitees()
        .iter()
        .map(|invitee| invitee.name().to_lowercase())
        .collect();
    let mut push = |label: String, address: &Address| {
        let name = address.name1.trim().to_lowercase();
        if name.is_empty()
            || listed.contains(&name)
            || choices
                .iter()
                .any(|known| known.address.name1.trim().to_lowercase() == name)
        {
            return;
        }
        choices.push(AddressChoice {
            label,
            address: address.clone(),
        });
    };
    for index in 0..project.lv_count() {
        if index == project.active_lv() {
            continue;
        }
        for invitee in project.invitees_of(index) {
            push(
                format!("{} (Bieter in LV {})", invitee.name(), index + 1),
                &invitee.address,
            );
        }
    }
    for contact in &project.info().contacts {
        let role = if contact.role.trim().is_empty() {
            "Adresse"
        } else {
            contact.role.trim()
        };
        push(
            format!("{} ({role})", contact.address.name1.trim()),
            &contact.address,
        );
    }
    choices
}

/// „Müller GmbH – 123.456,78 €“ for the bid choice.
pub fn bid_label(project: &Project, bid: BidId) -> String {
    project
        .bids()
        .iter()
        .find(|known| known.id() == bid)
        .map(|known| format!("{} – {}", known.name(), known.net_total(project.lv())))
        .unwrap_or_else(|| "–".to_string())
}

/// For bids just read in: the invited bidder each one most likely belongs to
/// (by name, every invited bidder at most once, not those already linked).
pub fn proposed_links(project: &Project, bids: &[(BidId, String)]) -> Vec<Option<InviteeId>> {
    let mut used: Vec<InviteeId> = Vec::new();
    bids.iter()
        .map(|(_, name)| {
            let best = project
                .invitees()
                .iter()
                .filter(|invitee| invitee.bid.is_none() && !used.contains(&invitee.id))
                .map(|invitee| {
                    (
                        proava_domain::name_similarity(invitee.name(), name),
                        invitee.id,
                    )
                })
                .filter(|(score, _)| *score >= 60)
                .max_by_key(|(score, id)| (*score, std::cmp::Reverse(*id)))
                .map(|(_, id)| id);
            if let Some(id) = best {
                used.push(id);
            }
            best
        })
        .collect()
}

/// Choice „Angefragter Bieter“ under a bidder's name (dialog after reading bids).
pub fn link_choice(
    ui: &mut egui::Ui,
    palette: &Palette,
    position: usize,
    link: &mut Option<InviteeId>,
    invitees: &[(InviteeId, String)],
) {
    crate::widgets::row(ui, |ui| {
        ui.label(
            RichText::new("Angefragter Bieter")
                .size(12.0)
                .color(palette.text_dim),
        );
        let shown = link
            .and_then(|id| invitees.iter().find(|(known, _)| *known == id))
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| "– nicht im Bieterverzeichnis –".to_string());
        egui::ComboBox::from_id_salt(("bids_read_link", position))
            .selected_text(shown)
            .width(300.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(link, None, "– nicht im Bieterverzeichnis –");
                for (id, name) in invitees {
                    ui.selectable_value(link, Some(*id), name);
                }
            });
    });
    ui.add_space(8.0);
}

/// Counts for the heading: (angefragt, abgegeben, kein Angebot, ausgeschlossen).
pub fn counts(project: &Project) -> [usize; 4] {
    let mut counts = [0; 4];
    for invitee in project.invitees() {
        let slot = InviteStatus::ALL
            .iter()
            .position(|status| *status == invitee.status)
            .unwrap_or(0);
        counts[slot] += 1;
    }
    counts
}

/// Text of the question before removing `id` from the Bieterverzeichnis:
/// what goes and what stays (linked bid, filed letters).
///
/// The linked bid stays and is only unlinked: it is a document received in
/// the procedure (Wertung, Preisspiegel, maybe the contract) – removing it is
/// its own step („Angebot entfernen …“). Filed letters stay in the
/// Schriftverkehr: they show what was sent to whom.
pub fn remove_question(project: &Project, id: InviteeId) -> Option<String> {
    let invitee = project.invitees().iter().find(|invitee| invitee.id == id)?;
    let name = if invitee.name().is_empty() {
        "Der Bieter ohne Namen".to_string()
    } else {
        format!("„{}“", invitee.name())
    };
    let mut text = format!("{name} aus dem Bieterverzeichnis dieses LV entfernen?");
    if let Some(bid) = invitee.bid {
        let awarded = project
            .bids()
            .iter()
            .find(|known| known.id() == bid)
            .is_some_and(|known| known.is_awarded());
        text.push_str(&format!(
            "\n\nDas verknüpfte Angebot ({}) bleibt erhalten und wird nur gelöst – es steht weiter unter \
             Angebote und im Preisspiegel{}.",
            bid_label(project, bid),
            if awarded { ", auch der Auftrag bleibt" } else { "" }
        ));
    }
    let number = project.lv().number();
    let filed = project
        .correspondence()
        .iter()
        .filter(|entry| entry.recipient == Some(id) && entry.lv_number == number)
        .count();
    if filed > 0 {
        let letters = if filed == 1 {
            "Ein abgelegtes Schreiben an diesen Bieter bleibt".to_string()
        } else {
            format!("{filed} abgelegte Schreiben an diesen Bieter bleiben")
        };
        text.push_str(&format!(
            "\n\n{letters} im Schriftverkehr des Projekts (Nachweis, was verschickt wurde)."
        ));
    }
    text.push_str("\n\nStrg+Z holt den Eintrag zurück.");
    Some(text)
}

/// Tooltip of the tool buttons without a selection.
const CHOOSE_FIRST: &str = "Erst einen Bieter wählen";

const ROW_H: f32 = 30.0;
const HEAD_H: f32 = 24.0;
/// Fixed columns: Nr., Ort, Angefordert, Status, Angebot, Eingang, NA; the
/// name takes the rest.
const COLUMN_WIDTHS: [f32; 7] = [40.0, 140.0, 96.0, 172.0, 270.0, 140.0, 40.0];
const NAME_MIN: f32 = 200.0;

#[derive(Default)]
pub struct BiddersView {
    selected: Option<InviteeId>,
    collapsed: bool,
    /// A bidder was just added or opened: its name field takes the focus.
    focus_name: bool,
    /// Bring the detail form into view (after a double click, „Bearbeiten“).
    show_detail: bool,
    /// „Entfernen …“ asked for this bidder.
    confirm_remove: Option<InviteeId>,
    keys: crate::list_keys::ListFocus,
}

enum Change {
    Add(Address),
    Remove(InviteeId),
    Move(InviteeId, bool),
    Link(InviteeId, Option<BidId>),
}

impl BiddersView {
    pub fn new() -> Self {
        Self::default()
    }

    /// The selected bidder (Bieterverzeichnis).
    pub fn selected(&self) -> Option<InviteeId> {
        self.selected
    }

    /// Height the area wants above the bids: heading, tool row, table, detail
    /// form – at most `share` of the available height (the list scrolls).
    pub fn desired_height(&self, project: &Project, available: f32, share: f32) -> f32 {
        let wanted = if self.collapsed {
            40.0
        } else if project.invitees().is_empty() {
            110.0
        } else {
            let rows = project.invitees().len() as f32;
            let detail = if self.selected.is_some() { 400.0 } else { 0.0 };
            84.0 + HEAD_H + rows * ROW_H + detail
        };
        wanted.min((available * share).max(110.0))
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
    ) -> ViewResponse {
        let mut response = ViewResponse::default();
        let mut changes = Vec::new();
        let area = ui.max_rect();
        if self
            .selected
            .is_some_and(|id| !project.invitees().iter().any(|invitee| invitee.id == id))
        {
            self.selected = None;
        }
        let [requested, submitted, no_bid, excluded] = counts(project);

        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new(
                    RichText::new(format!("Bieterverzeichnis ({})", project.invitees().len()))
                        .size(15.0)
                        .strong()
                        .color(palette.text),
                ).frame(false))
                .on_hover_text(if self.collapsed { "Aufklappen" } else { "Zuklappen" })
                .clicked()
            {
                self.collapsed = !self.collapsed;
            }
            ui.label(
                RichText::new(format!(
                    "{requested} angefragt · {submitted} abgegeben · {no_bid} kein Angebot · {excluded} ausgeschlossen"
                ))
                .size(12.5)
                .color(palette.text_dim),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("Niederschrift …")
                    .on_hover_text("Submissionsprotokoll mit Eingang, Summen, Nachlass, Nebenangeboten und Unterschriftsfeldern")
                    .clicked()
                {
                    response.requests.push(ViewRequest::ExportSubmission);
                }
                let filed = project.correspondence().len();
                if ui
                    .button(format!("Abgelegt ({filed}) …"))
                    .on_hover_text("Schriftverkehr im Projekt: die abgelegten Schreiben als PDF")
                    .clicked()
                {
                    response.requests.push(ViewRequest::FiledLetters(None));
                }
                if ui
                    .button("Schriftverkehr …")
                    .on_hover_text("Aufforderung zur Angebotsabgabe, Zuschlags- und Absageschreiben an die Bieter")
                    .clicked()
                {
                    response.requests.push(ViewRequest::Letters);
                }
            });
        });
        if self.collapsed {
            self.apply(changes, project, &mut response);
            return response;
        }
        ui.add_space(4.0);
        self.tool_row(ui, palette, project, &mut changes);
        ui.add_space(6.0);

        if project.invitees().is_empty() {
            ui.label(
                RichText::new(
                    "Noch keine angefragten Bieter. Mit „Bieter hinzufügen“ die Firmen erfassen, die die \
                     Unterlagen bekommen – eingelesene Angebote werden ihnen dann zugeordnet (beim Einlesen \
                     auch automatisch).",
                )
                .size(12.5)
                .color(palette.text_dim),
            );
        } else {
            egui::ScrollArea::vertical()
                .id_salt("bidders_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    self.table(ui, palette, project, &mut changes, &mut response);
                    if let Some(id) = self.selected {
                        ui.add_space(8.0);
                        self.detail(ui, palette, project, id, &mut changes, &mut response);
                    }
                });
        }
        self.keyboard(ui, project, &mut changes);
        self.keys.track(ui.ctx(), area);
        self.confirm(ui, palette, project, &mut changes);
        self.apply(changes, project, &mut response);
        response
    }

    /// Tool row above the list: add, edit, remove, move – the last ones
    /// greyed out without a selection (Anwender 18.11.).
    fn tool_row(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &Project,
        changes: &mut Vec<Change>,
    ) {
        let selected = self.selected;
        let chosen = selected.is_some();
        let index = selected.and_then(|id| {
            project
                .invitees()
                .iter()
                .position(|invitee| invitee.id == id)
        });
        let last = project.invitees().len().saturating_sub(1);
        ui.horizontal(|ui| {
            if widgets::icon_label_button(ui, palette, Icon::Plus, "Bieter hinzufügen")
                .on_hover_text("Neue Firma im Bieterverzeichnis dieses LV")
                .clicked()
            {
                changes.push(Change::Add(Address::default()));
            }
            let choices = address_choices(project);
            ui.add_enabled_ui(!choices.is_empty(), |ui| {
                crate::menus::menu(ui, "Aus Projekt-Adressen …", |ui| {
                    for choice in &choices {
                        if ui.button(&choice.label).clicked() {
                            changes.push(Change::Add(choice.address.clone()));
                            ui.close();
                        }
                    }
                })
                .response
                .on_disabled_hover_text(
                    "Keine weiteren Adressen in den Stammdaten oder anderen LVs",
                );
            });
            ui.add_space(12.0);
            if widgets::list_button(ui, palette, None, "Bearbeiten", chosen, CHOOSE_FIRST)
                .on_hover_text("Angaben des gewählten Bieters bearbeiten (Doppelklick, Enter)")
                .clicked()
            {
                self.open_detail();
            }
            if widgets::list_button(
                ui,
                palette,
                Some(Icon::Trash),
                "Entfernen …",
                chosen,
                CHOOSE_FIRST,
            )
            .on_hover_text(
                "Gewählten Bieter aus dem Bieterverzeichnis entfernen (Entf) – mit Rückfrage",
            )
            .clicked()
            {
                self.confirm_remove = selected;
            }
            if widgets::list_button(
                ui,
                palette,
                Some(Icon::Up),
                "Nach oben",
                index.is_some_and(|index| index > 0),
                CHOOSE_FIRST,
            )
            .clicked()
            {
                if let Some(id) = selected {
                    changes.push(Change::Move(id, true));
                }
            }
            if widgets::list_button(
                ui,
                palette,
                Some(Icon::Down),
                "Nach unten",
                index.is_some_and(|index| index < last),
                CHOOSE_FIRST,
            )
            .clicked()
            {
                if let Some(id) = selected {
                    changes.push(Change::Move(id, false));
                }
            }
        });
    }

    /// Selects `id` and brings its form into view with the name focused.
    fn open_detail(&mut self) {
        if self.selected.is_some() {
            self.focus_name = true;
            self.show_detail = true;
        }
    }

    fn keyboard(&mut self, ui: &egui::Ui, project: &Project, changes: &mut [Change]) {
        if self.confirm_remove.is_some() || !changes.is_empty() {
            return;
        }
        let Some(key) = self.keys.key(ui) else {
            return;
        };
        let ids: Vec<InviteeId> = project
            .invitees()
            .iter()
            .map(|invitee| invitee.id)
            .collect();
        match key {
            ListKey::Up | ListKey::Down => {
                if let Some(next) =
                    crate::list_keys::step(&ids, self.selected, key == ListKey::Down)
                {
                    self.selected = Some(next);
                }
            }
            ListKey::Delete => self.confirm_remove = self.selected,
            ListKey::Enter => self.open_detail(),
        }
    }

    /// The question before removing a bidder.
    fn confirm(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &Project,
        changes: &mut Vec<Change>,
    ) {
        let Some(id) = self.confirm_remove else {
            return;
        };
        let Some(question) = remove_question(project, id) else {
            self.confirm_remove = None;
            return;
        };
        let ((cancelled, confirmed), close) = widgets::modal_with_id(
            ui,
            palette,
            egui::Id::unique("proava_remove_bidder"),
            520.0,
            |ui| {
                widgets::title(ui, palette, "Bieter entfernen", "");
                ui.label(RichText::new(question).size(13.0).color(palette.text));
                ui.add_space(14.0);
                widgets::danger_dialog_buttons(ui, palette, "Entfernen")
            },
        );
        let enter = ui.ctx().input(|input| input.key_pressed(egui::Key::Enter));
        if confirmed || enter {
            changes.push(Change::Remove(id));
            self.confirm_remove = None;
        } else if cancelled || close {
            self.confirm_remove = None;
        }
    }

    fn table(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        changes: &mut Vec<Change>,
        response: &mut ViewResponse,
    ) {
        let bids: Vec<(BidId, String)> = project
            .bids()
            .iter()
            .map(|bid| (bid.id(), bid_label(project, bid.id())))
            .collect();
        let rows: Vec<InvitedBidder> = project.invitees().to_vec();
        let scroll = self.keys.take_scroll();
        let fixed: f32 = COLUMN_WIDTHS.iter().sum();
        let name_w = (ui.available_width() - fixed - 12.0).max(NAME_MIN);
        let widths = [
            COLUMN_WIDTHS[0],
            name_w,
            COLUMN_WIDTHS[1],
            COLUMN_WIDTHS[2],
            COLUMN_WIDTHS[3],
            COLUMN_WIDTHS[4],
            COLUMN_WIDTHS[5],
            COLUMN_WIDTHS[6],
        ];
        let total: f32 = widths.iter().sum::<f32>() + 12.0;
        let lefts = |left: f32| {
            let mut x = left + 6.0;
            widths.map(|width| {
                let at = x;
                x += width;
                at
            })
        };

        let (head, _) = ui.allocate_exact_size(egui::vec2(total, HEAD_H), egui::Sense::hover());
        let painter = ui.painter().clone();
        for (index, title) in [
            "Nr.",
            "Bieter",
            "Ort",
            "Angefordert",
            "Status",
            "Angebot",
            "Eingang",
            "NA",
        ]
        .into_iter()
        .enumerate()
        {
            painter.text(
                egui::pos2(lefts(head.left())[index], head.center().y),
                egui::Align2::LEFT_CENTER,
                title,
                crate::fonts::bold(12.0),
                palette.text_dim,
            );
        }
        painter.line_segment(
            [head.left_bottom(), head.right_bottom()],
            egui::Stroke::new(1.0, palette.line),
        );

        for (number, invitee) in rows.iter().enumerate() {
            let (rect, row) =
                ui.allocate_exact_size(egui::vec2(total, ROW_H), egui::Sense::click());
            let name = if invitee.name().is_empty() {
                "(ohne Namen)"
            } else {
                invitee.name()
            };
            let selected = self.selected == Some(invitee.id);
            widgets::name_row(&row, name, selected);
            let fill = if selected {
                palette.selected
            } else if row.hovered() {
                palette.row_hover
            } else if number % 2 == 1 {
                palette.row_alt
            } else {
                egui::Color32::TRANSPARENT
            };
            painter.rect_filled(rect, egui::CornerRadius::same(4), fill);
            if selected {
                widgets::selection_bar(&painter, palette, rect);
                if scroll {
                    ui.scroll_to_rect(rect, None);
                }
            }
            let x = lefts(rect.left());
            let middle = rect.center().y;
            let write = |index: usize, text: &str, color: egui::Color32| {
                crate::comparison_view::text_fit(
                    &painter,
                    egui::pos2(x[index], middle),
                    egui::Align2::LEFT_CENTER,
                    text,
                    egui::FontId::proportional(12.5),
                    color,
                    widths[index] - 10.0,
                );
            };
            write(0, &(number + 1).to_string(), palette.text_dim);
            write(
                1,
                name,
                if invitee.name().is_empty() {
                    palette.text_dim
                } else {
                    palette.text
                },
            );
            write(2, &invitee.address.city, palette.text);
            write(3, &invitee.requested, palette.text);
            write(6, &invitee.received, palette.text);
            write(7, if invitee.side_offers { "ja" } else { "" }, palette.text);

            if row.clicked() || row.secondary_clicked() {
                self.selected = Some(invitee.id);
                self.keys.take();
            }
            if row.double_clicked() {
                self.selected = Some(invitee.id);
                self.open_detail();
            }
            row.context_menu(|ui| {
                self.selected = Some(invitee.id);
                self.keys.take();
                if ui.button("Bearbeiten").clicked() {
                    self.open_detail();
                    ui.close();
                }
                let index = number;
                if ui
                    .add_enabled(index > 0, egui::Button::new("Nach oben"))
                    .clicked()
                {
                    changes.push(Change::Move(invitee.id, true));
                    ui.close();
                }
                if ui
                    .add_enabled(index + 1 < rows.len(), egui::Button::new("Nach unten"))
                    .clicked()
                {
                    changes.push(Change::Move(invitee.id, false));
                    ui.close();
                }
                if invitee.bid.is_some() && ui.button("Angebot lösen").clicked() {
                    changes.push(Change::Link(invitee.id, None));
                    ui.close();
                }
                if ui.button("Abgelegte Schreiben …").clicked() {
                    response
                        .requests
                        .push(ViewRequest::FiledLetters(Some(invitee.name().to_string())));
                    ui.close();
                }
                ui.separator();
                if ui.button("Entfernen …").clicked() {
                    self.confirm_remove = Some(invitee.id);
                    ui.close();
                }
            });

            // Status and Angebot stay choices in the row (on top of the row).
            let cell = |index: usize| {
                egui::Rect::from_min_size(
                    egui::pos2(x[index], rect.top() + 3.0),
                    egui::vec2(widths[index] - 8.0, rect.height() - 6.0),
                )
            };
            let mut status = invitee.status;
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(cell(4))
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            egui::ComboBox::from_id_salt(("bidder_status", invitee.id.0))
                .selected_text(status.label())
                .width(widths[4] - 12.0)
                .show_ui(&mut child, |ui| {
                    for choice in InviteStatus::ALL {
                        ui.selectable_value(&mut status, choice, choice.label());
                    }
                });
            if status != invitee.status {
                if let Some(target) = project.invitee_mut(invitee.id) {
                    target.status = status;
                    response.dirty = true;
                }
            }
            let mut linked = invitee.bid;
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(cell(5))
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            egui::ComboBox::from_id_salt(("bidder_bid", invitee.id.0))
                .selected_text(
                    linked
                        .map(|bid| bid_label(project, bid))
                        .unwrap_or_else(|| "–".into()),
                )
                .width(widths[5] - 12.0)
                .show_ui(&mut child, |ui| {
                    ui.selectable_value(&mut linked, None, "– kein Angebot verknüpft –");
                    for (id, label) in &bids {
                        ui.selectable_value(&mut linked, Some(*id), label);
                    }
                });
            if linked != invitee.bid {
                changes.push(Change::Link(invitee.id, linked));
            }
        }
    }

    fn detail(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        id: InviteeId,
        changes: &mut Vec<Change>,
        response: &mut ViewResponse,
    ) {
        let Some(before) = project
            .invitees()
            .iter()
            .find(|invitee| invitee.id == id)
            .cloned()
        else {
            return;
        };
        let mut edited = before.clone();
        let same_name = before
            .bid
            .is_none()
            .then(|| matching_bid(project, &before.address.name1))
            .flatten();
        let frame = egui::Frame::new()
            .fill(palette.panel)
            .stroke(egui::Stroke::new(1.0, palette.line))
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "Bieter bearbeiten: {}",
                            if before.name().is_empty() {
                                "(ohne Namen)"
                            } else {
                                before.name()
                            }
                        ))
                        .strong()
                        .color(palette.text),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::list_button(
                            ui,
                            palette,
                            Some(Icon::Trash),
                            "Entfernen …",
                            true,
                            "",
                        )
                        .on_hover_text(
                            "Diesen Bieter aus dem Bieterverzeichnis entfernen – mit Rückfrage",
                        )
                        .clicked()
                        {
                            self.confirm_remove = Some(id);
                        }
                    });
                });
                if let Some((bid, name)) = &same_name {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "Ein eingelesenes Angebot heißt ebenso: „{name}“."
                            ))
                            .size(12.5)
                            .color(palette.text_dim),
                        );
                        if ui.button("Angebot verknüpfen").clicked() {
                            changes.push(Change::Link(id, Some(*bid)));
                        }
                    });
                }
                ui.add_space(6.0);
                form_kit::split(
                    ui,
                    0.5,
                    |ui| {
                        form_kit::address_with_focus(
                            ui,
                            palette,
                            &mut edited.address,
                            &mut self.focus_name,
                        );
                    },
                    |ui| {
                        form_kit::field(
                            ui,
                            palette,
                            "Ansprechpartner",
                            &mut edited.contact_person,
                            None,
                        );
                        let today = crate::date_picker::today(ui.ctx());
                        let requested_error =
                            proava_domain::GermanDate::complete(&edited.requested, today)
                                .is_none()
                                .then(|| proava_domain::optional_date(&edited.requested).err())
                                .flatten();
                        form_kit::date_field(
                            ui,
                            palette,
                            "Unterlagen angefordert am",
                            &mut edited.requested,
                            requested_error.as_deref(),
                        );
                        form_kit::field_with(
                            ui,
                            palette,
                            "Eingang des Angebots",
                            &mut edited.received,
                            None,
                            "TT.MM.JJJJ, HH:MM Uhr",
                        );
                        ui.checkbox(&mut edited.side_offers, "Nebenangebote abgegeben");
                        ui.add_space(6.0);
                        form_kit::multiline(
                            ui,
                            palette,
                            "Bemerkungen (für die Niederschrift)",
                            &mut edited.remark,
                            2,
                        );
                        ui.add_space(4.0);
                        form_kit::multiline(
                            ui,
                            palette,
                            "Absagegrund (Absageschreiben, Platzhalter {absagegrund})",
                            &mut edited.rejection_reason,
                            2,
                        );
                    },
                );
            });
        if std::mem::take(&mut self.show_detail) {
            ui.scroll_to_rect(frame.response.rect, Some(egui::Align::TOP));
        }
        if edited != before {
            if let Some(target) = project.invitee_mut(id) {
                *target = edited;
                response.dirty = true;
            }
        }
    }

    fn apply(&mut self, changes: Vec<Change>, project: &mut Project, response: &mut ViewResponse) {
        for change in changes {
            match change {
                Change::Add(address) => {
                    let id = project.add_invitee(InvitedBidder::new(address));
                    self.selected = Some(id);
                    self.focus_name = true;
                    self.collapsed = false;
                    self.keys.take();
                    response.label = Some("Bieter hinzufügen");
                }
                Change::Remove(id) => {
                    let ids: Vec<InviteeId> = project
                        .invitees()
                        .iter()
                        .map(|invitee| invitee.id)
                        .collect();
                    let index = ids.iter().position(|known| *known == id);
                    if let Some(removed) = project.remove_invitee(id) {
                        let name = if removed.name().is_empty() {
                            "Bieter ohne Namen".to_string()
                        } else {
                            format!("„{}“", removed.name())
                        };
                        response.toast = Some(format!(
                            "{name} aus dem Bieterverzeichnis entfernt – Strg+Z holt ihn zurück."
                        ));
                        response.label = Some("Bieter entfernen");
                        // The neighbour keeps the selection (like the LV table).
                        let rest: Vec<InviteeId> =
                            ids.into_iter().filter(|known| *known != id).collect();
                        self.selected = index.and_then(|index| {
                            rest.get(index.min(rest.len().saturating_sub(1))).copied()
                        });
                    }
                }
                Change::Move(id, up) => {
                    project.move_invitee(id, up);
                    response.label = Some("Bieter verschieben");
                }
                Change::Link(id, bid) => {
                    let result = match bid {
                        Some(bid) => project.link_bid(bid, Some(id)).map(|()| {
                            // Empty address fields from the bid (Ort, Straße …).
                            let from = project
                                .bids()
                                .iter()
                                .find(|known| known.id() == bid)
                                .map(|known| known.bidder().clone());
                            if let (Some(from), Some(invitee)) = (from, project.invitee_mut(id)) {
                                fill_empty_fields(&mut invitee.address, &from);
                            }
                        }),
                        None => {
                            let current = project
                                .invitees()
                                .iter()
                                .find(|invitee| invitee.id == id)
                                .and_then(|invitee| invitee.bid);
                            match current {
                                Some(current) => project.link_bid(current, None),
                                None => Ok(()),
                            }
                        }
                    };
                    if let Err(error) = result {
                        response.toast = Some(error);
                    }
                }
            }
            response.dirty = true;
        }
    }
}

/// An unlinked bid whose bidder has this name (case and spaces ignored).
fn matching_bid(project: &Project, name: &str) -> Option<(BidId, String)> {
    let key = |text: &str| {
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let wanted = key(name);
    if wanted.is_empty() {
        return None;
    }
    let linked: Vec<BidId> = project
        .invitees()
        .iter()
        .filter_map(|invitee| invitee.bid)
        .collect();
    project
        .bids()
        .iter()
        .filter(|bid| !linked.contains(&bid.id()))
        .find(|bid| key(bid.name()) == wanted)
        .map(|bid| (bid.id(), bid.name().to_string()))
}

/// Takes the fields of `from` where `address` has none.
fn fill_empty_fields(address: &mut Address, from: &Address) {
    for (field, value) in [
        (&mut address.name2, &from.name2),
        (&mut address.street, &from.street),
        (&mut address.postal_code, &from.postal_code),
        (&mut address.city, &from.city),
        (&mut address.phone, &from.phone),
        (&mut address.email, &from.email),
    ] {
        if field.trim().is_empty() && !value.trim().is_empty() {
            field.clone_from(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use proava_domain::{Address, Bid, Contact, InvitedBidder, Lv, OzMask, Project, ProjectName};

    use super::{address_choices, counts, remove_question};

    /// Anwender 18.11.: the question names the linked bid (stays, only
    /// unlinked, also when awarded) – nothing else for a plain entry.
    #[test]
    fn remove_question_names_what_stays() {
        let lv = Lv::new("LV", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        let mut project = Project::new(ProjectName::parse("P").unwrap(), "2026-11-18", lv);
        let plain = project.add_invitee(InvitedBidder::new(Address::named("Ohne GmbH")));
        let linked = project.add_invitee(InvitedBidder::new(Address::named("Mit GmbH")));
        let bid = project.add_bid(Bid::new(Address::named("Mit GmbH"), "m.x84"));
        project.link_bid(bid, Some(linked)).unwrap();
        project.award_bid(bid, "18.11.2026").unwrap();

        let text = remove_question(&project, plain).unwrap();
        assert_eq!(
            text,
            "„Ohne GmbH“ aus dem Bieterverzeichnis dieses LV entfernen?\n\nStrg+Z holt den Eintrag zurück."
        );
        let text = remove_question(&project, linked).unwrap();
        assert!(
            text.contains("bleibt erhalten und wird nur gelöst")
                && text.contains("auch der Auftrag bleibt"),
            "{text}"
        );
        assert!(!text.contains("Schreiben"), "keine Schreiben: {text}");
    }

    #[test]
    fn choices_from_contacts_and_other_lvs_without_doubles() {
        let mask = || OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap();
        let mut project = Project::new(
            ProjectName::parse("P").unwrap(),
            "2026",
            Lv::new("GA", mask()),
        );
        project.info_mut().contacts.push(Contact {
            role: "Architekt".into(),
            address: Address::named("Büro Plan"),
        });
        project.add_invitee(InvitedBidder::new(Address::named("Müller GmbH")));
        let second = project.add_lv(Lv::new("Elektro", mask()));
        project.set_active_lv(second);
        project.add_invitee(InvitedBidder::new(Address::named("Schmidt KG")));
        let choices = address_choices(&project);
        let labels: Vec<&str> = choices.iter().map(|choice| choice.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["Müller GmbH (Bieter in LV 1)", "Büro Plan (Architekt)"]
        );
        project.add_invitee(InvitedBidder::new(Address::named("müller gmbh")));
        assert_eq!(address_choices(&project).len(), 1, "schon auf der Liste");
        let bid = project.add_bid(Bid::new(Address::named("Schmidt KG"), "s.x84"));
        let schmidt = project.invitees()[0].id;
        project.link_bid(bid, Some(schmidt)).unwrap();
        assert_eq!(counts(&project), [1, 1, 0, 0]);
    }

    #[test]
    fn read_bids_get_their_invited_bidder_once() {
        use super::proposed_links;
        use proava_domain::BidId;
        let mut project = Project::new(
            ProjectName::parse("P").unwrap(),
            "2026",
            Lv::new("GA", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap()),
        );
        let mueller =
            project.add_invitee(InvitedBidder::new(Address::named("Müller Elektro GmbH")));
        let schmidt =
            project.add_invitee(InvitedBidder::new(Address::named("Schmidt Regeltechnik")));
        let links = proposed_links(
            &project,
            &[
                (BidId(1), "Schmidt Regeltechnik GmbH".into()),
                (BidId(2), "Müller-Elektro".into()),
                (BidId(3), "Müller Elektro".into()),
                (BidId(4), "Bieter 4".into()),
            ],
        );
        assert_eq!(links, vec![Some(schmidt), Some(mueller), None, None]);
    }
}
