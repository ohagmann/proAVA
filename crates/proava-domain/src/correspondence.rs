//! Schriftverkehr-Archiv: sent letters as PDF inside the project (folder
//! `schriftverkehr/` of the project file), with a list of what went to whom.
//! One entry per recipient – a collective PDF of several bidders is filed as
//! one letter per bidder, so the archive answers „what did Müller get“.

use std::fmt;
use std::sync::Arc;

use crate::invitation::InviteeId;
use crate::letters::LetterKind;
use crate::project::Project;

/// From this size of all filed PDFs on, proAVA mentions that the project
/// file grows with them.
pub const ARCHIVE_HINT_BYTES: u64 = 25 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct CorrespondenceId(pub u64);

/// What kind of letter an entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CorrespondenceKind {
    /// Made from a template of the Schriftverkehr.
    Letter(LetterKind),
    /// Any other PDF filed by the user (eigenes Schreiben).
    Own,
}

impl CorrespondenceKind {
    pub const ALL: [CorrespondenceKind; 5] = [
        CorrespondenceKind::Letter(LetterKind::Invitation),
        CorrespondenceKind::Letter(LetterKind::Award),
        CorrespondenceKind::Letter(LetterKind::Rejection),
        CorrespondenceKind::Letter(LetterKind::Release),
        CorrespondenceKind::Own,
    ];

    /// Short name for lists.
    pub fn label(self) -> &'static str {
        match self {
            CorrespondenceKind::Letter(LetterKind::Invitation) => "Aufforderung",
            CorrespondenceKind::Letter(LetterKind::Award) => "Zuschlag",
            CorrespondenceKind::Letter(LetterKind::Rejection) => "Absage",
            CorrespondenceKind::Letter(LetterKind::Release) => "Freigabe an den Bauherrn",
            CorrespondenceKind::Own => "Eigenes Schreiben",
        }
    }

    /// Key in the project file and the MCP server.
    pub fn key(self) -> &'static str {
        match self {
            CorrespondenceKind::Letter(kind) => kind.key(),
            CorrespondenceKind::Own => "eigene",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        let key = key.trim().to_lowercase();
        Self::ALL.into_iter().find(|kind| kind.key() == key)
    }

    /// Part of the file name in the archive.
    fn file_label(self) -> &'static str {
        match self {
            CorrespondenceKind::Letter(kind) => kind.file_label(),
            CorrespondenceKind::Own => "Schreiben",
        }
    }
}

/// One filed letter.
#[derive(Clone, PartialEq)]
pub struct CorrespondenceEntry {
    pub id: CorrespondenceId,
    pub kind: CorrespondenceKind,
    /// LV the letter belongs to (number and title when it was filed).
    pub lv_number: String,
    pub lv_title: String,
    /// The invited bidder (Bieterverzeichnis of that LV), if any.
    pub recipient: Option<InviteeId>,
    /// Name of the recipient (bidder, Bauherr …) when the letter was filed.
    pub recipient_name: String,
    /// Date of the letter, TT.MM.JJJJ.
    pub date: String,
    pub subject: String,
    /// File name inside `schriftverkehr/` of the project file (unique).
    pub file: String,
    /// Size of the PDF in bytes (also when the PDF itself is missing).
    pub size: u64,
    pub remark: String,
    /// The PDF; empty when the project file lacked it. Shared, so that
    /// copies of the project (undo, background work) cost nothing.
    pub pdf: Arc<Vec<u8>>,
}

impl fmt::Debug for CorrespondenceEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CorrespondenceEntry")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("lv_number", &self.lv_number)
            .field("recipient", &self.recipient)
            .field("recipient_name", &self.recipient_name)
            .field("date", &self.date)
            .field("subject", &self.subject)
            .field("file", &self.file)
            .field("size", &self.size)
            .field("remark", &self.remark)
            .field("pdf_bytes", &self.pdf.len())
            .finish()
    }
}

impl CorrespondenceEntry {
    /// A new entry with the PDF; id and file name are set when it is added
    /// to a project ([`Project::add_correspondence`]).
    pub fn new(
        kind: CorrespondenceKind,
        date: impl Into<String>,
        subject: impl Into<String>,
        pdf: Vec<u8>,
    ) -> Self {
        Self {
            id: CorrespondenceId::default(),
            kind,
            lv_number: String::new(),
            lv_title: String::new(),
            recipient: None,
            recipient_name: String::new(),
            date: date.into(),
            subject: subject.into(),
            file: String::new(),
            size: pdf.len() as u64,
            remark: String::new(),
            pdf: Arc::new(pdf),
        }
    }

