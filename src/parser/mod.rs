use anyhow::Result;
use kuchiki::traits::TendrilSink;
use scraper::{Html, Selector};
use tracing::debug;
use url::Url;
use whatlang::detect;

use crate::models::PageRecord;

#[derive(Debug, Default, Clone, Copy)]
pub struct Parser;

impl Parser {
    pub fn new() -> Self {
        Self
    }

    pub fn parse(&self, base_url: &Url, html_content: &str) -> Result<PageRecord> {
        debug!(url = %base_url, "Parsing HTML content");
        let document = Html::parse_document(html_content);

        let title = Self::extract_title(&document);
        let description = Self::extract_description(&document);
        let headings = Self::extract_headings(&document);
        let content = Self::extract_content(html_content);
        let summary = Self::build_summary(&content);
        let language = Self::detect_language(&content);
        let links = Self::extract_links(&document, base_url);

        let record = PageRecord::new(
            base_url.to_string(),
            title,
            description,
            headings,
            content,
            summary,
            language,
            links,
        );
        debug!(url = %record.url, links = record.links.len(), "Parsed page record");
        Ok(record)
    }

    fn extract_title(document: &Html) -> String {
        Selector::parse("title")
            .ok()
            .and_then(|selector| document.select(&selector).next())
            .map(|e| normalize_whitespace(&e.text().collect::<String>()))
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| "No Title".to_string())
    }

    fn extract_description(document: &Html) -> Option<String> {
        let selector = Selector::parse("meta").ok()?;
        document
            .select(&selector)
            .filter(|meta| {
                meta.value()
                    .attr("name")
                    .map(|name| name.eq_ignore_ascii_case("description"))
                    .unwrap_or(false)
            })
            .filter_map(|meta| meta.value().attr("content"))
            .map(|content| normalize_whitespace(content))
            .find(|content| !content.is_empty())
    }

    fn extract_headings(document: &Html) -> Vec<String> {
        let selector = Selector::parse("h1, h2, h3").unwrap();
        document
            .select(&selector)
            .filter_map(|heading| {
                let text = normalize_whitespace(&heading.text().collect::<String>());
                if text.is_empty() { None } else { Some(text) }
            })
            .collect()
    }

    fn extract_links(document: &Html, base_url: &Url) -> Vec<String> {
        let selector = Selector::parse("a[href]").unwrap();
        document
            .select(&selector)
            .filter_map(|element| element.value().attr("href"))
            .filter_map(|href| base_url.join(href).ok())
            .filter(|normalized| matches!(normalized.scheme(), "http" | "https"))
            .map(|normalized| normalized.to_string())
            .collect()
    }

    fn extract_content(html_content: &str) -> String {
        let document = kuchiki::parse_html().one(html_content.to_string());
        for selector in ["script", "style", "noscript"] {
            if let Ok(nodes) = document.select(selector) {
                for node in nodes {
                    node.as_node().detach();
                }
            }
        }

        normalize_whitespace(&document.text_contents())
    }

    fn build_summary(content: &str) -> Option<String> {
        if content.is_empty() {
            return None;
        }

        let mut summary: String = content.chars().take(280).collect();
        if content.len() > summary.len() {
            summary.push_str("...");
        }

        Some(summary)
    }

    fn detect_language(content: &str) -> Option<String> {
        if content.len() < 50 {
            return None;
        }
        detect(content).map(|info| info.lang().code().to_string())
    }
}

fn normalize_whitespace(input: &str) -> String {
    input
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_html_document() {
        let parser = Parser::new();
        let base = Url::parse("https://example.com/").unwrap();
        let html = r#"
            <html>
                <head>
                    <title>Example</title>
                    <meta name="description" content="Example site" />
                </head>
                <body>
                    <script>console.log('ignore me');</script>
                    <h1>Heading</h1>
                    <p>Hello <strong>world</strong></p>
                    <a href="/about">About</a>
                </body>
            </html>
        "#;

        let record = parser.parse(&base, html).unwrap();

        assert_eq!(record.title, "Example");
        assert_eq!(record.description.as_deref(), Some("Example site"));
        assert!(record.headings.contains(&"Heading".to_string()));
        assert!(record.content.contains("Hello world"));
        assert!(record.summary.is_some());
        assert_eq!(record.links, vec!["https://example.com/about".to_string()]);
    }

    #[test]
    fn detects_language_when_possible() {
        let parser = Parser::new();
        let base = Url::parse("https://example.com/").unwrap();
        let html = r#"
            <html>
                <body>
                    <p>Este é um texto em português para testar a detecção de idioma.</p>
                </body>
            </html>
        "#;

        let record = parser.parse(&base, html).unwrap();
        assert_eq!(record.language.as_deref(), Some("por"));
    }
}
