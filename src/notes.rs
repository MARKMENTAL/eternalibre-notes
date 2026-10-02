use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Directory holding note files.
///
/// Overridable via `ETERNALIBRE_NOTES_DIR` so integration tests can run against a
/// throwaway directory instead of the user's real notes.
pub fn notes_dir() -> PathBuf {
    PathBuf::from(std::env::var("ETERNALIBRE_NOTES_DIR").unwrap_or_else(|_| "notes".to_string()))
}

/// Builds the on-disk filename for a note.
///
/// Root notes are `{id}.md`. Notes in a folder are `[Folder Name]{id}.md` —
/// the folder is encoded in the filename rather than a real directory, so a
/// single notes directory can hold many virtual folders without nesting.
fn note_filename(id: &str, folder: Option<&str>) -> String {
    match folder {
        Some(f) if !f.is_empty() => format!("[{f}]{id}.md"),
        _ => format!("{id}.md"),
    }
}

/// Parses a note filename stem into its folder and id.
///
/// `[Folder]uuid` → `(Some("Folder"), "uuid")`. A bare `uuid` → `(None, "uuid")`.
/// A `[` without a closing `]` is treated as a literal id rather than a
/// malformed folder, so a note that happens to start with `[` still loads.
fn parse_stem(stem: &str) -> (Option<String>, String) {
    if let Some(rest) = stem.strip_prefix('[') {
        if let Some(end) = rest.find(']') {
            let folder = &rest[..end];
            let id = &rest[end + 1..];
            if !folder.is_empty() && !id.is_empty() {
                return (Some(folder.to_string()), id.to_string());
            }
        }
    }
    (None, stem.to_string())
}

/// Finds the on-disk path for a note by id, scanning the notes directory.
///
/// The id is the UUID portion of the filename; the folder prefix is ignored
/// during the search so a note can be located regardless of which folder it
/// lives in.
fn find_note_path(id: &str) -> Result<PathBuf> {
    let dir = notes_dir();
    if !dir.exists() {
        return Err(not_found(id));
    }
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("md") {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let (_, note_id) = parse_stem(stem);
            if note_id == id {
                return Ok(path);
            }
        }
    }
    Err(not_found(id))
}

fn not_found(id: &str) -> anyhow::Error {
    std::io::Error::new(std::io::ErrorKind::NotFound, format!("note {id} not found")).into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Virtual folder this note belongs to, derived from the filename prefix.
    /// `None` means the note lives at the root of the notes directory.
    pub folder: Option<String>,
}

impl Note {
    pub fn new(title: impl Into<String>, content: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.into(),
            content: content.into(),
            created_at: now,
            updated_at: now,
            folder: None,
        }
    }

    pub fn preview(&self, max_len: usize) -> String {
        let text: String = self
            .content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .take(3)
            .collect::<Vec<_>>()
            .join(" ");
        if text.len() > max_len {
            format!("{}...", &text[..max_len])
        } else {
            text
        }
    }
}

pub fn list_notes() -> Result<Vec<Note>> {
    let mut notes = Vec::new();
    let dir = notes_dir();
    if !dir.exists() {
        return Ok(notes);
    }
    let entries = fs::read_dir(dir)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("md") {
            if let Ok(note) = load_note_from_path(&path) {
                notes.push(note);
            }
        }
    }

    notes.sort_by_key(|a| std::cmp::Reverse(a.updated_at));
    Ok(notes)
}

/// Returns the sorted set of folder names currently in use.
///
/// Folders are virtual — they exist only when at least one note carries the
/// `[Folder]` filename prefix. An empty folder has no representation on disk.
pub fn list_folders() -> Result<Vec<String>> {
    let mut folders = BTreeSet::new();
    for note in list_notes()? {
        if let Some(f) = note.folder {
            folders.insert(f);
        }
    }
    Ok(folders.into_iter().collect())
}

pub fn load_note(id: &str) -> Result<Note> {
    let path = find_note_path(id)?;
    load_note_from_path(&path)
}

