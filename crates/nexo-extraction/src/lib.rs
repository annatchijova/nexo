//! Typed extraction outcomes for the extractor images registered here.
//!
//! `nexo-sandbox` deliberately returns raw bytes and does not interpret
//! them — the sandbox boundary and the meaning of one extractor's output
//! are different concerns. This crate is the other half: it knows the
//! fixed, hand-written JSON shape `nexo-extractor-plaintext` promises to
//! produce (see that crate's `main.rs`) and turns the bytes read back from
//! the sandboxed container into a typed `PlaintextExtraction`, or a typed
//! `ExtractionAdapterError` if the container produced something outside
//! that promised shape — which is treated as an adapter/contract bug, not
//! as case data, and never silently coerced into an empty result.

use nexo_sandbox::{run_extraction, SandboxError, SandboxLimits};
use serde::Deserialize;

pub const PLAINTEXT_EXTRACTOR_IMAGE: &str = "nexo-extractor-plaintext:local";

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum RawResult {
    Observations { items: Vec<RawObservation> },
    Failure { reason: String },
}

#[derive(Debug, Deserialize)]
struct RawObservation {
    locator: String,
    text: String,
}

/// One candidate observation extracted from an artifact: a locator (line
/// reference for this extractor) and the exact text at that locator. This
/// is a candidate, not yet a `nexo_core::ObservationNode` — promoting it
/// into the case graph is an application-layer decision that also needs an
/// `ArtifactId`, a `ToolVersion`, and a `UtcInstant`, none of which this
/// crate has authority to invent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationCandidate {
    pub locator: String,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlaintextExtraction {
    Observations(Vec<ObservationCandidate>),
    /// A bounded, named rejection reason (`invalid_utf8`, `input_too_large`,
    /// `too_many_lines`, `line_too_long`) — never a partial or best-effort
    /// result, per `docs/SANDBOX.md`.
    Failure(String),
}

#[derive(Debug)]
pub enum ExtractionAdapterError {
    Sandbox(SandboxError),
    /// The container exited successfully and produced a result file, but
    /// its content does not match the shape this adapter knows how to
    /// read. This must never be treated as "no observations": it means the
    /// extractor image and this adapter have drifted apart, which is a
    /// deployment bug to surface loudly, not paper over.
    MalformedResult(serde_json::Error),
}

impl From<SandboxError> for ExtractionAdapterError {
    fn from(value: SandboxError) -> Self {
        Self::Sandbox(value)
    }
}

pub fn extract_plaintext(
    input_bytes: &[u8],
    limits: &SandboxLimits,
) -> Result<PlaintextExtraction, ExtractionAdapterError> {
    let raw_bytes = run_extraction(PLAINTEXT_EXTRACTOR_IMAGE, input_bytes, limits)?;
    let raw: RawResult =
        serde_json::from_slice(&raw_bytes).map_err(ExtractionAdapterError::MalformedResult)?;
    Ok(match raw {
        RawResult::Observations { items } => PlaintextExtraction::Observations(
            items
                .into_iter()
                .map(|item| ObservationCandidate {
                    locator: item.locator,
                    text: item.text,
                })
                .collect(),
        ),
        RawResult::Failure { reason } => PlaintextExtraction::Failure(reason),
    })
}

pub const EML_EXTRACTOR_IMAGE: &str = "nexo-extractor-eml:local";

/// One candidate extracted from an email: either a message header NEXO
/// chose to surface as evidence (`header:from`, `header:to`,
/// `header:subject`, `header:date`) or a line of the decoded text body
/// (`body:line:<n>`) — see `docs/EXTRACTOR_EML_CONTRACT.md`. Reuses
/// `ObservationCandidate`'s shape since both are "a locator plus the exact
/// text at it," but kept as its own extraction result type (`EmlExtraction`
/// below) rather than folded into `PlaintextExtraction`, since the two
/// extractors accept different input formats and fail for different typed
/// reasons.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EmlExtraction {
    Observations(Vec<ObservationCandidate>),
    /// A bounded, named rejection reason — see
    /// `docs/EXTRACTOR_EML_CONTRACT.md` for the full list (`invalid_utf8`,
    /// `missing_boundary`, `no_text_part_found`, `unsupported_charset`,
    /// `unsupported_transfer_encoding`, ...). Never a partial or
    /// best-effort result.
    Failure(String),
}

