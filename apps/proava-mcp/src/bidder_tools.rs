//! Bieterverzeichnis and Niederschrift über die Angebotsöffnung.

use proava_domain::{
    optional_date, Address, BidId, InviteStatus, InvitedBidder, InviteeId, Project,
};

use crate::args::Args;
use crate::exchange_tools::{stem, target};
use crate::format::money;
use crate::lv_tools::{label, merge_address};
use crate::server::Server;

fn status_of(text: &str) -> Result<InviteStatus, String> {
    let wanted = text.trim().to_lowercase();
    InviteStatus::ALL
        .into_iter()
        .find(|status| status.label().to_lowercase() == wanted || status.key() == wanted)
        .or(match wanted.as_str() {
            "abgegeben" | "angebot" => Some(InviteStatus::Submitted),
            "absage" | "keins" => Some(InviteStatus::NoBid),
            _ => None,
        })
        .ok_or_else(|| {
            format!(
                "Status „{}“ – bitte angefragt, abgegeben, kein Angebot oder ausgeschlossen.",
                text.trim()
            )
        })
}

pub(crate) fn read_bidders(server: &mut Server, args: &Args) -> Result<String, String> {
    let loaded = server.open(args)?;
    let project = &loaded.project;
    let mut lines = vec![format!(
        "Bieterverzeichnis {} ({} Bieter) – Id | Bieter | Ort | Ansprechpartner | E-Mail | angefordert | Status | Angebot | Eingang | Nebenangebote | Bemerkung | Absagegrund:",
        label(project.lv()),
        project.invitees().len()
    )];
    for invitee in project.invitees() {
        lines.push(format!(
            "#{} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {}",
            invitee.id.0,
            invitee.name(),
            invitee.address.city,
            invitee.contact_person,
            invitee.address.email,
            invitee.requested,
            invitee.status.label(),
            invitee
                .bid
                .map(|bid| bid_text(project, bid))
                .unwrap_or_else(|| "–".into()),
            invitee.received,
            if invitee.side_offers { "ja" } else { "nein" },
            invitee.remark.replace('\n', " / "),
            invitee.rejection_reason.replace('\n', " / ")
        ));
    }
    let loose: Vec<String> = project
        .bids()
        .iter()
        .filter(|bid| project.invitee_of_bid(bid.id()).is_none())
        .map(|bid| {
            let hint = project
                .suggest_invitee(bid.name())
                .map(|id| format!(" → passt zu #{}", id.0))
                .unwrap_or_default();
            format!("  Angebot {} {}{hint}", bid.id().0, bid.name())
        })
        .collect();
    if !loose.is_empty() {
        lines.push(
            "Angebote ohne angefragten Bieter (mit bieter_aendern angebot verknüpfen):".into(),
        );
        lines.extend(loose);
    }
    Ok(lines.join("\n"))
}

fn bid_text(project: &Project, bid: BidId) -> String {
    project
        .bids()
        .iter()
        .find(|known| known.id() == bid)
        .map(|known| {
            format!(
                "Angebot {} ({})",
                bid.0,
                money(known.net_total(project.lv()))
            )
        })
        .unwrap_or_else(|| format!("Angebot {} (fehlt)", bid.0))
}

/// Fields shared by bieter_anlegen and bieter_aendern.
fn apply_fields(
    project: &mut Project,
    id: InviteeId,
    args: &Args,
) -> Result<Vec<&'static str>, String> {
    let mut changed = Vec::new();
    let link = args.opt_count("angebot")?;
    let unlink = args.get("angebot").is_some_and(|value| value.is_null());
    {
        let invitee = project
            .invitee_mut(id)
            .ok_or_else(|| format!("Bieter #{} gibt es im LV nicht.", id.0))?;
        let before = invitee.address.clone();
        merge_address(&mut invitee.address, args)?;
        if invitee.address != before {
            changed.push("Adresse");
        }
        if let Some(text) = args.opt_text("ansprechpartner")? {
            invitee.contact_person = text.trim().to_string();
            changed.push("Ansprechpartner");
        }
        if let Some(text) = args.opt_text("angefordert")? {
            optional_date(text).map_err(|error| format!("angefordert: {error}"))?;
            invitee.requested = text.trim().to_string();
            changed.push("angefordert");
        }
        if let Some(text) = args.opt_text("eingang")? {
            invitee.received = text.trim().to_string();
            changed.push("Eingang");
        }
        if let Some(side) = args.opt_flag("nebenangebote")? {
            invitee.side_offers = side;
            changed.push("Nebenangebote");
        }
        if let Some(text) = args.opt_text("bemerkung")? {
            invitee.remark = text.trim().to_string();
            changed.push("Bemerkung");
        }
        if let Some(text) = args.opt_text("absagegrund")? {
            invitee.rejection_reason = text.trim().to_string();
            changed.push("Absagegrund");
        }
    }
    if let Some(bid) = link {
        project.link_bid(BidId(bid), Some(id))?;
        changed.push("Angebot verknüpft");
    } else if unlink {
        if let Some(bid) = project
            .invitees()
            .iter()
            .find(|invitee| invitee.id == id)
            .and_then(|invitee| invitee.bid)
        {
            project.link_bid(bid, None)?;
        }
        changed.push("Angebot gelöst");
    }
    if let Some(text) = args.opt_text("status")? {
        let status = status_of(text)?;
        if let Some(invitee) = project.invitee_mut(id) {
            invitee.status = status;
        }
        changed.push("Status");
    }
    Ok(changed)
}

