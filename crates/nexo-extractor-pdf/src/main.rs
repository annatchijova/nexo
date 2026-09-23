//! PDF text extractor, built to run only inside the `docs/SANDBOX.md`
//! worker boundary: no host filesystem access beyond the fixed
//! input/output paths below, no network, no arguments or environment
//! trusted for anything.
//!
//! Contract: see `docs/EXTRACTOR_PDF_CONTRACT.md`. This is a **bounded
//! subset** of PDF, not a general renderer: it locates objects by a linear
//! scan (no xref/object-stream resolution), reads page content streams
//! (direct or `/FlateDecode`), and decodes `Tj`/`TJ`/`'`/`"` text-showing
//! operators under WinAnsiEncoding. A font using `/Subtype /Type0` or
//! `/Encoding /Identity-H` is a bounded failure, not a garbled best-effort
//! decode — see the contract's "Non-goals" for the full list of what this
//! does not attempt.
//!
//! Output is always one of exactly two typed shapes, written to
//! `/output/result.json`, same convention as every other extractor in this
//! workspace:
//!   {"kind":"observations","items":[{"locator":"page:1:text:1","text":"..."}]}
//!   {"kind":"failure","reason":"<bounded reason code>"}

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use flate2::read::ZlibDecoder;

const INPUT_PATH: &str = "/input/artifact";
const OUTPUT_PATH: &str = "/output/result.json";
const OUTPUT_TMP_PATH: &str = "/output/result.json.tmp";

/// Hard caps, independent of and in addition to the orchestrator's
/// container-level memory/CPU/time limits (docs/SANDBOX.md "defense in
/// depth"). Unlike the plain-text and email extractors, PDF genuinely can
/// amplify: `/FlateDecode` is real decompression, so a small input can
/// legitimately expand into a much larger output — this is the one
/// extractor in the workspace where an archive-bomb-style cap is not
/// merely theoretical.
const MAX_INPUT_BYTES: u64 = 25 * 1024 * 1024; // 25 MiB
const MAX_OBJECTS: usize = 100_000;
const MAX_PAGES: usize = 2_000;
/// Cap on one content stream's decoded size.
const MAX_STREAM_DECODED_BYTES: usize = 10 * 1024 * 1024; // 10 MiB
/// Cap on the sum of every stream's decoded size across the whole document
/// — defense in depth against many small streams each under the per-stream
/// cap but large in aggregate.
const MAX_TOTAL_DECODED_BYTES: usize = 50 * 1024 * 1024; // 50 MiB
const MAX_LINES: usize = 50_000;
const MAX_LINE_BYTES: usize = 100_000;

fn main() {
    match run() {
        Ok(()) => {}
        Err(message) => {
            eprintln!("nexo-extractor-pdf: {message}");
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

    match extract(&buffer) {
        Ok(items) => write_result(&observations_json(&items)),
        Err(reason) => write_result(&failure_json(reason)),
    }
}

#[derive(Debug, PartialEq)]
struct Item {
    locator: String,
    text: String,
}

// ---------------------------------------------------------------------
// Object scanning: a linear, non-recursive scan for `N G obj ... endobj`
// blocks. No xref table, incremental-update, or object-stream (PDF 1.5+
// compressed cross-reference) resolution — see the contract's Non-goals.
// The last occurrence of a given object number wins, which happens to be
// the correct behavior for simple incrementally-updated files too.
// ---------------------------------------------------------------------

fn find_objects(bytes: &[u8]) -> Result<HashMap<u32, &[u8]>, &'static str> {
    let mut objects = HashMap::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        let (num, mut j) = match parse_uint(bytes, i) {
            Some(v) => v,
            None => {
                i += 1;
                continue;
            }
        };
        j = skip_whitespace(bytes, j);
        let (_gen, j2) = match parse_uint(bytes, j) {
            Some(v) => v,
            None => {
                i = start + 1;
                continue;
            }
        };
        let j3 = skip_whitespace(bytes, j2);
        if !bytes[j3..].starts_with(b"obj") {
            i = start + 1;
            continue;
        }
        let body_start = j3 + 3;
        let Some(rel_end) = find_bytes(&bytes[body_start..], b"endobj") else {
            i = start + 1;
            continue;
        };
        let body_end = body_start + rel_end;
        objects.insert(num as u32, &bytes[body_start..body_end]);
        if objects.len() > MAX_OBJECTS {
            return Err("too_many_objects");
        }
        i = body_end + "endobj".len();
    }
    Ok(objects)
}

fn parse_uint(bytes: &[u8], start: usize) -> Option<(u64, usize)> {
    let mut i = start;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == start {
        return None;
    }
    std::str::from_utf8(&bytes[start..i])
        .ok()?
        .parse::<u64>()
        .ok()
        .map(|v| (v, i))
}

fn is_pdf_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n' | 0x0c | 0x00)
}

