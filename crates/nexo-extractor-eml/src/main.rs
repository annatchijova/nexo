//! Email (`.eml`, RFC 5322 + a bounded subset of MIME) extractor, built to
//! run only inside the `docs/SANDBOX.md` worker boundary: no host
//! filesystem access beyond the fixed input/output paths below, no
//! network, no arguments or environment trusted for anything.
//!
//! Contract: see `docs/EXTRACTOR_EML_CONTRACT.md`.
//!
//! Output is always one of exactly two typed shapes, written to
//! `/output/result.json`:
//!   {"kind":"observations","items":[{"locator":"header:from","text":"..."},{"locator":"body:line:<n>","text":"..."}]}
//!   {"kind":"failure","reason":"<bounded reason code>"}
//! A bounded failure is a successful run (exit code 0): the artifact was
//! rejected on its merits, not because the extractor crashed. Exit code 1
//! is reserved for the extractor itself failing to do its job (cannot read
//! the input path, cannot write the output path); the orchestrator treats
//! that as `SandboxError::ExtractorCrashed`, distinct from a bounded
//! failure — same split `nexo-extractor-plaintext` uses.
//!
//! This binary is deliberately self-contained and duplicates the small
//! amount of JSON-encoding and byte-reading logic `nexo-extractor-plaintext`
//! also has, rather than sharing a crate with it: each extractor that faces
//! hostile bytes stays independently auditable, with no shared code whose
//! bug would affect every extractor at once.

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
const MAX_HEADERS: usize = 500;
const MAX_HEADER_BYTES: usize = 10_000;
const MAX_LINES: usize = 50_000;
const MAX_LINE_BYTES: usize = 100_000;
const MAX_MIME_PARTS: usize = 50;

fn main() {
    match run() {
        Ok(()) => {}
        Err(message) => {
            eprintln!("nexo-extractor-eml: {message}");
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

    match extract(text) {
        Ok(items) => write_result(&observations_json(&items)),
        Err(reason) => write_result(&failure_json(reason)),
    }
}

#[derive(Debug, PartialEq)]
struct Item {
    locator: String,
    text: String,
}

/// The message headers this extractor surfaces as evidence in their own
/// right (who, to whom, about what, when) — never the full header block,
/// which may carry routing/server internals with no evidentiary value.
struct Headers {
    date: Option<String>,
    from: Option<String>,
    to: Option<String>,
    subject: Option<String>,
    content_type: Option<String>,
    content_transfer_encoding: Option<String>,
}

fn extract(message: &str) -> Result<Vec<Item>, &'static str> {
    let (headers, body) = split_headers_and_body(message)?;
    let mut items = Vec::new();
    if let Some(date) = &headers.date {
        items.push(Item { locator: "header:date".into(), text: date.clone() });
    }
    if let Some(from) = &headers.from {
        items.push(Item { locator: "header:from".into(), text: from.clone() });
    }
    if let Some(to) = &headers.to {
        items.push(Item { locator: "header:to".into(), text: to.clone() });
    }
    if let Some(subject) = &headers.subject {
        items.push(Item { locator: "header:subject".into(), text: subject.clone() });
    }

    let (media_type, params) = parse_content_type(headers.content_type.as_deref());
    check_charset(&params)?;

    let body_text = if media_type.starts_with("multipart/") {
        let boundary = params
            .iter()
            .find(|(name, _)| name == "boundary")
            .map(|(_, value)| value.clone())
            .ok_or("missing_boundary")?;
        extract_first_text_part(body, &boundary)?
    } else {
        decode_body(body, headers.content_transfer_encoding.as_deref())?
    };

    append_body_lines(&body_text, &mut items)?;
    Ok(items)
}

/// Splits on the first blank line, per RFC 5322 §2.1: everything before it
/// is the header block, everything after is the body. Unfolds continuation
/// lines (starting with a space or tab, RFC 5322 §2.2.3) into their
/// header's value with a single joining space. A message with no blank
/// line at all is treated as all-headers, empty body — a valid, if
/// unusual, shape, not an error.
fn split_headers_and_body(message: &str) -> Result<(Headers, &str), &'static str> {
    let mut headers = Headers {
        date: None,
        from: None,
        to: None,
        subject: None,
        content_type: None,
        content_transfer_encoding: None,
    };

    let mut header_count = 0usize;
    let mut current: Option<(String, String)> = None; // (lowercased name, value)
    let mut byte_offset = 0usize;
    let mut body_start = message.len();

    for line in message.split_inclusive('\n') {
        let trimmed_end = line.trim_end_matches(['\n', '\r']);
        if trimmed_end.is_empty() {
            body_start = byte_offset + line.len();
            break;
        }
        if (trimmed_end.starts_with(' ') || trimmed_end.starts_with('\t'))
            && let Some((_, value)) = current.as_mut()
        {
            value.push(' ');
            value.push_str(trimmed_end.trim());
            if value.len() > MAX_HEADER_BYTES {
                return Err("header_too_long");
            }
        } else {
            if let Some((name, value)) = current.take() {
                store_header(&mut headers, &name, value);
            }
            let Some((name, value)) = trimmed_end.split_once(':') else {
                // Not a header line and not a continuation: malformed
                // header block. Treated the same as "no body found beyond
                // this point" rather than a distinct failure code — a
                // message this malformed has no reliable body boundary
                // either.
                return Err("malformed_header");
            };
            header_count += 1;
            if header_count > MAX_HEADERS {
                return Err("too_many_headers");
            }
            let value = value.trim().to_string();
            if value.len() > MAX_HEADER_BYTES {
                return Err("header_too_long");
            }
            current = Some((name.trim().to_ascii_lowercase(), value));
        }
        byte_offset += line.len();
    }
    if let Some((name, value)) = current.take() {
        store_header(&mut headers, &name, value);
    }

    Ok((headers, &message[body_start.min(message.len())..]))
}

