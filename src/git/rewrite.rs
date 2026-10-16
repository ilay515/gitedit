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

pub fn execute_rewrite(
    repo: &Repository,
    branch_name: &str,
    plan: &RewritePlan,
) -> Result<RewriteResult, GiteditError> {
    if plan.entries.is_empty() {
        return Err(GiteditError::EmptyRange);
    }

    // 1. Verify merge commits and boundaries:
    // Entries are in git log order (index 0 is newest/HEAD, last index is oldest)
    let n = plan.entries.len();
    if plan.entries[n - 1].action == CommitAction::Squash || plan.entries[n - 1].action == CommitAction::Fixup {
        return Err(GiteditError::SquashAtBoundary);
    }

    // Map short_sha or original_oid to commit objects
    let mut commits_by_oid: HashMap<Oid, Commit> = HashMap::new();
    let mut resolved_entries: Vec<ManifestEntry> = Vec::new();

    for entry in &plan.entries {
        // Find commit in repo by full oid or short_sha prefix
        let commit = if entry.original_oid != Oid::ZERO_SHA1 {
            match repo.find_commit(entry.original_oid) {
                Ok(c) => c,
                Err(_) => repo.revparse_single(&entry.short_sha)?.peel_to_commit()?,
            }
        } else {
            repo.revparse_single(&entry.short_sha)?.peel_to_commit()?
        };

        if (entry.action == CommitAction::Squash || entry.action == CommitAction::Fixup || entry.action == CommitAction::Drop)
            && is_merge_commit(&commit)
        {
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

    // 2. Pre-process squash groups
    let mut squash_target_for: HashMap<Oid, Oid> = HashMap::new();
    let mut squash_messages: HashMap<Oid, Vec<(CommitAction, String)>> = HashMap::new();
    let mut squash_trees: HashMap<Oid, Oid> = HashMap::new();

    let mut i = 0;
    while i < resolved_entries.len() {
        let entry = &resolved_entries[i];
        if entry.action == CommitAction::Squash || entry.action == CommitAction::Fixup {
            // Find base commit below
            let mut target_oid = None;
            for candidate in resolved_entries.iter().skip(i + 1) {
                if candidate.action == CommitAction::Commit {
                    target_oid = Some(candidate.original_oid);
                    break;
                }
            }

            let base_oid = target_oid.ok_or(GiteditError::SquashAtBoundary)?;

            squash_target_for.insert(entry.original_oid, base_oid);
            squash_messages
                .entry(base_oid)
                .or_default()
                .push((entry.action, entry.message.clone()));

            squash_trees.entry(base_oid).or_insert_with(|| {
                let newest_commit = &commits_by_oid[&entry.original_oid];
                newest_commit.tree_id()
            });
        }
        i += 1;
    }

    // 3. Collect all commits that need rewriting in topological order (oldest first)
    let mut topological_entries = resolved_entries.clone();
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
                let tree_oid = if let Some(squashed_tree) = squash_trees.get(&entry.original_oid) {
                    *squashed_tree
                } else {
                    original_commit.tree_id()
                };
                let tree = repo.find_tree(tree_oid)?;

                // Remap parents
                let mut parents = Vec::new();
                for p_oid in original_commit.parent_ids() {
                    let remapped_p = *remap.get(&p_oid).unwrap_or(&p_oid);
                    if remapped_p != Oid::ZERO_SHA1 {
                        parents.push(repo.find_commit(remapped_p)?);
                    }
                }
                let parent_refs: Vec<&Commit> = parents.iter().collect();

                // Compute final message
                let mut final_message = entry.message.clone();
                if let Some(sq_list) = squash_messages.get(&entry.original_oid) {
                    for (action, msg) in sq_list.iter().rev() {
                        if *action == CommitAction::Squash && !msg.trim().is_empty() {
                            final_message.push_str("\n\n");
                            final_message.push_str(msg.trim());
                        }
                    }
                }

                // Parse author & date
                let ref_date = {
                    let t = original_commit.time();
                    let offset = FixedOffset::east_opt(t.offset_minutes() * 60).unwrap_or(FixedOffset::east_opt(0).unwrap());
                    DateTime::from_timestamp(t.seconds(), 0).map(|dt| dt.with_timezone(&offset))
                };

                let parsed_dt = parse_date(&entry.date_raw, ref_date)?;
                let timestamp = parsed_dt.timestamp();
                let offset_minutes = parsed_dt.offset().local_minus_utc() / 60;
                let git_time = Time::new(timestamp, offset_minutes);

                let author_sig = Signature::new(
                    &entry.author_name,
                    &entry.author_email,
                    &git_time,
                )?;
                let committer_sig = author_sig.clone();

                let new_commit_oid = if plan.dry_run {
                    original_commit.id()
                } else {
                    repo.commit(
                        None,
                        &author_sig,
                        &committer_sig,
                        &final_message,
                        &tree,
                        &parent_refs,
                    )?
                };

                remap.insert(entry.original_oid, new_commit_oid);

                for (sq_oid, base_oid) in &squash_target_for {
                    if *base_oid == entry.original_oid {
                        remap.insert(*sq_oid, new_commit_oid);
                    }
                }

                commits_rewritten += 1;
            }
        }
    }

    let newest_original_oid = resolved_entries[0].original_oid;
    let new_tip = *remap.get(&newest_original_oid).unwrap_or(&newest_original_oid);

    let mut backup_ref = None;
    if !plan.dry_run {
        let b_ref = create_backup(repo, branch_name)?;
        backup_ref = Some(b_ref);

        let branch_ref_name = format!("refs/heads/{}", branch_name);
        let mut b_ref = repo.find_reference(&branch_ref_name)?;
        b_ref.set_target(new_tip, "gitedit: rewrite history")?;
    }

    Ok(RewriteResult {
        commits_rewritten,
        commits_squashed,
        commits_dropped,
        new_tip,
        backup_ref,
        remap,
    })
}
