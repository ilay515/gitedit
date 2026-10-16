use crate::date_parser::parse_date;
use crate::errors::GiteditError;
use crate::git::backup::create_backup;
use crate::git::range::{format_short_sha, is_merge_commit};
use crate::manifest::types::{CommitAction, ManifestEntry};
use chrono::{DateTime, FixedOffset};
use git2::{Commit, Oid, Repository, Signature, Time};
use std::collections::HashMap;

pub struct RewritePlan {
    pub entries: Vec<ManifestEntry>,
    pub dry_run: bool,
}

pub struct RewriteResult {
    pub commits_rewritten: usize,
    pub commits_squashed: usize,
    pub commits_dropped: usize,
    #[allow(dead_code)]
    pub new_tip: Oid,
    pub backup_ref: Option<String>,
    pub remap: HashMap<Oid, Oid>,
}

/// Pre-processed squash grouping metadata.
pub struct SquashGroups {
    pub target_for: HashMap<Oid, Oid>,
    pub messages: HashMap<Oid, Vec<(CommitAction, String)>>,
    pub trees: HashMap<Oid, Oid>,
}

/// Validate boundaries, ensure merge commits are not improperly targeted, and resolve commit hashes.
pub fn validate_and_resolve_entries<'a>(
    repo: &'a Repository,
    entries: &[ManifestEntry],
) -> Result<(Vec<ManifestEntry>, HashMap<Oid, Commit<'a>>), GiteditError> {
    if entries.is_empty() {
        return Err(GiteditError::EmptyRange);
    }

    let n = entries.len();
    if entries[n - 1].action == CommitAction::Squash || entries[n - 1].action == CommitAction::Fixup {
        return Err(GiteditError::SquashAtBoundary);
    }

    let mut commits_by_oid: HashMap<Oid, Commit> = HashMap::new();
    let mut resolved_entries: Vec<ManifestEntry> = Vec::with_capacity(entries.len());

    for entry in entries {
        let commit = if entry.original_oid != Oid::ZERO_SHA1 {
            match repo.find_commit(entry.original_oid) {
                Ok(c) => c,
                Err(_) => repo.revparse_single(&entry.short_sha)?.peel_to_commit()?,
            }
        } else {
            repo.revparse_single(&entry.short_sha)?.peel_to_commit()?
        };

        let is_destructive = matches!(
            entry.action,
            CommitAction::Squash | CommitAction::Fixup | CommitAction::Drop
        );
        if is_destructive && is_merge_commit(&commit) {
            return Err(GiteditError::MergeCommitUnsupported {
                action: entry.action.as_str().to_string(),
                sha: format_short_sha(commit.id()),
                parent_count: commit.parent_count(),
            });
        }

        let mut entry_clone = entry.clone();
        entry_clone.original_oid = commit.id();
        commits_by_oid.insert(commit.id(), commit);
        resolved_entries.push(entry_clone);
    }

    Ok((resolved_entries, commits_by_oid))
}

/// Pre-process squash and fixup chains, associating them with their target base commits.
pub fn build_squash_groups(
    resolved_entries: &[ManifestEntry],
    commits_by_oid: &HashMap<Oid, Commit>,
) -> Result<SquashGroups, GiteditError> {
    let mut target_for: HashMap<Oid, Oid> = HashMap::new();
    let mut messages: HashMap<Oid, Vec<(CommitAction, String)>> = HashMap::new();
    let mut trees: HashMap<Oid, Oid> = HashMap::new();

    for (i, entry) in resolved_entries.iter().enumerate() {
        if entry.action == CommitAction::Squash || entry.action == CommitAction::Fixup {
            let base_oid = resolved_entries
                .iter()
                .skip(i + 1)
                .find(|candidate| candidate.action == CommitAction::Commit)
                .map(|candidate| candidate.original_oid)
                .ok_or(GiteditError::SquashAtBoundary)?;

            target_for.insert(entry.original_oid, base_oid);
            messages
                .entry(base_oid)
                .or_default()
                .push((entry.action, entry.message.clone()));

            trees.entry(base_oid).or_insert_with(|| {
                let newest_commit = &commits_by_oid[&entry.original_oid];
                newest_commit.tree_id()
            });
        }
    }

    Ok(SquashGroups {
        target_for,
        messages,
        trees,
    })
}

/// Create a git signature for the author / committer based on entry fields.
fn build_signature(
    entry: &ManifestEntry,
    original_commit: &Commit,
) -> Result<Signature<'static>, GiteditError> {
    let ref_date = {
        let t = original_commit.time();
        let offset = FixedOffset::east_opt(t.offset_minutes() * 60)
            .unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
        DateTime::from_timestamp(t.seconds(), 0).map(|dt| dt.with_timezone(&offset))
    };

    let parsed_dt = parse_date(&entry.date_raw, ref_date)?;
    let timestamp = parsed_dt.timestamp();
    let offset_minutes = parsed_dt.offset().local_minus_utc() / 60;
    let git_time = Time::new(timestamp, offset_minutes);

    Signature::new(&entry.author_name, &entry.author_email, &git_time).map_err(Into::into)
}

/// Build the final commit message, concatenating any squashed messages.
fn build_commit_message(entry: &ManifestEntry, squash_groups: &SquashGroups) -> String {
    let mut final_message = entry.message.clone();
    if let Some(sq_list) = squash_groups.messages.get(&entry.original_oid) {
        for (action, msg) in sq_list.iter().rev() {
            if *action == CommitAction::Squash && !msg.trim().is_empty() {
                final_message.push_str("\n\n");
                final_message.push_str(msg.trim());
            }
        }
    }
    final_message
}

