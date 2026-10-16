use crate::errors::GiteditError;
use chrono::Utc;
use git2::{Oid, Repository};

pub struct BackupEntry {
    pub ref_name: String,
    pub target_oid: Oid,
    pub timestamp_str: String,
}

pub fn create_backup(repo: &Repository, branch_name: &str) -> Result<String, GiteditError> {
    let head_commit = repo.head()?.peel_to_commit()?;
    let head_oid = head_commit.id();

    let now_str = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let backup_ref_name = format!("refs/gitedit/backups/{}_{}", now_str, branch_name);

    repo.reference(&backup_ref_name, head_oid, true, "gitedit: create backup")?;

    // Also update ORIG_HEAD
    let _ = repo.reference("ORIG_HEAD", head_oid, true, "gitedit: set ORIG_HEAD");

    Ok(backup_ref_name)
}

pub fn list_backups(repo: &Repository) -> Result<Vec<BackupEntry>, GiteditError> {
    let mut entries = Vec::new();
    let pattern = "refs/gitedit/backups/*";
    let refs = repo.references_glob(pattern)?;

    for r_res in refs {
        let r = r_res?;
        if let (Ok(name), Some(target)) = (r.name(), r.target()) {
            let parts: Vec<&str> = name.split('/').collect();
            let timestamp_str = parts.last().unwrap_or(&"unknown").to_string();
            entries.push(BackupEntry {
                ref_name: name.to_string(),
                target_oid: target,
                timestamp_str,
            });
        }
    }

    // Sort newest first
    entries.sort_by(|a, b| b.ref_name.cmp(&a.ref_name));
    Ok(entries)
}

pub fn restore_latest_backup(repo: &Repository, branch_name: &str) -> Result<(String, Oid, Oid), GiteditError> {
    let backups = list_backups(repo)?;
    let latest = backups.into_iter().next().ok_or(GiteditError::NoBackup)?;

    let current_branch_ref = format!("refs/heads/{}", branch_name);
    let mut current_ref = repo.find_reference(&current_branch_ref)?;
    let old_tip = current_ref.target().unwrap_or(Oid::ZERO_SHA1);

    current_ref.set_target(latest.target_oid, "gitedit: undo rewrite")?;

    // Delete the backup ref so consecutive undos step backwards
    let mut backup_ref = repo.find_reference(&latest.ref_name)?;
    backup_ref.delete()?;

    Ok((latest.ref_name, old_tip, latest.target_oid))
}
