use crate::errors::GiteditError;
use crate::git::range::format_short_sha;
use crate::manifest::types::{CommitAction, ManifestEntry};
use chrono::FixedOffset;
use git2::{Commit, Oid};
use nom::{
    bytes::complete::take_till,
    character::complete::{char, space0},
    combinator::eof,
    IResult, Parser,
};

/// Convert a `git2::Commit` into a `ManifestEntry`.
pub fn commit_to_entry(
    commit: &Commit,
    action: CommitAction,
    head_oid: Oid,
    branch_name: &str,
) -> ManifestEntry {
    let oid = commit.id();
    let short_sha = format_short_sha(oid);
    let author = commit.author();
    let author_name = author.name().unwrap_or("").to_string();
    let author_email = author.email().unwrap_or("").to_string();

    let t = commit.time();
    let offset = FixedOffset::east_opt(t.offset_minutes() * 60)
        .unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
    let dt = chrono::DateTime::from_timestamp(t.seconds(), 0)
        .map(|d| d.with_timezone(&offset))
        .unwrap_or_default();
    let date_raw = dt.format("%Y-%m-%d %H:%M:%S %:z").to_string();

    let message = commit.message().unwrap_or("").to_string();
    let is_head = oid == head_oid;
    let branch_decorations = if is_head {
        format!("(HEAD -> {})", branch_name)
    } else {
        String::new()
    };

    ManifestEntry {
        action,
        original_oid: oid,
        short_sha,
        author_name,
        author_email,
        date_raw,
        message,
        is_head,
        branch_decorations,
    }
}

/// Nom parser for `"Name <email>"` mailbox format.
fn nom_author_spec(input: &str) -> IResult<&str, (&str, &str)> {
    let (input, _) = space0.parse(input)?;
    let (input, name) = take_till(|c| c == '<').parse(input)?;
    let (input, _) = char('<').parse(input)?;
    let (input, email) = take_till(|c| c == '>').parse(input)?;
    let (input, _) = char('>').parse(input)?;
    let (input, _) = space0.parse(input)?;
    let (input, _) = eof.parse(input)?;
    Ok((input, (name.trim(), email.trim())))
}

/// Parse an author specification string in `"Name <email>"` format using nom.
pub fn parse_author_spec(input: &str) -> Result<(String, String), GiteditError> {
    match nom_author_spec.parse(input.trim()) {
        Ok((_, (name, email))) => Ok((name.to_string(), email.to_string())),
        Err(_) => Err(GiteditError::InvalidAuthor {
            input: input.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_author_spec() {
        let (name, email) = parse_author_spec("Alice Smith <alice@example.com>").unwrap();
        assert_eq!(name, "Alice Smith");
        assert_eq!(email, "alice@example.com");

        let (name2, email2) = parse_author_spec("Bob <bob@test.org> ").unwrap();
        assert_eq!(name2, "Bob");
        assert_eq!(email2, "bob@test.org");

        assert!(parse_author_spec("Invalid Author").is_err());
        assert!(parse_author_spec("Missing <email").is_err());
    }
}