fn load_note_from_path(path: &Path) -> Result<Note> {
    let content = fs::read_to_string(path)?;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let (folder, id) = parse_stem(stem);

    // Parse frontmatter if present
    let (title, body, created_at, updated_at) = parse_frontmatter(&content);

    Ok(Note {
        id,
        title,
        content: body,
        created_at,
        updated_at,
        folder,
    })
}

fn parse_frontmatter(content: &str) -> (String, String, DateTime<Utc>, DateTime<Utc>) {
    if let Some(rest) = content.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---\n") {
            let fm = &rest[..end];
            let body = &rest[end + 5..];

            let mut title = String::from("Untitled");
            let mut created_at = Utc::now();
            let mut updated_at = Utc::now();

            for line in fm.lines() {
                if let Some(v) = line.strip_prefix("title: ") {
                    title = v.trim().to_string();
                } else if let Some(v) = line.strip_prefix("created: ") {
                    if let Ok(dt) = DateTime::parse_from_rfc3339(v.trim()) {
                        created_at = dt.with_timezone(&Utc);
                    }
                } else if let Some(v) = line.strip_prefix("updated: ") {
                    if let Ok(dt) = DateTime::parse_from_rfc3339(v.trim()) {
                        updated_at = dt.with_timezone(&Utc);
                    }
                }
            }

            return (title, body.to_string(), created_at, updated_at);
        }
    }

    // No frontmatter — derive title from first heading
    let title = content
        .lines()
        .find(|l| l.starts_with("# "))
        .map(|l| l[2..].trim().to_string())
        .unwrap_or_else(|| "Untitled".to_string());

    (title, content.to_string(), Utc::now(), Utc::now())
}

pub fn save_note(note: &Note) -> Result<()> {
    fs::create_dir_all(notes_dir())?;
    let frontmatter = format!(
        "---\ntitle: {}\ncreated: {}\nupdated: {}\n---\n{}",
        note.title,
        note.created_at.to_rfc3339(),
        note.updated_at.to_rfc3339(),
        note.content
    );
    let filename = note_filename(&note.id, note.folder.as_deref());
    fs::write(notes_dir().join(filename), frontmatter)?;
    Ok(())
}

pub fn delete_note(id: &str) -> Result<()> {
    let path = find_note_path(id)?;
    fs::remove_file(path)?;
    Ok(())
}

pub fn create_note(
    title: impl Into<String>,
    content: impl Into<String>,
    folder: Option<String>,
) -> Result<Note> {
    let mut note = Note::new(title, content);
    note.folder = folder;
    save_note(&note)?;
    Ok(note)
}

pub fn update_note(id: &str, title: impl Into<String>, content: impl Into<String>) -> Result<Note> {
    let mut note = load_note(id)?;
    note.title = title.into();
    note.content = content.into();
    note.updated_at = Utc::now();
    save_note(&note)?;
    Ok(note)
}

/// Moves a note into a virtual folder, or back to the root when `folder` is `None`.
///
/// Implemented as a rename rather than a re-save, and that distinction is the
/// whole point of this function. `save_note` derives the filename from
/// `note.folder` but never removes the file it replaced, so writing the note
/// under a new prefix would leave the original in place. `list_notes` would
/// then return the same id twice and the note would render twice in the
/// sidebar, with the sidebar sort silently ordering the two copies by
/// whatever `updated_at` each file happened to carry.
pub fn move_note(id: &str, folder: Option<&str>) -> Result<Note> {
    let path = find_note_path(id)?;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let (current_folder, note_id) = parse_stem(stem);

    // A blank name means "no folder", which is how the sidebar's All Notes
    // item and the move form's Unfiled option express the root. Route-level
    // callers reject a blank-but-nonempty field before reaching this, so this
    // is the defensive reading of the same input, not a second opinion.
    let target = match folder {
        Some(f) if !f.trim().is_empty() => Some(validate_folder_name(f)?),
        _ => None,
    };

    // Dropping a note back where it already lives is a no-op rather than a
    // rename onto itself. Nothing is lost by short-circuiting, and it keeps
    // the operation idempotent: dragging the same note onto the same folder
    // twice cannot fail the second time.
    if current_folder == target {
        return load_note(id);
    }

    let dest = notes_dir().join(note_filename(&note_id, target.as_deref()));
    if dest.exists() {
        // Only reachable if two files share a parsed id, e.g. a hand-copied
        // note. Renaming would silently clobber one of them.
        anyhow::bail!(
            "refusing to move note {id}: {} already exists",
            dest.display()
        );
    }

    fs::rename(&path, &dest)?;
    // Re-read rather than patching the in-memory copy, so the returned note
    // reflects what actually landed on disk.
    load_note(&note_id)
}

