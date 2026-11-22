//! Values a title passes down to the rows below it (Kostengruppe,
//! Leistungsbereich): a row's own value wins, else that of the nearest title
//! above it (titles: their own, else their parent's).

use std::collections::HashMap;

use crate::lv::{Lv, LvRow, RowId};

/// The own value of a row (titles and positions; others have none).
pub(crate) type Own = fn(&LvRow) -> Option<&str>;

/// Value that applies to the row at `index`, `true` = inherited from a title.
pub(crate) fn effective_value(lv: &Lv, index: usize, own: Own) -> Option<(String, bool)> {
    let rows = lv.rows();
    let row = rows.get(index)?;
    if let Some(code) = own(row) {
        return Some((code.to_string(), false));
    }
    let mut level_limit = row
        .as_group()
        .map(|group| group.level())
        .unwrap_or(usize::MAX);
    for above in rows[..index].iter().rev() {
        if let LvRow::Group(group) = above {
            if group.level() < level_limit {
                if let Some(code) = own(above) {
                    return Some((code.to_string(), true));
                }
                level_limit = group.level();
            }
        }
    }
    None
}

/// Index of the title the row at `index` inherits its value from; None when
/// the row has its own value or nothing to inherit.
pub(crate) fn inherited_from(lv: &Lv, index: usize, own: Own) -> Option<usize> {
    let rows = lv.rows();
    let row = rows.get(index)?;
    if own(row).is_some() {
        return None;
    }
    let mut level_limit = row
        .as_group()
        .map(|group| group.level())
        .unwrap_or(usize::MAX);
    for (above_index, above) in rows[..index].iter().enumerate().rev() {
        if let LvRow::Group(group) = above {
            if group.level() < level_limit {
                if own(above).is_some() {
                    return Some(above_index);
                }
                level_limit = group.level();
            }
        }
    }
    None
}

/// [`effective_value`] of every row in one pass; the titles at the indices
/// in `titles` carry the given value instead of their own.
pub(crate) fn effective_values(
    lv: &Lv,
    own: Own,
    titles: &HashMap<usize, Option<&str>>,
) -> Vec<Option<(String, bool)>> {
    // Titles above the current row: (level, value that applies).
    let mut stack: Vec<(usize, Option<String>)> = Vec::new();
    let mut result = Vec::with_capacity(lv.rows().len());
    for (index, row) in lv.rows().iter().enumerate() {
        if let LvRow::Group(group) = row {
            while stack
                .last()
                .is_some_and(|(level, _)| *level >= group.level())
            {
                stack.pop();
            }
        }
        let parent = stack.last().and_then(|(_, code)| code.clone());
        let mine = match row {
            LvRow::Group(_) if titles.contains_key(&index) => titles[&index].map(str::to_string),
            _ => own(row).map(str::to_string),
        };
        let effective = match mine {
            Some(code) => Some((code, false)),
            None => parent.map(|code| (code, true)),
        };
        if let LvRow::Group(group) = row {
            stack.push((
                group.level(),
                effective.as_ref().map(|(code, _)| code.clone()),
            ));
        }
        result.push(effective);
    }
    result
}

/// Finished positions whose value would change if the titles at `titles`
/// got `code`: they have none of their own and inherit it. Rows that are no
/// titles are ignored.
pub(crate) fn locked_heirs(lv: &Lv, titles: &[usize], code: Option<&str>, own: Own) -> Vec<RowId> {
    let changes: HashMap<usize, Option<&str>> = titles
        .iter()
        .filter(|index| lv.rows().get(**index).is_some_and(LvRow::is_group))
        .map(|index| (*index, code))
        .collect();
    if changes.is_empty() {
        return Vec::new();
    }
    let before = effective_values(lv, own, &HashMap::new());
    let after = effective_values(lv, own, &changes);
    lv.rows()
        .iter()
        .enumerate()
        .filter_map(|(index, row)| match row {
            LvRow::Position(position)
                if position.is_locked()
                    && before[index].as_ref().map(|(code, _)| code)
                        != after[index].as_ref().map(|(code, _)| code) =>
            {
                Some(position.id())
            }
            _ => None,
        })
        .collect()
}
