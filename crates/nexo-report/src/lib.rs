//! Human-readable reports over an already-rendered, sealed NEXO evaluation.
//!
//! Design adapted from Anna Tchijova's `zaynor/src/zaynor/report.py`
//! (Apache-2.0) — confirmed by reading that file in full — but built
//! against NEXO's own sealed shape rather than adapted line-for-line from
//! it: ZAYNOR's `ZaynorAuthoritativeResult` carries per-finding MITRE
//! technique IDs, an agent-pipeline status table, and a verdict scale
//! (`MALICE`/`SUSPICION`/...) that NEXO's `ActionEvaluation` simply does
//! not have. What is reused is the *shape of the idea*: a report is a
//! read-only projection over a value that was already sealed elsewhere,
//! never a second, looser decision; every report names the exact digest of
//! what it projects so a reader can independently recompute it; and the
//! chain-of-custody section distinguishes a bit-for-bit deterministic hash
//! (the evaluation's own seal) from a report-generation hash that varies
//! by when the report was rendered, so two reports of the same sealed
//! evaluation stay distinguishable without pretending the *evaluation*
//! changed.
//!
//! This crate performs no I/O and reads no clock: `generated_at` is
//! supplied by the caller, exactly as `nexo-integrity`/`nexo-sandbox`
//! already require elsewhere in this workspace. `nexo-api` is the adapter
//! that gathers a `ReportInput` from durable state and calls this crate.

use std::fmt::Write as _;

use chrono::{DateTime, Utc};
use serde_json::Value;

/// Everything a report needs, gathered by the caller from durable state.
/// `result` is the exact JSON `nexo-api::explain::render` already produced
/// and sealed as `result_sha256` — this crate never re-derives it from a
/// `nexo_core::ActionEvaluation`, only renders what was already decided.
pub struct ReportInput<'a> {
    pub case_id: i64,
    pub evaluation_id: i64,
    pub bundle_key: &'a str,
    pub bundle_display_name: &'a str,
    pub generated_at: DateTime<Utc>,
    pub result: &'a Value,
    pub result_sha256: &'a str,
}

/// Tone vocabulary shared by Markdown (as a word) and HTML (as a CSS
/// class): what a reader should feel at a glance, derived only from
/// `result["kind"]`/`result["variant"]`/`result["status"]` — fields the
/// rendering in `nexo-api::explain` already guarantees are present, never
/// inferred beyond that.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Tone {
    Ok,
    Caution,
    Fail,
    Muted,
}

impl Tone {
    fn label(self) -> &'static str {
        match self {
            Tone::Ok => "ok",
            Tone::Caution => "caution",
            Tone::Fail => "fail",
            Tone::Muted => "muted",
        }
    }
}

fn tone_and_headline(result: &Value) -> (Tone, String) {
    match result.get("kind").and_then(Value::as_str) {
        Some("actionable") => {
            let status = result.get("status").and_then(Value::as_str).unwrap_or("unknown");
            let available = result.get("available").and_then(Value::as_bool).unwrap_or(false);
            let tone = if available { Tone::Ok } else { Tone::Caution };
            (tone, format!("Actionable — {status}"))
        }
        Some("non_actionable") => {
            let variant = result.get("variant").and_then(Value::as_str).unwrap_or("unknown");
            let tone = match variant {
                "contraindicated" => Tone::Fail,
                "insufficient_facts" | "policy_not_current" => Tone::Caution,
                _ => Tone::Muted,
            };
            let mut headline = format!("Not actionable — {variant}");
            if variant == "abstain"
                && let Some(cause) = result.get("cause").and_then(Value::as_str)
            {
                let _ = write!(headline, " ({cause})");
            }
            (tone, headline)
        }
        _ => (Tone::Muted, "Unknown result shape".to_string()),
    }
}

/// The two-hash chain of custody: one bit-for-bit deterministic
/// (`result_sha256`, unchanged for the same sealed evaluation, independently
/// recomputable by re-fetching `GET /v1/cases/{id}/evaluations`), one that
/// varies by design (`report_hash`, folding in *when this report was
/// rendered* so two reports of the identical evaluation stay
/// distinguishable) — same split ZAYNOR's reporter uses and for the same
/// reason: the report's own render time is not part of what the evaluation
/// sealed, so it must never be mixed into `result_sha256` itself.
fn report_hash(result_sha256: &str, generated_at: DateTime<Utc>) -> String {
    let mut fields = std::collections::BTreeMap::new();
    fields.insert(
        "result_sha256".to_string(),
        nexo_integrity::CanonicalValue::Text(result_sha256.to_string()),
    );
    fields.insert(
        "generated_at_unix_seconds".to_string(),
        nexo_integrity::CanonicalValue::I64(generated_at.timestamp()),
    );
    nexo_integrity::seal(&nexo_integrity::CanonicalValue::Map(fields)).to_string()
}

