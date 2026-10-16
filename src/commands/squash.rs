use crate::cli::Cli;
use crate::errors::GiteditError;
use crate::git::range::resolve_range;
use crate::git::repo::RepoContext;
use crate::git::rewrite::{execute_rewrite, RewritePlan};
use crate::manifest::commit_to_entry;
use crate::manifest::types::{CommitAction, ManifestEntry};
use colored::Colorize;
use git2::{Commit, Oid, Repository};

/// Determine how many commits to squash based on CLI arguments or range notation.
pub fn determine_squash_count(
    repo: &Repository,
    count: Option<usize>,
    range: Option<&str>,
) -> Result<usize, GiteditError> {
    let num_to_squash = match (count, range) {
        (Some(c), _) => c,
        (None, Some(r)) => {
            let (f, t) = if let Some((from_part, to_part)) = r.split_once("..") {
                (Some(from_part), Some(to_part))
            } else {
                (None, Some(r))
            };
            let commits = resolve_range(repo, f, t, 1000)?;
            commits.len()
        }
        (None, None) => 2, // default squash last 2 commits
    };

    if num_to_squash < 2 {
        return Err(GiteditError::Other("Must squash at least 2 commits".into()));
    }

    Ok(num_to_squash)
}

/// Build manifest entries configured for squashing `num_to_squash` recent commits into the base commit.
pub fn build_squash_entries(
    commits: &[Commit],
    num_to_squash: usize,
    fixup: bool,
    custom_message: Option<&str>,
    head_oid: Oid,
    branch_name: &str,
) -> Vec<ManifestEntry> {
    let mut entries = Vec::with_capacity(commits.len());

    for (idx, c) in commits.iter().enumerate() {
        let action = if idx < num_to_squash - 1 {
            if fixup {
                CommitAction::Fixup
            } else {
                CommitAction::Squash
            }
        } else {
            CommitAction::Commit
        };

        entries.push(commit_to_entry(c, action, head_oid, branch_name));
    }

    if let Some(msg) = custom_message {
        if let Some(base) = entries.last_mut() {
            base.message = msg.to_string();
        }
    }

    entries
}

pub fn run(
    cli: &Cli,
    range: Option<&str>,
    count: Option<usize>,
    custom_message: Option<&str>,
    fixup: bool,
) -> Result<(), GiteditError> {
    let ctx = RepoContext::open(".")?;
    let num_to_squash = determine_squash_count(&ctx.repo, count, range)?;

    let commits = resolve_range(&ctx.repo, None, None, num_to_squash)?;
    if commits.len() < num_to_squash {
        return Err(GiteditError::Other(format!(
            "Not enough commits in history to squash {} (found {})",
            num_to_squash,
            commits.len()
        )));
    }

    let entries = build_squash_entries(
        &commits,
        num_to_squash,
        fixup,
        custom_message,
        ctx.head_oid,
        &ctx.branch_name,
    );

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
