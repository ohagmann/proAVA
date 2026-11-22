//! Kosten-LVs im MCP-Server: Kostenschätzung und Kostenberechnung als
//! Kurztext-LV (DIN 276). Hier die Sperre – Vergabe- und
//! Abrechnungswerkzeuge lehnen ein Kosten-LV ab, es geht nie an Bieter –,
//! die Art beim Anlegen und „Kostenberechnung aus Kostenschätzung“.

use proava_application::PrintKind;
use proava_domain::{
    calculation_from, cost_lv_template, kg400_groups, CostStage, Lv, LvPurpose, Project,
};

use crate::args::Args;
use crate::server::Server;

/// Tools of the tender (Bieter, Angebote, Auftrag, Nachträge, Abrechnung):
/// they refuse a Kosten-LV.
const TENDER_TOOLS: [&str; 22] = [
    "bieter_anlegen",
    "bieter_aendern",
    "submission_drucken",
    "anschreiben_drucken",
    "angebot_einlesen",
    "angebot_nachlass_setzen",
    "auftrag_erteilen",
    "preise_uebernehmen",
    "auftrags_lv_exportieren",
    "auftrag_festschreiben",
    "rechnung_anlegen",
    "rechnung_aendern",
    "rechnungsposition_setzen",
    "aufmass_setzen",
    "freigabe_berechnen",
    "nachtrag_anlegen",
    "nachtrag_aendern",
    "nachtrag_status",
    "nachtragsposition_setzen",
    "nachtragsangebot_einlesen",
    "nachtrag_gaeb_exportieren",
    "nachtrag_drucken",
];

/// Print kinds of the tender.
fn tender_print(kind: PrintKind) -> bool {
    matches!(
        kind,
        PrintKind::RequestForQuotation
            | PrintKind::Contract
            | PrintKind::PriceComparison
            | PrintKind::BidPrices
            | PrintKind::Submission
            | PrintKind::BillingCheck
            | PrintKind::Release
    )
}

/// Whether `name` with these arguments belongs to the tender.
fn is_tender_call(name: &str, args: &Args) -> bool {
    if TENDER_TOOLS.contains(&name) {
        return true;
    }
    let text = |key: &str| {
        args.opt_text(key)
            .ok()
            .flatten()
            .unwrap_or_default()
            .trim()
            .to_lowercase()
    };
    match name {
        "gaeb_exportieren" => text("art") == "x83",
        "pdf_drucken" => PrintKind::from_key(&text("dokument")).is_some_and(tender_print),
        _ => false,
    }
}

/// Refuses a tool of the tender for a Kosten-LV (before the tool runs).
pub(crate) fn refuse_for_cost_lv(server: &Server, name: &str, args: &Args) -> Result<(), String> {
    if !is_tender_call(name, args) || args.opt_text("projekt").ok().flatten().is_none() {
        return Ok(());
    }
    let Ok(loaded) = server.open(args) else {
        // The tool itself reports what is wrong with project or LV.
        return Ok(());
    };
    let lv = loaded.project.lv();
    match lv.purpose().cost_stage() {
        Some(stage) => Err(format!(
            "{} ist eine {} (Kosten-LV, Kostenermittlung nach DIN 276) – {name} gibt es nur für \
             Ausschreibungs-LVs: Bieter, Angebote, Preisspiegel, Auftrag, Nachträge und Abrechnung \
             gehören nie zu einem Kosten-LV. Mit lv ein Ausschreibungs-LV wählen (lvs_lesen zeigt die Art).",
            crate::lv_tools::label(lv),
            stage.label()
        )),
        None => Ok(()),
    }
}

/// `art` of lv_anlegen: Ausschreibung (default), Kostenschätzung, Kostenberechnung.
pub(crate) fn purpose_argument(args: &Args) -> Result<LvPurpose, String> {
    let Some(text) = args.opt_text("art")? else {
        return Ok(LvPurpose::Tender);
    };
    let key = text.trim().to_lowercase();
    if key.is_empty() || key == "ausschreibung" {
        return Ok(LvPurpose::Tender);
    }
    CostStage::from_key(&key).map(LvPurpose::Cost).ok_or_else(|| {
        format!("art: „{text}“ gibt es nicht – ausschreibung, kostenschaetzung oder kostenberechnung.")
    })
}

