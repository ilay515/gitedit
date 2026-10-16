use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "gitedit",
    version,
    about = "Rewrite Git commit messages, authors, dates, and squash history safely"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Number of commits to load from HEAD (default: 100)
    #[arg(short = 'n', long, default_value = "100", global = true)]
    pub limit: usize,

    /// Oldest commit boundary (exclusive, e.g. HEAD~20 or commit SHA)
    #[arg(long, global = true)]
    pub from: Option<String>,

    /// Newest commit boundary (default: HEAD)
    #[arg(long, global = true)]
    pub to: Option<String>,

    /// Override $GIT_EDITOR / $EDITOR command
    #[arg(long, global = true)]
    pub editor: Option<String>,

    /// Dry run mode: preview proposed rewrite without modifying references
    #[arg(short = 'd', long, global = true)]
    pub dry_run: bool,

    /// Force execution even if working directory has uncommitted changes
    #[arg(short = 'f', long, global = true)]
    pub force: bool,

    /// Print detailed mapping of old SHA -> new SHA
    #[arg(short = 'v', long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Batch edit commits (opens $EDITOR by default)
    Batch {
        /// Positional commit range (e.g. HEAD~10..HEAD)
        range: Option<String>,

        /// Filter commits matching author name or email
        #[arg(long)]
        match_author: Option<String>,

        /// New author formatted as "Name <email@example.com>"
        #[arg(long)]
        new_author: Option<String>,

        /// Shift dates across commits by duration (e.g. "+2h", "-1d")
        #[arg(long)]
        shift_dates: Option<String>,

        /// Set specific date across commits
        #[arg(long)]
        set_date: Option<String>,

        /// Non-interactive: apply CLI flags without opening editor
        #[arg(long)]
        no_editor: bool,
    },

    /// Edit a single commit (default: HEAD)
    Edit {
        /// Commit reference to edit (default: HEAD)
        #[arg(default_value = "HEAD")]
        commit: String,

        /// New commit message
        #[arg(short = 'm', long)]
        message: Option<String>,

        /// New author formatted as "Name <email>"
        #[arg(long)]
        author: Option<String>,

        /// New author name only
        #[arg(long)]
        author_name: Option<String>,

        /// New author email only
        #[arg(long)]
        author_email: Option<String>,

        /// New date (author and committer date)
        #[arg(long)]
        date: Option<String>,

        /// Open this single commit in $EDITOR
        #[arg(short = 'e', long)]
        editor: bool,
    },

    /// Squash commits into a single commit
    Squash {
        /// Positional commit range to squash (e.g. HEAD~3..HEAD)
        range: Option<String>,

        /// Number of recent commits to squash into 1
        #[arg(short = 'n', long)]
        count: Option<usize>,

        /// Message for the squashed commit
        #[arg(short = 'm', long)]
        message: Option<String>,

        /// Keep only the base commit message, discarding squashed messages
        #[arg(long)]
        fixup: bool,
    },

    /// Undo the last gitedit rewrite operation
    Undo,

    /// List previous gitedit backups and rollback targets
    History,
}
