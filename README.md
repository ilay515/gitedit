# gitedit

A fast, safe, and intuitive Rust CLI for rewriting Git history. Modify commit messages, authors, dates, and squash commits directly in your `$EDITOR` or via scriptable CLI flags—without merge conflicts or unnecessary working directory checkouts.

## Features

- **Vim / `$EDITOR` Batch Manifest**: Opens a clean manifest in `git log` order (newest/HEAD at top). Modify messages, authors, dates, or use Vim search-and-replace (`:%s/old@work.com/new@work.com/g`) across up to 100 commits at once.
- **Commit Squashing & Fixup**: Fold commits into their parents by changing `commit` to `squash` (or `s`) / `fixup` (or `f`), or use `gitedit squash -n <COUNT>`.
- **Automatic Committer Sync**: Modifying Author or Date automatically keeps Committer name, email, and timestamp in sync.
- **Intuitive & Specific Date Engine**: Supports ISO 8601 (`2026-10-16 14:30:00 +03:00`), Unix epochs (`@1760613600`), relative shifts (`+2h`, `-1d`), and natural phrases (`"yesterday 15:00"`).
- **Safety First & Instant Rollback**: Creates automatic backup refs (`refs/gitedit/backups/...`) and supports one-command `gitedit undo`.

## Installation

```bash
cargo build --release
# Binary available at target/release/gitedit
```

## Quick Start

```bash
# 1. Open recent 100 commits in $EDITOR (e.g. Vim)
gitedit

# 2. Edit a single commit's message, author, or date
gitedit edit HEAD~1 --message "docs: update readme" --author "Jane Doe <jane@example.com>" --date "yesterday 14:00"

# 3. Squash the last 3 commits into 1
gitedit squash -n 3 -m "feat: complete user authentication flow"

# 4. Batch rewrite author without opening editor
gitedit batch --match-author "personal@email.com" --new-author "Work Name <work@company.com>" --no-editor

# 5. Rollback any operation
gitedit undo

# 6. View backup history
gitedit history
```

## License

MIT OR Apache-2.0
