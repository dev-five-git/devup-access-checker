//! 최소 robots.txt 파서 (RFC 9309). 그룹 선택, 가장 긴 규칙 우선, 동률 시 Allow 우선.

use regex::Regex;

#[derive(Debug, Clone)]
struct Rule {
    allow: bool,
    pattern: String,
}

#[derive(Debug, Clone, Default)]
struct Group {
    agents: Vec<String>,
    rules: Vec<Rule>,
}

#[derive(Debug, Clone, Default)]
pub struct Robots {
    groups: Vec<Group>,
    /// `Sitemap:` 지시자 값
    pub sitemaps: Vec<String>,
    /// (user-agent 목록, Crawl-delay 값)
    pub crawl_delays: Vec<(String, String)>,
    /// 해석할 수 없는 줄 (알 수 없는 지시자, 콜론 없음)
    pub invalid_lines: Vec<String>,
}

impl Robots {
    pub fn parse(body: &str) -> Self {
        let mut robots = Robots::default();
        let mut current: Option<Group> = None;
        let mut last_was_agent = false;

        for raw in body.lines() {
            let line = raw.split('#').next().unwrap_or_default().trim();
            let Some((key, value)) = line.split_once(':') else {
                if !line.is_empty() {
                    robots.invalid_lines.push(line.to_string());
                }
                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim();
            match key.as_str() {
                "user-agent" => {
                    if !last_was_agent && let Some(g) = current.take() {
                        robots.groups.push(g);
                    }
                    current
                        .get_or_insert_with(Group::default)
                        .agents
                        .push(value.to_ascii_lowercase());
                    last_was_agent = true;
                }
                "allow" | "disallow" => {
                    last_was_agent = false;
                    if let Some(g) = current.as_mut()
                        && !value.is_empty()
                    {
                        g.rules.push(Rule {
                            allow: key == "allow",
                            pattern: value.to_string(),
                        });
                    }
                }
                "sitemap" => {
                    // RFC 상 그룹과 무관한 전역 지시자. `Sitemap: https://...` 의 콜론 이후 전체를 사용.
                    if !value.is_empty() {
                        robots.sitemaps.push(value.to_string());
                    }
                }
                "crawl-delay" => {
                    last_was_agent = false;
                    let agents = current
                        .as_ref()
                        .map(|g| g.agents.join(","))
                        .unwrap_or_default();
                    robots.crawl_delays.push((agents, value.to_string()));
                }
                // 표준 외이지만 널리 쓰이는 지시자
                "host" | "clean-param" | "noindex" | "request-rate" | "visit-time"
                | "content-signal" => last_was_agent = false,
                _ => {
                    last_was_agent = false;
                    robots.invalid_lines.push(line.to_string());
                }
            }
        }
        if let Some(g) = current {
            robots.groups.push(g);
        }
        robots
    }

    /// 해당 product token 에 적용되는 그룹이 명시적으로 존재하는지 (`*` 제외).
    pub fn has_group_for(&self, token: &str) -> bool {
        let token = token.to_ascii_lowercase();
        self.groups.iter().any(|g| g.agents.contains(&token))
    }

    /// `token` 크롤러가 `path` 에 접근 가능한지.
    pub fn is_allowed(&self, token: &str, path: &str) -> bool {
        let token = token.to_ascii_lowercase();
        let mut selected: Vec<&Group> = self
            .groups
            .iter()
            .filter(|g| g.agents.contains(&token))
            .collect();
        if selected.is_empty() {
            selected = self
                .groups
                .iter()
                .filter(|g| g.agents.iter().any(|a| a == "*"))
                .collect();
        }
        let mut best: Option<(usize, bool)> = None;
        for rule in selected.iter().flat_map(|g| &g.rules) {
            if !pattern_matches(&rule.pattern, path) {
                continue;
            }
            let len = rule.pattern.len();
            best = match best {
                Some((l, a)) if l > len || (l == len && a) => Some((l, a)),
                _ => Some((len, rule.allow)),
            };
        }
        best.is_none_or(|(_, allow)| allow)
    }
}

/// 비교 전에 양쪽을 같은 형태로 맞춘다: 비 ASCII 는 퍼센트 인코딩, `%xx` 는 대문자.
fn normalize(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%'
            && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit()
        {
            out.push('%');
            out.push(bytes[i + 1].to_ascii_uppercase() as char);
            out.push(bytes[i + 2].to_ascii_uppercase() as char);
            i += 3;
        } else if b >= 0x80 {
            out.push_str(&format!("%{b:02X}"));
            i += 1;
        } else {
            out.push(b as char);
            i += 1;
        }
    }
    out
}

