pub mod types;
pub mod format;
pub mod parse;
pub mod diff;
pub mod entry;

pub use format::format_manifest;
pub use parse::parse_manifest;
pub use diff::has_manifest_changes;
pub use entry::{commit_to_entry, parse_author_spec};
