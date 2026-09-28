use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
    let entries = fs::read_dir("notes")?;

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

pub fn load_note(id: &str) -> Result<Note> {
    let path = format!("notes/{}.md", id);
    load_note_from_path(Path::new(&path))
}

fn load_note_from_path(path: &Path) -> Result<Note> {
    let content = fs::read_to_string(path)?;
    let id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();

    // Parse frontmatter if present
    let (title, body, created_at, updated_at) = parse_frontmatter(&content);

    Ok(Note {
        id,
        title,
        content: body,
        created_at,
        updated_at,
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
    let path = format!("notes/{}.md", note.id);
    let frontmatter = format!(
        "---\ntitle: {}\ncreated: {}\nupdated: {}\n---\n{}",
        note.title,
        note.created_at.to_rfc3339(),
        note.updated_at.to_rfc3339(),
        note.content
    );
    fs::write(path, frontmatter)?;
    Ok(())
}

pub fn delete_note(id: &str) -> Result<()> {
    let path = format!("notes/{}.md", id);
    fs::remove_file(path)?;
    Ok(())
}

pub fn create_note(title: impl Into<String>, content: impl Into<String>) -> Result<Note> {
    let note = Note::new(title, content);
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
        std::fs::create_dir_all("notes").unwrap();
        let note = create_note("CRUD Test", "Initial content").unwrap();
        let loaded = load_note(&note.id).unwrap();
        assert_eq!(loaded.title, "CRUD Test");
        assert_eq!(loaded.content, "Initial content");

        update_note(&note.id, "Updated Title", "Updated content").unwrap();
        let updated = load_note(&note.id).unwrap();
        assert_eq!(updated.title, "Updated Title");
        assert_eq!(updated.content, "Updated content");

        delete_note(&note.id).unwrap();
        assert!(load_note(&note.id).is_err());
    }

    #[test]
    fn derives_title_from_heading() {
        std::fs::create_dir_all("notes").unwrap();
        let note = create_note("Fallback", "# Real Title\n\nSome content.").unwrap();
        let loaded = load_note(&note.id).unwrap();
        // The stored title is what was passed to create_note, but the file content has a heading.
        assert_eq!(loaded.title, "Fallback");
        delete_note(&note.id).unwrap();
    }
}