/// A new Kosten-LV for lv_anlegen: with `aus` a copy of that Kostenschätzung
/// as Kostenberechnung, else the template with titles for `kostengruppen`
/// (2nd level of KG 400; default all, [] = none) – unless a mask is given.
pub(crate) fn new_cost_lv(
    project: &Project,
    args: &Args,
    stage: CostStage,
    title: &str,
    step: u32,
) -> Result<Option<Lv>, String> {
    if let Some(source) = args.get("aus").filter(|value| !value.is_null()) {
        if stage != CostStage::Calculation {
            return Err(
                "aus geht nur mit art kostenberechnung (Kostenberechnung aus Kostenschätzung)."
                    .into(),
            );
        }
        let index = crate::lv_tools::select_lv(project, Some(source))?;
        let lv = project
            .lv_at(index)
            .ok_or("aus: dieses LV gibt es nicht.")?;
        if lv.purpose() != LvPurpose::Cost(CostStage::Estimate) {
            return Err(format!(
                "aus: {} ist keine Kostenschätzung (Kosten-LV).",
                crate::lv_tools::label(lv)
            ));
        }
        let mut calculation = calculation_from(lv);
        if !title.trim().is_empty() {
            calculation.set_title(title.trim());
        }
        return Ok(Some(calculation));
    }
    if args
        .opt_text("oz_maske")?
        .is_some_and(|mask| !mask.trim().is_empty())
    {
        return Ok(None);
    }
    let groups: Vec<String> = match args.get("kostengruppen").filter(|value| !value.is_null()) {
        None => kg400_groups()
            .into_iter()
            .map(|(code, _)| code.to_string())
            .collect(),
        Some(serde_json::Value::Array(values)) => values
            .iter()
            .map(|value| match value {
                serde_json::Value::String(text) => Ok(text.trim().to_string()),
                serde_json::Value::Number(number) => Ok(number.to_string()),
                _ => Err("kostengruppen: Liste von KG wie [\"420\", \"430\"].".to_string()),
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err("kostengruppen: Liste von KG wie [\"420\", \"430\"].".into()),
    };
    let known: Vec<&str> = kg400_groups().into_iter().map(|(code, _)| code).collect();
    if let Some(unknown) = groups.iter().find(|code| !known.contains(&code.as_str())) {
        return Err(format!(
            "kostengruppen: „{unknown}“ ist keine KG der 2. Ebene von KG 400 ({}).",
            known.join(", ")
        ));
    }
    let groups: Vec<&str> = groups.iter().map(String::as_str).collect();
    Ok(Some(cost_lv_template(stage, title, &groups, step)))
}

/// „Ausschreibung“, „Kostenschätzung (Kosten-LV)“ for lvs_lesen.
pub(crate) fn purpose_text(lv: &Lv) -> String {
    match lv.purpose().cost_stage() {
        None => "Ausschreibung".to_string(),
        Some(stage) => format!("{} (Kosten-LV)", stage.label()),
    }
}

/// kosten_lesen: the Kosten-LVs and the sums per Stufe and KG (2nd level) of
/// all Kostenermittlungen, Kosten-LVs and Datenpunktrechnungen together;
/// `None` without Kosten-LV.
pub(crate) fn overview(project: &Project) -> Option<String> {
    let indices = project.cost_lv_indices();
    if indices.is_empty() {
        return None;
    }
    let mut out = format!(
        "{} (Kostenermittlung als Kurztext-LV, bearbeiten mit den LV-Werkzeugen und lv):\n\n\
         | LV | Stufe | Positionen | Summe netto |\n|---|---|---|---|",
        proava_domain::wording::count(indices.len(), "Kosten-LV", "Kosten-LVs")
    );
    for index in indices {
        let Some(lv) = project.lv_at(index) else {
            continue;
        };
        out.push_str(&format!(
            "\n| {} | {} | {} | {} |",
            crate::lv_tools::label(lv),
            purpose_text(lv),
            lv.position_count(),
            lv.net_total().plain()
        ));
    }
    // GU-Fall: Datenpunkt-Abschnitte (KG 480) inside the Kosten-LVs.
    for index in project.cost_lv_indices() {
        let Some(lv) = project.lv_at(index) else {
            continue;
        };
        for row in lv.point_sections() {
            let row = &lv.rows()[row];
            let Some(points) = row.as_group().and_then(proava_domain::LvGroup::points) else {
                continue;
            };
            let result = points.result();
            out.push_str(&format!(
                "\nIm Kosten-LV {}: Datenpunkt-Abschnitt {} {} – {} DP, {} (zählt mit dem LV; kosten_lesen mit lv zeigt ihn)",
                crate::lv_tools::label(lv),
                row.ordinal(),
                row.short_text(),
                result.points,
                result.total.plain()
            ));
        }
    }
    for stage in CostStage::ALL {
        let sums = project.cost_sums(stage, 2);
        if sums.is_empty() {
            continue;
        }
        let parts: Vec<String> = sums
            .iter()
            .map(|(code, amount)| {
                let code = if code.is_empty() {
                    "ohne KG"
                } else {
                    code.as_str()
                };
                format!("{code}: {}", amount.plain())
            })
            .collect();
        out.push_str(&format!(
            "\n\n{} je KG (2. Ebene, alle Kostenermittlungen dieser Stufe zusammen): {}",
            stage.label(),
            parts.join(" · ")
        ));
    }
    Some(out)
}
