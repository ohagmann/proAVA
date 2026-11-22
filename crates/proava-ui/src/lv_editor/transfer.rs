//! Taking rows over from another source into the open LV, and the rows the
//! user chose for the library.

use proava_domain::transfer::{self, Transfer};
use proava_domain::{LvRow, Project, RowId};

use super::{EditorOutput, LvEditor};

impl LvEditor {
    /// Inserts a transfer behind the selection; one undo step.
    pub fn insert_transfer(&mut self, project: &mut Project, taken: &Transfer) -> EditorOutput {
        let selected = self.selected_index(project.lv());
        let before = project.lv().clone();
        match transfer::insert(project, selected, taken) {
            Ok(range) => {
                self.undo.record(&before, None);
                self.undo.seal();
                let first = project.lv().rows()[range.start].id();
                self.select(project.lv(), first);
                self.marked = project.lv().rows()[range.clone()]
                    .iter()
                    .map(LvRow::id)
                    .collect();
                self.reload();
                EditorOutput {
                    dirty: true,
                    toast: Some(format!(
                        "{} übernommen ({}) – Strg+Z macht es rückgängig.",
                        crate::format::count(range.len(), "Zeile", "Zeilen"),
                        crate::format::count(taken.position_count(), "Position", "Positionen")
                    )),
                    ..EditorOutput::default()
                }
            }
            Err(error) => EditorOutput::toast(error),
        }
    }

    /// The selected rows (titles stand for their blocks), in document order.
    pub fn selected_rows(&self, project: &Project) -> Vec<RowId> {
        let ids = self.selection_ids();
        project
            .lv()
            .rows()
            .iter()
            .map(LvRow::id)
            .filter(|id| ids.contains(id))
            .collect()
    }

    /// Rows of the Übernahme-Fenster were dropped on the table: the index
    /// to insert after (None = before the first row).
    pub fn take_transfer_drop(&mut self, project: &Project) -> Option<Option<usize>> {
        let (row, after) = self.cells.dropped.take()?;
        self.cells.drop_hover = None;
        let index = project.lv().index_of(row)?;
        Some(if after {
            Some(index)
        } else {
            index.checked_sub(1)
        })
    }

    /// Inserts a transfer after row `after` (None = at the very start).
    pub fn insert_transfer_after(
        &mut self,
        project: &mut Project,
        taken: &Transfer,
        after: Option<usize>,
    ) -> EditorOutput {
        match after {
            Some(index) => {
                let id = project.lv().rows()[index].id();
                self.selected = Some(id);
                self.marked.clear();
                self.insert_transfer(project, taken)
            }
            None => {
                // before the first row: insert after it, then move the rows up
                let before = project.lv().clone();
                let mut output = self.insert_transfer(project, taken);
                if output.dirty {
                    let count = taken.rows.len().min(project.lv().rows().len());
                    let start = project.lv().rows().len().min(1);
                    let range = start..start + count;
                    if crate::struct_ops::move_rows_to(project.lv_mut(), range, 0).is_err() {
                        output.toast = Some("Am Anfang einfügen ging nicht – die Zeilen stehen hinter der ersten Zeile.".into());
                    }
                    let _ = before;
                }
                output
            }
        }
    }
}
