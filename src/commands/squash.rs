use crate::cli::Cli;
use crate::errors::GiteditError;
use crate::git::range::{format_short_sha, resolve_range};
use crate::git::repo::RepoContext;
use crate::git::rewrite::{execute_rewrite, RewritePlan};
use crate::manifest::types::{CommitAction, ManifestEntry};
use chrono::FixedOffset;
use colored::Colorize;

pub fn run(
    cli: &Cli,
    range: Option<&str>,
    count: Option<usize>,
    custom_message: Option<&str>,
    fixup: bool,
) -> Result<(), GiteditError> {
    let ctx = RepoContext::open(".")?;
    ctx.check_dirty(cli.force)?;

    let num_to_squash = match (count, range) {
        (Some(c), _) => c,
        (None, Some(r)) => {
            let (f, t) = if let Some((from_part, to_part)) = r.split_once("..") {
                (Some(from_part), Some(to_part))
            } else {
                (None, Some(r))
            };
            let commits = resolve_range(&ctx.repo, f, t, 1000)?;
            commits.len()
        }
        (None, None) => 2, // default squash last 2 commits
    };

    if num_to_squash < 2 {
        return Err(GiteditError::Other("Must squash at least 2 commits".into()));
    }

    let commits = resolve_range(&ctx.repo, None, None, num_to_squash)?;
    if commits.len() < num_to_squash {
        return Err(GiteditError::Other(format!(
            "Not enough commits in history to squash {} (found {})",
            num_to_squash,
            commits.len()
        )));
    }

    let mut entries = Vec::new();
    let head_oid = ctx.head_oid;

    for (idx, c) in commits.iter().enumerate() {
        let oid = c.id();
        let short_sha = format_short_sha(oid);
        let author = c.author();
        let author_name = author.name().unwrap_or("").to_string();
        let author_email = author.email().unwrap_or("").to_string();

        let t = c.time();
        let offset = FixedOffset::east_opt(t.offset_minutes() * 60).unwrap_or(FixedOffset::east_opt(0).unwrap());
        let dt = chrono::DateTime::from_timestamp(t.seconds(), 0)
            .map(|d| d.with_timezone(&offset))
            .unwrap_or_default();
        let date_raw = dt.format("%Y-%m-%d %H:%M:%S %:z").to_string();

        let msg = c.message().unwrap_or("").to_string();

        // In git log order:
        // index 0..n-2 are newer commits, they are marked Squash/Fixup
        // index n-1 is the oldest base commit, it is marked Commit
        let action = if idx < num_to_squash - 1 {
            if fixup {
                CommitAction::Fixup
            } else {
                CommitAction::Squash
            }
        } else {
            CommitAction::Commit
        };

        let is_head = oid == head_oid;
        let branch_decorations = if is_head {
            format!("(HEAD -> {})", ctx.branch_name)
        } else {
            String::new()
        };

        entries.push(ManifestEntry {
            action,
            original_oid: oid,
            short_sha,
            author_name,
            author_email,
            date_raw,
            message: msg,
            is_head,
            branch_decorations,
        });
    }

    if let Some(msg) = custom_message {
        // Set message on the base commit (last entry)
        if let Some(base) = entries.last_mut() {
            base.message = msg.to_string();
        }
    }

    let plan = RewritePlan {
        entries,
        dry_run: cli.dry_run,
    };

    let result = execute_rewrite(&ctx.repo, &ctx.branch_name, &plan)?;

    println!(
        "{}",
        format!("✓ Successfully squashed {} commits into 1!", num_to_squash)
            .green()
            .bold()
    );
    if let Some(b_ref) = result.backup_ref {
        println!("Backup saved: {}", b_ref.cyan());
        println!("Run '{}' to rollback.", "gitedit undo".bold());
    }

    Ok(())
}