fn is_pdf_delimiter(byte: u8) -> bool {
    matches!(byte, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

fn skip_whitespace(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && is_pdf_whitespace(bytes[i]) {
        i += 1;
    }
    i
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

// ---------------------------------------------------------------------
// Small, targeted dictionary lookups over a raw object body. Deliberately
// not a general PDF object parser (no `Value` enum, no recursive-descent
// dictionary/array parser) — every helper below answers exactly one
// question this extractor needs, over the specific shapes real PDF
// writers produce for it, per the contract's "bounded subset" scope.
// ---------------------------------------------------------------------

/// Finds `/{key} /{value}` and returns `value`, e.g. `name_value(body,
/// "Type")` on `/Type /Page` returns `Some("Page")`. Only the first
/// occurrence of `key` is inspected.
fn name_value(body: &[u8], key: &str) -> Option<String> {
    let needle = format!("/{key}");
    let at = find_bytes(body, needle.as_bytes())?;
    let mut i = at + needle.len();
    // A key like "Type" must not match inside a longer name like
    // "TypeX" — the character right after it must be a delimiter or
    // whitespace.
    if i < body.len() && !is_pdf_whitespace(body[i]) && !is_pdf_delimiter(body[i]) {
        return None;
    }
    i = skip_whitespace(body, i);
    if i >= body.len() || body[i] != b'/' {
        return None;
    }
    read_name(body, i).map(|(name, _)| name)
}

fn read_name(body: &[u8], slash_at: usize) -> Option<(String, usize)> {
    let mut i = slash_at + 1;
    let mut out = String::new();
    while i < body.len() && !is_pdf_whitespace(body[i]) && !is_pdf_delimiter(body[i]) {
        if body[i] == b'#' && i + 2 < body.len() {
            let hi = hex_value(body[i + 1]);
            let lo = hex_value(body[i + 2]);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push(((hi << 4) | lo) as char);
                i += 3;
                continue;
            }
        }
        out.push(body[i] as char);
        i += 1;
    }
    Some((out, i))
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Finds `/{key} N G R` (or an array `[N G R ...]`) and returns every
/// referenced object number.
fn indirect_refs(body: &[u8], key: &str) -> Vec<u32> {
    let needle = format!("/{key}");
    let Some(at) = find_bytes(body, needle.as_bytes()) else { return Vec::new() };
    let mut i = skip_whitespace(body, at + needle.len());
    if i < body.len() && body[i] == b'[' {
        let mut refs = Vec::new();
        i += 1;
        loop {
            i = skip_whitespace(body, i);
            if i >= body.len() || body[i] == b']' {
                break;
            }
            let Some((num, next)) = parse_ref(body, i) else { break };
            refs.push(num);
            i = next;
        }
        refs
    } else if let Some((num, _)) = parse_ref(body, i) {
        vec![num]
    } else {
        Vec::new()
    }
}

/// Parses `N G R` starting at `start` (after leading whitespace already
/// skipped by the caller where relevant).
fn parse_ref(body: &[u8], start: usize) -> Option<(u32, usize)> {
    let (num, j) = parse_uint(body, start)?;
    let j = skip_whitespace(body, j);
    let (_gen, j2) = parse_uint(body, j)?;
    let j2 = skip_whitespace(body, j2);
    if body[j2..].starts_with(b"R") {
        Some((num as u32, j2 + 1))
    } else {
        None
    }
}

/// Returns the raw bytes of `/{key} <<...>>` — either the inline
/// dictionary right after the key, or (by following one indirect
/// reference) another object's body that itself is expected to be such a
/// dictionary.
fn dict_value<'a>(body: &'a [u8], key: &str, objects: &HashMap<u32, &'a [u8]>) -> Option<&'a [u8]> {
    let needle = format!("/{key}");
    let at = find_bytes(body, needle.as_bytes())?;
    let i = skip_whitespace(body, at + needle.len());
    if body[i..].starts_with(b"<<") {
        let end = matching_dict_end(&body[i..])?;
        Some(&body[i..i + end])
    } else {
        let (num, _) = parse_ref(body, i)?;
        objects.get(&num).copied()
    }
}

/// Given a byte slice starting with `<<`, returns the index one past the
/// matching `>>`, honoring nested `<<`/`>>` pairs.
fn matching_dict_end(body: &[u8]) -> Option<usize> {
    let mut depth = 0i32;
    let mut i = 0;
    while i + 1 < body.len() {
        if body[i] == b'<' && body[i + 1] == b'<' {
            depth += 1;
            i += 2;
        } else if body[i] == b'>' && body[i + 1] == b'>' {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return Some(i);
            }
        } else {
            i += 1;
        }
    }
    None
}

// ---------------------------------------------------------------------
// Content stream extraction: locate a page's `/Contents` stream(s),
// decode them, and tokenize the result for text-showing operators.
// ---------------------------------------------------------------------

fn stream_bytes<'a>(object_body: &'a [u8], objects: &HashMap<u32, &'a [u8]>) -> Option<&'a [u8]> {
    let stream_kw = find_bytes(object_body, b"stream")?;
    let mut i = stream_kw + "stream".len();
    // Exactly one EOL follows the `stream` keyword per spec (CRLF or LF).
    if object_body[i..].starts_with(b"\r\n") {
        i += 2;
    } else if object_body.get(i) == Some(&b'\n') {
        i += 1;
    }
    let length = resolve_length(object_body, objects)?;
    if i + length <= object_body.len() {
        Some(&object_body[i..i + length])
    } else {
        // A `/Length` that doesn't match reality (common in hand-edited or
        // slightly non-conformant files): fall back to scanning for the
        // literal `endstream` keyword instead of trusting the declared
        // length past the buffer's own end.
        let rel_end = find_bytes(&object_body[i..], b"endstream")?;
        Some(&object_body[i..i + rel_end])
    }
}

/// Resolves `/Length` (of a stream) to a byte count, whether it is a
/// direct integer (`/Length 123`) or an indirect reference
/// (`/Length 3 0 R`, common in real-world PDF writers, including
/// LibreOffice's own output — a naive "parse the leading digits" reader
/// would misread the `3` in the latter form as the length itself instead
/// of an object number, so the reference form is checked *first*.
fn resolve_length(body: &[u8], objects: &HashMap<u32, &[u8]>) -> Option<usize> {
    let needle = b"/Length";
    let at = find_bytes(body, needle)?;
    let i = skip_whitespace(body, at + needle.len());
    if let Some((num, _)) = parse_ref(body, i) {
        let referenced = objects.get(&num)?;
        return std::str::from_utf8(referenced).ok()?.trim().parse().ok();
    }
    let (n, _) = parse_uint(body, i)?;
    Some(n as usize)
}

fn decode_stream(
    raw: &[u8],
    object_body: &[u8],
    total_decoded: &mut usize,
) -> Result<Vec<u8>, &'static str> {
    let is_flate = name_value(object_body, "Filter")
        .map(|f| f == "FlateDecode")
        .unwrap_or(false)
        || find_bytes(object_body, b"/FlateDecode").is_some();
    let decoded = if is_flate {
        let mut decoder = ZlibDecoder::new(raw);
        let mut out = Vec::new();
        let mut limited = Read::by_ref(&mut decoder).take(MAX_STREAM_DECODED_BYTES as u64 + 1);
        limited.read_to_end(&mut out).map_err(|_| "malformed_stream")?;
        if out.len() > MAX_STREAM_DECODED_BYTES {
            return Err("stream_too_large");
        }
        out
    } else {
        if raw.len() > MAX_STREAM_DECODED_BYTES {
            return Err("stream_too_large");
        }
        raw.to_vec()
    };
    *total_decoded += decoded.len();
    if *total_decoded > MAX_TOTAL_DECODED_BYTES {
        return Err("stream_too_large");
    }
    Ok(decoded)
}

/// `true` if this font resource must not be decoded at all (not even via
/// its own `/ToUnicode` CMap, if it has one): a CID-keyed
/// (`/Subtype /Type0`) or `/Encoding /Identity-H` font uses multi-byte
/// character codes this extractor's one-byte-per-code assumption cannot
/// safely interpret, whether or not a ToUnicode CMap is attached. Decoding
/// its bytes one byte at a time anyway would silently produce wrong text,
/// which is worse than a bounded failure — see the contract's "Non-goals".
fn font_is_unsupported(font_body: &[u8]) -> bool {
    name_value(font_body, "Subtype").as_deref() == Some("Type0")
        || name_value(font_body, "Encoding").as_deref() == Some("Identity-H")
}

/// What's needed to decode one supported (non-Type0) font's text bytes.
struct FontInfo {
    unsupported: bool,
    /// Per-byte-code Unicode mapping from the font's own `/ToUnicode`
    /// CMap, when it has one. Real-world PDF writers very commonly embed
    /// a subsetted font whose internal glyph codes have no relationship
    /// to WinAnsiEncoding at all (LibreOffice's own PDF export does this),
    /// relying entirely on `/ToUnicode` for text to remain extractable —
    /// so this is checked before falling back to WinAnsiEncoding, not
    /// treated as an optional extra.
    to_unicode: HashMap<u8, String>,
}

/// Builds a map from font resource name (`"F1"`) to what's needed to
/// decode its text, for one page. Resources inherited from an ancestor
/// `Pages` node (rather than set on the page object itself) are not
/// resolved — a page relying on inherited resources is treated as having
/// no known fonts, so its text is decoded under the default (WinAnsi)
/// assumption.
fn page_fonts<'a>(
    page_body: &'a [u8],
    objects: &HashMap<u32, &'a [u8]>,
    total_decoded: &mut usize,
) -> Result<HashMap<String, FontInfo>, &'static str> {
    let mut fonts = HashMap::new();
    let Some(resources) = dict_value(page_body, "Resources", objects) else { return Ok(fonts) };
    let Some(font_dict) = dict_value(resources, "Font", objects) else { return Ok(fonts) };
    let mut i = 0;
    while i < font_dict.len() {
        if font_dict[i] == b'/' {
            let Some((name, next)) = read_name(font_dict, i) else { break };
            let j = skip_whitespace(font_dict, next);
            if let Some((obj_num, after_ref)) = parse_ref(font_dict, j) {
                if let Some(font_body) = objects.get(&obj_num) {
                    let unsupported = font_is_unsupported(font_body);
                    let to_unicode = if unsupported {
                        HashMap::new()
                    } else {
                        read_to_unicode_map(font_body, objects, total_decoded)?
                    };
                    fonts.insert(name, FontInfo { unsupported, to_unicode });
                }
                i = after_ref;
                continue;
            }
        }
        i += 1;
    }
    Ok(fonts)
}

