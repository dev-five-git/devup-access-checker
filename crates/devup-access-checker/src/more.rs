//! 추가 접근성·성능 신호 (HTML 만으로 판단). 네트워크 접근 없음.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use url::Url;

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("valid selector")
}

/// WAI-ARIA 1.2 와 Graphics ARIA 역할. `doc-*` (DPUB-ARIA) 는 접두사로 허용한다.
const ARIA_ROLES: &[&str] = &[
    "alert",
    "alertdialog",
    "application",
    "article",
    "banner",
    "blockquote",
    "button",
    "caption",
    "cell",
    "checkbox",
    "code",
    "columnheader",
    "combobox",
    "complementary",
    "contentinfo",
    "definition",
    "deletion",
    "dialog",
    "directory",
    "document",
    "emphasis",
    "feed",
    "figure",
    "form",
    "generic",
    "grid",
    "gridcell",
    "group",
    "heading",
    "img",
    "insertion",
    "link",
    "list",
    "listbox",
    "listitem",
    "log",
    "main",
    "mark",
    "marquee",
    "math",
    "menu",
    "menubar",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "meter",
    "navigation",
    "none",
    "note",
    "option",
    "paragraph",
    "presentation",
    "progressbar",
    "radio",
    "radiogroup",
    "region",
    "row",
    "rowgroup",
    "rowheader",
    "scrollbar",
    "search",
    "searchbox",
    "separator",
    "slider",
    "spinbutton",
    "status",
    "strong",
    "subscript",
    "superscript",
    "switch",
    "tab",
    "table",
    "tablist",
    "tabpanel",
    "term",
    "textbox",
    "time",
    "timer",
    "toolbar",
    "tooltip",
    "tree",
    "treegrid",
    "treeitem",
    "graphics-document",
    "graphics-object",
    "graphics-symbol",
];

/// WAI-ARIA 1.2 의 aria-* 속성 (접두사 제외).
const ARIA_ATTRS: &[&str] = &[
    "activedescendant",
    "atomic",
    "autocomplete",
    "braillelabel",
    "brailleroledescription",
    "busy",
    "checked",
    "colcount",
    "colindex",
    "colindextext",
    "colspan",
    "controls",
    "current",
    "describedby",
    "description",
    "details",
    "disabled",
    "dropeffect",
    "errormessage",
    "expanded",
    "flowto",
    "grabbed",
    "haspopup",
    "hidden",
    "invalid",
    "keyshortcuts",
    "label",
    "labelledby",
    "level",
    "live",
    "modal",
    "multiline",
    "multiselectable",
    "orientation",
    "owns",
    "placeholder",
    "posinset",
    "pressed",
    "readonly",
    "relevant",
    "required",
    "roledescription",
    "rowcount",
    "rowindex",
    "rowindextext",
    "rowspan",
    "selected",
    "setsize",
    "sort",
    "valuemax",
    "valuemin",
    "valuenow",
    "valuetext",
];

static ALL: LazyLock<Selector> = LazyLock::new(|| sel("*"));
static ROLE: LazyLock<Selector> = LazyLock::new(|| sel("[role]"));
// 소리 없는(muted) 영상은 장식용 배경 영상이 대부분이라 자막 대상에서 뺀다
static VIDEO: LazyLock<Selector> = LazyLock::new(|| sel("video:not([muted])"));
static CAPTION_TRACK: LazyLock<Selector> =
    LazyLock::new(|| sel("track[kind=captions], track[kind=subtitles]"));
static AREA: LazyLock<Selector> = LazyLock::new(|| sel("area[href]"));
static OBJECT_EMBED: LazyLock<Selector> = LazyLock::new(|| sel("object, embed"));
static LANG: LazyLock<Selector> = LazyLock::new(|| sel("body [lang]"));
static INTERACTIVE_CONTAINER: LazyLock<Selector> = LazyLock::new(|| sel("a[href], button"));
static INTERACTIVE: LazyLock<Selector> = LazyLock::new(|| {
    sel(
        "a[href], button, input:not([type=hidden]), select, textarea, [tabindex]:not([tabindex='-1'])",
    )
});
static LIST_ITEM: LazyLock<Selector> = LazyLock::new(|| sel("li"));
static LIST: LazyLock<Selector> = LazyLock::new(|| sel("ul, ol"));
static HIDDEN_ROOT: LazyLock<Selector> =
    LazyLock::new(|| sel("html[aria-hidden=true], body[aria-hidden=true]"));
