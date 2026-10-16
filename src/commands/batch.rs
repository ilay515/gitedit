use crate::cli::Cli;
use crate::date_parser::parse_date;
use crate::editor::{edit_manifest_with_retry, resolve_editor};
use crate::errors::GiteditError;
use crate::git::range::{format_short_sha, resolve_range};
use crate::git::repo::RepoContext;
use crate::git::rewrite::{execute_rewrite, RewritePlan, RewriteResult};
use crate::manifest::types::{CommitAction, ManifestEntry};
use crate::manifest::{commit_to_entry, format_manifest, has_manifest_changes, parse_author_spec};
use colored::Colorize;
use regex::Regex;

/// Parse optional positional range string (e.g. `HEAD~10..HEAD` or `HEAD~10`) or fallback to CLI flags.
pub fn parse_range_boundaries(
    range: Option<&str>,
    cli_from: Option<&str>,
    cli_to: Option<&str>,
) -> (Option<String>, Option<String>) {
    if let Some(r) = range {
        if let Some((f, t)) = r.split_once("..") {
            (Some(f.to_string()), Some(t.to_string()))
        } else {
            (None, Some(r.to_string()))
        }
    } else {
        (cli_from.map(String::from), cli_to.map(String::from))
    }
}

/// Apply author matching regex filter and replace matching authors using nom-parsed author spec.
pub fn apply_author_updates(
    entries: &mut [ManifestEntry],
    match_author: Option<&str>,
    new_author: Option<&str>,
) -> Result<(), GiteditError> {
    let Some(new_auth) = new_author else {
        return Ok(());
    };

    let (parsed_name, parsed_email) = parse_author_spec(new_auth)?;

    if let Some(author_pat) = match_author {
        let re = Regex::new(author_pat)
            .map_err(|e| GiteditError::Other(format!("Invalid regex: {}", e)))?;
        for entry in entries.iter_mut() {
            if re.is_match(&entry.author_name) || re.is_match(&entry.author_email) {
                entry.author_name = parsed_name.clone();
                entry.author_email = parsed_email.clone();
            }
        }
    } else {
        for entry in entries.iter_mut() {
            entry.author_name = parsed_name.clone();
            entry.author_email = parsed_email.clone();
        }
    }

    Ok(())
}

/// Apply relative date shifts or fixed date overrides across all manifest entries.
pub fn apply_date_adjustments(
    entries: &mut [ManifestEntry],
    shift_dates: Option<&str>,
    set_date: Option<&str>,
) -> Result<(), GiteditError> {
    if let Some(shift) = shift_dates {
        for entry in entries.iter_mut() {
            let parsed = parse_date(shift, None)?;
            entry.date_raw = parsed.format("%Y-%m-%d %H:%M:%S %:z").to_string();
        }
    }

    if let Some(fixed_date) = set_date {
        let parsed = parse_date(fixed_date, None)?;
        for entry in entries.iter_mut() {
            entry.date_raw = parsed.format("%Y-%m-%d %H:%M:%S %:z").to_string();
        }
    }

    Ok(())
}

/// Open editor or accept programmatic entries if `no_editor` is specified.
fn resolve_manifest_entries(
    ctx: &RepoContext,
    cli: &Cli,
    entries: Vec<ManifestEntry>,
    no_editor: bool,
) -> Result<Vec<ManifestEntry>, GiteditError> {
    if no_editor {
        Ok(entries)
    } else {
        let manifest_text = format_manifest(&entries);
        let editor_cmd = resolve_editor(cli.editor.as_deref());
        println!("Opening editor '{}'...", editor_cmd.cyan());
        edit_manifest_with_retry(ctx.repo.path(), &manifest_text, &editor_cmd)
    }
}

/// Print formatted execution results to standard output.
fn print_rewrite_summary(result: &RewriteResult, dry_run: bool, verbose: bool) {
    if dry_run {
        println!(
            "{}",
            format!(
                "✓ Dry run complete. {} commit(s) would be rewritten, {} squashed, {} dropped.",
                result.commits_rewritten, result.commits_squashed, result.commits_dropped
            )
            .green()
        );
        return;
    }

    println!("\n{}", "✓ Git history successfully updated!".green().bold());
    println!(
        "Rewritten: {}, Squashed: {}, Dropped: {}",
        result.commits_rewritten.to_string().cyan(),
        result.commits_squashed.to_string().yellow(),
        result.commits_dropped.to_string().red()
    );
    if let Some(ref b_ref) = result.backup_ref {
        println!("Backup saved: {}", b_ref.cyan());
        println!("Run '{}' to rollback at any time.", "gitedit undo".bold());
    }

    if verbose {
        println!("\n{}", "Commit Hash Mapping:".bold());
        for (old_sha, new_sha) in &result.remap {
            println!("  {} -> {}", format_short_sha(*old_sha), format_short_sha(*new_sha));
        }
    }
}

pub fn run(
    cli: &Cli,
    range: Option<&str>,
    match_author: Option<&str>,
    new_author: Option<&str>,
    shift_dates: Option<&str>,
    set_date: Option<&str>,
    no_editor: bool,
) -> Result<(), GiteditError> {
    let ctx = RepoContext::open(".")?;

    if ctx.has_upstream() {
        println!(
            "{}",
            "⚠ Warning: Current branch tracks a remote. Rewriting will require 'git push --force-with-lease'.".yellow()
        );
    }

    let (from_spec, to_spec) = parse_range_boundaries(range, cli.from.as_deref(), cli.to.as_deref());

    let commits = resolve_range(
        &ctx.repo,
        from_spec.as_deref(),
        to_spec.as_deref(),
        cli.limit,
    )?;

    println!(
        "Loaded {} commit(s) on branch '{}'.",
        commits.len().to_string().cyan().bold(),
        ctx.branch_name.green().bold()
    );

    // Convert commits to initial ManifestEntry items using shared helper
    let mut entries: Vec<ManifestEntry> = commits
        .iter()
        .map(|c| commit_to_entry(c, CommitAction::Commit, ctx.head_oid, &ctx.branch_name))
        .collect();

    // Apply CLI flags if specified
    apply_author_updates(&mut entries, match_author, new_author)?;
    apply_date_adjustments(&mut entries, shift_dates, set_date)?;

    let final_entries = resolve_manifest_entries(&ctx, cli, entries, no_editor)?;

    if !has_manifest_changes(&final_entries, &commits) {
        println!("{}", "No changes made to manifest. Nothing to do.".yellow());
        return Ok(());
    }

    let plan = RewritePlan {
        entries: final_entries,
        dry_run: cli.dry_run,
    };

    if cli.dry_run {
        println!("{}", "\n[DRY RUN] Simulating history rewrite:".yellow().bold());
    }

    let result = execute_rewrite(&ctx.repo, &ctx.branch_name, &plan)?;
    print_rewrite_summary(&result, cli.dry_run, cli.verbose);

    Ok(())
}
