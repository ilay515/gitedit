mod helpers;
use helpers::create_test_repo;
use gitedit::git::range::resolve_range;
use gitedit::git::rewrite::{execute_rewrite, RewritePlan};
use gitedit::manifest::types::{CommitAction, ManifestEntry};

#[test]
fn test_squash_two_commits() {
    let (_dir, repo) = create_test_repo(3);
    let commits = resolve_range(&repo, None, None, 10).unwrap();
    assert_eq!(commits.len(), 3);

    // In git log order:
    // commits[0] is Commit 3
    // commits[1] is Commit 2
    // commits[2] is Commit 1
    // Squash Commit 3 into Commit 2:
    let entries = vec![
        ManifestEntry {
            action: CommitAction::Squash,
            original_oid: commits[0].id(),
            short_sha: commits[0].id().to_string()[..7].to_string(),
            author_name: "Original 3".to_string(),
            author_email: "orig3@example.com".to_string(),
            date_raw: "2026-10-16 12:00:00 +00:00".to_string(),
            message: "commit message 3".to_string(),
            is_head: true,
            branch_decorations: "(HEAD -> master)".to_string(),
        },
        ManifestEntry {
            action: CommitAction::Commit,
            original_oid: commits[1].id(),
            short_sha: commits[1].id().to_string()[..7].to_string(),
            author_name: "Original 2".to_string(),
            author_email: "orig2@example.com".to_string(),
            date_raw: "2026-10-15 12:00:00 +00:00".to_string(),
            message: "commit message 2".to_string(),
            is_head: false,
            branch_decorations: "".to_string(),
        },
        ManifestEntry {
            action: CommitAction::Commit,
            original_oid: commits[2].id(),
            short_sha: commits[2].id().to_string()[..7].to_string(),
            author_name: "Original 1".to_string(),
            author_email: "orig1@example.com".to_string(),
            date_raw: "2026-10-14 12:00:00 +00:00".to_string(),
            message: "commit message 1".to_string(),
            is_head: false,
            branch_decorations: "".to_string(),
        },
    ];

    let plan = RewritePlan {
        entries,
        dry_run: false,
    };

    let result = execute_rewrite(&repo, "master", &plan).unwrap();
    assert_eq!(result.commits_squashed, 1);
    assert_eq!(result.commits_rewritten, 2);

    let updated_commits = resolve_range(&repo, None, None, 10).unwrap();
    assert_eq!(updated_commits.len(), 2);

    let combined = &updated_commits[0];
    let msg = combined.message().unwrap();
    assert!(msg.contains("commit message 2"));
    assert!(msg.contains("commit message 3"));
    // Combined commit tree must equal the squashed child (Commit 3's tree)
    assert_eq!(combined.tree_id(), commits[0].tree_id());
}