fn pattern_matches(pattern: &str, path: &str) -> bool {
    let pattern = normalize(pattern);
    let path = normalize(path);
    let (body, anchored) = match pattern.strip_suffix('$') {
        Some(b) => (b, true),
        None => (pattern.as_str(), false),
    };
    let mut re = String::from("^");
    for (i, part) in body.split('*').enumerate() {
        if i > 0 {
            re.push_str(".*");
        }
        re.push_str(&regex::escape(part));
    }
    if anchored {
        re.push('$');
    }
    Regex::new(&re).is_ok_and(|r| r.is_match(&path))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
User-agent: *
Disallow: /admin
Allow: /admin/public

User-agent: GPTBot
User-agent: CCBot
Disallow: /

User-agent: ClaudeBot
Disallow:

Sitemap: https://example.com/sitemap.xml # main
";

    #[test]
    fn group_selection_and_rules() {
        let r = Robots::parse(SAMPLE);
        assert!(r.is_allowed("Googlebot", "/"));
        assert!(!r.is_allowed("Googlebot", "/admin/x"));
        assert!(r.is_allowed("Googlebot", "/admin/public/x"));
        assert!(!r.is_allowed("gptbot", "/"));
        assert!(!r.is_allowed("CCBot", "/"));
        assert!(
            r.is_allowed("ClaudeBot", "/admin"),
            "specific group overrides *"
        );
        assert!(r.has_group_for("GPTBot"));
        assert!(!r.has_group_for("PerplexityBot"));
        assert_eq!(r.sitemaps, vec!["https://example.com/sitemap.xml"]);
    }

    #[test]
    fn wildcards_and_anchor() {
        let r = Robots::parse("User-agent: *\nDisallow: /*.pdf$\nDisallow: /tmp*\n");
        assert!(!r.is_allowed("x", "/a/b.pdf"));
        assert!(r.is_allowed("x", "/a/b.pdf?x=1"));
        assert!(!r.is_allowed("x", "/tmpfile"));
        assert!(r.is_allowed("x", "/"));
    }

    #[test]
    fn crawl_delay_and_invalid_lines() {
        let r = Robots::parse(
            "User-agent: Yeti\nCrawl-delay: 10\nDisalow: /x\nhello world\nHost: a.com\n",
        );
        assert_eq!(r.crawl_delays, vec![("yeti".to_string(), "10".to_string())]);
        assert_eq!(r.invalid_lines, vec!["Disalow: /x", "hello world"]);
    }

    #[test]
    fn empty_robots_allows_all() {
        assert!(Robots::parse("").is_allowed("GPTBot", "/"));
    }

    #[test]
    fn matches_query_and_non_ascii_paths() {
        let r =
            Robots::parse("User-agent: *\nDisallow: /*?sort=\nDisallow: /검색\nDisallow: /a%2fb\n");
        assert!(!r.is_allowed("Googlebot", "/list?sort=price"));
        assert!(r.is_allowed("Googlebot", "/list?page=2"));
        // url 크레이트가 만든 경로(퍼센트 인코딩)와 원문 패턴이 맞아야 한다
        assert!(!r.is_allowed("Googlebot", "/%EA%B2%80%EC%83%89"));
        assert!(!r.is_allowed("Googlebot", "/a%2Fb"));
    }
}
