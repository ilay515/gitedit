mod helpers;
use helpers::create_test_repo;
use gitedit::git::range::resolve_range;
use gitedit::git::rewrite::{execute_rewrite, RewritePlan};
use gitedit::manifest::{format_manifest, parse_manifest};
use gitedit::manifest::types::{CommitAction, ManifestEntry};

#[test]
fn test_edit_single_commit_metadata() {
    let (_dir, repo) = create_test_repo(3);
    let commits = resolve_range(&repo, None, None, 10).unwrap();
    assert_eq!(commits.len(), 3);

    // Commit 2 is commits[1] in git log order
    let target = &commits[1];
    let original_tree_id = target.tree_id();

    let entries = vec![
        ManifestEntry {
            action: CommitAction::Commit,
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
            author_name: "New Author".to_string(),
            author_email: "newauthor@example.com".to_string(),
            date_raw: "2026-01-01 10:00:00 +00:00".to_string(),
            message: "edited commit message 2".to_string(),
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
    assert_eq!(result.commits_rewritten, 3);

    let updated_commits = resolve_range(&repo, None, None, 10).unwrap();
    assert_eq!(updated_commits.len(), 3);

    let edited_commit = &updated_commits[1];
    assert_eq!(edited_commit.message().unwrap(), "edited commit message 2");
    assert_eq!(edited_commit.author().name().unwrap(), "New Author");
    assert_eq!(edited_commit.author().email().unwrap(), "newauthor@example.com");
    // Verify tree is identical!
    assert_eq!(edited_commit.tree_id(), original_tree_id);
}

#[test]
fn test_roundtrip_through_manifest_format_and_parse() {
    let (_dir, repo) = create_test_repo(2);
    let commits = resolve_range(&repo, None, None, 10).unwrap();

    let entries = vec![
        ManifestEntry {
            action: CommitAction::Commit,
            original_oid: commits[0].id(),
            short_sha: commits[0].id().to_string()[..7].to_string(),
            author_name: "Original 2".to_string(),
            author_email: "orig2@example.com".to_string(),
            date_raw: "2026-10-16 12:00:00 +00:00".to_string(),
            message: "commit message 2".to_string(),
            is_head: true,
            branch_decorations: "(HEAD -> master)".to_string(),
        },
        ManifestEntry {
            action: CommitAction::Commit,
            original_oid: commits[1].id(),
            short_sha: commits[1].id().to_string()[..7].to_string(),
            author_name: "Original 1".to_string(),
            author_email: "orig1@example.com".to_string(),
            date_raw: "2026-10-15 12:00:00 +00:00".to_string(),
            message: "commit message 1".to_string(),
            is_head: false,
            branch_decorations: "".to_string(),
        },
    ];

    // Format to string (which writes short 7-char sha)
    let text = format_manifest(&entries);
    // Parse it back like Vim editor output
    let parsed_entries = parse_manifest(&text).unwrap();

    // Execute rewrite using the parsed entries
    let plan = RewritePlan {
        entries: parsed_entries,
        dry_run: false,
    };

    let result = execute_rewrite(&repo, "master", &plan).unwrap();
    assert_eq!(result.commits_rewritten, 2);
}

#[test]
fn test_no_changes_detected_on_identical_manifest() {
    use gitedit::manifest::has_manifest_changes;

    let (_dir, repo) = create_test_repo(2);
    let commits = resolve_range(&repo, None, None, 10).unwrap();

    let mut entries = Vec::new();
    for c in &commits {
        let t = c.time();
        let offset = chrono::FixedOffset::east_opt(t.offset_minutes() * 60).unwrap();
        let dt = chrono::DateTime::from_timestamp(t.seconds(), 0).unwrap().with_timezone(&offset);

        entries.push(ManifestEntry {
            action: CommitAction::Commit,
            original_oid: c.id(),
            short_sha: c.id().to_string()[..7].to_string(),
            author_name: c.author().name().unwrap().to_string(),
            author_email: c.author().email().unwrap().to_string(),
            date_raw: dt.format("%Y-%m-%d %H:%M:%S %:z").to_string(),
            message: c.message().unwrap().to_string(),
            is_head: false,
            branch_decorations: String::new(),
        });
    }

    // Format then parse (simulating user opening editor and immediately saving without changes)
    let formatted = format_manifest(&entries);
    let parsed = parse_manifest(&formatted).unwrap();

    assert!(!has_manifest_changes(&parsed, &commits));

    // Now modify one character in author name
    let mut modified = parsed.clone();
    modified[0].author_name.push('!');
    assert!(has_manifest_changes(&modified, &commits));

    // Or modify action to Squash
    let mut modified_action = parsed.clone();
    modified_action[0].action = CommitAction::Squash;
    assert!(has_manifest_changes(&modified_action, &commits));
}
