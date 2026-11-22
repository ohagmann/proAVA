//! Undo across LVs: one order of steps for the whole project.
//!
//! The LV editor keeps an undo stack of `Lv` snapshots – one per LV, the
//! workspace swaps them when the user switches LVs. Changes that are not
//! inside one LV (Projektstammdaten, LV anlegen/löschen/verschieben) are
//! project steps here. `History` remembers in which order all steps
//! happened, so Strg+Z always undoes the newest one – also when it belongs
//! to another LV (that LV is shown first).

use std::collections::HashMap;

use proava_domain::{
    ContractSnapshot, CorrespondenceEntry, CostEstimate, LetterTemplate, Lv, LvListSnapshot,
    OffersSnapshot, Project, ProjectInfo,
};

use crate::lv_editor::EditorView;
use crate::undo::UndoStack;

const LIMIT: usize = 300;

/// A change outside the LV content, with the state *before* it.
#[derive(Debug, Clone)]
pub enum ProjectChange {
    Info(Box<ProjectInfo>),
    LvList(Box<LvListSnapshot>),
    /// Bids and Bieterverzeichnis of the LV with this session key.
    Offers(u64, Box<OffersSnapshot>),
    /// Letter templates of the project (Schriftverkehr).
    Letters(Vec<LetterTemplate>),
    /// Auftrag, Nachträge and Abrechnung of the LV with this session key.
    Contract(u64, Box<ContractSnapshot>),
    /// Filed letters of the project (Schriftverkehr-Archiv); the PDFs are shared.
    Correspondence(Vec<CorrespondenceEntry>),
    /// Kostenschätzungen und -berechnungen of the project.
    Costs(Vec<CostEstimate>),
    /// Kostenverfolgung: budget, notes, choices, recorded states.
    Tracking(Box<proava_domain::CostTracking>),
    /// Several changes made together (e.g. Versanddatum and filed letters).
    Several(Vec<ProjectChange>),
    /// The whole project (without images, see `Project::swap_state`) –
    /// a change an assistant made through the bridge; the number names the
    /// step (the answer to the assistant asks whether it is still there).
    Whole(u64, Box<Project>),
}

/// Changes typed in quick succession into the same place count as one step.
const COALESCE_SECONDS: f64 = 1.5;