    /// The same entry with this PDF (and its size).
    pub fn with_pdf(mut self, pdf: Vec<u8>) -> Self {
        self.size = pdf.len() as u64;
        self.pdf = Arc::new(pdf);
        self
    }

    /// „LV 1 – Gebäudeautomation“ (or only one of both).
    pub fn lv_label(&self) -> String {
        match (self.lv_number.trim(), self.lv_title.trim()) {
            ("", title) => title.to_string(),
            (number, "") => format!("LV {number}"),
            (number, title) => format!("LV {number} – {title}"),
        }
    }

    /// The PDF is in the project (not lost in a damaged file).
    pub fn has_pdf(&self) -> bool {
        !self.pdf.is_empty()
    }

    /// Date as JJJJMMTT for sorting (unknown dates first).
    pub fn sort_date(&self) -> String {
        let parts: Vec<&str> = self.date.trim().split('.').collect();
        match parts.as_slice() {
            [day, month, year] => format!("{year:0>4}{month:0>2}{day:0>2}"),
            _ => String::new(),
        }
    }
}

/// Entry for the letter of `kind` to the invited bidder `invitee` of the
/// active LV, with the subject as printed (template filled for that bidder).
/// `None` when the bidder is not in the Bieterverzeichnis.
pub fn bidder_letter_entry(
    project: &Project,
    kind: LetterKind,
    invitee: InviteeId,
    date: &str,
    pdf: Vec<u8>,
) -> Option<CorrespondenceEntry> {
    let bidder = project
        .invitees()
        .iter()
        .find(|known| known.id == invitee)?;
    let values = crate::letters::letter_values(project, bidder, date);
    let (subject, _) = project.letter_template(kind).filled(&values);
    let mut entry =
        CorrespondenceEntry::new(CorrespondenceKind::Letter(kind), date, subject.trim(), pdf);
    entry.recipient = Some(invitee);
    entry.recipient_name = bidder.name().trim().to_string();
    entry.lv_number = project.lv().number().to_string();
    entry.lv_title = project.lv().title().to_string();
    Some(entry)
}

