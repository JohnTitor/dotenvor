use std::borrow::Cow;
use std::collections::HashMap;
use std::io::BufRead;
use std::path::Path;

use crate::error::{Error, ParseError, ParseErrorKind};
use crate::model::{Entry, KeyParsingMode};

/// Parse dotenv entries from UTF-8 text.
pub fn parse_str(input: &str) -> Result<Vec<Entry>, Error> {
    parse_str_with_mode(input, KeyParsingMode::Strict)
}

/// Parse dotenv entries from UTF-8 text using a specific key parsing mode.
pub fn parse_str_with_mode(
    input: &str,
    key_parsing_mode: KeyParsingMode,
) -> Result<Vec<Entry>, Error> {
    parse_str_with_source(input, None, key_parsing_mode, false).map_err(Error::from)
}

/// Parse dotenv entries from UTF-8 bytes.
pub fn parse_bytes(input: &[u8]) -> Result<Vec<Entry>, Error> {
    parse_bytes_with_mode(input, KeyParsingMode::Strict)
}

/// Parse dotenv entries from UTF-8 bytes using a specific key parsing mode.
pub fn parse_bytes_with_mode(
    input: &[u8],
    key_parsing_mode: KeyParsingMode,
) -> Result<Vec<Entry>, Error> {
    let text = std::str::from_utf8(input)?;
    parse_str_with_mode(text, key_parsing_mode)
}

/// Parse dotenv entries from a buffered reader.
pub fn parse_reader<R: BufRead>(reader: R) -> Result<Vec<Entry>, Error> {
    parse_reader_with_mode(reader, KeyParsingMode::Strict)
}

/// Parse dotenv entries from a buffered reader using a specific key parsing mode.
pub fn parse_reader_with_mode<R: BufRead>(
    mut reader: R,
    key_parsing_mode: KeyParsingMode,
) -> Result<Vec<Entry>, Error> {
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf)?;
    parse_bytes_with_mode(&buf, key_parsing_mode)
}

pub(crate) fn parse_str_with_source(
    input: &str,
    source: Option<&Path>,
    key_parsing_mode: KeyParsingMode,
    preserve_literal_dollar_escapes: bool,
) -> Result<Vec<Entry>, ParseError> {
    let normalized = normalize_newlines(input);
    let input = normalized.as_ref();

    let mut entries = Vec::new();
    let mut by_key = HashMap::<String, usize>::new();

    let mut offset = 0usize;
    let mut line_num = 1u32;
    let bytes = input.as_bytes();

    while offset < bytes.len() {
        let statement_start = offset;
        let statement_line = line_num;
        let mut idx = offset;
        let mut newline_count = 0u32;
        let mut active_quote: Option<u8> = None;
        let mut value_started = false;

        while idx < bytes.len() {
            let byte = bytes[idx];

            if byte == b'\n' {
                newline_count += 1;
                if active_quote.is_none() {
                    break;
                }
            } else if let Some(quote) = active_quote {
                if byte == quote && !is_preceded_by_odd_backslashes(bytes, idx) {
                    active_quote = None;
                }
            } else if !value_started && byte == b'=' {
                value_started = true;
            } else if value_started && (byte == b'"' || byte == b'\'' || byte == b'`') {
                active_quote = Some(byte);
            }
            idx += 1;
        }

        let statement = &input[statement_start..idx];
        let parsed = parse_line(
            statement,
            statement_line,
            source,
            key_parsing_mode,
            preserve_literal_dollar_escapes,
        )?;
        let Some(entry) = parsed else {
            if idx < bytes.len() && bytes[idx] == b'\n' {
                idx += 1;
            }
            line_num += newline_count;
            offset = idx;
            continue;
        };

        if let Some(existing_idx) = by_key.get(&entry.key).copied() {
            entries[existing_idx] = entry;
        } else {
            by_key.insert(entry.key.clone(), entries.len());
            entries.push(entry);
        }

        if idx < bytes.len() && bytes[idx] == b'\n' {
            idx += 1;
        }
        line_num += newline_count;
        offset = idx;
    }

    Ok(entries)
}

