//! Google 검색 센터·Yandex·Daum·Apple 문서와 대조해 추가한 정적 신호. 네트워크 접근 없음.

use std::sync::LazyLock;

use regex::Regex;
use scraper::{ElementRef, Html, Node, Selector};
use url::Url;

static ALL: LazyLock<Selector> = LazyLock::new(|| Selector::parse("*").expect("valid selector"));
static HIDDEN_STYLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)font-size\s*:\s*0(?:px|em|rem|%)?\s*(?:;|$)|opacity\s*:\s*0(?:\.0+)?\s*(?:;|$)|text-indent\s*:\s*-\d{3,}|(?:left|top)\s*:\s*-\d{3,}px").unwrap()
});
static YANDEX_NOINDEX_OPEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<noindex>|<!--\s*noindex\s*-->").unwrap());
static YANDEX_NOINDEX_CLOSE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)</noindex>|<!--\s*/noindex\s*-->").unwrap());

/// Google 이미지가 지원하는 형식 (확장자 기준).
const IMAGE_FORMATS: &[&str] = &[
    "bmp", "gif", "jpg", "jpeg", "png", "webp", "svg", "avif", "ico",
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HreflangLink {
    pub lang: String,
    pub href: String,
    pub media: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GoogleFacts {
    /// data-nosnippet 이 span·div·section 외 요소에 붙은 수
    pub data_nosnippet_invalid: Vec<String>,
    pub hreflang: Vec<HreflangLink>,
    pub rel_prev: bool,
    pub rel_next: bool,
    /// src 없는 img (data-src 만 등)·img 없는 picture
    pub img_missing_src: usize,
    /// Google 이미지 미지원 형식의 img src
    pub img_unsupported: Vec<String>,
    /// 인라인 style 로 숨긴 텍스트·링크
    pub hidden_text: Vec<String>,
    /// 퍼센트 인코딩 안 된 링크 URL (공백·비ASCII)
    pub unencoded_links: Vec<String>,
    /// 외부 링크 rel 별 수 (sponsored, ugc, nofollow)
    pub outbound_sponsored: usize,
    pub outbound_ugc: usize,
    pub outbound_nofollow: usize,
    pub outbound_total: usize,
    /// 밑줄로 단어를 구분한 같은 호스트 링크 경로
    pub underscore_paths: Vec<String>,
    /// Yandex `<noindex>` 여는·닫는 태그 수
    pub yandex_noindex_open: usize,
    pub yandex_noindex_close: usize,
    /// apple-itunes-app meta 가 head 밖에 있음
    pub itunes_meta_outside_head: bool,
    /// 본문에서 가장 많이 쓴 문자 체계
    pub body_script: Option<&'static str>,
}

fn push(v: &mut Vec<String>, s: &str) {
    if v.len() < 20 && !v.iter().any(|x| x == s) {
        v.push(s.to_string());
    }
}

fn own_text(el: &ElementRef) -> bool {
    el.children()
        .any(|c| matches!(c.value(), Node::Text(t) if !t.trim().is_empty()))
}

pub fn extract(doc: &Html, html: &str, base: &Url) -> GoogleFacts {
    let mut f = GoogleFacts {
        yandex_noindex_open: YANDEX_NOINDEX_OPEN.find_iter(html).count(),
        yandex_noindex_close: YANDEX_NOINDEX_CLOSE.find_iter(html).count(),
        ..Default::default()
    };
    let host = base.host_str().unwrap_or_default();
    for el in doc.select(&ALL) {
        let e = el.value();
        let name = e.name();
        let get = |a: &str| e.attr(a).map(str::trim);
        let rel = get("rel").unwrap_or_default().to_ascii_lowercase();
        let rels: Vec<&str> = rel.split_ascii_whitespace().collect();
        if e.attr("data-nosnippet").is_some() && !matches!(name, "span" | "div" | "section") {
            push(&mut f.data_nosnippet_invalid, name);
        }
        if name == "link"
            && rels.contains(&"alternate")
            && let Some(lang) = get("hreflang")
        {
            f.hreflang.push(HreflangLink {
                lang: lang.to_string(),
                href: get("href").unwrap_or_default().to_string(),
                media: e.attr("media").is_some(),
            });
        }
        if matches!(name, "link" | "a") {
            f.rel_prev |= rels.contains(&"prev") || rels.contains(&"previous");
            f.rel_next |= rels.contains(&"next");
        }
        if name == "img" {
            let src = get("src").unwrap_or_default();
            if src.is_empty() {
                f.img_missing_src += 1;
            } else if !src.starts_with("data:") {
                let path = src.split(['?', '#']).next().unwrap_or_default();
                let file = path.rsplit('/').next().unwrap_or_default();
                if let Some((_, ext)) = file.rsplit_once('.')
                    && ext.len() <= 5
                    && !IMAGE_FORMATS.contains(&ext.to_ascii_lowercase().as_str())
                {
                    push(&mut f.img_unsupported, src);
                }
            }
        }
        if name == "picture"
            && !el
                .children()
                .filter_map(ElementRef::wrap)
                .any(|c| c.value().name() == "img")
        {
            f.img_missing_src += 1;
        }
        if let Some(style) = get("style")
            && HIDDEN_STYLE.is_match(style)
            && (own_text(&el) || name == "a")
        {
            push(&mut f.hidden_text, &format!("{name}[style=\"{style}\"]"));
        }
        if name == "a"
            && let Some(href) = e.attr("href")
        {
            let h = href.trim();
            if h.contains(' ') || !h.is_ascii() {
                push(&mut f.unencoded_links, h);
            }
            if let Ok(u) = base.join(h)
                && matches!(u.scheme(), "http" | "https")
            {
                if u.host_str() == Some(host) {
                    if u.path().contains('_') {
                        push(&mut f.underscore_paths, u.path());
                    }
                } else {
                    f.outbound_total += 1;
                    f.outbound_sponsored += usize::from(rels.contains(&"sponsored"));
                    f.outbound_ugc += usize::from(rels.contains(&"ugc"));
                    f.outbound_nofollow += usize::from(rels.contains(&"nofollow"));
                }
            }
        }
        if name == "meta"
            && get("name").is_some_and(|n| n.eq_ignore_ascii_case("apple-itunes-app"))
            && !el
                .ancestors()
                .filter_map(ElementRef::wrap)
                .any(|a| a.value().name() == "head")
        {
            f.itunes_meta_outside_head = true;
        }
    }
    let mut text = String::new();
    if let Some(body) = doc
        .select(&Selector::parse("body").expect("valid selector"))
        .next()
    {
        for node in body.descendants() {
            if let Some(t) = node.value().as_text()
                && !node.ancestors().filter_map(ElementRef::wrap).any(|a| {
                    matches!(
                        a.value().name(),
                        "script" | "style" | "noscript" | "template"
                    )
                })
                && text.len() < 20_000
            {
                text.push_str(t);
            }
        }
    }
    f.body_script = dominant_script(&text);
    f
}

/// Google 이 정의한 robots meta 규칙 (robots-tags.json) + 다른 검색엔진의 알려진 값.
pub const KNOWN_ROBOTS: &[&str] = &[
    "all",
    "noindex",
    "nofollow",
    "none",
    "nosnippet",
    "indexifembedded",
    "max-snippet",
    "max-image-preview",
    "max-video-preview",
    "notranslate",
    "noimageindex",
    "unavailable_after",
    "index",
    "follow",
    "noarchive",
    "nocache",
    "nosourceinfo",
    "noodp",
    "noydir",
    "noyaca",
    "ai-training",
    "ai-grounding",
];

/// hreflang 값이 ISO 639-1(-ISO 3166-1 alpha-2) 또는 x-default 인지.
pub fn hreflang_valid(code: &str) -> bool {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?:x-default|[a-z]{2}(?:-[a-z]{4})?(?:-[a-z]{2})?)$").unwrap()
    });
    RE.is_match(code.trim()) && !code.trim().eq_ignore_ascii_case("en-uk")
}

