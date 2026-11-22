//! GAEB and PDF through the [`proava_application::ExchangeFiles`] port.

use std::path::{Path, PathBuf};

use proava_application::{ContractExport, GaebExport, PrintJob, PrintKind};
use proava_domain::{normalize_ordinal, BidId, ProjectName};

use crate::args::Args;
use crate::format::lv_totals;
use crate::read_tools::numbering_note;
use crate::server::{Server, PROJECT_EXTENSION};

/// Totals of the LV after an import, one per line.
fn totals_lines(lv: &proava_domain::Lv) -> String {
    let net = lv.net_total();
    let vat = lv.vat(net);
    let money = |value| proava_domain::price_check::amount(value, lv.currency());
    format!(
        "LV netto: {}\nMwSt {} %: {}\nLV brutto: {}",
        money(net),
        proava_domain::format_decimal_de(lv.vat_percent(), 0),
        money(vat),
        money(net + vat)
    )
}

fn warnings_text(warnings: &[String]) -> String {
    if warnings.is_empty() {
        return String::new();
    }
    let mut text = format!("\nHinweise ({}):", warnings.len());
    for warning in warnings.iter().take(50) {
        text.push_str(&format!("\n- {warning}"));
    }
    if warnings.len() > 50 {
        text.push_str(&format!("\n… (+{})", warnings.len() - 50));
    }
    text
}

/// Target file: `ziel` or next to the project; refuses to overwrite unless asked.
pub(crate) fn target(
    server: &Server,
    args: &Args,
    project: &Path,
    default_name: String,
) -> Result<PathBuf, String> {
    let path = match args
        .opt_text("ziel")?
        .filter(|text| !text.trim().is_empty())
    {
        Some(path) => PathBuf::from(path.trim()),
        None => project.with_file_name(default_name),
    };
    let path = server.allowed(&path)?;
    if !path.parent().is_some_and(Path::is_dir) {
        return Err(format!("Zielordner nicht gefunden: {}", path.display()));
    }
    if path.exists() && !args.flag("ueberschreiben")? {
        return Err(format!(
            "{} gibt es schon – mit ueberschreiben: true ersetzen oder anderes ziel wählen.",
            path.display()
        ));
    }
    Ok(path)
}

pub(crate) fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "Projekt".to_string())
}

pub(crate) fn export_gaeb(server: &mut Server, args: &Args) -> Result<String, String> {
    let loaded = server.open(args)?;
    let kind = match args.text("art")?.trim().to_uppercase().as_str() {
        "X83" | "83" => GaebExport::RequestForQuotation,
        "X82" | "82" => GaebExport::PricedLv,
        "X81" | "81" => GaebExport::Description,
        other => return Err(format!("GAEB-Art „{other}“ – bitte X83, X82 oder X81.")),
    };
    let datan = match args
        .opt_text("format")?
        .map(|text| text.trim().to_lowercase())
    {
        None => false,
        Some(format) if format.is_empty() || format == "xml" => false,
        Some(format) if matches!(format.as_str(), "gaeb90" | "datan" | "d8x") => true,
        Some(other) => return Err(format!("Format „{other}“ – bitte xml oder gaeb90.")),
    };
    let number = &kind.extension()[1..];
    let extension = format!("{}{number}", if datan { "D" } else { "X" });
    let file = target(
        server,
        args,
        &loaded.path,
        format!("{}.{extension}", stem(&loaded.path)),
    )?;
    let actual = file
        .extension()
        .map(|extension| extension.to_string_lossy().to_uppercase())
        .unwrap_or_default();
    if actual != format!("X{number}") && actual != format!("D{number}") {
        return Err(format!(
            "Das Ziel {} passt nicht zu {} – die Endung muss .X{number} (GAEB DA XML) oder .D{number} (GAEB 90) sein.",
            file.display(),
            kind.extension()
        ));
    }
    let options = proava_application::GaebExportOptions {
        images: args.opt_flag("bilder")?.unwrap_or(true),
    };
    let warnings = server
        .exchange()
        .export_gaeb_with(&loaded.project, kind, &file, options)?;
    Ok(format!(
        "{} geschrieben: {}{}",
        kind.label(),
        file.display(),
        warnings_text(&warnings)
    ))
}

