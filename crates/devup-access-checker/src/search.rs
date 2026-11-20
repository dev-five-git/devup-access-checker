//! 네이버 서치어드바이저·Bing 웹마스터 가이드라인과 대조해 추가한 정적 신호.
//! 대조 결과는 `docs/coverage.md` 에 있다. 네트워크 접근 없음.

use std::sync::LazyLock;

use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use url::Url;

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("valid selector")
}

static ALL: LazyLock<Selector> = LazyLock::new(|| sel("*"));
static PSEUDO_CALL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z_$][\w$.]*\s*\(.*\)\s*;?$").unwrap());
static GENERIC_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:(?:img|dsc|dscn|dcim|image|photo|pic|screenshot|untitled|scan)[-_ ]?\d+|\d+|[0-9a-f-]{16,})$")
        .unwrap()
});
static CACHE_BUST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:\d{10,}|[0-9a-f]{8,})$").unwrap());

/// 타임스탬프(숫자 10자리 이상) 또는 해시(영문자가 섞인 16진수 8자리 이상). `20250613` 같은 날짜 버전은 제외.
fn is_cache_bust(v: &str) -> bool {
    CACHE_BUST.is_match(v) && (v.len() >= 10 || !v.bytes().all(|b| b.is_ascii_digit()))
}

/// 반복 단어에서 뺄 영어 기능어.
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "of", "to", "in", "on", "an", "or", "at", "by", "from", "is",
    "are", "your", "you", "our", "we", "it", "its", "as", "be", "that", "this",
];
static HASHED_FILE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)[.-][0-9a-f]{8,}\.js$").unwrap());

