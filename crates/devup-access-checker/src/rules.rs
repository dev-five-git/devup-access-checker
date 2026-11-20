//! 표준 점검 목록(axe-core 규칙, KWCAG 2.2, Lighthouse, Google 검색 센터)과 대조해 추가한 정적 신호.
//! 대조 결과는 `docs/coverage.md` 에 있다. 네트워크 접근 없음.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use scraper::{ElementRef, Html, Node, Selector};
use url::Url;

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("valid selector")
}

fn attr<'a>(el: &ElementRef<'a>, name: &str) -> Option<&'a str> {
    el.value()
        .attr(name)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn text(el: &ElementRef) -> String {
    el.text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// 화면에 보일 수 있는 텍스트: script·style·noscript·template, aria-hidden·hidden 하위는 뺀다.
fn visible_text(el: &ElementRef) -> String {
    let mut out = Vec::new();
    for node in el.descendants() {
        let Node::Text(t) = node.value() else {
            continue;
        };
        let hidden = node
            .ancestors()
            .filter_map(ElementRef::wrap)
            .take_while(|a| a.id() != el.id())
            .any(|a| {
                matches!(
                    a.value().name(),
                    "script" | "style" | "noscript" | "template"
                ) || a.value().attr("aria-hidden") == Some("true")
                    || a.value().attr("hidden").is_some()
            });
        if !hidden {
            out.push(t.trim());
        }
    }
    out.join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// 비교용: 소문자 글자·숫자만 남긴다.
fn alnum(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn first_role(el: &ElementRef) -> Option<String> {
    el.value()
        .attr("role")
        .and_then(|r| r.split_whitespace().next())
        .map(str::to_ascii_lowercase)
}

/// 중복 없이 최대 20개까지 모은다.
fn push_unique(v: &mut Vec<String>, s: &str) {
    if v.len() < 20 && !v.iter().any(|x| x == s) {
        v.push(s.to_string());
    }
}

/// 값 형식이 정해진 ARIA 속성: (속성, 허용 값). 빈 목록은 정수.
const ARIA_VALUE_RULES: &[(&str, &[&str])] = &[
    ("aria-hidden", &["true", "false", "undefined"]),
    ("aria-disabled", &["true", "false"]),
    ("aria-readonly", &["true", "false"]),
    ("aria-required", &["true", "false"]),
    ("aria-multiline", &["true", "false"]),
    ("aria-multiselectable", &["true", "false"]),
    ("aria-modal", &["true", "false"]),
    ("aria-atomic", &["true", "false"]),
    ("aria-busy", &["true", "false"]),
    ("aria-expanded", &["true", "false", "undefined"]),
    ("aria-selected", &["true", "false", "undefined"]),
    ("aria-checked", &["true", "false", "mixed", "undefined"]),
    ("aria-pressed", &["true", "false", "mixed", "undefined"]),
    ("aria-invalid", &["true", "false", "grammar", "spelling"]),
    (
        "aria-current",
        &["page", "step", "location", "date", "time", "true", "false"],
    ),
    (
        "aria-haspopup",
        &["true", "false", "menu", "listbox", "tree", "grid", "dialog"],
    ),
    ("aria-live", &["off", "polite", "assertive"]),
    ("aria-orientation", &["horizontal", "vertical", "undefined"]),
    ("aria-sort", &["ascending", "descending", "none", "other"]),
    ("aria-autocomplete", &["inline", "list", "both", "none"]),
    ("aria-level", &[]),
    ("aria-posinset", &[]),
    ("aria-setsize", &[]),
    ("aria-colcount", &[]),
    ("aria-colindex", &[]),
    ("aria-colspan", &[]),
    ("aria-rowcount", &[]),
    ("aria-rowindex", &[]),
    ("aria-rowspan", &[]),
];

/// 이름(aria-label 등)을 붙일 수 없는 역할 (WAI-ARIA 1.2 "Prohibited")
const NAME_PROHIBITED_ROLES: &[&str] = &[
    "caption",
    "code",
    "deletion",
    "emphasis",
    "generic",
    "insertion",
    "paragraph",
    "presentation",
    "none",
    "strong",
    "subscript",
    "superscript",
    "mark",
    "time",
    "suggestion",
];

/// 컨테이너 역할 → 자식으로 있어야 하는 역할
const REQUIRED_CHILDREN: &[(&str, &[&str])] = &[
    ("list", &["listitem"]),
    (
        "menu",
        &["menuitem", "menuitemcheckbox", "menuitemradio", "group"],
    ),
    (
        "menubar",
        &["menuitem", "menuitemcheckbox", "menuitemradio", "group"],
    ),
    ("tablist", &["tab"]),
    ("listbox", &["option", "group"]),
    ("tree", &["treeitem", "group"]),
    ("radiogroup", &["radio"]),
    ("grid", &["row", "rowgroup"]),
    ("table", &["row", "rowgroup"]),
    ("treegrid", &["row", "rowgroup"]),
    ("feed", &["article"]),
];

/// 역할 → 조상으로 있어야 하는 역할
const REQUIRED_PARENT: &[(&str, &[&str])] = &[
    ("tab", &["tablist"]),
    ("option", &["listbox", "group", "combobox"]),
    ("menuitem", &["menu", "menubar", "group"]),
    ("menuitemcheckbox", &["menu", "menubar", "group"]),
    ("menuitemradio", &["menu", "menubar", "group"]),
    ("treeitem", &["tree", "group"]),
    ("listitem", &["list", "group", "directory"]),
    ("row", &["table", "grid", "treegrid", "rowgroup"]),
    ("cell", &["row"]),
    ("gridcell", &["row"]),
    ("columnheader", &["row"]),
    ("rowheader", &["row"]),
];

/// ARIA 1.2 에서 폐기된 역할
const DEPRECATED_ROLES: &[&str] = &["directory", "doc-biblioentry", "doc-endnote"];

/// 랜드마크 요소·역할 (region 규칙의 "랜드마크 안" 판정용)
const LANDMARK_TAGS: &[&str] = &["header", "nav", "main", "footer", "aside", "form", "search"];
const LANDMARK_ROLES: &[&str] = &[
    "banner",
    "navigation",
    "main",
    "contentinfo",
    "complementary",
    "form",
    "search",
    "region",
];
/// header·footer 를 banner·contentinfo 가 아니게 만드는 조상 (HTML-AAM)
const SECTIONING: &[&str] = &["article", "aside", "main", "nav", "section"];

static ALL: LazyLock<Selector> = LazyLock::new(|| sel("*"));
static ROLE: LazyLock<Selector> = LazyLock::new(|| sel("[role]"));
static LABELLED: LazyLock<Selector> = LazyLock::new(|| sel("[aria-label], [aria-labelledby]"));
static CONTROLS: LazyLock<Selector> = LazyLock::new(|| sel("input, select, textarea"));
static LABEL_FOR: LazyLock<Selector> = LazyLock::new(|| sel("label[for]"));
static NAMED_BY_LABEL: LazyLock<Selector> = LazyLock::new(|| {
    sel("a[aria-label], button[aria-label], [role=button][aria-label], [role=link][aria-label]")
});
static STATUS_WIDGET: LazyLock<Selector> =
    LazyLock::new(|| sel("meter, progress, [role=meter], [role=progressbar], [role=tooltip]"));
static IFRAME: LazyLock<Selector> = LazyLock::new(|| sel("iframe[title]"));
static ISMAP: LazyLock<Selector> = LazyLock::new(|| sel("img[ismap]"));
static DL: LazyLock<Selector> = LazyLock::new(|| sel("dl"));
static DT_DD: LazyLock<Selector> = LazyLock::new(|| sel("dt, dd"));
static DATA_TABLE: LazyLock<Selector> = LazyLock::new(|| sel("table"));
static TABLE_TH: LazyLock<Selector> = LazyLock::new(|| sel("th"));
static CAPTION: LazyLock<Selector> = LazyLock::new(|| sel("caption"));
static STYLE_ATTR: LazyLock<Selector> = LazyLock::new(|| sel("[style]"));
static PASTE: LazyLock<Selector> = LazyLock::new(|| sel("input[onpaste], textarea[onpaste]"));
static ANCHOR: LazyLock<Selector> = LazyLock::new(|| sel("a"));
static HTML_EL: LazyLock<Selector> = LazyLock::new(|| sel("html"));
static BODY: LazyLock<Selector> = LazyLock::new(|| sel("body"));
static HEADER_FOOTER_MAIN: LazyLock<Selector> =
    LazyLock::new(|| sel("header, footer, main, [role=banner], [role=contentinfo], [role=main]"));
static EXTERNAL_RESOURCE: LazyLock<Selector> =
    LazyLock::new(|| sel("script[src], link[rel~=stylesheet][href]"));
static FORM: LazyLock<Selector> = LazyLock::new(|| sel("form[action]"));
static BLANK: LazyLock<Selector> = LazyLock::new(|| sel("a[target=_blank][href]"));
static PLUGIN: LazyLock<Selector> = LazyLock::new(|| sel("object, embed, applet"));
static LINK_ARD: LazyLock<Selector> = LazyLock::new(|| sel("link[rel~=ard i][href]"));
static LINK_AI_CATALOG: LazyLock<Selector> = LazyLock::new(|| sel("link[rel~=ai-catalog i][href]"));
static ALL_FORMS: LazyLock<Selector> = LazyLock::new(|| sel("form"));
static INLINE_SCRIPT: LazyLock<Selector> = LazyLock::new(|| sel("script:not([src])"));
static NOSNIPPET_ATTR: LazyLock<Selector> = LazyLock::new(|| sel("[data-nosnippet]"));
static INLINE_SPACING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:letter-spacing|word-spacing|line-height)\s*:[^;]*!important").unwrap()
});
static PASTE_BLOCK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)return\s+false|preventDefault").unwrap());
static PLUGIN_TYPE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)x-shockwave-flash|x-java|silverlight|\.swf\b|\.class\b|x-director").unwrap()
});

