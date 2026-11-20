//! HTML 문서에서 SEO/GEO 관련 사실(fact)을 추출한다. 네트워크 접근 없음.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use scraper::{ElementRef, Html, Node, Selector};
use serde_json::Value;
use url::Url;

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("valid selector")
}

static META: LazyLock<Selector> = LazyLock::new(|| sel("meta"));
static LINK: LazyLock<Selector> = LazyLock::new(|| sel("link[rel]"));
static TITLE: LazyLock<Selector> = LazyLock::new(|| sel("title"));
static BASE: LazyLock<Selector> = LazyLock::new(|| sel("base[href]"));
static HTML: LazyLock<Selector> = LazyLock::new(|| sel("html"));
static BODY: LazyLock<Selector> = LazyLock::new(|| sel("body"));
static H1: LazyLock<Selector> = LazyLock::new(|| sel("h1"));
static H2: LazyLock<Selector> = LazyLock::new(|| sel("h2"));
static IMG: LazyLock<Selector> = LazyLock::new(|| sel("img"));
static ANCHOR: LazyLock<Selector> = LazyLock::new(|| sel("a[href]"));
static JSON_LD: LazyLock<Selector> = LazyLock::new(|| sel("script[type]"));
static ITEMSCOPE: LazyLock<Selector> = LazyLock::new(|| sel("[itemscope]"));
static SUBRESOURCE: LazyLock<Selector> = LazyLock::new(|| {
    sel(
        "img[src], script[src], iframe[src], source[src], video[src], audio[src], embed[src], link[rel~=stylesheet][href]",
    )
});
static WS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

/// 대체 언어 링크 (`<link rel="alternate" hreflang>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hreflang {
    pub lang: String,
    pub href: String,
}

/// JSON-LD 블록 분석 결과.
#[derive(Debug, Clone, Default)]
pub struct JsonLd {
    pub blocks: usize,
    pub invalid_blocks: usize,
    /// 모든 블록에서 발견된 `@type` (중복 제거, 발견 순서)
    pub types: Vec<String>,
    /// Organization 등의 `sameAs` 링크
    pub same_as: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PageFacts {
    /// `<base href>` 로 정해진 문서 기준 URL (없으면 None). 상대 링크는 이것을 기준으로 해석한다.
    pub document_base: Option<Url>,
    pub lang: Option<String>,
    pub title: Option<String>,
    pub charset: Option<String>,
    /// `<meta name>` (소문자 키) → content. 첫 번째 값만 보존.
    pub meta_name: HashMap<String, String>,
    /// `<meta property>` (소문자 키) → content. 첫 번째 값만 보존.
    pub meta_property: HashMap<String, String>,
    pub canonical: Option<String>,
    pub favicon: Option<String>,
    pub apple_touch_icon: Option<String>,
    pub manifest: Option<String>,
    pub hreflang: Vec<Hreflang>,
    /// RSS / Atom 피드 링크 (`<link rel="alternate" type="application/rss+xml">`)
    pub feeds: Vec<String>,
    /// `<meta http-equiv="refresh">` content
    pub meta_refresh: Option<String>,
    pub font_preloads: usize,
    pub image_preloads: usize,
    pub json_ld: JsonLd,
    pub has_microdata: bool,
    pub h1_count: usize,
    pub h2_count: usize,
    pub img_total: usize,
    pub img_without_alt: usize,
    /// width·height 속성이 하나라도 없는 이미지 수
    pub img_without_dimensions: usize,
    /// `loading="lazy"` 이미지 수
    pub img_lazy: usize,
    /// body 의 가시 텍스트 길이 (script/style/noscript/template 제외, 공백 축약)
    pub body_text_len: usize,
    pub internal_links: usize,
    pub external_links: usize,
    /// https 페이지에서 http:// 로 로드되는 하위 리소스
    pub insecure_subresources: Vec<String>,
    /// 중복 태그, 헤딩 순서, 리소스, 스키마 속성 등 추가 사실
    pub extra: crate::extra::ExtraFacts,
    /// 웹 접근성 사실
    pub a11y: crate::a11y::A11yFacts,
    pub more: crate::more::MoreFacts,
    pub rules: crate::rules::RuleFacts,
    /// 신뢰·법적 정보
    pub trust: crate::trust::TrustFacts,
}

impl PageFacts {
    /// 상대 링크 해석 기준: `<base href>` 가 있으면 그것, 없으면 페이지 URL.
    pub fn link_base<'a>(&'a self, page: &'a Url) -> &'a Url {
        self.document_base.as_ref().unwrap_or(page)
    }

