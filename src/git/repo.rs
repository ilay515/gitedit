use crate::errors::GiteditError;
use git2::{BranchType, Repository, StatusOptions};
use std::path::Path;

pub struct RepoContext {
    pub repo: Repository,
    pub branch_name: String,
    pub head_oid: git2::Oid,
}

impl RepoContext {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, GiteditError> {
        let repo = Repository::open(path)?;

        let (branch_name, head_oid) = {
            let head = repo.head().map_err(|e| GiteditError::Other(format!("Failed to get HEAD: {}", e)))?;
            let oid = head.target().ok_or_else(|| {
                GiteditError::Other("HEAD is not pointing to a direct commit (symbolic or detached without target)".into())
            })?;

            if !head.is_branch() {
                return Err(GiteditError::Other(
                    "You are currently in a detached HEAD state. Please checkout a branch before running gitedit.".into()
                ));
            }

            let name = head.shorthand().unwrap_or("HEAD").to_string();
            (name, oid)
        };

        Ok(Self {
            repo,
            branch_name,
            head_oid,
        })
    }

    pub fn check_dirty(&self, force: bool) -> Result<(), GiteditError> {
        if force {
            return Ok(());
        }

        let mut opts = StatusOptions::new();
        opts.include_untracked(false);
        opts.include_ignored(false);

        let statuses = self.repo.statuses(Some(&mut opts))?;
        if !statuses.is_empty() {
            return Err(GiteditError::DirtyWorkingTree);
        }

        Ok(())
    }

    pub fn has_upstream(&self) -> bool {
        if let Ok(branch) = self.repo.find_branch(&self.branch_name, BranchType::Local) {
            branch.upstream().is_ok()
        } else {
            false
        }
    }
}