/// 마이크로포맷 루트 클래스 (microformats2 와 classic).
const MICROFORMATS: &[&str] = &[
    "h-card",
    "h-entry",
    "h-event",
    "h-product",
    "h-recipe",
    "h-review",
    "h-review-aggregate",
    "vcard",
    "vevent",
    "hrecipe",
    "hreview",
    "hreview-aggregate",
    "hproduct",
];
const FAVICON_RELS: &[&str] = &[
    "shortcut icon",
    "icon",
    "apple-touch-icon",
    "apple-touch-icon-precomposed",
];
const HEAD_ONLY_META: &[&str] = &[
    "description",
    "robots",
    "naver-site-verification",
    "msvalidate.01",
    "google-site-verification",
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchFacts {
    /// `<link rel=canonical>` href 원문
    pub canonical_raw: Option<String>,
    /// favicon 계열 링크 (정규화한 rel, href 원문)
    pub favicons: Vec<(String, String)>,
    /// body 안에 들어간 head 전용 요소
    pub head_in_body: Vec<String>,
    /// `meta[name=naver]` content
    pub naver_meta: Option<String>,
    /// `a[rel~=nosublink]` 수
    pub nosublink_links: usize,
    pub fragment_routes: Vec<String>,
    pub fragment_route_count: usize,
    pub pseudo_js_links: Vec<String>,
    pub pseudo_js_count: usize,
    /// 같은 호스트 script 중 타임스탬프·해시 쿼리
    pub cache_busting_scripts: Vec<String>,
    /// 해시가 들어간 스크립트 파일명 수
    pub hashed_script_files: usize,
    pub images: usize,
    pub generic_image_names: Vec<String>,
    pub generic_image_count: usize,
    /// 같은 호스트 링크 중 파라미터 3개 이상
    pub param_heavy_links: Vec<String>,
    pub param_heavy_count: usize,
    pub max_link_params: usize,
    pub rdfa: bool,
    pub microformats: Vec<String>,
    pub og_image_socialonly: bool,
    /// `[data-snippet]` 요소 수 (Bing)
    pub data_snippet: usize,
}

fn push(v: &mut Vec<String>, s: &str) {
    if v.len() < 20 && !v.iter().any(|x| x == s) {
        v.push(s.to_string());
    }
}

/// body 안에 있는지 (svg 안의 title 은 제외).
fn in_body(el: &ElementRef) -> bool {
    let mut body = false;
    for a in el.ancestors().filter_map(ElementRef::wrap) {
        match a.value().name() {
            "svg" => return false,
            "body" => body = true,
            _ => {}
        }
    }
    body
}

pub fn extract(doc: &Html, base: &Url) -> SearchFacts {
    let mut f = SearchFacts::default();
    let host = base.host_str().unwrap_or_default();
    for el in doc.select(&ALL) {
        let e = el.value();
        let name = e.name();
        let get = |a: &str| e.attr(a).map(str::trim).filter(|v| !v.is_empty());
        let rel = get("rel")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .split_ascii_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let tokens: Vec<&str> = rel.split(' ').collect();
        if name == "link" && tokens.contains(&"canonical") && f.canonical_raw.is_none() {
            f.canonical_raw = get("href").map(str::to_string);
        }
        if name == "link" && FAVICON_RELS.contains(&rel.as_str()) {
            f.favicons
                .push((rel.clone(), get("href").unwrap_or_default().to_string()));
        }
        let meta_name = get("name").map(str::to_ascii_lowercase).unwrap_or_default();
        if in_body(&el) {
            let head_only = match name {
                "title" => true,
                "meta" => HEAD_ONLY_META.contains(&meta_name.as_str()),
                "link" => tokens.contains(&"canonical"),
                _ => false,
            };
            if head_only {
                let what = if name == "meta" {
                    format!("meta[name={meta_name}]")
                } else if name == "link" {
                    "link[rel=canonical]".into()
                } else {
                    name.to_string()
                };
                push(&mut f.head_in_body, &what);
            }
        }
        if name == "meta" && meta_name == "naver" {
            f.naver_meta = get("content").map(str::to_string);
        }
        if name == "meta"
            && get("property").is_some_and(|p| p.eq_ignore_ascii_case("og:image"))
            && e.attr("socialonly").is_some()
        {
            f.og_image_socialonly = true;
        }
        if name == "a" && tokens.contains(&"nosublink") {
            f.nosublink_links += 1;
        }
        if name == "a"
            && let Some(href) = e.attr("href").map(str::trim)
        {
            let onclick = e.attr("onclick").is_some();
            if PSEUDO_CALL.is_match(href) && !href.contains(':')
                || (href.starts_with('#') && onclick)
            {
                f.pseudo_js_count += 1;
                push(&mut f.pseudo_js_links, href);
            }
            if let Ok(u) = base.join(href)
                && u.host_str() == Some(host)
            {
                if u.fragment()
                    .is_some_and(|fr| fr.starts_with('!') || fr.starts_with('/'))
                {
                    f.fragment_route_count += 1;
                    push(&mut f.fragment_routes, href);
                }
                let params = u.query_pairs().count();
                f.max_link_params = f.max_link_params.max(params);
                if params >= 3 {
                    f.param_heavy_count += 1;
                    push(&mut f.param_heavy_links, u.as_str());
                }
            }
        }
        if name == "script"
            && let Some(u) = get("src").and_then(|s| base.join(s).ok())
            && u.host_str() == Some(host)
        {
            if u.query_pairs().any(|(_, v)| is_cache_bust(&v)) {
                push(&mut f.cache_busting_scripts, u.as_str());
            }
            f.hashed_script_files += usize::from(HASHED_FILE.is_match(u.path()));
        }
        if name == "img"
            && let Some(src) = get("src")
        {
            f.images += 1;
            let file = src
                .split(['?', '#'])
                .next()
                .unwrap_or_default()
                .rsplit('/')
                .next()
                .unwrap_or_default();
            let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
            if GENERIC_NAME.is_match(stem) {
                f.generic_image_count += 1;
                push(&mut f.generic_image_names, file);
            }
        }
        if e.attr("typeof").is_some()
            || (e.attr("property").is_some()
                && (e.attr("vocab").is_some() || e.attr("prefix").is_some()))
        {
            f.rdfa = true;
        }
        for c in e.classes() {
            if MICROFORMATS.contains(&c) {
                push(&mut f.microformats, c);
            }
        }
        f.data_snippet += usize::from(e.attr("data-snippet").is_some());
    }
    f
}

/// 404 페이지 본문에서 다른 호스트로 요청하는 script·iframe·stylesheet 의 호스트.
pub fn third_party_hosts(html: &str, base: &Url) -> Vec<String> {
    let doc = Html::parse_document(html);
    let mut hosts = Vec::new();
    for el in doc.select(&sel(
        "script[src], iframe[src], link[rel~=stylesheet][href]",
    )) {
        let src = el
            .value()
            .attr("src")
            .or_else(|| el.value().attr("href"))
            .unwrap_or_default();
        if let Ok(u) = base.join(src)
            && let Some(h) = u.host_str()
            && Some(h) != base.host_str()
        {
            push(&mut hosts, h);
        }
    }
    hosts
}

/// RFC 952/1123 호스트명 규칙 위반 레이블 (IDN 은 punycode 로 바꾼 뒤 본다).
pub fn invalid_host_labels(host: &str) -> Vec<String> {
    host.trim_end_matches('.')
        .split('.')
        .filter(|l| {
            l.is_empty()
                || l.len() > 63
                || l.starts_with('-')
                || l.ends_with('-')
                || !l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
        .map(str::to_string)
        .collect()
}

/// 제목·설명에서 2번 이상 나온 단어 (2글자 이상, 대소문자 무시).
pub fn repeated_terms(text: &str) -> Vec<(String, usize)> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for t in text.split(|c: char| c.is_whitespace() || "|·-–—,:/".contains(c)) {
        let t = t.to_lowercase();
        if t.chars().count() < 2 || STOPWORDS.contains(&t.as_str()) {
            continue;
        }
        match counts.iter_mut().find(|(w, _)| *w == t) {
            Some((_, n)) => *n += 1,
            None => counts.push((t, 1)),
        }
    }
    counts.retain(|(_, n)| *n >= 2);
    counts
}

static ITEM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<(?:item|entry)[\s>]").unwrap());
static CONTENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<(?:content:encoded|content)[\s>]").unwrap());
static RSS_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<link>\s*(?:<!\[CDATA\[)?\s*(.*?)\s*(?:\]\]>)?\s*</link>").unwrap()
});
static ATOM_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<(?:atom:)?link\b[^>]*\bhref\s*=\s*["']([^"']*)["']"#).unwrap()
});