fn normalize_newlines(input: &str) -> Cow<'_, str> {
    if !input.contains('\r') {
        return Cow::Borrowed(input);
    }

    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            out.push('\n');
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            continue;
        }
        out.push(ch);
    }

    Cow::Owned(out)
}

fn is_preceded_by_odd_backslashes(bytes: &[u8], idx: usize) -> bool {
    if idx == 0 {
        return false;
    }

    let mut cursor = idx;
    let mut backslash_count = 0usize;
    while cursor > 0 && bytes[cursor - 1] == b'\\' {
        cursor -= 1;
        backslash_count += 1;
    }

    backslash_count % 2 == 1
}

fn parse_line(
    line: &str,
    line_num: u32,
    source: Option<&Path>,
    key_parsing_mode: KeyParsingMode,
    preserve_literal_dollar_escapes: bool,
) -> Result<Option<Entry>, ParseError> {
    let mut working = line.trim_start();
    if working.is_empty() || working.starts_with('#') {
        return Ok(None);
    }

    if let Some(rest) = working.strip_prefix("export")
        && rest
            .chars()
            .next()
            .map(|ch| ch.is_whitespace())
            .unwrap_or(false)
    {
        working = rest.trim_start();
    }

    if working.is_empty() {
        return Err(ParseError::new(line_num, 1, ParseErrorKind::MissingKey));
    }

    let Some(eq_idx) = working.find('=') else {
        let column = working.chars().count() as u32 + 1;
        return Err(ParseError::new(
            line_num,
            column,
            ParseErrorKind::InvalidSyntax,
        ));
    };

    let key = working[..eq_idx].trim_end();
    if key.is_empty() {
        return Err(ParseError::new(line_num, 1, ParseErrorKind::MissingKey));
    }
    if !is_valid_key(key, key_parsing_mode) {
        return Err(ParseError::new(line_num, 1, ParseErrorKind::InvalidKey));
    }

    let value_input = working[eq_idx + 1..].trim_start();
    let value_column = (line.len() - value_input.len()) as u32 + 1;
    let value = parse_value(
        value_input,
        line_num,
        value_column,
        preserve_literal_dollar_escapes,
    )?;

    Ok(Some(Entry {
        key: key.to_owned(),
        value,
        source: source.map(Path::to_path_buf),
        line: line_num,
    }))
}

fn parse_value(
    input: &str,
    line_num: u32,
    column: u32,
    preserve_literal_dollar_escapes: bool,
) -> Result<String, ParseError> {
    if input.is_empty() {
        return Ok(String::new());
    }

    if input.starts_with('\'') {
        return parse_single_quoted(input, line_num, column, preserve_literal_dollar_escapes);
    }
    if input.starts_with('"') {
        return parse_double_quoted(input, line_num, column, preserve_literal_dollar_escapes);
    }
    if input.starts_with('`') {
        return parse_backtick_quoted(input, line_num, column);
    }

    let value = input
        .split_once('#')
        .map(|(head, _)| head)
        .unwrap_or(input)
        .trim_end();
    Ok(value.to_owned())
}

fn parse_single_quoted(
    input: &str,
    line_num: u32,
    column: u32,
    preserve_literal_dollar_escapes: bool,
) -> Result<String, ParseError> {
    let parsed = parse_literal_quoted(input, '\'', line_num, column)?;
    if !preserve_literal_dollar_escapes {
        return Ok(parsed);
    }
    Ok(escape_dollar_signs(&parsed))
}

fn parse_backtick_quoted(input: &str, line_num: u32, column: u32) -> Result<String, ParseError> {
    parse_literal_quoted(input, '`', line_num, column)
}