/// Resolves and decodes a font's `/ToUnicode` stream, if it has one, into
/// a per-byte-code map. Absent, unreadable, or unparseable ToUnicode data
/// is not an error for the font as a whole — it just means this font falls
/// back to WinAnsiEncoding, same as a font with no `/ToUnicode` entry at
/// all.
fn read_to_unicode_map(
    font_body: &[u8],
    objects: &HashMap<u32, &[u8]>,
    total_decoded: &mut usize,
) -> Result<HashMap<u8, String>, &'static str> {
    let needle = b"/ToUnicode";
    let Some(at) = find_bytes(font_body, needle) else { return Ok(HashMap::new()) };
    let i = skip_whitespace(font_body, at + needle.len());
    let Some((obj_num, _)) = parse_ref(font_body, i) else { return Ok(HashMap::new()) };
    let Some(cmap_object) = objects.get(&obj_num) else { return Ok(HashMap::new()) };
    let Some(raw_stream) = stream_bytes(cmap_object, objects) else { return Ok(HashMap::new()) };
    let decoded = decode_stream(raw_stream, cmap_object, total_decoded)?;
    Ok(parse_to_unicode_cmap(&decoded))
}

/// Parses the `beginbfchar`/`endbfchar` and `beginbfrange`/`endbfrange`
/// sections of a ToUnicode CMap (a small PostScript-like language, per PDF
/// spec Annex D). Only single-byte source codes are handled — this
/// extractor only ever calls this for non-Type0 fonts, whose codes are one
/// byte each, so a wider source code in the CMap is simply skipped rather
/// than misread. Any entry this parser cannot make sense of is skipped;
/// this never fails the whole document, since a partially-parsed CMap
/// (source code -> WinAnsi fallback for the rest) is still strictly better
/// than discarding the page.
fn parse_to_unicode_cmap(bytes: &[u8]) -> HashMap<u8, String> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mode {
        None,
        BfChar,
        BfRange,
    }

    let mut map = HashMap::new();
    let mut lexer = Lexer::new(bytes);
    let mut mode = Mode::None;
    let mut pending: Vec<Vec<u8>> = Vec::new();

    while let Some(token) = lexer.next_token() {
        match token {
            Token::Keyword(kw) => {
                match kw.as_str() {
                    "beginbfchar" => mode = Mode::BfChar,
                    "beginbfrange" => mode = Mode::BfRange,
                    "endbfchar" | "endbfrange" => mode = Mode::None,
                    _ => {}
                }
                pending.clear();
            }
            Token::HexString(hex) => {
                pending.push(hex);
                match mode {
                    Mode::BfChar if pending.len() == 2 => {
                        let dst = pending.pop().unwrap();
                        let src = pending.pop().unwrap();
                        if src.len() == 1 {
                            map.insert(src[0], utf16be_to_string(&dst));
                        }
                    }
                    Mode::BfRange if pending.len() == 3 => {
                        let start = pending.pop().unwrap();
                        let hi = pending.pop().unwrap();
                        let lo = pending.pop().unwrap();
                        if lo.len() == 1 && hi.len() == 1 && start.len() == 2 {
                            let lo = lo[0];
                            let hi = hi[0];
                            let start_value = u16::from_be_bytes([start[0], start[1]]);
                            for code in lo..=hi {
                                let value = start_value.wrapping_add((code - lo) as u16);
                                map.insert(code, utf16be_to_string(&value.to_be_bytes()));
                            }
                        }
                    }
                    _ => {}
                }
            }
            Token::ArrayStart => {
                // The `<lo> <hi> [ <d0> <d1> ... ]` bfrange form (each
                // code in the range mapped individually, rather than to a
                // consecutive run starting at one value) is not parsed —
                // skip the array so the lexer stays in sync, leaving those
                // specific codes to fall back to WinAnsiEncoding.
                let mut depth = 1;
                while depth > 0 {
                    match lexer.next_token() {
                        Some(Token::ArrayStart) => depth += 1,
                        Some(Token::ArrayEnd) => depth -= 1,
                        Some(_) => {}
                        None => break,
                    }
                }
                pending.clear();
            }
            _ => {}
        }
    }
    map
}

