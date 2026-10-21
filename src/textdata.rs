//! Shared reader for Ozbound's plain-text content files (quests, dialogue).
//!
//! A file is a list of `[kind arg]` section headers and `key = value` fields. Blank lines
//! and lines starting with `#` are ignored; section kinds and field keys are
//! case-insensitive. Every problem is reported with a 1-based line number where one exists.

use std::fmt;

/// A problem in content data. `line` is 1-based, or `None` for content built in code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentError {
    pub line: Option<usize>,
    pub message: String,
}

impl ContentError {
    /// An error at a file line. Line `0` means "no line" (content built in code).
    pub fn at(line: usize, message: impl Into<String>) -> Self {
        Self {
            line: if line == 0 { None } else { Some(line) },
            message: message.into(),
        }
    }
}

impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: {}", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for ContentError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Section { kind: String, arg: String },
    Field { key: String, value: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub number: usize,
    pub item: Item,
}

/// Splits a content file into sections and fields. Handles CRLF and a leading BOM.
pub fn read_lines(text: &str) -> Result<Vec<Line>, ContentError> {
    let mut out = Vec::new();
    for (index, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        let number = index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let item = if let Some(rest) = line.strip_prefix('[') {
            let inner = rest
                .strip_suffix(']')
                .ok_or_else(|| ContentError::at(number, "section header is missing its closing ]"))?
                .trim();
            let (kind, arg) = match inner.split_once(char::is_whitespace) {
                Some((kind, arg)) => (kind, arg.trim()),
                None => (inner, ""),
            };
            if kind.is_empty() {
                return Err(ContentError::at(number, "section header has no name"));
            }
            Item::Section {
                kind: kind.to_ascii_lowercase(),
                arg: arg.to_string(),
            }
        } else if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphabetic() || c == '_') {
                return Err(ContentError::at(
                    number,
                    format!("`{key}` is not a valid field name (use letters and underscores)"),
                ));
            }
            Item::Field {
                key: key.to_ascii_lowercase(),
                value: value.trim().to_string(),
            }
        } else {
            return Err(ContentError::at(
                number,
                "expected `[section]` or `key = value`",
            ));
        };
        out.push(Line { number, item });
    }
    Ok(out)
}

/// Ids are lower-case ASCII letters, digits, and underscores, and are never empty.
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Location ids are capital ASCII letters, digits, and underscores (`MUNCHKIN_LANDS`), and
/// start with a letter. They match the stable ids in `LOCATIONS.md`.
pub fn is_valid_location_id(id: &str) -> bool {
    id.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && id.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Checks an id, naming what it identifies in the error (`what` is e.g. "quest id").
pub fn check_id(line: usize, what: &str, id: &str) -> Result<(), ContentError> {
    if is_valid_id(id) {
        Ok(())
    } else {
        Err(ContentError::at(
            line,
            format!("{what} `{id}` must use only lower-case letters, digits, and underscores"),
        ))
    }
}

/// Parses a positive whole number.
pub fn parse_positive(line: usize, what: &str, text: &str) -> Result<u32, ContentError> {
    match text.parse::<u32>() {
        Ok(n) if n > 0 => Ok(n),
        _ => Err(ContentError::at(
            line,
            format!("{what} must be a whole number above zero, found `{text}`"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sections_fields_comments_and_crlf() {
        let text = "\u{feff}# hi\r\n\r\n[Quest  first_one ]\r\nTitle = A = B \r\n[end]\r\n";
        let lines = read_lines(text).unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].number, 3);
        assert_eq!(
            lines[0].item,
            Item::Section {
                kind: "quest".into(),
                arg: "first_one".into()
            }
        );
        assert_eq!(
            lines[1].item,
            Item::Field {
                key: "title".into(),
                value: "A = B".into()
            }
        );
        assert_eq!(
            lines[2].item,
            Item::Section {
                kind: "end".into(),
                arg: String::new()
            }
        );
    }

    #[test]
    fn rejects_malformed_lines_with_line_numbers() {
        let err = read_lines("[ok]\n[broken\n").unwrap_err();
        assert_eq!(err.line, Some(2));
        assert!(err.message.contains("closing ]"));
        let err = read_lines("\njust words\n").unwrap_err();
        assert_eq!(err.line, Some(2));
        let err = read_lines("[]\n").unwrap_err();
        assert!(err.message.contains("no name"));
        let err = read_lines("bad key = 1\n").unwrap_err();
        assert!(err.message.contains("not a valid field name"));
        let err = read_lines(" = 1\n").unwrap_err();
        assert!(err.message.contains("not a valid field name"));
    }

    #[test]
    fn ids_and_numbers() {
        assert!(is_valid_id("moon_2"));
        for bad in ["", "Moon", "a-b", "a b", "é"] {
            assert!(!is_valid_id(bad), "{bad}");
        }
        assert!(check_id(4, "quest id", "Bad")
            .unwrap_err()
            .to_string()
            .starts_with("line 4: quest id `Bad`"));
        assert_eq!(parse_positive(1, "count", "3"), Ok(3));
        for bad in ["0", "-1", "x", "", "99999999999"] {
            assert!(parse_positive(1, "count", bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn location_ids_are_capitals_digits_and_underscores() {
        assert!(is_valid_location_id("MUNCHKIN_LANDS"));
        assert!(is_valid_location_id("WOODS_FIRST"));
        assert!(is_valid_location_id("A1"));
        for bad in ["", "munchkin", "Munchkin_Lands", "_START", "1ST", "A-B", "A B", "É"] {
            assert!(!is_valid_location_id(bad), "{bad}");
        }
    }

    #[test]
    fn error_display_includes_line_only_when_known() {
        assert_eq!(ContentError::at(7, "oops").to_string(), "line 7: oops");
        assert_eq!(ContentError::at(0, "oops").to_string(), "oops");
    }
}
