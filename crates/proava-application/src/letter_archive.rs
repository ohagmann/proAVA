//! Schriftverkehr: write letters (one PDF or one per bidder) and, on
//! request, file them in the project (Schriftverkehr-Archiv).
//!
//! The archive keeps **one entry per recipient** with exactly the letter
//! that recipient got – also when the user prints all letters into one
//! collective PDF. Reason: the archive has to answer „what did Müller get,
//! and when“ (Absage nach § 134 GWB, Zuschlag); a collective PDF would mix
//! the bidders, could not be filtered by bidder, and removing one bidder's
//! letter would mean cutting pages. Letters number their pages per letter,
//! so the filed copy has the same pages as in the collective PDF.

use std::path::{Path, PathBuf};

use proava_domain::{CorrespondenceEntry, InviteeId, PageFrame, Project};

use crate::exchange::{ExchangeFiles, LetterRequest, PdfLogo};
use crate::print::PrintLook;

/// Where the letters go.
#[derive(Debug, Clone, PartialEq)]
pub enum LetterTarget {
    /// All letters in one PDF.
    One(PathBuf),
    /// One PDF per bidder (the release letter has only one recipient).
    PerRecipient(Vec<(InviteeId, PathBuf)>),
}

/// One run of the Schriftverkehr.
#[derive(Debug, Clone)]
pub struct LetterJob {
    pub request: LetterRequest,
    pub target: LetterTarget,
    /// File each letter in the project („Im Projekt ablegen“).
    pub file_in_project: bool,
    pub logo: Option<PdfLogo>,
    pub frame: PageFrame,
    pub look: PrintLook,
    /// Date of the letters, TT.MM.JJJJ (archive entries).
    pub date: String,
}

/// What was written and what to file.
#[derive(Debug, Clone, Default)]
pub struct LetterOutput {
    pub pages: usize,
    pub written: Vec<PathBuf>,
    /// Entries for [`Project::add_correspondence`], one per recipient.
    pub filed: Vec<CorrespondenceEntry>,
}

