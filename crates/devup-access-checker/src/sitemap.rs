//! sitemap.xml 요약 (네트워크 접근 없음).

use std::sync::LazyLock;

use regex::Regex;

static LOC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<loc>\s*(?:<!\[CDATA\[)?\s*(.*?)\s*(?:\]\]>)?\s*</loc>").unwrap()
});
static LASTMOD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<lastmod>").unwrap());
static INDEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<sitemapindex[\s>]").unwrap());

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitemapSummary {
    /// `<sitemapindex>` 여부 (하위 sitemap 목록)
    pub is_index: bool,
    /// `<loc>` 개수 (index 면 하위 sitemap 수, 아니면 URL 수)
    pub entries: usize,
    /// `<lastmod>` 개수
    pub with_lastmod: usize,
    /// `<loc>` 값 (XML 엔티티 `&amp;` 복원)
    pub locs: Vec<String>,
}

pub fn summarize(xml: &str) -> SitemapSummary {
    let locs: Vec<String> = LOC
        .captures_iter(xml)
        .map(|c| c[1].replace("&amp;", "&"))
        .collect();
    SitemapSummary {
        is_index: INDEX.is_match(xml),
        entries: locs.len(),
        with_lastmod: LASTMOD.find_iter(xml).count(),
        locs,
    }
}

static NAMESPACE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<(?:urlset|sitemapindex)\b[^>]*xmlns\s*=\s*["']http://www\.sitemaps\.org/schemas/sitemap/0\.9/?["']"#)
        .unwrap()
});
static PRIORITY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<priority>\s*(.*?)\s*</priority>").unwrap());
static CHANGEFREQ: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<changefreq>\s*(.*?)\s*</changefreq>").unwrap());
static LASTMOD_VALUE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<lastmod>\s*(.*?)\s*</lastmod>").unwrap());
static W3C_DATETIME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^\d{4}(?:-\d{2}(?:-\d{2}(?:T\d{2}:\d{2}(?::\d{2}(?:\.\d+)?)?(?:Z|[+-]\d{2}:\d{2}))?)?)?$",
    )
    .unwrap()
});
static CDATA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<!\[CDATA\[.*?\]\]>").unwrap());
static ENTITY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"&(?:amp|lt|gt|quot|apos|#\d+|#x[0-9A-Fa-f]+);").unwrap());

/// sitemaps.org 프로토콜 규칙 위반 (값은 최대 20개 예시).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SitemapIssues {
    /// 루트 요소에 `http://www.sitemaps.org/schemas/sitemap/0.9` 네임스페이스 선언
    pub namespace: bool,
    pub invalid_priority: Vec<String>,
    pub invalid_changefreq: Vec<String>,
    pub invalid_lastmod: Vec<String>,
    /// 엔티티로 바꾸지 않은 `&` 수 (CDATA 제외)
    pub unescaped_ampersands: usize,
    /// 절대 http(s) URL 이 아니거나 2048자 이상인 loc
    pub invalid_loc: Vec<String>,
    /// sitemap 파일이 있는 디렉터리 밖(또는 스킴·포트가 다른) 같은 호스트 URL
    pub out_of_scope: Vec<String>,
}

fn sample(v: &mut Vec<String>, s: &str) {
    if v.len() < 20 {
        v.push(s.to_string());
    }
}

