//! Schriftverkehr-Archiv in the project file: entries in `projekt.xml`,
//! PDFs in `schriftverkehr/` of the container (schema 2, additive).

use std::io::{Cursor, Read, Write};
use std::path::PathBuf;

use proava_application::ProjectRepository;
use proava_domain::{
    Address, CorrespondenceEntry, CorrespondenceKind, InvitedBidder, LetterKind, Lv, OzMask,
    Project, ProjectName,
};
use proava_infrastructure::{project_from_bytes, project_to_bytes, XmlProjectFile};

/// A folder only this test uses: a unique name, created exclusively (two
/// tests – also of other test runs at the same time – never share one); it is
/// removed with this value, nothing else.
struct TempDir(PathBuf, #[allow(dead_code)] tempfile::TempDir);

impl TempDir {
    fn new(label: &str) -> Self {
        let dir = tempfile::Builder::new()
            .prefix(&format!("proava-test-{label}-"))
            .tempdir()
            .unwrap();
        Self(dir.path().to_path_buf(), dir)
    }
}

/// A tiny but real-looking PDF (the archive does not look inside).
fn pdf(text: &str) -> Vec<u8> {
    format!("%PDF-1.7\n% {text}\n%%EOF\n").into_bytes()
}

fn project_with_archive() -> Project {
    let mut lv = Lv::new(
        "Gebäudeautomation & MSR",
        OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap(),
    );
    lv.set_number("480");
    let mut project = Project::new(
        ProjectName::parse("Archiv").unwrap(),
        "2026-11-18T09:00:00",
        lv,
    );
    let mueller = project.add_invitee(InvitedBidder::new(Address::named("Müller & Söhne <GmbH>")));
    let mut absage = proava_domain::bidder_letter_entry(
        &project,
        LetterKind::Rejection,
        mueller,
        "18.11.2026",
        pdf("Absage Müller"),
    )
    .unwrap();
    absage.remark = "per Mail \"vorab\"".into();
    project.add_correspondence(absage);
    let mut own = CorrespondenceEntry::new(
        CorrespondenceKind::Own,
        "19.11.2026",
        "Protokoll Bietergespräch",
        pdf("eigen"),
    );
    own.recipient_name = "Bauherr".into();
    project.add_correspondence(own);
    project
}

fn entry_names(bytes: &[u8]) -> Vec<String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|index| archive.by_index(index).unwrap().name().unwrap().to_string())
        .collect()
}

#[test]
fn archive_round_trip_with_pdfs() {
    let project = project_with_archive();
    let bytes = project_to_bytes(&project).unwrap();
    let names = entry_names(&bytes);
    assert!(
        names.contains(&"schriftverkehr/0001_Absage_Müller_Söhne_GmbH.pdf".to_string()),
        "{names:?}"
    );
    assert!(
        names.contains(&"schriftverkehr/0002_Schreiben_Bauherr.pdf".to_string()),
        "{names:?}"
    );

    let loaded = project_from_bytes(&bytes).unwrap();
    assert_eq!(
        loaded.correspondence(),
        project.correspondence(),
        "Liste und PDFs unverändert"
    );
    let first = &loaded.correspondence()[0];
    assert_eq!(
        first.kind,
        CorrespondenceKind::Letter(LetterKind::Rejection)
    );
    assert_eq!(first.recipient_name, "Müller & Söhne <GmbH>");
    assert_eq!(first.remark, "per Mail \"vorab\"");
    assert_eq!(first.lv_label(), "LV 480 – Gebäudeautomation & MSR");
    assert_eq!(first.pdf.as_slice(), pdf("Absage Müller").as_slice());
    // Ids continue after loading.
    let mut again = loaded.clone();
    let id = again.add_correspondence(CorrespondenceEntry::new(
        CorrespondenceKind::Own,
        "",
        "x",
        pdf("x"),
    ));
    assert_eq!(id.0, 3);
}

#[test]
fn files_without_archive_stay_as_they_were() {
    let mut project = project_with_archive();
    project.replace_correspondence(Vec::new());
    let bytes = project_to_bytes(&project).unwrap();
    assert!(!entry_names(&bytes)
        .iter()
        .any(|name| name.starts_with("schriftverkehr/")));
    let mut xml = String::new();
    zip::ZipArchive::new(Cursor::new(bytes.as_slice()))
        .unwrap()
        .by_name("projekt.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(
        !xml.contains("<correspondence"),
        "kein leeres Element in alten Projekten"
    );
    assert!(project_from_bytes(&bytes)
        .unwrap()
        .correspondence()
        .is_empty());
}

/// A file whose PDF went missing (damaged by hand, older tool) still opens;
/// the entry stays with its size, without the PDF.
#[test]
fn missing_pdf_keeps_the_entry() {
    let project = project_with_archive();
    let bytes = project_to_bytes(&project).unwrap();
    let mut source = zip::ZipArchive::new(Cursor::new(bytes.as_slice())).unwrap();
    let mut target = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..source.len() {
        let mut entry = source.by_index(index).unwrap();
        let name = entry.name().unwrap().to_string();
        if name.contains("Absage") {
            continue;
        }
        let mut content = Vec::new();
        entry.read_to_end(&mut content).unwrap();
        target
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        target.write_all(&content).unwrap();
    }
    let damaged = target.finish().unwrap().into_inner();
    let loaded = project_from_bytes(&damaged).unwrap();
    let first = &loaded.correspondence()[0];
    assert!(!first.has_pdf());
    assert_eq!(first.size, pdf("Absage Müller").len() as u64);
    assert!(loaded.correspondence()[1].has_pdf());
    // Saving again keeps the entry (still without PDF).
    let resaved = project_from_bytes(&project_to_bytes(&loaded).unwrap()).unwrap();
    assert_eq!(resaved.correspondence().len(), 2);
}

#[test]
fn saving_is_atomic_and_the_backup_holds_the_archive() {
    let folder = TempDir::new("archiv");
    let file = folder.0.join("Archiv.proava");
    let repository = XmlProjectFile::new();
    let project = project_with_archive();
    repository.save_project(&file, &project).unwrap();
    assert!(!folder.0.join("Archiv.proava.tmp").exists());

    let backup = repository
        .backup(&file, &folder.0.join("Sicherung"))
        .unwrap();
    let restored = repository.restore(&backup, &folder.0).unwrap();
    let loaded = repository.load_project(&restored).unwrap();
    assert_eq!(
        loaded.correspondence(),
        project.correspondence(),
        "Sicherung enthält das Archiv"
    );
}

/// The cleanup of the viewer copies asks whether a proAVA still runs.
#[cfg(target_os = "linux")]
#[test]
fn an_ended_process_is_recognised() {
    assert_eq!(
        XmlProjectFile::new().proava_running(u32::MAX - 1),
        Some(false)
    );
}
