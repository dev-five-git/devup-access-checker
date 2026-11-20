//! Nu HTML Checker·HTMLHint·webhint·Chrome DevTools 와 대조해 추가한 HTML 적합성·성능 정적 신호.
//! 대조 결과는 `docs/coverage.md` 에 있다. 네트워크 접근 없음.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use unicode_normalization::is_nfc;
use url::Url;

static ALL: LazyLock<Selector> = LazyLock::new(|| Selector::parse("*").expect("valid selector"));
static BAD_PERCENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"%(?:[^0-9A-Fa-f]|[0-9A-Fa-f][^0-9A-Fa-f]|[0-9A-Fa-f]?$)").unwrap()
});
static DATETIME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"^(?:\d{4,}(?:-\d{2}(?:-\d{2})?)?",
        r"|--\d{2}-\d{2}",
        r"|\d{4,}-W\d{2}",
        r"|\d{2}:\d{2}(?::\d{2}(?:\.\d{1,3})?)?",
        r"|\d{4,}-\d{2}-\d{2}[T ]\d{2}:\d{2}(?::\d{2}(?:\.\d{1,3})?)?(?:Z|[+-]\d{2}:?\d{2})?",
        r"|Z|[+-]\d{2}:?\d{2}",
        r"|P(?:\d+D)?(?:T(?:\d+H)?(?:\d+M)?(?:\d+(?:\.\d+)?S)?)?",
        r"|(?:\s*\d+(?:\.\d+)?\s*[wdhms])+\s*)$"
    ))
    .unwrap()
});
static AD_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:^|[-_])ad(?:[-_]|$)").unwrap());
static UNSAFE_CHARS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new("[\u{0}-\u{8}\u{b}\u{c}\u{e}-\u{1f}\u{7f}-\u{9f}\u{ad}\u{600}-\u{604}\u{70f}\u{17b4}\u{17b5}\u{200c}-\u{200f}\u{2028}-\u{202f}\u{2060}-\u{206f}\u{feff}\u{fff0}-\u{ffff}]").unwrap()
});
static GENERIC_FIELD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)e-?mail|phone|\btel\b|mobile|zip|postal|card").unwrap());

