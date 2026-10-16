use crate::errors::GiteditError;
use crate::git::backup::list_backups;
use crate::git::range::format_short_sha;
use crate::git::repo::RepoContext;
use colored::Colorize;

pub fn run() -> Result<(), GiteditError> {
    let ctx = RepoContext::open(".")?;
    let backups = list_backups(&ctx.repo)?;

    if backups.is_empty() {
        println!("No previous gitedit backups found.");
        return Ok(());
    }

    println!("{}", "gitedit Backup History:".bold());
    println!("{:<25} {:<40} {:<10}", "Timestamp", "Backup Ref", "Target SHA");
    println!("{:-<75}", "");

    for b in backups {
        println!(
            "{:<25} {:<40} {}",
            b.timestamp_str.cyan(),
            b.ref_name,
            format_short_sha(b.target_oid).green()
        );
    }

    Ok(())
}