    /// `name` 우선, 없으면 `property` 에서 찾는다 (og:/twitter: 는 두 방식이 혼용된다).
    pub fn meta(&self, key: &str) -> Option<&str> {
        let key = key.to_ascii_lowercase();
        self.meta_name
            .get(&key)
            .or_else(|| self.meta_property.get(&key))
            .map(String::as_str)
    }
}

fn attr<'a>(el: &ElementRef<'a>, name: &str) -> Option<&'a str> {
    el.value().attr(name)
}

fn clean(s: &str) -> String {
    WS.replace_all(s.trim(), " ").into_owned()
}

fn rel_tokens(el: &ElementRef) -> Vec<String> {
    attr(el, "rel")
        .unwrap_or_default()
        .split_ascii_whitespace()
        .map(|t| t.to_ascii_lowercase())
        .collect()
}

/// body 가시 텍스트 길이.
fn visible_text_len(doc: &Html) -> usize {
    let Some(body) = doc.select(&BODY).next() else {
        return 0;
    };
    let mut text = String::new();
    for node in body.descendants() {
        let Node::Text(t) = node.value() else {
            continue;
        };
        let hidden = node.ancestors().any(|a| {
            a.value()
                .as_element()
                .is_some_and(|e| matches!(e.name(), "script" | "style" | "noscript" | "template"))
        });
        if !hidden {
            text.push_str(t);
            text.push(' ');
        }
    }
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .count()
}

fn collect_types(v: &Value, types: &mut Vec<String>, same_as: &mut Vec<String>) {
    match v {
        Value::Array(items) => items.iter().for_each(|i| collect_types(i, types, same_as)),
        Value::Object(map) => {
            match map.get("@type") {
                Some(Value::String(t)) => push_unique(types, t),
                Some(Value::Array(ts)) => ts
                    .iter()
                    .filter_map(Value::as_str)
                    .for_each(|t| push_unique(types, t)),
                _ => {}
            }
            match map.get("sameAs") {
                Some(Value::String(s)) => push_unique(same_as, s),
                Some(Value::Array(ss)) => ss
                    .iter()
                    .filter_map(Value::as_str)
                    .for_each(|s| push_unique(same_as, s)),
                _ => {}
            }
            for (k, child) in map {
                if k != "@type" && k != "sameAs" {
                    collect_types(child, types, same_as);
                }
            }
        }
        _ => {}
    }
}

fn push_unique(list: &mut Vec<String>, s: &str) {
    if !list.iter().any(|x| x == s) {
        list.push(s.to_string());
    }
}