pub fn extract_eml(
    input_bytes: &[u8],
    limits: &SandboxLimits,
) -> Result<EmlExtraction, ExtractionAdapterError> {
    let raw_bytes = run_extraction(EML_EXTRACTOR_IMAGE, input_bytes, limits)?;
    let raw: RawResult =
        serde_json::from_slice(&raw_bytes).map_err(ExtractionAdapterError::MalformedResult)?;
    Ok(match raw {
        RawResult::Observations { items } => EmlExtraction::Observations(
            items
                .into_iter()
                .map(|item| ObservationCandidate {
                    locator: item.locator,
                    text: item.text,
                })
                .collect(),
        ),
        RawResult::Failure { reason } => EmlExtraction::Failure(reason),
    })
}

pub const PDF_EXTRACTOR_IMAGE: &str = "nexo-extractor-pdf:local";

/// One candidate extracted from a PDF: one text-showing operation
/// (`Tj`/`TJ`/`'`/`"`) on a given page, located as `page:<n>:text:<m>` —
/// see `docs/EXTRACTOR_PDF_CONTRACT.md`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PdfExtraction {
    Observations(Vec<ObservationCandidate>),
    /// A bounded, named rejection reason — see
    /// `docs/EXTRACTOR_PDF_CONTRACT.md` for the full list (`no_pages_found`,
    /// `stream_too_large`, `unsupported_font_encoding`, ...). Never a
    /// partial or best-effort result.
    Failure(String),
}

