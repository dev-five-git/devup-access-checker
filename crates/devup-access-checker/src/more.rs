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

/// 인터랙티브 ARIA 역할 (초점·이름이 필요)
const INTERACTIVE_ROLES: &[&str] = &[
    "button",
    "link",
    "checkbox",
    "radio",
    "switch",
    "tab",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "option",
    "slider",
    "spinbutton",
    "textbox",
    "combobox",
    "treeitem",
    "searchbox",
];

/// 역할별 필수 ARIA 속성 (WAI-ARIA 1.2)
const REQUIRED_ARIA: &[(&str, &str)] = &[
    ("checkbox", "aria-checked"),
    ("radio", "aria-checked"),
    ("switch", "aria-checked"),
    ("menuitemcheckbox", "aria-checked"),
    ("menuitemradio", "aria-checked"),
    ("slider", "aria-valuenow"),
    ("scrollbar", "aria-valuenow"),
    ("scrollbar", "aria-controls"),
    ("combobox", "aria-expanded"),
    ("heading", "aria-level"),
];

/// HTML autocomplete 토큰 (자동 완성 세부 토큰 + 주소·연락처 수식어)
const AUTOCOMPLETE_TOKENS: &[&str] = &[
    "on",
    "off",
    "name",
    "honorific-prefix",
    "given-name",
    "additional-name",
    "family-name",
    "honorific-suffix",
    "nickname",
    "email",
    "username",
    "new-password",
    "current-password",
    "one-time-code",
    "organization-title",
    "organization",
    "street-address",
    "address-line1",
    "address-line2",
    "address-line3",
    "address-level4",
    "address-level3",
    "address-level2",
    "address-level1",
    "country",
    "country-name",
    "postal-code",
    "cc-name",
    "cc-given-name",
    "cc-additional-name",
    "cc-family-name",
    "cc-number",
    "cc-exp",
    "cc-exp-month",
    "cc-exp-year",
    "cc-csc",
    "cc-type",
    "transaction-currency",
    "transaction-amount",
    "language",
    "bday",
    "bday-day",
    "bday-month",
    "bday-year",
    "sex",
    "url",
    "photo",
    "tel",
    "tel-country-code",
    "tel-national",
    "tel-area-code",
    "tel-local",
    "tel-local-prefix",
    "tel-local-suffix",
    "tel-extension",
    "impp",
    "webauthn",
    "shipping",
    "billing",
    "home",
    "work",
    "mobile",
    "fax",
    "pager",
];

static NATIVE_FOCUSABLE: LazyLock<Selector> = LazyLock::new(|| {
    sel("a[href], button, input, select, textarea, summary, iframe, [contenteditable]")
});
static ONCLICK: LazyLock<Selector> = LazyLock::new(|| {
    sel(
        "div[onclick], span[onclick], li[onclick], img[onclick], p[onclick], td[onclick], tr[onclick]",
    )
});
static DIALOG: LazyLock<Selector> =
    LazyLock::new(|| sel("dialog, [role=dialog], [role=alertdialog]"));
static FIELDSET: LazyLock<Selector> = LazyLock::new(|| sel("fieldset"));
static RADIO: LazyLock<Selector> = LazyLock::new(|| sel("input[type=radio][name]"));
static AUTOCOMPLETE: LazyLock<Selector> =
    LazyLock::new(|| sel("input[autocomplete], select[autocomplete], textarea[autocomplete]"));
static TH: LazyLock<Selector> = LazyLock::new(|| sel("th"));
static SCOPE: LazyLock<Selector> = LazyLock::new(|| sel("[scope]"));
static MAIN_LANDMARK: LazyLock<Selector> = LazyLock::new(|| sel("main, [role=main]"));
static NAV_LANDMARK: LazyLock<Selector> = LazyLock::new(|| sel("nav, [role=navigation]"));
static ASIDE_LANDMARK: LazyLock<Selector> = LazyLock::new(|| sel("aside, [role=complementary]"));
static IMG_ALT_TEXT: LazyLock<Selector> = LazyLock::new(|| sel("img[alt]"));
static SUMMARY: LazyLock<Selector> = LazyLock::new(|| sel("summary"));
static REDUNDANT_ALT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:image|picture|photo|graphic)\s+of\b|^(?:이미지|사진|그림)\s*[:：]|(?:이미지|사진|그림)$|\b(?:image|picture|photo)$").unwrap()
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
    /// 인터랙티브 role 인데 초점을 받을 수 없는 요소 (tabindex 없음)
    pub custom_controls: usize,
    pub custom_controls_unfocusable: usize,
    /// 인터랙티브 role 인데 이름이 없는 요소
    pub custom_controls_unnamed: usize,
    /// role·tabindex 없이 onclick 이 걸린 비인터랙티브 요소
    pub click_without_keyboard: usize,
    /// 역할에 필요한 ARIA 속성 누락 (예: role=checkbox 에 aria-checked 없음)
    pub required_aria_missing: Vec<String>,
    pub dialogs: usize,
    pub dialogs_unnamed: usize,
    pub fieldsets: usize,
    pub fieldsets_without_legend: usize,
    pub radio_groups: usize,
    /// fieldset·radiogroup 으로 묶이지 않은 라디오 그룹 이름
    pub radio_groups_ungrouped: Vec<String>,
    pub autocomplete_fields: usize,
    pub autocomplete_invalid: Vec<String>,
    pub table_headers: usize,
    pub empty_table_headers: usize,
    pub scope_attrs: usize,
    pub scope_invalid: usize,
    pub main_landmarks: usize,
    /// 이름 없이 여러 번 나오는 nav·aside 랜드마크 종류
    pub unnamed_repeated_landmarks: Vec<String>,
    pub alt_texts: usize,
    /// "이미지", "image of" 처럼 중복 표현이 든 alt
    pub redundant_alts: Vec<String>,
    pub summaries: usize,
    pub empty_summaries: usize,
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