/// 문자열에서 가장 많이 쓴 문자 체계 (hangul/latin/han/kana/cyrillic/arabic/other). 글자가 없으면 None.
pub fn dominant_script(text: &str) -> Option<&'static str> {
    let mut counts = [0usize; 6];
    for c in text.chars().filter(|c| c.is_alphabetic()) {
        let i = match c as u32 {
            0xAC00..=0xD7A3 | 0x1100..=0x11FF | 0x3130..=0x318F => 0,
            0x41..=0x5A | 0x61..=0x7A | 0xC0..=0x24F => 1,
            0x4E00..=0x9FFF | 0x3400..=0x4DBF => 2,
            0x3040..=0x30FF => 3,
            0x400..=0x4FF => 4,
            0x600..=0x6FF => 5,
            _ => continue,
        };
        counts[i] += 1;
    }
    let (i, n) = counts.iter().enumerate().max_by_key(|(_, n)| **n)?;
    (*n > 0).then_some(["hangul", "latin", "han", "kana", "cyrillic", "arabic"][i])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_signals() {
        let base = Url::parse("https://a.com/").unwrap();
        let html = r#"<html><head><link rel="alternate" hreflang="en" href="https://a.com/en" media="only screen">
<link rel="alternate" hreflang="ko" href="/ko"><link rel="prev" href="/p1"><meta name="apple-itunes-app" content="app-id=1"></head>
<body><p data-nosnippet>x</p><span data-nosnippet>ok</span><img data-src="a.jpg"><img src="b.tiff?x=1"><img src="c.webp"><img src="data:image/png;base64,x">
<picture><source srcset="d.avif"></picture><picture><img src="e.png"></picture>
<p style="font-size:0">숨김</p><a href="/x" style="text-indent:-9999px"></a><div style="opacity:0"><span>안쪽</span></div>
<a href="/검색 결과">한글</a><a href="https://b.com/" rel="sponsored nofollow">광고</a><a href="https://c.com/" rel="ugc">댓글</a>
<a href="/my_page">밑줄</a><a href="mailto:a@a.com">메일</a><a rel="next" href="/p3">다음</a>
<noindex>숨김</noindex><!--noindex--></body></html>"#;
        let f = extract(&Html::parse_document(html), html, &base);
        assert_eq!(f.data_nosnippet_invalid, ["p"]);
        assert_eq!(f.hreflang.len(), 2);
        assert!(f.hreflang[0].media && !f.hreflang[1].media);
        assert!(f.rel_prev && f.rel_next);
        assert_eq!(f.img_missing_src, 2);
        assert_eq!(f.img_unsupported, ["b.tiff?x=1"]);
        assert_eq!(f.hidden_text.len(), 2, "{:?}", f.hidden_text);
        assert_eq!(f.unencoded_links, ["/검색 결과"]);
        assert_eq!(
            (
                f.outbound_total,
                f.outbound_sponsored,
                f.outbound_ugc,
                f.outbound_nofollow
            ),
            (2, 1, 1, 1)
        );
        assert_eq!(f.underscore_paths, ["/my_page"]);
        assert_eq!((f.yandex_noindex_open, f.yandex_noindex_close), (2, 1));
        assert!(!f.itunes_meta_outside_head);
        assert_eq!(f.body_script, Some("hangul"));
        let html = r#"<html><body><meta name="apple-itunes-app" content="x"></body></html>"#;
        assert!(extract(&Html::parse_document(html), html, &base).itunes_meta_outside_head);
    }

    #[test]
    fn helpers() {
        for c in ["en", "ko-KR", "zh-Hant-TW", "x-default", "EN-us"] {
            assert!(hreflang_valid(c), "{c}");
        }
        for c in ["en-UK", "kor", "es-419", ""] {
            assert!(!hreflang_valid(c), "{c}");
        }
        assert_eq!(
            dominant_script("데브파이브 홈페이지 DevFive"),
            Some("hangul")
        );
        assert_eq!(dominant_script("Hello 세계"), Some("latin"));
        assert_eq!(dominant_script("中文 日本語 かな"), Some("han"));
        assert_eq!(dominant_script("かなカナ"), Some("kana"));
        assert_eq!(dominant_script("Привет"), Some("cyrillic"));
        assert_eq!(dominant_script("مرحبا"), Some("arabic"));
        assert_eq!(dominant_script("123 !!"), None);
        assert_eq!(dominant_script("ไทย"), None);
    }
}
