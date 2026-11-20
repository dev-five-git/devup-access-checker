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
    /// (user-agent 목록, `Content-Signal` 값) — Cloudflare Content Signals
    pub content_signals: Vec<(String, String)>,
    /// (user-agent 목록, `Content-Usage` 값) — IETF aipref
    pub content_usage: Vec<(String, String)>,
    /// `License:` 지시자 값 — RSL
    pub licenses: Vec<String>,
    /// 첫 `User-agent` 앞에 있어 어느 그룹에도 속하지 않는 Allow/Disallow 수
    pub orphan_rules: usize,
    /// 규칙 형식 오류 (경로가 `/`·`*` 로 시작하지 않음, 1024자 초과, 비ASCII, 절대 URL 이 아닌 Sitemap)
    pub rule_errors: Vec<String>,
    /// Google 이 지원하지 않는 지시자 줄 (noindex·host·request-rate·visit-time)
    pub unsupported: Vec<String>,
    /// Yandex `Clean-param` 값
    pub clean_params: Vec<String>,
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
                    // Google: 제품 토큰 뒤의 텍스트(`Googlebot/2.1`)는 무시한다
                    let token = if value.starts_with('*') {
                        "*".to_string()
                    } else {
                        value
                            .split(['/', ' ', '*'])
                            .next()
                            .unwrap_or_default()
                            .to_ascii_lowercase()
                    };
                    current
                        .get_or_insert_with(Group::default)
                        .agents
                        .push(token);
                    last_was_agent = true;
                }
                "allow" | "disallow" => {
                    last_was_agent = false;
                    robots.orphan_rules += usize::from(current.is_none());
                    if !value.is_empty() {
                        let problem = if !(value.starts_with('/') || value.starts_with('*')) {
                            Some("경로가 / 로 시작하지 않음")
                        } else if value.len() > 1024 {
                            Some("1024자 초과")
                        } else if !value.is_ascii() {
                            Some("비ASCII 문자 (퍼센트 인코딩 필요)")
                        } else {
                            None
                        };
                        if let Some(p) = problem {
                            robots.rule_errors.push(format!(
                                "{p}: {}",
                                line.chars().take(120).collect::<String>()
                            ));
                        }
                    }
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
                        let lower = value.to_ascii_lowercase();
                        if !(lower.starts_with("http://") || lower.starts_with("https://")) {
                            robots
                                .rule_errors
                                .push(format!("Sitemap 이 절대 URL 이 아님: {value}"));
                        }
                    }
                }
                "crawl-delay" => {
                    let agents = current
                        .as_ref()
                        .map(|g| g.agents.join(","))
                        .unwrap_or_default();
                    robots.crawl_delays.push((agents, value.to_string()));
                }
                "content-signal" | "content-usage" => {
                    let agents = current
                        .as_ref()
                        .map(|g| g.agents.join(","))
                        .unwrap_or_default();
                    let list = if key == "content-signal" {
                        &mut robots.content_signals
                    } else {
                        &mut robots.content_usage
                    };
                    list.push((agents, value.to_string()));
                }
                "license" => {
                    if !value.is_empty() {
                        robots.licenses.push(value.to_string());
                    }
                }
                "clean-param" => robots.clean_params.push(value.to_string()),
                // 표준 외 지시자 (Google 은 무시)
                "host" | "noindex" | "request-rate" | "visit-time" => {
                    robots.unsupported.push(line.to_string())
                }
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
        // 가장 구체적인 그룹: googlebot-image 는 googlebot 그룹을 따른다 (Google)
        if selected.is_empty()
            && let Some((family, _)) = token.split_once('-')
        {
            selected = self
                .groups
                .iter()
                .filter(|g| g.agents.iter().any(|a| a == family))
                .collect();
        }
        // Applebot 은 자기 그룹이 없으면 Googlebot 규칙을 따른다 (Apple 문서)
        if selected.is_empty() && token == "applebot" {
            selected = self
                .groups
                .iter()
                .filter(|g| g.agents.iter().any(|a| a == "googlebot"))
                .collect();
        }
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

    #[test]
    fn ai_preference_directives_and_orphans() {
        let r = Robots::parse(
            "Disallow: /early\nUser-agent: *\nContent-Signal: search=yes, ai-train=no\nContent-Usage: train-ai=n\n\
             License: https://a.com/license.xml\nLicense:\nAllow: /\n\nUser-agent: Googlebot\nDisallow: /g\n",
        );
        assert!(r.invalid_lines.is_empty(), "{:?}", r.invalid_lines);
        assert_eq!(
            r.content_signals,
            [("*".to_string(), "search=yes, ai-train=no".to_string())]
        );
        assert_eq!(
            r.content_usage,
            [("*".to_string(), "train-ai=n".to_string())]
        );
        assert_eq!(r.licenses, ["https://a.com/license.xml"]);
        assert_eq!(r.orphan_rules, 1);
        // Applebot 은 Googlebot 그룹을 따른다
        assert!(!r.is_allowed("Applebot", "/g"));
        assert!(r.is_allowed("Applebot", "/x"));
        let r =
            Robots::parse("User-agent: Applebot\nAllow: /\nUser-agent: Googlebot\nDisallow: /\n");
        assert!(r.is_allowed("Applebot", "/g"));
        let r = Robots::parse("User-agent: *\nDisallow: /\n");
        assert!(!r.is_allowed("Applebot", "/"));
    }

    #[test]
    fn google_parsing_rules() {
        let r = Robots::parse(
            "User-agent: Googlebot/2.1 (+http://www.google.com/bot.html)\nCrawl-delay: 5\nUser-agent: Daum\nDisallow: /private\n\
             Disallow: private2\nAllow: /한글\nDisallow: /xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\nSitemap: /sitemap.xml\nHost: a.com\nNoindex: /x\n\
             Clean-param: ref /some_dir/\nUser-agent: *star\nAllow: /\n",
        );
        assert!(
            !r.is_allowed("Googlebot", "/private"),
            "Crawl-delay 는 그룹을 끊지 않는다"
        );
        assert!(!r.is_allowed("daum", "/private"));
        assert!(
            !r.is_allowed("Googlebot-Image", "/private"),
            "googlebot 그룹을 따른다"
        );
        assert!(
            r.is_allowed("Mistral-AI", "/private"),
            "그룹 없는 계열은 * 로"
        );
        assert_eq!(r.rule_errors.len(), 4, "{:?}", r.rule_errors);
        assert_eq!(r.unsupported, ["Host: a.com", "Noindex: /x"]);
        assert_eq!(r.clean_params, ["ref /some_dir/"]);
    }
}
