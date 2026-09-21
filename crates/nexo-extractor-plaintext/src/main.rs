//! Plain-text / chat-export extractor, built to run only inside the
//! `docs/SANDBOX.md` worker boundary: no host filesystem access beyond the
//! fixed input/output paths below, no network, no arguments or environment
//! trusted for anything (this binary takes zero argv/env input on purpose,
//! so nothing outside the mounted `/input` file can influence its
//! behavior).
//!
//! Contract: see `docs/EXTRACTOR_PLAINTEXT_CONTRACT.md`.
//!
//! Output is always one of exactly two typed shapes, written to
//! `/output/result.json`:
//!   {"kind":"observations","items":[{"locator":"line:<n>","text":"..."}]}
//!   {"kind":"failure","reason":"<bounded reason code>"}
//! A bounded failure is a successful run (exit code 0): the artifact was
//! rejected on its merits, not because the extractor crashed. Exit code 1
//! is reserved for the extractor itself failing to do its job (cannot read
//! the input path, cannot write the output path) and carries no
//! extraction result at all; the orchestrator treats that as
//! `SandboxError::ExtractorCrashed`, distinct from a bounded failure.

use std::fs;
use std::io::{Read, Write};
use std::path::Path;

const INPUT_PATH: &str = "/input/artifact";
const OUTPUT_PATH: &str = "/output/result.json";
const OUTPUT_TMP_PATH: &str = "/output/result.json.tmp";

/// Hard caps, independent of and in addition to the orchestrator's
/// container-level memory/CPU/time limits (docs/SANDBOX.md "defense in
/// depth" — the extractor does not rely solely on the outer boundary).
const MAX_INPUT_BYTES: u64 = 25 * 1024 * 1024; // 25 MiB
const MAX_LINES: usize = 50_000;
const MAX_LINE_BYTES: usize = 100_000;

fn main() {
    match run() {
        Ok(()) => {}
        Err(message) => {
            eprintln!("nexo-extractor-plaintext: {message}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<(), String> {
    let input_path = Path::new(INPUT_PATH);
    let metadata = fs::metadata(input_path).map_err(|e| format!("cannot stat input: {e}"))?;

    if metadata.len() > MAX_INPUT_BYTES {
        return write_result(&failure_json("input_too_large"));
    }

    let mut file = fs::File::open(input_path).map_err(|e| format!("cannot open input: {e}"))?;
    // Bounded read regardless of what `stat` reported: read one byte past
    // the cap so a file that grew between stat and open is still caught
    // here rather than trusted.
    let mut buffer = Vec::with_capacity((metadata.len() as usize).min(MAX_INPUT_BYTES as usize + 1));
    let mut limited = Read::by_ref(&mut file).take(MAX_INPUT_BYTES + 1);
    limited
        .read_to_end(&mut buffer)
        .map_err(|e| format!("cannot read input: {e}"))?;
    if buffer.len() as u64 > MAX_INPUT_BYTES {
        return write_result(&failure_json("input_too_large"));
    }

    let text = match std::str::from_utf8(&buffer) {
        Ok(text) => text,
        Err(_) => return write_result(&failure_json("invalid_utf8")),
    };

    let mut items = Vec::new();
    for (zero_based_index, line) in text.lines().enumerate() {
        if zero_based_index >= MAX_LINES {
            return write_result(&failure_json("too_many_lines"));
        }
        if line.len() > MAX_LINE_BYTES {
            return write_result(&failure_json("line_too_long"));
        }
        if line.trim().is_empty() {
            continue;
        }
        items.push((zero_based_index + 1, line));
    }

    write_result(&observations_json(&items))
}

fn failure_json(reason: &str) -> String {
    format!(r#"{{"kind":"failure","reason":"{reason}"}}"#)
}

fn observations_json(items: &[(usize, &str)]) -> String {
    let mut out = String::from(r#"{"kind":"observations","items":["#);
    for (index, (line_number, text)) in items.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(r#"{"locator":"line:"#);
        out.push_str(&line_number.to_string());
        out.push_str(r#"","text":""#);
        escape_json_string(text, &mut out);
        out.push_str(r#""}"#);
    }
    out.push_str("]}");
    out
}

/// Escapes a string for embedding as a JSON string body. Handles the
/// characters JSON requires escaping (quote, backslash, and C0 control
/// characters); everything else, including multi-byte UTF-8, passes
/// through unchanged since JSON strings are UTF-8 by specification.
fn escape_json_string(input: &str, out: &mut String) {
    for ch in input.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
}

fn write_result(json: &str) -> Result<(), String> {
    let mut file =
        fs::File::create(OUTPUT_TMP_PATH).map_err(|e| format!("cannot create output: {e}"))?;
    file.write_all(json.as_bytes())
        .map_err(|e| format!("cannot write output: {e}"))?;
    file.sync_all().map_err(|e| format!("cannot flush output: {e}"))?;
    fs::rename(OUTPUT_TMP_PATH, OUTPUT_PATH).map_err(|e| format!("cannot finalize output: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_quotes_backslashes_and_control_characters() {
        let mut out = String::new();
        escape_json_string("a\"b\\c\nd\te", &mut out);
        assert_eq!(out, "a\\\"b\\\\c\\nd\\te");
    }

    #[test]
    fn escapes_low_control_characters_not_covered_by_named_escapes() {
        let mut out = String::new();
        escape_json_string("\u{0001}", &mut out);
        assert_eq!(out, "\\u0001");
    }

    #[test]
    fn multi_byte_utf8_passes_through_unchanged() {
        let mut out = String::new();
        escape_json_string("café ☕", &mut out);
        assert_eq!(out, "café ☕");
    }

    #[test]
    fn observations_json_shape_is_well_formed_for_empty_input() {
        assert_eq!(observations_json(&[]), r#"{"kind":"observations","items":[]}"#);
    }

    #[test]
    fn observations_json_carries_line_locator_and_text() {
        let items = vec![(3, "hello"), (7, "world")];
        assert_eq!(
            observations_json(&items),
            r#"{"kind":"observations","items":[{"locator":"line:3","text":"hello"},{"locator":"line:7","text":"world"}]}"#
        );
    }
}
