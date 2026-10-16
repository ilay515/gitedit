use crate::errors::GiteditError;
use crate::manifest::types::{CommitAction, ManifestEntry};
use git2::Oid;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_till, take_while1},
    character::complete::{char, space0, space1},
    combinator::{eof, map, rest},
    IResult, Parser,
};

/// Parse a commit action keyword into a [`CommitAction`].
pub fn parse_action_keyword(input: &str) -> IResult<&str, CommitAction> {
    alt((
        map(tag("commit"), |_| CommitAction::Commit),
        map(tag("squash"), |_| CommitAction::Squash),
        map(tag("s"), |_| CommitAction::Squash),
        map(tag("fixup"), |_| CommitAction::Fixup),
        map(tag("f"), |_| CommitAction::Fixup),
        map(tag("drop"), |_| CommitAction::Drop),
        map(tag("d"), |_| CommitAction::Drop),
    ))
    .parse(input)
}

/// Parse a 4 to 64 character hexadecimal commit hash.
pub fn parse_hex_sha(input: &str) -> IResult<&str, &str> {
    let (rest, sha) = take_while1(|c: char| c.is_ascii_hexdigit()).parse(input)?;
    if (4..=64).contains(&sha.len()) {
        Ok((rest, sha))
    } else {
        Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Verify,
        )))
    }
}

/// Parse a full manifest action line: `<action> <sha> [branch_decorations]`.
pub fn parse_action_line(input: &str) -> IResult<&str, (CommitAction, &str, String)> {
    let (input, action) = parse_action_keyword.parse(input)?;
    let (input, _) = space1.parse(input)?;
    let (input, sha) = parse_hex_sha.parse(input)?;
    let (input, _) = space0.parse(input)?;
    let (input, decorations) = rest.parse(input)?;
    Ok((input, (action, sha, decorations.trim().to_string())))
}

/// Check if a line represents a manifest action line.
pub fn is_action_line(line: &str) -> bool {
    parse_action_line.parse(line.trim()).is_ok()
}

/// Parse an author line: `Author: Name <email>`.
pub fn parse_author_line(input: &str) -> IResult<&str, (String, String)> {
    let (input, _) = tag("Author:").parse(input)?;
    let (input, _) = space0.parse(input)?;
    let (input, name) = take_till(|c| c == '<').parse(input)?;
    let (input, _) = char('<').parse(input)?;
    let (input, email) = take_till(|c| c == '>').parse(input)?;
    let (input, _) = char('>').parse(input)?;
    let (input, _) = space0.parse(input)?;
    let (input, _) = eof.parse(input)?;
    Ok((input, (name.trim().to_string(), email.trim().to_string())))
}

/// Parse a date line: `Date: <raw_date_string>`.
pub fn parse_date_line(input: &str) -> IResult<&str, String> {
    let (input, _) = tag("Date:").parse(input)?;
    let (input, _) = space0.parse(input)?;
    let (input, date) = rest.parse(input)?;
    Ok((input, date.trim().to_string()))
}

/// Resolve a SHA string to an Oid if it is a complete 40-char (SHA-1) or 64-char (SHA-256) hash.
/// Otherwise, returns `Oid::ZERO_SHA1` to allow `repo.revparse_single` to resolve prefixes.
pub fn resolve_oid(sha_str: &str) -> Oid {
    if sha_str.len() == 40 || sha_str.len() == 64 {
        Oid::from_str(sha_str).unwrap_or(Oid::ZERO_SHA1)
    } else {
        Oid::ZERO_SHA1
    }
}

/// Advances index past comments (`#...`) and empty lines.
fn advance_past_ignored_lines(lines: &[&str], mut i: usize) -> usize {
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            i += 1;
        } else {
            break;
        }
    }
    i
}

/// Trims leading and trailing empty lines from a list of strings, joining with newlines.
fn clean_message_lines(mut raw_lines: Vec<&str>) -> String {
    while raw_lines.first().is_some_and(|l| l.is_empty()) {
        raw_lines.remove(0);
    }
    while raw_lines.last().is_some_and(|l| l.is_empty()) {
        raw_lines.pop();
    }
    raw_lines.join("\n")
}

