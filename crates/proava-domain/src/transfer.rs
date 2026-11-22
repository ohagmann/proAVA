//! Taking rows over from another LV or project (Übernahme, Bibliothek):
//! collect the chosen rows with what they need (Ausführungsbeschreibungen,
//! images), then insert them into a target LV with new ids, renamed
//! references and free order numbers.

use std::collections::{HashMap, HashSet};

use crate::edit_status::EditStatus;
use crate::lv::{Lv, LvRow, RowId};
use crate::project::{ImageAsset, Project};
use crate::rich_text::{RichText, TextBlock};

/// What to carry over besides texts and structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferOptions {
    pub quantities: bool,
    pub prices: bool,
}

impl Default for TransferOptions {
    fn default() -> Self {
        Self {
            quantities: true,
            prices: false,
        }
    }
}

/// Rows taken from a source, ready to be inserted somewhere else.
#[derive(Debug, Clone, Default)]
pub struct Transfer {
    pub rows: Vec<LvRow>,
    /// Images the long texts of `rows` show.
    pub images: Vec<ImageAsset>,
}

impl Transfer {
    pub fn position_count(&self) -> usize {
        self.rows.iter().filter(|row| row.is_position()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// Collects the rows `ids` of `lv` (a title stands for its whole block) in
/// document order. Ausführungsbeschreibungen the chosen positions refer to
/// come along (in front) even when not chosen.
pub fn collect(source: &Project, lv: &Lv, ids: &[RowId], options: TransferOptions) -> Transfer {
    let wanted: HashSet<RowId> = ids.iter().copied().collect();
    let mut chosen = vec![false; lv.rows().len()];
    let mut covered_until = 0;
    for (index, row) in lv.rows().iter().enumerate() {
        if index < covered_until || !wanted.contains(&row.id()) {
            continue;
        }
        let range = lv.block_range(index);
        covered_until = range.end;
        for flag in &mut chosen[range] {
            *flag = true;
        }
    }
    let mut rows: Vec<LvRow> = lv
        .rows()
        .iter()
        .zip(&chosen)
        .filter(|(_, chosen)| **chosen)
        .map(|(row, _)| row.clone())
        .collect();
    let inside: HashSet<RowId> = rows.iter().map(LvRow::id).collect();
    let mut needed: Vec<RowId> = Vec::new();
    for row in &rows {
        if let Some(performance) = row
            .as_position()
            .and_then(|position| position.performance())
        {
            if !inside.contains(&performance) && !needed.contains(&performance) {
                needed.push(performance);
            }
        }
    }
    let extra: Vec<LvRow> = needed
        .iter()
        .filter_map(|id| lv.row_by_id(*id).cloned())
        .collect();
    rows.splice(0..0, extra);

    for row in &mut rows {
        if let LvRow::Position(position) = row {
            position.set_status(EditStatus::Open);
            if !options.quantities {
                position.set_quantity(rust_decimal::Decimal::ZERO);
            }
            if !options.prices {
                position.set_unit_price(None);
                position.set_price_components(Vec::new());
            }
        }
    }
    let images = images_of(source, &rows);
    Transfer { rows, images }
}

/// The images of `source` the texts of `rows` show.
pub fn images_of(source: &Project, rows: &[LvRow]) -> Vec<ImageAsset> {
    let used: HashSet<String> = rows
        .iter()
        .flat_map(texts)
        .flat_map(|text| text.images().map(str::to_string).collect::<Vec<_>>())
        .collect();
    source
        .images()
        .iter()
        .filter(|image| used.contains(&image.file))
        .cloned()
        .collect()
}

/// Adds `images` to `target` (same name and content → reused, otherwise a
/// free name) and returns old name → name in the project.
fn import_images(target: &mut Project, images: &[ImageAsset]) -> HashMap<String, String> {
    let mut names = HashMap::new();
    for image in images {
        let name = match target.image(&image.file) {
            Some(existing) if existing.same_content(image) => image.file.clone(),
            _ => target.add_asset_copy(image),
        };
        names.insert(image.file.clone(), name);
    }
    names
}

fn rename_row_images(row: &mut LvRow, names: &HashMap<String, String>) {
    match row {
        LvRow::Position(position) => {
            let mut text = position.long_text().clone();
            rename_images(&mut text, names);
            position.set_long_text(text);
            for sub in position.sub_descriptions_mut() {
                rename_images(&mut sub.long_text, names);
            }
        }
        LvRow::Note(note) => rename_images(note.text_mut(), names),
        LvRow::Performance(performance) => rename_images(performance.long_text_mut(), names),
        LvRow::Group(_) | LvRow::Spacer(_) => {}
    }
}

/// A whole LV from elsewhere (clipboard, other project) as a new LV of
/// `target` behind the others, under the LV number `number`. Images come
/// along; without `prices` the unit prices are left out. Bids never come
/// along (an LV has none). Returns the index of the new LV.
pub fn add_lv(
    target: &mut Project,
    mut lv: Lv,
    images: &[ImageAsset],
    number: &str,
    prices: bool,
) -> usize {
    let names = import_images(target, images);
    for index in 0..lv.rows().len() {
        if let Some(row) = lv.row_mut(index) {
            rename_row_images(row, &names);
            if let LvRow::Position(position) = row {
                position.set_status(EditStatus::Open);
                if !prices {
                    position.set_unit_price(None);
                    position.set_price_components(Vec::new());
                }
            }
        }
    }
    lv.set_number(number);
    target.add_lv(lv)
}

pub(crate) fn texts(row: &LvRow) -> Vec<&RichText> {
    match row {
        LvRow::Position(position) => {
            let mut texts = vec![position.long_text()];
            texts.extend(position.sub_descriptions().iter().map(|sub| &sub.long_text));
            texts
        }
        LvRow::Note(note) => vec![note.text()],
        LvRow::Performance(performance) => vec![performance.long_text()],
        LvRow::Group(_) | LvRow::Spacer(_) => Vec::new(),
    }
}

pub(crate) fn rename_images(text: &mut RichText, names: &HashMap<String, String>) {
    for block in text.blocks_mut() {
        if let TextBlock::Image { file, .. } = block {
            if let Some(new) = names.get(file) {
                *file = new.clone();
            }
        }
    }
}

/// Inserts a transfer into the active LV of `target` after row `selected`
/// (None: at the end). Titles move to the level a new title would get
/// there; every row gets a new id and a free order number (an
/// automatically numbered LV is renumbered); references inside the
/// transfer follow, others are dropped; images are added to the project
/// (same name and content → reused). Returns the inserted index range;
/// nothing changes on an error.
pub fn insert(
    target: &mut Project,
    selected: Option<usize>,
    transfer: &Transfer,
) -> Result<std::ops::Range<usize>, String> {
    insert_with(target, Place::After(selected), transfer)
}

/// Like [`insert`], but at the end of the title at `title` (index), one
/// level below it (e.g. into a title of the library).
pub fn insert_under(
    target: &mut Project,
    title: usize,
    transfer: &Transfer,
) -> Result<std::ops::Range<usize>, String> {
    if !target.lv().rows().get(title).is_some_and(LvRow::is_group) {
        return Err("Das Ziel ist kein Titel.".to_string());
    }
    insert_with(target, Place::Under(title), transfer)
}

#[derive(Clone, Copy)]
enum Place {
    After(Option<usize>),
    Under(usize),
}

fn insert_with(
    target: &mut Project,
    place: Place,
    transfer: &Transfer,
) -> Result<std::ops::Range<usize>, String> {
    if transfer.rows.is_empty() {
        return Err("Nichts gewählt.".to_string());
    }
    // A mask without title levels cannot number titles: only their content
    // comes over (like pasting).
    let without_titles;
    let transfer = if target.lv().level_count() == 0 && transfer.rows.iter().any(LvRow::is_group) {
        without_titles = Transfer {
            rows: transfer
                .rows
                .iter()
                .filter(|row| !row.is_group())
                .cloned()
                .collect(),
            images: transfer.images.clone(),
        };
        if without_titles.rows.is_empty() {
            return Err(format!(
                "Die OZ-Maske „{}“ hat keine Titelstufe – Titel lassen sich hier nicht einfügen.",
                target.lv().oz_mask().pattern()
            ));
        }
        &without_titles
    } else {
        transfer
    };
    let before = target.clone();
    let result = insert_inner(target, place, transfer);
    if result.is_err() {
        *target = before;
    }
    result
}

fn insert_inner(
    target: &mut Project,
    place: Place,
    transfer: &Transfer,
) -> Result<std::ops::Range<usize>, String> {
    // Images first: names may change.
    let names = import_images(target, &transfer.images);
    let lv = target.lv_mut();
    let max_level = lv.level_count().saturating_sub(1);
    let lowest = transfer
        .rows
        .iter()
        .filter_map(LvRow::as_group)
        .map(|group| group.level())
        .min();
    let highest = transfer
        .rows
        .iter()
        .filter_map(LvRow::as_group)
        .map(|group| group.level())
        .max()
        .unwrap_or(0);
    let (index, delta) = match (place, lowest) {
        (Place::Under(title), lowest) => {
            let level = lv.rows()[title]
                .as_group()
                .map(|group| group.level())
                .unwrap_or(0)
                + 1;
            let index = lv.block_range(title).end;
            let delta = match lowest {
                Some(lowest) => {
                    let wanted = level as isize - lowest as isize;
                    let room = max_level as isize - highest as isize;
                    wanted.min(room.max(-(lowest as isize)))
                }
                None => 0,
            };
            (index, delta)
        }
        (Place::After(selected), Some(lowest)) => {
            // The level a new title would get there – but no deeper than
            // the transferred subtitles still fit into the mask.
            let wanted = lv.level_for_new_group(selected) as isize - lowest as isize;
            let room = max_level as isize - highest as isize;
            let delta = wanted.min(room.max(-(lowest as isize)));
            let level = (lowest as isize + delta).max(0) as usize;
            let first_group = transfer.rows.iter().position(LvRow::is_group).unwrap_or(0);
            let index = if transfer.rows[..first_group]
                .iter()
                .all(LvRow::is_performance)
                && transfer.rows[first_group].is_group()
            {
                lv.insert_group_index(selected, level)
            } else {
                lv.insert_index(selected)
            };
            (index, delta)
        }
        (Place::After(selected), None) => (lv.insert_index(selected), 0),
    };
    let old_ids: Vec<RowId> = transfer.rows.iter().map(LvRow::id).collect();
    for (offset, row) in transfer.rows.iter().enumerate() {
        let mut row = row.clone().with_id(RowId::UNSET);
        match &mut row {
            LvRow::Group(group) => {
                let level = (group.level() as isize + delta).clamp(0, max_level as isize);
                group.set_level(level as usize);
                group.set_ordinal("");
            }
            LvRow::Position(position) => position.set_ordinal(""),
            LvRow::Note(_) | LvRow::Performance(_) | LvRow::Spacer(_) => {}
        }
        rename_row_images(&mut row, &names);
        lv.insert_numbered(index + offset, row)?;
    }
    let count = transfer.rows.len();
    let renamed: HashMap<RowId, RowId> = old_ids
        .into_iter()
        .zip(lv.rows()[index..index + count].iter().map(LvRow::id))
        .collect();
    for new in renamed.values().copied().collect::<Vec<_>>() {
        let Some(position) = lv.position_by_id(new) else {
            continue;
        };
        let performance = position.performance().map(|old| renamed.get(&old).copied());
        let repeats = position.repeats().map(|old| renamed.get(&old).copied());
        if let Some(performance) = performance {
            lv.link_performance(new, performance)?;
        }
        if let Some(repeats) = repeats {
            lv.link_repetition(new, repeats)?;
        }
        // Zuschlag on listed positions: the taken-over copies; positions of
        // the source that did not come along are dropped (their ids mean
        // nothing here).
        if let Some(row) = lv
            .index_of(new)
            .and_then(|index| lv.row_mut(index))
            .and_then(LvRow::as_position_mut)
        {
            if let Some(mut surcharge) = row.surcharge().cloned() {
                surcharge.references.retain_mut(|reference| {
                    match renamed.get(&reference.position) {
                        Some(id) => {
                            reference.position = *id;
                            true
                        }
                        None => false,
                    }
                });
                row.set_surcharge(Some(surcharge));
            }
        }
    }
    lv.unlink_missing_references();
    // Datenpunkt-Abschnitte take the stage of this LV (none in a tender LV).
    lv.fit_point_sections();
    Ok(index..index + count)
}

/// Copies the images `used` of `source` into `target`: same content is
/// reused (also under another name), else added under a free name. Returns
/// the renamed ones (old → new).
pub(crate) fn bring_images(
    target: &mut Project,
    source: &Project,
    used: &HashSet<String>,
) -> HashMap<String, String> {
    let mut names = HashMap::new();
    for image in source
        .images()
        .iter()
        .filter(|image| used.contains(&image.file))
    {
        let same = match target.image(&image.file) {
            Some(existing) if existing.same_content(image) => Some(image.file.clone()),
            _ => target
                .images()
                .iter()
                .find(|existing| existing.same_content(image))
                .map(|existing| existing.file.clone()),
        };
        let name = same.unwrap_or_else(|| target.add_asset_copy(image));
        if name != image.file {
            names.insert(image.file.clone(), name);
        }
    }
    names
}

/// Adds a copy of `lv` (from another project, e.g. a GAEB file read into
/// memory) as a new LV of `target`, order numbers and structure unchanged.
/// The images its texts show come along (same name and content → reused,
/// otherwise renamed); every position starts with status „offen“, the
/// remark „Angebotsaufforderung exportiert“ is dropped. Returns the index
/// of the new LV (it does not become active).
pub fn add_lv_from(target: &mut Project, source: &Project, lv: &Lv) -> usize {
    let mut copy = lv.clone();
    copy.set_request_exported(None);
    let used: HashSet<String> = copy
        .rows()
        .iter()
        .flat_map(texts)
        .flat_map(|text| text.images().map(str::to_string).collect::<Vec<_>>())
        .collect();
    let names = bring_images(target, source, &used);
    for index in 0..copy.rows().len() {
        let Some(row) = copy.row_mut(index) else {
            continue;
        };
        match row {
            LvRow::Position(position) => {
                position.set_status(EditStatus::Open);
                if !names.is_empty() {
                    let mut text = position.long_text().clone();
                    rename_images(&mut text, &names);
                    position.set_long_text(text);
                    for sub in position.sub_descriptions_mut() {
                        rename_images(&mut sub.long_text, &names);
                    }
                }
            }
            LvRow::Note(note) => rename_images(note.text_mut(), &names),
            LvRow::Performance(performance) => rename_images(performance.long_text_mut(), &names),
            LvRow::Group(_) | LvRow::Spacer(_) => {}
        }
    }
    target.add_lv(copy)
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::*;
    use crate::{LvGroup, OzMask, PerformanceDescription, Position, ProjectName};

    fn project(name: &str, mask: &str) -> Project {
        let lv = Lv::new("LV", OzMask::parse(mask).unwrap());
        Project::new(ProjectName::parse(name).unwrap(), "2026-11-17", lv)
    }

    fn source() -> (Project, Vec<RowId>) {
        let mut project = project("Quelle", "11.22.PPPP");
        let image = project.add_image("schrank.png", vec![1, 2, 3]);
        let lv = project.lv_mut();
        let t1 = lv.push_group(LvGroup::new(0, "", "Automation"));
        let t11 = lv.push_group(LvGroup::new(1, "", "ASP01"));
        let ab = lv.push_row(LvRow::Performance(PerformanceDescription::new(
            "1",
            "Trassen",
            RichText::plain("verzinkt"),
        )));
        let mut ddc = Position::new("", "DDC", Decimal::from(2), "St", Some(Decimal::from(100)));
        ddc.set_long_text(RichText::new(vec![TextBlock::Image {
            file: image,
            width_mm: None,
        }]));
        ddc.set_status(EditStatus::Done);
        let ddc = lv.push_position(ddc);
        let modul = lv.push_position(Position::new(
            "",
            "Modul",
            Decimal::from(4),
            "St",
            Some(Decimal::TEN),
        ));
        lv.link_performance(ddc, Some(ab)).unwrap();
        lv.link_repetition(modul, Some(ddc)).unwrap();
        let t2 = lv.push_group(LvGroup::new(0, "", "Feld"));
        let t21 = lv.push_group(LvGroup::new(1, "", "Fühler"));
        let fuehler = lv.push_position(Position::new("", "Fühler", Decimal::from(9), "St", None));
        lv.link_performance(fuehler, Some(ab)).unwrap();
        lv.renumber();
        (project, vec![t1, t11, ab, ddc, modul, t2, t21, fuehler])
    }

    #[test]
    fn a_title_comes_with_its_block_references_and_images() {
        let (source, ids) = source();
        let transfer = collect(&source, source.lv(), &[ids[0]], TransferOptions::default());
        assert_eq!(transfer.rows.len(), 5);
        assert_eq!(transfer.images.len(), 1);
        let mut target = project("Ziel", "11.22.PPPP");
        target.add_image("schrank.png", vec![9, 9]);
        let lv = target.lv_mut();
        lv.push_group(LvGroup::new(0, "", "Vorhanden"));
        lv.push_group(LvGroup::new(1, "", "Alt"));
        lv.push_position(Position::new("", "Alt", Decimal::ONE, "St", None));
        lv.renumber();
        let range = insert(&mut target, Some(2), &transfer).unwrap();
        assert_eq!(range, 3..8);
        let lv = target.lv();
        assert_eq!(
            lv.rows()[3].ordinal(),
            "02",
            "after the selected block, same level"
        );
        assert_eq!(lv.rows()[4].ordinal(), "02.01");
        let ddc = lv.rows()[6].as_position().unwrap();
        assert_eq!(ddc.ordinal(), "02.01.0010");
        assert_eq!(ddc.status(), EditStatus::Open, "status is always open");
        assert_eq!(ddc.unit_price(), None, "prices left out by default");
        assert_eq!(ddc.quantity(), Decimal::from(2));
        assert_eq!(
            ddc.performance(),
            Some(lv.rows()[5].id()),
            "reference follows"
        );
        let modul = lv.rows()[7].as_position().unwrap();
        assert_eq!(modul.repeats(), Some(ddc.id()));
        let image = ddc.long_text().images().next().unwrap().to_string();
        assert_eq!(image, "schrank_2.png", "other image with the same name");
        assert_eq!(target.image(&image).unwrap().bytes, vec![1, 2, 3]);
        assert!(lv.duplicate_ordinals().is_empty());
    }

    #[test]
    fn a_zuschlag_refers_to_the_taken_over_positions_only() {
        use crate::{Surcharge, SurchargeKind, SurchargeReference};
        let (mut source, ids) = source();
        let (ddc, fuehler, t11) = (ids[3], ids[7], ids[1]);
        let lv = source.lv_mut();
        let index = lv.index_of(ids[4]).unwrap() + 1;
        let mut zuschlag = Position::new(
            "01.01.0025",
            "Zuschlag",
            Decimal::ZERO,
            "",
            Some(Decimal::TEN),
        );
        zuschlag.set_surcharge(Some(Surcharge {
            split: false,
            kind: SurchargeKind::Listed,
            references: vec![
                SurchargeReference {
                    position: ddc,
                    quantity: None,
                },
                SurchargeReference {
                    position: fuehler,
                    quantity: None,
                },
            ],
        }));
        lv.insert_row(index, LvRow::Position(zuschlag));
        let options = TransferOptions {
            quantities: true,
            prices: true,
        };
        let transfer = collect(&source, source.lv(), &[t11], options);
        let mut target = project("Ziel", "11.22.PPPP");
        // rows in the target whose ids may equal those of the source
        let lv = target.lv_mut();
        lv.push_group(LvGroup::new(0, "", "Vorhanden"));
        lv.push_group(LvGroup::new(1, "", "Alt"));
        for _ in 0..10 {
            lv.push_position(Position::new("", "Alt", Decimal::ONE, "St", None));
        }
        lv.renumber();
        let last = lv.rows().len() - 1;
        let range = insert(&mut target, Some(last), &transfer).unwrap();
        let lv = target.lv();
        let rows = &lv.rows()[range];
        let new_ddc = rows
            .iter()
            .find(|row| row.short_text() == "DDC")
            .unwrap()
            .id();
        let pasted = rows
            .iter()
            .filter_map(LvRow::as_position)
            .find(|position| position.is_surcharge())
            .unwrap();
        let references = &pasted.surcharge().unwrap().references;
        assert_eq!(references.len(), 1, "the Fühler did not come along");
        assert_eq!(references[0].position, new_ddc);
    }

    #[test]
    fn single_positions_bring_their_description_and_options_apply() {
        let (source, ids) = source();
        let options = TransferOptions {
            quantities: false,
            prices: true,
        };
        let transfer = collect(&source, source.lv(), &[ids[7], ids[4]], options);
        let kinds: Vec<String> = transfer.rows.iter().map(LvRow::short_text).collect();
        assert_eq!(
            kinds,
            ["Ausführungsbeschreibung A0001 – Trassen", "Modul", "Fühler"]
        );
        let mut target = project("Ziel", "11.PPPP");
        target
            .lv_mut()
            .push_row(LvRow::Performance(PerformanceDescription::new(
                "1",
                "Eigene",
                RichText::default(),
            )));
        target.lv_mut().push_group(LvGroup::new(0, "", "Titel"));
        target.lv_mut().renumber();
        let range = insert(&mut target, None, &transfer).unwrap();
        let lv = target.lv();
        let copied_ab = lv.rows()[range.start].as_performance().unwrap();
        assert_eq!(copied_ab.number(), "2", "number taken → next free");
        let modul = lv.rows()[range.start + 1].as_position().unwrap();
        assert_eq!(modul.quantity(), Decimal::ZERO);
        assert_eq!(modul.unit_price(), Some(Decimal::TEN));
        assert_eq!(modul.repeats(), None, "base not taken over");
        let fuehler = lv.rows()[range.start + 2].as_position().unwrap();
        assert_eq!(fuehler.performance(), Some(copied_ab.id()));
        assert_eq!(fuehler.ordinal(), "01.0020");
    }

    #[test]
    fn same_image_is_reused_and_errors_change_nothing() {
        let (source, ids) = source();
        let transfer = collect(&source, source.lv(), &[ids[3]], TransferOptions::default());
        let mut target = project("Ziel", "11.22.PPPP");
        target.add_image("schrank.png", vec![1, 2, 3]);
        target.lv_mut().push_group(LvGroup::new(0, "", "T"));
        target.lv_mut().push_group(LvGroup::new(1, "", "U"));
        target.lv_mut().renumber();
        insert(&mut target, Some(1), &transfer).unwrap();
        assert_eq!(target.images().len(), 1);
        assert!(insert(&mut target, None, &Transfer::default()).is_err());
    }

    #[test]
    fn a_whole_lv_comes_with_images_and_without_prices_on_request() {
        let (source, _) = source();
        let images = images_of(&source, source.lv().rows());
        assert_eq!(images.len(), 1);
        let mut target = project("Ziel", "11.PPPP");
        target.add_image("schrank.png", vec![7]);
        let index = add_lv(&mut target, source.lv().clone(), &images, "2", false);
        assert_eq!(index, 1);
        let lv = target.lv_at(index).unwrap();
        assert_eq!(lv.number(), "2");
        assert_eq!(lv.rows().len(), source.lv().rows().len());
        let ddc = lv
            .positions()
            .find(|position| position.short_text() == "DDC")
            .unwrap();
        assert_eq!(ddc.unit_price(), None);
        assert_eq!(ddc.status(), EditStatus::Open);
        assert_eq!(ddc.long_text().images().next(), Some("schrank_2.png"));
        assert_eq!(target.image("schrank_2.png").unwrap().bytes, vec![1, 2, 3]);
        assert!(target.bids_of(index).is_empty());
    }

    #[test]
    fn under_a_title_goes_one_level_deeper_at_its_end() {
        let (source, ids) = source();
        let transfer = collect(&source, source.lv(), &[ids[1]], TransferOptions::default());
        let mut library = project("Bibliothek", "11.222.33.44.PPPP");
        library
            .lv_mut()
            .push_group(LvGroup::new(0, "", "Raumautomation"));
        library.lv_mut().push_group(LvGroup::new(0, "", "Feld"));
        library.lv_mut().renumber();
        let range = insert_under(&mut library, 0, &transfer).unwrap();
        let lv = library.lv();
        assert_eq!(range.start, 1);
        assert_eq!(
            lv.rows()[1].as_group().unwrap().level(),
            1,
            "ASP01 below Raumautomation"
        );
        assert_eq!(lv.rows()[1].ordinal(), "01.001");
        assert_eq!(lv.rows()[range.end].short_text(), "Feld");
        assert!(
            insert_under(&mut library, 2, &transfer).is_err(),
            "not a title"
        );
    }

    #[test]
    fn a_whole_lv_keeps_its_order_numbers_and_brings_its_images() {
        let (mut source, _) = source();
        source
            .lv_mut()
            .set_request_exported(Some("2026-11-01".into()));
        let mut target = project("Ziel", "11.PPPP");
        target.add_image("schrank.png", vec![7]);
        let index = add_lv_from(&mut target, &source, source.lv());
        assert_eq!(index, 1);
        assert_eq!(target.active_lv(), 0, "the new LV does not become active");
        let lv = target.lv_at(1).unwrap();
        assert_eq!(lv.oz_mask().pattern(), "11.22.PPPP");
        let ordinals: Vec<&str> = lv.rows().iter().map(LvRow::ordinal).collect();
        let source_ordinals: Vec<&str> = source.lv().rows().iter().map(LvRow::ordinal).collect();
        assert_eq!(ordinals, source_ordinals, "order numbers as in the source");
        assert!(lv.request_exported().is_none());
        let ddc = lv
            .positions()
            .find(|position| position.short_text() == "DDC")
            .unwrap();
        assert_eq!(ddc.status(), EditStatus::Open);
        let image = ddc.long_text().images().next().unwrap().to_string();
        assert_eq!(image, "schrank_2.png");
        assert_eq!(target.image(&image).unwrap().bytes, vec![1, 2, 3]);
        let again = add_lv_from(&mut target, &source, source.lv());
        assert_eq!(again, 2);
        assert_eq!(
            target.images().len(),
            2,
            "same content: reused, not stored twice"
        );
        let second = target.lv_at(2).unwrap();
        let ddc = second
            .positions()
            .find(|position| position.short_text() == "DDC")
            .unwrap();
        assert_eq!(ddc.long_text().images().next(), Some("schrank_2.png"));
    }

    /// Gefunden mit den Eigenschaftstests: Titel ließen sich in ein LV
    /// ohne Titelstufe übernehmen – dort kann sie keine OZ nummerieren. Wie
    /// beim Einfügen kommt nur der Inhalt; nur Titel werden abgelehnt.
    #[test]
    fn into_a_mask_without_titles_only_the_content_comes() {
        let (source, ids) = source();
        let transfer = collect(&source, source.lv(), &[ids[0]], TransferOptions::default());
        let mut flat = project("Flach", "PPPP");
        let range = insert(&mut flat, None, &transfer).unwrap();
        let lv = flat.lv();
        assert!(lv.rows().iter().all(|row| !row.is_group()), "keine Titel");
        assert_eq!(range.len(), 3, "AB, DDC, Modul");
        assert!(lv.duplicate_ordinals().is_empty());
        let only_title = Transfer {
            rows: vec![LvRow::Group(LvGroup::new(0, "", "Nur Titel"))],
            images: Vec::new(),
        };
        let before = flat.clone();
        let error = insert(&mut flat, None, &only_title).unwrap_err();
        assert!(error.contains("keine Titelstufe"), "{error}");
        assert_eq!(flat, before);
    }
}