/// 텍스트·aria-label·aria-labelledby·title 중 하나라도 있으면 이름이 있는 것으로 본다.
fn has_name(el: &ElementRef) -> bool {
    !el.text().collect::<String>().trim().is_empty()
        || attr(el, "aria-label").is_some()
        || attr(el, "aria-labelledby").is_some()
        || attr(el, "title").is_some()
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

    // 커스텀 컨트롤 (div role=button 등)
    for el in doc.select(&ROLE) {
        let role = el
            .value()
            .attr("role")
            .and_then(|r| r.split_whitespace().next())
            .unwrap_or_default()
            .to_ascii_lowercase();
        for (r, required) in REQUIRED_ARIA {
            if role == *r && el.value().attr(required).is_none() {
                push_unique(&mut f.required_aria_missing, &format!("{r}:{required}"));
            }
        }
        if !INTERACTIVE_ROLES.contains(&role.as_str()) {
            continue;
        }
        f.custom_controls += 1;
        let native = NATIVE_FOCUSABLE.matches(&el);
        if !native && el.value().attr("tabindex").is_none() {
            f.custom_controls_unfocusable += 1;
        }
        if !has_name(&el) {
            f.custom_controls_unnamed += 1;
        }
    }
    f.click_without_keyboard = doc
        .select(&ONCLICK)
        .filter(|e| e.value().attr("role").is_none() && e.value().attr("tabindex").is_none())
        .count();

    for d in doc.select(&DIALOG) {
        f.dialogs += 1;
        if attr(&d, "aria-label").is_none()
            && attr(&d, "aria-labelledby").is_none()
            && attr(&d, "title").is_none()
        {
            f.dialogs_unnamed += 1;
        }
    }

    // 폼 그룹
    for fs in doc.select(&FIELDSET) {
        f.fieldsets += 1;
        let first = fs.children().filter_map(ElementRef::wrap).next();
        if !first.is_some_and(|c| {
            c.value().name() == "legend" && !c.text().collect::<String>().trim().is_empty()
        }) {
            f.fieldsets_without_legend += 1;
        }
    }
    let mut groups: Vec<(String, usize, bool)> = Vec::new();
    for r in doc.select(&RADIO) {
        let name = r.value().attr("name").unwrap_or_default().to_string();
        let grouped = r.ancestors().filter_map(ElementRef::wrap).any(|a| {
            a.value().name() == "fieldset"
                || a.value()
                    .attr("role")
                    .is_some_and(|x| x == "radiogroup" || x == "group")
        });
        match groups.iter_mut().find(|(n, _, _)| *n == name) {
            Some(g) => {
                g.1 += 1;
                g.2 &= grouped;
            }
            None => groups.push((name, 1, grouped)),
        }
    }
    for (name, count, grouped) in groups {
        if count >= 2 {
            f.radio_groups += 1;
            if !grouped {
                push_unique(&mut f.radio_groups_ungrouped, &name);
            }
        }
    }
    for el in doc.select(&AUTOCOMPLETE) {
        f.autocomplete_fields += 1;
        let v = el
            .value()
            .attr("autocomplete")
            .unwrap_or_default()
            .to_ascii_lowercase();
        let valid = !v.trim().is_empty()
            && v.split_whitespace()
                .all(|t| AUTOCOMPLETE_TOKENS.contains(&t) || t.starts_with("section-"));
        if !valid {
            push_unique(&mut f.autocomplete_invalid, v.trim());
        }
    }

    // 표
    for th in doc.select(&TH) {
        f.table_headers += 1;
        if !has_name(&th) {
            f.empty_table_headers += 1;
        }
    }
    for el in doc.select(&SCOPE) {
        f.scope_attrs += 1;
        let v = el
            .value()
            .attr("scope")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if el.value().name() != "th"
            || !matches!(v.as_str(), "row" | "col" | "rowgroup" | "colgroup")
        {
            f.scope_invalid += 1;
        }
    }

    // 랜드마크
    f.main_landmarks = doc.select(&MAIN_LANDMARK).count();
    for (kind, selector) in [("nav", &NAV_LANDMARK), ("aside", &ASIDE_LANDMARK)] {
        let items: Vec<ElementRef> = doc.select(selector).collect();
        if items.len() > 1
            && items
                .iter()
                .any(|e| attr(e, "aria-label").is_none() && attr(e, "aria-labelledby").is_none())
        {
            f.unnamed_repeated_landmarks.push(kind.to_string());
        }
    }

    // 대체 텍스트·요약
    for img in doc.select(&IMG_ALT_TEXT) {
        let alt = img.value().attr("alt").unwrap_or_default().trim();
        if alt.is_empty() {
            continue;
        }
        f.alt_texts += 1;
        if REDUNDANT_ALT.is_match(alt) {
            push_unique(&mut f.redundant_alts, alt);
        }
    }
    for s in doc.select(&SUMMARY) {
        f.summaries += 1;
        if !has_name(&s) {
            f.empty_summaries += 1;
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

    #[test]
    fn keyboard_form_table_landmark_signals() {
        let f = run(r##"<html><body>
<div role="button">열기</div><div role="button" tabindex="0" aria-label="닫기"></div><span role="link"></span>
<button role="switch" aria-checked="false">알림</button><div role="checkbox" tabindex="0">동의</div>
<div role="heading">제목</div><div role="slider" tabindex="0" aria-valuenow="3" aria-label="볼륨"></div>
<div onclick="go()">클릭</div><span onclick="x()" role="button" tabindex="0">ok</span>
<dialog open>내용</dialog><div role="dialog" aria-labelledby="t"><h2 id="t">t</h2></div>
<fieldset><legend>배송</legend><input type="radio" name="ship"><input type="radio" name="ship"></fieldset>
<fieldset><input type="radio" name="pay"><input type="radio" name="pay"></fieldset>
<input type="radio" name="size"><input type="radio" name="size"><input type="radio" name="solo">
<div role="radiogroup" aria-label="색"><input type="radio" name="color"><input type="radio" name="color"></div>
<input autocomplete="email"><input autocomplete="shipping street-address"><input autocomplete="nope">
<input autocomplete="section-a tel"><input autocomplete="">
<table><tr><th>이름</th><th></th><td scope="row">x</td><th scope="col">값</th><th scope="column">y</th></tr></table>
<main></main><div role="main"></div>
<nav aria-label="주"></nav><nav></nav><aside aria-label="a"></aside><aside aria-label="b"></aside>
<img src="a.png" alt="회사 로고 이미지"><img src="b.png" alt="Image of a cat"><img src="c.png" alt="사진: 사옥">
<img src="d.png" alt="강아지"><img src="e.png" alt="">
<details><summary></summary>x</details><details><summary>더보기</summary>y</details>
</body></html>"##);
        assert_eq!(f.custom_controls, 7);
        assert_eq!(
            f.custom_controls_unfocusable, 2,
            "div role=button, span role=link"
        );
        assert_eq!(f.custom_controls_unnamed, 1, "span role=link");
        assert_eq!(f.click_without_keyboard, 1);
        assert_eq!(
            f.required_aria_missing,
            ["checkbox:aria-checked", "heading:aria-level"]
        );
        assert_eq!((f.dialogs, f.dialogs_unnamed), (2, 1));
        assert_eq!((f.fieldsets, f.fieldsets_without_legend), (2, 1));
        assert_eq!(f.radio_groups, 4);
        assert_eq!(f.radio_groups_ungrouped, ["size"]);
        assert_eq!(f.autocomplete_fields, 5);
        assert_eq!(f.autocomplete_invalid, ["nope", ""]);
        assert_eq!((f.table_headers, f.empty_table_headers), (4, 1));
        assert_eq!((f.scope_attrs, f.scope_invalid), (3, 2));
        assert_eq!(f.main_landmarks, 2);
        assert_eq!(f.unnamed_repeated_landmarks, ["nav"]);
        assert_eq!(f.alt_texts, 4);
        assert_eq!(
            f.redundant_alts,
            ["회사 로고 이미지", "Image of a cat", "사진: 사옥"]
        );
        assert_eq!((f.summaries, f.empty_summaries), (2, 1));
    }
}