#[derive(Debug, Clone, Default)]
pub struct RuleFacts {
    // ----- 마크업 -----
    pub has_html5_doctype: bool,
    /// HTML 파서 오류 수와 앞 10개
    pub parse_errors: usize,
    pub parse_error_samples: Vec<String>,
    // ----- ARIA -----
    pub aria_value_attrs: usize,
    pub aria_invalid_values: Vec<String>,
    pub labelled_elements: usize,
    /// 이름을 붙일 수 없는 요소(역할 없는 div·span 등)의 aria-label/labelledby
    pub aria_name_prohibited: Vec<String>,
    pub containers: usize,
    pub missing_required_children: Vec<String>,
    pub child_roles: usize,
    pub missing_required_parent: Vec<String>,
    pub deprecated_roles: Vec<String>,
    pub status_widgets: usize,
    pub status_widgets_unnamed: usize,
    // ----- 레이블 -----
    pub form_controls: usize,
    /// title 속성만으로 이름이 붙은 입력 요소
    pub title_only_labels: usize,
    /// label 이 둘 이상 연결된 입력 요소
    pub multiple_labels: usize,
    pub labelled_widgets: usize,
    /// aria-label 이 화면에 보이는 텍스트를 포함하지 않는 링크·버튼
    pub label_name_mismatch: Vec<String>,
    // ----- 랜드마크 -----
    /// 다른 랜드마크 안에 든 main/banner/contentinfo
    pub nested_top_landmarks: Vec<String>,
    pub banners: usize,
    pub contentinfos: usize,
    /// 랜드마크 밖에 있는 본문 텍스트 글자 수
    pub text_outside_landmarks: usize,
    pub has_landmarks: bool,
    // ----- 기타 접근성 -----
    pub titled_iframes: usize,
    pub duplicate_iframe_titles: Vec<String>,
    pub html_lang: Option<String>,
    pub xml_lang: Option<String>,
    pub server_side_image_maps: usize,
    pub definition_lists: usize,
    pub definition_list_violations: usize,
    pub data_tables: usize,
    pub data_tables_without_caption: usize,
    pub inline_spacing_important: usize,
    pub paste_blocked: usize,
    pub links_with_text: usize,
    /// 같은 텍스트인데 목적지가 다른 링크 텍스트
    pub same_text_different_href: Vec<String>,
    // ----- SEO·보안 -----
    pub anchors: usize,
    /// href 없거나 javascript: 인 링크 (검색엔진이 따라가지 못함)
    pub uncrawlable_links: usize,
    pub third_party_resources: usize,
    /// integrity(SRI) 없는 외부 호스트 스크립트·스타일시트
    pub third_party_without_sri: Vec<String>,
    pub forms: usize,
    /// https 페이지에서 http 로 보내는 폼
    pub insecure_forms: Vec<String>,
    pub blank_links: usize,
    pub blank_without_noopener: usize,
    pub plugins: usize,
    pub data_nosnippet: usize,
    // ----- AI 에이전트 -----
    /// `<link rel="ard">` 대상
    pub ard_link: Option<String>,
    /// `<link rel="ai-catalog">` 대상 (ARD 이전 관계)
    pub ai_catalog_link: Option<String>,
    /// WebMCP 선언형 주석(toolname) 이 있는 폼 수
    pub webmcp_forms: usize,
    pub webmcp_forms_described: usize,
    pub all_forms: usize,
    /// 인라인 스크립트가 navigator.modelContext (WebMCP 명령형 API) 를 쓰는지
    pub webmcp_script: bool,
}

