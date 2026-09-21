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

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    use std::time::Duration;

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
}
