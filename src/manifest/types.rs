use git2::Oid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitAction {
    Commit, // Keep / edit metadata
    Squash, // Fold into commit below, concatenate messages
    Fixup,  // Fold into commit below, discard this message
    Drop,   // Remove from history, reparent children
}

impl CommitAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommitAction::Commit => "commit",
            CommitAction::Squash => "squash",
            CommitAction::Fixup => "fixup",
            CommitAction::Drop => "drop",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    pub action: CommitAction,
    pub original_oid: Oid,
    pub short_sha: String,
    pub author_name: String,
    pub author_email: String,
    pub date_raw: String,
    pub message: String,
    pub is_head: bool,
    pub branch_decorations: String,
}