fn in_landmark(el: &ElementRef) -> bool {
    el.ancestors().filter_map(ElementRef::wrap).any(|a| {
        LANDMARK_TAGS.contains(&a.value().name())
            || first_role(&a).is_some_and(|r| LANDMARK_ROLES.contains(&r.as_str()))
            || (a.value().name() == "section"
                && (attr(&a, "aria-label").is_some() || attr(&a, "aria-labelledby").is_some()))
    })
}

fn is_top_level_header_footer(el: &ElementRef) -> bool {
    !el.ancestors()
        .filter_map(ElementRef::wrap)
        .any(|a| SECTIONING.contains(&a.value().name()))
}

pub fn extract(doc: &Html, html: &str, page_url: &Url) -> RuleFacts {
    let mut f = RuleFacts {
        has_html5_doctype: html
            .trim_start_matches('\u{feff}')
            .trim_start()
            .get(..15)
            .is_some_and(|h| h.eq_ignore_ascii_case("<!doctype html>")),
        parse_errors: doc.errors.len(),
        parse_error_samples: doc.errors.iter().take(10).map(|e| e.to_string()).collect(),
        ..Default::default()
    };

    // ARIA 값·이름
    let value_rules: HashMap<&str, &[&str]> = ARIA_VALUE_RULES.iter().copied().collect();
    for el in doc.select(&ALL) {
        for (name, value) in el.value().attrs() {
            let Some(allowed) = value_rules.get(name) else {
                continue;
            };
            f.aria_value_attrs += 1;
            let v = value.trim().to_ascii_lowercase();
            let ok = if allowed.is_empty() {
                v.parse::<u32>().is_ok()
            } else {
                allowed.contains(&v.as_str())
            };
            if !ok {
                push_unique(
                    &mut f.aria_invalid_values,
                    &format!("{name}=\"{}\"", value.trim()),
                );
            }
        }
    }
    for el in doc.select(&LABELLED) {
        f.labelled_elements += 1;
        let role = first_role(&el);
        let tag = el.value().name();
        let prohibited = match role.as_deref() {
            Some(r) => NAME_PROHIBITED_ROLES.contains(&r),
            None => matches!(
                tag,
                "div" | "span" | "p" | "b" | "i" | "em" | "strong" | "code"
            ),
        };
        if prohibited {
            push_unique(
                &mut f.aria_name_prohibited,
                &role.unwrap_or_else(|| tag.to_string()),
            );
        }
    }
    for el in doc.select(&ROLE) {
        let Some(role) = first_role(&el) else {
            continue;
        };
        if DEPRECATED_ROLES.contains(&role.as_str()) {
            push_unique(&mut f.deprecated_roles, &role);
        }
        if let Some((_, children)) = REQUIRED_CHILDREN.iter().find(|(r, _)| *r == role) {
            f.containers += 1;
            let has = el.descendants().filter_map(ElementRef::wrap).any(|d| {
                first_role(&d).is_some_and(|r| children.contains(&r.as_str()))
                    || native_role_match(&d, children)
            });
            if !has && el.value().attr("aria-owns").is_none() {
                push_unique(&mut f.missing_required_children, &role);
            }
        }
        if let Some((_, parents)) = REQUIRED_PARENT.iter().find(|(r, _)| *r == role) {
            f.child_roles += 1;
            let has = el.ancestors().filter_map(ElementRef::wrap).any(|a| {
                first_role(&a).is_some_and(|r| parents.contains(&r.as_str()))
                    || native_role_match(&a, parents)
            });
            if !has {
                push_unique(&mut f.missing_required_parent, &role);
            }
        }
    }
    for w in doc.select(&STATUS_WIDGET) {
        f.status_widgets += 1;
        let labelled_by_label = w.value().attr("id").is_some_and(|id| {
            doc.select(&LABEL_FOR)
                .any(|l| l.value().attr("for") == Some(id))
        });
        if attr(&w, "aria-label").is_none()
            && attr(&w, "aria-labelledby").is_none()
            && attr(&w, "title").is_none()
            && !labelled_by_label
        {
            f.status_widgets_unnamed += 1;
        }
    }

    // 레이블
    let mut for_count: HashMap<&str, usize> = HashMap::new();
    for l in doc.select(&LABEL_FOR) {
        *for_count
            .entry(l.value().attr("for").unwrap_or_default())
            .or_default() += 1;
    }
    for c in doc.select(&CONTROLS) {
        let ty = c
            .value()
            .attr("type")
            .unwrap_or("text")
            .to_ascii_lowercase();
        if matches!(
            ty.as_str(),
            "hidden" | "submit" | "button" | "reset" | "image"
        ) {
            continue;
        }
        f.form_controls += 1;
        let id = c.value().attr("id").unwrap_or_default();
        let by_for = for_count.get(id).copied().unwrap_or_default();
        let wrapped = c
            .ancestors()
            .filter_map(ElementRef::wrap)
            .any(|a| a.value().name() == "label");
        let aria = attr(&c, "aria-label").is_some() || attr(&c, "aria-labelledby").is_some();
        if by_for == 0 && !wrapped && !aria && attr(&c, "title").is_some() {
            f.title_only_labels += 1;
        }
        if by_for + usize::from(wrapped) > 1 {
            f.multiple_labels += 1;
        }
    }
    for el in doc.select(&NAMED_BY_LABEL) {
        let visible = visible_text(&el);
        let key = alnum(&visible);
        // 기호·아이콘 글자("×", "i")는 비교하지 않는다
        if key.chars().count() < 2 {
            continue;
        }
        f.labelled_widgets += 1;
        let label = attr(&el, "aria-label").unwrap_or_default();
        if !alnum(label).contains(&key) {
            push_unique(&mut f.label_name_mismatch, &format!("{visible} → {label}"));
        }
    }

    // 랜드마크
    for el in doc.select(&HEADER_FOOTER_MAIN) {
        let tag = el.value().name();
        let role = first_role(&el);
        let kind = match (tag, role.as_deref()) {
            (_, Some("banner")) => "banner",
            (_, Some("contentinfo")) => "contentinfo",
            (_, Some("main")) | ("main", _) => "main",
            ("header", None) if is_top_level_header_footer(&el) => "banner",
            ("footer", None) if is_top_level_header_footer(&el) => "contentinfo",
            _ => continue,
        };
        f.has_landmarks = true;
        match kind {
            "banner" => f.banners += 1,
            "contentinfo" => f.contentinfos += 1,
            _ => {}
        }
        if in_landmark(&el) {
            push_unique(&mut f.nested_top_landmarks, kind);
        }
    }
    if let Some(body) = doc.select(&BODY).next() {
        for node in body.descendants() {
            let Node::Text(t) = node.value() else {
                continue;
            };
            // <template> 내용은 문서 조각(fragment) 아래에 있어 부모가 요소가 아니다
            let Some(parent) = node.parent().and_then(ElementRef::wrap) else {
                continue;
            };
            if matches!(
                parent.value().name(),
                "script" | "style" | "noscript" | "template"
            ) {
                continue;
            }
            let len = t.trim().chars().count();
            if len > 0 && !LANDMARK_TAGS.contains(&parent.value().name()) && !in_landmark(&parent) {
                f.text_outside_landmarks += len;
            }
        }
    }

    // iframe 제목, lang, 이미지맵, 정의 목록, 표 제목
    let titles: Vec<String> = doc
        .select(&IFRAME)
        .filter_map(|i| attr(&i, "title").map(str::to_lowercase))
        .collect();
    f.titled_iframes = titles.len();
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for t in &titles {
        let n = seen.entry(t).or_default();
        *n += 1;
        if *n == 2 {
            push_unique(&mut f.duplicate_iframe_titles, t);
        }
    }
    if let Some(h) = doc.select(&HTML_EL).next() {
        f.html_lang = attr(&h, "lang").map(str::to_string);
        f.xml_lang = attr(&h, "xml:lang").map(str::to_string);
    }
    f.server_side_image_maps = doc.select(&ISMAP).count();
    for dl in doc.select(&DL) {
        f.definition_lists += 1;
        f.definition_list_violations += dl
            .children()
            .filter_map(ElementRef::wrap)
            .filter(|c| {
                !matches!(
                    c.value().name(),
                    "dt" | "dd" | "div" | "script" | "template"
                )
            })
            .count();
    }
    for item in doc.select(&DT_DD) {
        let in_dl = item
            .ancestors()
            .filter_map(ElementRef::wrap)
            .any(|a| a.value().name() == "dl");
        if !in_dl {
            f.definition_list_violations += 1;
        }
    }
    for t in doc.select(&DATA_TABLE) {
        if t.select(&TABLE_TH).next().is_none()
            || first_role(&t).is_some_and(|r| r == "presentation" || r == "none")
        {
            continue;
        }
        f.data_tables += 1;
        let captioned = t.select(&CAPTION).any(|c| !text(&c).is_empty())
            || attr(&t, "aria-label").is_some()
            || attr(&t, "aria-labelledby").is_some();
        if !captioned {
            f.data_tables_without_caption += 1;
        }
    }
    f.inline_spacing_important = doc
        .select(&STYLE_ATTR)
        .filter(|e| INLINE_SPACING.is_match(e.value().attr("style").unwrap_or_default()))
        .count();
    f.paste_blocked = doc
        .select(&PASTE)
        .filter(|e| PASTE_BLOCK.is_match(e.value().attr("onpaste").unwrap_or_default()))
        .count();

    // 링크: 같은 텍스트·다른 목적지, 크롤링 불가, 새 창 noopener
    let mut by_text: HashMap<String, Vec<String>> = HashMap::new();
    for a in doc.select(&ANCHOR) {
        f.anchors += 1;
        let href = a.value().attr("href").map(str::trim);
        let crawlable = href
            .is_some_and(|h| !h.is_empty() && !h.to_ascii_lowercase().starts_with("javascript:"));
        if !crawlable {
            // 이름만 있는 앵커(<a name>/<a id>)는 링크가 아니다
            if a.value().attr("onclick").is_some() || href.is_some() {
                f.uncrawlable_links += 1;
            }
            continue;
        }
        let t = text(&a).to_lowercase();
        let Ok(dest) = page_url.join(href.unwrap_or_default()) else {
            continue;
        };
        if t.is_empty() || dest.scheme() == "mailto" || dest.scheme() == "tel" {
            continue;
        }
        let mut dest = dest;
        dest.set_fragment(None);
        let list = by_text.entry(t).or_default();
        if !list.contains(&dest.to_string()) {
            list.push(dest.to_string());
        }
    }
    f.links_with_text = by_text.len();
    let mut dup: Vec<&String> = by_text
        .iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|(k, _)| k)
        .collect();
    dup.sort();
    for t in dup {
        push_unique(&mut f.same_text_different_href, t);
    }
    for a in doc.select(&BLANK) {
        f.blank_links += 1;
        let rel = a
            .value()
            .attr("rel")
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !rel
            .split_whitespace()
            .any(|r| r == "noopener" || r == "noreferrer")
        {
            f.blank_without_noopener += 1;
        }
    }

    // 외부 리소스 SRI, 폼, 플러그인, data-nosnippet
    for el in doc.select(&EXTERNAL_RESOURCE) {
        let raw = el
            .value()
            .attr("src")
            .or_else(|| el.value().attr("href"))
            .unwrap_or_default();
        let Ok(u) = page_url.join(raw.trim()) else {
            continue;
        };
        if u.host_str() == page_url.host_str() || !matches!(u.scheme(), "http" | "https") {
            continue;
        }
        f.third_party_resources += 1;
        if attr(&el, "integrity").is_none() {
            push_unique(
                &mut f.third_party_without_sri,
                u.host_str().unwrap_or_default(),
            );
        }
    }
    for form in doc.select(&FORM) {
        f.forms += 1;
        if page_url.scheme() == "https"
            && page_url
                .join(form.value().attr("action").unwrap_or_default().trim())
                .is_ok_and(|u| u.scheme() == "http")
        {
            push_unique(
                &mut f.insecure_forms,
                form.value().attr("action").unwrap_or_default().trim(),
            );
        }
    }
    f.plugins = doc
        .select(&PLUGIN)
        .filter(|p| {
            p.value().name() == "applet"
                || ["type", "data", "src", "classid"]
                    .iter()
                    .any(|a| PLUGIN_TYPE.is_match(p.value().attr(a).unwrap_or_default()))
        })
        .count();
    f.data_nosnippet = doc.select(&NOSNIPPET_ATTR).count();

    // AI 에이전트: ARD 링크, WebMCP
    let href = |s: &Selector| {
        doc.select(s)
            .next()
            .and_then(|l| attr(&l, "href").and_then(|h| page_url.join(h).ok()))
            .map(|u| u.to_string())
    };
    f.ard_link = href(&LINK_ARD);
    f.ai_catalog_link = href(&LINK_AI_CATALOG);
    for form in doc.select(&ALL_FORMS) {
        f.all_forms += 1;
        if attr(&form, "toolname").is_some() {
            f.webmcp_forms += 1;
            if attr(&form, "tooldescription").is_some() {
                f.webmcp_forms_described += 1;
            }
        }
    }
    f.webmcp_script = doc
        .select(&INLINE_SCRIPT)
        .any(|s| s.text().any(|t| t.contains("modelContext")));
    f
}

