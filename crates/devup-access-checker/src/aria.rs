//! ARIA in HTML·IBM Equal Access·HTML_CodeSniffer·WAVE·W3C i18n checker·EN 301 549 와 대조해 추가한 정적 신호.
//! 대조 결과는 `docs/coverage.md` 에 있다. 네트워크 접근 없음.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use url::Url;

static ALL: LazyLock<Selector> = LazyLock::new(|| Selector::parse("*").expect("valid selector"));
static BLINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)text-decoration(?:-line)?\s*:[^;}]*\bblink\b").unwrap());
static DIRECTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bdirection\s*:\s*(?:rtl|ltr)|unicode-bidi\s*:").unwrap());

/// 역할을 지정할 수 없는 요소 (ARIA in HTML "No role")
const NO_ROLE: &[&str] = &[
    "base", "col", "colgroup", "head", "link", "map", "meta", "noscript", "param", "script",
    "slot", "source", "style", "template", "title", "track", "dd", "label", "legend", "picture",
    "datalist",
];
/// 자기 역할만 허용하는 요소
const OWN_ROLE: &[(&str, &str)] = &[
    ("main", "main"),
    ("meter", "meter"),
    ("progress", "progressbar"),
    ("optgroup", "group"),
    ("option", "option"),
    ("textarea", "textbox"),
    ("body", "generic"),
];
/// aria-* 를 쓸 수 없는 요소
const NO_ARIA: &[&str] = &[
    "base", "col", "colgroup", "head", "html", "link", "map", "meta", "noscript", "param",
    "script", "slot", "source", "style", "template", "title", "track", "datalist",
];
/// aria-hidden 만 쓸 수 있는 요소
const ONLY_HIDDEN: &[&str] = &["br", "wbr", "picture"];

/// 역할에 묶인(전역이 아닌) ARIA 속성 → 허용 역할 (WAI-ARIA 1.2)
const ROLE_ATTRS: &[(&str, &[&str])] = &[
    (
        "aria-checked",
        &[
            "checkbox",
            "menuitemcheckbox",
            "menuitemradio",
            "option",
            "radio",
            "switch",
            "treeitem",
        ],
    ),
    ("aria-pressed", &["button"]),
    (
        "aria-selected",
        &[
            "gridcell",
            "option",
            "row",
            "tab",
            "columnheader",
            "rowheader",
            "treeitem",
        ],
    ),
    (
        "aria-expanded",
        &[
            "application",
            "button",
            "checkbox",
            "combobox",
            "gridcell",
            "link",
            "listbox",
            "menuitem",
            "row",
            "rowheader",
            "tab",
            "treeitem",
            "columnheader",
            "menuitemcheckbox",
            "menuitemradio",
            "switch",
        ],
    ),
    (
        "aria-valuenow",
        &[
            "meter",
            "progressbar",
            "scrollbar",
            "separator",
            "slider",
            "spinbutton",
        ],
    ),
    (
        "aria-valuemin",
        &[
            "meter",
            "progressbar",
            "scrollbar",
            "separator",
            "slider",
            "spinbutton",
        ],
    ),
    (
        "aria-valuemax",
        &[
            "meter",
            "progressbar",
            "scrollbar",
            "separator",
            "slider",
            "spinbutton",
        ],
    ),
    (
        "aria-valuetext",
        &[
            "meter",
            "progressbar",
            "scrollbar",
            "separator",
            "slider",
            "spinbutton",
        ],
    ),
    (
        "aria-level",
        &["heading", "listitem", "row", "treeitem", "comment"],
    ),
    (
        "aria-multiselectable",
        &["grid", "listbox", "tablist", "tree", "treegrid"],
    ),
    (
        "aria-required",
        &[
            "checkbox",
            "combobox",
            "gridcell",
            "listbox",
            "radiogroup",
            "spinbutton",
            "textbox",
            "searchbox",
            "tree",
            "columnheader",
            "rowheader",
            "treegrid",
        ],
    ),
    ("aria-autocomplete", &["combobox", "searchbox", "textbox"]),
    ("aria-multiline", &["textbox", "searchbox"]),
    ("aria-placeholder", &["textbox", "searchbox"]),
    ("aria-modal", &["dialog", "alertdialog"]),
    ("aria-sort", &["columnheader", "rowheader"]),
    ("aria-colcount", &["table", "grid", "treegrid"]),
    ("aria-rowcount", &["table", "grid", "treegrid"]),
];

