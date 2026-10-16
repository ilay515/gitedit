use crate::errors::GiteditError;
use crate::git::backup::restore_latest_backup;
use crate::git::range::format_short_sha;
use crate::git::repo::RepoContext;
use colored::Colorize;

pub fn run() -> Result<(), GiteditError> {
    let ctx = RepoContext::open(".")?;
    let (backup_ref, old_tip, restored_tip) = restore_latest_backup(&ctx.repo, &ctx.branch_name)?;

    println!("{}", "✓ Successfully rolled back last gitedit rewrite!".green().bold());
    println!("Restored branch '{}' from {}", ctx.branch_name.cyan(), backup_ref.bold());
    println!("Branch tip: {} -> {}", format_short_sha(old_tip), format_short_sha(restored_tip).green().bold());

    Ok(())
}
