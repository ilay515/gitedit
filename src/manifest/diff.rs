use crate::date_parser::parse_date;
use crate::manifest::types::{CommitAction, ManifestEntry};
use chrono::{DateTime, FixedOffset};
use git2::Commit;

pub fn has_manifest_changes(entries: &[ManifestEntry], original_commits: &[Commit]) -> bool {
    if entries.len() != original_commits.len() {
        return true;
    }

    for (entry, orig) in entries.iter().zip(original_commits.iter()) {
        if entry.action != CommitAction::Commit {
            return true;
        }

        // Compare commit identity:
        // entry.short_sha should match original commit's SHA prefix
        let orig_sha = orig.id().to_string();
        if !orig_sha.starts_with(&entry.short_sha) && entry.original_oid != orig.id() {
            return true;
        }

        // Compare author name and email
        let orig_author = orig.author();
        if entry.author_name != orig_author.name().unwrap_or("") {
            return true;
        }
        if entry.author_email != orig_author.email().unwrap_or("") {
            return true;
        }

        // Compare commit message
        let orig_msg = orig.message().unwrap_or("").trim();
        if entry.message.trim() != orig_msg {
            return true;
        }

        // Compare date
        let orig_time = orig.time();
        let orig_offset = FixedOffset::east_opt(orig_time.offset_minutes() * 60)
            .unwrap_or(FixedOffset::east_opt(0).unwrap());
        let orig_dt = DateTime::from_timestamp(orig_time.seconds(), 0)
            .map(|dt| dt.with_timezone(&orig_offset));

        if let Ok(parsed_dt) = parse_date(&entry.date_raw, orig_dt) {
            if parsed_dt.timestamp() != orig_time.seconds()
                || parsed_dt.offset().local_minus_utc() / 60 != orig_time.offset_minutes()
            {
                return true;
            }
        } else {
            return true;
        }
    }

    false
}
