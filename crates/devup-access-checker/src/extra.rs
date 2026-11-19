//! 추가 SEO/GEO 사실: 중복 태그, 헤딩 순서, 렌더링 차단 리소스, 이미지 포맷, 스키마 필수 속성 등.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use scraper::{ElementRef, Html, Selector};
use serde_json::Value;
use url::Url;

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("valid selector")
}

static TITLE: LazyLock<Selector> = LazyLock::new(|| sel("title"));
static META: LazyLock<Selector> = LazyLock::new(|| sel("meta[name]"));
static LINK: LazyLock<Selector> = LazyLock::new(|| sel("link[rel]"));
static HEADINGS: LazyLock<Selector> = LazyLock::new(|| sel("h1, h2, h3, h4, h5, h6"));
static ANCHOR: LazyLock<Selector> = LazyLock::new(|| sel("a[href]"));
static HEAD_SCRIPT: LazyLock<Selector> = LazyLock::new(|| sel("head script[src]"));
static HEAD_STYLESHEET: LazyLock<Selector> = LazyLock::new(|| sel("head link[rel~=stylesheet]"));
static IMG: LazyLock<Selector> = LazyLock::new(|| sel("img"));
static PICTURE_SOURCE: LazyLock<Selector> = LazyLock::new(|| sel("picture source[type]"));
static RESOURCE: LazyLock<Selector> =
    LazyLock::new(|| sel("script[src], link[rel~=stylesheet][href]"));
static JSON_LD: LazyLock<Selector> = LazyLock::new(|| sel("script[type]"));
static TAG: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"<[^>]*>").unwrap());

/// 스키마 타입별 필수(권장) 속성. `a|b` 는 둘 중 하나만 있어도 충족.
pub const SCHEMA_REQUIRED: &[(&str, &[&str], &[&str])] = &[
    // (id, 해당 @type 들, 필수 속성)
    (
        "organization",
        &[
            "Organization",
            "Corporation",
            "LocalBusiness",
            "NGO",
            "EducationalOrganization",
            "GovernmentOrganization",
            "OnlineBusiness",
            "OnlineStore",
            "NewsMediaOrganization",
        ],
        &["name", "url", "logo"],
    ),
    ("website", &["WebSite"], &["name", "url"]),
    (
        "article",
        &[
            "Article",
            "NewsArticle",
            "BlogPosting",
            "TechArticle",
            "Report",
        ],
        &["headline", "author", "datePublished", "image"],
    ),
    (
        "product",
        &["Product"],
        &["name", "image", "offers|review|aggregateRating"],
    ),
    ("breadcrumb_list", &["BreadcrumbList"], &["itemListElement"]),
    ("faq_page", &["FAQPage"], &["mainEntity"]),
    ("local_business", &["LocalBusiness"], &["name", "address"]),
];

