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
    ctx.check_dirty(cli.force)?;

    if ctx.has_upstream() {
        println!(
            "{}",
            "⚠ Warning: Current branch tracks a remote. Rewriting will require 'git push --force-with-lease'.".yellow()
        );
    }

    // Parse range if given (e.g. HEAD~10..HEAD)
    let (from_spec, to_spec) = if let Some(r) = range {
        if let Some((f, t)) = r.split_once("..") {
            (Some(f.to_string()), Some(t.to_string()))
        } else {
            (None, Some(r.to_string()))
        }
    } else {
        (cli.from.clone(), cli.to.clone())
    };

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

    // Convert commits to initial ManifestEntry items
    let mut entries = Vec::new();
    let head_oid = ctx.head_oid;

    for c in &commits {
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

        let message = c.message().unwrap_or("").to_string();
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
            author_name,
            author_email,
            date_raw,
            message,
            is_head,
            branch_decorations,
        });
    }

    let parse_re = Regex::new(r"^(.*?)\s*<([^>]+)>$").unwrap();

    // Apply CLI flags if specified
    if let Some(author_pat) = match_author {
        let re = Regex::new(author_pat).map_err(|e| GiteditError::Other(format!("Invalid regex: {}", e)))?;
        for entry in &mut entries {
            if re.is_match(&entry.author_name) || re.is_match(&entry.author_email) {
                if let Some(new_auth) = new_author {
                    if let Some(caps) = parse_re.captures(new_auth.trim()) {
                        entry.author_name = caps[1].trim().to_string();
                        entry.author_email = caps[2].trim().to_string();
                    } else {
                        return Err(GiteditError::InvalidAuthor {
                            input: new_auth.to_string(),
                        });
                    }
                }
            }
        }
    } else if let Some(new_auth) = new_author {
        if let Some(caps) = parse_re.captures(new_auth.trim()) {
            for entry in &mut entries {
                entry.author_name = caps[1].trim().to_string();
                entry.author_email = caps[2].trim().to_string();
            }
        } else {
            return Err(GiteditError::InvalidAuthor {
                input: new_auth.to_string(),
            });
        }
    }

    if let Some(shift) = shift_dates {
        for entry in &mut entries {
            let parsed = parse_date(shift, None)?;
            entry.date_raw = parsed.format("%Y-%m-%d %H:%M:%S %:z").to_string();
        }
    }

    if let Some(fixed_date) = set_date {
        let parsed = parse_date(fixed_date, None)?;
        for entry in &mut entries {
            entry.date_raw = parsed.format("%Y-%m-%d %H:%M:%S %:z").to_string();
        }
    }

    // Open editor unless no_editor is set
    let final_entries = if no_editor {
        entries
    } else {
        let manifest_text = format_manifest(&entries);
        let editor_cmd = resolve_editor(cli.editor.as_deref());
        println!("Opening editor '{}'...", editor_cmd.cyan());
        edit_manifest_with_retry(ctx.repo.path(), &manifest_text, &editor_cmd)?
    };

    let plan = RewritePlan {
        entries: final_entries,
        dry_run: cli.dry_run,
    };

    if cli.dry_run {
        println!("{}", "\n[DRY RUN] Simulating history rewrite:".yellow().bold());
    }

    let result = execute_rewrite(&ctx.repo, &ctx.branch_name, &plan)?;

    if cli.dry_run {
        println!(
            "{}",
            format!(
                "✓ Dry run complete. {} commit(s) would be rewritten, {} squashed, {} dropped.",
                result.commits_rewritten, result.commits_squashed, result.commits_dropped
            )
            .green()
        );
        return Ok(());
    }

    println!("\n{}", "✓ Git history successfully updated!".green().bold());
    println!(
        "Rewritten: {}, Squashed: {}, Dropped: {}",
        result.commits_rewritten.to_string().cyan(),
        result.commits_squashed.to_string().yellow(),
        result.commits_dropped.to_string().red()
    );
    if let Some(b_ref) = result.backup_ref {
        println!("Backup saved: {}", b_ref.cyan());
        println!("Run '{}' to rollback at any time.", "gitedit undo".bold());
    }

    if cli.verbose {
        println!("\n{}", "Commit Hash Mapping:".bold());
        for (old_sha, new_sha) in result.remap {
            println!("  {} -> {}", format_short_sha(old_sha), format_short_sha(new_sha));
        }
    }

    Ok(())
}