fn parse_literal_quoted(
    input: &str,
    quote: char,
    line_num: u32,
    column: u32,
) -> Result<String, ParseError> {
    let mut closing_idx = None;
    for (idx, ch) in input.char_indices().skip(1) {
        if ch == quote {
            if is_preceded_by_odd_backslashes(input.as_bytes(), idx) {
                continue;
            }
            closing_idx = Some(idx);
            break;
        }
    }

    let Some(end_idx) = closing_idx else {
        return Err(ParseError::new(
            line_num,
            column,
            ParseErrorKind::UnterminatedQuote,
        ));
    };

    let tail = input[end_idx + 1..].trim_start();
    if !tail.is_empty() && !tail.starts_with('#') {
        return Err(ParseError::new(
            line_num,
            column + end_idx as u32 + 1,
            ParseErrorKind::InvalidSyntax,
        ));
    }

    Ok(input[1..end_idx].to_owned())
}

fn parse_double_quoted(
    input: &str,
    line_num: u32,
    column: u32,
    preserve_literal_dollar_escapes: bool,
) -> Result<String, ParseError> {
    let mut out = String::with_capacity(input.len().saturating_sub(2));
    let mut escaped = false;
    let mut closing_idx = None;

    for (idx, ch) in input.char_indices().skip(1) {
        if escaped {
            let unescaped = match ch {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '\\' => '\\',
                '"' => '"',
                '$' if preserve_literal_dollar_escapes => {
                    out.push('\\');
                    '$'
                }
                _ => ch,
            };
            out.push(unescaped);
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '"' => {
                closing_idx = Some(idx);
                break;
            }
            _ => out.push(ch),
        }
    }

    let Some(end_idx) = closing_idx else {
        return Err(ParseError::new(
            line_num,
            column,
            ParseErrorKind::UnterminatedQuote,
        ));
    };

    let tail = input[end_idx + 1..].trim_start();
    if !tail.is_empty() && !tail.starts_with('#') {
        return Err(ParseError::new(
            line_num,
            column + end_idx as u32 + 1,
            ParseErrorKind::InvalidSyntax,
        ));
    }

    Ok(out)
}

fn escape_dollar_signs(value: &str) -> String {
    if !value.contains('$') {
        return value.to_owned();
    }

    let mut out =
        String::with_capacity(value.len() + value.bytes().filter(|byte| *byte == b'$').count());
    for ch in value.chars() {
        if ch == '$' {
            out.push('\\');
        }
        out.push(ch);
    }

    out
}

fn is_valid_key(key: &str, key_parsing_mode: KeyParsingMode) -> bool {
    match key_parsing_mode {
        KeyParsingMode::Strict => key.chars().all(is_valid_strict_key_char),
        KeyParsingMode::Permissive => key.chars().all(is_valid_permissive_key_char),
    }
}

fn is_valid_strict_key_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' || ch == '-'
}