fn store_header(headers: &mut Headers, lowercase_name: &str, value: String) {
    match lowercase_name {
        "date" if headers.date.is_none() => headers.date = Some(value),
        "from" if headers.from.is_none() => headers.from = Some(value),
        "to" if headers.to.is_none() => headers.to = Some(value),
        "subject" if headers.subject.is_none() => headers.subject = Some(value),
        "content-type" if headers.content_type.is_none() => headers.content_type = Some(value),
        "content-transfer-encoding" if headers.content_transfer_encoding.is_none() => {
            headers.content_transfer_encoding = Some(value)
        }
        _ => {}
    }
}

/// Parses `type/subtype; name=value; name="value"` into a lowercased media
/// type and its parameters. Per RFC 2045 §5.2, an absent Content-Type
/// defaults to `text/plain; charset=us-ascii`.
fn parse_content_type(raw: Option<&str>) -> (String, Vec<(String, String)>) {
    let Some(raw) = raw else {
        return ("text/plain".to_string(), vec![("charset".to_string(), "us-ascii".to_string())]);
    };
    let mut parts = raw.split(';');
    let media_type = parts.next().unwrap_or("").trim().to_ascii_lowercase();
    let mut params = Vec::new();
    for param in parts {
        let Some((name, value)) = param.split_once('=') else { continue };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim().trim_matches('"').to_string();
        params.push((name, value));
    }
    (media_type, params)
}

fn check_charset(params: &[(String, String)]) -> Result<(), &'static str> {
    if let Some((_, charset)) = params.iter().find(|(name, _)| name == "charset") {
        let charset = charset.to_ascii_lowercase();
        if charset != "utf-8" && charset != "us-ascii" && charset != "ascii" {
            return Err("unsupported_charset");
        }
    }
    Ok(())
}

fn decode_body(body: &str, transfer_encoding: Option<&str>) -> Result<String, &'static str> {
    match transfer_encoding.map(str::to_ascii_lowercase).as_deref() {
        None | Some("7bit") | Some("8bit") | Some("binary") => Ok(body.to_string()),
        Some("quoted-printable") => decode_quoted_printable(body),
        Some("base64") => decode_base64_text(body),
        Some(_) => Err("unsupported_transfer_encoding"),
    }
}