const METHODOLOGY: &str = "NEXO's deterministic domain core (nexo-core) evaluates a case against a \
cited policy bundle before any explanation is rendered. This report renders \
what that evaluation already decided; it cannot add legal or factual \
support, change an action's state, or invent a route. result_sha256 is the \
canonical seal of the rendered evaluation as returned by GET \
/v1/cases/{case_id}/evaluations at the time this report was generated: \
recompute it independently by re-fetching that endpoint and hashing its \
result field the same way to confirm this report was not altered after \
the fact.";

fn citation_rows(citations: &[Value]) -> Vec<(String, String, String)> {
    citations
        .iter()
        .map(|c| {
            (
                c.get("proposition").and_then(Value::as_str).unwrap_or("-").to_string(),
                c.get("source_issuer").and_then(Value::as_str).unwrap_or("-").to_string(),
                c.get("source_locator").and_then(Value::as_str).unwrap_or("-").to_string(),
            )
        })
        .collect()
}

fn factual_support_rows(items: &[Value]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|item| {
            let kind = item.get("kind").and_then(Value::as_str).unwrap_or("-").to_string();
            let node = item
                .get("case_node_id")
                .map(|v| {
                    if v.is_null() {
                        "unresolved".to_string()
                    } else {
                        v.to_string()
                    }
                })
                .unwrap_or_else(|| "-".to_string());
            (kind, node)
        })
        .collect()
}

fn as_array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value.get(key).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