/// HTML 문서에서 사실을 추출한다. `base` 는 내부/외부 링크 구분과 혼합 콘텐츠 판별에 쓴다.
pub fn extract(html: &str, page_url: &Url) -> PageFacts {
    let doc = Html::parse_document(html);
    // 첫 번째 <base href> 만 유효하다 (HTML 명세)
    let document_base = doc
        .select(&BASE)
        .find_map(|b| b.value().attr("href"))
        .and_then(|h| page_url.join(h.trim()).ok())
        .filter(|u| matches!(u.scheme(), "http" | "https"));
    let base = document_base.as_ref().unwrap_or(page_url);
    let mut f = PageFacts {
        document_base: document_base.clone(),
        lang: doc
            .select(&HTML)
            .next()
            .and_then(|e| attr(&e, "lang"))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        title: doc
            .select(&TITLE)
            .find(|t| !crate::extra::in_svg(t))
            .map(|t| clean(&t.text().collect::<String>()))
            .filter(|s| !s.is_empty()),
        ..Default::default()
    };

    for m in doc.select(&META) {
        if f.charset.is_none() {
            if let Some(c) = attr(&m, "charset") {
                f.charset = Some(c.trim().to_string());
            } else if attr(&m, "http-equiv").is_some_and(|h| h.eq_ignore_ascii_case("content-type"))
                && let Some(c) = attr(&m, "content").and_then(|c| {
                    c.to_ascii_lowercase()
                        .split("charset=")
                        .nth(1)
                        .map(str::to_string)
                })
            {
                f.charset = Some(c.trim().to_string());
            }
        }
        let Some(content) = attr(&m, "content").map(|c| c.trim().to_string()) else {
            continue;
        };
        if f.meta_refresh.is_none()
            && attr(&m, "http-equiv").is_some_and(|h| h.trim().eq_ignore_ascii_case("refresh"))
        {
            f.meta_refresh = Some(content.clone());
        }
        if let Some(n) = attr(&m, "name") {
            f.meta_name
                .entry(n.trim().to_ascii_lowercase())
                .or_insert_with(|| content.clone());
        }
        if let Some(p) = attr(&m, "property") {
            f.meta_property
                .entry(p.trim().to_ascii_lowercase())
                .or_insert(content);
        }
    }

    for l in doc.select(&LINK) {
        let rels = rel_tokens(&l);
        let Some(href) = attr(&l, "href").map(str::trim).filter(|h| !h.is_empty()) else {
            continue;
        };
        let has = |r: &str| rels.iter().any(|x| x == r);
        if has("canonical") && f.canonical.is_none() {
            f.canonical = Some(href.to_string());
        }
        if has("icon") && f.favicon.is_none() {
            f.favicon = Some(href.to_string());
        }
        if (has("apple-touch-icon") || has("apple-touch-icon-precomposed"))
            && f.apple_touch_icon.is_none()
        {
            f.apple_touch_icon = Some(href.to_string());
        }
        if has("manifest") && f.manifest.is_none() {
            f.manifest = Some(href.to_string());
        }
        let ty = attr(&l, "type")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if has("alternate") && (ty == "application/rss+xml" || ty == "application/atom+xml") {
            push_unique(&mut f.feeds, href);
        }
        if has("alternate")
            && let Some(lang) = attr(&l, "hreflang")
        {
            f.hreflang.push(Hreflang {
                lang: lang.trim().to_string(),
                href: href.to_string(),
            });
        }
        if has("preload") {
            match attr(&l, "as").map(str::to_ascii_lowercase).as_deref() {
                Some("font") => f.font_preloads += 1,
                Some("image") => f.image_preloads += 1,
                _ => {}
            }
        }
    }

    for s in doc.select(&JSON_LD) {
        if !attr(&s, "type").is_some_and(|t| t.trim().eq_ignore_ascii_case("application/ld+json")) {
            continue;
        }
        f.json_ld.blocks += 1;
        let raw: String = s.text().collect();
        match serde_json::from_str::<Value>(raw.trim()) {
            Ok(v) => collect_types(&v, &mut f.json_ld.types, &mut f.json_ld.same_as),
            Err(_) => f.json_ld.invalid_blocks += 1,
        }
    }

    f.has_microdata = doc.select(&ITEMSCOPE).next().is_some();
    f.h1_count = doc.select(&H1).count();
    f.h2_count = doc.select(&H2).count();
    for img in doc.select(&IMG) {
        f.img_total += 1;
        if attr(&img, "alt").is_none() {
            f.img_without_alt += 1;
        }
        if attr(&img, "width").is_none() || attr(&img, "height").is_none() {
            f.img_without_dimensions += 1;
        }
        if attr(&img, "loading").is_some_and(|v| v.trim().eq_ignore_ascii_case("lazy")) {
            f.img_lazy += 1;
        }
    }
    f.body_text_len = visible_text_len(&doc);

    for a in doc.select(&ANCHOR) {
        let href = attr(&a, "href").unwrap_or_default().trim();
        if href.starts_with('#') || href.is_empty() {
            continue;
        }
        let Ok(u) = base.join(href) else { continue };
        match u.scheme() {
            "http" | "https" => {
                if u.host_str() == page_url.host_str() {
                    f.internal_links += 1;
                } else {
                    f.external_links += 1;
                }
            }
            _ => {}
        }
    }

    if page_url.scheme() == "https" {
        for el in doc.select(&SUBRESOURCE) {
            let src = attr(&el, "src")
                .or_else(|| attr(&el, "href"))
                .unwrap_or_default()
                .trim();
            if src.len() >= 7 && src[..7].eq_ignore_ascii_case("http://") {
                push_unique(&mut f.insecure_subresources, src);
            }
        }
    }

    f.extra = crate::extra::extract(&doc, base);
    f.a11y = crate::a11y::extract(&doc);
    f.trust = crate::trust::extract(&doc, base);
    f.more = crate::more::extract(&doc, base);
    f.rules = crate::rules::extract(&doc, html, page_url);
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://example.com/").unwrap()
    }

    #[test]
    fn extracts_head_meta() {
        let html = r#"<!doctype html><html lang="ko"><head>
            <meta charset="utf-8">
            <svg><title>icon</title></svg>
            <title>  Hello
               World </title>
            <meta name="Description" content=" desc ">
            <meta property="og:title" content="OG">
            <meta name="twitter:card" content="summary">
            <link rel="canonical" href="https://example.com/">
            <link rel="shortcut icon" href="/favicon.ico">
            <link rel="alternate" hreflang="en" href="/en">
            <link rel="preload" as="font" href="/a.woff2">
            <link as="image" rel="preload" href="/a.png">
            <link rel="alternate" type="application/rss+xml" href="/feed.xml">
            <meta http-equiv="refresh" content="0; url=/ko">
        </head><body></body></html>"#;
        let f = extract(html, &base());
        assert_eq!(f.lang.as_deref(), Some("ko"));
        assert_eq!(f.title.as_deref(), Some("Hello World"));
        assert_eq!(f.charset.as_deref(), Some("utf-8"));
        assert_eq!(f.meta("description"), Some("desc"));
        assert_eq!(f.meta("og:title"), Some("OG"));
        assert_eq!(f.meta("twitter:card"), Some("summary"));
        assert_eq!(f.canonical.as_deref(), Some("https://example.com/"));
        assert_eq!(f.favicon.as_deref(), Some("/favicon.ico"));
        assert_eq!(
            f.hreflang,
            vec![Hreflang {
                lang: "en".into(),
                href: "/en".into()
            }]
        );
        assert_eq!((f.font_preloads, f.image_preloads), (1, 1));
        assert_eq!(f.feeds, vec!["/feed.xml"]);
        assert_eq!(f.meta_refresh.as_deref(), Some("0; url=/ko"));
    }

    #[test]
    fn json_ld_types_and_invalid_blocks() {
        let html = r#"<html><head>
            <script type="application/ld+json">{"@context":"https://schema.org","@graph":[
              {"@type":"Organization","sameAs":["https://x.com/a"]},{"@type":["WebSite","Thing"]}]}</script>
            <script type="application/ld+json">{ broken</script>
        </head></html>"#;
        let f = extract(html, &base());
        assert_eq!(f.json_ld.blocks, 2);
        assert_eq!(f.json_ld.invalid_blocks, 1);
        assert_eq!(f.json_ld.types, vec!["Organization", "WebSite", "Thing"]);
        assert_eq!(f.json_ld.same_as, vec!["https://x.com/a"]);
    }

    #[test]
    fn body_structure() {
        let html = r##"<html><body>
            <h1>A</h1><h2>B</h2><h2>C</h2>
            <img src="a.png" alt="" width="1" height="1" loading="lazy"><img src="http://cdn.example.com/b.png" width="1">
            <script>var hidden = "not counted";</script>
            <p>Hello   world</p>
            <a href="/x">in</a><a href="https://other.com">out</a><a href="#top">skip</a>
            <a href="mailto:a@b.c">mail</a>
        </body></html>"##;
        let f = extract(html, &base());
        assert_eq!((f.h1_count, f.h2_count), (1, 2));
        assert_eq!((f.img_total, f.img_without_alt), (2, 1));
        assert_eq!((f.img_without_dimensions, f.img_lazy), (1, 1));
        assert_eq!((f.internal_links, f.external_links), (1, 1));
        assert_eq!(
            f.insecure_subresources,
            vec!["http://cdn.example.com/b.png"]
        );
        // "A B C Hello world in out skip mail"
        assert_eq!(
            f.body_text_len,
            "A B C Hello world in out skip mail".chars().count()
        );
    }

    #[test]
    fn no_mixed_content_on_http_page() {
        let html = r#"<html><body><img src="http://a.com/x.png"></body></html>"#;
        let f = extract(html, &Url::parse("http://example.com/").unwrap());
        assert!(f.insecure_subresources.is_empty());
    }

    #[test]
    fn base_href_sets_link_base() {
        let page = Url::parse("https://a.com/ko/about").unwrap();
        let f = extract(
            r#"<html><head><base href="/"><link rel="icon" href="favicon.ico"></head><body><a href="x">x</a></body></html>"#,
            &page,
        );
        assert_eq!(f.link_base(&page).as_str(), "https://a.com/");
        assert_eq!(
            f.link_base(&page)
                .join(f.favicon.as_deref().unwrap())
                .unwrap()
                .as_str(),
            "https://a.com/favicon.ico"
        );
        let f = extract("<html><head></head></html>", &page);
        assert_eq!(f.link_base(&page), &page);
    }

    /// 자주 나오지 않는 마크업 형태들을 한 문서에 모아 추출 결과를 확인한다.
    #[test]
    fn extracts_less_common_markup() {
        let long_alt = "가".repeat(130);
        let html = format!(
            r#"<html><head>
<meta http-equiv="Content-Type" content="text/html; charset=EUC-KR">
<link rel="icon" href="">
<link rel="apple-touch-icon" href="/apple.png">
<link rel="preload" as="script" href="/x.js">
<script type="text/javascript">var x = 1;</script>
<script type="application/ld+json">{{"@type":"Article","sameAs":"https://x.com/a","name":null,"wordCount":5,
  "datePublished":["2024-01-01"],"dateModified":5,"x":{{"dateModified":"2024-02-02"}}}}</script>
<style>a:focus-visible {{ outline: 2px solid; }}</style>
</head><body>
<input type="button" value="보내기"><input type="button">
<img src="/a.tiff" alt="{long_alt}">
<a name="anchor-only" href=" ">x</a>
</body></html>"#
        );
        let f = extract(&html, &base());
        assert_eq!(f.charset.as_deref(), Some("euc-kr"));
        assert_eq!(f.favicon, None, "빈 href 는 무시");
        assert_eq!(f.apple_touch_icon.as_deref(), Some("/apple.png"));
        assert_eq!(f.font_preloads, 0);
        assert_eq!(f.json_ld.same_as, vec!["https://x.com/a".to_string()]);
        assert_eq!(f.a11y.buttons, 2);
        assert_eq!(f.a11y.unnamed_buttons, 1);
        assert_eq!(f.a11y.long_alts, 1);
        assert!(
            f.extra.image_formats.is_empty(),
            "알 수 없는 확장자는 집계하지 않음"
        );
        let article = &f.extra.ld_objects[0];
        assert!(
            !article.keys.contains(&"name".to_string()),
            "null 값은 빈 값"
        );
        assert!(article.keys.contains(&"wordCount".to_string()));
        assert_eq!(f.extra.published.as_deref(), Some("2024-01-01"));
        assert_eq!(f.extra.modified.as_deref(), Some("2024-02-02"));
        assert!(f.extra.inline_css.has_focus_visible);
    }

    /// frameset 문서에는 body 가 없다.
    #[test]
    fn frameset_document_has_no_body_text() {
        let f = extract(
            r#"<html><head><title>t</title></head><frameset><frame src="a.html"></frameset></html>"#,
            &base(),
        );
        assert_eq!(f.body_text_len, 0);
    }

    /// 특이한 마크업에서도 추출이 패닉하지 않아야 한다 (실제 사이트에서 template 로 패닉한 적이 있음).
    #[test]
    fn extract_never_panics_on_odd_markup() {
        let cases = [
            "",
            "plain text",
            "<template><p>a</p>텍스트<template>중첩</template></template>",
            "<svg><title>t</title><a href='x'>링크</a><foreignObject><p>p</p></foreignObject></svg>",
            "<math><mi>x</mi></math>",
            "<html><frameset><frame src=a></frameset></html>",
            "<table><tr><td><table><tr><th>중첩</th></tr></table></td></tr></table>",
            "<div><span><a href=",
            "<p>&#xFFFFFF; &#0; &unknown;</p>",
            "<input type=radio name><label for>x</label><a href='javascript:'>j</a>",
            "<div role aria-label aria-hidden lang xml:lang tabindex accesskey style onclick>x</div>",
            "<base href='http://['><link rel=icon href='http://['><img src='http://[' alt>",
            "<select><option>a<optgroup><option>b</select><dl><dd>x</dl>",
            "<noscript><p>대체</p></noscript><script>document.write('<p>')</script>",
        ];
        let page = base();
        for html in cases {
            let f = extract(html, &page);
            let _ = f.link_base(&page);
        }
        let deep = "<div>".repeat(5_000);
        let _ = extract(&deep, &page);
    }
}
