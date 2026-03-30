use std::time::Duration;

#[derive(Clone, Debug, Default)]
pub struct RobotsRules {
    disallow: Vec<String>,
    allow: Vec<String>,
    crawl_delay: Option<Duration>,
}

impl RobotsRules {
    pub fn allow_all() -> Self {
        Self::default()
    }

    pub fn allows(&self, path: &str) -> bool {
        if self
            .allow
            .iter()
            .any(|rule| !rule.is_empty() && path.starts_with(rule))
        {
            return true;
        }

        for rule in &self.disallow {
            if rule.is_empty() {
                continue;
            }
            if path.starts_with(rule) {
                return false;
            }
        }

        true
    }

    pub fn crawl_delay(&self) -> Option<Duration> {
        self.crawl_delay
    }

    fn push_disallow(&mut self, value: &str) {
        self.disallow.push(value.to_string());
    }

    fn push_allow(&mut self, value: &str) {
        self.allow.push(value.to_string());
    }

    fn set_crawl_delay(&mut self, value: &str) {
        if let Ok(delay) = value.parse::<f64>() {
            if delay > 0.0 {
                let secs = delay;
                self.crawl_delay = Some(Duration::from_secs_f64(secs));
            }
        }
    }
}

pub fn parse_robots(content: &str, agent: &str) -> RobotsRules {
    let mut rules = RobotsRules::default();
    let mut section_relevant = false;
    let agent_lower = agent.to_ascii_lowercase();

    for line in content.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            section_relevant = false;
            continue;
        }

        let mut parts = line.splitn(2, ':');
        let directive = parts.next().unwrap().trim().to_ascii_lowercase();
        let value = parts.next().map(|v| v.trim()).unwrap_or("");

        match directive.as_str() {
            "user-agent" => {
                let ua = value.to_ascii_lowercase();
                section_relevant = ua == "*" || ua == agent_lower;
            }
            "disallow" if section_relevant => rules.push_disallow(value),
            "allow" if section_relevant => rules.push_allow(value),
            "crawl-delay" if section_relevant => rules.set_crawl_delay(value),
            _ => {}
        }
    }

    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_rules() {
        let robots = r#"
        User-agent: RustyCrawlerMVP/0.1
        Disallow: /private
        Allow: /private/info
        Crawl-delay: 5
        "#;

        let rules = parse_robots(robots, "RustyCrawlerMVP/0.1");
        assert!(rules.allows("/"));
        assert!(!rules.allows("/private/data"));
        assert!(rules.allows("/private/info/public"));
        assert_eq!(rules.crawl_delay(), Some(Duration::from_secs(5)));
    }

    #[test]
    fn ignores_other_agents() {
        let robots = r#"
        User-agent: other
        Disallow: /

        User-agent: *
        Disallow: /tmp
        "#;

        let rules = parse_robots(robots, "RustyCrawlerMVP/0.1");
        assert!(rules.allows("/hello"));
        assert!(!rules.allows("/tmp/foo"));
    }
}
