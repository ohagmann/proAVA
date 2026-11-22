//! Right-click menu of the LV table and the structure commands behind it
//! (Untertitel, Hinweis davor, Ausführungsbeschreibung, Ordnungszahl,
//! Neu nummerieren, Verknüpfen, LV-Befehle der Shell).

use eframe::egui::{self, Key, Modifiers};
use proava_domain::{EditStatus, Lv, LvRow, Project, RowId, RowKind};

use super::{Dialog, EditorOutput, Focus, LvEditor};
use crate::lv_ops::{self, Command};
use crate::status_view::StatusFilter;
use crate::struct_ops;
use crate::theme::Palette;
use crate::widgets;

/// Commands of the shell that the LV editor only asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LvAction {
    /// Gliederung und OZ-Maske (Assistent).
    Outline,
    Check,
    NewLv,
    LvProperties,
    /// Aus anderem Projekt / GAEB / Bibliothek übernehmen.
    Transfer,
    /// Die gewählten Zeilen in eine Bibliothek übernehmen (Dialog).
    ToLibrary,
    /// Mit anderem Stand vergleichen.
    Compare,
}

/// Entry of „Umwandeln in“ that removes the STLB-Bau key.
pub const FREE_ENTRY: &str = "Freie Position (ohne STLB-Bau-Kennung) …";

/// Structure commands besides [`Command`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructCommand {
    NewSubtitle,
    NewNoteBefore,
    NewPerformance,
    EditOrdinal,
    Renumber,
    /// Set (Some) or clear (None) the Ausführungsbeschreibung of the selected positions.
    Link(Option<RowId>),
    /// New Ausführungsbeschreibung for the selected positions.
    LinkNew,
    GoToPerformance,
    /// Take the STLB-Bau placeholder mark off the selected positions (the
    /// switch is gone, Anwender 18.11.; only shown at placeholders).
    UnmarkStlb,
    /// Zeilenart der gewählten Zeile ändern (Position, Hinweis, Ausführungsbeschreibung).
    ConvertTo(RowKind),
    /// STLB-Bau-Kennung der gewählten Positionen entfernen: der Langtext wird frei.
    ToFree,
    Lv(LvAction),
}