/// Decodes a ToUnicode `<dst>` value: normally 2 bytes (one UTF-16BE code
/// unit); a longer value (a ligature mapping to multiple characters) is
/// decoded as consecutive UTF-16BE code units. A lone unpaired surrogate,
/// which real fonts do not produce here, decodes to the Unicode
/// replacement character rather than panicking.
fn utf16be_to_string(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes.chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
    char::decode_utf16(units).map(|r| r.unwrap_or('\u{FFFD}')).collect()
}

#[derive(Debug, PartialEq)]
enum Token {
    LiteralString(Vec<u8>),
    HexString(Vec<u8>),
    Name(String),
    Number,
    ArrayStart,
    ArrayEnd,
    Keyword(String),
}

struct Lexer<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn next_token(&mut self) -> Option<Token> {
        loop {
            self.pos = skip_whitespace(self.bytes, self.pos);
            if self.pos >= self.bytes.len() {
                return None;
            }
            let byte = self.bytes[self.pos];
            if byte == b'%' {
                while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\n' {
                    self.pos += 1;
                }
                continue;
            }
            break;
        }
        let byte = self.bytes[self.pos];
        match byte {
            b'(' => self.lex_literal_string(),
            b'<' if self.bytes.get(self.pos + 1) == Some(&b'<') => {
                // Inline dictionary (e.g. inline-image parameters): skip
                // it balanced rather than parsing it, per the contract's
                // non-goal on inline images.
                let end = matching_dict_end(&self.bytes[self.pos..])?;
                self.pos += end;
                self.next_token()
            }
            b'<' => self.lex_hex_string(),
            b'/' => self.lex_name(),
            b'[' => {
                self.pos += 1;
                Some(Token::ArrayStart)
            }
            b']' => {
                self.pos += 1;
                Some(Token::ArrayEnd)
            }
            b'0'..=b'9' | b'+' | b'-' | b'.' => self.lex_number(),
            _ => self.lex_keyword(),
        }
    }

    fn lex_literal_string(&mut self) -> Option<Token> {
        self.pos += 1; // consume '('
        let mut depth = 1;
        let mut out = Vec::new();
        while self.pos < self.bytes.len() && depth > 0 {
            let b = self.bytes[self.pos];
            match b {
                b'\\' if self.pos + 1 < self.bytes.len() => {
                    let esc = self.bytes[self.pos + 1];
                    match esc {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0c),
                        b'(' | b')' | b'\\' => out.push(esc),
                        b'\n' => {} // line continuation: dropped
                        b'\r' => {
                            // \ followed by CRLF or lone CR: also a line
                            // continuation.
                            if self.bytes.get(self.pos + 2) == Some(&b'\n') {
                                self.pos += 1;
                            }
                        }
                        b'0'..=b'7' => {
                            let mut value = 0u32;
                            let mut n = 0;
                            let mut k = self.pos + 1;
                            while n < 3 && k < self.bytes.len() && (b'0'..=b'7').contains(&self.bytes[k]) {
                                value = value * 8 + (self.bytes[k] - b'0') as u32;
                                k += 1;
                                n += 1;
                            }
                            out.push(value as u8);
                            self.pos = k - 1;
                        }
                        other => out.push(other),
                    }
                    self.pos += 2;
                }
                b'(' => {
                    depth += 1;
                    out.push(b);
                    self.pos += 1;
                }
                b')' => {
                    depth -= 1;
                    if depth > 0 {
                        out.push(b);
                    }
                    self.pos += 1;
                }
                _ => {
                    out.push(b);
                    self.pos += 1;
                }
            }
        }
        Some(Token::LiteralString(out))
    }

    fn lex_hex_string(&mut self) -> Option<Token> {
        self.pos += 1; // consume '<'
        let mut nibbles = Vec::new();
        while self.pos < self.bytes.len() && self.bytes[self.pos] != b'>' {
            if let Some(v) = hex_value(self.bytes[self.pos]) {
                nibbles.push(v);
            }
            self.pos += 1;
        }
        if self.pos < self.bytes.len() {
            self.pos += 1; // consume '>'
        }
        if nibbles.len() % 2 != 0 {
            nibbles.push(0);
        }
        let bytes = nibbles.chunks(2).map(|pair| (pair[0] << 4) | pair[1]).collect();
        Some(Token::HexString(bytes))
    }

    fn lex_name(&mut self) -> Option<Token> {
        let (name, next) = read_name(self.bytes, self.pos)?;
        self.pos = next;
        Some(Token::Name(name))
    }

    fn lex_number(&mut self) -> Option<Token> {
        while self.pos < self.bytes.len()
            && (self.bytes[self.pos].is_ascii_digit()
                || matches!(self.bytes[self.pos], b'+' | b'-' | b'.'))
        {
            self.pos += 1;
        }
        Some(Token::Number)
    }

    fn lex_keyword(&mut self) -> Option<Token> {
        let start = self.pos;
        while self.pos < self.bytes.len()
            && !is_pdf_whitespace(self.bytes[self.pos])
            && !is_pdf_delimiter(self.bytes[self.pos])
        {
            self.pos += 1;
        }
        if self.pos == start {
            // A stray delimiter byte we don't otherwise handle (e.g. `{`,
            // `}`, a lone `>`): skip it rather than looping forever.
            self.pos += 1;
        }
        let keyword = String::from_utf8_lossy(&self.bytes[start..self.pos]).into_owned();
        if keyword == "BI" {
            // Inline image: skip to the matching `EI` keyword rather than
            // tokenizing its raw binary data, per the contract's
            // non-goals.
            if let Some(rel) = find_bytes(&self.bytes[self.pos..], b"EI") {
                self.pos += rel + 2;
            } else {
                self.pos = self.bytes.len();
            }
            return self.next_token();
        }
        Some(Token::Keyword(keyword))
    }
}