pub fn validate(xml: &str, sitemap_url: &url::Url) -> SitemapIssues {
    let mut out = SitemapIssues {
        namespace: NAMESPACE.is_match(xml),
        ..Default::default()
    };
    for c in PRIORITY.captures_iter(xml) {
        if !c[1].parse::<f64>().is_ok_and(|p| (0.0..=1.0).contains(&p)) {
            sample(&mut out.invalid_priority, &c[1]);
        }
    }
    const FREQ: &[&str] = &[
        "always", "hourly", "daily", "weekly", "monthly", "yearly", "never",
    ];
    for c in CHANGEFREQ.captures_iter(xml) {
        if !FREQ.contains(&c[1].to_ascii_lowercase().as_str()) {
            sample(&mut out.invalid_changefreq, &c[1]);
        }
    }
    for c in LASTMOD_VALUE.captures_iter(xml) {
        if !W3C_DATETIME.is_match(&c[1]) {
            sample(&mut out.invalid_lastmod, &c[1]);
        }
    }
    let plain = CDATA.replace_all(xml, "");
    out.unescaped_ampersands = plain.matches('&').count() - ENTITY.find_iter(&plain).count();
    let dir = &sitemap_url.path()[..=sitemap_url.path().rfind('/').unwrap_or(0)];
    for loc in summarize(xml).locs {
        match url::Url::parse(&loc) {
            Ok(u) if matches!(u.scheme(), "http" | "https") && loc.len() < 2048 => {
                let same_host = u.host_str() == sitemap_url.host_str();
                let in_scope = u.scheme() == sitemap_url.scheme()
                    && u.port_or_known_default() == sitemap_url.port_or_known_default()
                    && u.path().starts_with(dir);
                if same_host && !in_scope {
                    sample(&mut out.out_of_scope, &loc);
                }
            }
            _ => sample(
                &mut out.invalid_loc,
                &loc.chars().take(200).collect::<String>(),
            ),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_urls_and_index() {
        let s = summarize(
            "<?xml version=\"1.0\"?><urlset><url><loc>https://a.com/?x=1&amp;y=2</loc><lastmod>2026-01-01</lastmod></url>\
             <url><LOC> <![CDATA[https://a.com/b]]> </LOC></url></urlset>",
        );
        assert!(!s.is_index);
        assert_eq!((s.entries, s.with_lastmod), (2, 1));
        assert_eq!(s.locs, vec!["https://a.com/?x=1&y=2", "https://a.com/b"]);
        let s =
            summarize("<sitemapindex xmlns=\"x\"><sitemap><loc>a</loc></sitemap></sitemapindex>");
        assert!(s.is_index);
        assert_eq!(s.entries, 1);
    }

    #[test]
    fn validates_protocol_rules() {
        let base = url::Url::parse("https://a.com/blog/sitemap.xml").unwrap();
        let xml = format!(
            "<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\"><url><loc>https://a.com/blog/1?a=1&b=2</loc>\
             <priority>0.5</priority><priority>1.5</priority><priority>높음</priority><changefreq>Daily</changefreq>\
             <changefreq>sometimes</changefreq><lastmod>2026-01-01</lastmod><lastmod>2026-01-01T10:00:00+09:00</lastmod>\
             <lastmod>2026/01/01</lastmod></url><url><loc>https://a.com/shop/1</loc></url><url><loc>http://a.com/blog/2</loc></url>\
             <url><loc>https://b.com/x</loc></url><url><loc>/relative</loc></url><url><loc>https://a.com/blog/{}</loc></url>\
             <url><loc><![CDATA[https://a.com/blog/?x=1&y=2]]></loc></url><url><loc>https://a.com/blog/?q=a&amp;b=c&#38;</loc></url></urlset>",
            "x".repeat(2050)
        );
        let v = validate(&xml, &base);
        assert!(v.namespace);
        assert_eq!(v.invalid_priority, ["1.5", "높음"]);
        assert_eq!(v.invalid_changefreq, ["sometimes"]);
        assert_eq!(v.invalid_lastmod, ["2026/01/01"]);
        assert_eq!(v.unescaped_ampersands, 1);
        assert_eq!(
            v.out_of_scope,
            ["https://a.com/shop/1", "http://a.com/blog/2"]
        );
        assert_eq!(v.invalid_loc.len(), 2);
        let root = url::Url::parse("https://a.com/sitemap.xml").unwrap();
        let v = validate(
            "<urlset><url><loc>https://a.com/x</loc></url></urlset>",
            &root,
        );
        assert_eq!(v, SitemapIssues::default());
        let many: String = (0..25).map(|_| "<priority>9</priority>").collect();
        assert_eq!(validate(&many, &root).invalid_priority.len(), 20);
    }
}