impl StructCommand {
    pub fn label(self) -> &'static str {
        match self {
            StructCommand::NewSubtitle => "Neuer Untertitel",
            StructCommand::NewNoteBefore => "Neuer Hinweis davor",
            StructCommand::NewPerformance => "Neue Ausführungsbeschreibung",
            StructCommand::EditOrdinal => "Ordnungszahl ändern …",
            StructCommand::Renumber => "Neu nummerieren …",
            StructCommand::Link(None) => "– keine –",
            StructCommand::Link(Some(_)) => "Ausführungsbeschreibung",
            StructCommand::LinkNew => "Neue Ausführungsbeschreibung für die Auswahl",
            StructCommand::GoToPerformance => "Zur Ausführungsbeschreibung springen",
            StructCommand::UnmarkStlb => "Platzhalter-Markierung entfernen",
            StructCommand::ConvertTo(RowKind::Position) => "In Position umwandeln …",
            StructCommand::ConvertTo(RowKind::Note) => "In Hinweis (Text) umwandeln …",
            StructCommand::ConvertTo(RowKind::Performance) => {
                "In Ausführungsbeschreibung umwandeln …"
            }
            StructCommand::ToFree => super::stlb_lock::TO_FREE,
            StructCommand::Lv(LvAction::Outline) => "Gliederung und OZ-Maske …",
            StructCommand::Lv(LvAction::Check) => "LV prüfen …",
            StructCommand::Lv(LvAction::NewLv) => "Neues LV …",
            StructCommand::Lv(LvAction::LvProperties) => "LV-Eigenschaften …",
            StructCommand::Lv(LvAction::Transfer) => "Aus anderem Projekt / GAEB übernehmen …",
            StructCommand::Lv(LvAction::ToLibrary) => crate::strings::LIBRARY_STORE,
            StructCommand::Lv(LvAction::Compare) => "Mit anderem Stand vergleichen …",
        }
    }

    pub fn shortcut(self) -> &'static str {
        match self {
            StructCommand::NewSubtitle => "Umschalt+F12",
            StructCommand::NewNoteBefore => "Strg+F10",
            StructCommand::NewPerformance => "F11",
            StructCommand::EditOrdinal => "Strg+Umschalt+O",
            _ => "",
        }
    }

    /// Keys of the structure commands (they also work while typing).
    pub fn for_key(key: Key, modifiers: Modifiers) -> Option<Self> {
        let only = |command: bool, shift: bool| {
            modifiers.command == command && modifiers.shift == shift && !modifiers.alt
        };
        // Function keys (Anwender 18.11.): F11 Ausführungsbeschreibung,
        // Umschalt+F12 Untertitel, Strg+F10 Hinweis davor (Umschalt+F10 is the
        // context menu).
        match key {
            Key::F11 if only(false, false) => return Some(StructCommand::NewPerformance),
            Key::F12 if only(false, true) => return Some(StructCommand::NewSubtitle),
            Key::F10 if only(true, false) => return Some(StructCommand::NewNoteBefore),
            _ => {}
        }
        if !(modifiers.command && modifiers.shift && !modifiers.alt) {
            return None;
        }
        Some(match key {
            Key::T => StructCommand::NewSubtitle,
            Key::H => StructCommand::NewNoteBefore,
            Key::A => StructCommand::NewPerformance,
            Key::O => StructCommand::EditOrdinal,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuChoice {
    Editor(Command),
    Struct(StructCommand),
}

/// What the menu needs to know, built once per frame.
pub struct MenuState {
    choices: Vec<(RowId, String)>,
    /// Ausführungsbeschreibung all selected positions share (None: none or different).
    current: Option<Option<RowId>>,
    positions_selected: bool,
    multi: bool,
    can_paste: bool,
    can_subtitle: bool,
    /// The selection holds a position (or a title over one) with an open
    /// GP-Prüfung of the import.
    price_check: bool,
    /// The selection holds a hint without text („In Abstandshalter umwandeln“).
    blank_notes: bool,
    /// Only Abstandshalter selected: status, texts, properties, OZ and the
    /// library have nothing to work on – the menu leaves them out.
    only_spacers: bool,
    /// The selection holds an STLB-Bau placeholder („Platzhalter-Markierung entfernen“).
    stlb_placeholders: bool,
    /// The LV is a Kosten-LV („Datenpunkt-Abschnitt einfügen“).
    cost_lv: bool,
    /// Kind of the one selected row that can become another kind
    /// („Umwandeln in“); None for several rows, titles and Abstandshalter.
    kind: Option<RowKind>,
    /// The selection holds a position with an STLB-Bau text („In freie
    /// Position umwandeln“).
    stlb_texts: bool,
}

impl MenuState {
    pub fn of(lv: &Lv, selection: &[usize], can_paste: bool) -> Self {
        let positions: Vec<&proava_domain::Position> = selection
            .iter()
            .filter_map(|&index| lv.rows().get(index).and_then(LvRow::as_position))
            .collect();
        let current = positions
            .first()
            .map(|first| first.performance())
            .filter(|first| {
                positions
                    .iter()
                    .all(|position| position.performance() == *first)
            });
        let can_subtitle = selection.first().is_some_and(|&index| {
            lv.rows()[..=index.min(lv.rows().len().saturating_sub(1))]
                .iter()
                .rev()
                .find_map(LvRow::as_group)
                .is_some_and(|group| group.level() + 1 < lv.level_count())
        });
        let price_check = selection.iter().any(|&index| {
            index < lv.rows().len()
                && lv.block_range(index).any(|row| {
                    lv.rows()[row]
                        .as_position()
                        .is_some_and(|position| position.price_check().is_some())
                })
        });
        let blank_notes = selection
            .iter()
            .any(|&index| lv.rows().get(index).is_some_and(LvRow::is_blank_note));
        let only_spacers = !selection.is_empty()
            && selection
                .iter()
                .all(|&index| lv.rows().get(index).is_some_and(LvRow::is_spacer));
        let stlb_placeholders = positions
            .iter()
            .any(|position| position.is_stlb_placeholder());
        let kind = match selection {
            [index] => lv.rows().get(*index).and_then(RowKind::of),
            _ => None,
        };
        let ids: Vec<RowId> = selection
            .iter()
            .filter_map(|&index| lv.rows().get(index).map(LvRow::id))
            .collect();
        let stlb_texts = !lv.text_locked_positions(&ids).is_empty();
        Self {
            cost_lv: !lv.purpose().is_tender(),
            stlb_texts,
            price_check,
            blank_notes,
            only_spacers,
            stlb_placeholders,
            kind,
            choices: struct_ops::performance_choices(lv),
            current,
            positions_selected: !positions.is_empty(),
            multi: selection.len() > 1,
            can_paste,
            can_subtitle,
        }
    }
}

/// The menu for a row (`row` = None: empty area below the table).
pub fn show(ui: &mut egui::Ui, state: &MenuState, row: Option<&LvRow>) -> Option<MenuChoice> {
    let mut chosen = None;
    let has_row = row.is_some();
    let is_group = row.is_some_and(LvRow::is_group);
    // An Abstandshalter (alone or with others of its kind) has no text, status or OZ.
    let spacers = row.is_some_and(LvRow::is_spacer) && state.only_spacers;
    let numbered = row.is_some_and(LvRow::has_ordinal);

    crate::menus::menu(ui, "Einfügen", |ui| {
        editor(ui, &mut chosen, Command::NewPosition, true);
        editor(ui, &mut chosen, Command::NewPositionBefore, has_row);
        ui.separator();
        editor(ui, &mut chosen, Command::NewTitle, true);
        structure(
            ui,
            &mut chosen,
            StructCommand::NewSubtitle,
            state.can_subtitle,
        );
        ui.separator();
        editor(ui, &mut chosen, Command::NewNote, true);
        structure(ui, &mut chosen, StructCommand::NewNoteBefore, has_row);
        editor(ui, &mut chosen, Command::NewSpacer, true);
        structure(ui, &mut chosen, StructCommand::NewPerformance, true);
        if state.cost_lv {
            editor(ui, &mut chosen, Command::NewSplit, true);
            editor(ui, &mut chosen, Command::NewPointsTemplate, true);
            editor(ui, &mut chosen, Command::NewPointSection, true);
            editor(
                ui,
                &mut chosen,
                Command::PointSectionToKurztext,
                row.and_then(LvRow::as_group)
                    .is_some_and(proava_domain::LvGroup::is_point_section),
            );
        }
        ui.separator();
        editor(ui, &mut chosen, Command::FromStlb, true);
        editor(ui, &mut chosen, Command::FromAusschreiben, true);
        ui.separator();
        structure(ui, &mut chosen, StructCommand::Lv(LvAction::Transfer), true);
    });
    if has_row {
        crate::menus::menu(ui, "Bearbeiten", |ui| {
            editor(ui, &mut chosen, Command::Cut, true);
            editor(ui, &mut chosen, Command::Copy, true);
            editor(ui, &mut chosen, Command::Paste, state.can_paste);
            editor(ui, &mut chosen, Command::Duplicate, !state.multi);
            if !spacers {
                ui.separator();
                editor(ui, &mut chosen, Command::EditShort, !state.multi);
                editor(
                    ui,
                    &mut chosen,
                    Command::EditLong,
                    !state.multi && !is_group,
                );
                editor(ui, &mut chosen, Command::Properties, !state.multi);
                structure(
                    ui,
                    &mut chosen,
                    StructCommand::EditOrdinal,
                    numbered && !state.multi,
                );
            }
            if state.kind.is_some() || state.stlb_texts {
                crate::menus::menu(ui, "Umwandeln in", |ui| {
                    if let Some(current) = state.kind {
                        for kind in RowKind::ALL {
                            if ui
                                .add_enabled(kind != current, egui::Button::new(kind.label()))
                                .clicked()
                            {
                                chosen = Some(MenuChoice::Struct(StructCommand::ConvertTo(kind)));
                            }
                        }
                    }
                    if state.stlb_texts {
                        ui.separator();
                        if ui
                            .button(FREE_ENTRY)
                            .on_hover_text(proava_domain::stlb::STLB_CONVERT_QUESTION)
                            .clicked()
                        {
                            chosen = Some(MenuChoice::Struct(StructCommand::ToFree));
                        }
                    }
                    if state.blank_notes {
                        ui.separator();
                        editor(ui, &mut chosen, Command::NoteToSpacer, true);
                    }
                });
            } else if state.blank_notes {
                editor(ui, &mut chosen, Command::NoteToSpacer, true);
            }
            ui.separator();
            editor(ui, &mut chosen, Command::MoveUp, true);
            editor(ui, &mut chosen, Command::MoveDown, true);
            if is_group {
                editor(ui, &mut chosen, Command::Deeper, !state.multi);
                editor(ui, &mut chosen, Command::Higher, !state.multi);
            }
            ui.separator();
            editor(ui, &mut chosen, Command::Delete, true);
        });
    } else {
        editor(ui, &mut chosen, Command::Paste, state.can_paste);
    }
    // Positionen, Titel, Hinweise, Ausführungsbeschreibungen: straight in
    // the menu, not under Bearbeiten (Anwender 19.11.).
    if has_row && !spacers {
        structure(
            ui,
            &mut chosen,
            StructCommand::Lv(LvAction::ToLibrary),
            true,
        );
    }
    if state.stlb_placeholders {
        structure(ui, &mut chosen, StructCommand::UnmarkStlb, true);
    }
    if has_row && !spacers {
        crate::menus::menu(ui, "Status", |ui| {
            for status in [EditStatus::Done, EditStatus::InProgress, EditStatus::Open] {
                editor(ui, &mut chosen, Command::SetStatus(status), true);
            }
            editor(ui, &mut chosen, Command::ToggleDone, true);
            if state.price_check {
                ui.separator();
                editor(ui, &mut chosen, Command::ConfirmPriceCheck, true);
            }
        });
    }
    crate::menus::menu(ui, "Suchen", |ui| {
        editor(ui, &mut chosen, Command::Find, true);
        editor(ui, &mut chosen, Command::FindReplace, true);
        editor(ui, &mut chosen, Command::NextMissingQuantity, true);
        ui.separator();
        for filter in StatusFilter::ALL {
            editor(ui, &mut chosen, Command::Filter(filter), true);
        }
    });
    if state.positions_selected {
        crate::menus::menu(ui, "Verknüpfen", |ui| {
            ui.label(egui::RichText::new("Ausführungsbeschreibung").weak());
            let none = StructCommand::Link(None).label().to_string();
            for (target, text) in std::iter::once((None, &none))
                .chain(state.choices.iter().map(|(id, text)| (Some(*id), text)))
            {
                let active = state.current == Some(target);
                if ui.radio(active, text.as_str()).clicked() && !active {
                    chosen = Some(MenuChoice::Struct(StructCommand::Link(target)));
                }
            }
            ui.separator();
            structure(ui, &mut chosen, StructCommand::LinkNew, true);
            structure(
                ui,
                &mut chosen,
                StructCommand::GoToPerformance,
                !state.multi && matches!(state.current, Some(Some(_))),
            );
        });
    }
    crate::menus::menu(ui, "Leistungsverzeichnis", |ui| {
        structure(ui, &mut chosen, StructCommand::Lv(LvAction::Outline), true);
        structure(ui, &mut chosen, StructCommand::Renumber, true);
        structure(ui, &mut chosen, StructCommand::Lv(LvAction::Check), true);
        editor(ui, &mut chosen, Command::AllBlankNotesToSpacers, true);
        structure(ui, &mut chosen, StructCommand::Lv(LvAction::Compare), true);
        ui.separator();
        editor(ui, &mut chosen, Command::CollapseAll, true);
        editor(ui, &mut chosen, Command::ExpandAll, true);
        ui.separator();
        structure(ui, &mut chosen, StructCommand::Lv(LvAction::NewLv), true);
        structure(
            ui,
            &mut chosen,
            StructCommand::Lv(LvAction::LvProperties),
            true,
        );
    });
    if has_row {
        ui.separator();
        editor(ui, &mut chosen, Command::Delete, true);
    }
    if chosen.is_some() {
        ui.close();
    }
    chosen
}

fn editor(ui: &mut egui::Ui, chosen: &mut Option<MenuChoice>, command: Command, enabled: bool) {
    // The names of the menu bar (Einfügen: „Neue Position darunter“ …).
    let label = command.label();
    if ui
        .add_enabled(
            enabled,
            egui::Button::new(label).shortcut_text(command.shortcut()),
        )
        .clicked()
    {
        *chosen = Some(MenuChoice::Editor(command));
    }
}

fn structure(
    ui: &mut egui::Ui,
    chosen: &mut Option<MenuChoice>,
    command: StructCommand,
    enabled: bool,
) {
    if ui
        .add_enabled(
            enabled,
            egui::Button::new(command.label()).shortcut_text(command.shortcut()),
        )
        .clicked()
    {
        *chosen = Some(MenuChoice::Struct(command));
    }
}

/// Choices of the dialog „Neu nummerieren“.
pub struct RenumberForm {
    pub positions_only: bool,
    pub keep_done: bool,
    /// Ausführungsbeschreibungen 1, 2, 3 … by place (shown when there are any).
    pub performances: bool,
    /// Title the renumbering may be limited to.
    pub scope: Option<RowId>,
    pub only_scope: bool,
}

impl RenumberForm {
    pub fn new(lv: &Lv, selected: Option<usize>) -> Self {
        let scope = struct_ops::renumber_scope(lv, selected);
        Self {
            positions_only: !lv.is_auto_numbered(),
            keep_done: true,
            performances: lv.performance_count() > 0,
            only_scope: scope.is_some()
                && selected.is_some_and(|index| lv.rows().get(index).is_some_and(LvRow::is_group)),
            scope,
        }
    }

    pub fn mode(&self) -> proava_domain::RenumberMode {
        proava_domain::RenumberMode {
            positions_only: self.positions_only,
            keep_done: self.keep_done,
            scope: self.scope.filter(|_| self.only_scope),
        }
    }
}

/// The commands that make a new row (and the take-over dialogs that do):
/// their keys work everywhere in the LV view – table, a cell being edited,
/// the Eigenschaftenfenster, the windows „Eigenschaften“ and „Langtext“
/// (there after the window's row).
pub fn new_row_for_key(key: Key, modifiers: Modifiers) -> Option<MenuChoice> {
    if let Some(command) = StructCommand::for_key(key, modifiers) {
        return matches!(
            command,
            StructCommand::NewSubtitle
                | StructCommand::NewNoteBefore
                | StructCommand::NewPerformance
        )
        .then_some(MenuChoice::Struct(command));
    }
    crate::lv_ops::command_for_key(key, modifiers)
        .filter(|command| {
            matches!(
                command,
                Command::NewPosition
                    | Command::NewPositionBefore
                    | Command::NewTitle
                    | Command::NewNote
                    | Command::FromStlb
                    | Command::FromAusschreiben
            )
        })
        .map(MenuChoice::Editor)
}

/// Umschalt+F10 or the menu key open the context menu of the selected row.
pub fn opens_menu(key: Key, modifiers: Modifiers) -> bool {
    (key == Key::F10 && modifiers.shift && !modifiers.command && !modifiers.alt)
        || (key == Key::Menu && !modifiers.command && !modifiers.alt)
}

fn keyboard_menu_key() -> egui::Id {
    egui::Id::unique("lv_keyboard_menu")
}

/// Menu opened by the keyboard for a row; `false` once it is open (it then
/// stays at the row instead of the pointer until it closes).
#[derive(Clone, Copy)]
struct KeyboardMenu(RowId, bool);

pub(super) fn open_keyboard_menu(ctx: &egui::Context, row: RowId) {
    ctx.data_mut(|data| data.insert_temp(keyboard_menu_key(), KeyboardMenu(row, true)));
}

/// For the keyboard menu of `row`: Some(true) to open it now, Some(false)
/// while it is open, None otherwise.
pub(super) fn keyboard_menu_for(ctx: &egui::Context, row: RowId) -> Option<bool> {
    ctx.data(|data| data.get_temp::<KeyboardMenu>(keyboard_menu_key()))
        .filter(|KeyboardMenu(wanted, _)| *wanted == row)
        .map(|KeyboardMenu(_, first)| first)
}

/// After showing: remember that it is open, forget it once it closed.
pub(super) fn keyboard_menu_shown(ctx: &egui::Context, response: &egui::Response) {
    let open = egui::Popup::is_id_open(ctx, egui::Popup::default_response_id(response));
    ctx.data_mut(|data| {
        let state = data.get_temp::<KeyboardMenu>(keyboard_menu_key());
        match state {
            Some(KeyboardMenu(row, true)) => {
                data.insert_temp(keyboard_menu_key(), KeyboardMenu(row, false));
            }
            Some(KeyboardMenu(_, false)) if !open => {
                data.remove::<KeyboardMenu>(keyboard_menu_key());
            }
            _ => {}
        }
    });
}

/// egui id of the OZ field of a row in the edit panel.
pub(super) fn ordinal_field_id(row: RowId) -> egui::Id {
    egui::Id::unique(("ordinal_field", row))
}

pub(super) fn focus_ordinal_key() -> egui::Id {
    egui::Id::unique("focus_ordinal_field")
}

/// Ask the OZ field of `row` for the keyboard focus once it is shown.
#[derive(Clone, Copy)]
pub(super) struct FocusOrdinal(pub RowId);

impl LvEditor {
    pub(super) fn run_choice(
        &mut self,
        choice: MenuChoice,
        project: &mut Project,
        ctx: &egui::Context,
    ) -> EditorOutput {
        match choice {
            MenuChoice::Editor(command) => self.run(command, project, ctx),
            MenuChoice::Struct(command) => self.run_struct(command, project, ctx),
        }
    }

    pub fn run_struct(
        &mut self,
        command: StructCommand,
        project: &mut Project,
        ctx: &egui::Context,
    ) -> EditorOutput {
        let selected = self.selected_index(project.lv());
        match command {
            StructCommand::NewSubtitle => {
                let before = project.lv().clone();
                // On the deepest level there is no subtitle: a title on the
                // same level after this one instead (typing on would else
                // go into the current title's Kurztext).
                let lv = project.lv();
                let deepest_title = selected
                    .and_then(|index| lv.rows()[..=index].iter().rposition(LvRow::is_group))
                    .filter(|&parent| {
                        lv.level_count() > 0
                            && lv.rows()[parent]
                                .as_group()
                                .map_or(0, |group| group.level())
                                + 1
                                >= lv.level_count()
                    });
                if let Some(parent) = deepest_title {
                    let label = lv.level_label(lv.level_count() - 1).to_string();
                    let result = struct_ops::insert_title(project.lv_mut(), Some(parent));
                    let mut output = self.inserted(
                        project,
                        before,
                        result
                            .as_ref()
                            .map(|(index, _)| *index)
                            .map_err(Clone::clone),
                        Focus::Short,
                    );
                    if result.is_ok() {
                        output.toast = Some(format!(
                            "„{label}“ ist die tiefste Stufe – neuer Titel auf derselben Stufe angelegt."
                        ));
                    }
                    return output;
                }
                let result = struct_ops::insert_subtitle(project.lv_mut(), selected);
                self.inserted(project, before, result, Focus::Short)
            }
            StructCommand::NewNoteBefore => {
                let before = project.lv().clone();
                let result = lv_ops::insert_note(project.lv_mut(), selected, true);
                self.inserted(project, before, result, Focus::Long)
            }
            StructCommand::NewPerformance => {
                let before = project.lv().clone();
                let result = struct_ops::insert_performance(project.lv_mut(), selected);
                self.inserted(project, before, result, Focus::Short)
            }
            StructCommand::EditOrdinal => match selected {
                Some(index) if project.lv().rows()[index].has_ordinal() => {
                    let id = project.lv().rows()[index].id();
                    self.marked.clear();
                    ctx.data_mut(|data| data.insert_temp(focus_ordinal_key(), FocusOrdinal(id)));
                    EditorOutput::default()
                }
                _ => EditorOutput::toast("Nur Titel und Positionen haben eine Ordnungszahl."),
            },
            StructCommand::Renumber => {
                self.dialog = Some(Dialog::Renumber(RenumberForm::new(project.lv(), selected)));
                EditorOutput::default()
            }
            StructCommand::Link(target) => {
                let ids: Vec<RowId> = self.selection_ids().into_iter().collect();
                let before = project.lv().clone();
                match struct_ops::link_many(project.lv_mut(), &ids, target) {
                    Ok(0) => EditorOutput::default(),
                    Ok(count) => {
                        self.undo.record(&before, None);
                        EditorOutput {
                            dirty: true,
                            toast: Some(match target {
                                Some(_) => format!(
                                    "{} verknüpft.",
                                    crate::format::count(count, "Position", "Positionen")
                                ),
                                None => format!(
                                    "Bezug bei {} entfernt.",
                                    crate::format::count(count, "Position", "Positionen")
                                ),
                            }),
                            ..EditorOutput::default()
                        }
                    }
                    Err(error) => EditorOutput::toast(error),
                }
            }
            StructCommand::LinkNew => {
                let ids: Vec<RowId> = self.selection_ids().into_iter().collect();
                let before = project.lv().clone();
                let result = struct_ops::insert_performance_for(project.lv_mut(), &ids);
                let count = ids
                    .iter()
                    .filter(|id| project.lv().position_by_id(**id).is_some())
                    .count();
                let mut output = self.inserted(project, before, result, Focus::Short);
                if output.dirty {
                    output.toast = Some(format!(
                        "Neue Ausführungsbeschreibung, {} verknüpft.",
                        crate::format::count(count, "Position", "Positionen")
                    ));
                }
                output
            }
            StructCommand::UnmarkStlb => {
                let ids = self.selection_ids();
                let before = project.lv().clone();
                let mut changed = 0;
                let mut locked = 0;
                for id in ids {
                    let Some(index) = project.lv().index_of(id) else {
                        continue;
                    };
                    if let Some(position) = project
                        .lv_mut()
                        .row_mut(index)
                        .and_then(LvRow::as_position_mut)
                    {
                        if !position.is_stlb_placeholder() {
                            continue;
                        }
                        if position.is_locked() {
                            locked += 1;
                            continue;
                        }
                        position.set_stlb_placeholder(false);
                        changed += 1;
                    }
                }
                if changed == 0 {
                    return EditorOutput::toast(if locked > 0 {
                        format!(
                            "Fertige Positionen bleiben gesperrt – {}.",
                            proava_domain::LOCKED_HINT
                        )
                    } else {
                        "Kein STLB-Platzhalter gewählt.".to_string()
                    });
                }
                self.undo.record(&before, None);
                EditorOutput {
                    dirty: true,
                    toast: Some(format!(
                        "Platzhalter-Markierung bei {} entfernt – Strg+Z macht es rückgängig.",
                        crate::format::count(changed, "Position", "Positionen")
                    )),
                    ..EditorOutput::default()
                }
            }
            StructCommand::GoToPerformance => {
                let target = selected
                    .and_then(|index| project.lv().rows()[index].as_position())
                    .and_then(|position| position.performance());
                match target {
                    Some(id) => {
                        self.select(project.lv(), id);
                        EditorOutput::default()
                    }
                    None => EditorOutput::toast("Die Position hat keine Ausführungsbeschreibung."),
                }
            }
            StructCommand::ToFree => {
                let ids: Vec<RowId> = self.selection_ids().into_iter().collect();
                self.request_free(project, &ids)
            }
            StructCommand::ConvertTo(kind) => {
                if self.selection_ids().len() > 1 {
                    return EditorOutput::toast(
                        "Umwandeln geht für eine Zeile – bitte nur eine wählen.",
                    );
                }
                match selected {
                    Some(index) => self.request_conversion(project, index, kind),
                    None => {
                        EditorOutput::toast("Erst die Zeile wählen, die umgewandelt werden soll.")
                    }
                }
            }
            StructCommand::Lv(action) => EditorOutput {
                lv_action: Some(action),
                ..EditorOutput::default()
            },
        }
    }

    /// Dialogs of the structure commands; true when one was shown.
    pub(super) fn structure_dialogs(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        output: &mut EditorOutput,
    ) -> bool {
        match self.dialog {
            Some(Dialog::Renumber(_)) => {
                let Some(Dialog::Renumber(mut form)) = self.dialog.take() else {
                    return false;
                };
                let lv = project.lv();
                let mode = form.mode();
                let preview = lv.numbering_preview(mode);
                let affected = lv.done_positions_affected(mode);
                let warnings = struct_ops::renumber_warnings(lv, project.bids().len());
                let scope_title = form
                    .scope
                    .and_then(|id| lv.row_by_id(id))
                    .map(|row| format!("{} {}", row.ordinal(), row.short_text()));
                let ((cancelled, confirmed), close) = widgets::modal(ui, palette, 600.0, |ui| {
                    widgets::title(
                        ui,
                        palette,
                        "Neu nummerieren",
                        "Vorschau unten: alt → neu. Strg+Z macht es rückgängig.",
                    );
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Bereich")
                                .size(12.5)
                                .color(palette.text_dim),
                        );
                        ui.radio_value(&mut form.only_scope, false, "ganzes LV");
                        if let Some(title) = &scope_title {
                            ui.radio_value(
                                &mut form.only_scope,
                                true,
                                format!("nur Titel {title}"),
                            );
                        }
                    });
                    ui.radio_value(
                        &mut form.positions_only,
                        true,
                        "Nur Positionen – Titelnummern behalten (481, 482 …)",
                    );
                    ui.radio_value(
                        &mut form.positions_only,
                        false,
                        "Titel und Positionen – Titel 01, 02 …, Positionen in Schritten",
                    );
                    if !affected.is_empty() || !form.keep_done {
                        ui.checkbox(
                            &mut form.keep_done,
                            "Fertige Positionen behalten ihre Ordnungszahl (empfohlen)",
                        );
                    }
                    if project.lv().performance_count() > 0 {
                        ui.checkbox(
                            &mut form.performances,
                            "Ausführungsbeschreibungen fortlaufend nummerieren (A0001, A0002 … nach Lage im LV)",
                        );
                    }
                    for warning in &warnings {
                        ui.label(egui::RichText::new(warning).size(12.5).color(palette.warn));
                    }
                    ui.add_space(6.0);
                    // Feste Höhe: der Dialog springt nicht, wenn sich die Liste ändert.
                    egui::Frame::new()
                        .stroke(egui::Stroke::new(1.0, palette.line))
                        .corner_radius(egui::CornerRadius::same(6))
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.set_min_height(200.0);
                            ui.set_max_height(200.0);
                            egui::ScrollArea::vertical()
                                .id_salt("renumber_preview")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    if !affected.is_empty() {
                                        let text = if form.keep_done {
                                            match affected.len() {
                                                1 => "1 fertige Position behält ihre OZ:".to_string(),
                                                count => format!("{count} fertige Positionen behalten ihre OZ:"),
                                            }
                                        } else {
                                            match affected.len() {
                                                1 => "1 fertige Position bekommt eine neue OZ:".to_string(),
                                                count => format!("{count} fertige Positionen bekommen eine neue OZ:"),
                                            }
                                        };
                                        ui.label(egui::RichText::new(text).size(12.5).color(palette.warn));
                                        ui.label(
                                            egui::RichText::new(affected.join(", "))
                                                .monospace()
                                                .size(12.0),
                                        );
                                        ui.add_space(6.0);
                                    }
                                    if preview.is_empty() {
                                        ui.label(egui::RichText::new("Keine Ordnungszahl ändert sich.").color(palette.text_dim));
                                    } else {
                                        ui.label(
                                            egui::RichText::new(format!("{} ändern sich:", crate::format::count(preview.len(), "Ordnungszahl", "Ordnungszahlen")))
                                                .size(12.5)
                                                .color(palette.text_dim),
                                        );
                                        for change in preview.iter().take(500) {
                                            ui.label(
                                                egui::RichText::new(format!("{:<18} → {}", change.old, change.new))
                                                    .monospace()
                                                    .size(12.5),
                                            );
                                        }
                                        if preview.len() > 500 {
                                            ui.label(format!("… und {} weitere", preview.len() - 500));
                                        }
                                    }
                                });
                        });
                    ui.add_space(10.0);
                    widgets::dialog_buttons(ui, palette, "Nummerieren")
                });
                if close || cancelled {
                    return true;
                }
                if confirmed {
                    let before = project.lv().clone();
                    let changed = project.lv_mut().renumber_with(form.mode());
                    let performances = if form.performances {
                        project.lv_mut().renumber_performances()
                    } else {
                        0
                    };
                    if changed + performances > 0 {
                        self.undo.record(&before, None);
                        output.dirty = true;
                        // The dialog warned about the sent X83 already.
                        self.sent_warned = !warnings.is_empty();
                    }
                    let mut toast = format!(
                        "{} neu vergeben.",
                        crate::format::count(changed, "Ordnungszahl", "Ordnungszahlen")
                    );
                    if performances > 0 {
                        toast.push_str(&format!(
                            " {} neu nummeriert.",
                            crate::format::count(
                                performances,
                                "Ausführungsbeschreibung",
                                "Ausführungsbeschreibungen"
                            )
                        ));
                    }
                    output.toast = Some(toast);
                    return true;
                }
                self.dialog = Some(Dialog::Renumber(form));
                true
            }
            Some(Dialog::RemovePerformance { id, users }) => {
                let heading = project
                    .lv()
                    .performance_by_id(id)
                    .map(|performance| performance.heading())
                    .unwrap_or_default();
                let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
                let ((cancelled, confirmed), close) = widgets::modal(ui, palette, 480.0, |ui| {
                    widgets::title(
                        ui,
                        palette,
                        "Ausführungsbeschreibung löschen?",
                        &match users {
                            0 => format!("„{heading}“ – keine Position verwendet sie."),
                            1 => format!("„{heading}“ – 1 Position verliert den Bezug. Strg+Z holt sie zurück."),
                            n => format!("„{heading}“ – {n} Positionen verlieren den Bezug. Strg+Z holt sie zurück."),
                        },
                    );
                    widgets::danger_dialog_buttons(ui, palette, "Löschen")
                });
                if close || cancelled {
                    self.dialog = None;
                } else if confirmed || enter {
                    self.dialog = None;
                    let before = project.lv().clone();
                    let index = project.lv().index_of(id);
                    if let Some((_, cleared)) = project.lv_mut().remove_performance(id) {
                        self.undo.record(&before, None);
                        if let Some(index) = index {
                            let rows = project.lv().rows();
                            self.selected = rows
                                .get(index.min(rows.len().saturating_sub(1)))
                                .map(LvRow::id);
                        }
                        output.dirty = true;
                        output.toast = Some(format!(
                            "Ausführungsbeschreibung gelöscht, {} entfernt.",
                            crate::format::count(cleared, "Bezug", "Bezüge")
                        ));
                    }
                }
                true
            }
            _ => false,
        }
    }
}