fn extract_page_text(
    content: &[u8],
    fonts: &HashMap<String, FontInfo>,
    page_index: usize,
    items: &mut Vec<Item>,
) -> Result<(), &'static str> {
    let mut lexer = Lexer::new(content);
    let mut last_string: Option<Vec<u8>> = None;
    let mut array_strings: Vec<Vec<u8>> = Vec::new();
    let mut in_array = false;
    let mut last_name: Option<String> = None;
    let mut current_font: Option<String> = None;
    let mut text_index = 0usize;

    let emit = |bytes: Vec<u8>,
                     current_font: &Option<String>,
                     items: &mut Vec<Item>,
                     text_index: &mut usize|
     -> Result<(), &'static str> {
        let font_info = current_font.as_ref().and_then(|name| fonts.get(name));
        if font_info.map(|info| info.unsupported).unwrap_or(false) {
            return Err("unsupported_font_encoding");
        }
        let decoded = decode_text(&bytes, font_info);
        if decoded.trim().is_empty() {
            return Ok(());
        }
        if decoded.len() > MAX_LINE_BYTES {
            return Err("line_too_long");
        }
        *text_index += 1;
        if items.len() >= MAX_LINES {
            return Err("too_many_lines");
        }
        items.push(Item {
            locator: format!("page:{}:text:{}", page_index + 1, *text_index),
            text: decoded,
        });
        Ok(())
    };

    while let Some(token) = lexer.next_token() {
        match token {
            Token::ArrayStart => {
                in_array = true;
                array_strings.clear();
            }
            Token::ArrayEnd => {
                in_array = false;
            }
            Token::LiteralString(bytes) | Token::HexString(bytes) => {
                if in_array {
                    array_strings.push(bytes);
                } else {
                    last_string = Some(bytes);
                }
            }
            Token::Name(name) => {
                last_name = Some(name);
            }
            Token::Number => {}
            Token::Keyword(kw) => match kw.as_str() {
                "Tf" => {
                    if let Some(name) = last_name.take() {
                        current_font = Some(name);
                    }
                }
                "Tj" | "'" | "\"" => {
                    if let Some(s) = last_string.take() {
                        emit(s, &current_font, items, &mut text_index)?;
                    }
                }
                "TJ" if !array_strings.is_empty() => {
                    let combined = array_strings.concat();
                    array_strings.clear();
                    emit(combined, &current_font, items, &mut text_index)?;
                }
                _ => {}
            },
        }
    }
    Ok(())
}