pub fn extract_pdf(
    input_bytes: &[u8],
    limits: &SandboxLimits,
) -> Result<PdfExtraction, ExtractionAdapterError> {
    let raw_bytes = run_extraction(PDF_EXTRACTOR_IMAGE, input_bytes, limits)?;
    let raw: RawResult =
        serde_json::from_slice(&raw_bytes).map_err(ExtractionAdapterError::MalformedResult)?;
    Ok(match raw {
        RawResult::Observations { items } => PdfExtraction::Observations(
            items
                .into_iter()
                .map(|item| ObservationCandidate {
                    locator: item.locator,
                    text: item.text,
                })
                .collect(),
        ),
        RawResult::Failure { reason } => PdfExtraction::Failure(reason),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    use std::time::Duration;

    fn image_available_named(image: &str) -> bool {
        Command::new("docker")
            .args(["image", "inspect", image])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    fn image_available() -> bool {
        Command::new("docker")
            .args(["image", "inspect", PLAINTEXT_EXTRACTOR_IMAGE])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    fn limits() -> SandboxLimits {
        SandboxLimits {
            wall_clock: Duration::from_secs(10),
            ..SandboxLimits::conservative_default()
        }
    }

    #[test]
    fn valid_text_produces_observations_with_line_locators() {
        if !image_available() {
            eprintln!(
                "skipping: {PLAINTEXT_EXTRACTOR_IMAGE} not built \
                 (see crates/nexo-extractor-plaintext/Dockerfile)"
            );
            return;
        }
        let input = "hello world\n\nsegunda linea\ncafé \u{2615}\n";
        let result = extract_plaintext(input.as_bytes(), &limits()).unwrap();
        assert_eq!(
            result,
            PlaintextExtraction::Observations(vec![
                ObservationCandidate {
                    locator: "line:1".into(),
                    text: "hello world".into(),
                },
                ObservationCandidate {
                    locator: "line:3".into(),
                    text: "segunda linea".into(),
                },
                ObservationCandidate {
                    locator: "line:4".into(),
                    text: "café \u{2615}".into(),
                },
            ])
        );
    }

    #[test]
    fn invalid_utf8_is_a_bounded_failure_not_a_crash() {
        if !image_available() {
            eprintln!("skipping: {PLAINTEXT_EXTRACTOR_IMAGE} not built");
            return;
        }
        let input: &[u8] = &[0xff, 0xfe, b'n', b'o', b't', b' ', b'u', b't', b'f', b'8'];
        let result = extract_plaintext(input, &limits()).unwrap();
        assert_eq!(result, PlaintextExtraction::Failure("invalid_utf8".into()));
    }

    #[test]
    fn oversized_input_is_a_bounded_failure() {
        if !image_available() {
            eprintln!("skipping: {PLAINTEXT_EXTRACTOR_IMAGE} not built");
            return;
        }
        // One byte past the extractor's 25 MiB cap.
        let oversized = vec![b'a'; 25 * 1024 * 1024 + 1];
        let result = extract_plaintext(&oversized, &limits()).unwrap();
        assert_eq!(
            result,
            PlaintextExtraction::Failure("input_too_large".into())
        );
    }

    #[test]
    fn empty_input_produces_an_empty_observation_list_not_a_failure() {
        if !image_available() {
            eprintln!("skipping: {PLAINTEXT_EXTRACTOR_IMAGE} not built");
            return;
        }
        let result = extract_plaintext(b"", &limits()).unwrap();
        assert_eq!(result, PlaintextExtraction::Observations(vec![]));
    }

    #[test]
    fn eml_message_produces_header_and_body_observations() {
        if !image_available_named(EML_EXTRACTOR_IMAGE) {
            eprintln!(
                "skipping: {EML_EXTRACTOR_IMAGE} not built \
                 (see crates/nexo-extractor-eml/Dockerfile)"
            );
            return;
        }
        let message = b"From: alice@example.com\r\nTo: bob@example.com\r\nSubject: Hola\r\nDate: Mon, 1 Jan 2024 00:00:00 +0000\r\n\r\nHola Bob.\r\n";
        let result = extract_eml(message, &limits()).unwrap();
        assert_eq!(
            result,
            EmlExtraction::Observations(vec![
                ObservationCandidate { locator: "header:date".into(), text: "Mon, 1 Jan 2024 00:00:00 +0000".into() },
                ObservationCandidate { locator: "header:from".into(), text: "alice@example.com".into() },
                ObservationCandidate { locator: "header:to".into(), text: "bob@example.com".into() },
                ObservationCandidate { locator: "header:subject".into(), text: "Hola".into() },
                ObservationCandidate { locator: "body:line:1".into(), text: "Hola Bob.".into() },
            ])
        );
    }

    #[test]
    fn eml_invalid_utf8_is_a_bounded_failure_not_a_crash() {
        if !image_available_named(EML_EXTRACTOR_IMAGE) {
            eprintln!("skipping: {EML_EXTRACTOR_IMAGE} not built");
            return;
        }
        let input: &[u8] = &[0xff, 0xfe, b'n', b'o', b't', b' ', b'u', b't', b'f', b'8'];
        let result = extract_eml(input, &limits()).unwrap();
        assert_eq!(result, EmlExtraction::Failure("invalid_utf8".into()));
    }

    #[test]
    fn eml_oversized_input_is_a_bounded_failure() {
        if !image_available_named(EML_EXTRACTOR_IMAGE) {
            eprintln!("skipping: {EML_EXTRACTOR_IMAGE} not built");
            return;
        }
        let oversized = vec![b'a'; 25 * 1024 * 1024 + 1];
        let result = extract_eml(&oversized, &limits()).unwrap();
        assert_eq!(result, EmlExtraction::Failure("input_too_large".into()));
    }

    #[test]
    fn eml_unsupported_transfer_encoding_is_a_bounded_failure() {
        if !image_available_named(EML_EXTRACTOR_IMAGE) {
            eprintln!("skipping: {EML_EXTRACTOR_IMAGE} not built");
            return;
        }
        let message = b"Content-Transfer-Encoding: x-proprietary\r\n\r\nbody\r\n";
        let result = extract_eml(message, &limits()).unwrap();
        assert_eq!(
            result,
            EmlExtraction::Failure("unsupported_transfer_encoding".into())
        );
    }

    #[test]
    fn eml_empty_input_produces_an_empty_observation_list_not_a_failure() {
        if !image_available_named(EML_EXTRACTOR_IMAGE) {
            eprintln!("skipping: {EML_EXTRACTOR_IMAGE} not built");
            return;
        }
        let result = extract_eml(b"", &limits()).unwrap();
        assert_eq!(result, EmlExtraction::Observations(vec![]));
    }

    #[test]
    fn pdf_message_produces_text_observations() {
        if !image_available_named(PDF_EXTRACTOR_IMAGE) {
            eprintln!(
                "skipping: {PDF_EXTRACTOR_IMAGE} not built \
                 (see crates/nexo-extractor-pdf/Dockerfile)"
            );
            return;
        }
        let message = b"BT /F1 12 Tf (Hola PDF) Tj ET";
        let pdf = format!(
            "%PDF-1.4\n\
             3 0 obj\n<< /Type /Page /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>\nendobj\n\
             4 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n\
             5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
            message.len(),
            String::from_utf8_lossy(message)
        );
        let result = extract_pdf(pdf.as_bytes(), &limits()).unwrap();
        assert_eq!(
            result,
            PdfExtraction::Observations(vec![ObservationCandidate {
                locator: "page:1:text:1".into(),
                text: "Hola PDF".into(),
            }])
        );
    }

    #[test]
    fn pdf_oversized_input_is_a_bounded_failure() {
        if !image_available_named(PDF_EXTRACTOR_IMAGE) {
            eprintln!("skipping: {PDF_EXTRACTOR_IMAGE} not built");
            return;
        }
        let oversized = vec![b'a'; 25 * 1024 * 1024 + 1];
        let result = extract_pdf(&oversized, &limits()).unwrap();
        assert_eq!(result, PdfExtraction::Failure("input_too_large".into()));
    }

    /// Not a hand-built fixture: a real PDF produced by LibreOffice Writer
    /// from a plain-text document (`soffice --headless --convert-to pdf`),
    /// containing accented Spanish text. Hand-built fixtures only prove the
    /// parser accepts what it was written to accept — this proves it
    /// handles a real PDF writer's actual output, including whatever
    /// object layout, compression, and font setup LibreOffice happens to
    /// produce, not just this crate's own assumptions about PDF shape.
    #[test]
    fn real_libreoffice_pdf_is_extracted_with_accented_text_intact() {
        if !image_available_named(PDF_EXTRACTOR_IMAGE) {
            eprintln!("skipping: {PDF_EXTRACTOR_IMAGE} not built");
            return;
        }
        let pdf_bytes = std::fs::read(
            concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/real_libreoffice_export.pdf"),
        )
        .expect("fixture PDF must be readable");
        let result = extract_pdf(&pdf_bytes, &limits()).unwrap();
        let PdfExtraction::Observations(items) = result else {
            panic!("expected observations, got a failure: {result:?}");
        };
        assert!(!items.is_empty(), "a real PDF with real text must yield observations");
        let all_text: String = items.iter().map(|item| item.text.as_str()).collect::<Vec<_>>().join(" ");
        assert!(
            all_text.contains("Solicito acceso"),
            "expected the document's own text in the extraction, got: {all_text:?}"
        );
        assert!(
            all_text.contains("café") || all_text.contains("corazón"),
            "expected accented Spanish text to survive extraction intact, got: {all_text:?}"
        );
    }
}
