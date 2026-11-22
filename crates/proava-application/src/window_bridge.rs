//! The local bridge seen from the program: the port of the channel and what
//! both sides need to name rows (selection, OZ, ids, „die nächsten drei“) and
//! to describe them in answers. The protocol itself is in
//! `proava-bridge-protocol`, the channel (socket, pipe, folder) in the
//! infrastructure, the handling of the commands in the UI.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

pub use proava_bridge_protocol as protocol;
use proava_bridge_protocol::{LvInfo, Reply, Request, Response, RowInfo, Rows, WindowEntry};
use proava_domain::lv_search::LvHit;
use proava_domain::{format_decimal_de, Decimal, Lv, LvRow, Project, RowId};

/// Called when a request comes in (the program wakes its window).
pub type Wake = Arc<dyn Fn() + Send + Sync>;

/// One request for the window, with the way back.
pub struct Incoming {
    pub request: Request,
    reply: Sender<Response>,
}

impl Incoming {
    /// A request and the receiver of its answer (tests).
    pub fn new(request: Request) -> (Self, Receiver<Response>) {
        let (reply, answer) = mpsc::channel();
        (Self { request, reply }, answer)
    }

    pub fn id(&self) -> u64 {
        self.request.id
    }

    /// Sends the answer (a channel that gave up is fine).
    pub fn answer(self, reply: Reply) {
        let _ = self.reply.send(Response::new(self.request.id, reply));
    }
}

/// Sends one request to the window behind a channel (socket path or pipe
/// name) and waits at most the given time – on a helper thread, never on
/// the UI thread.
pub type Messenger =
    Arc<dyn Fn(&str, &Request, Duration) -> Result<Response, String> + Send + Sync>;

/// The entries of the running windows (the own one included).
pub type WindowList = Arc<dyn Fn() -> Vec<WindowEntry> + Send + Sync>;

/// What a window needs to talk to the other windows (dragging rows over).
#[derive(Clone)]
pub struct BridgeHandle {
    /// PID of this window (its own entry is left out where it matters).
    pub pid: u32,
    pub windows: WindowList,
    pub messenger: Messenger,
    /// Where whole projects pass to and from the MCP server (see
    /// `proava_bridge_protocol::EXCHANGE_FOLDER`); None = not set up.
    pub exchange: Option<std::path::PathBuf>,
}

/// The channel of the window (Unix socket or named pipe and the entry in
/// the folder of the windows); the composition root of the program hands
/// one in, tests one in a temporary folder.
pub trait WindowChannel: Send {
    /// Opens the channel of this program (`pid`); `wake` is called for every
    /// request, which then waits in the receiver.
    fn open(&mut self, pid: u32, wake: Wake) -> Result<Receiver<Incoming>, String>;
    /// Writes the entry of the window (state at the last change).
    fn publish(&self, entry: &WindowEntry);
    /// Closes the channel and takes the entry away.
    fn close(&mut self);
    /// Where the window is reached (socket path or pipe name).
    fn address(&self) -> String;
    /// The entries of the running windows in the same folder.
    fn window_list(&self) -> WindowList;
    /// The way to send requests to other windows.
    fn messenger(&self) -> Messenger;
    /// The exchange folder of the bridge (whole projects for the MCP
    /// server); None = the channel has none.
    fn exchange_folder(&self) -> Option<std::path::PathBuf> {
        None
    }
}

/// How long the channel waits for the window before it says „beschäftigt“.
/// The window answers itself before that when a question of proAVA waits
/// for the user ([`QUESTION_PATIENCE`]).
pub const WINDOW_PATIENCE: Duration = Duration::from_secs(60);

/// How long an insert waits for the user's answer to a question of proAVA
/// before it answers „wartet auf den Anwender“.
pub const QUESTION_PATIENCE: Duration = Duration::from_secs(25);

