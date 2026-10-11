use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const MAX_QUOTE_LENGTH: usize = 500;
const MAX_AUTHOR_LENGTH: usize = 100;
const MAX_QUOTES: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quote {
    pub text: String,
    pub author: String,
}

pub struct QuoteStore {
    path: PathBuf,
}

impl QuoteStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// All saved quotes in insertion order. Missing or unreadable storage
    /// reads as empty so the welcome page still renders; the parse error is
    /// not hidden from writers, only from readers.
    pub fn list(&self) -> Vec<Quote> {
        match std::fs::read_to_string(&self.path) {
            Ok(body) => {
                if body.trim().is_empty() {
                    Vec::new()
                } else {
                    serde_json::from_str(&body).unwrap_or_default()
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(_) => Vec::new(),
        }
    }

    pub fn add(&self, text: &str, author: &str) -> Result<Quote> {
        let text = text.trim();
        let author = author.trim();
        if text.is_empty() {
            return Err(anyhow!("Quote cannot be blank."));
        }
        if text.chars().count() > MAX_QUOTE_LENGTH {
            return Err(anyhow!(
                "Quote must be at most {MAX_QUOTE_LENGTH} characters."
            ));
        }
        if author.chars().count() > MAX_AUTHOR_LENGTH {
            return Err(anyhow!(
                "Author must be at most {MAX_AUTHOR_LENGTH} characters."
            ));
        }
        if text.chars().any(char::is_control) || author.chars().any(char::is_control) {
            return Err(anyhow!("Quotes cannot contain control characters."));
        }

        let mut quotes = self.list();
        if quotes.len() >= MAX_QUOTES {
            return Err(anyhow!("Quote list is full ({MAX_QUOTES} quotes)."));
        }
        let quote = Quote {
            text: text.to_string(),
            author: author.to_string(),
        };
        quotes.push(quote.clone());
        self.write(&quotes)?;
        Ok(quote)
    }

    pub fn remove(&self, index: usize) -> Result<()> {
        let mut quotes = self.list();
        if index >= quotes.len() {
            return Err(anyhow!("Quote not found."));
        }
        quotes.remove(index);
        if quotes.is_empty() {
            match std::fs::remove_file(&self.path) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err).context("removing saved quotes"),
            }
            return Ok(());
        }
        self.write(&quotes)
    }

    fn write(&self, quotes: &[Quote]) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).context("creating notes directory for quotes")?;
        }
        let body = serde_json::to_string_pretty(quotes).context("encoding quotes")?;
        std::fs::write(&self.path, body).context("saving quotes")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "eternalibre-quotes-{tag}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        p.set_extension("json");
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn missing_storage_lists_no_quotes() {
        let store = QuoteStore::new(temp_path("missing"));
        assert!(store.list().is_empty());
    }

    #[test]
    fn add_persists_quote_and_author() {
        let path = temp_path("add");
        let store = QuoteStore::new(path.clone());
        store.add("Stay curious.", "Ada").unwrap();
        let quotes = store.list();
        assert_eq!(quotes.len(), 1);
        assert_eq!(quotes[0].text, "Stay curious.");
        assert_eq!(quotes[0].author, "Ada");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn add_rejects_blank_and_oversized_quotes() {
        let store = QuoteStore::new(temp_path("invalid"));
        assert!(store.add("   ", "Someone").is_err());
        assert!(store.add(&"x".repeat(MAX_QUOTE_LENGTH + 1), "").is_err());
        assert!(store
            .add("Fine", &"y".repeat(MAX_AUTHOR_LENGTH + 1))
            .is_err());
    }

    #[test]
    fn remove_deletes_by_index_and_clears_the_file() {
        let path = temp_path("remove");
        let store = QuoteStore::new(path.clone());
        store.add("First", "A").unwrap();
        store.add("Second", "B").unwrap();
        store.remove(0).unwrap();
        assert_eq!(store.list().len(), 1);
        store.remove(0).unwrap();
        assert!(store.list().is_empty());
        assert!(!path.exists());
        assert!(store.remove(0).is_err());
    }
}