/// Collect message lines (4-space indented or blank) following the Date field.
fn collect_commit_message(lines: &[&str], mut i: usize) -> (String, usize) {
    let mut message_lines = Vec::new();

    while i < lines.len() {
        let current_line = lines[i];
        if current_line.trim().starts_with('#') {
            i += 1;
            continue;
        }
        if is_action_line(current_line) {
            break;
        }

        if let Some(stripped) = current_line.strip_prefix("    ") {
            message_lines.push(stripped);
            i += 1;
        } else if current_line.trim().is_empty() {
            message_lines.push("");
            i += 1;
        } else {
            break;
        }
    }

    let message = clean_message_lines(message_lines);
    (message, i)
}

/// Parse an entire manifest text buffer into a list of [`ManifestEntry`].
pub fn parse_manifest(content: &str) -> Result<Vec<ManifestEntry>, GiteditError> {
    let mut entries = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        if trimmed.starts_with('#') || trimmed.is_empty() {
            i += 1;
            continue;
        }

        let action_line_num = i + 1;
        let (_, (action, sha_str, decorations)) = parse_action_line
            .parse(trimmed)
            .map_err(|_| GiteditError::ManifestParse {
                line: action_line_num,
                message: format!("Unrecognized line: '{}'", line),
            })?;

        let short_sha = sha_str.to_string();
        let original_oid = resolve_oid(sha_str);
        i += 1;

        // Skip comments and blank lines before Author
        i = advance_past_ignored_lines(&lines, i);
        if i >= lines.len() {
            return Err(GiteditError::ManifestParse {
                line: action_line_num,
                message: format!("Commit {} missing Author field", short_sha),
            });
        }

        // Parse Author line
        let (author_name, author_email) = parse_author_line
            .parse(lines[i].trim())
            .map(|(_, res)| res)
            .map_err(|_| GiteditError::ManifestParse {
                line: i + 1,
                message: format!("Expected 'Author: Name <email>', found '{}'", lines[i]),
            })?;
        i += 1;

        // Skip comments and blank lines before Date
        i = advance_past_ignored_lines(&lines, i);
        if i >= lines.len() {
            return Err(GiteditError::ManifestParse {
                line: action_line_num,
                message: format!("Commit {} missing Date field", short_sha),
            });
        }

        // Parse Date line
        let date_raw = parse_date_line
            .parse(lines[i].trim())
            .map(|(_, res)| res)
            .map_err(|_| GiteditError::ManifestParse {
                line: i + 1,
                message: format!("Expected 'Date: ...', found '{}'", lines[i]),
            })?;
        i += 1;

        // Collect commit message
        let (message, next_i) = collect_commit_message(&lines, i);
        i = next_i;

        let is_head = decorations.contains("HEAD");
        entries.push(ManifestEntry {
            action,
            original_oid,
            short_sha,
            author_name,
            author_email,
            date_raw,
            message,
            is_head,
            branch_decorations: decorations,
        });
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_action_line_variants() {
        let (_, (action, sha, decor)) = parse_action_line("commit 1234abc (HEAD -> main)").unwrap();
        assert_eq!(action, CommitAction::Commit);
        assert_eq!(sha, "1234abc");
        assert_eq!(decor, "(HEAD -> main)");

        let (_, (action, sha, decor)) = parse_action_line("s 5678def").unwrap();
        assert_eq!(action, CommitAction::Squash);
        assert_eq!(sha, "5678def");
        assert_eq!(decor, "");

        let (_, (action, sha, _)) = parse_action_line("fixup 9999aaa").unwrap();
        assert_eq!(action, CommitAction::Fixup);
        assert_eq!(sha, "9999aaa");

        let (_, (action, sha, _)) = parse_action_line("d 0000bbb").unwrap();
        assert_eq!(action, CommitAction::Drop);
        assert_eq!(sha, "0000bbb");
    }

    #[test]
    fn test_parse_author_and_date_lines() {
        let (_, (name, email)) = parse_author_line("Author: Jane Doe <jane@example.com>").unwrap();
        assert_eq!(name, "Jane Doe");
        assert_eq!(email, "jane@example.com");

        let (_, date) = parse_date_line("Date:   2026-10-16 14:00:00 +03:00").unwrap();
        assert_eq!(date, "2026-10-16 14:00:00 +03:00");
    }

    #[test]
    fn test_parse_manifest_errors() {
        let manifest_missing_author = "commit 1234abcd\n# comment\n";
        let err = parse_manifest(manifest_missing_author).unwrap_err();
        assert!(matches!(err, GiteditError::ManifestParse { line: 1, .. }));

        let manifest_bad_line = "not_a_valid_action 1234abcd\n";
        let err2 = parse_manifest(manifest_bad_line).unwrap_err();
        assert!(matches!(err2, GiteditError::ManifestParse { line: 1, .. }));
    }
}