pub(crate) fn add_bidder(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let name = args.text("name1")?.trim().to_string();
    if name.is_empty() {
        return Err("Bitte den Namen des Bieters (name1) angeben.".to_string());
    }
    let id = loaded
        .project
        .add_invitee(InvitedBidder::new(Address::named(name.clone())));
    apply_fields(&mut loaded.project, id, args)?;
    let invitee = loaded
        .project
        .invitees()
        .iter()
        .find(|invitee| invitee.id == id)
        .cloned()
        .unwrap_or_default();
    let answer = format!(
        "Bieter #{} „{}“ ins Bieterverzeichnis von {} aufgenommen (Status {}).",
        id.0,
        name,
        label(loaded.project.lv()),
        invitee.status.label()
    );
    server.save_with_answer(&loaded, answer)
}

pub(crate) fn change_bidder(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let id = InviteeId(
        args.opt_count("bieter")?
            .ok_or("Bitte die Id des Bieters (bieter) aus bieter_lesen angeben.")?,
    );
    if args.opt_flag("entfernen")? == Some(true) {
        let removed = loaded
            .project
            .remove_invitee(id)
            .ok_or_else(|| format!("Bieter #{} gibt es im LV nicht.", id.0))?;
        let answer = format!(
            "Bieter #{} „{}“ aus dem Bieterverzeichnis entfernt.",
            id.0,
            removed.name()
        );
        return server.save_with_answer(&loaded, answer);
    }
    let changed = apply_fields(&mut loaded.project, id, args)?;
    if changed.is_empty() {
        return Err("Keine Angabe zum Ändern.".to_string());
    }
    server.save_with_answer(
        &loaded,
        format!("Bieter #{} geändert: {}.", id.0, changed.join(", ")),
    )
}

pub(crate) fn print_submission(server: &mut Server, args: &Args) -> Result<String, String> {
    let loaded = server.open(args)?;
    if loaded.project.invitees().is_empty() && loaded.project.bids().is_empty() {
        return Err(
            "Für die Niederschrift fehlen Bieter (bieter_anlegen) oder Angebote.".to_string(),
        );
    }
    let file = target(
        server,
        args,
        &loaded.path,
        format!("{}_Niederschrift_Submission.pdf", stem(&loaded.path)),
    )?;
    let logo = server.pdf_logo(args)?;
    server.exchange().export_submission_pdf(
        &loaded.project,
        logo.as_ref(),
        &server.print.page_frame,
        &server.print.look,
        &file,
    )?;
    Ok(format!(
        "Niederschrift über die Angebotsöffnung ({}) geschrieben: {}",
        label(loaded.project.lv()),
        file.display()
    ))
}