/// HTML 명세 요소 목록 (현행 + 폐기, SVG·MathML 은 제외하고 svg·math 루트만).
const HTML_ELEMENTS: &[&str] = &[
    "a",
    "abbr",
    "address",
    "area",
    "article",
    "aside",
    "audio",
    "b",
    "base",
    "bdi",
    "bdo",
    "blockquote",
    "body",
    "br",
    "button",
    "canvas",
    "caption",
    "cite",
    "code",
    "col",
    "colgroup",
    "data",
    "datalist",
    "dd",
    "del",
    "details",
    "dfn",
    "dialog",
    "div",
    "dl",
    "dt",
    "em",
    "embed",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hgroup",
    "hr",
    "html",
    "i",
    "iframe",
    "img",
    "input",
    "ins",
    "kbd",
    "label",
    "legend",
    "li",
    "link",
    "main",
    "map",
    "mark",
    "menu",
    "meta",
    "meter",
    "nav",
    "noscript",
    "object",
    "ol",
    "optgroup",
    "option",
    "output",
    "p",
    "param",
    "picture",
    "pre",
    "progress",
    "q",
    "rp",
    "rt",
    "ruby",
    "s",
    "samp",
    "script",
    "search",
    "section",
    "select",
    "selectedcontent",
    "slot",
    "small",
    "source",
    "span",
    "strong",
    "style",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "template",
    "textarea",
    "tfoot",
    "th",
    "thead",
    "time",
    "title",
    "tr",
    "track",
    "u",
    "ul",
    "var",
    "video",
    "wbr",
    "svg",
    "math",
    // 폐기 요소 (accessibility.obsolete_elements 가 따로 본다)
    "acronym",
    "applet",
    "basefont",
    "bgsound",
    "big",
    "blink",
    "center",
    "dir",
    "font",
    "frame",
    "frameset",
    "isindex",
    "keygen",
    "listing",
    "marquee",
    "menuitem",
    "multicol",
    "nextid",
    "nobr",
    "noembed",
    "noframes",
    "plaintext",
    "rb",
    "rtc",
    "spacer",
    "strike",
    "tt",
    "xmp",
    "image",
];
/// 같은 요소를 품을 수 없는 요소 (Nu "must not appear as a descendant of")
const NO_SELF_NEST: &[&str] = &[
    "form", "label", "progress", "meter", "dfn", "noscript", "address", "a", "button",
];
const BLOCKS: &[&str] = &[
    "div",
    "p",
    "ul",
    "ol",
    "dl",
    "table",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "section",
    "article",
    "nav",
    "header",
    "footer",
    "form",
    "figure",
    "blockquote",
    "pre",
    "hr",
    "main",
    "aside",
    "address",
    "fieldset",
];
const PHRASING_ONLY: &[&str] = &[
    "button", "label", "span", "legend", "h1", "h2", "h3", "h4", "h5", "h6", "p", "a",
];
const ENUMS: &[(&str, &str, &[&str])] = &[
    (
        "input",
        "type",
        &[
            "hidden",
            "text",
            "search",
            "tel",
            "url",
            "email",
            "password",
            "date",
            "month",
            "week",
            "time",
            "datetime-local",
            "number",
            "range",
            "color",
            "checkbox",
            "radio",
            "file",
            "submit",
            "image",
            "reset",
            "button",
        ],
    ),
    ("button", "type", &["submit", "reset", "button"]),
    ("form", "method", &["get", "post", "dialog"]),
    (
        "form",
        "enctype",
        &[
            "application/x-www-form-urlencoded",
            "multipart/form-data",
            "text/plain",
        ],
    ),
    ("*", "loading", &["lazy", "eager"]),
    ("*", "decoding", &["sync", "async", "auto"]),
    ("*", "fetchpriority", &["high", "low", "auto"]),
    ("*", "crossorigin", &["", "anonymous", "use-credentials"]),
    (
        "*",
        "referrerpolicy",
        &[
            "",
            "no-referrer",
            "no-referrer-when-downgrade",
            "same-origin",
            "origin",
            "strict-origin",
            "origin-when-cross-origin",
            "strict-origin-when-cross-origin",
            "unsafe-url",
        ],
    ),
    ("video", "preload", &["", "none", "metadata", "auto"]),
    ("audio", "preload", &["", "none", "metadata", "auto"]),
    (
        "track",
        "kind",
        &[
            "subtitles",
            "captions",
            "descriptions",
            "chapters",
            "metadata",
        ],
    ),
];
const PRELOAD_AS: &[&str] = &[
    "audio",
    "audioworklet",
    "document",
    "embed",
    "fetch",
    "font",
    "image",
    "manifest",
    "object",
    "paintworklet",
    "report",
    "script",
    "serviceworker",
    "sharedworker",
    "style",
    "track",
    "video",
    "worker",
    "xslt",
    "json",
];
/// 파사드로 대체할 수 있는 무거운 임베드 (third-party-web 의 facades 와 국내 채팅 위젯)
const FACADE_HOSTS: &[&str] = &[
    "youtube.com",
    "youtube-nocookie.com",
    "player.vimeo.com",
    "widget.intercom.io",
    "js.intercomcdn.com",
    "js.driftt.com",
    "static.zdassets.com",
    "cdn.livechatinc.com",
    "embed.tawk.to",
    "client.crisp.chat",
    "cdn.channel.io",
    "beacon-v2.helpscout.net",
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QualityFacts {
    pub base_after_resources: bool,
    pub bases: usize,
    pub invalid_nesting: Vec<String>,
    pub phrasing_violations: Vec<String>,
    pub form_controls: usize,
    pub form_errors: Vec<String>,
    pub invalid_urls: Vec<String>,
    pub invalid_enums: Vec<String>,
    pub times: usize,
    pub invalid_times: Vec<String>,
    pub unknown_elements: Vec<String>,
    pub non_nfc: usize,
    pub microdata_items: usize,
    pub microdata_errors: Vec<String>,
    pub sections: usize,
    pub sections_without_heading: usize,
    pub empty_src: Vec<String>,
    pub srcset_errors: Vec<String>,
    pub preloads: usize,
    pub preload_errors: Vec<String>,
    pub script_attr_errors: Vec<String>,
    pub script_json_errors: Vec<String>,
    pub speculation_rules: bool,
    pub preconnect_origins: Vec<String>,
    pub preconnect_unused: Vec<String>,
    pub preconnect_font_no_crossorigin: Vec<String>,
    pub iframes: usize,
    pub iframes_not_lazy: usize,
    pub facade_embeds: Vec<String>,
    pub videos: usize,
    pub videos_preload_heavy: usize,
    pub img_decoding_async: usize,
    pub generic_inputs: Vec<String>,
    pub forms: usize,
    pub forms_without_method: usize,
    pub inline_styles: usize,
    pub inline_handlers: usize,
    pub ad_class_names: Vec<String>,
    pub unsafe_attr_chars: Vec<String>,
}

fn push(v: &mut Vec<String>, s: String) {
    if v.len() < 20 && !v.contains(&s) {
        v.push(s);
    }
}

fn ancestors<'a>(el: &ElementRef<'a>) -> impl Iterator<Item = ElementRef<'a>> {
    el.ancestors().filter_map(ElementRef::wrap)
}

