use crate::errors::GiteditError;
use git2::{Commit, Oid, Repository, Sort};

/// Resolves commits to edit in `git log` order (newest/HEAD at index 0, older commits below).
pub fn resolve_range<'a>(
    repo: &'a Repository,
    from: Option<&str>,
    to: Option<&str>,
    limit: usize,
) -> Result<Vec<Commit<'a>>, GiteditError> {
    let to_oid = match to {
        Some(t) => repo.revparse_single(t)?.peel_to_commit()?.id(),
        None => repo.head()?.peel_to_commit()?.id(),
    };

    let from_oid = match from {
        Some(f) => Some(repo.revparse_single(f)?.peel_to_commit()?.id()),
        None => None,
    };

    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    revwalk.push(to_oid)?;

    if let Some(f_oid) = from_oid {
        revwalk.hide(f_oid)?;
    }

    let mut commits = Vec::new();
    for oid_res in revwalk {
        let oid = oid_res?;
        let commit = repo.find_commit(oid)?;
        commits.push(commit);
        if commits.len() >= limit {
            break;
        }
    }

    if commits.is_empty() {
        return Err(GiteditError::EmptyRange);
    }

    Ok(commits)
}

pub fn is_merge_commit(commit: &Commit) -> bool {
    commit.parent_count() > 1
}

pub fn format_short_sha(oid: Oid) -> String {
    let s = oid.to_string();
    if s.len() >= 7 {
        s[..7].to_string()
    } else {
        s
    }
}
