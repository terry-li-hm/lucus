use anyhow::Result;
use owo_colors::OwoColorize;

use crate::git;
use crate::output::OutputFormat;

#[derive(Clone, Debug)]
struct Row {
    branch: String,
    path: String,
    ahead: u32,
    behind: u32,
    uncommitted: u32,
}

pub fn run(format: OutputFormat) -> Result<()> {
    let repo_root = git::repo_root_from_cwd()?;
    let entries = git::list_worktrees(&repo_root)?;

    let mut rows = Vec::with_capacity(entries.len());
    for entry in entries {
        let stats = git::worktree_stats(&entry.path)?;
        rows.push(Row {
            branch: entry.branch,
            path: entry.path.display().to_string(),
            ahead: stats.ahead,
            behind: stats.behind,
            uncommitted: stats.uncommitted,
        });
    }

    match format {
        OutputFormat::Human => print_human(&rows),
        OutputFormat::Ndjson => print_ndjson(&rows),
    }

    Ok(())
}

fn print_human(rows: &[Row]) {
    let branch_width = rows
        .iter()
        .map(|row| row.branch.len())
        .max()
        .unwrap_or(6)
        .max(6);
    let path_width = rows
        .iter()
        .map(|row| row.path.len())
        .max()
        .unwrap_or(4)
        .max(4);

    println!(
        "{:<branch_width$}  {:<path_width$}  {:>5}  {:>6}  {:>11}",
        "BRANCH".bold(),
        "PATH".bold(),
        "AHEAD".bold(),
        "BEHIND".bold(),
        "UNCOMMITTED".bold(),
        branch_width = branch_width,
        path_width = path_width,
    );

    for row in rows {
        println!(
            "{:<branch_width$}  {:<path_width$}  {:>5}  {:>6}  {:>11}",
            row.branch.cyan(),
            row.path,
            row.ahead,
            row.behind,
            row.uncommitted,
            branch_width = branch_width,
            path_width = path_width,
        );
    }
}

fn print_ndjson(rows: &[Row]) {
    for line in ndjson_lines(rows) {
        println!("{line}");
    }
}

fn ndjson_lines(rows: &[Row]) -> Vec<String> {
    rows.iter()
        .map(|row| {
            format!(
                "{{\"branch\":\"{}\",\"path\":\"{}\",\"ahead\":{},\"behind\":{},\"uncommitted\":{}}}",
                escape_json(&row.branch),
                escape_json(&row.path),
                row.ahead,
                row.behind,
                row.uncommitted,
            )
        })
        .collect()
}