/// csv_exportieren: the LV as CSV in the proAVA CSV format.
pub(crate) fn export_csv(server: &mut Server, args: &Args) -> Result<String, String> {
    use proava_application::CsvEncoding;

    let loaded = server.open(args)?;
    let encoding = match args
        .opt_text("zeichensatz")?
        .map(|text| text.trim().to_lowercase().replace(['-', '_', ' '], ""))
        .as_deref()
    {
        None | Some("") | Some("utf8") => CsvEncoding::Utf8Bom,
        Some("windows1252" | "cp1252" | "ansi" | "latin1") => CsvEncoding::Windows1252,
        Some(other) => {
            return Err(format!(
                "Zeichensatz „{other}“ – bitte utf-8 oder windows-1252."
            ))
        }
    };
    let lv = loaded.project.lv();
    let default_name = match lv.number().trim() {
        "" => format!("{}.csv", stem(&loaded.path)),
        number => format!("{}_LV_{number}.csv", stem(&loaded.path)),
    };
    let file = target(server, args, &loaded.path, default_name)?;
    let notes = server
        .exchange()
        .export_csv(&loaded.project, &file, encoding)?;
    Ok(format!(
        "CSV geschrieben ({}): {}{}",
        match encoding {
            CsvEncoding::Utf8Bom => "UTF-8 mit BOM",
            CsvEncoding::Windows1252 => "Windows-1252",
        },
        file.display(),
        warnings_text(&notes)
    ))
}

pub(crate) fn import_bid(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let file = server.allowed(Path::new(args.text("datei")?.trim()))?;
    if !file.is_file() {
        return Err(format!("Datei nicht gefunden: {}", file.display()));
    }
    let adopt = args.opt_flag("ins_bieterverzeichnis")?.unwrap_or(true);
    let by_structure = match args
        .opt_text("zuordnung")?
        .map(|text| text.trim().to_lowercase())
    {
        None => false,
        Some(mode) if mode == "oz" => false,
        Some(mode) if mode == "text" => true,
        Some(other) => return Err(format!("zuordnung: „{other}“ – bitte oz oder text.")),
    };
    let mut manual = Vec::new();
    if let Some(value) = args.get("zuordnungen") {
        let map = value
            .as_object()
            .ok_or("zuordnungen: bitte {\"Bieter-OZ\": \"LV-OZ\"}.")?;
        for (file_ordinal, lv_ordinal) in map {
            let lv_ordinal = lv_ordinal.as_str().ok_or_else(|| {
                format!("zuordnungen: zu „{file_ordinal}“ bitte die LV-OZ als Text.")
            })?;
            manual.push((file_ordinal.clone(), lv_ordinal.to_string()));
        }
    }
    let options = proava_application::BidMatchOptions {
        by_structure,
        manual,
    };
    let import = server
        .exchange()
        .import_bid_with(&loaded.project, &file, &options)?;
    let id = loaded.project.add_bid(import.bid);
    let directory = if adopt {
        match loaded.project.adopt_bidder(id) {
            Some((invitee, true)) => {
                format!("\nBieterverzeichnis: aufgenommen als Bieter #{}", invitee.0)
            }
            Some((invitee, false)) => {
                format!("\nBieterverzeichnis: Bieter #{} zugeordnet", invitee.0)
            }
            None => String::new(),
        }
    } else {
        String::new()
    };
    let bid = loaded
        .project
        .bids()
        .iter()
        .find(|bid| bid.id() == id)
        .ok_or("Angebot nicht übernommen.")?;
    let protocol =
        proava_application::protocol::bid_protocol(&loaded.project, bid, &import.warnings);
    let answer = format!(
        "Angebot eingelesen (#{})\n{}{directory}",
        bid.id().0,
        protocol.lines().join("\n")
    );
    server.save_with_answer(&loaded, answer)
}

