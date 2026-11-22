//! „Ausschnitt aus PDF einfügen …“ in the LV editor: the dialog for the long
//! text of the edit panel and of the big long-text window, and putting the
//! chosen clip into the project and the text (the window „Eigenschaften“
//! has its own dialog, see `position_window`).

use eframe::egui;
use proava_application::{Launcher, PreparedClip};
use proava_domain::{Project, RowId, TextBlock};

use super::{set_long_text, EditorOutput, LvEditor};
use crate::pdf_clip_dialog::{ClipDialogResult, PdfClipDialog};
use crate::theme::Palette;

/// Where the clip goes into the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClipPlace {
    /// At the caret of the long text in the edit panel.
    Panel,
    /// At the caret of the big long-text window.
    Big,
    /// Behind the text.
    End,
}

impl LvEditor {
    /// Opens the dialog for the long text of `row`.
    pub(crate) fn open_clip_dialog(&mut self, launcher: &Launcher, row: RowId, place: ClipPlace) {
        if self.clip_dialog.is_some() {
            return;
        }
        let start = self
            .clip_folder
            .clone()
            .or_else(|| launcher.last_folder())
            .unwrap_or_else(|| launcher.home_folder());
        self.clip_dialog = Some((PdfClipDialog::open(launcher, start), row, place));
    }

    /// Opens the dialog with a PDF dropped on the editor.
    pub(crate) fn open_clip_dialog_with(
        &mut self,
        launcher: &Launcher,
        row: RowId,
        name: &str,
        bytes: Vec<u8>,
    ) -> EditorOutput {
        if self.clip_dialog.is_some() {
            return EditorOutput::default();
        }
        match PdfClipDialog::with_pdf(launcher, name, bytes) {
            Ok(dialog) => {
                self.clip_dialog = Some((dialog, row, ClipPlace::End));
                EditorOutput::default()
            }
            Err(error) => EditorOutput::toast(error),
        }
    }

    /// Shows the dialog (main window) and inserts what was chosen.
    pub(crate) fn clip_dialog_ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        launcher: &Launcher,
        output: &mut EditorOutput,
    ) {
        let Some((mut dialog, row, place)) = self.clip_dialog.take() else {
            return;
        };
        match dialog.show(ui, palette, launcher) {
            ClipDialogResult::Open => self.clip_dialog = Some((dialog, row, place)),
            ClipDialogResult::Cancelled => {
                self.clip_folder = dialog.folder.or(self.clip_folder.take());
            }
            ClipDialogResult::Chosen(prepared, width_mm) => {
                self.clip_folder = dialog.folder.or(self.clip_folder.take());
                output.merge(self.insert_pdf_clip(project, row, *prepared, width_mm, place));
            }
        }
    }

    /// Puts a clip into the project and into the long text of `row`.
    pub(crate) fn insert_pdf_clip(
        &mut self,
        project: &mut Project,
        row: RowId,
        prepared: PreparedClip,
        width_mm: f32,
        place: ClipPlace,
    ) -> EditorOutput {
        let Some(index) = project.lv().index_of(row) else {
            return EditorOutput::toast("Die Zeile gibt es nicht mehr.");
        };
        if project.lv().rows()[index].is_group() || project.lv().rows()[index].is_spacer() {
            return EditorOutput::toast(
                "PDF-Ausschnitte gehören in den Langtext einer Position oder eines Hinweises.",
            );
        }
        if let Err(message) = project.lv().ensure_editable_rows(&[index]) {
            return EditorOutput::toast(message);
        }
        // Images belong to the text: not into an STLB-Bau text.
        if let Err(message) = project
            .lv()
            .ensure_text_editable(&[project.lv().rows()[index].id()])
        {
            return EditorOutput::toast(message);
        }
        self.undo.record(project.lv(), None);
        let summary = prepared.summary_for(project);
        let file = project.add_pdf_clip(&prepared.name, prepared.bytes, prepared.clip);
        let (text, complements) = match place {
            ClipPlace::Panel if self.long_text_owner.map(|(owner, _)| owner) == Some(row) => (
                self.long_text.insert_image_sized(file, width_mm),
                self.long_text.complements(),
            ),
            ClipPlace::Big => match &mut self.big_text {
                Some(big) if big.row == row => (
                    big.editor.insert_image_sized(file, width_mm),
                    big.editor.complements(),
                ),
                _ => (
                    clip_at_end(project, index, file, width_mm),
                    super::complements_after_block(
                        project,
                        index,
                        super::long_text_blocks(project, index),
                        false,
                    ),
                ),
            },
            _ => (
                clip_at_end(project, index, file, width_mm),
                super::complements_after_block(
                    project,
                    index,
                    super::long_text_blocks(project, index),
                    false,
                ),
            ),
        };
        set_long_text(project, index, text);
        super::set_complements(project, index, complements);
        EditorOutput {
            dirty: true,
            toast: Some(summary),
            ..EditorOutput::default()
        }
    }
}

/// The long text of row `index` with the clip behind it.
pub(crate) fn clip_at_end(
    project: &Project,
    index: usize,
    file: String,
    width_mm: f32,
) -> proava_domain::RichText {
    let mut text = match &project.lv().rows()[index] {
        proava_domain::LvRow::Position(position) => position.long_text().clone(),
        proava_domain::LvRow::Note(note) => note.text().clone(),
        proava_domain::LvRow::Performance(performance) => performance.long_text().clone(),
        proava_domain::LvRow::Group(_) | proava_domain::LvRow::Spacer(_) => {
            proava_domain::RichText::default()
        }
    };
    text.push(TextBlock::Image {
        file,
        width_mm: Some(width_mm.round()),
    });
    text
}