static ACCESSKEY: LazyLock<Selector> = LazyLock::new(|| sel("[accesskey]"));
static PRESENTATION: LazyLock<Selector> = LazyLock::new(|| sel("[role=presentation], [role=none]"));
static SVG_TITLE: LazyLock<Selector> = LazyLock::new(|| sel("title"));
static BODY_IMG: LazyLock<Selector> = LazyLock::new(|| sel("body img"));
static FETCHPRIORITY_HIGH: LazyLock<Selector> = LazyLock::new(|| sel("[fetchpriority=high]"));
static INLINE_SCRIPT: LazyLock<Selector> = LazyLock::new(|| sel("script:not([src])"));
static SCRIPT_SRC: LazyLock<Selector> = LazyLock::new(|| sel("script[src]"));
static STYLE: LazyLock<Selector> = LazyLock::new(|| sel("style"));
static UNLOAD_ATTR: LazyLock<Selector> = LazyLock::new(|| sel("body[onunload], [onunload]"));
static DOCUMENT_WRITE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"document\s*\.\s*write(?:ln)?\s*\(").unwrap());
static UNLOAD_LISTENER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:addEventListener\s*\(\s*['"]unload['"]|\bonunload\s*=)"#).unwrap()
});

#[derive(Debug, Clone, Default)]
pub struct MoreFacts {
    // ----- 접근성 -----
    pub role_elements: usize,
    /// ARIA 명세에 없는 role 값
    pub invalid_roles: Vec<String>,
    pub aria_attr_count: usize,
    /// ARIA 명세에 없는 aria-* 속성
    pub invalid_aria_attrs: Vec<String>,
    pub role_img: usize,
    /// role="img" 인데 접근 가능한 이름이 없는 요소 수
    pub role_img_unnamed: usize,
    pub videos: usize,
    pub videos_without_captions: usize,
    pub areas: usize,
    pub areas_without_alt: usize,
    pub objects: usize,
    pub objects_without_alt: usize,
    pub element_langs: usize,
    pub invalid_element_langs: Vec<String>,
    pub interactive_containers: usize,
    /// 링크·버튼 안에 다른 인터랙티브 요소가 든 경우
    pub nested_interactive: usize,
    pub list_elements: usize,
    /// ul/ol 밖의 li, li 가 아닌 ul/ol 직계 자식
    pub list_violations: usize,
    pub aria_hidden_root: bool,
    pub accesskeys: usize,
    pub duplicate_accesskeys: Vec<String>,
    pub presentation_roles: usize,
    /// role=presentation|none 인데 초점을 받는 요소
    pub presentation_focusable: usize,
    // ----- 성능 -----
    pub body_images: usize,
    /// 본문 첫 이미지(LCP 후보)가 loading="lazy"
    pub first_image_lazy: bool,
    pub fetchpriority_high: usize,
    pub document_write: bool,
    pub script_srcs: usize,
    /// 같은 src 로 두 번 이상 들어간 스크립트
    pub duplicate_scripts: Vec<String>,
    /// 인라인 스크립트 바이트 (JSON 데이터 블록 제외)
    pub inline_script_bytes: usize,
    pub inline_style_bytes: usize,
    /// unload 이벤트 처리기 (bfcache 를 막는다)
    pub unload_handler: bool,
}

fn attr<'a>(el: &ElementRef<'a>, name: &str) -> Option<&'a str> {
    el.value()
        .attr(name)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// 중복 없이 최대 20개까지 모은다.
fn push_unique(v: &mut Vec<String>, s: &str) {
    if v.len() < 20 && !v.iter().any(|x| x == s) {
        v.push(s.to_string());
    }
}

fn duplicates(values: impl Iterator<Item = String>) -> Vec<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut order = Vec::new();
    for v in values {
        let n = seen.entry(v.clone()).or_insert(0);
        *n += 1;
        if *n == 2 {
            order.push(v);
        }
    }
    order
}

fn is_data_script(el: &ElementRef) -> bool {
    attr(el, "type").is_some_and(|t| {
        let t = t.to_ascii_lowercase();
        t.contains("json") || t == "text/template" || t == "text/x-template"
    })
}