/// What the channel does with a request: hand it to the window, wake it and
/// wait for the answer (at most `patience`).
pub fn forward(
    sender: Sender<Incoming>,
    wake: Wake,
    patience: Duration,
) -> impl Fn(Request) -> Response + Send + Sync + 'static {
    let sender = std::sync::Mutex::new(sender);
    move |request: Request| {
        let id = request.id;
        let (incoming, answer) = Incoming::new(request);
        let sent = sender
            .lock()
            .map(|sender| sender.send(incoming).is_ok())
            .unwrap_or(false);
        if !sent {
            return Response::new(id, Reply::refused("Das Fenster wird gerade geschlossen."));
        }
        wake();
        answer.recv_timeout(patience).unwrap_or_else(|_| {
            Response::new(
                id,
                Reply::refused(
                    "proAVA ist beschäftigt und hat nicht rechtzeitig geantwortet – später noch einmal versuchen.",
                ),
            )
        })
    }
}

// ----- Zeilen benennen -----

/// The LV named by number („2“, „LV 481“) or place in the list (1 = first);
/// None or empty: the active one.
pub fn lv_index(project: &Project, wanted: Option<&str>) -> Result<usize, String> {
    let wanted = wanted.map(str::trim).unwrap_or_default();
    if wanted.is_empty() {
        return Ok(project.active_lv());
    }
    let loose = wanted
        .strip_prefix("LV")
        .or_else(|| wanted.strip_prefix("lv"))
        .unwrap_or(wanted)
        .trim();
    if let Some(index) = project
        .lvs()
        .position(|lv| lv.number().trim() == wanted || lv.number().trim() == loose)
    {
        return Ok(index);
    }
    if let Ok(place) = loose.parse::<usize>() {
        if (1..=project.lv_count()).contains(&place) {
            return Ok(place - 1);
        }
    }
    Err(format!(
        "LV „{wanted}“ gibt es im Projekt nicht. Vorhanden: {}.",
        project
            .lvs()
            .enumerate()
            .map(|(index, lv)| format!("{} = {}", index + 1, lv_info_of(lv, index).label()))
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// A row by OZ („01.02.0010“, also „1.2.10“) or id („#12“, „12“ only with #).
pub fn find_row(lv: &Lv, wanted: &str) -> Result<usize, String> {
    let wanted = wanted.trim();
    if let Some(id) = wanted.strip_prefix('#') {
        let id: u64 = id
            .trim()
            .parse()
            .map_err(|_| format!("„{wanted}“ ist keine Zeilen-Id (#12)."))?;
        return lv
            .index_of(RowId(id))
            .ok_or_else(|| format!("Eine Zeile #{id} gibt es in diesem LV nicht."));
    }
    if wanted.is_empty() {
        return Err("Keine OZ angegeben.".to_string());
    }
    let exact: Vec<usize> = matching(lv, |ordinal| ordinal == wanted);
    let hits = if exact.is_empty() {
        let key = loose_key(wanted);
        matching(lv, |ordinal| loose_key(ordinal) == key)
    } else {
        exact
    };
    match hits.as_slice() {
        [index] => Ok(*index),
        [] => Err(format!("Die OZ „{wanted}“ gibt es in diesem LV nicht.")),
        several => Err(format!(
            "Die OZ „{wanted}“ gibt es mehrfach ({}) – bitte die Zeilen-Id angeben.",
            several
                .iter()
                .map(|index| format!("#{}", lv.rows()[*index].id().0))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn matching(lv: &Lv, wanted: impl Fn(&str) -> bool) -> Vec<usize> {
    lv.rows()
        .iter()
        .enumerate()
        .filter(|(_, row)| !row.ordinal().is_empty() && wanted(row.ordinal()))
        .map(|(index, _)| index)
        .collect()
}

/// „1.2.10“ and „01.02.0010“ are the same; spaces do not count.
fn loose_key(ordinal: &str) -> String {
    ordinal
        .split('.')
        .map(|piece| {
            let piece = piece.trim();
            let trimmed = piece.trim_start_matches('0');
            if trimmed.is_empty() && !piece.is_empty() {
                "0".to_string()
            } else {
                trimmed.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// The rows named in `rows` (in LV order, each once): the `selection` of the
/// window (if asked for), OZ and ids, then `weitere` further rows after the
/// last one – positions, hints and Ausführungsbeschreibungen; titles and
/// spacers do not count (they are structure).
pub fn pick_rows(lv: &Lv, rows: &Rows, selection: &[RowId]) -> Result<Vec<usize>, String> {
    let mut picked: Vec<usize> = Vec::new();
    if rows.auswahl {
        if selection.is_empty() {
            return Err("Im Fenster ist keine Zeile gewählt.".to_string());
        }
        picked.extend(selection.iter().filter_map(|id| lv.index_of(*id)));
    }
    for oz in &rows.oz {
        picked.push(find_row(lv, oz)?);
    }
    for id in &rows.ids {
        picked.push(
            lv.index_of(RowId(*id))
                .ok_or_else(|| format!("Eine Zeile #{id} gibt es in diesem LV nicht."))?,
        );
    }
    if picked.is_empty() {
        return Err("Keine Zeilen genannt (auswahl, oz oder ids).".to_string());
    }
    picked.sort_unstable();
    picked.dedup();
    if rows.weitere > 0 {
        let last = *picked.last().expect("not empty");
        let further: Vec<usize> = (last + 1..lv.rows().len())
            .filter(|&index| counts_as_further(&lv.rows()[index]))
            .take(rows.weitere)
            .collect();
        if further.len() < rows.weitere {
            return Err(format!(
                "Nach {} folgen nur noch {} – „weitere“ ist {}.",
                describe(lv, last),
                count(further.len(), "Zeile", "Zeilen"),
                rows.weitere
            ));
        }
        picked.extend(further);
    }
    Ok(picked)
}

fn counts_as_further(row: &LvRow) -> bool {
    !matches!(row, LvRow::Group(_) | LvRow::Spacer(_))
}

fn describe(lv: &Lv, index: usize) -> String {
    let row = &lv.rows()[index];
    if row.ordinal().is_empty() {
        format!("#{}", row.id().0)
    } else {
        row.ordinal().to_string()
    }
}

fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// `rows` with the titles above them left out where the title itself is in
/// the list (taking a title brings its content).
pub fn without_contained(lv: &Lv, rows: &[usize]) -> Vec<usize> {
    let mut result: Vec<usize> = Vec::new();
    let mut covered_until = 0;
    for &index in rows {
        if index < covered_until {
            continue;
        }
        if lv.rows()[index].is_group() {
            covered_until = lv.block_range(index).end;
        }
        result.push(index);
    }
    result
}

// ----- Zeilen beschreiben -----

pub fn lv_info(project: &Project, index: usize) -> LvInfo {
    project
        .lv_at(index)
        .map(|lv| lv_info_of(lv, index))
        .unwrap_or_default()
}

fn lv_info_of(lv: &Lv, index: usize) -> LvInfo {
    LvInfo {
        stelle: index + 1,
        nummer: lv.number().trim().to_string(),
        titel: lv.title().trim().to_string(),
        positionen: lv.position_count(),
    }
}

/// Titles above a row: „01 Raumautomation › 01.02 Bediengeräte“.
pub fn title_path(lv: &Lv, index: usize) -> String {
    let rows = lv.rows();
    let mut limit = rows
        .get(index)
        .and_then(LvRow::as_group)
        .map(|group| group.level())
        .unwrap_or(usize::MAX);
    let mut titles = Vec::new();
    for row in rows[..index.min(rows.len())].iter().rev() {
        if let LvRow::Group(group) = row {
            if group.level() < limit {
                titles.push(format!("{} {}", group.ordinal(), group.short_text()));
                limit = group.level();
            }
        }
    }
    titles.reverse();
    titles
        .iter()
        .map(|title| title.trim())
        .collect::<Vec<_>>()
        .join(" › ")
}

fn number(value: Decimal) -> String {
    let value = value.normalize();
    format_decimal_de(value, value.scale().min(3))
}

fn price(value: Decimal) -> String {
    format_decimal_de(value, value.normalize().scale().clamp(2, 3))
}

/// One row for an answer; `long_text` adds its text as plain text.
pub fn row_info(lv: &Lv, index: usize, long_text: bool) -> RowInfo {
    let row = &lv.rows()[index];
    let groups = lv.effective_cost_group(index);
    let mut info = RowInfo {
        id: row.id().0,
        oz: row.ordinal().to_string(),
        kurztext: row.short_text(),
        titelpfad: title_path(lv, index),
        kg: groups.map(|(code, _)| code).unwrap_or_default(),
        ..RowInfo::default()
    };
    match row {
        LvRow::Group(group) => {
            info.art = "Titel".to_string();
            let positions = lv.rows()[lv.block_range(index)]
                .iter()
                .filter(|row| row.is_position())
                .count();
            info.menge = count(positions, "Position", "Positionen");
            let _ = group;
        }
        LvRow::Position(position) => {
            info.art = "Position".to_string();
            info.menge = number(position.quantity());
            info.einheit = position.unit().to_string();
            info.ep = position.unit_price().map(price).unwrap_or_default();
            if position.unit_price().is_some() {
                info.gp = position.total_price().plain();
            }
            info.status = match position.status() {
                proava_domain::EditStatus::Open => String::new(),
                status => status.label().to_string(),
            };
            if long_text {
                info.langtext = Some(position.long_text().to_plain());
            }
        }
        LvRow::Note(note) => {
            info.art = "Hinweis".to_string();
            if long_text {
                info.langtext = Some(note.text().to_plain());
            }
        }
        LvRow::Performance(performance) => {
            info.art = "Ausführungsbeschreibung".to_string();
            info.oz = performance.mark();
            if long_text {
                info.langtext = Some(performance.long_text().to_plain());
            }
        }
        LvRow::Spacer(_) => info.art = "Abstand".to_string(),
    }
    info
}

/// A search hit for an answer (with the piece of text around the find).
pub fn hit_info(lv: &Lv, hit: &LvHit) -> RowInfo {
    let mut info = row_info(lv, hit.index, false);
    info.titelpfad = hit.path_text();
    let excerpt = hit.excerpt.trim();
    info.fundstelle = match hit.field {
        // Words in the Kurztext (shown anyway): the excerpt is the start of the long text.
        _ if excerpt.is_empty() => String::new(),
        proava_domain::lv_search::HitField::ShortText => format!("Langtext: {excerpt}"),
        field => format!("{}: {excerpt}", field.label()),
    };
    info
}

#[cfg(test)]
mod tests {
    use super::*;
    use proava_domain::{LvGroup, OzMask, Position, ProjectName};

    fn lv() -> Lv {
        let mask = OzMask::parse("11.PPPP").unwrap();
        let mut lv = Lv::new("Gebäudeautomation", mask);
        let mut rows = vec![LvRow::Group(LvGroup::new(0, "01", "Schaltschränke"))];
        for (oz, text) in [
            ("01.0010", "Schaltschrank ASP01"),
            ("01.0020", "Schaltschrank ASP02"),
        ] {
            rows.push(LvRow::Position(Position::new(
                oz,
                text,
                Decimal::from(2),
                "St",
                None,
            )));
        }
        rows.push(LvRow::Spacer(proava_domain::Spacer::new()));
        rows.push(LvRow::Group(LvGroup::new(0, "02", "Feldgeräte")));
        for (oz, text) in [
            ("02.0010", "Raumfühler"),
            ("02.0020", "Kanalfühler"),
            ("02.0030", "Außenfühler"),
        ] {
            rows.push(LvRow::Position(Position::new(
                oz,
                text,
                Decimal::ZERO,
                "St",
                None,
            )));
        }
        for row in rows {
            let at = lv.rows().len();
            lv.insert_row(at, row);
        }
        lv
    }

    #[test]
    fn rows_by_selection_oz_id_and_the_next_ones() {
        let lv = lv();
        let id = |index: usize| lv.rows()[index].id();
        let picked = pick_rows(
            &lv,
            &Rows {
                auswahl: true,
                weitere: 3,
                ..Rows::default()
            },
            &[id(2)],
        )
        .unwrap();
        // 01.0020, then (spacer and title 02 skipped) 02.0010, 02.0020, 02.0030
        let names: Vec<&str> = picked.iter().map(|&i| lv.rows()[i].ordinal()).collect();
        assert_eq!(names, ["01.0020", "02.0010", "02.0020", "02.0030"]);
        let picked = pick_rows(
            &lv,
            &Rows {
                oz: vec!["2.30".into(), "01.0010".into()],
                ids: vec![id(1).0],
                ..Rows::default()
            },
            &[],
        )
        .unwrap();
        assert_eq!(picked, [1, 7], "LV-Reihenfolge, jede Zeile einmal");
        let error = pick_rows(
            &lv,
            &Rows {
                oz: vec!["02.0020".into()],
                weitere: 2,
                ..Rows::default()
            },
            &[],
        )
        .unwrap_err();
        assert!(error.contains("nur noch 1 Zeile"), "{error}");
        assert!(pick_rows(&lv, &Rows::selection(), &[])
            .unwrap_err()
            .contains("keine Zeile gewählt"));
        assert!(pick_rows(&lv, &Rows::default(), &[])
            .unwrap_err()
            .contains("Keine Zeilen"));
        assert!(find_row(&lv, "#999").unwrap_err().contains("#999"));
        assert!(find_row(&lv, "03.0010").unwrap_err().contains("03.0010"));
        assert_eq!(find_row(&lv, &format!("#{}", id(5).0)), Ok(5));
    }

    #[test]
    fn titles_bring_their_content() {
        let lv = lv();
        assert_eq!(without_contained(&lv, &[0, 1, 2, 6]), vec![0, 6]);
        assert_eq!(without_contained(&lv, &[1, 4, 5, 6]), vec![1, 4]);
        assert_eq!(without_contained(&lv, &[1, 5, 6]), vec![1, 5, 6]);
    }

    #[test]
    fn rows_are_described_for_answers() {
        let lv = lv();
        let info = row_info(&lv, 1, true);
        assert_eq!(info.oz, "01.0010");
        assert_eq!(info.art, "Position");
        assert_eq!(info.menge, "2");
        assert_eq!(info.einheit, "St");
        assert_eq!(info.titelpfad, "01 Schaltschränke");
        assert_eq!(info.langtext.as_deref(), Some(""));
        let title = row_info(&lv, 0, false);
        assert_eq!(title.art, "Titel");
        assert_eq!(title.menge, "2 Positionen");
        assert_eq!(row_info(&lv, 3, false).art, "Abstand");
    }

    #[test]
    fn lvs_by_number_or_place() {
        let mut project = Project::new(
            ProjectName::parse("Rathaus 3").unwrap(),
            "2026-11-18T10:00:00",
            lv(),
        );
        let mut second = lv();
        second.set_number("481");
        project.add_lv(second);
        assert_eq!(lv_index(&project, None), Ok(0));
        assert_eq!(lv_index(&project, Some("481")), Ok(1));
        assert_eq!(lv_index(&project, Some("LV 481")), Ok(1));
        assert_eq!(lv_index(&project, Some("2")), Ok(1));
        let error = lv_index(&project, Some("7")).unwrap_err();
        assert!(error.contains("2 = LV 481 Gebäudeautomation"), "{error}");
        assert_eq!(lv_info(&project, 1).label(), "LV 481 Gebäudeautomation");
    }

    #[test]
    fn requests_wait_for_the_window_and_give_up() {
        use proava_bridge_protocol::Command;
        let (sender, receiver) = mpsc::channel::<Incoming>();
        let woken = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = woken.clone();
        let handler = forward(
            sender,
            Arc::new(move || {
                count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }),
            Duration::from_millis(300),
        );
        let window = std::thread::spawn(move || {
            let incoming = receiver.recv().unwrap();
            assert_eq!(incoming.id(), 4);
            incoming.answer(Reply::refused("gesehen"));
            // The second one is never answered.
            let _late = receiver.recv().unwrap();
            std::thread::sleep(Duration::from_millis(600));
        });
        let answer = handler(Request::new(4, Command::FensterLesen));
        assert_eq!(answer.antwort.refusal(), Some("gesehen"));
        let answer = handler(Request::new(5, Command::FensterLesen));
        assert_eq!(answer.id, 5);
        assert!(answer.antwort.refusal().unwrap().contains("beschäftigt"));
        assert_eq!(woken.load(std::sync::atomic::Ordering::SeqCst), 2);
        window.join().unwrap();
    }
}
