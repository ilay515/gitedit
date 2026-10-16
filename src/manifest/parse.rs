use crate::errors::GiteditError;
use crate::manifest::types::{CommitAction, ManifestEntry};
use git2::Oid;
use regex::Regex;

pub fn parse_manifest(content: &str) -> Result<Vec<ManifestEntry>, GiteditError> {
    let mut entries = Vec::new();
    let action_re = Regex::new(r"^(commit|squash|s|fixup|f|drop|d)\s+([0-9a-fA-F]{4,64})(?:\s+(.*))?$").unwrap();
    let author_re = Regex::new(r"^Author:\s*(.*?)\s*<([^>]+)>\s*$").unwrap();
    let date_re = Regex::new(r"^Date:\s*(.*)$").unwrap();

    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        if trimmed.starts_with('#') || trimmed.is_empty() {
            i += 1;
            continue;
        }

        if let Some(caps) = action_re.captures(trimmed) {
            let action_str = &caps[1];
            let sha_str = &caps[2];
            let decorations = caps.get(3).map(|m| m.as_str().trim().to_string()).unwrap_or_default();

            let action = match action_str {
                "commit" => CommitAction::Commit,
                "squash" | "s" => CommitAction::Squash,
                "fixup" | "f" => CommitAction::Fixup,
                "drop" | "d" => CommitAction::Drop,
                _ => unreachable!(),
            };

            let line_num = i + 1;
            // Only parse as full OID if the string is complete (40 hex chars for SHA-1, 64 for SHA-256)
            // Otherwise, keep it ZERO_SHA1 so that repository.revparse_single resolves the prefix!
            let original_oid = if sha_str.len() == 40 || sha_str.len() == 64 {
                Oid::from_str(sha_str).unwrap_or(Oid::ZERO_SHA1)
            } else {
                Oid::ZERO_SHA1
            };

            i += 1;

            while i < lines.len() && (lines[i].trim().starts_with('#') || lines[i].trim().is_empty()) {
                i += 1;
            }

            if i >= lines.len() {
                return Err(GiteditError::ManifestParse {
                    line: line_num,
                    message: format!("Commit {} missing Author field", sha_str),
                });
            }

            let (author_name, author_email) = if let Some(auth_caps) = author_re.captures(lines[i].trim()) {
                let name = auth_caps[1].trim().to_string();
                let email = auth_caps[2].trim().to_string();
                i += 1;
                (name, email)
            } else {
                return Err(GiteditError::ManifestParse {
                    line: i + 1,
                    message: format!("Expected 'Author: Name <email>', found '{}'", lines[i]),
                });
            };

            while i < lines.len() && (lines[i].trim().starts_with('#') || lines[i].trim().is_empty()) {
                i += 1;
            }

            if i >= lines.len() {
                return Err(GiteditError::ManifestParse {
                    line: line_num,
                    message: format!("Commit {} missing Date field", sha_str),
                });
            }

            let date_raw = if let Some(date_caps) = date_re.captures(lines[i].trim()) {
                let d = date_caps[1].trim().to_string();
                i += 1;
                d
            } else {
                return Err(GiteditError::ManifestParse {
                    line: i + 1,
                    message: format!("Expected 'Date: ...', found '{}'", lines[i]),
                });
            };

            let mut message_lines = Vec::new();
            while i < lines.len() {
                let current_line = lines[i];
                if current_line.trim().starts_with('#') {
                    i += 1;
                    continue;
                }
                if action_re.is_match(current_line.trim()) {
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

            while message_lines.first().is_some_and(|l| l.is_empty()) {
                message_lines.remove(0);
            }
            while message_lines.last().is_some_and(|l| l.is_empty()) {
                message_lines.pop();
            }

            let message = message_lines.join("\n");

            entries.push(ManifestEntry {
                action,
                original_oid,
                short_sha: sha_str.to_string(),
                author_name,
                author_email,
                date_raw,
                message,
                is_head: decorations.contains("HEAD"),
                branch_decorations: decorations,
            });
        } else {
            return Err(GiteditError::ManifestParse {
                line: i + 1,
                message: format!("Unrecognized line: '{}'", line),
            });
        }
    }

    Ok(entries)
}