/// JSON-LD 안의 `@type` 이 있는 객체 하나.
#[derive(Debug, Clone, Default)]
pub struct LdObject {
    pub types: Vec<String>,
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ExtraFacts {
    /// 태그 이름 → 개수 (title, description, canonical, viewport, robots)
    pub tag_counts: BTreeMap<&'static str, usize>,
    /// 문서 순서의 헤딩 레벨
    pub heading_levels: Vec<u8>,
    /// 건너뛴 헤딩 (예: "h2→h4")
    pub heading_skips: Vec<String>,
    /// 같은 호스트 링크 중 rel=nofollow
    pub internal_nofollow: usize,
    /// 같은 호스트 링크의 절대 URL (fragment 제거, 중복 제거, 현재 페이지 제외)
    pub internal_link_urls: Vec<String>,
    /// head 안 async/defer/module 없는 외부 스크립트
    pub render_blocking_scripts: Vec<String>,
    pub head_stylesheets: usize,
    /// preconnect / dns-prefetch 대상
    pub preconnect: Vec<String>,
    /// 확장자/`<source type>` 기준 이미지 포맷 → 개수
    pub image_formats: BTreeMap<String, usize>,
    pub img_srcset: usize,
    /// `<link rel="alternate" type="text/markdown">`
    pub markdown_alternate: Option<String>,
    /// 같은 호스트에서 로드하는 script/stylesheet 경로
    pub same_origin_resources: Vec<String>,
    pub ld_objects: Vec<LdObject>,
    pub published: Option<String>,
    pub modified: Option<String>,
    pub author: Option<String>,
    /// `<link rel="amphtml">`
    pub amphtml: Option<String>,
    /// 전체 요소 수
    pub dom_elements: usize,
    /// 최대 중첩 깊이
    pub dom_depth: usize,
    /// 다른 호스트에서 로드하는 스크립트의 호스트 목록
    pub third_party_script_hosts: Vec<String>,
    /// 질문형 헤딩 (h2~h4)
    pub question_headings: Vec<String>,
    /// 항목 2개 이상인 ul/ol 수
    pub lists: usize,
    /// 표 수 (role=presentation 제외)
    pub tables: usize,
    pub definition_lists: usize,
    /// body 안 noscript 요소 수와 텍스트 길이
    pub noscript_elements: usize,
    pub noscript_text_len: usize,
    /// 앱 연결 메타 (apple-itunes-app, al:*)
    pub app_link_meta: Vec<String>,
    /// 외부 스타일시트 절대 URL (문서 순서)
    pub stylesheet_urls: Vec<String>,
    /// 외부 스크립트 절대 URL (문서 순서)
    pub script_urls: Vec<String>,
    /// `<img src>` 절대 URL (http/https, 중복 제거)
    pub image_urls: Vec<String>,
    /// `<style>` 블록 분석
    pub inline_css: crate::css::CssFacts,
}

static STYLESHEET: LazyLock<Selector> = LazyLock::new(|| sel("link[rel~=stylesheet][href]"));
static STYLE_BLOCK: LazyLock<Selector> = LazyLock::new(|| sel("style"));
static IMG_SRC: LazyLock<Selector> = LazyLock::new(|| sel("img[src]"));

static SCRIPT_SRC: LazyLock<Selector> = LazyLock::new(|| sel("script[src]"));
static SUB_HEADINGS: LazyLock<Selector> = LazyLock::new(|| sel("h2, h3, h4"));
static LIST: LazyLock<Selector> = LazyLock::new(|| sel("ul, ol"));
static LIST_ITEM: LazyLock<Selector> = LazyLock::new(|| sel(":scope > li"));
static TABLE: LazyLock<Selector> = LazyLock::new(|| sel("table"));
static DL: LazyLock<Selector> = LazyLock::new(|| sel("dl"));
static NOSCRIPT: LazyLock<Selector> = LazyLock::new(|| sel("body noscript"));
static ALL_META: LazyLock<Selector> = LazyLock::new(|| sel("meta"));

/// 질문형 문장 끝 (한국어 의문형 어미 포함).
fn is_question(text: &str) -> bool {
    let t = text.trim().trim_end_matches(['.', '!']);
    t.ends_with('?')
        || t.ends_with('？')
        || [
            "인가요",
            "나요",
            "까요",
            "을까",
            "ㄹ까",
            "는지",
            "무엇인가",
            "어떻게",
            "왜",
        ]
        .iter()
        .any(|e| t.ends_with(e))
        || t.starts_with("어떻게 ")
        || t.starts_with("왜 ")
}

fn attr<'a>(el: &ElementRef<'a>, name: &str) -> Option<&'a str> {
    el.value()
        .attr(name)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn rels(el: &ElementRef) -> Vec<String> {
    el.value()
        .attr("rel")
        .unwrap_or_default()
        .split_ascii_whitespace()
        .map(str::to_ascii_lowercase)
        .collect()
}

pub fn in_svg(el: &ElementRef) -> bool {
    el.ancestors()
        .any(|a| a.value().as_element().is_some_and(|e| e.name() == "svg"))
}

