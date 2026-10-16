mod cli;
mod commands;
mod date_parser;
mod editor;
mod errors;
mod git;
mod manifest;

use clap::Parser;
use cli::{Cli, Commands};
use colored::Colorize;
use commands::edit::EditArgs;
use std::process;

fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        None => commands::batch::run(
            &cli,
            None,
            None,
            None,
            None,
            None,
            false,
        ),
        Some(Commands::Batch {
            range,
            match_author,
            new_author,
            shift_dates,
            set_date,
            no_editor,
        }) => commands::batch::run(
            &cli,
            range.as_deref(),
            match_author.as_deref(),
            new_author.as_deref(),
            shift_dates.as_deref(),
            set_date.as_deref(),
            *no_editor,
        ),
        Some(Commands::Edit {
            commit,
            message,
            author,
            author_name,
            author_email,
            date,
            editor,
        }) => commands::edit::run(
            &cli,
            EditArgs {
                commit_spec: commit,
                message: message.as_deref(),
                author: author.as_deref(),
                author_name: author_name.as_deref(),
                author_email: author_email.as_deref(),
                date: date.as_deref(),
                open_editor: *editor,
            },
        ),
        Some(Commands::Squash {
            range,
            count,
            message,
            fixup,
        }) => commands::squash::run(
            &cli,
            range.as_deref(),
            *count,
            message.as_deref(),
            *fixup,
        ),
        Some(Commands::Undo) => commands::undo::run(),
        Some(Commands::History) => commands::history::run(),
    };

    if let Err(e) = result {
        eprintln!("{} {}", "error:".red().bold(), e);
        process::exit(1);
    }
}
