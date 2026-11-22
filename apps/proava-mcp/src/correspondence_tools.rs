//! Schriftverkehr-Archiv: the letters filed in the project (PDFs in the
//! project file) – list and save one PDF as a file.

use proava_domain::{size_label, CorrespondenceId, CorrespondenceKind};

use crate::args::Args;
use crate::exchange_tools::target;
use crate::server::Server;

/// Table cell: no line breaks, no `|`.
fn cell(text: &str) -> String {
    let text = text.replace(['\n', '\r'], " ").replace('|', "/");
    if text.trim().is_empty() {
        "–".to_string()
    } else {
        text.trim().to_string()
    }
}

pub(crate) fn read_correspondence(server: &mut Server, args: &Args) -> Result<String, String> {
    let loaded = server.open(args)?;
    let kind = match args.opt_text("art")?.filter(|text| !text.trim().is_empty()) {
        Some(key) => Some(
            CorrespondenceKind::from_key(key)
                .ok_or("art: bitte aufforderung, zuschlag, absage, freigabe oder eigene.")?,
        ),
        None => None,
    };
    let recipient = args
        .opt_text("empfaenger")?
        .map(|text| text.trim().to_lowercase())
        .filter(|text| !text.is_empty());
    let project = &loaded.project;
    let all = project.correspondence();
    let shown: Vec<_> = all
        .iter()
        .filter(|entry| kind.is_none_or(|kind| entry.kind == kind))
        .filter(|entry| {
            recipient
                .as_deref()
                .is_none_or(|part| entry.recipient_name.to_lowercase().contains(part))
        })
        .collect();
    let mut answer = format!(
        "Schriftverkehr im Projekt: {} {}, {} (PDFs in der Projektdatei, Ordner schriftverkehr/)",
        all.len(),
        "Schreiben",
        size_label(project.correspondence_size())
    );
    if shown.len() != all.len() {
        answer.push_str(&format!("; nach Filter {}", shown.len()));
    }
    answer.push('.');
    if let Some(hint) = project.correspondence_hint() {
        answer.push_str(&format!("\n{hint}"));
    }
    if shown.is_empty() {
        answer.push_str(if all.is_empty() {
            "\nNoch nichts abgelegt – anschreiben_drucken legt jedes Schreiben ab (ablegen: true, Standard)."
        } else {
            "\nKein Schreiben passt zum Filter."
        });
        return Ok(answer);
    }
    answer.push_str("\n\n| Id | Datum | Art | Empfänger | Betreff | LV | Größe | Datei | Bemerkung |\n|---|---|---|---|---|---|---|---|---|");
    for entry in shown {
        let size = if entry.has_pdf() {
            size_label(entry.size)
        } else {
            "PDF fehlt".to_string()
        };
        let recipient = match entry.recipient {
            Some(id) => format!("{} (Bieter #{})", cell(&entry.recipient_name), id.0),
            None => cell(&entry.recipient_name),
        };
        answer.push_str(&format!(
            "\n| {} | {} | {} | {recipient} | {} | {} | {size} | {} | {} |",
            entry.id.0,
            cell(&entry.date),
            entry.kind.label(),
            cell(&entry.subject),
            cell(&entry.lv_label()),
            cell(&entry.file),
            cell(&entry.remark),
        ));
    }
    Ok(answer)
}

/// The PDF of one filed letter as a file (default: next to the project).
pub(crate) fn save_correspondence_pdf(server: &mut Server, args: &Args) -> Result<String, String> {
    let loaded = server.open(args)?;
    let id = CorrespondenceId(
        args.opt_count("id")?
            .ok_or("id: Nummer des Schreibens aus schriftverkehr_lesen.")?,
    );
    let entry = loaded.project.correspondence_entry(id).ok_or_else(|| {
        format!(
            "Ein abgelegtes Schreiben #{} gibt es nicht (schriftverkehr_lesen).",
            id.0
        )
    })?;
    if !entry.has_pdf() {
        return Err(format!("Zu #{} fehlt das PDF in der Projektdatei.", id.0));
    }
    let file = target(server, args, &loaded.path, entry.file.clone())?;
    server.exchange().write_file(&file, &entry.pdf)?;
    Ok(format!(
        "{} an {} ({}) gespeichert: {}",
        entry.kind.label(),
        cell(&entry.recipient_name),
        size_label(entry.size),
        file.display()
    ))
}
