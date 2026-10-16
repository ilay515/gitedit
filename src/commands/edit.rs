use crate::cli::Cli;
use crate::date_parser::parse_date;
use crate::editor::{edit_manifest_with_retry, resolve_editor};
use crate::errors::GiteditError;
use crate::git::range::resolve_range;
use crate::git::repo::RepoContext;
use crate::git::rewrite::{execute_rewrite, RewritePlan};
use crate::manifest::types::{CommitAction, ManifestEntry};
use crate::manifest::{commit_to_entry, format_manifest, has_manifest_changes, parse_author_spec};
use chrono::FixedOffset;
use colored::Colorize;
use git2::{Commit, Oid, Repository};

pub struct EditArgs<'a> {
    pub commit_spec: &'a str,
    pub message: Option<&'a str>,
    pub author: Option<&'a str>,
    pub author_name: Option<&'a str>,
    pub author_email: Option<&'a str>,
    pub date: Option<&'a str>,
    pub open_editor: bool,
}

/// Resolves commit chain starting at HEAD down to the specified target commit.
pub fn resolve_target_ancestry_chain<'a>(
    repo: &'a Repository,
    commit_spec: &str,
    limit: usize,
) -> Result<Vec<Commit<'a>>, GiteditError> {
    let target_commit = repo.revparse_single(commit_spec)?.peel_to_commit()?;
    let target_oid = target_commit.id();

    let commits = resolve_range(repo, None, None, limit)?;
    let mut relevant_commits = Vec::new();
    let mut found = false;

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
            commit_spec
        )));
    }

    Ok(relevant_commits)
}

/// Apply non-interactive CLI flags (message, author, date) to the target commit's manifest entry.
fn apply_cli_edit_overrides(
    entry: &mut ManifestEntry,
    args: &EditArgs,
    commit: &Commit,
) -> Result<(), GiteditError> {
    if let Some(m) = args.message {
        entry.message = m.to_string();
    }

    if let Some(a) = args.author {
        let (name, email) = parse_author_spec(a)?;
        entry.author_name = name;
        entry.author_email = email;
    }

    if let Some(an) = args.author_name {
        entry.author_name = an.to_string();
    }

    if let Some(ae) = args.author_email {
        entry.author_email = ae.to_string();
    }

    if let Some(d) = args.date {
        let t = commit.time();
        let offset = FixedOffset::east_opt(t.offset_minutes() * 60)
            .unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
        let dt = chrono::DateTime::from_timestamp(t.seconds(), 0)
            .map(|d| d.with_timezone(&offset));

        let parsed = parse_date(d, dt)?;
        entry.date_raw = parsed.format("%Y-%m-%d %H:%M:%S %:z").to_string();
    }

    Ok(())
}

/// Build entries for all commits between HEAD and target, applying CLI overrides if not opening editor.
fn build_edit_entries(
    relevant_commits: &[Commit],
    target_oid: Oid,
    args: &EditArgs,
    head_oid: Oid,
    branch_name: &str,
) -> Result<Vec<ManifestEntry>, GiteditError> {
    let mut entries = Vec::with_capacity(relevant_commits.len());

    for c in relevant_commits {
        let mut entry = commit_to_entry(c, CommitAction::Commit, head_oid, branch_name);
        if c.id() == target_oid && !args.open_editor {
            apply_cli_edit_overrides(&mut entry, args, c)?;
        }
        entries.push(entry);
    }

    Ok(entries)
}

pub fn run(
    cli: &Cli,
    args: EditArgs,
) -> Result<(), GiteditError> {
    let ctx = RepoContext::open(".")?;
    let relevant_commits = resolve_target_ancestry_chain(&ctx.repo, args.commit_spec, 1000)?;
    let target_oid = relevant_commits.last().unwrap().id();

    let entries = build_edit_entries(
        &relevant_commits,
        target_oid,
        &args,
        ctx.head_oid,
        &ctx.branch_name,
    )?;

    let final_entries = if args.open_editor {
        let manifest_text = format_manifest(&entries);
        let editor_cmd = resolve_editor(cli.editor.as_deref());
        println!("Opening editor '{}'...", editor_cmd.cyan());
        edit_manifest_with_retry(ctx.repo.path(), &manifest_text, &editor_cmd)?
    } else {
        entries
    };

    if !has_manifest_changes(&final_entries, &relevant_commits) {
        println!("{}", "No changes made. Nothing to do.".yellow());
        return Ok(());
    }

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
