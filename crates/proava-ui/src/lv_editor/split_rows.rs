//! Aufteilungszeilen im Kosten-LV (Anwender 19.11.): Einfügen ▸ „Aufteilung nach
//! KG einfügen“ legt hinter der Schreibmarke die Zeilen KG 481–489 mit der
//! Aufteilung der Datenpunktrechnung an (40/12/10/30/5/3 %), Bezug: die
//! Positionen des Titels davor – oder, wenn mehrere Positionen markiert sind,
//! genau diese (Liste). „GA nach Datenpunkten (Kurztext-Vorlage) einfügen“
//! bringt dazu den Titel und die Datenpunkte AI … BV mit EP. Prozent (EP) und
//! KG ändert man in der Tabelle; Strg+Z nimmt den Block zurück.

use proava_domain::{LvRow, Project, RowId};

use super::{EditorOutput, LvEditor};

impl LvEditor {
    /// One undo step; the first Aufteilungszeile (with the template: the
    /// first Datenpunkt, to type the quantity) is selected.
    pub(super) fn new_split(&mut self, project: &mut Project, template: bool) -> EditorOutput {
        let before = project.lv().clone();
        let selected = self.selected_index(project.lv());
        // Several positions marked: the block refers to exactly these.
        let chosen: Vec<RowId> = {
            let lv = project.lv();
            let ids = self.selection_ids();
            let mut chosen: Vec<(usize, RowId)> = ids
                .into_iter()
                .filter_map(|id| {
                    let index = lv.index_of(id)?;
                    let position = lv.rows()[index].as_position()?;
                    (!position.is_surcharge()).then_some((index, id))
                })
                .collect();
            chosen.sort();
            chosen.into_iter().map(|(_, id)| id).collect()
        };
        let result = if template {
            project.lv_mut().insert_points_template(selected)
        } else if chosen.len() > 1 {
            project.lv_mut().insert_split_block_on(&chosen)
        } else {
            project.lv_mut().insert_split_block(selected)
        };
        match result {
            Ok(range) => {
                self.undo.record(&before, None);
                let lv = project.lv();
                let focus = lv.rows()[range.clone()]
                    .iter()
                    .find(|row| matches!(row, LvRow::Position(_)))
                    .map(LvRow::id);
                if let Some(id) = focus {
                    self.selected = Some(id);
                    self.single(id);
                    self.scroll_to_selected = true;
                }
                self.reload();
                let toast = if template {
                    "GA nach Datenpunkten eingefügt – Mengen der Datenpunkte eintragen; die Aufteilung \
                     darunter trägt die Kosten (KG 481–489)."
                        .to_string()
                } else if chosen.len() > 1 {
                    format!(
                        "Aufteilung auf {} markierte Positionen eingefügt – Prozent (EP) und KG in der \
                         Tabelle ändern.",
                        chosen.len()
                    )
                } else {
                    "Aufteilung nach KG eingefügt – Bezug: die Positionen davor im Titel; Prozent (EP) \
                     und KG in der Tabelle ändern."
                        .to_string()
                };
                EditorOutput {
                    dirty: true,
                    toast: Some(toast),
                    ..EditorOutput::default()
                }
            }
            Err(error) => {
                *project.lv_mut() = before;
                EditorOutput::toast(error)
            }
        }
    }

    /// „Datenpunkt-Abschnitt in Kurztext-Positionen umwandeln“ for the title at
    /// `index` (the selected row): one undo step.
    pub(super) fn point_section_to_kurztext(
        &mut self,
        project: &mut Project,
        index: Option<usize>,
    ) -> EditorOutput {
        let Some(index) = index.filter(|&index| {
            project
                .lv()
                .rows()
                .get(index)
                .and_then(LvRow::as_group)
                .is_some_and(proava_domain::LvGroup::is_point_section)
        }) else {
            return EditorOutput::toast("Bitte zuerst einen Datenpunkt-Abschnitt (Titel) wählen.");
        };
        let before = project.lv().clone();
        match project.lv_mut().point_section_to_kurztext(index) {
            Ok(range) => {
                self.undo.record(&before, None);
                self.point_window = None;
                self.reload();
                let count = range.len();
                EditorOutput {
                    dirty: true,
                    toast: Some(format!(
                        "Datenpunkt-Abschnitt umgewandelt: {count} Zeilen (Datenpunkte je Bauteil als \
                         Rechengrundlage, darunter die Aufteilung); die Summe bleibt, die Zeilen der \
                         Anlagenliste entfallen (Strg+Z holt sie zurück)."
                    )),
                    ..EditorOutput::default()
                }
            }
            Err(error) => {
                *project.lv_mut() = before;
                EditorOutput::toast(error)
            }
        }
    }
}