/// 네이티브 요소가 암묵적으로 가진 역할과 맞는지 (ul/ol→list, li→listitem, table→table, tr→row 등).
fn native_role_match(el: &ElementRef, roles: &[&str]) -> bool {
    let implicit = match el.value().name() {
        "ul" | "ol" | "menu" => "list",
        "li" => "listitem",
        "table" => "table",
        "tr" => "row",
        "tbody" | "thead" | "tfoot" => "rowgroup",
        "td" => "cell",
        "th" => "columnheader",
        "option" => "option",
        "select" => "listbox",
        "article" => "article",
        "input"
            if el
                .value()
                .attr("type")
                .is_some_and(|t| t.eq_ignore_ascii_case("radio")) =>
        {
            "radio"
        }
        _ => return false,
    };
    roles.contains(&implicit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(html: &str) -> RuleFacts {
        extract(
            &Html::parse_document(html),
            html,
            &Url::parse("https://a.com/").unwrap(),
        )
    }

    #[test]
    fn markup_and_aria_rules() {
        let f = run(r#"<!DOCTYPE html><html lang="ko" xml:lang="en"><body>
<p><div>잘못된 중첩</div></p>
<div aria-hidden="yes"></div><button aria-expanded="maybe">x</button><h2 aria-level="2">ok</h2><div role="heading" aria-level="two">x</div>
<div aria-label="장식"></div><span role="presentation" aria-label="x"></span><nav aria-label="주"></nav>
<div role="list"><span>항목</span></div><ul role="list"><li>ok</li></ul>
<div role="tablist"><div role="tab">a</div></div><div role="tab">고아</div><div role="option">o</div>
<table role="grid"><tr><td role="gridcell">c</td></tr></table>
<div role="directory"></div>
<div role=" "></div><header role="banner">명시 배너</header>
<section aria-label="소식"><p>이름 있는 구역 안</p></section>
<label for="pg">진행률</label><progress id="pg" value="1" max="2"></progress>
<div role="tablist"><table><tr><td>c</td><th>h</th></tr></table><select><option>o</option></select><article>a</article><button role="tab">t</button></div>
<div role="radiogroup" aria-label="색"><input type="radio" name="c"></div>
<progress value="3" max="10"></progress><meter aria-label="용량" value="1"></meter><div role="tooltip" id="tt"></div>
</body></html>"#);
        assert!(f.has_html5_doctype);
        assert!(f.parse_errors > 0 && !f.parse_error_samples.is_empty());
        assert_eq!(
            f.aria_invalid_values,
            [
                "aria-hidden=\"yes\"",
                "aria-expanded=\"maybe\"",
                "aria-level=\"two\""
            ]
        );
        assert_eq!(f.aria_name_prohibited, ["div", "presentation"]);
        assert_eq!(f.missing_required_children, ["list"]);
        assert_eq!(f.missing_required_parent, ["tab", "option"]);
        assert_eq!(f.deprecated_roles, ["directory"]);
        assert_eq!((f.status_widgets, f.status_widgets_unnamed), (4, 2));
        assert_eq!(
            (f.html_lang.as_deref(), f.xml_lang.as_deref()),
            (Some("ko"), Some("en"))
        );
    }

    #[test]
    fn label_landmark_table_link_rules() {
        let f = run(r##"<html><body>
<header>사이트</header><header>두 번째 배너</header>
<main><header>본문 머리 (banner 아님)</header><footer role="contentinfo">중첩</footer></main>
<footer>바닥</footer>
<p>랜드마크 밖 텍스트</p>
<input id="q" title="검색어"><label for="n">이름</label><label for="n">성명</label><input id="n">
<label>감싼 <input></label><input aria-label="ok">
<a href="/a" aria-label="자세히 보기">구매</a><button aria-label="장바구니에 담기">담기</button>
<a href="/m" aria-label="See all HTML references">See all…</a><button aria-label="닫기">×</button>
<a href="/h" aria-label="메뉴"><span aria-hidden="true">아이콘</span><span hidden>숨김</span>메뉴<noscript>&lt;img&gt;</noscript></a>
<iframe title="지도"></iframe><iframe title="지도"></iframe>
<img src="m.png" ismap>
<dl><dt>용어</dt><dd>설명</dd><p>틀림</p></dl><dd>밖</dd>
<table><tr><th>이름</th></tr><tr><td>a</td></tr></table>
<table><caption>표 제목</caption><tr><th>값</th></tr></table><table><tr><td>배치</td></tr></table>
<p style="letter-spacing: 0 !important">x</p>
<input type="password" onpaste="return false"><textarea onpaste="log()"></textarea>
<a href="/x">더보기</a><a href="/y">더보기</a><a href="/z#top">소개</a><a href="/z">소개</a>
<a href="javascript:void(0)">js</a><a onclick="go()">클릭</a><a name="anchor"></a>
<a href="mailto:a@b.c">메일</a><a href="http://[">깨짐</a>
<a href="https://x.com" target="_blank">x</a><a href="https://y.com" target="_blank" rel="noopener">y</a>
<script src="https://cdn.example/lib.js"></script><script src="https://cdn2.example/lib.js" integrity="sha384-x"></script>
<script src="/local.js"></script><link rel="stylesheet" href="https://fonts.example/a.css"><script src="data:text/javascript,1"></script>
<script src="http://["></script>
<form action="http://a.com/login"></form><form action="/ok"></form>
<object data="movie.swf"></object><embed src="video.mp4"><applet></applet>
<div data-nosnippet>비공개</div>
</body></html>"##);
        assert!(!f.has_html5_doctype);
        assert_eq!(f.banners, 2);
        assert_eq!(f.contentinfos, 2);
        assert_eq!(f.nested_top_landmarks, ["contentinfo"]);
        assert!(f.text_outside_landmarks > 0 && f.has_landmarks);
        assert_eq!(f.title_only_labels, 1);
        assert_eq!(f.multiple_labels, 1);
        assert_eq!(f.labelled_widgets, 4);
        assert_eq!(f.label_name_mismatch, ["구매 → 자세히 보기"]);
        assert_eq!(f.duplicate_iframe_titles, ["지도"]);
        assert_eq!(f.server_side_image_maps, 1);
        assert_eq!((f.definition_lists, f.definition_list_violations), (1, 2));
        assert_eq!((f.data_tables, f.data_tables_without_caption), (2, 1));
        assert_eq!(f.inline_spacing_important, 1);
        assert_eq!(f.paste_blocked, 1);
        assert_eq!(f.same_text_different_href, ["더보기"]);
        assert_eq!(f.uncrawlable_links, 2);
        assert_eq!((f.blank_links, f.blank_without_noopener), (2, 1));
        assert_eq!(f.third_party_resources, 3);
        assert_eq!(f.third_party_without_sri, ["cdn.example", "fonts.example"]);
        assert_eq!(f.insecure_forms, ["http://a.com/login"]);
        assert_eq!(f.plugins, 2);
        assert_eq!(f.data_nosnippet, 1);
    }

    #[test]
    fn page_without_body_or_landmarks() {
        // template 안 텍스트는 화면에 보이지 않는다 (부모가 문서 조각)
        let f =
            run("<html><body><template>템플릿 텍스트</template><main>본문</main></body></html>");
        assert_eq!(f.text_outside_landmarks, 0);
        let f = run("<html><frameset></frameset></html>");
        assert_eq!(f.text_outside_landmarks, 0);
        assert!(!f.has_landmarks);
    }

    #[test]
    fn agent_signals() {
        let f = run(
            r#"<html><head><link rel="ard" href="/agents/ard.json"><link rel="AI-Catalog" href="https://a.com/c.json">
<script>navigator.modelContext.registerTool({name: "search"})</script></head><body>
<form toolname="search" tooldescription="사이트 검색"><input name="q"></form>
<form toolname="login"></form><form></form></body></html>"#,
        );
        assert_eq!(f.ard_link.as_deref(), Some("https://a.com/agents/ard.json"));
        assert_eq!(f.ai_catalog_link.as_deref(), Some("https://a.com/c.json"));
        assert_eq!(
            (f.all_forms, f.webmcp_forms, f.webmcp_forms_described),
            (3, 2, 1)
        );
        assert!(f.webmcp_script);
        let f = run("<html><body><script>var x = 1;</script></body></html>");
        assert!(f.ard_link.is_none() && !f.webmcp_script);
    }
}
