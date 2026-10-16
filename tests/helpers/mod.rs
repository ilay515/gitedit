use git2::{Commit, Repository, Signature, Time};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

pub fn create_test_repo(num_commits: usize) -> (TempDir, Repository) {
    let dir = TempDir::new().unwrap();
    let repo = Repository::init(dir.path()).unwrap();

    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Test User").unwrap();
    config.set_str("user.email", "test@example.com").unwrap();

    for i in 1..=num_commits {
        let file_path = dir.path().join(format!("file_{}.txt", i));
        fs::write(&file_path, format!("content for commit {}\n", i)).unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new(&format!("file_{}.txt", i))).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();

        let sig = Signature::new("Test User", "test@example.com", &Time::new(1700000000 + (i as i64 * 3600), 180)).unwrap();

        let parent_commits = if i == 1 {
            vec![]
        } else {
            let head = repo.head().unwrap().peel_to_commit().unwrap();
            vec![head]
        };
        let parent_refs: Vec<&Commit> = parent_commits.iter().collect();

        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            &format!("commit message {}", i),
            &tree,
            &parent_refs,
        ).unwrap();
    }

    (dir, repo)
}