/// Decodes one text-showing operator's byte string, one byte (one simple-font
/// character code) at a time: a code found in the font's own `/ToUnicode`
/// map decodes to exactly what that map says (this is the common case for
/// a subsetted embedded font, whose codes otherwise have no relationship
/// to any standard encoding at all); any other code falls back to
/// WinAnsiEncoding.
fn decode_text(bytes: &[u8], font: Option<&FontInfo>) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        if let Some(mapped) = font.and_then(|f| f.to_unicode.get(&b)) {
            out.push_str(mapped);
        } else {
            out.push(winansi_char(b));
        }
    }
    out
}

/// WinAnsiEncoding (Windows-1252) for the 0x80-0x9F range where it departs
/// from Latin-1; 0xA0-0xFF match Latin-1/Unicode byte-for-byte, and
/// 0x00-0x7F are passed through as ASCII/control bytes unchanged.
fn winansi_char(b: u8) -> char {
    match b {
        0x80 => '\u{20AC}',
        0x82 => '\u{201A}',
        0x83 => '\u{0192}',
        0x84 => '\u{201E}',
        0x85 => '\u{2026}',
        0x86 => '\u{2020}',
        0x87 => '\u{2021}',
        0x88 => '\u{02C6}',
        0x89 => '\u{2030}',
        0x8A => '\u{0160}',
        0x8B => '\u{2039}',
        0x8C => '\u{0152}',
        0x8E => '\u{017D}',
        0x91 => '\u{2018}',
        0x92 => '\u{2019}',
        0x93 => '\u{201C}',
        0x94 => '\u{201D}',
        0x95 => '\u{2022}',
        0x96 => '\u{2013}',
        0x97 => '\u{2014}',
        0x98 => '\u{02DC}',
        0x99 => '\u{2122}',
        0x9A => '\u{0161}',
        0x9B => '\u{203A}',
        0x9C => '\u{0153}',
        0x9E => '\u{017E}',
        0x9F => '\u{0178}',
        other => other as char,
    }
}

