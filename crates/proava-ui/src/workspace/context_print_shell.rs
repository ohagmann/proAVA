//! Druckdialog for the documents of a window (LV-Vergleich, Nachträge,
//! Schriftverkehr): open it with the kind, run it, hand Stil, Blattrahmen and
//! Seite to the output that follows the file dialog.

use eframe::egui;
use proava_application::{ChangeOrderPrint, PrintKind, PrintLook};

use super::{change_order_shell::ChangeOrderFile, Dialog, FilePurpose, WorkspaceScreen};
use crate::context_print::{ContextPrintDialog, ContextPrintResult};
use crate::theme::Palette;

/// What the dialog prints.
#[derive(Debug, Clone, Copy)]
pub(super) enum ContextTarget {
    /// Änderungsliste of the open LV-Vergleich.
    ChangeList,
    ChangeOrder(ChangeOrderPrint),
    /// The letters waiting in `pending_letters`.
    Letters,
}

/// Look and page of the next window document (set when the dialog prints).
#[derive(Debug, Clone)]
pub(super) struct ContextLook {
    pub look: PrintLook,
    pub landscape: bool,
}

impl WorkspaceScreen {
    fn context_dialog(&self, kind: PrintKind, detail: String) -> ContextPrintDialog {
        let preferences = self.launcher.preferences();
        ContextPrintDialog::new(
            kind,
            detail,
            self.launcher.print_settings(kind),
            &preferences.print_look,
            self.project.info().page_frame.is_some(),
        )
    }

    /// LV-Vergleich ▸ PDF …
    pub(super) fn open_change_list_print(&mut self) {
        let detail = self
            .compare
            .as_ref()
            .map(|view| format!("gegenüber {}", view.against()))
            .unwrap_or_default();
        let dialog = self.context_dialog(PrintKind::ChangeList, detail);
        self.dialog = Some(Dialog::ContextPrint(
            Box::new(dialog),
            ContextTarget::ChangeList,
        ));
    }

    /// Nachträge ▸ Prüfung drucken / Übersicht drucken.
    pub(super) fn open_change_order_print(&mut self, print: ChangeOrderPrint) {
        let (kind, detail) = match print {
            ChangeOrderPrint::Check(id) => (
                PrintKind::ChangeOrderCheck,
                self.project
                    .change_order(id)
                    .map(|order| format!("{} {}", order.label(), order.title))
                    .unwrap_or_default(),
            ),
            ChangeOrderPrint::Overview => (
                PrintKind::ChangeOrderOverview,
                crate::format::count(self.project.change_orders().len(), "Nachtrag", "Nachträge"),
            ),
        };
        let dialog = self.context_dialog(kind, detail);
        self.dialog = Some(Dialog::ContextPrint(
            Box::new(dialog),
            ContextTarget::ChangeOrder(print),
        ));
    }

    /// Schriftverkehr ▸ PDF erzeugen …: `pending_letters` is set already.
    pub(super) fn open_letters_print(&mut self, separate: Option<bool>, file_in_project: bool) {
        let detail = match &self.pending_letters {
            Some((kind, recipients, _)) if !kind.to_client() => format!(
                "{} an {}",
                kind.label(),
                crate::format::count(recipients.len(), "Bieter", "Bieter")
            ),
            Some((kind, _, _)) => format!("{} an den Bauherrn", kind.label()),
            None => String::new(),
        };
        let dialog = self
            .context_dialog(PrintKind::Letters, detail)
            .with_letter_options(separate, file_in_project);
        self.dialog = Some(Dialog::ContextPrint(
            Box::new(dialog),
            ContextTarget::Letters,
        ));
    }

    /// The look for the window document now written (once; else the settings).
    pub(super) fn take_context_look(&mut self) -> ContextLook {
        self.context_look.take().unwrap_or_else(|| ContextLook {
            look: self.launcher.preferences().print_look,
            landscape: false,
        })
    }

    /// One frame of the dialog; returns whether it stays open.
    pub(super) fn context_print_step(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        dialog: &mut ContextPrintDialog,
        target: ContextTarget,
    ) -> bool {
        match dialog.show(ui, palette) {
            ContextPrintResult::Open => true,
            ContextPrintResult::Cancelled => {
                self.launcher
                    .set_print_settings(dialog.kind(), dialog.settings());
                if matches!(target, ContextTarget::Letters) {
                    // Back to the letters (they wait while the dialog is open).
                    self.pending_letters = None;
                    self.pending_release_letter = None;
                    self.dialog = self.parked_letters.take().map(Dialog::Letters);
                }
                false
            }
            ContextPrintResult::EditFrame => {
                self.launcher
                    .set_print_settings(dialog.kind(), dialog.settings());
                let mut form = crate::master_data_view::MasterDataForm::of(&self.project);
                form.frame_default = self.launcher.preferences().page_frame;
                form.tab = crate::master_data_view::MasterTab::PageFrame;
                self.dialog = Some(Dialog::MasterData(Box::new(form)));
                false
            }
            ContextPrintResult::Print => {
                self.launcher
                    .set_print_settings(dialog.kind(), dialog.settings());
                let base = self.launcher.preferences().print_look;
                self.context_look = Some(ContextLook {
                    look: dialog.look(&base),
                    landscape: dialog.landscape(),
                });
                let purpose = match target {
                    ContextTarget::ChangeList => FilePurpose::ComparePdf,
                    ContextTarget::ChangeOrder(print) => {
                        FilePurpose::ChangeOrder(ChangeOrderFile::Print(print))
                    }
                    ContextTarget::Letters => {
                        if let Some((_, _, separate)) = self.pending_letters.as_mut() {
                            *separate = dialog.separate.unwrap_or(false);
                        }
                        self.pending_file_letters = dialog.file_in_project.unwrap_or(true);
                        if let Some(letters) = self.parked_letters.as_mut() {
                            letters.set_output_options(dialog.separate, self.pending_file_letters);
                        }
                        FilePurpose::Letters
                    }
                };
                self.open_file_dialog(purpose);
                false
            }
        }
    }
}