fn in_foreign(el: &ElementRef) -> bool {
    ancestors(el).any(|a| matches!(a.value().name(), "svg" | "math"))
}

pub fn extract(doc: &Html, base: &Url) -> QualityFacts {
    let mut f = QualityFacts::default();
    let ids: HashSet<&str> = doc
        .select(&ALL)
        .filter_map(|e| e.value().attr("id"))
        .collect();
    let forms_by_id: HashSet<&str> = doc
        .select(&ALL)
        .filter(|e| e.value().name() == "form")
        .filter_map(|e| e.value().attr("id"))
        .collect();
    let datalists: HashSet<&str> = doc
        .select(&ALL)
        .filter(|e| e.value().name() == "datalist")
        .filter_map(|e| e.value().attr("id"))
        .collect();
    let itemref_ids: HashSet<&str> = doc
        .select(&ALL)
        .filter_map(|e| e.value().attr("itemref"))
        .flat_map(str::split_ascii_whitespace)
        .collect();
    let mut seen_resource = false;
    let mut autofocus = 0;
    let mut referenced = String::new();

    for el in doc.select(&ALL) {
        let e = el.value();
        let name = e.name();
        let get = |a: &str| e.attr(a).map(str::trim);
        let lc = |a: &str| get(a).map(str::to_ascii_lowercase);
        for (k, v) in e.attrs() {
            let hint = name == "link"
                && e.attr("rel").is_some_and(|r| {
                    let r = r.to_ascii_lowercase();
                    r.contains("preconnect") || r.contains("dns-prefetch")
                });
            if matches!(k, "src" | "href" | "srcset" | "action" | "data" | "poster") && !hint {
                referenced.push_str(v);
                referenced.push(' ');
            }
            if UNSAFE_CHARS.is_match(v) {
                push(&mut f.unsafe_attr_chars, format!("{name}[{k}]"));
            }
        }

        // ===== 문서 구조 =====
        match name {
            "link" | "script" => seen_resource = true,
            "base" => {
                f.bases += 1;
                f.base_after_resources |= seen_resource;
            }
            _ => {}
        }
        if NO_SELF_NEST.contains(&name) && ancestors(&el).any(|a| a.value().name() == name) {
            push(&mut f.invalid_nesting, format!("{name} 안의 {name}"));
        }
        if matches!(name, "header" | "footer")
            && ancestors(&el).any(|a| matches!(a.value().name(), "header" | "footer"))
        {
            push(&mut f.invalid_nesting, format!("header·footer 안의 {name}"));
        }
        if name == "main"
            && ancestors(&el).any(|a| {
                matches!(
                    a.value().name(),
                    "article" | "aside" | "footer" | "header" | "nav"
                )
            })
        {
            push(
                &mut f.invalid_nesting,
                "article·aside·header·footer·nav 안의 main".into(),
            );
        }
        if BLOCKS.contains(&name)
            && let Some(p) = ancestors(&el).find(|a| PHRASING_ONLY.contains(&a.value().name()))
            && !(p.value().name() == "a"
                && !ancestors(&p)
                    .any(|x| PHRASING_ONLY.contains(&x.value().name()) && x.value().name() != "a"))
        {
            push(
                &mut f.phrasing_violations,
                format!("{} 안의 {name}", p.value().name()),
            );
        }
        if !in_foreign(&el) && !HTML_ELEMENTS.contains(&name) && !name.contains('-') {
            push(&mut f.unknown_elements, name.to_string());
        }

        // ===== 폼 =====
        if matches!(name, "input" | "select" | "textarea" | "button") {
            f.form_controls += 1;
            if let Some(fid) = get("form")
                && !forms_by_id.contains(fid)
            {
                push(&mut f.form_errors, format!("form=\"{fid}\" 대상 없음"));
            }
            autofocus += usize::from(e.attr("autofocus").is_some());
        }
        if name == "input"
            && let Some(l) = get("list")
            && !datalists.contains(l)
        {
            push(
                &mut f.form_errors,
                format!("list=\"{l}\" 가 datalist 가 아님"),
            );
        }
        if name == "select" {
            let options: Vec<ElementRef> = el
                .descendants()
                .filter_map(ElementRef::wrap)
                .filter(|o| o.value().name() == "option")
                .collect();
            let selected = options
                .iter()
                .filter(|o| o.value().attr("selected").is_some())
                .count();
            if selected > 1 && e.attr("multiple").is_none() {
                push(
                    &mut f.form_errors,
                    "multiple 없는 select 에 selected 여러 개".into(),
                );
            }
            let single = e.attr("multiple").is_none() && get("size").is_none_or(|s| s == "1");
            if e.attr("required").is_some() && single {
                let first_ok = options.first().is_some_and(|o| {
                    o.value().attr("value").map_or_else(
                        || o.text().collect::<String>().trim().is_empty(),
                        |v| v.is_empty(),
                    ) && !ancestors(o).any(|a| a.value().name() == "optgroup")
                });
                if !first_ok {
                    push(
                        &mut f.form_errors,
                        "required select 의 첫 option 값이 비어 있지 않음".into(),
                    );
                }
            }
        }
        if name == "label" {
            let labelable = el
                .descendants()
                .filter_map(ElementRef::wrap)
                .filter(|d| {
                    matches!(
                        d.value().name(),
                        "input"
                            | "select"
                            | "textarea"
                            | "button"
                            | "meter"
                            | "output"
                            | "progress"
                    ) && d
                        .value()
                        .attr("type")
                        .is_none_or(|t| !t.eq_ignore_ascii_case("hidden"))
                })
                .count();
            if labelable > 1 {
                push(&mut f.form_errors, "label 하나에 입력 요소 여러 개".into());
            }
        }
        if name == "form" {
            f.forms += 1;
            f.forms_without_method += usize::from(e.attr("method").is_none());
        }
        if name == "input"
            && lc("type").is_none_or(|t| t == "text" || t.is_empty())
            && e.attr("inputmode").is_none()
        {
            let hint = format!(
                "{} {} {}",
                get("name").unwrap_or_default(),
                get("id").unwrap_or_default(),
                get("autocomplete").unwrap_or_default()
            );
            if GENERIC_FIELD.is_match(&hint) {
                push(
                    &mut f.generic_inputs,
                    hint.split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_string(),
                );
            }
        }

        // ===== 속성 값 =====
        for a in ["href", "src", "action", "poster", "cite", "data"] {
            if let Some(v) = e.attr(a) {
                let t = v.trim();
                if !t.to_ascii_lowercase().starts_with("javascript:")
                    && !t.starts_with("data:")
                    && (t.contains([' ', '\\', '\t', '\n']) || BAD_PERCENT.is_match(t))
                {
                    push(
                        &mut f.invalid_urls,
                        format!("{name}[{a}]={}", t.chars().take(80).collect::<String>()),
                    );
                }
            }
        }
        for (el_name, attr, allowed) in ENUMS {
            if (*el_name == "*" || *el_name == name)
                && let Some(v) = lc(attr)
                && !allowed.contains(&v.as_str())
            {
                push(&mut f.invalid_enums, format!("{name}[{attr}={v}]"));
            }
        }
        if name == "time" {
            f.times += 1;
            let v = get("datetime")
                .map(str::to_string)
                .unwrap_or_else(|| el.text().collect::<String>().trim().to_string());
            if !DATETIME.is_match(&v) {
                push(&mut f.invalid_times, v.chars().take(40).collect());
            }
        }

        // ===== microdata =====
        let scope = e.attr("itemscope").is_some();
        f.microdata_items += usize::from(scope);
        if e.attr("itemprop").is_some()
            && !ancestors(&el).any(|a| a.value().attr("itemscope").is_some())
            && !e.attr("id").is_some_and(|id| itemref_ids.contains(id))
        {
            push(
                &mut f.microdata_errors,
                format!("{name}[itemprop] 이 item 밖에 있음"),
            );
        }
        if !scope
            && (e.attr("itemtype").is_some()
                || e.attr("itemid").is_some()
                || e.attr("itemref").is_some())
        {
            push(
                &mut f.microdata_errors,
                format!("{name}: itemscope 없이 itemtype·itemid·itemref"),
            );
        }
        for r in get("itemref").unwrap_or_default().split_ascii_whitespace() {
            if !ids.contains(r) {
                push(
                    &mut f.microdata_errors,
                    format!("itemref=\"{r}\" 대상 없음"),
                );
            }
        }
        if matches!(name, "section" | "article") {
            f.sections += 1;
            let heading = el
                .descendants()
                .filter_map(ElementRef::wrap)
                .any(|d| matches!(d.value().name(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6"));
            f.sections_without_heading += usize::from(!heading);
        }

        // ===== 리소스 =====
        if (matches!(
            name,
            "img" | "script" | "iframe" | "embed" | "source" | "audio" | "video"
        ) && get("src") == Some(""))
            || (name == "link" && get("href") == Some(""))
            || (name == "img"
                && e.attr("src").is_none()
                && e.attr("srcset").is_none()
                && e.attr("data-src").is_none())
        {
            push(&mut f.empty_src, name.to_string());
        }
        let srcset = get("srcset");
        if matches!(name, "img" | "source") {
            if srcset.is_some_and(|s| s.split(',').any(|c| c.trim().ends_with('w')))
                && e.attr("sizes").is_none()
            {
                push(
                    &mut f.srcset_errors,
                    format!("{name}: w 서술자에 sizes 없음"),
                );
            }
            if e.attr("sizes").is_some() && srcset.is_none() {
                push(&mut f.srcset_errors, format!("{name}: srcset 없이 sizes"));
            }
        }
        let rel = lc("rel").unwrap_or_default();
        let rels: Vec<&str> = rel.split_ascii_whitespace().collect();
        if name == "link" && rels.contains(&"preload") {
            f.preloads += 1;
            match lc("as") {
                None => push(&mut f.preload_errors, "as 없음".into()),
                Some(a) if !PRELOAD_AS.contains(&a.as_str()) => {
                    push(&mut f.preload_errors, format!("as={a}"))
                }
                Some(a) if a == "font" && e.attr("crossorigin").is_none() => push(
                    &mut f.preload_errors,
                    "글꼴 preload 에 crossorigin 없음".into(),
                ),
                _ => {}
            }
        }
        if name == "link"
            && rels.contains(&"preconnect")
            && let Some(u) = get("href").and_then(|h| base.join(h).ok())
        {
            let origin = u.origin().ascii_serialization();
            if !f.preconnect_origins.contains(&origin) {
                f.preconnect_origins.push(origin);
            }
            if u.host_str() == Some("fonts.gstatic.com") && e.attr("crossorigin").is_none() {
                push(&mut f.preconnect_font_no_crossorigin, u.to_string());
            }
        }
        if name == "script" {
            let typ = lc("type").unwrap_or_default();
            let inline = e.attr("src").is_none();
            if inline {
                for a in ["async", "defer", "integrity"] {
                    if e.attr(a).is_some() {
                        push(&mut f.script_attr_errors, format!("인라인 script 의 {a}"));
                    }
                }
                if e.attr("charset").is_some() {
                    push(
                        &mut f.script_attr_errors,
                        "src 없는 script 의 charset".into(),
                    );
                }
            }
            if typ == "module" && e.attr("nomodule").is_some() {
                push(
                    &mut f.script_attr_errors,
                    "type=module 과 nomodule 함께 사용".into(),
                );
            }
            if matches!(typ.as_str(), "importmap" | "speculationrules") {
                f.speculation_rules |= typ == "speculationrules";
                let body = el.text().collect::<String>();
                match serde_json::from_str::<serde_json::Value>(body.trim()) {
                    Ok(serde_json::Value::Object(m)) => {
                        let allowed: &[&str] = if typ == "importmap" {
                            &["imports", "scopes", "integrity"]
                        } else {
                            &["prefetch", "prerender", "tag"]
                        };
                        for k in m.keys().filter(|k| !allowed.contains(&k.as_str())) {
                            push(
                                &mut f.script_json_errors,
                                format!("{typ}: 알 수 없는 키 {k}"),
                            );
                        }
                    }
                    _ => push(
                        &mut f.script_json_errors,
                        format!("{typ}: JSON 객체가 아님"),
                    ),
                }
            }
        }
        if name == "iframe" {
            f.iframes += 1;
            if f.iframes > 1 && lc("loading").as_deref() != Some("lazy") {
                f.iframes_not_lazy += 1;
            }
        }
        if matches!(name, "iframe" | "script")
            && let Some(u) = get("src").and_then(|s| base.join(s).ok())
            && let Some(h) = u.host_str()
            && FACADE_HOSTS
                .iter()
                .any(|f2| h == *f2 || h.ends_with(&format!(".{f2}")))
        {
            push(&mut f.facade_embeds, h.to_string());
        }
        if name == "video" {
            f.videos += 1;
            if e.attr("autoplay").is_none()
                && !matches!(lc("preload").as_deref(), Some("none" | "metadata"))
            {
                f.videos_preload_heavy += 1;
            }
        }
        f.img_decoding_async +=
            usize::from(name == "img" && lc("decoding").as_deref() == Some("async"));
        f.inline_styles += usize::from(e.attr("style").is_some());
        f.inline_handlers += usize::from(e.attrs().any(|(k, _)| k.starts_with("on")));
        for token in e.classes().chain(e.id()) {
            if AD_TOKEN.is_match(token) {
                push(&mut f.ad_class_names, token.to_string());
            }
        }
    }
    if autofocus > 1 {
        push(&mut f.form_errors, "autofocus 여러 개".into());
    }
    for origin in &f.preconnect_origins {
        let host = origin.split("://").nth(1).unwrap_or_default();
        if !referenced.contains(host) {
            push(&mut f.preconnect_unused, origin.clone());
        }
    }
    if let Some(body) = doc
        .select(&Selector::parse("body").expect("valid selector"))
        .next()
    {
        f.non_nfc = body.text().filter(|t| !is_nfc(t)).count();
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(html: &str) -> QualityFacts {
        extract(
            &Html::parse_document(html),
            &Url::parse("https://a.com/").unwrap(),
        )
    }

    #[test]
    fn conformance_and_resources() {
        let f = run(
            r#"<html><head><link rel="stylesheet" href="/a.css"><base href="/"><base href="/x/">
<link rel="preconnect" href="https://fonts.gstatic.com"><link rel="preconnect" href="https://cdn.a.com">
<link rel="preconnect" href="https://b.com"><link rel="preconnect" href="https://c.com"><link rel="preconnect" href="https://d.com">
<link rel="preconnect" href="https://cdn.a.com"><link rel="preload" href="/f.woff2" as="font"><link rel="preload" href="/x"><link rel="preload" href="/y" as="banana">
<link rel="preload" href="/z" as="style"><script async>inline()</script><script charset="utf-8">x()</script><script type="module" nomodule src="/m.js"></script>
<script type="importmap">{"imports":{},"bogus":1}</script><script type="speculationrules">[1]</script><script type="speculationrules">{"prefetch":[],"bad":1}</script><link href=""></head>
<body><form><form></form></form><label><label>x</label></label><header><footer>f</footer></header><caption><table></table></caption>
<article><main>m</main></article><button><div>블록</div></button><a href="/"><div>허용</div></a><span><a href="/"><div>금지</div></a></span>
<x-widget></x-widget><foo>?</foo><svg><g></g></svg>
<input form="nope" autofocus><input list="dl"><datalist id="dl2"></datalist><input autofocus>
<select required><option value="1">a</option></select><select><option selected>a</option><option selected>b</option></select>
<select required><option value="">선택</option></select><label><input><input></label><select required><option></option></select><label><input type="hidden"><input></label>
<a href="/a b">s</a><img src="a\b.png"><a href="/%zz">p</a><a href="javascript:go(1 2)">j</a>
<input type="bogus"><button type="go">b</button><form method="put"></form><img loading="soon"><track kind="x">
<time datetime="2026/01">x</time><time>2026-01-01</time><time datetime="PT2H">d</time><time>어제</time>
<p>한글</p><p>한글</p>
<div itemprop="name">a</div><span itemtype="https://schema.org/Thing">b</span><div itemscope itemref="nope"></div><div itemscope itemref="r"><p itemprop="x">ok</p></div><p id="r" itemprop="y">ref</p>
<section><p>no heading</p></section><article><h2>ok</h2></article>
<img src=""><img srcset="a.png 100w"><img sizes="100vw" src="b.png"><img data-src="lazy.png">
<iframe src="https://www.youtube.com/embed/x"></iframe><iframe src="/b"></iframe><iframe src="/c" loading="lazy"></iframe>
<video src="v.mp4"></video><video src="w.mp4" autoplay></video><video preload="none"></video>
<img src="d.png" decoding="async"><input name="email"><input id="phone" inputmode="tel"><input name="title">
<p style="color:red" onclick="x()">s</p><div class="ad-banner"></div><div id="ads"></div><div class="header"></div><p title="a&#x200E;b">z</p>
</body></html>"#,
        );
        assert!(f.base_after_resources);
        assert_eq!(f.bases, 2);
        assert!(f.invalid_nesting.len() >= 3, "{:?}", f.invalid_nesting);
        assert!(
            f.phrasing_violations
                .contains(&"button 안의 div".to_string()),
            "{:?}",
            f.phrasing_violations
        );
        assert!(
            f.phrasing_violations.contains(&"a 안의 div".to_string()),
            "{:?}",
            f.phrasing_violations
        );
        assert_eq!(f.unknown_elements, ["foo"]);
        assert_eq!(f.form_errors.len(), 6, "{:?}", f.form_errors);
        assert_eq!(f.invalid_urls.len(), 3, "{:?}", f.invalid_urls);
        assert_eq!(f.invalid_enums.len(), 5, "{:?}", f.invalid_enums);
        assert_eq!((f.times, f.invalid_times.len()), (4, 2));
        assert_eq!(f.microdata_errors.len(), 3, "{:?}", f.microdata_errors);
        assert_eq!((f.sections, f.sections_without_heading), (3, 2));
        assert_eq!(f.empty_src.len(), 2, "{:?}", f.empty_src);
        assert_eq!(f.srcset_errors.len(), 2);
        assert_eq!((f.preloads, f.preload_errors.len()), (4, 3));
        assert_eq!(f.script_attr_errors.len(), 3, "{:?}", f.script_attr_errors);
        assert_eq!(f.script_json_errors.len(), 3, "{:?}", f.script_json_errors);
        assert!(f.speculation_rules);
        assert_eq!(f.preconnect_origins.len(), 5);
        assert!(f.preconnect_unused.len() >= 4);
        assert_eq!(f.preconnect_font_no_crossorigin.len(), 1);
        assert_eq!((f.iframes, f.iframes_not_lazy), (3, 1));
        assert_eq!(f.facade_embeds, ["www.youtube.com"]);
        assert_eq!((f.videos, f.videos_preload_heavy), (3, 1));
        assert_eq!(f.img_decoding_async, 1);
        assert_eq!(f.generic_inputs, ["email"]);
        assert_eq!((f.forms, f.forms_without_method), (2, 1));
        assert_eq!((f.inline_styles, f.inline_handlers), (1, 1));
        assert_eq!(f.ad_class_names, ["ad-banner"]);
        assert_eq!(f.unsafe_attr_chars, ["p[title]"]);
        let nfd: String = unicode_normalization::UnicodeNormalization::nfd("한글").collect();
        let f = run(&format!("<html><body><p>{nfd}</p></body></html>"));
        assert_eq!(f.non_nfc, 1);
        assert_eq!(run("<html><body></body></html>"), QualityFacts::default());
    }
}
