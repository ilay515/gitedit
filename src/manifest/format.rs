use crate::manifest::types::ManifestEntry;

pub fn format_manifest(entries: &[ManifestEntry]) -> String {
    let mut out = String::new();
    out.push_str("# gitedit — Batch History Editor (Newest/HEAD at top, oldest at bottom)\n");
    out.push_str("#\n");
    out.push_str("# Actions (change the keyword before the SHA):\n");
    out.push_str("#   commit <sha>  — keep and edit this commit\n");
    out.push_str("#   squash <sha>  — fold into commit BELOW (combine messages)\n");
    out.push_str("#   s <sha>       — alias for squash\n");
    out.push_str("#   fixup <sha>   — fold into commit BELOW (discard this message)\n");
    out.push_str("#   f <sha>       — alias for fixup\n");
    out.push_str("#   drop <sha>    — remove this commit from history\n");
    out.push_str("#   d <sha>       — alias for drop\n");
    out.push_str("#\n");
    out.push_str("# Fields:\n");
    out.push_str("#   Author: Name <email>         (syncs Committer automatically)\n");
    out.push_str("#   Date:   YYYY-MM-DD HH:MM:SS +ZZ:ZZ  (or relative like +2h, -1d, yesterday)\n");
    out.push_str("#   Message: indented with 4 spaces below Date\n");
    out.push_str("#\n");
    out.push_str("# Empty file or commenting out all commits will abort.\n");
    out.push_str("# vim: set ft=gitcommit:\n\n");

    for entry in entries {
        let action_str = entry.action.as_str();
        if !entry.branch_decorations.is_empty() {
            out.push_str(&format!("{} {} {}\n", action_str, entry.short_sha, entry.branch_decorations));
        } else {
            out.push_str(&format!("{} {}\n", action_str, entry.short_sha));
        }

        out.push_str(&format!("Author: {} <{}>\n", entry.author_name, entry.author_email));
        out.push_str(&format!("Date:   {}\n\n", entry.date_raw));

        for line in entry.message.lines() {
            if line.trim().is_empty() {
                out.push_str("    \n");
            } else {
                out.push_str(&format!("    {}\n", line));
            }
        }
        out.push('\n');
    }

    out
}
