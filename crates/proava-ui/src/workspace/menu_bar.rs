//! Die Menüleiste als Liste ([`layout`]): Projekt · Bearbeiten · Einfügen ·
//! LV · Prüfen · GAEB · Angebote · Kosten · Drucken · Ansicht · Extras ·
//! Hilfe (Anwender 18.11.). Jeder Befehl steht genau einmal darin – der Test
//! unten prüft es an derselben Liste, aus der die Menüs gezeichnet werden.
//! Kontextmenüs und Knöpfe dürfen Befehle wiederholen, die Menüleiste nicht.

use eframe::egui::{self, FontId, RichText};
use proava_application::{GaebExport, PanelSide, PrintKind};
use proava_domain::{EditStatus, RowKind};

use super::{CostCommand, LibraryCommand, ShellAction, Tab, TrackingCommand, WorkspaceScreen};
use crate::columns::Column;
use crate::lv_editor::{LvAction, StructCommand};
use crate::lv_list::LvListAction;
use crate::lv_ops::Command;
use crate::status_view::StatusFilter;
use crate::{lv_properties, print_dialog, strings};

/// One entry of the menu bar.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MenuItem {
    /// A command with its name and key (as in the command search).
    Entry(ShellAction),
    /// A command under a shorter name in its menu („Importieren …“ in GAEB).
    Named(&'static str, ShellAction),
    /// On/off with a check mark (Ansicht).
    Check(&'static str, ShellAction),
    /// One of several choices (Eigenschaftenfenster links/rechts).
    Choice(&'static str, ShellAction),
    Separator,
    Sub(&'static str, Vec<MenuItem>),
    /// Gehe zu LV: one entry per LV of the project (jumps to its head row).
    SwitchLv,
    /// Hilfe: Handbuch (PDF), Tastenkürzel, Über proAVA (windows of their own).
    Handbook,
    Shortcuts,
    About,
}

use MenuItem::{Check, Choice, Entry, Named, Separator, Sub};

/// The menus from left to right with their entries; `active` = index of the
/// LV shown (the LV commands act on it).
pub(crate) fn layout(active: usize) -> Vec<(&'static str, Vec<MenuItem>)> {
    let lv = |action: LvListAction| Entry(ShellAction::Lv(action));
    let editor = |command: Command| Entry(ShellAction::Editor(command));
    let structure = |command: StructCommand| Entry(ShellAction::Structure(command));
    let mut print = vec![Entry(ShellAction::PrintDialog), Separator];
    for kind in PrintKind::ALL {
        if kind == PrintKind::PriceComparison || kind == PrintKind::BillingCheck {
            print.push(Separator);
        }
        // Every kind opens the print dialog with it chosen.
        print.push(Entry(ShellAction::Print(kind)));
    }
    let mut costs: Vec<MenuItem> = Vec::new();
    for command in CostCommand::ALL {
        // Printing is under Drucken („Kostenschätzung / Kostenberechnung“).
        if command == CostCommand::Print {
            continue;
        }
        // Kurztext-LV first, then the Datenpunktrechnung of the GA.
        if command == CostCommand::New(proava_domain::CostStage::Estimate) {
            costs.push(Separator);
        }
        costs.push(Entry(ShellAction::Costs(command)));
    }
    costs.push(Separator);
    costs.extend(
        TrackingCommand::ALL
            .into_iter()
            .map(|command| Entry(ShellAction::Tracking(command))),
    );
    let mut convert: Vec<MenuItem> = RowKind::ALL
        .into_iter()
        .map(|kind| {
            Named(
                kind.label(),
                ShellAction::Structure(StructCommand::ConvertTo(kind)),
            )
        })
        .collect();
    convert.extend([
        Separator,
        Named(
            crate::lv_editor::FREE_ENTRY,
            ShellAction::Structure(StructCommand::ToFree),
        ),
        // GU-Fall → Kurztext (only in a Kosten-LV).
        editor(Command::PointSectionToKurztext),
    ]);
    let mut status = vec![editor(Command::ToggleDone)];
    status.extend(
        [EditStatus::Done, EditStatus::InProgress, EditStatus::Open]
            .map(|status| editor(Command::SetStatus(status))),
    );
    status.push(Separator);
    status.extend(StatusFilter::ALL.map(|filter| editor(Command::Filter(filter))));
    status.extend([
        Separator,
        editor(Command::ToggleGapMarks),
        editor(Command::ConfirmPriceCheck),
    ]);
    vec![
        (
            strings::MENU_PROJECT,
            vec![
                Entry(ShellAction::Save),
                Entry(ShellAction::MasterData),
                Entry(ShellAction::Backup),
                Entry(ShellAction::SaveAsTemplate),
                Separator,
                Entry(ShellAction::OpenOther),
                Entry(ShellAction::OpenReadOnly),
                Entry(ShellAction::NewWindow),
                Separator,
                Entry(ShellAction::Close),
            ],
        ),
        (
            strings::MENU_EDIT,
            vec![
                Entry(ShellAction::Undo),
                Entry(ShellAction::Redo),
                Separator,
                editor(Command::Find),
                editor(Command::FindReplace),
                Separator,
                // Zeilenart der gewählten Zeile (wie Rechtsklick ▸ Bearbeiten).
                Sub("Umwandeln in", convert),
                Sub("Bearbeitungsstatus", status),
                Separator,
                editor(Command::CollapseAll),
                editor(Command::ExpandAll),
            ],
        ),
        (
            strings::MENU_INSERT,
            vec![
                // As the right click ▸ Einfügen, with the same names.
                editor(Command::NewPosition),
                editor(Command::NewPositionBefore),
                Separator,
                editor(Command::NewTitle),
                structure(StructCommand::NewSubtitle),
                Separator,
                editor(Command::NewNote),
                structure(StructCommand::NewNoteBefore),
                editor(Command::NewSpacer),
                structure(StructCommand::NewPerformance),
                // Only in a Kosten-LV (GU-Fall).
                editor(Command::NewSplit),
                editor(Command::NewPointsTemplate),
                editor(Command::NewPointSection),
                Separator,
                editor(Command::FromStlb),
                editor(Command::FromAusschreiben),
                structure(StructCommand::Lv(LvAction::Transfer)),
            ],
        ),
        (
            strings::MENU_LV,
            vec![
                lv(LvListAction::New),
                lv(LvListAction::Properties(active)),
                MenuItem::SwitchLv,
                lv(LvListAction::Duplicate(active)),
                lv(LvListAction::Copy(active)),
                lv(LvListAction::Paste),
                lv(LvListAction::Move {
                    index: active,
                    up: true,
                }),
                lv(LvListAction::Move {
                    index: active,
                    up: false,
                }),
                lv(LvListAction::Delete(active)),
                Separator,
                Entry(ShellAction::Mask),
                editor(Command::Renumber),
                Separator,
                structure(StructCommand::Lv(LvAction::Compare)),
                // „Bibliothek ein-/ausblenden“ is Ansicht ▸ Bibliothek anzeigen.
                Sub(
                    "Bibliothek",
                    [
                        LibraryCommand::Store,
                        LibraryCommand::ImportGaeb,
                        LibraryCommand::Edit,
                        LibraryCommand::Sync,
                    ]
                    .map(|command| Entry(ShellAction::Library(command)))
                    .to_vec(),
                ),
            ],
        ),
        (
            strings::MENU_REVIEW,
            vec![
                // The only place of „LV prüfen …“ (the export checks anyway).
                Entry(ShellAction::Check),
                Entry(ShellAction::CheckAll),
                editor(Command::SpellCheck),
                editor(Command::NextMissingQuantity),
                editor(Command::OpenPoints),
                editor(Command::CostGroups),
                editor(Command::AllBlankNotesToSpacers),
                Separator,
                Entry(ShellAction::StlbCheckFile),
                Entry(ShellAction::StlbUpdate),
            ],
        ),
        (
            strings::MENU_GAEB,
            vec![
                Named("Importieren …", ShellAction::ImportGaeb),
                Entry(ShellAction::ImportCsv),
                Separator,
                Entry(ShellAction::Export(GaebExport::RequestForQuotation)),
                Entry(ShellAction::Export(GaebExport::PricedLv)),
                Entry(ShellAction::Export(GaebExport::Description)),
                Entry(ShellAction::ExportCsv),
            ],
        ),
        (
            strings::MENU_BIDS,
            vec![
                Entry(ShellAction::Bidders),
                Entry(ShellAction::ImportBids),
                Entry(ShellAction::NewBid),
                Entry(ShellAction::Letters),
                Entry(ShellAction::FiledLetters),
                Separator,
                Entry(ShellAction::Award),
                Entry(ShellAction::RevokeAward),
                Entry(ShellAction::Contract),
                Entry(ShellAction::AdoptPrices),
            ],
        ),
        ("Kosten", costs),
        (strings::MENU_PRINT, print),
        (
            "Ansicht",
            vec![
                Entry(ShellAction::ShowTab(Tab::Lv)),
                Entry(ShellAction::ShowTab(Tab::Bids)),
                Entry(ShellAction::ShowTab(Tab::Comparison)),
                Entry(ShellAction::ShowTab(Tab::Billing)),
                Entry(ShellAction::ShowTab(Tab::ChangeOrders)),
                Entry(ShellAction::ShowTab(Tab::Costs)),
                Entry(ShellAction::ShowTab(Tab::Tracking)),
                Separator,
                Check("Eigenschaftenfenster anzeigen", ShellAction::ToggleDetail),
                Choice(
                    "Eigenschaftenfenster links",
                    ShellAction::DetailSide(PanelSide::Left),
                ),
                Choice(
                    "Eigenschaftenfenster rechts",
                    ShellAction::DetailSide(PanelSide::Right),
                ),
                Separator,
                Check(
                    Column::CostGroup.menu_label(),
                    ShellAction::Editor(Command::ToggleCostGroupColumn),
                ),
                Check(
                    Column::WorkCategory.menu_label(),
                    ShellAction::Editor(Command::ToggleWorkCategoryColumn),
                ),
                Check(
                    crate::lv_editor::TITLE_SHARE_LABEL,
                    ShellAction::Editor(Command::ToggleTitleShare),
                ),
                Check(
                    "Bibliothek anzeigen",
                    ShellAction::Library(LibraryCommand::Toggle),
                ),
                Separator,
                Entry(ShellAction::ToggleTheme),
            ],
        ),
        (
            "Extras",
            vec![
                Entry(ShellAction::Settings),
                Entry(ShellAction::Editor(Command::Palette)),
            ],
        ),
        (
            "Hilfe",
            vec![
                MenuItem::Handbook,
                MenuItem::Shortcuts,
                Separator,
                MenuItem::About,
            ],
        ),
    ]
}

/// What the entries show: greyed out, ticked, chosen.
struct MenuState {
    can_undo: bool,
    can_redo: bool,
    active: usize,
    lv_count: usize,
    lv_labels: Vec<String>,
    side: PanelSide,
    detail_visible: bool,
    cost_group_column: bool,
    work_category_column: bool,
    title_share: bool,
    library_visible: bool,
    tab: Tab,
    printable: Vec<(PrintKind, bool)>,
    awarded: bool,
    /// The active LV is for the tender (not a Kosten-LV).
    tender: bool,
}

impl MenuState {
    fn enabled(&self, action: ShellAction) -> bool {
        match action {
            ShellAction::Undo => self.can_undo,
            ShellAction::Redo => self.can_redo,
            ShellAction::Lv(LvListAction::Move { up: true, .. }) => self.active > 0,
            ShellAction::Lv(LvListAction::Move { up: false, .. }) => {
                self.active + 1 < self.lv_count
            }
            ShellAction::Lv(LvListAction::Delete(_)) => self.lv_count > 1,
            ShellAction::Editor(
                Command::NewPointSection
                | Command::NewSplit
                | Command::NewPointsTemplate
                | Command::PointSectionToKurztext,
            ) => !self.tender,
            action if !self.tender && super::cost_lv_shell::tender_only(action) => false,
            ShellAction::RevokeAward => self.awarded,
            ShellAction::Print(kind) => self
                .printable
                .iter()
                .any(|(which, available)| *which == kind && *available),
            _ => true,
        }
    }

    fn checked(&self, action: ShellAction) -> bool {
        match action {
            ShellAction::ToggleDetail => self.detail_visible,
            ShellAction::DetailSide(side) => self.side == side,
            ShellAction::Editor(Command::ToggleCostGroupColumn) => self.cost_group_column,
            ShellAction::Editor(Command::ToggleWorkCategoryColumn) => self.work_category_column,
            ShellAction::Editor(Command::ToggleTitleShare) => self.title_share,
            ShellAction::Library(LibraryCommand::Toggle) => self.library_visible,
            ShellAction::ShowTab(tab) => self.tab == tab,
            _ => false,
        }
    }
}

impl WorkspaceScreen {
    pub(super) fn menu_bar(&self, ui: &mut egui::Ui, actions: &mut Vec<ShellAction>) {
        let state = MenuState {
            can_undo: self.history.can_undo(self.editor.can_undo()),
            can_redo: self.history.can_redo(self.editor.can_redo()),
            active: self.project.active_lv(),
            lv_count: self.project.lv_count(),
            lv_labels: self.project.lvs().map(lv_properties::lv_label).collect(),
            side: self.launcher.detail_side(),
            detail_visible: self.preferences.detail_visible,
            cost_group_column: self.editor.column_shown(Column::CostGroup),
            work_category_column: self.editor.column_shown(Column::WorkCategory),
            title_share: self.editor.title_share_shown(),
            library_visible: self.launcher.library_panel(),
            tab: self.tab,
            printable: PrintKind::ALL
                .map(|kind| (kind, print_dialog::available(&self.project, kind)))
                .to_vec(),
            awarded: self.project.awarded_bid().is_some(),
            tender: self.project.lv().purpose().is_tender(),
        };
        crate::menus::bar(ui, |ui| {
            ui.style_mut().override_font_id = Some(FontId::proportional(13.5));
            let menus = layout(state.active);
            let shown = menus_that_fit(ui, &menus);
            for (name, items) in &menus[..shown] {
                crate::menus::menu(ui, *name, |ui| show(ui, items, &state, actions));
            }
            if shown < menus.len() {
                // Narrow window: the last menus as submenus of „Weitere“.
                crate::menus::menu(ui, MORE, |ui| {
                    for (name, items) in &menus[shown..] {
                        crate::menus::menu(ui, *name, |ui| show(ui, items, &state, actions));
                    }
                });
            }
        });
    }
}

/// The menu that holds the menus without room in a narrow window.
pub(crate) const MORE: &str = "Weitere";

/// How many menus of the bar fit next to each other; when not all do, one
/// place stays free for [`MORE`].
fn menus_that_fit(ui: &egui::Ui, menus: &[(&'static str, Vec<MenuItem>)]) -> usize {
    let spacing = ui.spacing();
    let width = |name: &str| {
        ui.painter()
            .layout_no_wrap(
                name.to_string(),
                FontId::proportional(13.5),
                egui::Color32::WHITE,
            )
            .size()
            .x
            + 2.0 * spacing.button_padding.x
            + spacing.item_spacing.x
    };
    let available = ui.available_width();
    let total: f32 = menus.iter().map(|(name, _)| width(name)).sum();
    if total <= available {
        return menus.len();
    }
    let mut used = width(MORE);
    menus
        .iter()
        .take_while(|(name, _)| {
            used += width(name);
            used <= available
        })
        .count()
}

fn show(ui: &mut egui::Ui, items: &[MenuItem], state: &MenuState, actions: &mut Vec<ShellAction>) {
    for item in items {
        match item {
            Entry(action) => {
                let action = *action;
                let response = ui.add_enabled_ui(state.enabled(action), |ui| {
                    if let ShellAction::ShowTab(_) = action {
                        // The tab shown is marked.
                        ui.add(
                            egui::Button::selectable(state.checked(action), action.label())
                                .shortcut_text(action.shortcut()),
                        )
                    } else {
                        ui.add(egui::Button::new(action.label()).shortcut_text(action.shortcut()))
                    }
                });
                if response.inner.clicked() {
                    actions.push(action);
                    ui.close();
                }
            }
            Named(label, action) => {
                if ui.button(*label).clicked() {
                    actions.push(*action);
                    ui.close();
                }
            }
            Check(label, action) => {
                if check_entry(ui, state.checked(*action), label, action.shortcut()) {
                    actions.push(*action);
                    ui.close();
                }
            }
            Choice(label, action) => {
                if ui.radio(state.checked(*action), *label).clicked() {
                    actions.push(*action);
                    ui.close();
                }
            }
            Separator => {
                ui.separator();
            }
            Sub(name, items) => {
                crate::menus::menu(ui, *name, |ui| show(ui, items, state, actions));
            }
            MenuItem::SwitchLv => {
                crate::menus::menu(ui, "Gehe zu LV", |ui| {
                    for (index, label) in state.lv_labels.iter().enumerate() {
                        if ui.radio(index == state.active, label).clicked() {
                            actions.push(ShellAction::Lv(LvListAction::Switch(index)));
                            ui.close();
                        }
                    }
                });
            }
            MenuItem::Handbook => {
                if ui.button("Handbuch (PDF)").clicked() {
                    crate::help::request_handbook(ui.ctx());
                    ui.close();
                }
            }
            MenuItem::Shortcuts => {
                if ui
                    .add(egui::Button::new("Tastenkürzel …").shortcut_text("F1"))
                    .clicked()
                {
                    crate::help::request(ui.ctx(), crate::help::Topic::All);
                    ui.close();
                }
            }
            MenuItem::About => {
                if ui.button("Über proAVA …").clicked() {
                    crate::about::request(ui.ctx());
                    ui.close();
                }
            }
        }
    }
}

/// Menu entry with a check mark and the key on the right (like the other
/// entries); true when clicked.
fn check_entry(ui: &mut egui::Ui, checked: bool, label: &str, shortcut: &str) -> bool {
    let mut on = checked;
    let response = if shortcut.is_empty() {
        ui.add(egui::Checkbox::new(&mut on, label))
    } else {
        ui.add(egui::Checkbox::new(
            &mut on,
            (
                label,
                egui::Atom::grow(),
                RichText::new(shortcut).color(ui.visuals().weak_text_color()),
            ),
        ))
    };
    response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every command of the menu bar with its place („Prüfen ▸ LV prüfen …“).
    fn commands() -> Vec<(ShellAction, String, String)> {
        fn walk(path: &str, items: &[MenuItem], out: &mut Vec<(ShellAction, String, String)>) {
            for item in items {
                let (action, label) = match item {
                    Entry(action) => (*action, action.label()),
                    Named(label, action) | Check(label, action) | Choice(label, action) => {
                        (*action, label.to_string())
                    }
                    Sub(name, items) => {
                        walk(&format!("{path} ▸ {name}"), items, out);
                        continue;
                    }
                    _ => continue,
                };
                out.push((
                    same_command(action),
                    label.clone(),
                    format!("{path} ▸ {label}"),
                ));
            }
        }
        let mut out = Vec::new();
        for (name, items) in layout(0) {
            walk(name, &items, &mut out);
        }
        out
    }

    /// Commands that do the same under two names count as one.
    fn same_command(action: ShellAction) -> ShellAction {
        match action {
            ShellAction::Costs(CostCommand::Print) => ShellAction::Print(PrintKind::CostEstimate),
            other => other,
        }
    }

    #[test]
    fn every_command_is_in_the_menu_bar_once() {
        let commands = commands();
        let mut twice = Vec::new();
        for (index, (action, label, place)) in commands.iter().enumerate() {
            for (other, other_label, other_place) in &commands[index + 1..] {
                // Drucken lists the printouts: „Kosten nach Kostengruppen …“
                // there prints what Prüfen ▸ Kosten nach Kostengruppen shows.
                let printout = matches!(action, ShellAction::Print(_))
                    || matches!(other, ShellAction::Print(_));
                if action == other || (label == other_label && !printout) {
                    twice.push(format!("{place} = {other_place}"));
                }
            }
        }
        assert!(twice.is_empty(), "doppelt in der Menüleiste: {twice:#?}");
    }

    #[test]
    fn the_menu_bar_has_the_usual_order_and_lv_check_only_under_pruefen() {
        let names: Vec<&str> = layout(0).into_iter().map(|(name, _)| name).collect();
        assert_eq!(
            names,
            [
                "Projekt",
                "Bearbeiten",
                "Einfügen",
                "LV",
                "Prüfen",
                "GAEB",
                "Angebote",
                "Kosten",
                "Drucken",
                "Ansicht",
                "Extras",
                "Hilfe"
            ]
        );
        let places: Vec<String> = commands()
            .into_iter()
            .filter(|(action, _, _)| *action == ShellAction::Check)
            .map(|(_, _, place)| place)
            .collect();
        assert_eq!(places, ["Prüfen ▸ LV prüfen …"]);
    }
}