pub(crate) fn import_lv(server: &mut Server, args: &Args) -> Result<String, String> {
    let file = server.allowed(Path::new(args.text("datei")?.trim()))?;
    if !file.is_file() {
        return Err(format!("Datei nicht gefunden: {}", file.display()));
    }
    let folder = server.allowed(Path::new(args.text("ordner")?.trim()))?;
    if !folder.is_dir() {
        return Err(format!("Ordner nicht gefunden: {}", folder.display()));
    }
    let name = ProjectName::parse(args.text("name")?).map_err(|error| error.to_string())?;
    let project_folder = folder.join(name.as_str());
    let path = project_folder.join(format!("{}.{PROJECT_EXTENSION}", name.as_str()));
    if path.exists() {
        return Err(format!(
            "Dort gibt es schon ein Projekt: {}",
            path.display()
        ));
    }
    let created = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let (project, warnings) = server.exchange().import_lv(&file, &name, &created)?;
    std::fs::create_dir_all(&project_folder)
        .map_err(|error| format!("Ordner {}: {error}", project_folder.display()))?;
    server.save_new(&path, &project)?;
    let lv = project.lv();
    Ok(format!(
        "Projekt aus GAEB angelegt: {}\nLV „{}“ · Maske {} · {} Titel, {} Positionen, {} Hinweise · Nummerierung: {}\n{}{}",
        path.display(),
        lv.title(),
        lv.oz_mask().pattern(),
        lv.group_count(),
        lv.position_count(),
        lv.note_count(),
        numbering_note(lv),
        lv_totals(lv),
        warnings_text(&warnings)
    ))
}

/// „GAEB ▸ Importieren …“ for KIs: a GAEB LV (also a Zulieferung with
/// Vortexten, also inside a zip) as a new LV of a project or as a new
/// project; `vorschau` only reads. The answer is the import protocol.
pub(crate) fn import_gaeb(server: &mut Server, args: &Args) -> Result<String, String> {
    import_lv_file(server, args, false)
}

/// csv_importieren: like gaeb_importieren for a CSV table in the proAVA CSV format,
/// with the Währung of the new LV.
pub(crate) fn import_csv(server: &mut Server, args: &Args) -> Result<String, String> {
    import_lv_file(server, args, true)
}

/// Währung from the argument: code or name (Euro, Dollar, Franken …).
fn currency_code(text: &str) -> Result<&'static str, String> {
    match text.trim().to_lowercase().as_str() {
        "eur" | "euro" | "€" => Ok("EUR"),
        "usd" | "dollar" | "us-dollar" | "$" => Ok("USD"),
        "chf" | "franken" | "schweizer franken" | "sfr" => Ok("CHF"),
        other => Err(format!("Währung „{other}“ – bitte EUR, USD oder CHF.")),
    }
}