/// Remap the parents of an original commit according to previous rewrites.
fn remap_parents<'a>(
    repo: &'a Repository,
    original_commit: &Commit,
    remap: &HashMap<Oid, Oid>,
) -> Result<Vec<Commit<'a>>, GiteditError> {
    let mut parents = Vec::new();
    for p_oid in original_commit.parent_ids() {
        let remapped_p = *remap.get(&p_oid).unwrap_or(&p_oid);
        if remapped_p != Oid::ZERO_SHA1 {
            parents.push(repo.find_commit(remapped_p)?);
        }
    }
    Ok(parents)
}

/// Rewrite an individual commit, returning its new Oid.
fn rewrite_single_commit(
    repo: &Repository,
    entry: &ManifestEntry,
    original_commit: &Commit,
    remap: &HashMap<Oid, Oid>,
    squash_groups: &SquashGroups,
    dry_run: bool,
) -> Result<Oid, GiteditError> {
    let tree_oid = squash_groups
        .trees
        .get(&entry.original_oid)
        .copied()
        .unwrap_or_else(|| original_commit.tree_id());
    let tree = repo.find_tree(tree_oid)?;

    let parents = remap_parents(repo, original_commit, remap)?;
    let parent_refs: Vec<&Commit> = parents.iter().collect();

    let final_message = build_commit_message(entry, squash_groups);
    let author_sig = build_signature(entry, original_commit)?;
    let committer_sig = author_sig.clone();

    if dry_run {
        Ok(original_commit.id())
    } else {
        let new_oid = repo.commit(
            None,
            &author_sig,
            &committer_sig,
            &final_message,
            &tree,
            &parent_refs,
        )?;
        Ok(new_oid)
    }
}

/// Process rewrites in topological order (oldest to newest).
fn apply_topological_rewrites(
    repo: &Repository,
    resolved_entries: &[ManifestEntry],
    commits_by_oid: &HashMap<Oid, Commit>,
    squash_groups: &SquashGroups,
    dry_run: bool,
) -> Result<(HashMap<Oid, Oid>, usize, usize, usize), GiteditError> {
    let mut topological_entries = resolved_entries.to_vec();
    topological_entries.reverse();

    let mut remap: HashMap<Oid, Oid> = HashMap::new();
    let mut commits_rewritten = 0;
    let mut commits_squashed = 0;
    let mut commits_dropped = 0;

    for entry in topological_entries {
        let original_commit = &commits_by_oid[&entry.original_oid];

        match entry.action {
            CommitAction::Drop => {
                let parent_id = if original_commit.parent_count() > 0 {
                    let p = original_commit.parent_id(0)?;
                    *remap.get(&p).unwrap_or(&p)
                } else {
                    Oid::ZERO_SHA1
                };
                remap.insert(entry.original_oid, parent_id);
                commits_dropped += 1;
            }
            CommitAction::Squash | CommitAction::Fixup => {
                commits_squashed += 1;
            }
            CommitAction::Commit => {
                let new_commit_oid = rewrite_single_commit(
                    repo,
                    &entry,
                    original_commit,
                    &remap,
                    squash_groups,
                    dry_run,
                )?;

                remap.insert(entry.original_oid, new_commit_oid);

                for (sq_oid, base_oid) in &squash_groups.target_for {
                    if *base_oid == entry.original_oid {
                        remap.insert(*sq_oid, new_commit_oid);
                    }
                }

                commits_rewritten += 1;
            }
        }
    }

    Ok((remap, commits_rewritten, commits_squashed, commits_dropped))
}

/// Update the target branch reference to new_tip and create a safety backup.
fn finalize_branch_reference(
    repo: &Repository,
    branch_name: &str,
    new_tip: Oid,
    dry_run: bool,
) -> Result<Option<String>, GiteditError> {
    if dry_run {
        return Ok(None);
    }

    let b_ref = create_backup(repo, branch_name)?;
    let branch_ref_name = format!("refs/heads/{}", branch_name);
    let mut current_branch = repo.find_reference(&branch_ref_name)?;
    current_branch.set_target(new_tip, "gitedit: rewrite history")?;

    Ok(Some(b_ref))
}

/// Execute a full rewrite plan on the specified branch.
pub fn execute_rewrite(
    repo: &Repository,
    branch_name: &str,
    plan: &RewritePlan,
) -> Result<RewriteResult, GiteditError> {
    let (resolved_entries, commits_by_oid) = validate_and_resolve_entries(repo, &plan.entries)?;
    let squash_groups = build_squash_groups(&resolved_entries, &commits_by_oid)?;

    let (remap, commits_rewritten, commits_squashed, commits_dropped) = apply_topological_rewrites(
        repo,
        &resolved_entries,
        &commits_by_oid,
        &squash_groups,
        plan.dry_run,
    )?;

    let newest_original_oid = resolved_entries[0].original_oid;
    let new_tip = *remap.get(&newest_original_oid).unwrap_or(&newest_original_oid);

    let backup_ref = finalize_branch_reference(repo, branch_name, new_tip, plan.dry_run)?;

    Ok(RewriteResult {
        commits_rewritten,
        commits_squashed,
        commits_dropped,
        new_tip,
        backup_ref,
        remap,
    })
}