/// Schriftverkehr: letters to invited bidders as one PDF or one per bidder,
/// or the letter to the client about an invoice; with `ablegen` (default)
/// each recipient's letter is filed in the project (Schriftverkehr-Archiv).
pub(crate) fn print_letters(server: &mut Server, args: &Args) -> Result<String, String> {
    use proava_application::{write_letters, LetterJob, LetterRequest, LetterTarget};
    let mut loaded = server.open(args)?;
    let kind = proava_domain::LetterKind::from_key(args.text("art")?)
        .ok_or("art: bitte aufforderung, zuschlag, absage oder freigabe.")?;
    let file_in_project = args.opt_flag("ablegen")?.unwrap_or(true);
    let today = chrono::Local::now().format("%d.%m.%Y").to_string();
    let (request, letter_target, recipients) = if kind.to_client() {
        // Abrechnung: one letter to the client about one invoice.
        let invoice = args.opt_count("rechnung")?.map(proava_domain::InvoiceId);
        if let Some(id) = invoice {
            if loaded.project.billing().invoice(id).is_none() {
                return Err(format!(
                    "Eine Rechnung #{} gibt es in diesem LV nicht.",
                    id.0
                ));
            }
        }
        let number = invoice
            .or_else(|| {
                loaded
                    .project
                    .billing()
                    .invoices()
                    .last()
                    .map(|invoice| invoice.id)
            })
            .and_then(|id| loaded.project.billing().invoice(id))
            .map(|invoice| file_part(&invoice.number))
            .filter(|number| !number.is_empty() && number != "Bieter")
            .map(|number| format!("_{number}"))
            .unwrap_or_default();
        let name = format!(
            "{}_{}{number}_{}.pdf",
            stem(&loaded.path),
            kind.file_label(),
            chrono::Local::now().format("%Y-%m-%d")
        );
        let file = target(server, args, &loaded.path, name)?;
        (
            LetterRequest::Release { invoice },
            LetterTarget::One(file),
            Vec::new(),
        )
    } else {
        let recipients: Vec<InviteeId> = match args.opt_count("bieter")? {
            Some(id) => {
                let id = InviteeId(id);
                if !loaded
                    .project
                    .invitees()
                    .iter()
                    .any(|invitee| invitee.id == id)
                {
                    return Err(format!(
                        "Bieter #{} gibt es im Bieterverzeichnis nicht.",
                        id.0
                    ));
                }
                vec![id]
            }
            None if args.opt_flag("alle")? == Some(true) => loaded
                .project
                .invitees()
                .iter()
                .map(|invitee| invitee.id)
                .collect(),
            None => proava_domain::default_recipients(&loaded.project, kind),
        };
        if recipients.is_empty() {
            return Err(format!(
                "Kein Empfänger für „{}“ (Bieterverzeichnis leer, kein beauftragtes Angebot oder keine übrigen Angebote).",
                kind.label()
            ));
        }
        // Like proAVA: <Projekt>_<Art>_<JJJJ-MM-TT>[_<Bieter>].pdf
        let stem_name = format!(
            "{}_{}_{}",
            stem(&loaded.path),
            kind.file_label(),
            chrono::Local::now().format("%Y-%m-%d")
        );
        let letter_target = if args.opt_flag("einzeln")? == Some(true) {
            let folder = match args
                .opt_text("ziel")?
                .filter(|text| !text.trim().is_empty())
            {
                Some(path) => std::path::PathBuf::from(path.trim()),
                None => loaded
                    .path
                    .parent()
                    .map(std::path::Path::to_path_buf)
                    .unwrap_or_default(),
            };
            let folder = server.allowed(&folder)?;
            if !folder.is_dir() {
                return Err(format!("Zielordner nicht gefunden: {}", folder.display()));
            }
            let overwrite = args.flag("ueberschreiben")?;
            let mut files = Vec::new();
            for id in &recipients {
                let name = loaded
                    .project
                    .invitees()
                    .iter()
                    .find(|invitee| invitee.id == *id)
                    .map(|invitee| file_part(invitee.name()))
                    .unwrap_or_else(|| "Bieter".into());
                let file = folder.join(format!("{stem_name}_{name}.pdf"));
                if file.exists() && !overwrite {
                    return Err(format!(
                        "{} gibt es schon – mit ueberschreiben: true ersetzen.",
                        file.display()
                    ));
                }
                files.push((*id, file));
            }
            LetterTarget::PerRecipient(files)
        } else {
            LetterTarget::One(target(
                server,
                args,
                &loaded.path,
                format!("{stem_name}.pdf"),
            )?)
        };
        (
            LetterRequest::Bidders {
                kind,
                recipients: recipients.clone(),
            },
            letter_target,
            recipients,
        )
    };
    let job = LetterJob {
        request,
        target: letter_target,
        file_in_project,
        logo: server.pdf_logo(args)?,
        frame: server.print.page_frame.clone(),
        look: server.print.look.clone(),
        date: today.clone(),
    };
    let output = write_letters(server.exchange(), &loaded.project, &job)?;
    let files = output
        .written
        .iter()
        .map(|file| file.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let mut answer = if kind.to_client() {
        format!(
            "{} geschrieben ({} Seiten): {files}",
            kind.label(),
            output.pages
        )
    } else {
        format!(
            "{} an {} Bieter geschrieben: {files}",
            kind.label(),
            recipients.len()
        )
    };
    let mut changed = false;
    if kind == proava_domain::LetterKind::Invitation {
        for id in &recipients {
            if let Some(invitee) = loaded.project.invitee_mut(*id) {
                invitee.requested = today.clone();
            }
        }
        answer.push_str(&format!(
            "\nVersanddatum {today} im Bieterverzeichnis gesetzt."
        ));
        changed = true;
    }
    if !output.filed.is_empty() {
        let count = output.filed.len();
        for entry in output.filed {
            loaded.project.add_correspondence(entry);
        }
        answer.push_str(&format!(
            "\n{count} {} im Projekt abgelegt (schriftverkehr_lesen).",
            if count == 1 {
                "Schreiben"
            } else {
                "Schreiben je Empfänger"
            }
        ));
        if let Some(hint) = loaded.project.correspondence_hint() {
            answer.push_str(&format!("\n{hint}"));
        }
        changed = true;
    }
    if changed {
        return server.save_with_answer(&loaded, answer);
    }
    Ok(answer)
}

/// File name part for a bidder.
fn file_part(name: &str) -> String {
    let cleaned: String = name
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
    let cleaned = cleaned.trim_matches('_').to_string();
    if cleaned.is_empty() {
        "Bieter".to_string()
    } else {
        cleaned.chars().take(40).collect()
    }
}