#[derive(Debug, Clone)]
enum Step {
    /// One step in the undo stack of the LV with this session key.
    Lv(u64),
    Project(ProjectChange, &'static str),
}

/// What the workspace has to do for one Strg+Z / Strg+Y.
#[derive(Debug)]
pub enum Action {
    /// Show the LV with this key (if not active) and let the editor undo/redo.
    Lv(u64),
    /// A project step was undone/redone; the text is for the status line.
    Done(&'static str),
    Nothing,
}

#[derive(Default)]
pub struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// Editor pushes already seen for the active LV.
    seen: u64,
    /// Undo stacks and views of the LVs that are not active.
    stacks: HashMap<u64, UndoStack<Lv>>,
    views: HashMap<u64, EditorView>,
    /// Last offers step: (LV key, label, time, length of the undo list then).
    last_offers: Option<(u64, &'static str, f64, usize)>,
    /// Last letter template step: (time, length of the undo list then).
    last_letters: Option<(f64, usize)>,
    /// Last Abrechnung/Nachtrag step: (LV key, label, time, length of the undo list then).
    last_contract: Option<(u64, &'static str, f64, usize)>,
    /// Last Bemerkung step in the archive: (time, length of the undo list then).
    last_correspondence: Option<(f64, usize)>,
    /// Last Kostenermittlung step: (label, time, length of the undo list then).
    last_costs: Option<(&'static str, f64, usize)>,
}

impl History {
    /// Takes note of steps the editor of the active LV added since the last call.
    pub fn note_editor(&mut self, active_key: u64, pushes: u64) {
        if pushes > self.seen {
            for _ in self.seen..pushes {
                self.undo.push(Step::Lv(active_key));
            }
            self.redo.clear();
            self.trim();
        }
        self.seen = pushes;
    }

    /// A change outside the LV content; `before` is the state to go back to.
    pub fn record_project(&mut self, before: ProjectChange, label: &'static str) {
        self.undo.push(Step::Project(before, label));
        self.redo.clear();
        self.trim();
    }

    /// A change of bids or invited bidders; with the same LV and label within
    /// a moment of the previous one (typing a name) it joins that step.
    pub fn record_offers(
        &mut self,
        key: u64,
        before: OffersSnapshot,
        label: &'static str,
        now: f64,
        join: bool,
    ) {
        if let Some((last_key, last_label, time, length)) = self.last_offers.filter(|_| join) {
            if last_key == key
                && last_label == label
                && now - time < COALESCE_SECONDS
                && length == self.undo.len()
            {
                self.last_offers = Some((key, label, now, length));
                return;
            }
        }
        self.record_project(ProjectChange::Offers(key, Box::new(before)), label);
        self.last_offers = Some((key, label, now, self.undo.len()));
    }

    /// A change of Auftrag, Nachträge or Abrechnung; with the same LV and
    /// label within a moment of the previous one (typing) it joins that step.
    pub fn record_contract(
        &mut self,
        key: u64,
        before: ContractSnapshot,
        label: &'static str,
        now: f64,
        join: bool,
    ) {
        if let Some((last_key, last_label, time, length)) = self.last_contract.filter(|_| join) {
            if last_key == key
                && last_label == label
                && now - time < COALESCE_SECONDS
                && length == self.undo.len()
            {
                self.last_contract = Some((key, label, now, length));
                return;
            }
        }
        self.record_project(ProjectChange::Contract(key, Box::new(before)), label);
        self.last_contract = Some((key, label, now, self.undo.len()));
    }

    /// A change of the letter templates; typing in a row (within a moment,
    /// nothing else in between) is one step.
    pub fn record_letters(&mut self, before: Vec<LetterTemplate>, now: f64) {
        if let Some((time, length)) = self.last_letters {
            if now - time < COALESCE_SECONDS && length == self.undo.len() {
                self.last_letters = Some((now, length));
                return;
            }
        }
        self.record_project(ProjectChange::Letters(before), "Briefvorlage ändern");
        self.last_letters = Some((now, self.undo.len()));
    }

    /// A change of the filed letters; with `join` (typing a Bemerkung) it
    /// joins the previous such step within a moment.
    pub fn record_correspondence(
        &mut self,
        before: Vec<CorrespondenceEntry>,
        label: &'static str,
        now: f64,
        join: bool,
    ) {
        if let Some((time, length)) = self.last_correspondence.filter(|_| join) {
            if now - time < COALESCE_SECONDS && length == self.undo.len() {
                self.last_correspondence = Some((now, length));
                return;
            }
        }
        self.record_project(ProjectChange::Correspondence(before), label);
        self.last_correspondence = join.then_some((now, self.undo.len()));
    }

    /// A change of the Kostenermittlungen; with `join` (typing in a field)
    /// and the same label within a moment of the previous one it joins that step.
    pub fn record_costs(
        &mut self,
        before: Vec<CostEstimate>,
        label: &'static str,
        now: f64,
        join: bool,
    ) {
        if let Some((last_label, time, length)) = self.last_costs.filter(|_| join) {
            if last_label == label && now - time < COALESCE_SECONDS && length == self.undo.len() {
                self.last_costs = Some((label, now, length));
                return;
            }
        }
        self.record_project(ProjectChange::Costs(before), label);
        self.last_costs = join.then_some((label, now, self.undo.len()));
    }

    /// A change of the Kostenverfolgung (budget, notes …); typing joins like
    /// [`History::record_costs`].
    pub fn record_tracking(
        &mut self,
        before: proava_domain::CostTracking,
        label: &'static str,
        now: f64,
        join: bool,
    ) {
        if let Some((last_label, time, length)) = self.last_costs.filter(|_| join) {
            if last_label == label && now - time < COALESCE_SECONDS && length == self.undo.len() {
                self.last_costs = Some((label, now, length));
                return;
            }
        }
        self.record_project(ProjectChange::Tracking(Box::new(before)), label);
        self.last_costs = join.then_some((label, now, self.undo.len()));
    }

    /// Whether the change of an assistant with this number is still done
    /// (in the undo list, not taken back).
    pub fn holds_whole(&self, number: u64) -> bool {
        self.undo
            .iter()
            .any(|step| matches!(step, Step::Project(ProjectChange::Whole(known, _), _) if *known == number))
    }

    /// The newest step (undo) / the next redo concerns the Kostenermittlungen.
    pub fn costs_next(&self, redo: bool) -> bool {
        let list = if redo { &self.redo } else { &self.undo };
        matches!(list.last(), Some(Step::Project(ProjectChange::Costs(_), _)))
    }

    /// The newest step (undo) / the next redo concerns the Kostenverfolgung.
    pub fn tracking_next(&self, redo: bool) -> bool {
        let list = if redo { &self.redo } else { &self.undo };
        matches!(
            list.last(),
            Some(Step::Project(ProjectChange::Tracking(_), _))
        )
    }

    /// The newest step (undo) / the next redo concerns the filed letters –
    /// Strg+Z/Y work in the Schriftverkehr list then.
    pub fn correspondence_next(&self, redo: bool) -> bool {
        let list = if redo { &self.redo } else { &self.undo };
        match list.last() {
            Some(Step::Project(ProjectChange::Correspondence(_), _)) => true,
            Some(Step::Project(ProjectChange::Several(changes), _)) => changes
                .iter()
                .any(|change| matches!(change, ProjectChange::Correspondence(_))),
            _ => false,
        }
    }

    /// A step in the stack of a not active LV (LV-Eigenschaften from the list).
    pub fn record_other_lv(&mut self, key: u64, before: &Lv) {
        self.stacks
            .entry(key)
            .or_insert_with(crate::lv_editor::LvEditor::new_undo_stack)
            .record(before, None);
        self.undo.push(Step::Lv(key));
        self.redo.clear();
        self.trim();
    }

    /// The editor switches LV: keep the old LV's stack and view, hand over
    /// the new LV's ones. `stack` is the editor's stack (swapped in place).
    pub fn switch(
        &mut self,
        from: u64,
        to: u64,
        stack: &mut UndoStack<Lv>,
        view: EditorView,
    ) -> Option<EditorView> {
        let mut incoming = self
            .stacks
            .remove(&to)
            .unwrap_or_else(crate::lv_editor::LvEditor::new_undo_stack);
        std::mem::swap(stack, &mut incoming);
        self.stacks.insert(from, incoming);
        self.views.insert(from, view);
        self.seen = stack.push_count();
        self.views.remove(&to)
    }

    /// The newest step (undo) / the next redo is a letter template change –
    /// Strg+Z/Y work in the Schriftverkehr dialog then.
    pub fn letters_next(&self, redo: bool) -> bool {
        let list = if redo { &self.redo } else { &self.undo };
        matches!(
            list.last(),
            Some(Step::Project(ProjectChange::Letters(_), _))
        )
    }

    /// The editor of the LV `key` undid (`redo` false) or redid a step on its
    /// own (Strg+Z/Y in one of its text fields): the same step moves here too,
    /// so the order of the whole project stays right.
    pub fn editor_moved(&mut self, key: u64, redo: bool) {
        let (from, to) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        if let Some(position) = from
            .iter()
            .rposition(|step| matches!(step, Step::Lv(other) if *other == key))
        {
            let step = from.remove(position);
            to.push(step);
        }
    }

    /// Whether the newest step (undo) / the next redo belongs to the LV with
    /// this key – then Strg+Z/Y in one of its text fields stays with the
    /// editor; otherwise the order of the whole project wins.
    pub fn next_is_lv(&self, key: u64, redo: bool) -> bool {
        let list = if redo { &self.redo } else { &self.undo };
        matches!(list.last(), Some(Step::Lv(other)) if *other == key)
    }

    pub fn can_undo(&self, editor_can_undo: bool) -> bool {
        editor_can_undo || !self.undo.is_empty()
    }

    pub fn can_redo(&self, editor_can_redo: bool) -> bool {
        editor_can_redo || !self.redo.is_empty()
    }

    /// Undoes the newest step. `can_undo(key)` says whether the LV's stack
    /// (active editor or stored) still has something.
    pub fn undo(
        &mut self,
        project: &mut Project,
        active_key: u64,
        editor_can_undo: bool,
    ) -> Action {
        while let Some(step) = self.undo.pop() {
            match step {
                Step::Lv(key) => {
                    let possible = if key == active_key {
                        editor_can_undo
                    } else {
                        project.lv_index_of_key(key).is_some()
                            && self.stacks.get(&key).is_some_and(UndoStack::can_undo)
                    };
                    if possible {
                        self.redo.push(Step::Lv(key));
                        return Action::Lv(key);
                    }
                }
                Step::Project(change, label) => {
                    let after = apply(project, change);
                    self.redo.push(Step::Project(after, label));
                    return Action::Done(label);
                }
            }
        }
        // The editor undid something on its own (Strg+Z in a field) – its
        // stack may still hold steps the order list does not know.
        if editor_can_undo {
            Action::Lv(active_key)
        } else {
            Action::Nothing
        }
    }

    pub fn redo(
        &mut self,
        project: &mut Project,
        active_key: u64,
        editor_can_redo: bool,
    ) -> Action {
        while let Some(step) = self.redo.pop() {
            match step {
                Step::Lv(key) => {
                    let possible = if key == active_key {
                        editor_can_redo
                    } else {
                        project.lv_index_of_key(key).is_some()
                            && self.stacks.get(&key).is_some_and(UndoStack::can_redo)
                    };
                    if possible {
                        self.undo.push(Step::Lv(key));
                        return Action::Lv(key);
                    }
                }
                Step::Project(change, label) => {
                    let before = apply(project, change);
                    self.undo.push(Step::Project(before, label));
                    return Action::Done(label);
                }
            }
        }
        if editor_can_redo {
            Action::Lv(active_key)
        } else {
            Action::Nothing
        }
    }

    fn trim(&mut self) {
        if self.undo.len() > LIMIT {
            let extra = self.undo.len() - LIMIT;
            self.undo.drain(..extra);
        }
    }
}

/// Puts `change` into the project and returns what was there before.
fn apply(project: &mut Project, change: ProjectChange) -> ProjectChange {
    match change {
        ProjectChange::Info(info) => {
            let before = std::mem::replace(project.info_mut(), *info);
            ProjectChange::Info(Box::new(before))
        }
        ProjectChange::LvList(snapshot) => {
            ProjectChange::LvList(Box::new(project.restore_lv_list(*snapshot)))
        }
        ProjectChange::Offers(key, snapshot) => match project.lv_index_of_key(key) {
            Some(index) => match project.restore_offers(index, (*snapshot).clone()) {
                Some(before) => ProjectChange::Offers(key, Box::new(before)),
                None => ProjectChange::Offers(key, snapshot),
            },
            // The LV is gone (deleted); keep the step for a redo of the deletion.
            None => ProjectChange::Offers(key, snapshot),
        },
        ProjectChange::Letters(templates) => {
            ProjectChange::Letters(project.replace_letter_templates(templates))
        }
        ProjectChange::Contract(key, snapshot) => match project.lv_index_of_key(key) {
            Some(index) => match project.restore_contract(index, (*snapshot).clone()) {
                Some(before) => ProjectChange::Contract(key, Box::new(before)),
                None => ProjectChange::Contract(key, snapshot),
            },
            None => ProjectChange::Contract(key, snapshot),
        },
        ProjectChange::Correspondence(entries) => {
            ProjectChange::Correspondence(project.replace_correspondence(entries))
        }
        ProjectChange::Costs(estimates) => {
            ProjectChange::Costs(project.replace_cost_estimates(estimates))
        }
        ProjectChange::Tracking(tracking) => {
            ProjectChange::Tracking(Box::new(project.replace_cost_tracking(*tracking)))
        }
        ProjectChange::Whole(number, state) => {
            ProjectChange::Whole(number, Box::new(project.swap_state(*state)))
        }
        ProjectChange::Several(changes) => {
            // Back in reverse order; the result undoes in the original order.
            let mut done: Vec<ProjectChange> = changes
                .into_iter()
                .rev()
                .map(|change| apply(project, change))
                .collect();
            done.reverse();
            ProjectChange::Several(done)
        }
    }
}

#[cfg(test)]
mod tests {
    use proava_domain::{Lv, OzMask, Project, ProjectName};

    use super::{Action, History, ProjectChange};
    use crate::lv_editor::LvEditor;

    fn project() -> Project {
        let mask = || OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap();
        let mut project = Project::new(
            ProjectName::parse("Test").unwrap(),
            "2026-11-17",
            Lv::new("LV 1", mask()),
        );
        project.add_lv(Lv::new("LV 2", mask()));
        project
    }

    #[test]
    fn newest_step_first_also_across_lvs() {
        let mut project = project();
        let first = project.lv_key(0).unwrap();
        let second = project.lv_key(1).unwrap();
        let mut history = History::default();
        let mut editor_stack = LvEditor::new_undo_stack();

        // 1. Änderung im LV 1
        editor_stack.record(project.lv(), None);
        history.note_editor(first, editor_stack.push_count());
        // 2. Stammdaten
        let before = project.info().clone();
        project.info_mut().label = "Neubau".into();
        history.record_project(ProjectChange::Info(Box::new(before)), "Stammdaten");
        // 3. Wechsel zu LV 2 und Änderung dort
        history.switch(first, second, &mut editor_stack, Default::default());
        project.set_active_lv(1);
        editor_stack.record(project.lv(), None);
        history.note_editor(second, editor_stack.push_count());

        assert!(
            matches!(history.undo(&mut project, second, true), Action::Lv(key) if key == second)
        );
        editor_stack.undo(project.lv());
        assert!(matches!(
            history.undo(&mut project, second, false),
            Action::Done("Stammdaten")
        ));
        assert_eq!(project.info().label, "");
        // Der nächste Schritt gehört LV 1 (nicht aktiv): der Arbeitsbereich wechselt dorthin.
        assert!(
            matches!(history.undo(&mut project, second, false), Action::Lv(key) if key == first)
        );
        history.switch(second, first, &mut editor_stack, Default::default());
        project.set_active_lv(0);
        editor_stack.undo(project.lv());
        assert!(matches!(
            history.undo(&mut project, first, false),
            Action::Nothing
        ));

        // Wiederholen in derselben Folge wie vorher
        assert!(matches!(history.redo(&mut project, first, true), Action::Lv(key) if key == first));
        editor_stack.redo(project.lv());
        assert!(matches!(
            history.redo(&mut project, first, false),
            Action::Done("Stammdaten")
        ));
        assert_eq!(project.info().label, "Neubau");
        assert!(
            matches!(history.redo(&mut project, first, false), Action::Lv(key) if key == second)
        );
    }

    #[test]
    fn lv_list_changes_undo_and_redo() {
        let mut project = project();
        let mut history = History::default();
        let before = project.lv_list_snapshot();
        project.remove_lv(1).unwrap();
        history.record_project(ProjectChange::LvList(Box::new(before)), "LV löschen");
        assert_eq!(project.lv_count(), 1);
        let key = project.lv_key(0).unwrap();
        assert!(matches!(
            history.undo(&mut project, key, false),
            Action::Done(_)
        ));
        assert_eq!(project.lv_count(), 2);
        assert!(matches!(
            history.redo(&mut project, key, false),
            Action::Done(_)
        ));
        assert_eq!(project.lv_count(), 1);
    }

    #[test]
    fn offers_undo_and_typing_is_one_step() {
        use proava_domain::{Address, InvitedBidder};
        let mut project = project();
        let key = project.lv_key(0).unwrap();
        let mut history = History::default();
        let before = project.offers_snapshot(0).unwrap();
        let id = project.add_invitee(InvitedBidder::new(Address::named("M")));
        history.record_offers(key, before, "Bieterverzeichnis", 10.0, false);
        let typed = project.offers_snapshot(0).unwrap();
        project.invitee_mut(id).unwrap().address.name1 = "Mü".into();
        history.record_offers(key, typed, "Bieterverzeichnis", 10.5, true);
        assert!(matches!(
            history.undo(&mut project, key, false),
            Action::Done("Bieterverzeichnis")
        ));
        assert!(
            project.invitees().is_empty(),
            "Anlegen und Tippen sind ein Schritt"
        );
        assert!(matches!(
            history.undo(&mut project, key, false),
            Action::Nothing
        ));
        assert!(matches!(
            history.redo(&mut project, key, false),
            Action::Done(_)
        ));
        assert_eq!(project.invitees()[0].address.name1, "Mü");
        let later = project.offers_snapshot(0).unwrap();
        project.invitee_mut(id).unwrap().status = proava_domain::InviteStatus::NoBid;
        history.record_offers(key, later, "Bieterverzeichnis", 30.0, true);
        let shaped = project.offers_snapshot(0).unwrap();
        project.add_invitee(InvitedBidder::new(Address::named("N")));
        history.record_offers(key, shaped, "Bieterverzeichnis", 30.2, false);
        assert!(matches!(
            history.undo(&mut project, key, false),
            Action::Done(_)
        ));
        assert_eq!(
            project.invitees().len(),
            1,
            "Anlegen ist ein eigener Schritt"
        );
        assert!(matches!(
            history.undo(&mut project, key, false),
            Action::Done(_)
        ));
        assert_eq!(
            project.invitees()[0].status,
            proava_domain::InviteStatus::Requested
        );
    }

    #[test]
    fn letter_templates_undo_typing_as_one_step() {
        use proava_domain::{LetterKind, LetterTemplate};
        let mut project = project();
        let key = project.lv_key(0).unwrap();
        let mut history = History::default();
        let mut template = LetterTemplate::standard(LetterKind::Rejection);
        let before = project.letter_templates().to_vec();
        template.subject = "A".into();
        project.set_letter_template(template.clone());
        history.record_letters(before, 5.0);
        let before = project.letter_templates().to_vec();
        template.subject = "AB".into();
        project.set_letter_template(template.clone());
        history.record_letters(before, 5.4);
        let before = project.letter_templates().to_vec();
        template.subject = "ABC".into();
        project.set_letter_template(template.clone());
        history.record_letters(before, 20.0);
        assert!(matches!(
            history.undo(&mut project, key, false),
            Action::Done("Briefvorlage ändern")
        ));
        assert_eq!(project.letter_template(LetterKind::Rejection).subject, "AB");
        assert!(matches!(
            history.undo(&mut project, key, false),
            Action::Done(_)
        ));
        assert!(
            project.letter_templates().is_empty(),
            "zurück zur Standardvorlage"
        );
        assert!(matches!(
            history.redo(&mut project, key, false),
            Action::Done(_)
        ));
        assert_eq!(project.letter_template(LetterKind::Rejection).subject, "AB");
    }

    #[test]
    fn filed_letters_and_versanddatum_undo_in_one_step() {
        use proava_domain::{Address, CorrespondenceEntry, CorrespondenceKind, InvitedBidder};
        let mut project = project();
        let key = project.lv_key(0).unwrap();
        let id = project.add_invitee(InvitedBidder::new(Address::named("M")));
        let mut history = History::default();
        let offers = project.offers_snapshot(0).unwrap();
        let archive = project.correspondence_snapshot();
        project.invitee_mut(id).unwrap().requested = "18.11.2026".into();
        project.add_correspondence(CorrespondenceEntry::new(
            CorrespondenceKind::Own,
            "",
            "x",
            vec![1],
        ));
        history.record_project(
            ProjectChange::Several(vec![
                ProjectChange::Offers(key, Box::new(offers)),
                ProjectChange::Correspondence(archive),
            ]),
            "Schriftverkehr ablegen",
        );
        assert!(history.correspondence_next(false));
        assert!(matches!(
            history.undo(&mut project, key, false),
            Action::Done("Schriftverkehr ablegen")
        ));
        assert!(project.correspondence().is_empty());
        assert_eq!(project.invitees()[0].requested, "");
        assert!(history.correspondence_next(true));
        assert!(matches!(
            history.redo(&mut project, key, false),
            Action::Done(_)
        ));
        assert_eq!(project.correspondence().len(), 1);
        assert_eq!(project.invitees()[0].requested, "18.11.2026");

        // Typing a Bemerkung is one step.
        let entry = project.correspondence()[0].id;
        let before = project.correspondence_snapshot();
        project.set_correspondence_remark(entry, "p");
        history.record_correspondence(before, "Bemerkung", 50.0, true);
        let before = project.correspondence_snapshot();
        project.set_correspondence_remark(entry, "per Post");
        history.record_correspondence(before, "Bemerkung", 50.5, true);
        assert!(matches!(
            history.undo(&mut project, key, false),
            Action::Done("Bemerkung")
        ));
        assert_eq!(project.correspondence()[0].remark, "");
    }

    #[test]
    fn new_change_clears_redo() {
        let mut project = project();
        let key = project.lv_key(0).unwrap();
        let mut history = History::default();
        let before = project.info().clone();
        history.record_project(ProjectChange::Info(Box::new(before)), "Stammdaten");
        history.undo(&mut project, key, false);
        history.note_editor(key, 1);
        assert!(matches!(
            history.redo(&mut project, key, false),
            Action::Nothing
        ));
    }

    #[test]
    fn cost_steps_join_while_typing_and_undo_back() {
        let mut project = project();
        let mut history = History::default();
        let empty = project.cost_estimates().to_vec();
        let id = project.add_cost_estimate(proava_domain::CostEstimate::template(
            proava_domain::CostStage::Calculation,
            "18.11.2026",
            "",
        ));
        history.record_costs(empty, "Kostenermittlung anlegen", 1.0, false);
        // Typing the title: three changes, one step.
        for (time, title) in [(2.0, "K"), (2.5, "KB"), (3.0, "KB LPH 3")] {
            let before = project.cost_estimates().to_vec();
            project.cost_estimate_mut(id).unwrap().title = title.into();
            history.record_costs(before, "Kostenermittlung ändern", time, true);
        }
        assert!(history.costs_next(false));
        assert!(matches!(
            history.undo(&mut project, 0, false),
            Action::Done("Kostenermittlung ändern")
        ));
        assert_eq!(project.cost_estimates()[0].title, "Kostenberechnung");
        assert!(matches!(
            history.undo(&mut project, 0, false),
            Action::Done("Kostenermittlung anlegen")
        ));
        assert!(project.cost_estimates().is_empty());
        assert!(matches!(
            history.redo(&mut project, 0, false),
            Action::Done(_)
        ));
        assert_eq!(project.cost_estimates().len(), 1);
        assert!(history.costs_next(true));
    }
}
