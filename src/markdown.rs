use pulldown_cmark::{html, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use crate::syntax::{highlight_code, syntax_state};
use crate::themes::Theme;

pub fn render_markdown(input: &str, theme: &Theme) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_FOOTNOTES);

    let parser = Parser::new_ext(input, options);
    let mut events: Vec<Event<'_>> = Vec::new();
    let mut parser = parser.peekable();

    while let Some(event) = parser.next() {
        if let Event::Start(Tag::CodeBlock(kind)) = event {
            let lang = match kind {
                CodeBlockKind::Fenced(lang) => Some(lang.to_string()),
                CodeBlockKind::Indented => None,
            };

            let mut code = String::new();
            loop {
                match parser.next() {
                    Some(Event::Text(text)) => code.push_str(&text),
                    Some(Event::End(TagEnd::CodeBlock)) => break,
                    Some(other) => {
                        // Defensive: if we see something unexpected, push it through.
                        events.push(other);
                    }
                    None => break,
                }
            }

            let highlighted = highlight_code(&code, lang.as_deref(), theme, syntax_state());
            events.push(Event::Html(highlighted.into()));
        } else {
            events.push(event);
        }
    }

    let mut html_output = String::new();
    html::push_html(&mut html_output, events.into_iter());
    html_output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::themes::get_theme;

    #[test]
    fn renders_headings_and_paragraphs() {
        let md = "# Hello\n\nThis is a paragraph.";
        let html = render_markdown(md, get_theme("Dark"));
        assert!(html.contains("<h1>Hello</h1>"));
        assert!(html.contains("<p>This is a paragraph.</p>"));
    }

    #[test]
    fn renders_tasklists() {
        let md = "- [x] Done\n- [ ] Todo";
        let html = render_markdown(md, get_theme("Dark"));
        assert!(html.contains("<input"));
        assert!(html.contains("checked"));
    }

    #[test]
    fn renders_tables() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |";
        let html = render_markdown(md, get_theme("Dark"));
        assert!(html.contains("<table>"));
        assert!(html.contains("<th>A</th>"));
    }

    #[test]
    fn highlights_rust_code_blocks() {
        let md = "```rust\nfn main() {\n    println!(\"hello\");\n}\n```";
        let html = render_markdown(md, get_theme("Dark"));
        assert!(html.contains("<pre"));
        assert!(html.contains("fn"));
        assert!(html.contains("main"));
        // Syntect emits inline style spans for highlighted tokens.
        assert!(html.contains("style=\"color:"));
    }

    #[test]
    fn falls_back_for_unknown_language() {
        let md = "```notareallanguage\nsome code\n```";
        let html = render_markdown(md, get_theme("Dark"));
        assert!(html.contains("some code"));
        assert!(html.contains("<pre"));
    }
}