fn extract(bytes: &[u8]) -> Result<Vec<Item>, &'static str> {
    let objects = find_objects(bytes)?;

    let mut page_numbers: Vec<u32> = objects
        .iter()
        .filter(|(_, body)| name_value(body, "Type").as_deref() == Some("Page"))
        .map(|(num, _)| *num)
        .collect();
    page_numbers.sort_unstable();
    if page_numbers.is_empty() {
        return Err("no_pages_found");
    }
    if page_numbers.len() > MAX_PAGES {
        return Err("too_many_pages");
    }

    let mut items = Vec::new();
    let mut total_decoded = 0usize;
    for (page_index, page_num) in page_numbers.iter().enumerate() {
        let page_body = objects[page_num];
        let fonts = page_fonts(page_body, &objects, &mut total_decoded)?;
        let content_refs = indirect_refs(page_body, "Contents");
        let mut content = Vec::new();
        for content_num in content_refs {
            let Some(content_object) = objects.get(&content_num) else { continue };
            let Some(raw_stream) = stream_bytes(content_object, &objects) else { continue };
            let decoded = decode_stream(raw_stream, content_object, &mut total_decoded)?;
            content.extend_from_slice(&decoded);
            content.push(b'\n');
        }
        extract_page_text(&content, &fonts, page_index, &mut items)?;
    }
    Ok(items)
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

    fn flate_compress(data: &[u8]) -> Vec<u8> {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    /// Builds a minimal single-page PDF with one content stream showing
    /// `text` via a single `Tj` operator, uncompressed. Object numbers:
    /// 1 = Catalog (unused by this extractor), 2 = Pages (unused), 3 =
    /// Page, 4 = content stream, 5 = font.
    fn minimal_pdf(text_operator_body: &str) -> Vec<u8> {
        let content = format!("BT /F1 12 Tf {text_operator_body} ET");
        format!(
            "%PDF-1.4\n\
             3 0 obj\n<< /Type /Page /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>\nendobj\n\
             4 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n\
             5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
            content.len(),
            content
        )
        .into_bytes()
    }

    #[test]
    fn simple_tj_text_is_extracted() {
        let pdf = minimal_pdf("(Hola mundo) Tj");
        let items = extract(&pdf).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].locator, "page:1:text:1");
        assert_eq!(items[0].text, "Hola mundo");
    }

    #[test]
    fn tj_array_concatenates_string_pieces_and_ignores_kerning_numbers() {
        let pdf = minimal_pdf("[(Hola) -250 (mundo)] TJ");
        let items = extract(&pdf).unwrap();
        assert_eq!(items[0].text, "Holamundo");
    }

    #[test]
    fn escaped_parens_and_backslash_in_literal_string_are_decoded() {
        let pdf = minimal_pdf(r"(a \(b\) c \\ d) Tj");
        let items = extract(&pdf).unwrap();
        assert_eq!(items[0].text, "a (b) c \\ d");
    }

    #[test]
    fn hex_string_operand_is_decoded() {
        // "Hi" as hex.
        let pdf = minimal_pdf("<4869> Tj");
        let items = extract(&pdf).unwrap();
        assert_eq!(items[0].text, "Hi");
    }

    #[test]
    fn accented_latin1_byte_in_literal_string_decodes_as_the_matching_unicode_char() {
        // 0xE9 is 'é' under both WinAnsiEncoding and Latin-1. Built as raw
        // bytes rather than through minimal_pdf's `&str` parameter since a
        // Rust `str` literal cannot contain a lone non-ASCII byte.
        let mut content = b"BT /F1 12 Tf (Caf".to_vec();
        content.push(0xe9);
        content.extend_from_slice(b") Tj ET");
        let mut pdf = format!(
            "%PDF-1.4\n\
             3 0 obj\n<< /Type /Page /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>\nendobj\n\
             4 0 obj\n<< /Length {} >>\nstream\n",
            content.len()
        )
        .into_bytes();
        pdf.extend_from_slice(&content);
        pdf.extend_from_slice(b"\nendstream\nendobj\n5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n");
        let items = extract(&pdf).unwrap();
        assert_eq!(items[0].text, "Café");
    }

    #[test]
    fn flate_decode_content_stream_is_inflated_before_tokenizing() {
        let content = b"BT /F1 12 Tf (Comprimido) Tj ET";
        let compressed = flate_compress(content);
        let pdf = format!(
            "%PDF-1.4\n\
             3 0 obj\n<< /Type /Page /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>\nendobj\n\
             4 0 obj\n<< /Length {} /Filter /FlateDecode >>\nstream\n",
            compressed.len()
        )
        .into_bytes();
        let mut pdf = pdf;
        pdf.extend_from_slice(&compressed);
        pdf.extend_from_slice(b"\nendstream\nendobj\n5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n");
        let items = extract(&pdf).unwrap();
        assert_eq!(items[0].text, "Comprimido");
    }

    #[test]
    fn to_unicode_cmap_decodes_codes_that_have_no_relationship_to_winansi() {
        // Codes 0x01/0x02 mean nothing under WinAnsiEncoding (they're C0
        // control codes) — this is exactly the shape a subsetted embedded
        // font uses in practice (LibreOffice's own PDF export does this):
        // the font's own /ToUnicode CMap is the only way to recover "Hi".
        let content = "BT /F1 12 Tf <0102> Tj ET";
        let cmap = "/CIDInit /ProcSet findresource begin\n\
                     12 dict begin\nbegincmap\n1 begincodespacerange\n<00> <FF>\nendcodespacerange\n\
                     2 beginbfchar\n<01> <0048>\n<02> <0069>\nendbfchar\nendcmap\nend end\n";
        let pdf = format!(
            "%PDF-1.4\n\
             3 0 obj\n<< /Type /Page /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>\nendobj\n\
             4 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n\
             5 0 obj\n<< /Type /Font /Subtype /TrueType /BaseFont /Subset+Foo /ToUnicode 6 0 R >>\nendobj\n\
             6 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n",
            content.len(),
            content,
            cmap.len(),
            cmap,
        );
        let items = extract(pdf.as_bytes()).unwrap();
        assert_eq!(items[0].text, "Hi");
    }

    #[test]
    fn to_unicode_bfrange_maps_a_consecutive_run_of_codes() {
        let content = "BT /F1 12 Tf <0203> Tj ET"; // codes 2, 3 -> 'B', 'C'
        let cmap = "1 begincodespacerange\n<00> <FF>\nendcodespacerange\n\
                     1 beginbfrange\n<01> <05> <0041>\nendbfrange\n"; // 1..5 -> A..E
        let pdf = format!(
            "%PDF-1.4\n\
             3 0 obj\n<< /Type /Page /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>\nendobj\n\
             4 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n\
             5 0 obj\n<< /Type /Font /Subtype /TrueType /BaseFont /Subset+Foo /ToUnicode 6 0 R >>\nendobj\n\
             6 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n",
            content.len(),
            content,
            cmap.len(),
            cmap,
        );
        let items = extract(pdf.as_bytes()).unwrap();
        assert_eq!(items[0].text, "BC");
    }

    #[test]
    fn type0_identity_h_font_is_a_bounded_failure_not_a_garbled_decode() {
        let content = "BT /F1 12 Tf (AB) Tj ET";
        let pdf = format!(
            "%PDF-1.4\n\
             3 0 obj\n<< /Type /Page /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>\nendobj\n\
             4 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n\
             5 0 obj\n<< /Type /Font /Subtype /Type0 /Encoding /Identity-H >>\nendobj\n",
            content.len(),
            content
        );
        assert_eq!(extract(pdf.as_bytes()), Err("unsupported_font_encoding"));
    }

    #[test]
    fn document_with_no_page_objects_is_a_bounded_failure() {
        let pdf = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog >>\nendobj\n";
        assert_eq!(extract(pdf), Err("no_pages_found"));
    }

    #[test]
    fn empty_input_is_a_bounded_failure_not_a_crash() {
        assert_eq!(extract(b""), Err("no_pages_found"));
    }

    #[test]
    fn oversized_decompressed_stream_is_a_bounded_failure() {
        let content = vec![b'a'; MAX_STREAM_DECODED_BYTES + 1];
        let compressed = flate_compress(&content);
        let mut pdf = format!(
            "%PDF-1.4\n\
             3 0 obj\n<< /Type /Page /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>\nendobj\n\
             4 0 obj\n<< /Length {} /Filter /FlateDecode >>\nstream\n",
            compressed.len()
        )
        .into_bytes();
        pdf.extend_from_slice(&compressed);
        pdf.extend_from_slice(b"\nendstream\nendobj\n5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n");
        assert_eq!(extract(&pdf), Err("stream_too_large"));
    }

    #[test]
    fn many_streams_each_under_the_per_stream_cap_still_hit_the_total_cap() {
        // Each page's own decoded stream sits right at the per-stream cap
        // (allowed on its own); enough of them must still be rejected once
        // their sum crosses MAX_TOTAL_DECODED_BYTES — the defense-in-depth
        // cap against many small-looking streams that are each individually
        // "fine" but bomb the document in aggregate.
        let per_stream = vec![b'a'; MAX_STREAM_DECODED_BYTES];
        let compressed = flate_compress(&per_stream);
        let page_count = MAX_TOTAL_DECODED_BYTES / MAX_STREAM_DECODED_BYTES + 1;

        let mut pdf = b"%PDF-1.4\n".to_vec();
        for i in 0..page_count {
            let page_obj = 3 + i * 2;
            let content_obj = page_obj + 1;
            pdf.extend_from_slice(
                format!(
                    "{page_obj} 0 obj\n<< /Type /Page /Contents {content_obj} 0 R >>\nendobj\n"
                )
                .as_bytes(),
            );
            pdf.extend_from_slice(
                format!(
                    "{content_obj} 0 obj\n<< /Length {} /Filter /FlateDecode >>\nstream\n",
                    compressed.len()
                )
                .as_bytes(),
            );
            pdf.extend_from_slice(&compressed);
            pdf.extend_from_slice(b"\nendstream\nendobj\n");
        }
        assert_eq!(extract(&pdf), Err("stream_too_large"));
    }

    #[test]
    fn multiple_content_streams_for_one_page_are_concatenated() {
        let part_one = "BT /F1 12 Tf (Uno) Tj";
        let part_two = "(Dos) Tj";
        let pdf = format!(
            "%PDF-1.4\n\
             3 0 obj\n<< /Type /Page /Resources << /Font << /F1 6 0 R >> >> /Contents [4 0 R 5 0 R] >>\nendobj\n\
             4 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n\
             5 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n\
             6 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
            part_one.len(),
            part_one,
            part_two.len(),
            part_two,
        );
        let items = extract(pdf.as_bytes()).unwrap();
        let texts: Vec<&str> = items.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(texts, vec!["Uno", "Dos"]);
    }

    #[test]
    fn observations_json_shape_is_well_formed_for_empty_items() {
        assert_eq!(observations_json(&[]), r#"{"kind":"observations","items":[]}"#);
    }
}