pub fn extract(doc: &Html, base: &Url) -> MoreFacts {
    let mut f = MoreFacts::default();

    // ARIA role / 속성
    for el in doc.select(&ROLE) {
        f.role_elements += 1;
        let role = el.value().attr("role").unwrap_or_default();
        let known = role.split_whitespace().any(|r| {
            let r = r.to_ascii_lowercase();
            ARIA_ROLES.contains(&r.as_str()) || r.starts_with("doc-")
        });
        if !known {
            push_unique(&mut f.invalid_roles, role.trim());
        }
        if role
            .split_whitespace()
            .next()
            .is_some_and(|r| r.eq_ignore_ascii_case("img"))
        {
            f.role_img += 1;
            let named = attr(&el, "aria-label").is_some()
                || attr(&el, "aria-labelledby").is_some()
                || el
                    .select(&SVG_TITLE)
                    .any(|t| !t.text().collect::<String>().trim().is_empty());
            if !named {
                f.role_img_unnamed += 1;
            }
        }
    }
    for el in doc.select(&ALL) {
        for (name, _) in el.value().attrs() {
            if let Some(a) = name.strip_prefix("aria-") {
                f.aria_attr_count += 1;
                if !ARIA_ATTRS.contains(&a) && !f.invalid_aria_attrs.iter().any(|x| x == name) {
                    f.invalid_aria_attrs.push(name.to_string());
                }
            }
        }
    }

    // 미디어·대체 텍스트
    for v in doc.select(&VIDEO) {
        f.videos += 1;
        if v.select(&CAPTION_TRACK).next().is_none() {
            f.videos_without_captions += 1;
        }
    }
    for a in doc.select(&AREA) {
        f.areas += 1;
        if attr(&a, "alt").is_none() && attr(&a, "aria-label").is_none() {
            f.areas_without_alt += 1;
        }
    }
    for o in doc.select(&OBJECT_EMBED) {
        f.objects += 1;
        let inner = o.text().collect::<String>();
        if attr(&o, "aria-label").is_none()
            && attr(&o, "title").is_none()
            && inner.trim().is_empty()
        {
            f.objects_without_alt += 1;
        }
    }

    // 요소 단위 lang
    for el in doc.select(&LANG) {
        f.element_langs += 1;
        let lang = el.value().attr("lang").unwrap_or_default();
        if !crate::a11y::valid_lang(lang) {
            push_unique(&mut f.invalid_element_langs, lang);
        }
    }

    // 중첩 인터랙티브
    for c in doc.select(&INTERACTIVE_CONTAINER) {
        f.interactive_containers += 1;
        if c.select(&INTERACTIVE).any(|d| d.id() != c.id()) {
            f.nested_interactive += 1;
        }
    }

    // 목록 구조
    for li in doc.select(&LIST_ITEM) {
        f.list_elements += 1;
        let parent = li
            .parent()
            .and_then(ElementRef::wrap)
            .map(|p| p.value().name());
        if !matches!(parent, Some("ul" | "ol" | "menu")) {
            f.list_violations += 1;
        }
    }
    for list in doc.select(&LIST) {
        f.list_elements += 1;
        f.list_violations += list
            .children()
            .filter_map(ElementRef::wrap)
            .filter(|c| !matches!(c.value().name(), "li" | "script" | "template"))
            .count();
    }

    f.aria_hidden_root = doc.select(&HIDDEN_ROOT).next().is_some();

    let keys: Vec<String> = doc
        .select(&ACCESSKEY)
        .filter_map(|e| attr(&e, "accesskey").map(str::to_lowercase))
        .collect();
    f.accesskeys = keys.len();
    f.duplicate_accesskeys = duplicates(keys.into_iter());

    for p in doc.select(&PRESENTATION) {
        f.presentation_roles += 1;
        let focusable = p.value().name() == "button"
            || p.value().name() == "select"
            || p.value().name() == "textarea"
            || (p.value().name() == "a" && p.value().attr("href").is_some())
            || (p.value().name() == "input" && p.value().attr("type") != Some("hidden"))
            || p.value().attr("tabindex").is_some_and(|t| t.trim() != "-1");
        if focusable {
            f.presentation_focusable += 1;
        }
    }

    // ----- 성능 -----
    let mut imgs = doc.select(&BODY_IMG);
    if let Some(first) = imgs.next() {
        f.first_image_lazy =
            attr(&first, "loading").is_some_and(|l| l.eq_ignore_ascii_case("lazy"));
        f.body_images = 1 + imgs.count();
    }
    f.fetchpriority_high = doc.select(&FETCHPRIORITY_HIGH).count();

    for s in doc.select(&INLINE_SCRIPT) {
        if is_data_script(&s) {
            continue;
        }
        let code: String = s.text().collect();
        f.inline_script_bytes += code.len();
        f.document_write |= DOCUMENT_WRITE.is_match(&code);
        f.unload_handler |= UNLOAD_LISTENER.is_match(&code);
    }
    f.unload_handler |= doc.select(&UNLOAD_ATTR).next().is_some();
    f.inline_style_bytes = doc
        .select(&STYLE)
        .map(|s| s.text().map(str::len).sum::<usize>())
        .sum();

    let srcs: Vec<String> = doc
        .select(&SCRIPT_SRC)
        .filter_map(|s| attr(&s, "src").and_then(|src| base.join(src).ok()))
        .map(|u| u.to_string())
        .collect();
    f.script_srcs = srcs.len();
    f.duplicate_scripts = duplicates(srcs.into_iter());
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(html: &str) -> MoreFacts {
        extract(
            &Html::parse_document(html),
            &Url::parse("https://a.com/").unwrap(),
        )
    }

    #[test]
    fn accessibility_signals() {
        let f = run(r##"<html><body aria-hidden="true">
<div role="buton">x</div><div role="buton">y</div><nav role="navigation doc-toc"></nav><span role="img"></span>
<svg role="img"><title>로고</title></svg><div role="img" aria-label="차트"></div>
<div aria-lable="오타" aria-label="정상"></div>
<video src="a.mp4"></video><video><track kind="captions" src="c.vtt"></video><video muted autoplay src="bg.mp4"></video>
<map><area href="/a"><area href="/b" alt="b"></map>
<object data="x.swf"></object><object data="y">대체 텍스트</object><embed src="z" title="z">
<p lang="ko">가</p><p lang="한국어">나</p>
<a href="/x"><button>중첩</button></a><button>단독</button>
<li>밖</li><ul><li>a</li><div>b</div></ul>
<a href="/1" accesskey="s">1</a><a href="/2" accesskey="S">2</a>
<a href="/p" role="presentation">p</a><span role="none">n</span>
</body></html>"##);
        assert_eq!(f.invalid_roles, ["buton"]);
        assert_eq!((f.role_img, f.role_img_unnamed), (3, 1));
        assert_eq!(f.invalid_aria_attrs, ["aria-lable"]);
        assert_eq!((f.videos, f.videos_without_captions), (2, 1));
        assert_eq!((f.areas, f.areas_without_alt), (2, 1));
        assert_eq!((f.objects, f.objects_without_alt), (3, 1));
        assert_eq!(f.invalid_element_langs, ["한국어"]);
        assert_eq!(f.nested_interactive, 1);
        assert_eq!(f.list_violations, 2);
        assert!(f.aria_hidden_root);
        assert_eq!(f.duplicate_accesskeys, ["s"]);
        assert_eq!((f.presentation_roles, f.presentation_focusable), (2, 1));
    }

    #[test]
    fn performance_signals() {
        let f = run(r#"<html><head>
<script src="/a.js"></script><script src="https://a.com/a.js"></script><script src="/b.js"></script>
<script>document.write("<p>"); window.addEventListener('unload', f);</script>
<script type="application/ld+json">{"document.write(":1}</script>
<style>a{color:red}</style>
</head><body><img src="/hero.jpg" loading="lazy" fetchpriority="high"><img src="/b.jpg"></body></html>"#);
        assert!(f.first_image_lazy);
        assert_eq!(f.body_images, 2);
        assert_eq!(f.fetchpriority_high, 1);
        assert!(f.document_write && f.unload_handler);
        assert_eq!(f.duplicate_scripts, ["https://a.com/a.js"]);
        assert_eq!(f.script_srcs, 3);
        assert!(f.inline_script_bytes > 0 && f.inline_style_bytes == 12);

        let f = run(r#"<html><body onunload="x()"><img src="/a.jpg"></body></html>"#);
        assert!(!f.first_image_lazy && f.unload_handler && !f.document_write);
        let f = run("<html><body></body></html>");
        assert_eq!(f.body_images, 0);
    }
}