/// Entry for the letter to the client (Auftraggeber, else Bauherr) about the
/// release of `invoice` (None = the last one) of the active LV.
pub fn release_letter_entry(
    project: &Project,
    invoice: Option<crate::billing::InvoiceId>,
    date: &str,
    pdf: Vec<u8>,
) -> Result<CorrespondenceEntry, String> {
    let kind = LetterKind::Release;
    let values = crate::letters::release_letter_values(project, invoice, date)?;
    let (subject, _) = project.letter_template(kind).filled(&values);
    let client = project.info().client();
    let name = [client.name1.trim(), client.name2.trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut entry =
        CorrespondenceEntry::new(CorrespondenceKind::Letter(kind), date, subject.trim(), pdf);
    entry.recipient_name = name;
    entry.lv_number = project.lv().number().to_string();
    entry.lv_title = project.lv().title().to_string();
    Ok(entry)
}

/// Letters, digits and `-`; the rest `_`, at most 40 characters.
fn file_part(text: &str) -> String {
    let cleaned: String = text
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut parts: Vec<&str> = cleaned.split('_').filter(|part| !part.is_empty()).collect();
    parts.dedup();
    parts.join("_").chars().take(40).collect()
}

impl Project {
    /// The filed letters, oldest first.
    pub fn correspondence(&self) -> &[CorrespondenceEntry] {
        self.correspondence_list()
    }

    pub fn correspondence_entry(&self, id: CorrespondenceId) -> Option<&CorrespondenceEntry> {
        self.correspondence_list()
            .iter()
            .find(|entry| entry.id == id)
    }

    /// Files a letter: new id and a unique file name
    /// (`0003_Absage_Mueller_GmbH.pdf`); an empty LV is the active one.
    pub fn add_correspondence(&mut self, mut entry: CorrespondenceEntry) -> CorrespondenceId {
        let id = CorrespondenceId(
            self.correspondence_list()
                .iter()
                .map(|known| known.id.0)
                .max()
                .unwrap_or(0)
                + 1,
        );
        entry.id = id;
        if entry.lv_number.is_empty() && entry.lv_title.is_empty() {
            entry.lv_number = self.lv().number().to_string();
            entry.lv_title = self.lv().title().to_string();
        }
        if !entry.pdf.is_empty() {
            entry.size = entry.pdf.len() as u64;
        }
        let wanted = if entry.file.trim().is_empty() {
            let who = file_part(&entry.recipient_name);
            let mut name = format!("{:04}_{}", id.0, entry.kind.file_label());
            if !who.is_empty() {
                name.push('_');
                name.push_str(&who);
            }
            name
        } else {
            let stem = entry
                .file
                .trim()
                .trim_end_matches(".pdf")
                .trim_end_matches(".PDF");
            file_part(stem.rsplit(['/', '\\']).next().unwrap_or(stem))
        };
        let wanted = if wanted.is_empty() {
            format!("{:04}_Schreiben", id.0)
        } else {
            wanted
        };
        let taken = |name: &str| {
            self.correspondence_list()
                .iter()
                .any(|known| known.file == name)
        };
        let mut file = format!("{wanted}.pdf");
        let mut counter = 2;
        while taken(&file) {
            file = format!("{wanted}_{counter}.pdf");
            counter += 1;
        }
        entry.file = file;
        self.correspondence_list_mut().push(entry);
        id
    }

    /// Removes an entry with its PDF.
    pub fn remove_correspondence(&mut self, id: CorrespondenceId) -> Option<CorrespondenceEntry> {
        let list = self.correspondence_list_mut();
        let index = list.iter().position(|entry| entry.id == id)?;
        Some(list.remove(index))
    }

    /// Bemerkung of an entry.
    pub fn set_correspondence_remark(&mut self, id: CorrespondenceId, remark: &str) -> bool {
        match self
            .correspondence_list_mut()
            .iter_mut()
            .find(|entry| entry.id == id)
        {
            Some(entry) => {
                entry.remark = remark.to_string();
                true
            }
            None => false,
        }
    }

    /// Puts back a whole archive (Rückgängig) and returns the one that was there.
    pub fn replace_correspondence(
        &mut self,
        entries: Vec<CorrespondenceEntry>,
    ) -> Vec<CorrespondenceEntry> {
        std::mem::replace(self.correspondence_list_mut(), entries)
    }

    /// Size of all filed PDFs in bytes.
    pub fn correspondence_size(&self) -> u64 {
        self.correspondence_list()
            .iter()
            .map(|entry| entry.size)
            .sum()
    }

    /// The archive as it is now, for Rückgängig (the PDFs are shared).
    pub fn correspondence_snapshot(&self) -> Vec<CorrespondenceEntry> {
        self.correspondence_list().to_vec()
    }

    /// Names of all recipients in the archive, sorted, each once (filter).
    pub fn correspondence_recipients(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .correspondence_list()
            .iter()
            .map(|entry| entry.recipient_name.trim().to_string())
            .filter(|name| !name.is_empty())
            .collect();
        names.sort_by_key(|name| name.to_lowercase());
        names.dedup();
        names
    }

    /// „Der Schriftverkehr im Projekt ist 31 MB groß …“ once the filed PDFs
    /// pass [`ARCHIVE_HINT_BYTES`].
    pub fn correspondence_hint(&self) -> Option<String> {
        let size = self.correspondence_size();
        (size >= ARCHIVE_HINT_BYTES).then(|| {
            format!(
                "Der abgelegte Schriftverkehr ist {} groß; die Projektdatei und jede Sicherung wachsen damit. \
                 Nicht mehr benötigte Schreiben lassen sich in der Liste entfernen.",
                size_label(size)
            )
        })
    }
}

/// „12 KB“, „3,4 MB“
pub fn size_label(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    if bytes < KB {
        format!("{bytes} B")
    } else if bytes < MB {
        format!("{} KB", bytes.div_ceil(KB))
    } else {
        let tenths = (bytes * 10).div_ceil(MB);
        format!("{},{} MB", tenths / 10, tenths % 10)
    }
}

impl Project {
    /// Entry as read from a project file: id and file name are kept (the
    /// PDF lies there under that name).
    pub fn restore_correspondence(&mut self, entry: CorrespondenceEntry) {
        let clash = self
            .correspondence_list()
            .iter()
            .any(|known| known.id == entry.id || known.file == entry.file);
        if clash || entry.file.trim().is_empty() {
            self.add_correspondence(entry);
        } else {
            self.correspondence_list_mut().push(entry);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Lv, OzMask, ProjectName};

    fn project() -> Project {
        let mut lv = Lv::new(
            "Gebäudeautomation",
            OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap(),
        );
        lv.set_number("480");
        Project::new(ProjectName::parse("Test").unwrap(), "2026-11-18", lv)
    }

    #[test]
    fn entries_get_ids_unique_file_names_and_the_lv() {
        let mut project = project();
        let mut entry = CorrespondenceEntry::new(
            CorrespondenceKind::Letter(LetterKind::Rejection),
            "18.11.2026",
            "Information über den Ausgang",
            vec![1, 2, 3],
        );
        entry.recipient_name = "Müller & Söhne GmbH".into();
        entry.recipient = Some(InviteeId(4));
        let first = project.add_correspondence(entry.clone());
        let second = project.add_correspondence(entry.clone());
        assert_ne!(first, second);
        let list = project.correspondence();
        assert_eq!(list[0].file, "0001_Absage_Müller_Söhne_GmbH.pdf");
        assert_eq!(list[1].file, "0002_Absage_Müller_Söhne_GmbH.pdf");
        assert_eq!(list[0].lv_label(), "LV 480 – Gebäudeautomation");
        assert_eq!(list[0].size, 3);
        assert_eq!(project.correspondence_size(), 6);

        let mut own = CorrespondenceEntry::new(CorrespondenceKind::Own, "", "Protokoll", vec![9]);
        own.file = "/tmp/Protokoll Bieter.pdf".into();
        let own = project.add_correspondence(own.clone());
        assert_eq!(
            project.correspondence_entry(own).unwrap().file,
            "Protokoll_Bieter.pdf"
        );
        let mut same = CorrespondenceEntry::new(CorrespondenceKind::Own, "", "Protokoll", vec![9]);
        same.file = "Protokoll Bieter.pdf".into();
        let same = project.add_correspondence(same);
        assert_eq!(
            project.correspondence_entry(same).unwrap().file,
            "Protokoll_Bieter_2.pdf"
        );

        assert!(project.set_correspondence_remark(first, "per Mail"));
        let before = project.replace_correspondence(Vec::new());
        assert_eq!(before.len(), 4);
        project.replace_correspondence(before);
        assert!(project.remove_correspondence(first).is_some());
        assert!(project.correspondence_entry(first).is_none());
        assert_eq!(project.correspondence().len(), 3);
    }

    #[test]
    fn letters_become_entries_with_the_printed_subject() {
        use crate::{Address, InvitedBidder};
        let mut project = project();
        let id = project.add_invitee(InvitedBidder::new(Address::named("Müller GmbH")));
        let entry = bidder_letter_entry(
            &project,
            LetterKind::Rejection,
            id,
            "18.11.2026",
            b"%PDF".to_vec(),
        )
        .unwrap();
        assert_eq!(entry.recipient, Some(id));
        assert_eq!(entry.recipient_name, "Müller GmbH");
        assert_eq!(entry.lv_label(), "LV 480 – Gebäudeautomation");
        assert!(
            !entry.subject.contains('{'),
            "Platzhalter gefüllt: {}",
            entry.subject
        );
        assert_eq!(entry.size, 4);
        assert!(
            bidder_letter_entry(&project, LetterKind::Award, InviteeId(99), "", Vec::new())
                .is_none()
        );
        // Without Auftrag and Rechnung no letter to the client.
        assert!(release_letter_entry(&project, None, "18.11.2026", Vec::new()).is_err());
    }

    #[test]
    fn size_hint_from_25_mb_on() {
        let mut project = project();
        assert_eq!(size_label(500), "500 B");
        assert_eq!(size_label(2048), "2 KB");
        assert_eq!(size_label(3 * 1024 * 1024 + 1), "3,1 MB");
        let mut entry = CorrespondenceEntry::new(CorrespondenceKind::Own, "", "x", Vec::new());
        entry.size = ARCHIVE_HINT_BYTES - 1;
        entry.file = "a.pdf".into();
        project.restore_correspondence(entry.clone());
        assert!(project.correspondence_hint().is_none());
        entry.size = 1;
        entry.file = "b.pdf".into();
        entry.id = CorrespondenceId(7);
        project.restore_correspondence(entry);
        let hint = project.correspondence_hint().unwrap();
        assert!(hint.contains("25,0 MB"), "{hint}");
        assert_eq!(
            project.correspondence()[1].id,
            CorrespondenceId(7),
            "Id aus der Datei bleibt"
        );
    }

    #[test]
    fn kinds_have_keys_and_dates_sort() {
        for kind in CorrespondenceKind::ALL {
            assert_eq!(CorrespondenceKind::from_key(kind.key()), Some(kind));
        }
        assert_eq!(CorrespondenceKind::from_key("brief"), None);
        let entry = CorrespondenceEntry::new(CorrespondenceKind::Own, "3.2.2027", "x", Vec::new());
        assert_eq!(entry.sort_date(), "20270203");
        assert!(!entry.has_pdf());
        let debug = format!("{entry:?}");
        assert!(debug.contains("pdf_bytes: 0"), "{debug}");
    }
}