fn escape_json(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\u{0008}' => escaped.push_str("\\b"),
            '\u{000c}' => escaped.push_str("\\f"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                escaped.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => escaped.push(c),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::{Row, escape_json, ndjson_lines};

    fn row(branch: &str, path: &str) -> Row {
        Row {
            branch: branch.to_owned(),
            path: path.to_owned(),
            ahead: 1,
            behind: 2,
            uncommitted: 3,
        }
    }

    #[test]
    fn escape_json_quotes_backslashes_and_whitespace() {
        assert_eq!(escape_json(r#"a"b\c"#), r#"a\"b\\c"#);
        assert_eq!(escape_json("a\nb\rc\td"), "a\\nb\\rc\\td");
        assert_eq!(escape_json("feat/你好"), "feat/你好");
        assert_eq!(escape_json(""), "");
    }

    /// Regression: every JSON control character is escaped, so no raw C0
    /// byte can leak into NDJSON output.
    #[test]
    fn escape_json_escapes_all_control_characters() {
        for code in 0x00u32..=0x1f {
            let ch = char::from_u32(code).expect("C0 char");
            let escaped = escape_json(&ch.to_string());
            assert!(
                !escaped.contains(ch),
                "raw control U+{code:04X} leaked through escaping"
            );
        }
        assert_eq!(escape_json("\u{0008}"), "\\b");
        assert_eq!(escape_json("\u{000c}"), "\\f");
        assert_eq!(escape_json("\u{0001}"), "\\u0001");
        assert_eq!(escape_json("\u{001f}"), "\\u001f");
        assert_eq!(escape_json("\u{0000}"), "\\u0000");
    }

    #[test]
    fn ndjson_lines_with_control_characters_parse_as_strict_json() {
        let rows = vec![
            row("main", "/tmp/plain"),
            row("feat", "/tmp/x\u{0001}y\u{0000}z"),
            row("a\"b\\c\nd", "\u{0008}\u{000c}\u{001f}tab\there"),
            row("你好\u{007f}", "/tmp/ünïcode\u{0010}"),
        ];

        let lines = ndjson_lines(&rows);
        assert_eq!(lines.len(), rows.len());
        for (line, original) in lines.iter().zip(&rows) {
            let object = parse_strict_object(line)
                .unwrap_or_else(|error| panic!("line {line:?} is not strict JSON: {error}"));
            assert_eq!(object.len(), 5, "line {line:?} must have five fields");
            assert_eq!(object[0].0, "branch");
            assert_eq!(object[0].1, JsonValue::Str(original.branch.clone()));
            assert_eq!(object[1].0, "path");
            assert_eq!(object[1].1, JsonValue::Str(original.path.clone()));
            assert_eq!(object[2], ("ahead".to_owned(), JsonValue::Num(1.0)));
            assert_eq!(object[3], ("behind".to_owned(), JsonValue::Num(2.0)));
            assert_eq!(object[4], ("uncommitted".to_owned(), JsonValue::Num(3.0)));
        }
    }

    #[test]
    fn ndjson_lines_are_single_physical_lines() {
        let rows = vec![row("a\nb\rc\u{000b}", "p\nq")];
        for line in ndjson_lines(&rows) {
            assert!(
                !line.contains('\n'),
                "escaped output must not contain raw newlines"
            );
            assert!(
                !line.contains('\r'),
                "escaped output must not contain raw carriage returns"
            );
        }
    }

    // A minimal strict JSON parser (test-only) proving each emitted line is
    // valid JSON without adding a runtime dependency. It rejects raw control
    // bytes in strings, invalid escapes, malformed \u sequences, and any
    // trailing garbage after the value.
    #[derive(Debug, PartialEq)]
    enum JsonValue {
        Str(String),
        Num(f64),
    }

    fn parse_strict_object(line: &str) -> Result<Vec<(String, JsonValue)>, String> {
        let mut parser = Parser {
            bytes: line.as_bytes(),
            pos: 0,
        };
        let object = parser.parse_object()?;
        parser.skip_whitespace();
        if parser.pos != parser.bytes.len() {
            return Err("trailing bytes after object".to_owned());
        }
        Ok(object)
    }

    struct Parser<'a> {
        bytes: &'a [u8],
        pos: usize,
    }

    impl<'a> Parser<'a> {
        fn peek(&self) -> Option<u8> {
            self.bytes.get(self.pos).copied()
        }

        fn bump(&mut self) -> Option<u8> {
            let byte = self.peek();
            if byte.is_some() {
                self.pos += 1;
            }
            byte
        }

        fn expect(&mut self, expected: u8) -> Result<(), String> {
            match self.bump() {
                Some(byte) if byte == expected => Ok(()),
                other => Err(format!("expected {:?}, got {:?}", expected as char, other)),
            }
        }

        fn skip_whitespace(&mut self) {
            while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
                self.pos += 1;
            }
        }

        fn parse_object(&mut self) -> Result<Vec<(String, JsonValue)>, String> {
            self.expect(b'{')?;
            let mut fields = Vec::new();
            self.skip_whitespace();
            if self.peek() == Some(b'}') {
                self.pos += 1;
                return Ok(fields);
            }
            loop {
                self.skip_whitespace();
                let key = self.parse_string()?;
                self.skip_whitespace();
                self.expect(b':')?;
                self.skip_whitespace();
                let value = self.parse_value()?;
                fields.push((key, value));
                self.skip_whitespace();
                match self.bump() {
                    Some(b',') => continue,
                    Some(b'}') => return Ok(fields),
                    other => return Err(format!("expected ',' or '}}', got {other:?}")),
                }
            }
        }

        fn parse_value(&mut self) -> Result<JsonValue, String> {
            match self.peek() {
                Some(b'"') => Ok(JsonValue::Str(self.parse_string()?)),
                Some(byte) if byte.is_ascii_digit() => self.parse_number(),
                other => Err(format!("unexpected value byte {other:?}")),
            }
        }

        fn parse_string(&mut self) -> Result<String, String> {
            self.expect(b'"')?;
            let mut decoded = String::new();
            let mut raw_start = self.pos;
            loop {
                let byte = self
                    .bump()
                    .ok_or_else(|| "unterminated string".to_owned())?;
                match byte {
                    b'"' => {
                        decoded.push_str(self.raw_segment(raw_start)?);
                        return Ok(decoded);
                    }
                    b'\\' => {
                        decoded.push_str(self.raw_segment(raw_start)?);
                        self.parse_escape(&mut decoded)?;
                        raw_start = self.pos;
                    }
                    0x00..=0x1f => {
                        return Err(format!("raw control byte 0x{byte:02x} in string"));
                    }
                    _ => {}
                }
            }
        }

        /// Copy the bytes from `start` up to (excluding) the byte just
        /// consumed. Segments split only at ASCII boundaries, so they are
        /// always valid UTF-8 slices of the original line.
        fn raw_segment(&self, start: usize) -> Result<&'a str, String> {
            std::str::from_utf8(&self.bytes[start..self.pos.saturating_sub(1)])
                .map_err(|_| "invalid UTF-8 in string".to_owned())
        }

        fn parse_escape(&mut self, decoded: &mut String) -> Result<(), String> {
            let escape = self
                .bump()
                .ok_or_else(|| "unterminated escape".to_owned())?;
            match escape {
                b'"' => decoded.push('"'),
                b'\\' => decoded.push('\\'),
                b'/' => decoded.push('/'),
                b'b' => decoded.push('\u{0008}'),
                b'f' => decoded.push('\u{000c}'),
                b'n' => decoded.push('\n'),
                b'r' => decoded.push('\r'),
                b't' => decoded.push('\t'),
                b'u' => {
                    let mut code = 0u32;
                    for _ in 0..4 {
                        let digit = self
                            .bump()
                            .ok_or_else(|| "truncated unicode escape".to_owned())?;
                        let value = (digit as char)
                            .to_digit(16)
                            .ok_or_else(|| "invalid hex digit in unicode escape".to_owned())?;
                        code = code * 16 + value;
                    }
                    let ch = char::from_u32(code)
                        .ok_or_else(|| "invalid unicode escape code point".to_owned())?;
                    decoded.push(ch);
                }
                other => {
                    return Err(format!("invalid escape \\{}", other as char));
                }
            }
            Ok(())
        }
        fn parse_number(&mut self) -> Result<JsonValue, String> {
            let start = self.pos;
            while matches!(self.peek(), Some(byte) if byte.is_ascii_digit()) {
                self.pos += 1;
            }
            if self.pos == start {
                return Err("empty number".to_owned());
            }
            let text = std::str::from_utf8(&self.bytes[start..self.pos])
                .map_err(|_| "number not UTF-8".to_owned())?;
            text.parse::<f64>()
                .map(JsonValue::Num)
                .map_err(|_| "invalid number".to_owned())
        }
    }

    #[test]
    fn strict_parser_rejects_raw_controls_and_bad_escapes() {
        assert!(parse_strict_object("{\"a\":\"x\ty\"}").is_err());
        assert!(parse_strict_object("{\"a\":\"x\u{1}y\"}").is_err());
        assert!(parse_strict_object("{\"a\":\"x\\qy\"}").is_err());
        assert!(parse_strict_object("{\"a\":\"x\\u00zz\"}").is_err());
        assert!(parse_strict_object("{\"a\":1} trailing").is_err());
        assert!(parse_strict_object("{\"a\":\"ok\",\"b\":2}").is_ok());
    }
}
