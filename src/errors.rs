use thiserror::Error;

#[derive(Error, Debug)]
pub enum GiteditError {
    #[error("Repository has uncommitted changes. Use --force to bypass.")]
    DirtyWorkingTree,

    #[error("No commits found in the specified range")]
    EmptyRange,

    #[error("Manifest parse error at line {line}: {message}")]
    ManifestParse { line: usize, message: String },

    #[error("Invalid date format '{input}': {reason}")]
    InvalidDate { input: String, reason: String },

    #[error("Invalid author format '{input}': expected 'Name <email>'")]
    InvalidAuthor { input: String },

    #[error("Cannot {action} merge commit {sha} (has {parent_count} parents). Use git rebase --rebase-merges instead.")]
    MergeCommitUnsupported {
        action: String,
        sha: String,
        parent_count: usize,
    },

    #[error("Cannot squash the bottom-most commit in the range (nothing below to squash into)")]
    SquashAtBoundary,

    #[error("No backup found. Nothing to undo.")]
    NoBackup,

    #[error("Operation aborted by user.")]
    Aborted,

    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}