fn image_format(src: &str) -> Option<String> {
    let path = src.split(['?', '#']).next().unwrap_or_default();
    // Next.js 이미지 최적화 경로는 원본 확장자가 쿼리에 있다
    let path = if src.contains("/_next/image") {
        src.split("url=")
            .nth(1)
            .and_then(|u| u.split('&').next())
            .unwrap_or(path)
    } else {
        path
    };
    let ext = path
        .rsplit('/')
        .next()?
        .rsplit_once('.')?
        .1
        .to_ascii_lowercase();
    let ext = match ext.as_str() {
        "jpeg" => "jpg".to_string(),
        "png" | "jpg" | "gif" | "webp" | "avif" | "svg" | "ico" | "bmp" => ext,
        _ => return None,
    };
    Some(ext)
}

fn walk_ld(v: &Value, out: &mut Vec<LdObject>) {
    match v {
        Value::Array(items) => items.iter().for_each(|i| walk_ld(i, out)),
        Value::Object(map) => {
            let types: Vec<String> = match map.get("@type") {
                Some(Value::String(t)) => vec![t.clone()],
                Some(Value::Array(ts)) => ts
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
                _ => Vec::new(),
            };
            if !types.is_empty() {
                out.push(LdObject {
                    types,
                    keys: map
                        .iter()
                        .filter(|(_, v)| !is_empty_value(v))
                        .map(|(k, _)| k.clone())
                        .collect(),
                });
            }
            map.values().for_each(|c| walk_ld(c, out));
        }
        _ => {}
    }
}

fn is_empty_value(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.trim().is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        _ => false,
    }
}

/// JSON-LD 전체에서 처음 나오는 키 값 (문자열 또는 객체의 name).
fn find_ld_value(v: &Value, key: &str) -> Option<String> {
    match v {
        Value::Array(items) => items.iter().find_map(|i| find_ld_value(i, key)),
        Value::Object(map) => {
            if let Some(found) = map.get(key).and_then(value_text) {
                return Some(found);
            }
            map.values().find_map(|c| find_ld_value(c, key))
        }
        _ => None,
    }
}

fn value_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Value::Object(o) => o.get("name").and_then(value_text),
        Value::Array(a) => a.iter().find_map(value_text),
        _ => None,
    }
}

impl LdObject {
    /// `name|alt` 형태 속성 중 하나라도 있으면 충족.
    pub fn has(&self, prop: &str) -> bool {
        prop.split('|').any(|p| self.keys.iter().any(|k| k == p))
    }
}