/// RFC 2045 §6.7: `=XX` is a hex-encoded byte, a trailing `=` before the
/// line break is a soft line break (join with no newline), everything else
/// passes through unchanged. Both transformations are strictly
/// length-reducing or length-neutral — quoted-printable can only expand a
/// byte to `=XX` (3 bytes) when *encoding*; decoding it back is always
/// shorter than or equal to the encoded input, so this can never amplify
/// `body` past its own already-bounded length.
fn decode_quoted_printable(body: &str) -> Result<String, &'static str> {
    let bytes = body.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'=' if i + 2 < bytes.len() && bytes[i + 1] == b'\r' && bytes[i + 2] == b'\n' => {
                i += 3; // soft line break: drop it, no newline inserted
            }
            b'=' if i + 1 < bytes.len() && bytes[i + 1] == b'\n' => {
                i += 2; // soft line break, lone LF
            }
            b'=' if i + 2 < bytes.len() => {
                let hi = hex_value(bytes[i + 1]);
                let lo = hex_value(bytes[i + 2]);
                match (hi, lo) {
                    (Some(hi), Some(lo)) => {
                        out.push((hi << 4) | lo);
                        i += 3;
                    }
                    _ => {
                        out.push(b'=');
                        i += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| "invalid_utf8")
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Standard base64 (RFC 4648 §4), whitespace between groups ignored (MIME
/// wraps base64 bodies at a fixed line length, RFC 2045 §6.8). Decoding
/// strictly shrinks the input (4 encoded bytes -> at most 3 decoded bytes),
/// so this can never amplify `body` past its own already-bounded length.
fn decode_base64_text(body: &str) -> Result<String, &'static str> {
    let mut cleaned: Vec<u8> = Vec::with_capacity(body.len());
    for byte in body.bytes() {
        if !byte.is_ascii_whitespace() {
            cleaned.push(byte);
        }
    }
    if cleaned.is_empty() {
        return Ok(String::new());
    }
    if !cleaned.len().is_multiple_of(4) {
        return Err("invalid_base64");
    }
    let mut out = Vec::with_capacity(cleaned.len() / 4 * 3);
    for chunk in cleaned.chunks(4) {
        let mut values = [0u8; 4];
        let mut pad = 0;
        for (index, &byte) in chunk.iter().enumerate() {
            if byte == b'=' {
                pad += 1;
                values[index] = 0;
            } else {
                values[index] = base64_value(byte).ok_or("invalid_base64")?;
            }
        }
        let triple = ((values[0] as u32) << 18)
            | ((values[1] as u32) << 12)
            | ((values[2] as u32) << 6)
            | values[3] as u32;
        out.push((triple >> 16) as u8);
        if pad < 2 {
            out.push((triple >> 8) as u8);
        }
        if pad < 1 {
            out.push(triple as u8);
        }
    }
    String::from_utf8(out).map_err(|_| "invalid_utf8")
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Splits `body` on `--{boundary}` delimiter lines (RFC 2046 §5.1.1),
/// parses each part's own small header block, and returns the decoded text
/// of the first part whose media type is `text/plain` (or has no
/// Content-Type of its own, which per RFC 2045 §5.2 defaults to
/// `text/plain; charset=us-ascii`). Only the immediate parts are examined —
/// a part that is itself `multipart/*` is skipped, not recursed into; a
/// message that nests its text only inside a further multipart part is a
/// stated non-goal (`docs/EXTRACTOR_EML_CONTRACT.md`), not silently
/// misread.
fn extract_first_text_part(body: &str, boundary: &str) -> Result<String, &'static str> {
    let delimiter = format!("--{boundary}");
    let mut parts_seen = 0usize;
    let mut current_part_start: Option<usize> = None;

    for (line_start, line) in line_starts(body) {
        let content = line.trim_end_matches(['\n', '\r']);
        if content == delimiter || content == format!("{delimiter}--") {
            if let Some(start) = current_part_start {
                parts_seen += 1;
                if parts_seen > MAX_MIME_PARTS {
                    return Err("too_many_parts");
                }
                if let Some(text) = try_read_text_part(&body[start..line_start]) {
                    return Ok(text);
                }
            }
            current_part_start = Some(line_start + line.len());
            if content.ends_with("--") {
                break;
            }
        }
    }
    Err("no_text_part_found")
}

fn line_starts(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut offset = 0usize;
    text.split_inclusive('\n').map(move |line| {
        let start = offset;
        offset += line.len();
        (start, line)
    })
}

/// Parses one MIME part's own header block (Content-Type /
/// Content-Transfer-Encoding only) and returns its decoded text if its
/// media type is `text/plain`, otherwise `None`.
fn try_read_text_part(part: &str) -> Option<String> {
    let (headers, part_body) = split_headers_and_body(part).ok()?;
    let (media_type, params) = parse_content_type(headers.content_type.as_deref());
    if media_type != "text/plain" {
        return None;
    }
    if check_charset(&params).is_err() {
        return None;
    }
    decode_body(part_body, headers.content_transfer_encoding.as_deref()).ok()
}

fn append_body_lines(body: &str, items: &mut Vec<Item>) -> Result<(), &'static str> {
    for (zero_based_index, line) in body.lines().enumerate() {
        if zero_based_index >= MAX_LINES {
            return Err("too_many_lines");
        }
        if line.len() > MAX_LINE_BYTES {
            return Err("line_too_long");
        }
        if line.trim().is_empty() {
            continue;
        }
        items.push(Item {
            locator: format!("body:line:{}", zero_based_index + 1),
            text: line.to_string(),
        });
    }
    Ok(())
}

fn failure_json(reason: &str) -> String {
    format!(r#"{{"kind":"failure","reason":"{reason}"}}"#)
}

fn observations_json(items: &[Item]) -> String {
    let mut out = String::from(r#"{"kind":"observations","items":["#);
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(r#"{"locator":""#);
        escape_json_string(&item.locator, &mut out);
        out.push_str(r#"","text":""#);
        escape_json_string(&item.text, &mut out);
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
    fn simple_message_yields_header_and_body_observations() {
        let message = "From: alice@example.com\r\nTo: bob@example.com\r\nSubject: Hola\r\nDate: Mon, 1 Jan 2024 00:00:00 +0000\r\n\r\nHola Bob.\r\n\r\nGracias.\r\n";
        let items = extract(message).unwrap();
        let locators: Vec<&str> = items.iter().map(|i| i.locator.as_str()).collect();
        assert_eq!(
            locators,
            vec!["header:date", "header:from", "header:to", "header:subject", "body:line:1", "body:line:3"]
        );
        assert_eq!(items[1].text, "alice@example.com");
        assert_eq!(items[4].text, "Hola Bob.");
        assert_eq!(items[5].text, "Gracias.");
    }

    #[test]
    fn folded_header_continuation_is_unfolded_with_one_space() {
        let message = "Subject: Line one\r\n continued line two\r\n\r\nBody.\r\n";
        let items = extract(message).unwrap();
        assert_eq!(items[0].text, "Line one continued line two");
    }

    #[test]
    fn message_with_no_blank_line_is_all_headers_empty_body() {
        let message = "From: alice@example.com\r\nSubject: no body here\r\n";
        let items = extract(message).unwrap();
        let locators: Vec<&str> = items.iter().map(|i| i.locator.as_str()).collect();
        assert_eq!(locators, vec!["header:from", "header:subject"]);
    }

    #[test]
    fn quoted_printable_body_is_decoded() {
        // The trailing `=` before the CRLF is a soft line break (RFC 2045
        // §6.7): it is consumed, and the next line joins with no newline
        // or space inserted — "signo" and "de igual." become one word.
        let message = "Content-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\nCaf=C3=A9 despu=C3=A9s del signo=\r\nde igual.\r\n";
        let items = extract(message).unwrap();
        assert_eq!(items[0].text, "Café después del signode igual.");
    }

    #[test]
    fn base64_body_is_decoded() {
        // "Hola mundo" base64-encoded.
        let message =
            "Content-Type: text/plain\r\nContent-Transfer-Encoding: base64\r\n\r\nSG9sYSBtdW5kbw==\r\n";
        let items = extract(message).unwrap();
        assert_eq!(items[0].text, "Hola mundo");
    }

    #[test]
    fn multipart_picks_the_first_text_plain_part() {
        let message = "Content-Type: multipart/alternative; boundary=\"XYZ\"\r\n\r\n--XYZ\r\nContent-Type: text/plain\r\n\r\nplano\r\n--XYZ\r\nContent-Type: text/html\r\n\r\n<p>html</p>\r\n--XYZ--\r\n";
        let items = extract(message).unwrap();
        assert_eq!(items[0].text, "plano");
    }

    #[test]
    fn multipart_without_a_text_part_is_a_bounded_failure() {
        let message = "Content-Type: multipart/mixed; boundary=\"XYZ\"\r\n\r\n--XYZ\r\nContent-Type: text/html\r\n\r\n<p>only html</p>\r\n--XYZ--\r\n";
        assert_eq!(extract(message), Err("no_text_part_found"));
    }

    #[test]
    fn multipart_without_a_boundary_parameter_is_a_bounded_failure() {
        let message = "Content-Type: multipart/mixed\r\n\r\nsomething\r\n";
        assert_eq!(extract(message), Err("missing_boundary"));
    }

    #[test]
    fn unsupported_transfer_encoding_is_a_bounded_failure() {
        let message = "Content-Transfer-Encoding: x-proprietary\r\n\r\nbody\r\n";
        assert_eq!(extract(message), Err("unsupported_transfer_encoding"));
    }

    #[test]
    fn unsupported_charset_is_a_bounded_failure() {
        let message = "Content-Type: text/plain; charset=iso-8859-1\r\n\r\nbody\r\n";
        assert_eq!(extract(message), Err("unsupported_charset"));
    }

    #[test]
    fn malformed_header_block_is_a_bounded_failure() {
        let message = "This is not a header line at all\r\n\r\nbody\r\n";
        assert_eq!(extract(message), Err("malformed_header"));
    }

    #[test]
    fn empty_message_produces_an_empty_observation_list_not_a_failure() {
        let items = extract("").unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn observations_json_shape_is_well_formed_for_empty_input() {
        assert_eq!(observations_json(&[]), r#"{"kind":"observations","items":[]}"#);
    }
}