fn import_lv_file(server: &mut Server, args: &Args, csv: bool) -> Result<String, String> {
    use proava_application::gaeb_import::{self, ImportPreview, ImportTarget};

    let file = server.allowed(Path::new(args.text("datei")?.trim()))?;
    if !file.is_file() {
        return Err(format!("Datei nicht gefunden: {}", file.display()));
    }
    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let mut import = gaeb_import::read(server.exchange(), &file, &now)?;
    if csv && !import.is_csv() {
        return Err(format!(
            "{} ist keine CSV-Tabelle (Endung .csv, .tsv oder .txt) – GAEB-Dateien liest gaeb_importieren.",
            file.display()
        ));
    }
    if let Some(currency) = args
        .opt_text("waehrung")?
        .filter(|text| !text.trim().is_empty())
    {
        if !import.is_csv() {
            return Err(
                "waehrung gilt nur für CSV-Tabellen – eine GAEB-Datei nennt ihre Währung selbst."
                    .to_string(),
            );
        }
        import.set_currency(currency_code(currency)?);
    }
    let preview = ImportPreview::of(&import);
    let prices = args.opt_flag("preise")?.unwrap_or(preview.has_prices());
    let bid_note = if preview.is_bid() {
        "
Hinweis: Die Datei ist ein Angebot – als Angebot einlesen mit angebot_einlesen."
    } else {
        ""
    };
    if args.opt_flag("vorschau")?.unwrap_or(false) {
        // One fact per line, like the preview in proAVA.
        let mut protocol = preview.protocol();
        if preview.has_prices() {
            protocol.push(proava_application::ProtocolEntry::text(
                "Preise",
                format!(
                    "{} mit Einheitspreis (preise: false lässt sie weg)",
                    proava_domain::wording::count(
                        preview.priced_positions(),
                        "Position",
                        "Positionen"
                    )
                ),
            ));
        }
        protocol.push(proava_application::ProtocolEntry::text(
            "Gespeichert",
            "nichts (vorschau)",
        ));
        return Ok(format!("{}{bid_note}", protocol.text().trim_end()));
    }
    let ziel = args.opt_text("ziel")?.unwrap_or("lv").trim().to_lowercase();
    match ziel.as_str() {
        "lv" => {
            let mut loaded = server.open(args)?;
            let number = match args.opt_text("nummer")? {
                Some(number) => number.trim().to_string(),
                None => preview.suggested_number(&loaded.project),
            };
            let title = match args.opt_text("bezeichnung")? {
                Some(title) => title.trim().to_string(),
                None => preview.suggested_title(),
            };
            let (index, protocol) =
                gaeb_import::add_as_lv(&mut loaded.project, &import, &number, &title, prices)?;
            let lv = loaded
                .project
                .lv_at(index)
                .ok_or("LV fehlt nach dem Import.")?;
            let answer = format!(
                "{}\n{}\nWeitere Werkzeuge mit lv: „{}“{bid_note}",
                protocol.text().trim_end(),
                totals_lines(lv),
                if lv.number().is_empty() {
                    (index + 1).to_string()
                } else {
                    lv.number().to_string()
                }
            );
            server.save_with_answer(&loaded, answer)
        }
        "projekt" => {
            let folder = server.allowed(Path::new(args.text("ordner")?.trim()))?;
            if !folder.is_dir() {
                return Err(format!("Ordner nicht gefunden: {}", folder.display()));
            }
            let stem = file
                .file_stem()
                .map(|stem| stem.to_string_lossy().trim().to_string())
                .unwrap_or_default();
            let name = args
                .opt_text("name")?
                .map(str::trim)
                .unwrap_or(&stem)
                .to_string();
            let project = gaeb_import::as_project(&import, &name, prices)?;
            let project_folder = folder.join(project.name().as_str());
            let path =
                project_folder.join(format!("{}.{PROJECT_EXTENSION}", project.name().as_str()));
            if path.exists() {
                return Err(format!(
                    "Dort gibt es schon ein Projekt: {}",
                    path.display()
                ));
            }
            std::fs::create_dir_all(&project_folder)
                .map_err(|error| format!("Ordner {}: {error}", project_folder.display()))?;
            server.save_new(&path, &project)?;
            let protocol =
                gaeb_import::protocol(&import, &ImportTarget::Project { file: path }, prices);
            Ok(format!(
                "{}\n{}{bid_note}",
                protocol.text().trim_end(),
                totals_lines(project.lv())
            ))
        }
        other => Err(format!(
            "ziel „{other}“ – bitte „lv“ (in projekt) oder „projekt“ (neu)."
        )),
    }
}