/// RSS·Atom 피드 요약.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FeedFacts {
    pub valid: bool,
    /// rss2 / rss1 / atom
    pub format: Option<&'static str>,
    pub items: usize,
    /// item·entry 수 중 본문(content:encoded·content)이 있는 것
    pub full_content_items: usize,
    /// 피드 안의 링크 (채널·item)
    pub links: Vec<String>,
}

pub fn parse_feed(body: &str) -> FeedFacts {
    let head: String = body
        .chars()
        .take(4096)
        .collect::<String>()
        .to_ascii_lowercase();
    let format = if head.contains("<rss") && body.to_ascii_lowercase().contains("<channel") {
        Some("rss2")
    } else if head.contains("rdf:rdf") {
        Some("rss1")
    } else if head.contains("<feed") && head.contains("http://www.w3.org/2005/atom") {
        Some("atom")
    } else {
        None
    };
    FeedFacts {
        valid: format.is_some(),
        format,
        items: ITEM.find_iter(body).count(),
        full_content_items: CONTENT.find_iter(body).count(),
        links: RSS_LINK
            .captures_iter(body)
            .chain(ATOM_LINK.captures_iter(body))
            .map(|c| c[1].trim().to_string())
            .filter(|l| !l.is_empty())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(html: &str) -> SearchFacts {
        extract(
            &Html::parse_document(html),
            &Url::parse("https://a.com/p?x=1").unwrap(),
        )
    }

    #[test]
    fn markup_link_and_resource_signals() {
        let f = run(
            r##"<html><head><link rel="canonical" href="/p"><link rel="canonical" href="/q">
<link rel="Shortcut  Icon" href="/favicon.ico"><link rel="icon" href="a.png"><link rel="icon" href="https://a.com/b.png">
<meta name="naver" content="nosublinks"><meta property="og:image" content="a.jpg" socialonly>
<script src="/app.js?v=1712345678"></script><script src="/main.3f2a9c1d.js"></script><script src="https://cdn.x.com/a.js?v=1712345678"></script>
<script src="/b.js?v=2"></script><script src="/c.js?v=20250613"></script><script src="/d.js?h=3f2a9c1d"></script></head><body><title>본문 제목</title><meta name="description" content="x"><meta name="viewport" content="x">
<link rel="canonical" href="/z"><svg><title>아이콘</title></svg>
<a href="#!/faq">해시뱅</a><a href="/#/product">해시 라우트</a><a href="#top">위로</a><a href="https://b.com/#!/x">외부</a>
<a href="goPage(1, 'news')">함수</a><a href="#" onclick="go()">클릭</a><a href="javascript:go()">js</a>
<a href="/list?a=1&b=2&c=3">많음</a><a href="/list?a=1&b=2&c=3">중복</a><a href="/list?a=1">적음</a>
<a href="/x" rel="nosublink">x</a><a>앵커 없음</a>
<img src="/img/IMG_1234.JPG"><img src="/img/1234.png?w=1"><img src="/img/3f2a9c1d3f2a9c1d.webp"><img src="/img/제품-사진.jpg"><img>
<div vocab="https://schema.org/" typeof="Person"><span property="name">이름</span></div>
<div class="h-card vcard">카드</div><div class="h-card">카드2</div><p data-snippet>요약</p></body></html>"##,
        );
        assert_eq!(f.canonical_raw.as_deref(), Some("/p"));
        assert_eq!(f.favicons.len(), 3);
        assert_eq!(f.favicons[0].0, "shortcut icon");
        assert_eq!(
            f.head_in_body,
            ["title", "meta[name=description]", "link[rel=canonical]"]
        );
        assert_eq!(f.naver_meta.as_deref(), Some("nosublinks"));
        assert!(f.og_image_socialonly && f.rdfa);
        assert_eq!(f.nosublink_links, 1);
        assert_eq!((f.fragment_route_count, f.fragment_routes.len()), (2, 2));
        assert_eq!(f.pseudo_js_count, 2, "{:?}", f.pseudo_js_links);
        assert_eq!(
            f.cache_busting_scripts,
            [
                "https://a.com/app.js?v=1712345678",
                "https://a.com/d.js?h=3f2a9c1d"
            ]
        );
        assert_eq!(f.hashed_script_files, 1);
        assert_eq!((f.images, f.generic_image_count), (4, 3));
        assert_eq!(
            (
                f.param_heavy_count,
                f.param_heavy_links.len(),
                f.max_link_params
            ),
            (2, 1, 3)
        );
        assert_eq!(f.microformats, ["h-card", "vcard"]);
        assert_eq!(f.data_snippet, 1);

        let f = run(r#"<html><body><span property="og:title">x</span><p>본문</p></body></html>"#);
        assert_eq!(f, SearchFacts::default());
        let many: String = (0..25)
            .map(|i| format!("<a href=\"#!/p{i}\"></a>"))
            .collect();
        assert_eq!(run(&many).fragment_routes.len(), 20);
    }

    #[test]
    fn helpers() {
        let base = Url::parse("https://a.com/").unwrap();
        let html = r#"<html><head><script src="https://cdn.ads.com/a.js"></script><script src="/local.js"></script>
<link rel="stylesheet" href="https://fonts.x.com/f.css"><iframe src="https://cdn.ads.com/f"></iframe><script></script></head></html>"#;
        assert_eq!(
            third_party_hosts(html, &base),
            ["cdn.ads.com", "fonts.x.com"]
        );
        assert!(third_party_hosts("<p>x</p>", &base).is_empty());
        assert!(invalid_host_labels("www.a-b.com").is_empty());
        assert_eq!(invalid_host_labels("my_site.-a.com."), ["my_site", "-a"]);
        let long = "a".repeat(64);
        assert_eq!(
            invalid_host_labels(&format!("{long}..com")),
            [long.as_str(), ""]
        );
        assert_eq!(
            repeated_terms("서울 맛집 | 서울 맛집 추천 - Seoul seoul a a the the and and"),
            [
                ("서울".to_string(), 2),
                ("맛집".to_string(), 2),
                ("seoul".to_string(), 2)
            ]
        );
        assert!(repeated_terms("하나 둘 셋").is_empty());
    }

    #[test]
    fn feeds() {
        let f = parse_feed(
            "<rss version=\"2.0\"><channel><link><![CDATA[ https://a.com/ ]]></link><item><link>https://a.com/1</link>\
             <content:encoded>x</content:encoded></item><item><link></link></item></channel></rss>",
        );
        assert_eq!(
            (f.valid, f.format, f.items, f.full_content_items),
            (true, Some("rss2"), 2, 1)
        );
        assert_eq!(f.links, ["https://a.com/", "https://a.com/1"]);
        let f = parse_feed(r#"<rdf:RDF><item></item></rdf:RDF>"#);
        assert_eq!(f.format, Some("rss1"));
        let f = parse_feed(
            r#"<feed xmlns="http://www.w3.org/2005/Atom"><entry><link rel="alternate" href="/a"/><content type="html">x</content></entry></feed>"#,
        );
        assert_eq!(
            (f.format, f.items, f.full_content_items),
            (Some("atom"), 1, 1)
        );
        assert_eq!(f.links, ["/a"]);
        assert_eq!(
            parse_feed("<html><body>x</body></html>"),
            FeedFacts::default()
        );
        assert!(!parse_feed("<rss></rss>").valid);
    }
}