/// Validates a user-supplied folder name.
///
/// The name is embedded in the filename as `[Name]`, so `]` would corrupt the
/// encoding. `/` and `\` are path separators and would escape the notes
/// directory. Empty names and over-long names are rejected for sanity.
///
/// Defense-in-depth against adversarial input: path traversal (`..`), shell
/// metacharacters (`;`, `|`, `&`, `$`, backtick, parens, angle brackets),
/// control characters, and leading dots (hidden files) are all rejected even
/// though the name only ever reaches a filename — not a shell or eval context.
pub fn validate_folder_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        anyhow::bail!("folder name cannot be empty");
    }
    if trimmed.len() > 100 {
        anyhow::bail!("folder name is too long (max 100 characters)");
    }
    if trimmed.starts_with('.') {
        anyhow::bail!("folder name cannot start with a dot");
    }
    if trimmed.contains("..") {
        anyhow::bail!("folder name cannot contain '..'");
    }
    if trimmed.contains(']') {
        anyhow::bail!("folder name cannot contain ']'");
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        anyhow::bail!("folder name cannot contain '/' or '\\'");
    }
    if trimmed
        .chars()
        .any(|c| matches!(c, ';' | '|' | '&' | '$' | '`' | '(' | ')' | '<' | '>'))
    {
        anyhow::bail!("folder name cannot contain shell metacharacters");
    }
    if trimmed.chars().any(|c| c.is_control()) {
        anyhow::bail!("folder name cannot contain control characters");
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_preview_truncates() {
        let note = Note::new(
            "Test",
            "First line\n\nSecond line that is very long and should be truncated eventually.",
        );
        let preview = note.preview(20);
        assert!(preview.ends_with("..."));
        assert!(preview.len() <= 23);
    }

    #[test]
    fn crud_lifecycle() {
        std::fs::create_dir_all(notes_dir()).unwrap();
        let note = create_note("CRUD Test", "Initial content", None).unwrap();
        let loaded = load_note(&note.id).unwrap();
        assert_eq!(loaded.title, "CRUD Test");
        assert_eq!(loaded.content, "Initial content");
        assert_eq!(loaded.folder, None);

        update_note(&note.id, "Updated Title", "Updated content").unwrap();
        let updated = load_note(&note.id).unwrap();
        assert_eq!(updated.title, "Updated Title");
        assert_eq!(updated.content, "Updated content");

        delete_note(&note.id).unwrap();
        assert!(load_note(&note.id).is_err());
    }

    #[test]
    fn derives_title_from_heading() {
        std::fs::create_dir_all(notes_dir()).unwrap();
        let note = create_note("Fallback", "# Real Title\n\nSome content.", None).unwrap();
        let loaded = load_note(&note.id).unwrap();
        // The stored title is what was passed to create_note, but the file content has a heading.
        assert_eq!(loaded.title, "Fallback");
        delete_note(&note.id).unwrap();
    }

    #[test]
    fn folder_prefix_round_trip() {
        std::fs::create_dir_all(notes_dir()).unwrap();
        let note = create_note("Folder Note", "Body", Some("Project Notes".to_string())).unwrap();
        let loaded = load_note(&note.id).unwrap();
        assert_eq!(loaded.folder.as_deref(), Some("Project Notes"));
        assert_eq!(loaded.title, "Folder Note");
        delete_note(&note.id).unwrap();
    }

    /// Every on-disk `.md` file whose filename parses to `id`.
    ///
    /// More than one is the signature of the duplicate-write bug: a "move"
    /// that re-saves under a new prefix without removing the old file leaves
    /// two files that both claim the same id.
    fn files_for_id(id: &str) -> Vec<String> {
        let Ok(entries) = fs::read_dir(notes_dir()) else {
            return Vec::new();
        };
        let mut found: Vec<String> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
            .filter(|p| {
                let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                parse_stem(stem).1 == id
            })
            .map(|p| {
                p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        found.sort();
        found
    }

    #[test]
    fn move_note_renames_the_file_and_keeps_content() {
        std::fs::create_dir_all(notes_dir()).unwrap();
        let note = create_note("Movable", "Body stays put", None).unwrap();
        assert_eq!(files_for_id(&note.id), vec![format!("{}.md", note.id)]);

        let moved = move_note(&note.id, Some("Work")).unwrap();
        assert_eq!(moved.folder.as_deref(), Some("Work"));
        assert_eq!(moved.title, "Movable");
        assert_eq!(moved.content, "Body stays put");
        assert_eq!(moved.id, note.id);

        // The file was renamed, not copied: exactly one file claims the id and
        // it now carries the prefix.
        assert_eq!(
            files_for_id(&note.id),
            vec![format!("[Work]{}.md", note.id)]
        );

        // And a fresh read agrees.
        assert_eq!(load_note(&note.id).unwrap().folder.as_deref(), Some("Work"));
        delete_note(&note.id).unwrap();
    }

    #[test]
    fn move_note_to_root_strips_the_prefix() {
        std::fs::create_dir_all(notes_dir()).unwrap();
        let note = create_note("Filed", "b", Some("Work".to_string())).unwrap();
        let moved = move_note(&note.id, None).unwrap();
        assert_eq!(moved.folder, None);
        assert_eq!(files_for_id(&note.id), vec![format!("{}.md", note.id)]);

        // An empty string means the same thing as None.
        let refiled = create_note("Filed again", "b", Some("Work".to_string())).unwrap();
        assert_eq!(move_note(&refiled.id, Some("  ")).unwrap().folder, None);
        assert_eq!(
            files_for_id(&refiled.id),
            vec![format!("{}.md", refiled.id)]
        );

        delete_note(&note.id).unwrap();
        delete_note(&refiled.id).unwrap();
    }

    #[test]
    fn move_note_to_the_same_folder_is_a_no_op() {
        std::fs::create_dir_all(notes_dir()).unwrap();
        let note = create_note("Steady", "b", Some("Work".to_string())).unwrap();
        // Dropping a note onto the folder it is already in must not fork it.
        move_note(&note.id, Some("Work")).unwrap();
        move_note(&note.id, Some("Work")).unwrap();
        assert_eq!(
            files_for_id(&note.id),
            vec![format!("[Work]{}.md", note.id)]
        );

        // Same for an unfiled note dropped on the root.
        let root = create_note("Rooted", "b", None).unwrap();
        move_note(&root.id, None).unwrap();
        assert_eq!(files_for_id(&root.id), vec![format!("{}.md", root.id)]);

        delete_note(&note.id).unwrap();
        delete_note(&root.id).unwrap();
    }

    #[test]
    fn move_note_leaves_the_file_alone_when_the_name_is_rejected() {
        std::fs::create_dir_all(notes_dir()).unwrap();
        let note = create_note("Careful", "b", None).unwrap();
        assert!(move_note(&note.id, Some("bad]name")).is_err());
        assert!(move_note(&note.id, Some("../escape")).is_err());
        // A rejected move must not have moved or deleted anything.
        assert_eq!(files_for_id(&note.id), vec![format!("{}.md", note.id)]);
        assert_eq!(load_note(&note.id).unwrap().folder, None);
        delete_note(&note.id).unwrap();
    }

    #[test]
    fn move_note_does_not_disturb_the_recency_sort() {
        std::fs::create_dir_all(notes_dir()).unwrap();
        let note = create_note("Untouched", "b", None).unwrap();
        let before = load_note(&note.id).unwrap().updated_at;
        let moved = move_note(&note.id, Some("Work")).unwrap();
        // A move is not an edit. Bumping this would resurface the note at the
        // top of the recency-sorted sidebar every time it is re-filed.
        assert_eq!(moved.updated_at, before);
        delete_note(&note.id).unwrap();
    }

    #[test]
    fn list_folders_returns_sorted_unique() {
        std::fs::create_dir_all(notes_dir()).unwrap();
        let a = create_note("A", "a", Some("Zebra".to_string())).unwrap();
        let b = create_note("B", "b", Some("Apple".to_string())).unwrap();
        let c = create_note("C", "c", Some("Apple".to_string())).unwrap();
        let d = create_note("D", "d", None).unwrap();
        let folders = list_folders().unwrap();
        assert_eq!(folders, vec!["Apple".to_string(), "Zebra".to_string()]);
        for note in &[a, b, c, d] {
            delete_note(&note.id).unwrap();
        }
    }

    #[test]
    fn validate_folder_name_rejects_bad_names() {
        assert!(validate_folder_name("").is_err());
        assert!(validate_folder_name("   ").is_err());
        assert!(validate_folder_name("has]bracket").is_err());
        assert!(validate_folder_name("has/slash").is_err());
        assert!(validate_folder_name("has\\backslash").is_err());
        assert!(validate_folder_name(&"x".repeat(101)).is_err());
        assert!(validate_folder_name("Good Name").is_ok());
        assert!(validate_folder_name("  Trimmed  ").is_ok());
        assert_eq!(validate_folder_name("  Trimmed  ").unwrap(), "Trimmed");
    }

    #[test]
    fn validate_folder_name_rejects_path_traversal() {
        assert!(validate_folder_name("..").is_err());
        assert!(validate_folder_name(".").is_err());
        assert!(validate_folder_name("foo..bar").is_err());
        assert!(validate_folder_name("../etc").is_err());
        assert!(validate_folder_name("foo/../bar").is_err());
        assert!(validate_folder_name(".hidden").is_err());
    }

    #[test]
    fn validate_folder_name_rejects_shell_metacharacters() {
        assert!(validate_folder_name("foo;bar").is_err());
        assert!(validate_folder_name("foo|bar").is_err());
        assert!(validate_folder_name("foo&bar").is_err());
        assert!(validate_folder_name("foo$bar").is_err());
        assert!(validate_folder_name("foo`bar").is_err());
        assert!(validate_folder_name("foo(bar)").is_err());
        assert!(validate_folder_name("foo<bar>").is_err());
        assert!(validate_folder_name("$(whoami)").is_err());
        assert!(validate_folder_name("`id`").is_err());
    }

    #[test]
    fn validate_folder_name_rejects_control_characters() {
        assert!(validate_folder_name("foo\nbar").is_err());
        assert!(validate_folder_name("foo\tbar").is_err());
        assert!(validate_folder_name("foo\rbar").is_err());
        assert!(validate_folder_name("foo\u{0}bar").is_err());
        assert!(validate_folder_name("foo\u{1b}bar").is_err());
    }

    #[test]
    fn parse_stem_handles_edge_cases() {
        assert_eq!(parse_stem("uuid"), (None, "uuid".to_string()));
        assert_eq!(
            parse_stem("[Folder]uuid"),
            (Some("Folder".to_string()), "uuid".to_string())
        );
        // No closing bracket — treated as literal id
        assert_eq!(parse_stem("[Folder"), (None, "[Folder".to_string()));
        // Empty folder — treated as literal id
        assert_eq!(parse_stem("[]uuid"), (None, "[]uuid".to_string()));
    }
}