pub(crate) fn print_pdf(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let key = args
        .text("dokument")?
        .trim()
        .to_lowercase()
        .replace(' ', "_");
    let kind = PrintKind::from_key(&key).ok_or_else(|| {
        format!(
            "Dokument „{key}“ – bitte {}.",
            PrintKind::ALL.map(PrintKind::key).join(", ")
        )
    })?;
    if matches!(kind, PrintKind::PriceComparison | PrintKind::BidPrices)
        && loaded.project.bids().is_empty()
    {
        return Err("Dafür fehlen Angebote (angebot_einlesen).".to_string());
    }
    let mut job = PrintJob::new(kind);
    job.bid = args.opt_count("angebot")?.map(BidId);
    if kind.needs_invoice() {
        job.invoice = args.opt_count("rechnung")?.map(proava_domain::InvoiceId);
        if let Some(id) = job.invoice {
            if loaded.project.billing().invoice(id).is_none() {
                return Err(format!(
                    "Eine Rechnung #{} gibt es in diesem LV nicht.",
                    id.0
                ));
            }
        }
    }
    if kind.needs_cost_estimate() {
        let wanted = match args.get("kostenermittlung") {
            Some(serde_json::Value::Number(number)) => number.to_string(),
            Some(serde_json::Value::String(text)) => text.trim().to_string(),
            _ => String::new(),
        };
        let estimate = loaded.project.find_cost_estimate(&wanted).ok_or_else(|| {
            if loaded.project.cost_estimates().is_empty() {
                "Im Projekt gibt es keine Kostenermittlung (kosten_anlegen).".to_string()
            } else {
                format!("Eine Kostenermittlung „{wanted}“ gibt es nicht (kosten_lesen).")
            }
        })?;
        job.cost_estimate = Some(estimate.id);
    }
    if kind == PrintKind::CostTracking {
        job.tracking = crate::tracking_tools::options(&loaded.project, args)?;
    }
    if kind == PrintKind::BidPrices && job.bid.is_none() {
        return Err("Bei bieterpreise fehlt „angebot“ (Id aus angebote_lesen).".to_string());
    }
    let settings = &mut job.settings;
    for (key, field) in [
        ("langtexte", &mut settings.long_texts),
        ("bilder", &mut settings.images),
        ("hinweise", &mut settings.notes),
        ("ausfuehrungsbeschreibungen", &mut settings.performance),
        ("zusammenstellung", &mut settings.summary),
        ("titel_neue_seite", &mut settings.title_page_break),
        ("quer", &mut settings.landscape),
        ("inhaltsverzeichnis", &mut settings.contents),
        ("uebertrag", &mut settings.carry),
        ("lesezeichen_positionen", &mut settings.bookmark_positions),
        ("preise", &mut settings.prices),
        ("mengenermittlung", &mut settings.takeoff),
        ("kostengruppe", &mut settings.cost_groups),
        ("leistungsbereich", &mut settings.work_categories),
        ("diagramm", &mut settings.chart),
        ("anlagenliste", &mut settings.plants),
        ("kostenkontrolle", &mut settings.cost_control),
    ] {
        if let Some(value) = args.opt_flag(key)? {
            *field = value;
        }
    }
    if let Some(on) = args.opt_flag("blattrahmen")? {
        settings.border = Some(on);
    }
    if let Some(on) = args.opt_flag("gitter")? {
        settings.grid = Some(on);
    }
    if let Some(text) = args.opt_text("stil")? {
        settings.style = Some(
            proava_application::PrintStyle::from_key(text)
                .ok_or_else(|| format!("stil: „{}“ – bitte klassisch oder proava.", text.trim()))?,
        );
    }
    // deckblatt: true/false or the form („standard“, „briefkopf“, „kein“).
    match args.opt_text("deckblatt").ok().flatten() {
        Some(text) => match text.trim().to_lowercase().as_str() {
            "kein" | "ohne" | "nein" | "false" => settings.cover = false,
            "ja" | "true" => settings.cover = true,
            other => {
                settings.cover_form = proava_application::CoverForm::from_key(other)
                    .ok_or_else(|| format!("deckblatt: „{other}“ – bitte standard, briefkopf, kein oder true/false."))?;
                settings.cover = true;
            }
        },
        None => {
            if let Some(on) = args.opt_flag("deckblatt")? {
                settings.cover = on;
            }
        }
    }
    if let Some(size) = args.opt_count("schrift_pt")? {
        if !(9..=11).contains(&size) {
            return Err("schrift_pt: bitte 9, 10 oder 11.".to_string());
        }
        settings.font_size_pt = size as u8;
    }
    if let Some(text) = args.opt_text("kopftext")? {
        settings.header_text = text.to_string();
    }
    if let Some(text) = args.opt_text("fusstext")? {
        settings.footer_text = text.to_string();
    }
    if let Some(list) = args
        .opt_text("titel")?
        .filter(|text| !text.trim().is_empty())
    {
        let lv = loaded.project.lv();
        for wanted in list
            .split([',', ';'])
            .map(str::trim)
            .filter(|oz| !oz.is_empty())
        {
            let key = normalize_ordinal(wanted);
            let row = lv
                .rows()
                .iter()
                .find(|row| row.is_group() && normalize_ordinal(row.ordinal()) == key)
                .ok_or_else(|| format!("Einen Titel {wanted} gibt es nicht."))?;
            job.titles.push(row.id());
        }
    }
    let file = target(
        server,
        args,
        &loaded.path,
        format!(
            "{}_{}.pdf",
            stem(&loaded.path),
            job.cost_estimate
                .and_then(|id| loaded.project.cost_estimate(id))
                .map(|estimate| estimate.stage.label().to_string())
                .unwrap_or_else(|| kind.file_label().to_string())
        ),
    )?;
    job.logo = server.pdf_logo(args)?;
    job.frame = server.print.page_frame.clone();
    job.cover_blocks = server.print.cover_blocks;
    job.look = server.print.look.clone();
    job.author = server.print.author.clone();
    job.file_name = file
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    // Kopf-/Fußzeile nur für diesen Ausdruck (die Datei bleibt unverändert).
    let mut frame = loaded
        .project
        .info()
        .page_frame
        .clone()
        .unwrap_or_else(|| job.frame.clone());
    if crate::frame_args::apply(&mut frame, args)? {
        loaded.project.info_mut().page_frame = Some(frame);
    }
    let outcome = server
        .exchange()
        .export_print(&loaded.project, &job, &file)
        .map_err(|error| {
            if proava_application::is_memory_refusal(&error) {
                format!("{error} Per MCP: nur diese Titel drucken mit `titel` (OZ mit Komma).")
            } else {
                error
            }
        })?;
    Ok(format!(
        "{} geschrieben ({} Seiten): {}{}",
        kind.label(),
        outcome.pages,
        file.display(),
        warnings_text(&outcome.warnings)
    ))
}

pub(crate) fn export_contract(server: &mut Server, args: &Args) -> Result<String, String> {
    let loaded = server.open(args)?;
    let id =
        match args.opt_count("angebot")? {
            Some(id) => BidId(id),
            None => loaded.project.awarded_bid().map(|bid| bid.id()).ok_or(
                "Kein Angebot ist beauftragt – erst auftrag_erteilen oder angebot angeben.",
            )?,
        };
    let kind = match args
        .opt_text("format")?
        .map(|text| text.trim().to_lowercase())
        .as_deref()
    {
        None | Some("") | Some("x86") | Some("xml") => ContractExport::Xml,
        Some("d86") | Some("gaeb90") => ContractExport::Datan,
        Some("pdf") => ContractExport::Pdf,
        Some(other) => return Err(format!("Format „{other}“ – bitte x86, d86 oder pdf.")),
    };
    let file = target(
        server,
        args,
        &loaded.path,
        format!("{}_Auftrags-LV.{}", stem(&loaded.path), kind.extension()),
    )?;
    let warnings = server
        .exchange()
        .export_contract(&loaded.project, id, kind, &file)?;
    Ok(format!(
        "{} geschrieben: {}{}",
        kind.label(),
        file.display(),
        warnings_text(&warnings)
    ))
}
