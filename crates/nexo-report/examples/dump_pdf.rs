//! Manual PDF smoke check: `cargo run -p nexo-report --example dump_pdf --features pdf [output.pdf]`.
//! Verified by hand against `pdfinfo`/`pdftotext`: 1-page A4, Title/
//! Subject/Keywords carry the result_sha256, and the rendered text matches
//! every section `render_html`/`render_markdown` produce.

use chrono::DateTime;
use nexo_report::{render_pdf, ReportInput};
use serde_json::json;

fn main() {
    let result = json!({
        "kind": "actionable",
        "status": "supported",
        "available": true,
        "factual_support": [{"kind": "artifact", "case_node_id": 1}],
        "legal_support": [{
            "proposition": "Art. 14, Ley 25.326: derecho de acceso a los propios datos personales.",
            "source_issuer": "InfoLEG",
            "source_locator": "http://servicios.infoleg.gob.ar/infolegInternet/anexos/60000-64999/64790/texact.htm"
        }],
        "unmet_requirements": 0,
    });
    let input = ReportInput {
        case_id: 7,
        evaluation_id: 42,
        bundle_key: "ley-25326",
        bundle_display_name: "Ley 25.326 — acceso, rectificación y supresión de datos personales",
        generated_at: DateTime::from_timestamp(1_790_100_000, 0).unwrap(),
        result: &result,
        result_sha256: "fa7db50d5213cf1600b33422b910101afb71bf9b24c972529526aba5a177f58c",
    };
    let bytes = render_pdf(&input).expect("pdf render must succeed");
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "nexo-report-sample.pdf".to_string());
    std::fs::write(&out, &bytes).unwrap();
    eprintln!("wrote {} bytes to {out}", bytes.len());
}
