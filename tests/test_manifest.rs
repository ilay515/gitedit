use gitedit::manifest::format_manifest;
use gitedit::manifest::parse_manifest;
use gitedit::manifest::types::{CommitAction, ManifestEntry};
use git2::Oid;

#[test]
fn test_manifest_roundtrip() {
    let entries = vec![
        ManifestEntry {
            action: CommitAction::Commit,
            original_oid: Oid::from_str("1111111111111111111111111111111111111111").unwrap(),
            short_sha: "1111111".to_string(),
            author_name: "Alice Smith".to_string(),
            author_email: "alice@example.com".to_string(),
            date_raw: "2026-10-16 12:00:00 +00:00".to_string(),
            message: "feat: implement first feature\n\nDetailed description here.".to_string(),
            is_head: true,
            branch_decorations: "(HEAD -> main)".to_string(),
        },
        ManifestEntry {
            action: CommitAction::Squash,
            original_oid: Oid::from_str("2222222222222222222222222222222222222222").unwrap(),
            short_sha: "2222222".to_string(),
            author_name: "Bob Jones".to_string(),
            author_email: "bob@example.com".to_string(),
            date_raw: "2026-10-15 11:00:00 +00:00".to_string(),
            message: "fix: quick bug fix".to_string(),
            is_head: false,
            branch_decorations: "".to_string(),
        },
    ];

    let formatted = format_manifest(&entries);
    let parsed = parse_manifest(&formatted).unwrap();

    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].short_sha, "1111111");
    assert_eq!(parsed[0].action, CommitAction::Commit);
    assert_eq!(parsed[0].author_name, "Alice Smith");
    assert_eq!(parsed[0].author_email, "alice@example.com");
    assert_eq!(parsed[0].message, "feat: implement first feature\n\nDetailed description here.");

    assert_eq!(parsed[1].short_sha, "2222222");
    assert_eq!(parsed[1].action, CommitAction::Squash);
    assert_eq!(parsed[1].author_name, "Bob Jones");
    assert_eq!(parsed[1].author_email, "bob@example.com");
    assert_eq!(parsed[1].message, "fix: quick bug fix");
}