pub fn render_markdown(input: &ReportInput<'_>) -> String {
    let (tone, headline) = tone_and_headline(input.result);
    let report_hash = report_hash(input.result_sha256, input.generated_at);
    let mut out = String::new();

    let _ = writeln!(out, "# NEXO Case Report — case {}", input.case_id);
    let _ = writeln!(out);
    let _ = writeln!(out, "*Generated {} UTC*", input.generated_at.format("%Y-%m-%d %H:%M:%S"));
    let _ = writeln!(out);
    let _ = writeln!(out, "## Overview");
    let _ = writeln!(out);
    let _ = writeln!(out, "- **Evaluation:** {}", input.evaluation_id);
    let _ = writeln!(out, "- **Policy bundle:** {} (`{}`)", input.bundle_display_name, input.bundle_key);
    let _ = writeln!(out, "- **Result:** {headline} ({})", tone.label());
    let _ = writeln!(out, "- **Result SHA-256:** `{}`", input.result_sha256);
    let _ = writeln!(out);

    let legal_support = as_array(input.result, "legal_support");
    let citations = citation_rows(legal_support);
    let _ = writeln!(out, "## Legal support — why this is shown to you");
    let _ = writeln!(out);
    if citations.is_empty() {
        let _ = writeln!(out, "No legal support recorded for this result.");
    } else {
        let _ = writeln!(out, "| Proposition | Source | Locator |");
        let _ = writeln!(out, "|---|---|---|");
        for (proposition, issuer, locator) in &citations {
            let _ = writeln!(
                out,
                "| {} | {} | {} |",
                proposition.replace('|', "\\|"),
                issuer.replace('|', "\\|"),
                locator.replace('|', "\\|")
            );
        }
    }
    let _ = writeln!(out);

    let factual_support = as_array(input.result, "factual_support");
    let facts = factual_support_rows(factual_support);
    let _ = writeln!(out, "## Factual support");
    let _ = writeln!(out);
    if facts.is_empty() {
        let _ = writeln!(out, "No factual support recorded for this result.");
    } else {
        let _ = writeln!(out, "| Kind | Case node |");
        let _ = writeln!(out, "|---|---|");
        for (kind, node) in &facts {
            let _ = writeln!(out, "| {kind} | {node} |");
        }
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## Chain of custody");
    let _ = writeln!(out);
    let _ = writeln!(out, "```");
    let _ = writeln!(out, "result_sha256 (deterministic): {}", input.result_sha256);
    let _ = writeln!(out, "report_hash (timestamped)    : {report_hash}");
    let _ = writeln!(out, "```");
    let _ = writeln!(out);
    let _ = writeln!(out, "## Methodology");
    let _ = writeln!(out);
    let _ = writeln!(out, "{METHODOLOGY}");

    out
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn render_html(input: &ReportInput<'_>) -> String {
    let (tone, headline) = tone_and_headline(input.result);
    let report_hash = report_hash(input.result_sha256, input.generated_at);
    let legal_support = citation_rows(as_array(input.result, "legal_support"));
    let factual_support = factual_support_rows(as_array(input.result, "factual_support"));

    let legal_rows = if legal_support.is_empty() {
        "<tr><td colspan=\"3\" class=\"empty-state\">No legal support recorded for this result.</td></tr>".to_string()
    } else {
        legal_support
            .iter()
            .map(|(p, i, l)| {
                format!(
                    "<tr><td>{}</td><td>{}</td><td><code>{}</code></td></tr>",
                    escape_html(p),
                    escape_html(i),
                    escape_html(l)
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let factual_rows = if factual_support.is_empty() {
        "<tr><td colspan=\"2\" class=\"empty-state\">No factual support recorded for this result.</td></tr>".to_string()
    } else {
        factual_support
            .iter()
            .map(|(kind, node)| format!("<tr><td>{}</td><td><code>{}</code></td></tr>", escape_html(kind), escape_html(node)))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>NEXO Case Report — case {case_id}</title>
<style>
:root{{
  --bg:#F4F1EA; --bg-elevated:#FFFFFF; --bg-sunken:#ECE8DE;
  --ink:#1C2222; --ink-muted:#5B6460;
  --rule:#D5D6CE; --accent:#2B5D63; --accent-soft:#DCE7E6;
  --ok-bg:#DFEDE2; --ok-ink:#2A5A3C;
  --fail-bg:#F6E3D5; --fail-ink:#7A3A14;
  --caution-bg:#F4EED8; --caution-ink:#765D1C;
  --muted-bg:#E8E9E3; --muted-ink:#5B6460;
  --serif: Georgia,"Times New Roman",serif;
  --sans: -apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,Arial,sans-serif;
  --mono: "SF Mono","IBM Plex Mono",Menlo,Consolas,monospace;
}}
*{{box-sizing:border-box;}}
body{{margin:0;background:var(--bg);color:var(--ink);font-family:var(--sans);font-size:16px;line-height:1.55;}}
.wrap{{max-width:960px;margin:0 auto;padding:0 24px 80px;}}
header{{border-bottom:2px solid var(--ink);padding:36px 0 20px;margin-bottom:28px;}}
header h1{{font-family:var(--serif);font-size:clamp(24px,4vw,34px);margin:0 0 12px;}}
.badge{{display:inline-block;font-family:var(--mono);font-size:12.5px;letter-spacing:.03em;padding:5px 11px;border-radius:12px;}}
.badge.ok{{background:var(--ok-bg);color:var(--ok-ink);}}
.badge.fail{{background:var(--fail-bg);color:var(--fail-ink);}}
.badge.caution{{background:var(--caution-bg);color:var(--caution-ink);}}
.badge.muted{{background:var(--muted-bg);color:var(--muted-ink);}}
.meta{{font-family:var(--mono);font-size:12px;color:var(--ink-muted);margin-top:10px;}}
section{{background:var(--bg-elevated);border:1px solid var(--rule);border-radius:4px;margin:1.4rem 0;padding:20px 22px;}}
section h2{{font-family:var(--serif);font-size:19px;margin:0 0 12px;padding-bottom:8px;border-bottom:1px solid var(--rule);}}
table{{border-collapse:collapse;width:100%;}}
td,th{{text-align:left;padding:8px 10px;border-bottom:1px solid var(--rule);font-size:13.5px;vertical-align:top;}}
th{{font-family:var(--mono);font-size:11px;text-transform:uppercase;letter-spacing:.05em;color:var(--ink-muted);background:var(--bg-sunken);}}
.empty-state{{color:var(--ink-muted);font-style:italic;}}
.chain-block{{font-family:var(--mono);font-size:13px;background:var(--bg-sunken);padding:12px 16px;border-radius:3px;white-space:pre-wrap;}}
code{{font-family:var(--mono);}}
@media print {{
  body{{background:#fff;}}
  section{{box-shadow:none;break-inside:avoid;}}
}}
</style>
</head>
<body>
<div class="wrap">
<header>
<h1>NEXO Case Report — case {case_id}</h1>
<span class="badge {tone}">{headline}</span>
<p class="meta">Generated {generated_at} UTC · evaluation {evaluation_id} · bundle {bundle_display_name} (<code>{bundle_key}</code>)</p>
</header>
<section><h2>Legal support — why this is shown to you</h2>
<table><tr><th>Proposition</th><th>Source</th><th>Locator</th></tr>{legal_rows}</table>
</section>
<section><h2>Factual support</h2>
<table><tr><th>Kind</th><th>Case node</th></tr>{factual_rows}</table>
</section>
<section><h2>Chain of custody</h2>
<div class="chain-block">result_sha256 (deterministic): {result_sha256}
report_hash (timestamped)    : {report_hash}</div>
</section>
<section><h2>Methodology</h2><p>{methodology}</p></section>
</div>
</body>
</html>
"#,
        case_id = input.case_id,
        evaluation_id = input.evaluation_id,
        bundle_display_name = escape_html(input.bundle_display_name),
        bundle_key = escape_html(input.bundle_key),
        generated_at = input.generated_at.format("%Y-%m-%d %H:%M:%S"),
        tone = tone.label(),
        headline = escape_html(&headline),
        legal_rows = legal_rows,
        factual_rows = factual_rows,
        result_sha256 = escape_html(input.result_sha256),
        report_hash = escape_html(&report_hash),
        methodology = escape_html(METHODOLOGY),
    )
}

#[derive(Debug)]
pub enum PdfError {
    /// This build was compiled without the `pdf` feature. Mirrors ZAYNOR's
    /// own `ReportError` for a missing `reportlab` install: a typed,
    /// actionable error, never a panic, and never a silent fallback to a
    /// different format.
    FeatureDisabled,
    /// `printpdf` itself rejected the rendered HTML.
    Render(String),
}

/// Renders the same HTML `render_html` produces into a PDF, via
/// `printpdf`'s HTML-to-PDF mode (feature `pdf`) — the pure-Rust sibling of
/// ZAYNOR's `reportlab` choice: no external binary, no headless browser.
/// Reusing `render_html`'s markup means the PDF, HTML, and Markdown
/// renderers can never structurally drift from each other; only this
/// function's page setup and PDF-specific metadata are new.
///
/// The result's `result_sha256` is embedded twice, not once: as visible
/// text (the same chain-of-custody block every format carries, plus a
/// footer line printpdf repeats on every page) and in the PDF's own
/// document metadata (`subject`, `keywords`, `identifier`) — a tool like
/// `exiftool`/`pdfinfo` can recover the seal without opening the document
/// at all, the same property a reader gets from `zaynor audit` re-hashing
/// ZAYNOR's sealed result.
#[cfg(feature = "pdf")]
pub fn render_pdf(input: &ReportInput<'_>) -> Result<Vec<u8>, PdfError> {
    use std::collections::BTreeMap;

    let html = render_html(input);
    let images = BTreeMap::new();
    let fonts = BTreeMap::new();
    let options = printpdf::GeneratePdfOptions {
        show_page_numbers: Some(true),
        footer_text: Some(format!(
            "NEXO sealed report -- result_sha256 {} -- recompute via GET /v1/cases/{{id}}/evaluations",
            input.result_sha256
        )),
        ..Default::default()
    };
    let mut warnings = Vec::new();
    let mut doc = printpdf::PdfDocument::from_html(&html, &images, &fonts, &options, &mut warnings)
        .map_err(PdfError::Render)?;

    doc.metadata.info.document_title = format!("NEXO Case Report — case {}", input.case_id);
    doc.metadata.info.author = "NEXO".to_string();
    doc.metadata.info.producer = "nexo-report".to_string();
    doc.metadata.info.subject = format!("result_sha256:{}", input.result_sha256);
    doc.metadata.info.identifier = input.result_sha256.to_string();
    doc.metadata.info.keywords = vec![
        "NEXO".to_string(),
        "case-report".to_string(),
        format!("result_sha256:{}", input.result_sha256),
    ];

    let save_options = printpdf::PdfSaveOptions::default();
    let mut save_warnings = Vec::new();
    Ok(doc.save(&save_options, &mut save_warnings))
}

#[cfg(not(feature = "pdf"))]
pub fn render_pdf(_input: &ReportInput<'_>) -> Result<Vec<u8>, PdfError> {
    Err(PdfError::FeatureDisabled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_actionable() -> Value {
        json!({
            "kind": "actionable",
            "status": "supported",
            "available": true,
            "factual_support": [{"kind": "artifact", "case_node_id": 1}],
            "legal_support": [{
                "proposition": "Art. 14, Ley 25.326: derecho de acceso.",
                "source_issuer": "InfoLEG",
                "source_locator": "http://example.gov.ar/law"
            }],
            "unmet_requirements": 0,
        })
    }

    fn sample_insufficient() -> Value {
        json!({
            "kind": "non_actionable",
            "variant": "insufficient_facts",
            "legal_support": [],
            "missing_requirement_count": 1,
        })
    }

    fn input<'a>(result: &'a Value, sha: &'a str) -> ReportInput<'a> {
        ReportInput {
            case_id: 7,
            evaluation_id: 42,
            bundle_key: "ley-25326",
            bundle_display_name: "Ley 25.326",
            generated_at: DateTime::from_timestamp(1_790_000_000, 0).unwrap(),
            result,
            result_sha256: sha,
        }
    }

    #[test]
    fn markdown_names_the_case_and_citation_for_an_actionable_result() {
        let result = sample_actionable();
        let md = render_markdown(&input(&result, "abc123"));
        assert!(md.contains("case 7"));
        assert!(md.contains("Actionable — supported"));
        assert!(md.contains("Art. 14, Ley 25.326"));
        assert!(md.contains("abc123"));
    }

    #[test]
    fn markdown_names_the_variant_for_a_non_actionable_result() {
        let result = sample_insufficient();
        let md = render_markdown(&input(&result, "def456"));
        assert!(md.contains("Not actionable — insufficient_facts"));
        assert!(md.contains("No legal support recorded"));
    }

    #[test]
    fn html_embeds_the_citation_and_is_self_contained() {
        let result = sample_actionable();
        let html = render_html(&input(&result, "abc123"));
        assert!(html.contains("Art. 14, Ley 25.326"));
        assert!(html.contains("abc123"));
        assert!(!html.contains("<script"));
        assert!(!html.contains("http://cdn"));
    }

    #[test]
    fn html_escapes_attacker_controlled_citation_text() {
        let mut result = sample_actionable();
        result["legal_support"][0]["proposition"] = json!("<script>alert(1)</script>");
        let html = render_html(&input(&result, "abc123"));
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn report_hash_changes_with_generation_time_but_result_sha256_does_not() {
        let result = sample_actionable();
        let mut early = input(&result, "abc123");
        early.generated_at = DateTime::from_timestamp(1_000_000_000, 0).unwrap();
        let mut late = input(&result, "abc123");
        late.generated_at = DateTime::from_timestamp(2_000_000_000, 0).unwrap();

        let early_md = render_markdown(&early);
        let late_md = render_markdown(&late);
        assert!(early_md.contains("result_sha256 (deterministic): abc123"));
        assert!(late_md.contains("result_sha256 (deterministic): abc123"));
        // Different report_hash lines despite the identical result_sha256.
        let early_report_hash_line = early_md.lines().find(|l| l.contains("report_hash")).unwrap();
        let late_report_hash_line = late_md.lines().find(|l| l.contains("report_hash")).unwrap();
        assert_ne!(early_report_hash_line, late_report_hash_line);
    }

    #[cfg(not(feature = "pdf"))]
    #[test]
    fn render_pdf_without_the_feature_is_a_typed_error_not_a_panic() {
        let result = sample_actionable();
        let outcome = render_pdf(&input(&result, "abc123"));
        assert!(matches!(outcome, Err(PdfError::FeatureDisabled)));
    }

    #[cfg(feature = "pdf")]
    #[test]
    fn render_pdf_produces_a_real_pdf_carrying_the_hash_in_body_and_metadata() {
        let result = sample_actionable();
        let report_input = input(&result, "abc123def456");
        let bytes = render_pdf(&report_input).expect("pdf render must succeed for well-formed input");

        // A real PDF, not an empty or malformed stub.
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(bytes.len() > 500, "PDF should contain real rendered content, got {} bytes", bytes.len());

        // The hash must be recoverable from the file bytes without a PDF
        // parser: it is embedded as plain metadata text (subject/keywords/
        // identifier), not only inside a compressed content stream.
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            text.contains("abc123def456"),
            "result_sha256 must appear in the PDF's own bytes (metadata), not only in its rendered page content"
        );
    }

    #[cfg(feature = "pdf")]
    #[test]
    fn render_pdf_handles_a_non_actionable_result_without_crashing() {
        let result = sample_insufficient();
        let report_input = input(&result, "def456");
        let bytes = render_pdf(&report_input).expect("pdf render must succeed for a non-actionable result too");
        assert!(bytes.starts_with(b"%PDF-"));
    }
}
