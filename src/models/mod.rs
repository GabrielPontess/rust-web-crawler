use sqlx::FromRow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageRecord {
    pub url: String,
    pub title: String,
    pub description: Option<String>,
    pub headings: Vec<String>,
    pub content: String,
    pub summary: Option<String>,
    pub language: Option<String>,
    pub links: Vec<String>,
}

#[derive(Debug, Clone, FromRow, PartialEq)]
pub struct SearchResult {
    pub url: String,
    pub title: Option<String>,
    pub snippet: Option<String>,
    pub lang: Option<String>,
    pub score: f64,
}

impl PageRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        url: String,
        title: String,
        description: Option<String>,
        headings: Vec<String>,
        content: String,
        summary: Option<String>,
        language: Option<String>,
        links: Vec<String>,
    ) -> Self {
        Self {
            url,
            title,
            description,
            headings,
            content,
            summary,
            language,
            links,
        }
    }
}