pub fn extract(doc: &Html, base: &Url) -> ExtraFacts {
    let mut f = ExtraFacts::default();

    // ===== 중복 태그 =====
    // SVG 안의 <title> 은 문서 제목이 아니다. Next.js 스트리밍 메타처럼 body 에 있는 것도 센다.
    f.tag_counts
        .insert("title", doc.select(&TITLE).filter(|t| !in_svg(t)).count());
    for key in ["description", "viewport", "robots"] {
        let n = doc
            .select(&META)
            .filter(|m| {
                m.value()
                    .attr("name")
                    .is_some_and(|n| n.trim().eq_ignore_ascii_case(key))
            })
            .count();
        f.tag_counts.insert(key, n);
    }
    f.tag_counts.insert(
        "canonical",
        doc.select(&LINK)
            .filter(|l| rels(l).iter().any(|r| r == "canonical"))
            .count(),
    );

    // ===== 헤딩 순서 =====
    let mut prev: Option<u8> = None;
    for h in doc.select(&HEADINGS) {
        let level = h.value().name().as_bytes()[1] - b'0';
        if let Some(p) = prev
            && level > p + 1
        {
            let s = format!("h{p}→h{level}");
            if !f.heading_skips.contains(&s) {
                f.heading_skips.push(s);
            }
        }
        f.heading_levels.push(level);
        prev = Some(level);
    }

    // ===== 링크 =====
    let mut current = base.clone();
    current.set_fragment(None);
    for a in doc.select(&ANCHOR) {
        let Some(href) = attr(&a, "href") else {
            continue;
        };
        let Ok(mut u) = base.join(href) else { continue };
        if !matches!(u.scheme(), "http" | "https") || u.host_str() != base.host_str() {
            continue;
        }
        if rels(&a).iter().any(|r| r == "nofollow") {
            f.internal_nofollow += 1;
        }
        u.set_fragment(None);
        let s = u.to_string();
        if u != current && !f.internal_link_urls.contains(&s) {
            f.internal_link_urls.push(s);
        }
    }

    // ===== 렌더링 차단 / 리소스 힌트 =====
    for s in doc.select(&HEAD_SCRIPT) {
        let v = s.value();
        let module = v
            .attr("type")
            .is_some_and(|t| t.trim().eq_ignore_ascii_case("module"));
        if v.attr("async").is_none() && v.attr("defer").is_none() && !module {
            f.render_blocking_scripts
                .push(v.attr("src").unwrap_or_default().to_string());
        }
    }
    f.head_stylesheets = doc.select(&HEAD_STYLESHEET).count();
    for l in doc.select(&LINK) {
        let r = rels(&l);
        if (r.iter().any(|x| x == "preconnect" || x == "dns-prefetch"))
            && let Some(h) = attr(&l, "href")
            && !f.preconnect.iter().any(|x| x == h)
        {
            f.preconnect.push(h.to_string());
        }
        let ty = l
            .value()
            .attr("type")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if r.iter().any(|x| x == "alternate")
            && ty == "text/markdown"
            && f.markdown_alternate.is_none()
        {
            f.markdown_alternate = attr(&l, "href").map(str::to_string);
        }
    }
    for el in doc.select(&RESOURCE) {
        let raw = el
            .value()
            .attr("src")
            .or_else(|| el.value().attr("href"))
            .unwrap_or_default();
        if let Ok(u) = base.join(raw.trim())
            && u.host_str() == base.host_str()
            && !f
                .same_origin_resources
                .iter()
                .any(|x| x == &u[url::Position::BeforePath..url::Position::AfterQuery])
        {
            // robots 규칙은 경로 + 쿼리에 대해 판정한다
            f.same_origin_resources
                .push(u[url::Position::BeforePath..url::Position::AfterQuery].to_string());
        }
    }

    // ===== 이미지 포맷 =====
    for img in doc.select(&IMG) {
        let v = img.value();
        if v.attr("srcset").is_some_and(|s| !s.trim().is_empty()) {
            f.img_srcset += 1;
        }
        if let Some(fmt) = v.attr("src").and_then(image_format) {
            *f.image_formats.entry(fmt).or_default() += 1;
        }
    }
    for s in doc.select(&PICTURE_SOURCE) {
        if let Some(t) = s
            .value()
            .attr("type")
            .and_then(|t| t.trim().strip_prefix("image/"))
        {
            let t = if t == "svg+xml" { "svg" } else { t };
            *f.image_formats.entry(t.to_ascii_lowercase()).or_default() += 1;
        }
    }

    // ===== JSON-LD 객체 / 작성자·날짜 =====
    let mut ld_values: Vec<Value> = Vec::new();
    for s in doc.select(&JSON_LD) {
        if !s
            .value()
            .attr("type")
            .is_some_and(|t| t.trim().eq_ignore_ascii_case("application/ld+json"))
        {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(s.text().collect::<String>().trim()) {
            walk_ld(&v, &mut f.ld_objects);
            ld_values.push(v);
        }
    }
    let meta = |k: &str| -> Option<String> {
        doc.select(&sel("meta"))
            .find(|m| {
                let v = m.value();
                v.attr("property")
                    .or_else(|| v.attr("name"))
                    .is_some_and(|n| n.trim().eq_ignore_ascii_case(k))
            })
            .and_then(|m| attr(&m, "content").map(str::to_string))
    };
    let ld = |k: &str| ld_values.iter().find_map(|v| find_ld_value(v, k));
    f.published = meta("article:published_time").or_else(|| ld("datePublished"));
    f.modified = meta("article:modified_time")
        .or_else(|| meta("og:updated_time"))
        .or_else(|| ld("dateModified"));
    // ===== AMP / 앱 링크 메타 =====
    for l in doc.select(&LINK) {
        if rels(&l).iter().any(|r| r == "amphtml") && f.amphtml.is_none() {
            f.amphtml = attr(&l, "href").map(str::to_string);
        }
    }
    for m in doc.select(&ALL_META) {
        let v = m.value();
        let key = v
            .attr("name")
            .or_else(|| v.attr("property"))
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if (key == "apple-itunes-app" || key.starts_with("al:")) && !f.app_link_meta.contains(&key)
        {
            f.app_link_meta.push(key);
        }
    }

    // ===== DOM 크기 =====
    for node in doc.root_element().descendants() {
        if node.value().is_element() {
            f.dom_elements += 1;
            let depth = node.ancestors().filter(|a| a.value().is_element()).count() + 1;
            f.dom_depth = f.dom_depth.max(depth);
        }
    }

    // ===== 리소스 URL / 인라인 CSS =====
    let push_abs = |list: &mut Vec<String>, raw: Option<&str>| {
        if let Some(u) = raw.and_then(|r| base.join(r.trim()).ok())
            && matches!(u.scheme(), "http" | "https")
            && !list.iter().any(|x| x == u.as_str())
        {
            list.push(u.to_string());
        }
    };
    for l in doc.select(&STYLESHEET) {
        push_abs(&mut f.stylesheet_urls, l.value().attr("href"));
    }
    for s in doc.select(&SCRIPT_SRC) {
        push_abs(&mut f.script_urls, s.value().attr("src"));
    }
    for i in doc.select(&IMG_SRC) {
        push_abs(&mut f.image_urls, i.value().attr("src"));
    }
    for s in doc.select(&STYLE_BLOCK) {
        f.inline_css
            .merge(&crate::css::analyze(&s.text().collect::<String>()));
    }

    // ===== 서드파티 스크립트 =====
    for s in doc.select(&SCRIPT_SRC) {
        if let Ok(u) = base.join(s.value().attr("src").unwrap_or_default().trim())
            && matches!(u.scheme(), "http" | "https")
            && u.host_str() != base.host_str()
            && let Some(h) = u.host_str()
            && !f.third_party_script_hosts.iter().any(|x| x == h)
        {
            f.third_party_script_hosts.push(h.to_string());
        }
    }

    // ===== GEO: 질문형 헤딩 / 목록·표 / noscript =====
    for h in doc.select(&SUB_HEADINGS) {
        let text = h
            .text()
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !text.is_empty() && is_question(&text) {
            f.question_headings.push(text);
        }
    }
    f.lists = doc
        .select(&LIST)
        .filter(|l| l.select(&LIST_ITEM).count() >= 2)
        .count();
    f.tables = doc
        .select(&TABLE)
        .filter(|t| {
            !t.value()
                .attr("role")
                .is_some_and(|r| r == "presentation" || r == "none")
        })
        .count();
    f.definition_lists = doc.select(&DL).count();
    for n in doc.select(&NOSCRIPT) {
        f.noscript_elements += 1;
        // noscript 내용은 파서에 따라 텍스트로 들어오므로 태그를 제거하고 센다
        let raw: String = n.text().collect();
        let stripped = TAG.replace_all(&raw, " ");
        f.noscript_text_len += stripped
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .count();
    }

    f.author = meta("author")
        .or_else(|| meta("article:author"))
        .or_else(|| ld("author"));

    f
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(html: &str) -> ExtraFacts {
        extract(
            &Html::parse_document(html),
            &Url::parse("https://a.com/ko/").unwrap(),
        )
    }

    #[test]
    fn duplicates_and_headings() {
        let f = run(
            r#"<html><head><title>a</title><meta name="description" content="x"><meta name="Description" content="y">
            <link rel="canonical" href="/"></head><body><svg><title>icon</title></svg><h1>a</h1><h2>b</h2><h4>c</h4><h2>d</h2><h3>e</h3></body></html>"#,
        );
        assert_eq!(f.tag_counts["title"], 1);
        assert_eq!(f.tag_counts["description"], 2);
        assert_eq!(f.tag_counts["canonical"], 1);
        assert_eq!(f.heading_levels, vec![1, 2, 4, 2, 3]);
        assert_eq!(f.heading_skips, vec!["h2→h4"]);
    }

    #[test]
    fn links_and_resources() {
        let f = run(r##"<html><head>
            <script src="/a.js"></script><script src="/b.js" defer></script><script type="module" src="/m.js"></script>
            <link rel="stylesheet" href="/s.css"><link rel="preconnect" href="https://cdn.x">
            <link rel="alternate" type="text/markdown" href="/ko.md">
            </head><body>
            <a href="/x" rel="nofollow">x</a><a href="/x#top">x</a><a href="/ko/">self</a><a href="https://b.com/">ext</a>
            <img src="/a.webp" srcset="/a.webp 1x"><img src="/_next/image?url=%2Fb.png&w=64"><picture><source type="image/avif" srcset="c.avif"><img src="c.JPEG"></picture>
            </body></html>"##);
        assert_eq!(f.internal_nofollow, 1);
        assert_eq!(f.internal_link_urls, vec!["https://a.com/x"]);
        assert_eq!(f.render_blocking_scripts, vec!["/a.js"]);
        assert_eq!(f.head_stylesheets, 1);
        assert_eq!(f.preconnect, vec!["https://cdn.x"]);
        assert_eq!(f.markdown_alternate.as_deref(), Some("/ko.md"));
        assert_eq!(
            f.same_origin_resources,
            vec!["/a.js", "/b.js", "/m.js", "/s.css"]
        );
        assert_eq!(f.img_srcset, 1);
        let fmts: Vec<_> = f
            .image_formats
            .iter()
            .map(|(k, v)| (k.as_str(), *v))
            .collect();
        assert_eq!(fmts, vec![("avif", 1), ("jpg", 1), ("png", 1), ("webp", 1)]);
    }

    #[test]
    fn geo_and_dom_facts() {
        let f = run(
            r#"<html><head><link rel="amphtml" href="/amp"><meta name="apple-itunes-app" content="app-id=1">
            <script src="https://www.googletagmanager.com/gtm.js"></script><script src="/local.js"></script></head>
            <body><h2>데브파이브는 어떤 회사인가요?</h2><h3>What is GEO?</h3><h2>서비스 소개</h2>
            <ul><li>a</li><li>b</li></ul><ol><li>only</li></ol><table><tr><td>1</td></tr></table><dl></dl>
            <noscript><p>JavaScript 를 켜 주세요</p></noscript></body></html>"#,
        );
        assert_eq!(f.amphtml.as_deref(), Some("/amp"));
        assert_eq!(f.app_link_meta, vec!["apple-itunes-app"]);
        assert_eq!(f.third_party_script_hosts, vec!["www.googletagmanager.com"]);
        assert_eq!(
            f.question_headings,
            vec!["데브파이브는 어떤 회사인가요?", "What is GEO?"]
        );
        assert_eq!((f.lists, f.tables, f.definition_lists), (1, 1, 1));
        assert_eq!(f.noscript_elements, 1);
        assert_eq!(
            f.noscript_text_len,
            "JavaScript 를 켜 주세요".chars().count()
        );
        assert!(f.dom_elements > 10 && f.dom_depth >= 4);
    }

    #[test]
    fn json_ld_props_and_dates() {
        let f = run(r#"<html><head><script type="application/ld+json">
            {"@context":"https://schema.org","@graph":[
              {"@type":"Organization","name":"D","url":"https://a.com","logo":""},
              {"@type":"Article","headline":"h","author":{"@type":"Person","name":"Kim"},"datePublished":"2026-01-01"}]}
            </script><meta property="article:modified_time" content="2026-02-02"></head></html>"#);
        let org = f
            .ld_objects
            .iter()
            .find(|o| o.types.contains(&"Organization".into()))
            .unwrap();
        assert!(
            org.has("name") && org.has("url") && !org.has("logo"),
            "empty logo is missing"
        );
        assert_eq!(f.published.as_deref(), Some("2026-01-01"));
        assert_eq!(f.modified.as_deref(), Some("2026-02-02"));
        assert_eq!(f.author.as_deref(), Some("Kim"));
    }
}
