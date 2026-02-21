use std::collections::HashMap;
use std::io::BufRead;
use std::path::Path;

use crate::error::{Error, ParseError, ParseErrorKind};
use crate::model::Entry;

/// Parse dotenv entries from UTF-8 text.
pub fn parse_str(input: &str) -> Result<Vec<Entry>, Error> {
    parse_str_with_source(input, None).map_err(Error::from)
}

/// Parse dotenv entries from UTF-8 bytes.
pub fn parse_bytes(input: &[u8]) -> Result<Vec<Entry>, Error> {
    let text = std::str::from_utf8(input)?;
    parse_str(text)
}

/// Parse dotenv entries from a buffered reader.
pub fn parse_reader<R: BufRead>(mut reader: R) -> Result<Vec<Entry>, Error> {
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf)?;
    parse_bytes(&buf)
}

pub(crate) fn parse_str_with_source(
    input: &str,
    source: Option<&Path>,
) -> Result<Vec<Entry>, ParseError> {
    let mut entries = Vec::new();
    let mut by_key = HashMap::<String, usize>::new();

    for (idx, raw_line) in input.lines().enumerate() {
        let line_num = idx as u32 + 1;
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let parsed = parse_line(line, line_num, source)?;
        let Some(entry) = parsed else {
            continue;
        };

        if let Some(existing_idx) = by_key.get(&entry.key).copied() {
            entries[existing_idx] = entry;
        } else {
            by_key.insert(entry.key.clone(), entries.len());
            entries.push(entry);
        }
    }

    Ok(entries)
}

fn parse_line(
    line: &str,
    line_num: u32,
    source: Option<&Path>,
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
    if !is_valid_key(key) {
        return Err(ParseError::new(line_num, 1, ParseErrorKind::InvalidKey));
    }

    let value_input = working[eq_idx + 1..].trim_start();
    let value_column = (line.len() - value_input.len()) as u32 + 1;
    let value = parse_value(value_input, line_num, value_column)?;

    Ok(Some(Entry {
        key: key.to_owned(),
        value,
        source: source.map(Path::to_path_buf),
        line: line_num,
    }))
}

fn parse_value(input: &str, line_num: u32, column: u32) -> Result<String, ParseError> {
    if input.is_empty() {
        return Ok(String::new());
    }

    if input.starts_with('\'') {
        return parse_single_quoted(input, line_num, column);
    }
    if input.starts_with('"') {
        return parse_double_quoted(input, line_num, column);
    }

    let value = input
        .split_once('#')
        .map(|(head, _)| head)
        .unwrap_or(input)
        .trim_end();
    Ok(value.to_owned())
}

fn parse_single_quoted(input: &str, line_num: u32, column: u32) -> Result<String, ParseError> {
    let mut closing_idx = None;
    for (idx, ch) in input.char_indices().skip(1) {
        if ch == '\'' {
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

fn parse_double_quoted(input: &str, line_num: u32, column: u32) -> Result<String, ParseError> {
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

fn is_valid_key(key: &str) -> bool {
    key.chars().all(is_valid_key_char)
}

fn is_valid_key_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' || ch == '-'
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
}
