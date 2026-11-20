//! 공식 명세(WCAG 2.2 성공 기준, WHATWG HTML 폐기 속성)와 대조해 추가한 정적 신호.
//! 대조 결과는 `docs/coverage.md` 에 있다. 네트워크 접근 없음.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use scraper::{Html, Selector};

static ALL: LazyLock<Selector> = LazyLock::new(|| Selector::parse("*").expect("valid selector"));
static JUSTIFY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)text-align\s*:\s*justify").unwrap());

/// WHATWG HTML 명세 "Obsolete features" 의 폐기 속성 → 해당 요소 (빈 목록 = 모든 요소).
/// `scripts/check_coverage.py` 가 명세 원문과 다시 대조한다.
pub const OBSOLETE_ATTRS: &[(&str, &[&str])] = &[
    ("abbr", &["td"]),
    ("accept", &["form"]),
    (
        "align",
        &[
            "caption", "col", "div", "embed", "h1", "h2", "h3", "h4", "h5", "h6", "hr", "iframe",
            "img", "input", "legend", "object", "p", "table", "tbody", "td", "tfoot", "th",
            "thead", "tr",
        ],
    ),
    ("alink", &["body"]),
    ("allowtransparency", &["iframe"]),
    ("archive", &["object"]),
    ("axis", &["td", "th"]),
    (
        "background",
        &["body", "table", "tbody", "td", "tfoot", "th", "thead", "tr"],
    ),
    ("bgcolor", &["body", "table", "td", "th", "tr"]),
    ("border", &["img", "input", "object", "table"]),
    ("bordercolor", &["table"]),
    ("bottommargin", &["body"]),
    ("cellpadding", &["table"]),
    ("cellspacing", &["table"]),
    (
        "char",
        &["col", "tbody", "td", "tfoot", "th", "thead", "tr"],
    ),
    (
        "charoff",
        &["col", "tbody", "td", "tfoot", "th", "thead", "tr"],
    ),
    ("charset", &["a", "link", "script"]),
    ("classid", &["object"]),
    ("clear", &["br"]),
    ("code", &["object"]),
    ("codebase", &["object"]),
    ("codetype", &["object"]),
    ("color", &["hr"]),
    ("compact", &["dl", "menu", "ol", "ul"]),
    ("contextmenu", &[]),
    ("coords", &["a"]),
    (
        "datafld",
        &[
            "a", "button", "div", "fieldset", "frame", "iframe", "img", "input", "label", "legend",
            "marquee", "object", "select", "span", "textarea",
        ],
    ),
    (
        "dataformatas",
        &[
            "button", "div", "input", "label", "legend", "marquee", "object", "option", "select",
            "span", "table",
        ],
    ),
    ("datapagesize", &["table"]),
    (
        "datasrc",
        &[
            "a", "button", "div", "frame", "iframe", "img", "input", "label", "legend", "marquee",
            "object", "option", "select", "span", "table", "textarea",
        ],
    ),
    ("declare", &["object"]),
    ("dropzone", &[]),
    ("event", &["script"]),
    ("for", &["script"]),
    ("frame", &["table"]),
    ("frameborder", &["iframe"]),
    ("framespacing", &["iframe"]),
    (
        "height",
        &["table", "tbody", "td", "tfoot", "th", "thead", "tr"],
    ),
    ("hreflang", &["area"]),
    ("hspace", &["embed", "iframe", "img", "input", "object"]),
    ("ismap", &["input"]),
    ("label", &["menu"]),
    ("language", &["script"]),
    ("leftmargin", &["body"]),
    ("link", &["body"]),
    ("longdesc", &["iframe", "img"]),
    ("lowsrc", &["img"]),
    ("manifest", &["html"]),
    ("marginheight", &["body", "iframe"]),
    ("marginwidth", &["body", "iframe"]),
    ("methods", &["a", "link"]),
    ("name", &["a", "embed", "img", "option"]),
    ("nohref", &["area"]),
    ("noshade", &["hr"]),
    ("nowrap", &["td", "th"]),
    ("onshow", &[]),
    ("profile", &["head"]),
    ("rev", &["a", "link"]),
    ("rightmargin", &["body"]),
    ("rules", &["table"]),
    ("scheme", &["meta"]),
    ("scope", &["td"]),
    ("scrolling", &["iframe"]),
    ("shape", &["a"]),
    ("size", &["hr"]),
    ("standby", &["object"]),
    ("summary", &["table"]),
    ("target", &["link"]),
    ("text", &["body"]),
    ("topmargin", &["body"]),
    ("type", &["area", "li", "menu", "style", "ul"]),
    ("typemustmatch", &["object"]),
    ("urn", &["a", "link"]),
    ("usemap", &["input", "object"]),
    (
        "valign",
        &["col", "tbody", "td", "tfoot", "th", "thead", "tr"],
    ),
    ("version", &["html"]),
    ("vlink", &["body"]),
    ("vspace", &["embed", "iframe", "img", "input", "object"]),
    ("width", &["col", "hr", "pre", "table", "td", "th"]),
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpecFacts {
    /// 폐기 속성 사용 수와 예시 (`요소[속성]`)
    pub obsolete_attrs: usize,
    pub obsolete_attr_samples: Vec<String>,
    pub abbr_total: usize,
    /// title 없는 `<abbr>` (WCAG 3.1.4 약어)
    pub abbr_without_title: usize,
    /// 값을 바꾸면 페이지를 이동·제출하는 onchange (WCAG 3.2.2 입력 시)
    pub onchange_navigation: usize,
    /// 초점만 받아도 이동·제출하는 onfocus (WCAG 3.2.1 초점 시)
    pub onfocus_navigation: usize,
    /// 소리 있는 영상 (muted 제외)
    pub videos: usize,
    /// 음성 해설 트랙(`track kind="descriptions"`)이 없는 영상 (WCAG 1.2.5)
    pub videos_without_descriptions: usize,
    pub password_fields: usize,
    /// 자동 완성을 끈 비밀번호 칸 (WCAG 3.3.8 접근 가능한 인증)
    pub password_autocomplete_off: usize,
    /// 사이트 검색 (`<search>`, `role="search"`, `type="search"`) — WCAG 2.4.5 여러 경로
    pub site_search: bool,
    /// 이동 경로(브레드크럼) 표시 — WCAG 2.4.8 위치
    pub breadcrumb: bool,
    /// 인라인 style 의 `text-align: justify` (WCAG 1.4.8 시각적 표현)
    pub inline_justify: usize,
}

/// 명세가 "폐기되었지만 적합"으로 허용하는 값.
fn conforming(el: &str, attr: &str, value: &str) -> bool {
    let v = value.trim().to_ascii_lowercase();
    match (el, attr) {
        ("script", "charset") => v == "utf-8",
        ("a", "name") => !v.is_empty(),
        ("script", "language") => v == "javascript",
        ("style", "type") => v == "text/css",
        ("img", "border") => v == "0",
        _ => false,
    }
}

fn push_sample(v: &mut Vec<String>, s: String) {
    if v.len() < 20 && !v.contains(&s) {
        v.push(s);
    }
}

pub fn extract(doc: &Html) -> SpecFacts {
    let table: HashMap<&str, &[&str]> = OBSOLETE_ATTRS.iter().copied().collect();
    let mut f = SpecFacts::default();
    for el in doc.select(&ALL) {
        let e = el.value();
        let name = e.name();
        for (a, v) in e.attrs() {
            let Some(elems) = table.get(a) else {
                continue;
            };
            if (elems.is_empty() || elems.contains(&name)) && !conforming(name, a, v) {
                f.obsolete_attrs += 1;
                push_sample(&mut f.obsolete_attr_samples, format!("{name}[{a}]"));
            }
        }
        let lower = |a: &str| e.attr(a).map(|v| v.trim().to_ascii_lowercase());
        let role = lower("role").unwrap_or_default();
        let input_type = lower("type").unwrap_or_default();
        if name == "search"
            || role.split_ascii_whitespace().next() == Some("search")
            || (name == "input" && input_type == "search")
        {
            f.site_search = true;
        }
        let label = lower("aria-label").unwrap_or_default();
        let class_id = format!(
            "{} {}",
            lower("class").unwrap_or_default(),
            lower("id").unwrap_or_default()
        );
        if ["breadcrumb", "현재 위치", "현재위치"]
            .iter()
            .any(|k| label.contains(k))
            || class_id.contains("breadcrumb")
        {
            f.breadcrumb = true;
        }
        if name == "abbr" {
            f.abbr_total += 1;
            if lower("title").is_none_or(|t| t.is_empty()) {
                f.abbr_without_title += 1;
            }
        }
        let navigates = |a: &str| {
            let v = lower(a).unwrap_or_default();
            ["location", "submit(", "window.open", ".href"]
                .iter()
                .any(|k| v.contains(k))
        };
        f.onchange_navigation += usize::from(navigates("onchange"));
        f.onfocus_navigation += usize::from(navigates("onfocus"));
        if name == "video" && e.attr("muted").is_none() {
            f.videos += 1;
            let described = el
                .children()
                .filter_map(scraper::ElementRef::wrap)
                .any(|t| {
                    t.value().name() == "track"
                        && t.value()
                            .attr("kind")
                            .is_some_and(|k| k.trim().eq_ignore_ascii_case("descriptions"))
                });
            f.videos_without_descriptions += usize::from(!described);
        }
        if name == "input" && input_type == "password" {
            f.password_fields += 1;
            let form = el
                .ancestors()
                .filter_map(scraper::ElementRef::wrap)
                .find(|a| a.value().name() == "form");
            let auto = lower("autocomplete").or_else(|| {
                form.and_then(|f| f.value().attr("autocomplete"))
                    .map(|v| v.trim().to_ascii_lowercase())
            });
            if auto.as_deref() == Some("off") {
                f.password_autocomplete_off += 1;
            }
        }
        if lower("style").is_some_and(|s| JUSTIFY.is_match(&s)) {
            f.inline_justify += 1;
        }
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(html: &str) -> SpecFacts {
        extract(&Html::parse_document(html))
    }

    #[test]
    fn obsolete_attributes_follow_the_spec_table() {
        let f = run(
            r##"<html><body bgcolor="#fff"><table cellpadding="0" summary="s"><tr><td align="center" nowrap>x</td></tr></table>
<p align="left">p</p><div contextmenu="m">d</div><img src="a.png" border="0" alt=""><img src="b.png" border="1" alt="">
<a name="top">t</a><a name="">e</a><script charset="utf-8"></script><script charset="euc-kr"></script>
<script language="JavaScript"></script><style type="text/css"></style><span align="left">허용 (span 은 대상 아님)</span>
<h3 align="center">h</h3></body></html>"##,
        );
        assert_eq!(f.obsolete_attrs, 11, "{:?}", f.obsolete_attr_samples);
        for s in [
            "body[bgcolor]",
            "table[cellpadding]",
            "table[summary]",
            "td[align]",
            "td[nowrap]",
            "p[align]",
            "div[contextmenu]",
            "img[border]",
            "a[name]",
            "script[charset]",
            "h3[align]",
        ] {
            assert!(f.obsolete_attr_samples.contains(&s.to_string()), "{s}");
        }
        let f = run("<html><body><p>ok</p></body></html>");
        assert_eq!((f.obsolete_attrs, f.obsolete_attr_samples.len()), (0, 0));
    }

    #[test]
    fn samples_are_capped_and_unique() {
        let html: String = (0..30)
            .map(|i| {
                format!(
                    "<p align=\"left\" id=\"p{i}\"></p><h{} align=\"left\"></h{}>",
                    i % 6 + 1,
                    i % 6 + 1
                )
            })
            .collect();
        let f = run(&html);
        assert_eq!(f.obsolete_attrs, 60);
        assert_eq!(f.obsolete_attr_samples.len(), 7);
        let many: String = OBSOLETE_ATTRS
            .iter()
            .filter(|(_, e)| e.is_empty())
            .map(|(a, _)| format!("<b {a}=\"x\"></b>"))
            .chain(["table", "td", "tr", "body", "hr", "iframe", "img", "object", "col", "ul", "ol", "dl", "br",
                "embed", "input", "legend", "caption", "thead", "pre", "li", "area", "head", "link", "form", "meta"]
                .iter()
                .map(|t| format!("<{t} align=\"x\" width=\"1\" type=\"x\" compact clear=\"all\" hspace=\"1\" border=\"2\" scheme=\"x\" accept=\"x\" profile=\"x\" rev=\"x\"></{t}>")))
            .collect();
        assert_eq!(run(&many).obsolete_attr_samples.len(), 20);
    }

    #[test]
    fn wcag_signals() {
        let f = run(
            r#"<html><body><nav aria-label="Breadcrumb"><ol><li>홈</li></ol></nav>
<form role="search"><input type="search"></form>
<abbr title="World Wide Web">WWW</abbr><abbr>HTML</abbr><abbr title=" ">CSS</abbr>
<select onchange="location.href=this.value"></select><select onchange="this.form.submit()"></select><select onchange="track()"></select>
<form autocomplete="off"><input type="password"></form><form><input type="password" autocomplete="off"><input type="PASSWORD" autocomplete="current-password"></form>
<p style="text-align: justify">양쪽 정렬</p><input onfocus="window.location='/x'">
<video src="a.mp4"><track kind="descriptions" src="d.vtt"></video><video src="b.mp4"><track kind="captions"></video><video muted></video><p style="text-align:left">왼쪽</p></body></html>"#,
        );
        assert!(f.site_search && f.breadcrumb);
        assert_eq!((f.abbr_total, f.abbr_without_title), (3, 2));
        assert_eq!((f.onchange_navigation, f.onfocus_navigation), (2, 1));
        assert_eq!((f.videos, f.videos_without_descriptions), (2, 1));
        assert_eq!((f.password_fields, f.password_autocomplete_off), (3, 2));
        assert_eq!(f.inline_justify, 1);

        let f = run(
            r#"<html><body><search><input></search><div class="site-breadcrumb"></div><input type="password"></body></html>"#,
        );
        assert!(f.site_search && f.breadcrumb);
        assert_eq!((f.password_fields, f.password_autocomplete_off), (1, 0));
        let f = run(
            r#"<html><body><div aria-label="현재 위치"></div><input type="text" role="searchbox"></body></html>"#,
        );
        assert!(f.breadcrumb && !f.site_search);
        let f = run("<html><body><p>x</p></body></html>");
        assert_eq!(f, SpecFacts::default());
    }
}
