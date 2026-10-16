use crate::cli::Cli;
use crate::date_parser::parse_date;
use crate::editor::{edit_manifest_with_retry, resolve_editor};
use crate::errors::GiteditError;
use crate::git::range::{format_short_sha, resolve_range};
use crate::git::repo::RepoContext;
use crate::git::rewrite::{execute_rewrite, RewritePlan};
use crate::manifest::format_manifest;
use crate::manifest::types::{CommitAction, ManifestEntry};
use chrono::FixedOffset;
use colored::Colorize;
use regex::Regex;

pub struct EditArgs<'a> {
    pub commit_spec: &'a str,
    pub message: Option<&'a str>,
    pub author: Option<&'a str>,
    pub author_name: Option<&'a str>,
    pub author_email: Option<&'a str>,
    pub date: Option<&'a str>,
    pub open_editor: bool,
}

pub fn run(
    cli: &Cli,
    args: EditArgs,
) -> Result<(), GiteditError> {
    let ctx = RepoContext::open(".")?;
    ctx.check_dirty(cli.force)?;

    let target_commit = ctx.repo.revparse_single(args.commit_spec)?.peel_to_commit()?;
    let target_oid = target_commit.id();

    // Range must include all commits from HEAD down to target_commit
    let commits = resolve_range(&ctx.repo, None, None, 1000)?;
    let mut found = false;
    let mut relevant_commits = Vec::new();

    for c in commits {
        let is_target = c.id() == target_oid;
        relevant_commits.push(c);
        if is_target {
            found = true;
            break;
        }
    }

    if !found {
        return Err(GiteditError::Other(format!(
            "Commit {} is not an ancestor of current branch tip",
            args.commit_spec
        )));
    }

    let mut entries = Vec::new();
    let head_oid = ctx.head_oid;
    let re = Regex::new(r"^(.*?)\s*<([^>]+)>$").unwrap();

    for c in &relevant_commits {
        let oid = c.id();
        let short_sha = format_short_sha(oid);
        let c_auth = c.author();
        let mut a_name = c_auth.name().unwrap_or("").to_string();
        let mut a_email = c_auth.email().unwrap_or("").to_string();

        let t = c.time();
        let offset = FixedOffset::east_opt(t.offset_minutes() * 60).unwrap_or(FixedOffset::east_opt(0).unwrap());
        let dt = chrono::DateTime::from_timestamp(t.seconds(), 0)
            .map(|d| d.with_timezone(&offset))
            .unwrap_or_default();
        let mut date_raw = dt.format("%Y-%m-%d %H:%M:%S %:z").to_string();

        let mut msg = c.message().unwrap_or("").to_string();

        if oid == target_oid && !args.open_editor {
            if let Some(m) = args.message {
                msg = m.to_string();
            }
            if let Some(a) = args.author {
                if let Some(caps) = re.captures(a.trim()) {
                    a_name = caps[1].trim().to_string();
                    a_email = caps[2].trim().to_string();
                } else {
                    return Err(GiteditError::InvalidAuthor {
                        input: a.to_string(),
                    });
                }
            }
            if let Some(an) = args.author_name {
                a_name = an.to_string();
            }
            if let Some(ae) = args.author_email {
                a_email = ae.to_string();
            }
            if let Some(d) = args.date {
                let parsed = parse_date(d, Some(dt))?;
                date_raw = parsed.format("%Y-%m-%d %H:%M:%S %:z").to_string();
            }
        }

        let is_head = oid == head_oid;
        let branch_decorations = if is_head {
            format!("(HEAD -> {})", ctx.branch_name)
        } else {
            String::new()
        };

        entries.push(ManifestEntry {
            action: CommitAction::Commit,
            original_oid: oid,
            short_sha,
            author_name: a_name,
            author_email: a_email,
            date_raw,
            message: msg,
            is_head,
            branch_decorations,
        });
    }

    let final_entries = if args.open_editor {
        let manifest_text = format_manifest(&entries);
        let editor_cmd = resolve_editor(cli.editor.as_deref());
        println!("Opening editor '{}'...", editor_cmd.cyan());
        edit_manifest_with_retry(ctx.repo.path(), &manifest_text, &editor_cmd)?
    } else {
        entries
    };

    let plan = RewritePlan {
        entries: final_entries,
        dry_run: cli.dry_run,
    };

    let result = execute_rewrite(&ctx.repo, &ctx.branch_name, &plan)?;

    println!("{}", "✓ Commit successfully updated!".green().bold());
    if let Some(b_ref) = result.backup_ref {
        println!("Backup saved: {}", b_ref.cyan());
        println!("Run '{}' to rollback.", "gitedit undo".bold());
    }

    Ok(())
}