fn is_valid_permissive_key_char(ch: char) -> bool {
    ch.is_ascii() && ('!'..='~').contains(&ch) && ch != '='
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_values_and_comments() {
        let input = "A=1\nB = 2\n# skip\nC=hello # comment\nD=\n";
        let parsed = parse_str(input).expect("parse should succeed");

        assert_eq!(parsed.len(), 4);
        assert_eq!(parsed[0].key, "A");
        assert_eq!(parsed[0].value, "1");
        assert_eq!(parsed[1].key, "B");
        assert_eq!(parsed[1].value, "2");
        assert_eq!(parsed[2].key, "C");
        assert_eq!(parsed[2].value, "hello");
        assert_eq!(parsed[3].key, "D");
        assert_eq!(parsed[3].value, "");
    }

    #[test]
    fn parses_export_and_quotes() {
        let input = "export QUOTED=\"line\\nvalue\"\nSINGLE='raw value'\n";
        let parsed = parse_str(input).expect("parse should succeed");

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].key, "QUOTED");
        assert_eq!(parsed[0].value, "line\nvalue");
        assert_eq!(parsed[1].key, "SINGLE");
        assert_eq!(parsed[1].value, "raw value");
    }

    #[test]
    fn duplicate_keys_keep_last() {
        let input = "A=1\nA=2\n";
        let parsed = parse_str(input).expect("parse should succeed");

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].key, "A");
        assert_eq!(parsed[0].value, "2");
    }

    #[test]
    fn parses_unicode_values() {
        let input = "GREETING=こんにちは\n";
        let parsed = parse_str(input).expect("parse should succeed");

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].value, "こんにちは");
    }

    #[test]
    fn reports_invalid_key() {
        let input = "BAD KEY=value\n";
        let err = parse_str(input).expect_err("expected parse error");
        match err {
            Error::Parse(parse_err) => assert_eq!(parse_err.kind, ParseErrorKind::InvalidKey),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn reports_unterminated_quote() {
        let input = "A=\"value\n";
        let err = parse_str(input).expect_err("expected parse error");
        match err {
            Error::Parse(parse_err) => {
                assert_eq!(parse_err.kind, ParseErrorKind::UnterminatedQuote);
                assert_eq!(parse_err.line, 1);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn parses_multiline_quoted_values() {
        let input = "MULTI_DOUBLE=\"THIS\nIS\nA\nMULTILINE\nSTRING\"\n\
                     MULTI_SINGLE='THIS\nIS\nA\nMULTILINE\nSTRING'\n\
                     AFTER=after\n";
        let parsed = parse_str(input).expect("parse should succeed");

        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].key, "MULTI_DOUBLE");
        assert_eq!(parsed[0].value, "THIS\nIS\nA\nMULTILINE\nSTRING");
        assert_eq!(parsed[1].key, "MULTI_SINGLE");
        assert_eq!(parsed[1].value, "THIS\nIS\nA\nMULTILINE\nSTRING");
        assert_eq!(parsed[2].key, "AFTER");
        assert_eq!(parsed[2].value, "after");
    }

    #[test]
    fn parses_multiline_backtick_values() {
        let input = "MULTI_BACKTICK=`THIS\nIS\nA\n\"MULTILINE'S\"\nSTRING`\n";
        let parsed = parse_str(input).expect("parse should succeed");

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].key, "MULTI_BACKTICK");
        assert_eq!(parsed[0].value, "THIS\nIS\nA\n\"MULTILINE'S\"\nSTRING");
    }

    #[test]
    fn keeps_escaped_single_quotes_inside_multiline_single_quote() {
        let input = "OPTION_K='line one\nthis is \\'quoted\\'\none more line'\n";
        let parsed = parse_str(input).expect("parse should succeed");

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].key, "OPTION_K");
        assert_eq!(
            parsed[0].value,
            "line one\nthis is \\'quoted\\'\none more line"
        );
    }

    #[test]
    fn parses_double_quoted_value_ending_with_escaped_backslash() {
        let input = "PATH=\"C:\\\\Users\\\\\"\nNEXT=ok\n";
        let parsed = parse_str(input).expect("parse should succeed");

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].key, "PATH");
        assert_eq!(parsed[0].value, "C:\\Users\\");
        assert_eq!(parsed[1].key, "NEXT");
        assert_eq!(parsed[1].value, "ok");
    }

    #[test]
    fn parses_single_quoted_value_ending_with_backslash() {
        let input = "A='C:\\\\Temp\\\\'\nB=ok\n";
        let parsed = parse_str(input).expect("parse should succeed");

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].key, "A");
        assert_eq!(parsed[0].value, "C:\\\\Temp\\\\");
        assert_eq!(parsed[1].key, "B");
        assert_eq!(parsed[1].value, "ok");
    }

    #[test]
    fn parses_backtick_quoted_value_ending_with_backslash() {
        let input = "A=`C:\\\\Temp\\\\`\nB=ok\n";
        let parsed = parse_str(input).expect("parse should succeed");

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].key, "A");
        assert_eq!(parsed[0].value, "C:\\\\Temp\\\\");
        assert_eq!(parsed[1].key, "B");
        assert_eq!(parsed[1].value, "ok");
    }

    #[test]
    fn parses_comment_after_multiline_quote() {
        let input = "A=\"line 1\nline 2\" # trailing comment\nB=2\n";
        let parsed = parse_str(input).expect("parse should succeed");

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].key, "A");
        assert_eq!(parsed[0].value, "line 1\nline 2");
        assert_eq!(parsed[1].key, "B");
        assert_eq!(parsed[1].value, "2");
    }

    #[test]
    fn parses_crlf_newlines_in_multiline_quotes() {
        let input = "A=\"line1\r\nline2\"\r\nB=ok\r\n";
        let parsed = parse_str(input).expect("parse should succeed");

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].value, "line1\nline2");
        assert_eq!(parsed[1].value, "ok");
    }

    #[test]
    fn permissive_mode_accepts_extended_key_names() {
        let input = "KEYS:CAN:HAVE_COLONS=1\n%TEMP%=/tmp\n";
        let parsed =
            parse_str_with_mode(input, KeyParsingMode::Permissive).expect("parse should succeed");

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].key, "KEYS:CAN:HAVE_COLONS");
        assert_eq!(parsed[0].value, "1");
        assert_eq!(parsed[1].key, "%TEMP%");
        assert_eq!(parsed[1].value, "/tmp");
    }

    #[test]
    fn permissive_mode_allows_digit_prefixed_and_punctuation_keys() {
        let input = "1KEY=value\n.KEY=dot\nVAR+ALT=plus\n";
        let parsed =
            parse_str_with_mode(input, KeyParsingMode::Permissive).expect("parse should succeed");

        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].key, "1KEY");
        assert_eq!(parsed[0].value, "value");
        assert_eq!(parsed[1].key, ".KEY");
        assert_eq!(parsed[1].value, "dot");
        assert_eq!(parsed[2].key, "VAR+ALT");
        assert_eq!(parsed[2].value, "plus");
    }

    #[test]
    fn permissive_mode_does_not_treat_quotes_in_keys_as_value_quotes() {
        let input = "A\"B=1\nC=2\n";
        let parsed =
            parse_str_with_mode(input, KeyParsingMode::Permissive).expect("parse should succeed");

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].key, "A\"B");
        assert_eq!(parsed[0].value, "1");
        assert_eq!(parsed[1].key, "C");
        assert_eq!(parsed[1].value, "2");
    }

    #[test]
    fn strict_mode_rejects_extended_key_names() {
        let input = "KEYS:CAN:HAVE_COLONS=1\n";
        let err =
            parse_str_with_mode(input, KeyParsingMode::Strict).expect_err("expected parse error");
        match err {
            Error::Parse(parse_err) => assert_eq!(parse_err.kind, ParseErrorKind::InvalidKey),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn permissive_mode_rejects_unicode_control_and_whitespace_in_keys() {
        for input in [
            "foö=1\n",
            "Πoo=1\n",
            "foo\x07=1\n",
            "bar\0=1\n",
            "baz zed=1\n",
        ] {
            let err = parse_str_with_mode(input, KeyParsingMode::Permissive)
                .expect_err("expected parse error");
            match err {
                Error::Parse(parse_err) => assert_eq!(parse_err.kind, ParseErrorKind::InvalidKey),
                other => panic!("unexpected error: {other:?}"),
            }
        }
    }

    #[test]
    fn parse_reader_with_mode_supports_permissive_keys() {
        let reader = std::io::Cursor::new("KEY:ONE=1\n");
        let parsed = parse_reader_with_mode(reader, KeyParsingMode::Permissive)
            .expect("parse should succeed");
        assert_eq!(parsed[0].key, "KEY:ONE");
        assert_eq!(parsed[0].value, "1");
    }
}
