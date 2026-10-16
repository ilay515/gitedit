mod helpers;
use helpers::create_test_repo;
use gitedit::git::backup::restore_latest_backup;
use gitedit::git::range::resolve_range;
use gitedit::git::rewrite::{execute_rewrite, RewritePlan};
use gitedit::manifest::types::{CommitAction, ManifestEntry};

#[test]
fn test_undo_rollback() {
    let (_dir, repo) = create_test_repo(3);
    let original_commits = resolve_range(&repo, None, None, 10).unwrap();
    let original_tip = original_commits[0].id();

    let entries = vec![
        ManifestEntry {
            action: CommitAction::Commit,
            original_oid: original_commits[0].id(),
            short_sha: original_commits[0].id().to_string()[..7].to_string(),
            author_name: "Rewritten Author".to_string(),
            author_email: "rewritten@example.com".to_string(),
            date_raw: "2026-10-16 12:00:00 +00:00".to_string(),
            message: "rewritten head message".to_string(),
            is_head: true,
            branch_decorations: "(HEAD -> master)".to_string(),
        },
        ManifestEntry {
            action: CommitAction::Commit,
            original_oid: original_commits[1].id(),
            short_sha: original_commits[1].id().to_string()[..7].to_string(),
            author_name: "Author 2".to_string(),
            author_email: "auth2@example.com".to_string(),
            date_raw: "2026-10-15 12:00:00 +00:00".to_string(),
            message: "message 2".to_string(),
            is_head: false,
            branch_decorations: "".to_string(),
        },
        ManifestEntry {
            action: CommitAction::Commit,
            original_oid: original_commits[2].id(),
            short_sha: original_commits[2].id().to_string()[..7].to_string(),
            author_name: "Author 1".to_string(),
            author_email: "auth1@example.com".to_string(),
            date_raw: "2026-10-14 12:00:00 +00:00".to_string(),
            message: "message 1".to_string(),
            is_head: false,
            branch_decorations: "".to_string(),
        },
    ];

    let plan = RewritePlan {
        entries,
        dry_run: false,
    };

    let result = execute_rewrite(&repo, "master", &plan).unwrap();
    assert_ne!(result.new_tip, original_tip);

    // Rollback using restore_latest_backup
    let (_, _old_tip, restored_tip) = restore_latest_backup(&repo, "master").unwrap();
    assert_eq!(restored_tip, original_tip);

    let head_after_undo = repo.head().unwrap().peel_to_commit().unwrap().id();
    assert_eq!(head_after_undo, original_tip);
}
