use crate::errors::GiteditError;
use crate::manifest::parse_manifest;
use crate::manifest::types::ManifestEntry;
use colored::Colorize;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

pub fn resolve_editor(cli_override: Option<&str>) -> String {
    if let Some(cmd) = cli_override {
        return cmd.to_string();
    }
    if let Ok(cmd) = env::var("GIT_EDITOR") {
        if !cmd.trim().is_empty() {
            return cmd;
        }
    }
    if let Ok(cmd) = env::var("VISUAL") {
        if !cmd.trim().is_empty() {
            return cmd;
        }
    }
    if let Ok(cmd) = env::var("EDITOR") {
        if !cmd.trim().is_empty() {
            return cmd;
        }
    }
    "vim".to_string()
}

pub fn edit_manifest_with_retry(
    repo_git_dir: &Path,
    initial_content: &str,
    editor_cmd: &str,
) -> Result<Vec<ManifestEntry>, GiteditError> {
    let editmsg_path = repo_git_dir.join("GITEDIT_EDITMSG");
    fs::write(&editmsg_path, initial_content)?;

    loop {
        // Launch editor
        let parts: Vec<&str> = editor_cmd.split_whitespace().collect();
        let (bin, args) = match parts.split_first() {
            Some((first, rest)) => (*first, rest),
            None => ("vim", &[][..]),
        };

        let status = Command::new(bin)
            .args(args)
            .arg(&editmsg_path)
            .status()
            .map_err(|e| GiteditError::Other(format!("Failed to spawn editor '{}': {}", editor_cmd, e)))?;

        if !status.success() {
            let _ = fs::remove_file(&editmsg_path);
            return Err(GiteditError::Aborted);
        }

        let edited = fs::read_to_string(&editmsg_path)?;
        if edited.trim().is_empty() {
            let _ = fs::remove_file(&editmsg_path);
            println!("{}", "Empty manifest or all commits removed. Aborting.".yellow());
            return Err(GiteditError::Aborted);
        }

        match parse_manifest(&edited) {
            Ok(entries) => {
                let _ = fs::remove_file(&editmsg_path);
                return Ok(entries);
            }
            Err(e) => {
                eprintln!("\n{} {}", "Error parsing manifest:".red().bold(), e);
                print!("{}", "Would you like to [R]e-edit or [A]bort? (r/a): ".yellow().bold());
                io::stdout().flush().unwrap();

                let mut input = String::new();
                if io::stdin().read_line(&mut input).is_err() || input.trim().to_lowercase().starts_with('a') {
                    let _ = fs::remove_file(&editmsg_path);
                    return Err(GiteditError::Aborted);
                }
                // Otherwise loop and re-open editor with editmsg_path preserved
            }
        }
    }
}