fn file_name(file: &Path) -> String {
    file.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Writes the letters of `job` for the active LV of `project`. Files already
/// written stay when a later one fails (the error names the file).
pub fn write_letters(
    exchange: &dyn ExchangeFiles,
    project: &Project,
    job: &LetterJob,
) -> Result<LetterOutput, String> {
    let mut output = LetterOutput::default();
    let render = |request: &LetterRequest, shown: &str| {
        exchange.letters_pdf(
            project,
            request,
            job.logo.as_ref(),
            &job.frame,
            &job.look,
            shown,
        )
    };
    match (&job.request, &job.target) {
        (LetterRequest::Release { invoice }, target) => {
            let file = match target {
                LetterTarget::One(file) => file.clone(),
                LetterTarget::PerRecipient(files) => files
                    .first()
                    .map(|(_, file)| file.clone())
                    .ok_or("Kein Ziel für das Anschreiben.")?,
            };
            // Checked before anything is written.
            let entry = job
                .file_in_project
                .then(|| {
                    proava_domain::release_letter_entry(project, *invoice, &job.date, Vec::new())
                })
                .transpose()?;
            let (bytes, pages) = render(&job.request, &file_name(&file))?;
            exchange.write_file(&file, &bytes)?;
            output.pages = pages;
            output.written.push(file);
            output
                .filed
                .extend(entry.map(|entry| entry.with_pdf(bytes)));
        }
        (LetterRequest::Bidders { kind, .. }, LetterTarget::PerRecipient(files)) => {
            for (id, file) in files {
                let single = LetterRequest::Bidders {
                    kind: *kind,
                    recipients: vec![*id],
                };
                let (bytes, pages) = render(&single, &file_name(file))?;
                exchange.write_file(file, &bytes)?;
                output.pages += pages;
                output.written.push(file.clone());
                if job.file_in_project {
                    output.filed.extend(proava_domain::bidder_letter_entry(
                        project, *kind, *id, &job.date, bytes,
                    ));
                }
            }
        }
        (LetterRequest::Bidders { kind, recipients }, LetterTarget::One(file)) => {
            let shown = file_name(file);
            let (bytes, pages) = render(&job.request, &shown)?;
            exchange.write_file(file, &bytes)?;
            output.pages = pages;
            output.written.push(file.clone());
            if job.file_in_project {
                if let [only] = recipients.as_slice() {
                    output.filed.extend(proava_domain::bidder_letter_entry(
                        project, *kind, *only, &job.date, bytes,
                    ));
                } else {
                    // Each bidder's letter on its own, as in the collective PDF.
                    for id in recipients {
                        let single = LetterRequest::Bidders {
                            kind: *kind,
                            recipients: vec![*id],
                        };
                        let (bytes, _) = render(&single, &shown)?;
                        output.filed.extend(proava_domain::bidder_letter_entry(
                            project, *kind, *id, &job.date, bytes,
                        ));
                    }
                }
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use proava_domain::{Address, InvitedBidder, LetterKind, Lv, OzMask, ProjectName};

    use super::*;
    use crate::exchange::{BidImport, GaebExport};

    /// Renders „PDF“ bytes naming the recipients and records written files.
    #[derive(Default)]
    struct Fake {
        written: Mutex<Vec<(PathBuf, Vec<u8>)>>,
        rendered: Mutex<Vec<String>>,
    }

    impl ExchangeFiles for Fake {
        fn export_gaeb(&self, _: &Project, _: GaebExport, _: &Path) -> Result<Vec<String>, String> {
            unreachable!()
        }
        fn import_bid(&self, _: &Project, _: &Path) -> Result<BidImport, String> {
            unreachable!()
        }
        fn import_lv(
            &self,
            _: &Path,
            _: &ProjectName,
            _: &str,
        ) -> Result<(Project, Vec<String>), String> {
            unreachable!()
        }
        fn export_pdf(&self, _: &Project, _: crate::PdfDocument, _: &Path) -> Result<(), String> {
            unreachable!()
        }
        fn letters_pdf(
            &self,
            _: &Project,
            request: &LetterRequest,
            _: Option<&PdfLogo>,
            _: &PageFrame,
            _: &PrintLook,
            shown: &str,
        ) -> Result<(Vec<u8>, usize), String> {
            let text = match request {
                LetterRequest::Bidders { recipients, .. } => {
                    format!(
                        "{:?}|{shown}",
                        recipients.iter().map(|id| id.0).collect::<Vec<_>>()
                    )
                }
                LetterRequest::Release { .. } => format!("Freigabe|{shown}"),
            };
            self.rendered.lock().unwrap().push(text.clone());
            let pages = match request {
                LetterRequest::Bidders { recipients, .. } => recipients.len(),
                LetterRequest::Release { .. } => 1,
            };
            Ok((text.into_bytes(), pages))
        }
        fn write_file(&self, file: &Path, bytes: &[u8]) -> Result<(), String> {
            self.written
                .lock()
                .unwrap()
                .push((file.to_path_buf(), bytes.to_vec()));
            Ok(())
        }
    }

    fn project() -> (Project, Vec<InviteeId>) {
        let mut project = Project::new(
            ProjectName::parse("P").unwrap(),
            "2026",
            Lv::new("GA", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap()),
        );
        let ids = ["Müller GmbH", "Schmidt KG"]
            .iter()
            .map(|name| project.add_invitee(InvitedBidder::new(Address::named(*name))))
            .collect();
        (project, ids)
    }

    fn job(
        kind: LetterKind,
        recipients: Vec<InviteeId>,
        target: LetterTarget,
        file: bool,
    ) -> LetterJob {
        LetterJob {
            request: LetterRequest::Bidders { kind, recipients },
            target,
            file_in_project: file,
            logo: None,
            frame: PageFrame::default(),
            look: PrintLook::default(),
            date: "18.11.2026".into(),
        }
    }

    #[test]
    fn collective_pdf_files_one_letter_per_bidder() {
        let (project, ids) = project();
        let fake = Fake::default();
        let target = LetterTarget::One(PathBuf::from("/x/Alle.pdf"));
        let output = write_letters(
            &fake,
            &project,
            &job(LetterKind::Rejection, ids.clone(), target, true),
        )
        .unwrap();
        assert_eq!(output.pages, 2);
        assert_eq!(output.written, vec![PathBuf::from("/x/Alle.pdf")]);
        assert_eq!(
            fake.written.lock().unwrap().len(),
            1,
            "nur das Sammel-PDF wird geschrieben"
        );
        assert_eq!(output.filed.len(), 2);
        assert_eq!(output.filed[0].recipient, Some(ids[0]));
        assert_eq!(output.filed[1].recipient_name, "Schmidt KG");
        // The filed copy is the bidder's letter alone, footer as in the collective PDF.
        assert_eq!(
            output.filed[1].pdf.as_slice(),
            format!("[{}]|Alle.pdf", ids[1].0).as_bytes()
        );
        assert_eq!(output.filed[0].date, "18.11.2026");
    }

    #[test]
    fn one_pdf_per_bidder_is_filed_as_written() {
        let (project, ids) = project();
        let fake = Fake::default();
        let files = vec![
            (ids[0], PathBuf::from("/x/A.pdf")),
            (ids[1], PathBuf::from("/x/B.pdf")),
        ];
        let output = write_letters(
            &fake,
            &project,
            &job(
                LetterKind::Invitation,
                ids.clone(),
                LetterTarget::PerRecipient(files),
                true,
            ),
        )
        .unwrap();
        assert_eq!(
            fake.rendered.lock().unwrap().len(),
            2,
            "nichts doppelt erzeugt"
        );
        let written = fake.written.lock().unwrap();
        assert_eq!(output.filed[1].pdf.as_slice(), written[1].1.as_slice());
        assert_eq!(output.written.len(), 2);
    }

    #[test]
    fn without_filing_nothing_is_filed() {
        let (project, ids) = project();
        let fake = Fake::default();
        let target = LetterTarget::One(PathBuf::from("/x/Alle.pdf"));
        let output =
            write_letters(&fake, &project, &job(LetterKind::Award, ids, target, false)).unwrap();
        assert!(output.filed.is_empty());
        assert_eq!(
            fake.rendered.lock().unwrap().len(),
            1,
            "keine Einzel-PDFs ohne Ablage"
        );
    }

    #[test]
    fn release_letter_needs_contract_and_invoice() {
        let (project, _) = project();
        let fake = Fake::default();
        let job = LetterJob {
            request: LetterRequest::Release { invoice: None },
            ..job(
                LetterKind::Release,
                Vec::new(),
                LetterTarget::One(PathBuf::from("/x/F.pdf")),
                true,
            )
        };
        // The entry needs Auftrag and Rechnung – checked before writing.
        assert!(write_letters(&fake, &project, &job).is_err());
        assert!(fake.written.lock().unwrap().is_empty());
    }
}