/// 자식이 표현용으로만 취급되는 역할
const PRESENTATIONAL_CHILDREN: &[&str] = &[
    "button",
    "checkbox",
    "img",
    "meter",
    "menuitemcheckbox",
    "menuitemradio",
    "option",
    "progressbar",
    "radio",
    "scrollbar",
    "separator",
    "slider",
    "switch",
    "tab",
];
/// 자식이 표현용인 역할 안에 있으면 의미를 잃는 요소
const SEMANTIC_CHILDREN: &[&str] = &[
    "h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol", "table", "nav", "main",
];
const LANDMARKS: &[&str] = &[
    "navigation",
    "complementary",
    "region",
    "form",
    "search",
    "banner",
    "contentinfo",
    "main",
];
const RTL_LANGS: &[&str] = &[
    "ar", "he", "iw", "fa", "ur", "yi", "ps", "sd", "ug", "dv", "ckb", "syr",
];
const LABELABLE: &[&str] = &[
    "input", "select", "textarea", "button", "meter", "output", "progress",
];
const DOC_EXTS: &[&str] = &[
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "hwp", "hwpx", "odt", "rtf",
];
const STATEMENT_WORDS: &[&str] = &[
    "accessibility",
    "barrierefreiheit",
    "accessibilité",
    "accesibilidad",
    "accessibilità",
    "toegankelijkheid",
    "웹 접근성",
    "웹접근성",
    "접근성 정책",
    "접근성 안내",
    "접근성 성명",
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AriaFacts {
    pub role_redundant: Vec<String>,
    pub role_not_allowed: Vec<String>,
    pub aria_on_prohibited: Vec<String>,
    pub attr_not_for_role: Vec<String>,
    pub attr_conflict: Vec<String>,
    pub attr_redundant: Vec<String>,
    pub deprecated_attrs: Vec<String>,
    pub presentational_children: Vec<String>,
    pub presentation_semantic: Vec<String>,
    pub duplicate_landmark_names: Vec<String>,
    pub unnamed_region_app: usize,
    pub activedescendant_unfocusable: usize,
    pub combobox_invalid: Vec<String>,
    pub tabbable_non_interactive: usize,
    pub iframes: usize,
    pub iframe_tabindex_negative: usize,
    pub labels: usize,
    pub empty_labels: usize,
    pub label_for_not_control: Vec<String>,
    pub placeholder_only: usize,
    pub forms_with_text: usize,
    pub forms_without_submit: usize,
    pub decorative_conflict: usize,
    pub alt_redundant_with_text: usize,
    pub adjacent_duplicate_links: usize,
    pub figures: usize,
    pub figures_unlabeled: usize,
    pub canvases: usize,
    pub canvases_without_fallback: usize,
    pub redundant_title: usize,
    pub broken_fragment_links: Vec<String>,
    /// 끊어진 같은 페이지 링크가 본문 첫 세 링크 안에 있음 (건너뛰기 링크)
    pub broken_skip_link: bool,
    pub mouse_only_handlers: usize,
    pub inline_blink: usize,
    pub media: usize,
    pub media_without_controls: usize,
    pub statement_links: Vec<String>,
    pub document_links: Vec<(String, usize)>,
    pub xml_lang_issues: Vec<String>,
    pub dir_invalid: Vec<String>,
    pub any_dir_rtl: bool,
    pub html_dir_rtl: bool,
    pub inline_direction: usize,
    pub bdo_without_dir: usize,
    pub bidi_controls: usize,
    pub translate_invalid: Vec<String>,
    pub ruby_without_rt: usize,
    pub content_language_meta: Option<String>,
    pub bom_in_content: usize,
    pub accessibility_metadata: Vec<String>,
}

fn push(v: &mut Vec<String>, s: String) {
    if v.len() < 20 && !v.contains(&s) {
        v.push(s);
    }
}

fn has_attr(el: &ElementRef, a: &str) -> bool {
    el.value().attr(a).is_some()
}

fn attr_lc(el: &ElementRef, a: &str) -> Option<String> {
    el.value().attr(a).map(|v| v.trim().to_ascii_lowercase())
}

fn text_of(el: &ElementRef) -> String {
    el.text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn named(el: &ElementRef) -> bool {
    ["aria-label", "aria-labelledby", "title"]
        .iter()
        .any(|a| el.value().attr(a).is_some_and(|v| !v.trim().is_empty()))
}

fn has_ancestor(el: &ElementRef, names: &[&str]) -> bool {
    el.ancestors()
        .filter_map(ElementRef::wrap)
        .any(|a| names.contains(&a.value().name()))
}

/// 요소의 암묵 역할 (주요 요소만).
pub fn implicit_role(el: &ElementRef) -> Option<&'static str> {
    let name = el.value().name();
    let typ = attr_lc(el, "type").unwrap_or_default();
    Some(match name {
        "a" | "area" if has_attr(el, "href") => "link",
        "article" => "article",
        "aside" => "complementary",
        "button" => "button",
        "datalist" => "listbox",
        "details" => "group",
        "dialog" => "dialog",
        "fieldset" => "group",
        "figure" => "figure",
        "footer" if !has_ancestor(el, &["article", "aside", "main", "nav", "section"]) => {
            "contentinfo"
        }
        "form" if named(el) => "form",
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "heading",
        "header" if !has_ancestor(el, &["article", "aside", "main", "nav", "section"]) => "banner",
        "hr" => "separator",
        "img" if el.value().attr("alt") == Some("") => "presentation",
        "img" => "img",
        "input" => match typ.as_str() {
            "button" | "image" | "reset" | "submit" => "button",
            "checkbox" => "checkbox",
            "radio" => "radio",
            "range" => "slider",
            "number" => "spinbutton",
            "search" if !has_attr(el, "list") => "searchbox",
            "" | "text" | "email" | "tel" | "url" if !has_attr(el, "list") => "textbox",
            "" | "text" | "email" | "tel" | "url" | "search" => "combobox",
            _ => return None,
        },
        "li" => "listitem",
        "main" => "main",
        "menu" | "ol" | "ul" => "list",
        "meter" => "meter",
        "nav" => "navigation",
        "option" => "option",
        "output" => "status",
        "progress" => "progressbar",
        "search" => "search",
        "section" if named(el) => "region",
        "select"
            if has_attr(el, "multiple")
                || attr_lc(el, "size").is_some_and(|s| s.parse::<u32>().is_ok_and(|n| n > 1)) =>
        {
            "listbox"
        }
        "select" => "combobox",
        "table" => "table",
        "tbody" | "thead" | "tfoot" => "rowgroup",
        "td" => "cell",
        "th" => "columnheader",
        "textarea" => "textbox",
        "tr" => "row",
        _ => return None,
    })
}

fn explicit_role(el: &ElementRef) -> Option<String> {
    attr_lc(el, "role").and_then(|r| r.split_whitespace().next().map(str::to_string))
}

fn natively_focusable(el: &ElementRef) -> bool {
    match el.value().name() {
        "a" | "area" => has_attr(el, "href"),
        "button" | "select" | "textarea" | "iframe" | "summary" => true,
        "input" => attr_lc(el, "type").as_deref() != Some("hidden"),
        _ => has_attr(el, "contenteditable"),
    }
}

pub fn extract(doc: &Html, base: &Url) -> AriaFacts {
    let mut f = AriaFacts::default();
    let ids: HashMap<&str, ElementRef> = doc
        .select(&ALL)
        .filter_map(|e| e.value().attr("id").map(|i| (i, e)))
        .collect();
    let anchors: Vec<&str> = doc
        .select(&ALL)
        .filter(|e| e.value().name() == "a")
        .filter_map(|e| e.value().attr("name"))
        .collect();
    let role_attrs: HashMap<&str, &[&str]> = ROLE_ATTRS.iter().copied().collect();
    let mut landmark_names: Vec<(String, String)> = Vec::new();
    let mut body_links = 0usize;
    let mut prev_link: Option<(String, ElementRef)> = None;

    for el in doc.select(&ALL) {
        let e = el.value();
        let name = e.name();
        let role = explicit_role(&el);
        let implicit = implicit_role(&el);
        let effective = role.clone().or_else(|| implicit.map(str::to_string));
        let tag = |s: &str| format!("{name}: {s}");

        // ===== 역할 =====
        if let Some(r) = &role {
            if implicit == Some(r.as_str()) || r == "generic" && name != "body" {
                push(&mut f.role_redundant, tag(r));
            }
            let own = OWN_ROLE.iter().find(|(n, _)| *n == name).map(|(_, r)| *r);
            let hidden_input = name == "input" && attr_lc(&el, "type").as_deref() == Some("hidden");
            if NO_ROLE.contains(&name) || hidden_input || own.is_some_and(|o| o != r) {
                push(&mut f.role_not_allowed, tag(r));
            }
            if (r == "region" || r == "application") && !named(&el) {
                f.unnamed_region_app += 1;
            }
            if r == "combobox" {
                if el
                    .descendants()
                    .filter_map(ElementRef::wrap)
                    .skip(1)
                    .any(|d| {
                        explicit_role(&d).as_deref() == Some("textbox")
                            || d.value().name() == "input"
                    })
                {
                    push(
                        &mut f.combobox_invalid,
                        "ARIA 1.1 패턴 (combobox 안에 textbox)".into(),
                    );
                }
                if attr_lc(&el, "aria-autocomplete").as_deref() == Some("inline") {
                    push(&mut f.combobox_invalid, "aria-autocomplete=inline".into());
                }
            }
            if (r == "presentation" || r == "none")
                && matches!(name, "table" | "ul" | "ol" | "dl")
                && el
                    .descendants()
                    .filter_map(ElementRef::wrap)
                    .skip(1)
                    .any(|d| {
                        matches!(
                            d.value().name(),
                            "th" | "caption"
                                | "thead"
                                | "tfoot"
                                | "h1"
                                | "h2"
                                | "h3"
                                | "h4"
                                | "h5"
                                | "h6"
                        ) || has_attr(&d, "scope")
                            || has_attr(&d, "headers")
                    })
            {
                push(&mut f.presentation_semantic, tag(r));
            }
        }
        if let Some(r) = &effective {
            if PRESENTATIONAL_CHILDREN.contains(&r.as_str())
                && !(name == "img"
                    || name == "input"
                    || name == "meter"
                    || name == "progress"
                    || name == "hr")
                && el
                    .descendants()
                    .filter_map(ElementRef::wrap)
                    .skip(1)
                    .any(|d| has_attr(&d, "role") || SEMANTIC_CHILDREN.contains(&d.value().name()))
            {
                push(&mut f.presentational_children, tag(r));
            }
            if LANDMARKS.contains(&r.as_str()) && named(&el) {
                let label = attr_lc(&el, "aria-label")
                    .or_else(|| attr_lc(&el, "title"))
                    .unwrap_or_else(|| {
                        attr_lc(&el, "aria-labelledby")
                            .and_then(|id| ids.get(id.as_str()).map(|t| text_of(t).to_lowercase()))
                            .unwrap_or_default()
                    });
                landmark_names.push((r.clone(), label));
            }
        }

        // ===== aria-* 속성 =====
        let arias: Vec<(&str, &str)> = e.attrs().filter(|(k, _)| k.starts_with("aria-")).collect();
        if !arias.is_empty() {
            let hidden_input = name == "input" && attr_lc(&el, "type").as_deref() == Some("hidden");
            if NO_ARIA.contains(&name)
                || hidden_input
                || (ONLY_HIDDEN.contains(&name) && arias.iter().any(|(k, _)| *k != "aria-hidden"))
            {
                push(&mut f.aria_on_prohibited, name.to_string());
            }
        }
        for (k, v) in &arias {
            if matches!(*k, "aria-grabbed" | "aria-dropeffect") {
                push(&mut f.deprecated_attrs, k.to_string());
            }
            if let Some(allowed) = role_attrs.get(k)
                && !effective.as_deref().is_some_and(|r| allowed.contains(&r))
            {
                push(&mut f.attr_not_for_role, format!("{name}[{k}]"));
            }
            let v = v.trim().to_ascii_lowercase();
            let conflict = match *k {
                "aria-disabled" => has_attr(&el, "disabled") && v == "false",
                "aria-readonly" => {
                    (has_attr(&el, "readonly") && v == "false")
                        || (has_attr(&el, "contenteditable") && v == "true")
                }
                "aria-required" => has_attr(&el, "required") && v == "false",
                "aria-hidden" => {
                    attr_lc(&el, "hidden").as_deref() == Some("until-found") && v == "true"
                }
                "aria-placeholder" => has_attr(&el, "placeholder"),
                "aria-valuemax" => {
                    has_attr(&el, "max")
                        && el.value().attr("max").map(str::trim) != Some(v.as_str())
                }
                "aria-valuemin" => {
                    has_attr(&el, "min")
                        && el.value().attr("min").map(str::trim) != Some(v.as_str())
                }
                "aria-checked" => {
                    name == "input"
                        && matches!(attr_lc(&el, "type").as_deref(), Some("checkbox" | "radio"))
                }
                "aria-selected" => name == "option",
                "aria-multiselectable" => name == "select",
                "aria-haspopup" => name == "input" && has_attr(&el, "list"),
                _ => false,
            };
            if conflict {
                push(&mut f.attr_conflict, format!("{name}[{k}]"));
            }
            let redundant = match *k {
                "aria-disabled" => has_attr(&el, "disabled") && v == "true",
                "aria-readonly" => has_attr(&el, "readonly") && v == "true",
                "aria-required" => has_attr(&el, "required") && v == "true",
                "aria-hidden" => {
                    attr_lc(&el, "hidden").is_some_and(|h| h != "until-found") && v == "true"
                }
                _ => false,
            };
            if redundant {
                push(&mut f.attr_redundant, format!("{name}[{k}]"));
            }
        }
        if has_attr(&el, "aria-activedescendant")
            && !natively_focusable(&el)
            && !has_attr(&el, "tabindex")
        {
            f.activedescendant_unfocusable += 1;
        }
        let tabindex = attr_lc(&el, "tabindex").and_then(|t| t.parse::<i32>().ok());
        if tabindex.is_some_and(|t| t >= 0) && !natively_focusable(&el) && role.is_none() {
            f.tabbable_non_interactive += 1;
        }
        if name == "iframe" {
            f.iframes += 1;
            f.iframe_tabindex_negative += usize::from(tabindex.is_some_and(|t| t < 0));
        }

        // ===== 폼 =====
        if name == "label" {
            f.labels += 1;
            let img_alt = el
                .descendants()
                .filter_map(ElementRef::wrap)
                .filter(|d| d.value().name() == "img")
                .any(|d| d.value().attr("alt").is_some_and(|a| !a.trim().is_empty()));
            if text_of(&el).is_empty() && !img_alt && !named(&el) {
                f.empty_labels += 1;
            }
            if let Some(target) = el.value().attr("for").and_then(|t| ids.get(t)) {
                let tn = target.value().name();
                let hidden = tn == "input" && attr_lc(target, "type").as_deref() == Some("hidden");
                if !LABELABLE.contains(&tn) || hidden {
                    push(&mut f.label_for_not_control, format!("for → {tn}"));
                }
            }
        }
        if matches!(name, "input" | "textarea")
            && e.attr("placeholder").is_some_and(|p| !p.trim().is_empty())
        {
            let id = e.attr("id");
            let labelled = has_ancestor(&el, &["label"])
                || has_attr(&el, "aria-labelledby")
                || id.is_some_and(|id| {
                    doc.select(&ALL)
                        .any(|l| l.value().name() == "label" && l.value().attr("for") == Some(id))
                });
            f.placeholder_only += usize::from(!labelled);
        }
        if name == "form" {
            let fields = el.descendants().filter_map(ElementRef::wrap).filter(|d| {
                (d.value().name() == "input"
                    && matches!(
                        attr_lc(d, "type").as_deref(),
                        None | Some(
                            "" | "text"
                                | "email"
                                | "search"
                                | "tel"
                                | "url"
                                | "password"
                                | "number"
                        )
                    ))
                    || d.value().name() == "textarea"
            });
            if fields.count() > 0 {
                f.forms_with_text += 1;
                let submit = el.descendants().filter_map(ElementRef::wrap).any(|d| {
                    (d.value().name() == "button"
                        && matches!(attr_lc(&d, "type").as_deref(), None | Some("submit")))
                        || (d.value().name() == "input"
                            && matches!(attr_lc(&d, "type").as_deref(), Some("submit" | "image")))
                }) || e.attr("id").is_some_and(|id| {
                    doc.select(&ALL)
                        .any(|b| b.value().name() == "button" && b.value().attr("form") == Some(id))
                });
                f.forms_without_submit += usize::from(!submit);
            }
        }

        // ===== 이미지·미디어 =====
        if name == "img" {
            let alt = e.attr("alt");
            if (alt == Some("") && named(&el))
                || (matches!(role.as_deref(), Some("presentation" | "none"))
                    && alt.is_some_and(|a| !a.trim().is_empty()))
            {
                f.decorative_conflict += 1;
            }
            if let Some(a) = alt.filter(|a| !a.trim().is_empty())
                && let Some(link) = el
                    .ancestors()
                    .filter_map(ElementRef::wrap)
                    .find(|x| x.value().name() == "a")
                && norm(&text_of(&link)) == norm(a)
            {
                f.alt_redundant_with_text += 1;
            }
        }
        if name == "figure" {
            f.figures += 1;
            let caption = el
                .children()
                .filter_map(ElementRef::wrap)
                .any(|c| c.value().name() == "figcaption" && !text_of(&c).is_empty());
            f.figures_unlabeled += usize::from(!caption && !named(&el));
        }
        if name == "canvas" {
            f.canvases += 1;
            let fallback = !text_of(&el).is_empty()
                || el.children().filter_map(ElementRef::wrap).next().is_some();
            f.canvases_without_fallback += usize::from(
                !fallback && !named(&el) && attr_lc(&el, "aria-hidden").as_deref() != Some("true"),
            );
        }
        if (name == "video" && !has_attr(&el, "muted")) || name == "audio" {
            f.media += 1;
            f.media_without_controls += usize::from(!has_attr(&el, "controls"));
        }
        if let Some(title) = e.attr("title").map(norm).filter(|t| !t.is_empty())
            && [
                Some(text_of(&el)),
                e.attr("alt").map(str::to_string),
                e.attr("aria-label").map(str::to_string),
            ]
            .into_iter()
            .flatten()
            .any(|x| norm(&x) == title)
        {
            f.redundant_title += 1;
        }

        // ===== 링크 =====
        if name == "a"
            && let Some(href) = e.attr("href").map(str::trim)
        {
            if has_ancestor(&el, &["body"]) {
                body_links += 1;
            }
            if let Some(frag) = href.strip_prefix('#')
                && !frag.is_empty()
                && !matches!(frag, "top")
                && !frag.starts_with('!')
                && !frag.starts_with('/')
            {
                let target = percent_decode(frag);
                if !ids.contains_key(target.as_str()) && !anchors.contains(&target.as_str()) {
                    push(&mut f.broken_fragment_links, href.to_string());
                    f.broken_skip_link |= body_links <= 3;
                }
            }
            let resolved = base.join(href).map(|u| u.to_string()).unwrap_or_default();
            if let Some((prev, prev_el)) = &prev_link
                && *prev == resolved
                && prev_el
                    .next_siblings()
                    .find(|n| !n.value().as_text().is_some_and(|t| t.trim().is_empty()))
                    .map(|n| n.id())
                    == Some(el.id())
            {
                f.adjacent_duplicate_links += 1;
            }
            prev_link = Some((resolved, el));
        }
        let has = |a: &str| e.attr(a).is_some();
        if !natively_focusable(&el)
            && ((has("onmouseover") && !has("onfocus"))
                || (has("onmouseout") && !has("onblur"))
                || ((has("onmousedown") || has("onmouseup"))
                    && !has("onkeydown")
                    && !has("onkeyup"))
                || has("ondblclick")
                || has("onmousemove"))
        {
            f.mouse_only_handlers += 1;
        }
        if let Some(style) = e.attr("style") {
            f.inline_blink += usize::from(BLINK.is_match(style));
            f.inline_direction += usize::from(DIRECTION.is_match(style));
        }
        if name == "a"
            && let Some(href) = e.attr("href")
        {
            let label = format!(
                "{} {} {}",
                text_of(&el),
                e.attr("aria-label").unwrap_or_default(),
                e.attr("title").unwrap_or_default()
            )
            .to_lowercase();
            let path = href.to_ascii_lowercase();
            if STATEMENT_WORDS.iter().any(|w| label.contains(w))
                || [
                    "/accessibility",
                    "/barrierefreiheit",
                    "/accessibilite",
                    "/a11y",
                ]
                .iter()
                .any(|p| path.contains(p))
            {
                push(&mut f.statement_links, href.trim().to_string());
            }
            let file = path.split(['?', '#']).next().unwrap_or_default();
            if let Some((_, ext)) = file.rsplit('/').next().unwrap_or_default().rsplit_once('.')
                && DOC_EXTS.contains(&ext)
            {
                match f.document_links.iter_mut().find(|(x, _)| x == ext) {
                    Some((_, n)) => *n += 1,
                    None => f.document_links.push((ext.to_string(), 1)),
                }
            }
        }

        // ===== 국제화 =====
        let lang = e.attr("lang");
        let xml_lang = e.attr("xml:lang");
        match (lang, xml_lang) {
            (None, Some(x)) => push(
                &mut f.xml_lang_issues,
                format!("{name}: xml:lang={x} 만 있음"),
            ),
            (Some(l), Some(x)) if !l.trim().eq_ignore_ascii_case(x.trim()) => push(
                &mut f.xml_lang_issues,
                format!("{name}: lang={l} xml:lang={x}"),
            ),
            _ => {}
        }
        if let Some(d) = attr_lc(&el, "dir") {
            if !matches!(d.as_str(), "ltr" | "rtl" | "auto") {
                push(&mut f.dir_invalid, format!("{name}: {d}"));
            }
            f.any_dir_rtl |= d == "rtl";
            f.html_dir_rtl |= name == "html" && d == "rtl";
        }
        if name == "bdo" && !matches!(attr_lc(&el, "dir").as_deref(), Some("ltr" | "rtl")) {
            f.bdo_without_dir += 1;
        }
        if let Some(t) = attr_lc(&el, "translate")
            && !matches!(t.as_str(), "" | "yes" | "no")
        {
            push(&mut f.translate_invalid, t);
        }
        if name == "ruby"
            && !el
                .descendants()
                .filter_map(ElementRef::wrap)
                .any(|d| d.value().name() == "rt")
        {
            f.ruby_without_rt += 1;
        }
        if name == "meta" && attr_lc(&el, "http-equiv").as_deref() == Some("content-language") {
            f.content_language_meta = e.attr("content").map(str::to_string);
        }
        if let Some(v) = e.attr("itemprop")
            && v.split_whitespace()
                .any(|p| p.starts_with("accessibility") || p.starts_with("accessMode"))
        {
            push(&mut f.accessibility_metadata, v.to_string());
        }
    }
    // 같은 역할·같은 이름의 랜드마크
    for (i, (r, n)) in landmark_names.iter().enumerate() {
        if landmark_names[..i]
            .iter()
            .any(|(r2, n2)| r2 == r && n2 == n)
        {
            push(&mut f.duplicate_landmark_names, format!("{r}: {n}"));
        }
    }
    // 본문 텍스트의 양방향 제어 문자·중간 BOM
    if let Some(body) = doc
        .select(&Selector::parse("body").expect("valid selector"))
        .next()
    {
        for t in body.text() {
            f.bidi_controls += t
                .chars()
                .filter(|c| matches!(*c as u32, 0x202A..=0x202E | 0x2066..=0x2069))
                .count();
            f.bom_in_content += t.matches('\u{feff}').count();
        }
    }
    f
}

fn percent_decode(s: &str) -> String {
    url::form_urlencoded::parse(format!("x={s}").as_bytes())
        .next()
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default()
}

/// RTL 문자를 쓰는 언어인지 (주 언어 하위 태그 또는 문자 체계 하위 태그 기준).
pub fn is_rtl_lang(lang: &str) -> bool {
    let lower = lang.trim().to_ascii_lowercase();
    let mut parts = lower.split(['-', '_']);
    let primary = parts.next().unwrap_or_default();
    RTL_LANGS.contains(&primary)
        || parts.any(|p| {
            matches!(
                p,
                "arab" | "hebr" | "thaa" | "syrc" | "nkoo" | "adlm" | "rohg"
            )
        })
}

/// CSS 텍스트의 `text-decoration: blink` 수.
pub fn css_blink(css: &str) -> usize {
    BLINK.find_iter(css).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(html: &str) -> AriaFacts {
        extract(
            &Html::parse_document(html),
            &Url::parse("https://a.com/").unwrap(),
        )
    }

    #[test]
    fn roles_and_attributes() {
        let f = run(r#"<html><body>
<button role="button">a</button><nav role="navigation"></nav><div role="generic"></div><body role="generic"></body>
<label role="button">l</label><input type="hidden" role="button"><main role="region" aria-label="m"></main>
<meta aria-label="x"><br aria-label="x"><br aria-hidden="true"><input type="hidden" aria-label="h">
<div aria-checked="true"></div><div role="checkbox" aria-checked="true" tabindex="0"></div><button aria-pressed="true">p</button>
<input disabled aria-disabled="false"><input readonly aria-readonly="true"><input required aria-required="true"><div hidden aria-hidden="true"></div>
<input placeholder="p" aria-placeholder="p"><input type="range" max="5" aria-valuemax="9" min="0" aria-valuemin="0"><input type="checkbox" aria-checked="true">
<select aria-multiselectable="true"></select><select multiple><option aria-selected="true">o</option></select><input list="l" aria-haspopup="true">
<div contenteditable aria-readonly="true"></div><div hidden="until-found" aria-hidden="true"></div><input disabled aria-disabled="true">
<span aria-grabbed="true" aria-dropeffect="move"></span>
<div role="button"><h2>제목</h2></div><button><ul><li>x</li></ul></button>
<table role="presentation"><tr><th>h</th></tr></table><ul role="none"><li>x</li></ul>
<nav aria-label="메뉴"></nav><nav aria-label="메뉴"></nav><div role="region"></div><div role="application" aria-label="앱"></div>
<span id="lbl">메뉴</span><aside aria-labelledby="lbl"></aside><aside aria-labelledby="lbl"></aside>
<div aria-activedescendant="x"></div><div role="listbox" aria-activedescendant="x" tabindex="0"></div>
<div role="combobox"><input></div><div role="combobox" aria-autocomplete="inline"></div>
<span tabindex="0">x</span><div tabindex="-1"></div>
<header role="banner"></header><article><header role="banner"></header></article><section aria-label="s" role="region"></section>
<select role="combobox"></select><select size="3" role="listbox"></select><a href="/" role="link">l</a><img alt="" role="presentation">
<input type="number" role="spinbutton"><input type="search" role="searchbox"><input list="l" role="combobox"><input type="email" role="textbox">
<input type="color" role="button"><ol role="list"></ol><output role="status"></output><td role="cell"></td>
</body></html>"#);
        assert!(f.role_redundant.len() >= 10, "{:?}", f.role_redundant);
        for s in [
            "button: button",
            "nav: navigation",
            "div: generic",
            "header: banner",
            "select: combobox",
            "select: listbox",
            "img: presentation",
            "input: spinbutton",
        ] {
            assert!(
                f.role_redundant.contains(&s.to_string()),
                "{s} {:?}",
                f.role_redundant
            );
        }
        assert!(!f.role_redundant.contains(&"body: generic".to_string()));
        assert_eq!(
            f.role_not_allowed,
            ["label: button", "input: button", "main: region"]
        );
        assert_eq!(f.aria_on_prohibited, ["meta", "br", "input"]);
        assert_eq!(
            f.attr_not_for_role,
            ["div[aria-checked]", "select[aria-multiselectable]"]
        );
        assert_eq!(f.attr_conflict.len(), 9, "{:?}", f.attr_conflict);
        assert_eq!(f.attr_redundant.len(), 4, "{:?}", f.attr_redundant);
        let mut dep = f.deprecated_attrs.clone();
        dep.sort();
        assert_eq!(dep, ["aria-dropeffect", "aria-grabbed"]);
        assert_eq!(f.presentational_children, ["div: button", "button: button"]);
        assert_eq!(f.presentation_semantic, ["table: presentation"]);
        assert_eq!(
            f.duplicate_landmark_names,
            ["navigation: 메뉴", "complementary: 메뉴"]
        );
        assert_eq!(f.unnamed_region_app, 1);
        assert_eq!(f.activedescendant_unfocusable, 1);
        assert_eq!(f.combobox_invalid.len(), 2);
        assert_eq!(f.tabbable_non_interactive, 1);
    }

    #[test]
    fn forms_images_links_i18n() {
        let f = run(
            r##"<html lang="ar" xml:lang="ar" dir="ltr"><head><meta http-equiv="Content-Language" content="ko"></head><body>
<a href="#main">본문 바로가기</a><a href="#top">위</a><a href="#!/x">x</a><a href="#%EB%B3%B8">한글</a><a name="a1"></a><a href="#a1">a</a>
<label></label><label><img alt="아이콘"></label><label for="d">d</label><div id="d"></div><label for="h">h</label><input type="hidden" id="h">
<label for="t">t</label><input id="t" placeholder="이름"><input placeholder="검색"><label>e<input placeholder="메일"></label>
<form><input type="text"></form><form id="f2"><textarea></textarea></form><button form="f2">보내기</button><form><input type="text"><button>go</button></form>
<form><input type="checkbox"></form>
<img src="a.png" alt="" title="장식"><img src="b.png" alt="로고" role="none"><a href="/home"><img src="h.png" alt="홈"> 홈</a>
<a href="/p"><img src="p.png" alt="상품"></a><a href="/p">상품</a>
<figure><img src="f.png" alt=""></figure><figure><figcaption>캡션</figcaption></figure><figure aria-label="f"></figure>
<canvas></canvas><canvas>대체</canvas><canvas aria-hidden="true"></canvas>
<video src="v.mp4"></video><video controls></video><video muted></video><audio></audio>
<a href="/x" title="링크">링크</a><img src="c.png" alt="같음" title="같음">
<div onmouseover="a()"></div><div onmouseover="a()" onfocus="a()"></div><div ondblclick="b()"></div><button onmousedown="c()">ok</button>
<span style="text-decoration: blink">깜빡</span><p style="direction: rtl">r</p><p style="unicode-bidi: bidi-override">b</p>
<a href="/accessibility">접근성</a><a href="/policy">웹 접근성 정책</a><a href="/doc/a.pdf">pdf</a><a href="/b.PDF?x=1">pdf2</a><a href="/c.hwp">hwp</a>
<p xml:lang="en">x</p><p lang="en" xml:lang="fr">y</p><p dir="sideways">z</p><bdo>q</bdo><bdo dir="auto">r</bdo><bdo dir="rtl">s</bdo>
<p translate="maybe">t</p><p translate="no">u</p><ruby>漢</ruby><ruby>漢<rt>한</rt></ruby>
<p>&#x202E;뒤집힘 &#xFEFF;중간</p><div itemprop="accessibilityFeature">x</div>
</body></html>"##,
        );
        assert_eq!(f.broken_fragment_links, ["#main", "#%EB%B3%B8"]);
        assert!(f.broken_skip_link);
        assert_eq!((f.labels, f.empty_labels), (6, 1));
        assert_eq!(f.label_for_not_control, ["for → div", "for → input"]);
        assert_eq!(f.placeholder_only, 1);
        assert_eq!((f.forms_with_text, f.forms_without_submit), (3, 1));
        assert_eq!(f.decorative_conflict, 2);
        assert_eq!(f.alt_redundant_with_text, 1);
        assert_eq!(f.adjacent_duplicate_links, 1);
        assert_eq!((f.figures, f.figures_unlabeled), (3, 1));
        assert_eq!((f.canvases, f.canvases_without_fallback), (3, 1));
        assert_eq!((f.media, f.media_without_controls), (3, 2));
        assert_eq!(f.redundant_title, 2);
        assert_eq!(f.mouse_only_handlers, 2);
        assert_eq!((f.inline_blink, f.inline_direction), (1, 2));
        assert_eq!(f.statement_links, ["/accessibility", "/policy"]);
        assert_eq!(
            f.document_links,
            [("pdf".to_string(), 2), ("hwp".to_string(), 1)]
        );
        assert_eq!(
            f.xml_lang_issues,
            ["p: xml:lang=en 만 있음", "p: lang=en xml:lang=fr"]
        );
        assert_eq!(f.dir_invalid, ["p: sideways"]);
        assert!(!f.html_dir_rtl && f.any_dir_rtl);
        assert_eq!(f.bdo_without_dir, 2);
        assert_eq!(f.translate_invalid, ["maybe"]);
        assert_eq!(f.ruby_without_rt, 1);
        assert_eq!(f.content_language_meta.as_deref(), Some("ko"));
        assert_eq!((f.bidi_controls, f.bom_in_content), (1, 1));
        assert_eq!(f.accessibility_metadata, ["accessibilityFeature"]);
        let f = run(
            r##"<html dir="rtl"><body><a href="/a">a</a><a href="/b">b</a><a href="/c">c</a><a href="/d">d</a><a href="#none">x</a></body></html>"##,
        );
        assert!(f.html_dir_rtl && f.any_dir_rtl && !f.broken_skip_link);
    }

    #[test]
    fn implicit_roles_and_frames() {
        let f = run(
            r#"<html><body><datalist role="listbox"></datalist><details role="group"></details><dialog role="dialog"></dialog>
<fieldset role="group"></fieldset><hr role="separator"><meter role="meter"></meter><progress role="progressbar"></progress>
<search role="search"></search><iframe tabindex="-1"></iframe><iframe></iframe><div onmousedown="a()"></div></body></html>"#,
        );
        for s in [
            "details: group",
            "dialog: dialog",
            "fieldset: group",
            "hr: separator",
            "meter: meter",
            "progress: progressbar",
            "search: search",
        ] {
            assert!(
                f.role_redundant.contains(&s.to_string()),
                "{s} {:?}",
                f.role_redundant
            );
        }
        assert_eq!(
            (f.iframes, f.iframe_tabindex_negative, f.mouse_only_handlers),
            (2, 1, 1)
        );
    }

    #[test]
    fn helpers() {
        assert!(
            is_rtl_lang("ar")
                && is_rtl_lang("he-IL")
                && is_rtl_lang("ku-Arab")
                && !is_rtl_lang("ko")
                && !is_rtl_lang("en-US")
        );
        assert_eq!(
            css_blink("a{text-decoration: underline blink} b{text-decoration-line:blink}"),
            2
        );
        assert_eq!(percent_decode("%EB%B3%B8"), "본");
    }
}
