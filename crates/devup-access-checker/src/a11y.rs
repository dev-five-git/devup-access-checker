//! 웹 접근성(KWCAG 2.2 / WCAG) 기본 항목 중 정적 HTML 만으로 판별 가능한 사실.
//!
//! 색 대비, 키보드 초점 이동, 실제 낭독 순서처럼 렌더링이 필요한 항목은 다루지 않는다.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use scraper::{ElementRef, Html, Selector};

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("valid selector")
}

static CONTROLS: LazyLock<Selector> = LazyLock::new(|| sel("input, select, textarea"));
static LABEL_FOR: LazyLock<Selector> = LazyLock::new(|| sel("label[for]"));
static BUTTONS: LazyLock<Selector> = LazyLock::new(|| {
    sel(
        "button, [role=button], input[type=submit], input[type=button], input[type=reset], input[type=image]",
    )
});
static LINKS: LazyLock<Selector> = LazyLock::new(|| sel("a[href]"));
static BODY_LINKS: LazyLock<Selector> = LazyLock::new(|| sel("body a[href]"));
static IDS: LazyLock<Selector> = LazyLock::new(|| sel("[id]"));
static IFRAMES: LazyLock<Selector> = LazyLock::new(|| sel("iframe"));
static TABLES: LazyLock<Selector> = LazyLock::new(|| sel("table"));
static TH: LazyLock<Selector> = LazyLock::new(|| sel("th, [scope]"));
static AUTOPLAY: LazyLock<Selector> = LazyLock::new(|| sel("video[autoplay], audio[autoplay]"));
static TABINDEX: LazyLock<Selector> = LazyLock::new(|| sel("[tabindex]"));
/// WHATWG HTML 명세 "Obsolete features" 의 폐기 요소
static OBSOLETE: LazyLock<Selector> = LazyLock::new(|| {
    sel(
        "acronym, applet, basefont, bgsound, big, blink, center, dir, font, frame, frameset, \
         isindex, keygen, listing, marquee, menuitem, multicol, nextid, nobr, noembed, noframes, \
         plaintext, rb, rtc, spacer, strike, tt, xmp",
    )
});
static MAIN: LazyLock<Selector> = LazyLock::new(|| sel("main, [role=main]"));
static NAV: LazyLock<Selector> = LazyLock::new(|| sel("nav, [role=navigation]"));
static HEADER: LazyLock<Selector> = LazyLock::new(|| sel("header, [role=banner]"));
static FOOTER: LazyLock<Selector> = LazyLock::new(|| sel("footer, [role=contentinfo]"));
static NAMED_IMG: LazyLock<Selector> =
    LazyLock::new(|| sel("img[alt], svg[aria-label], svg title, [role=img][aria-label]"));
static TARGET_BLANK: LazyLock<Selector> = LazyLock::new(|| sel("a[target=_blank]"));

#[derive(Debug, Clone, Default)]
pub struct A11yFacts {
    pub form_controls: usize,
    /// 레이블 없는 입력 요소 (예: `input[name=q]`)
    pub unlabeled_controls: Vec<String>,
    pub buttons: usize,
    pub unnamed_buttons: usize,
    pub links: usize,
    /// 접근 가능한 이름이 없는 링크
    pub empty_links: usize,
    pub has_main: bool,
    pub has_nav: bool,
    pub has_header: bool,
    pub has_footer: bool,
    /// 본문 앞쪽의 건너뛰기 링크 href
    pub skip_link: Option<String>,
    pub iframes: usize,
    pub iframes_without_title: usize,
    pub duplicate_ids: Vec<String>,
    pub tables: usize,
    pub tables_without_headers: usize,
    pub autoplay_media: usize,
    pub positive_tabindex: usize,
    /// 폐기된 표현 요소 (marquee, blink, font, center) 개수
    pub obsolete_elements: usize,
    pub new_window_links: usize,
    /// 목적을 알 수 없는 링크 텍스트 ("더보기", "click here" 등)
    pub vague_link_texts: Vec<String>,
    /// alt 가 파일명처럼 보이는 이미지
    pub filename_alts: Vec<String>,
    /// 125자를 넘는 alt 수
    pub long_alts: usize,
    /// 존재하지 않는 id 를 가리키는 참조 (`for`, `aria-labelledby` 등)
    pub broken_id_refs: Vec<String>,
    /// aria-hidden="true" 인데 초점을 받을 수 있는(또는 그런 자손이 있는) 요소 수
    pub focusable_aria_hidden: usize,
    pub headings: usize,
    pub empty_headings: usize,
    /// 개인정보 입력란 중 autocomplete 없는 요소
    pub autocomplete_missing: Vec<String>,
    /// `<style>`·style 속성 안 `outline: none|0` 횟수
    pub outline_removed: usize,
    /// `:focus-visible` 규칙 존재
    pub has_focus_visible: bool,
}

static HEADINGS: LazyLock<Selector> = LazyLock::new(|| sel("h1, h2, h3, h4, h5, h6"));
static IMG_ALT: LazyLock<Selector> = LazyLock::new(|| sel("img[alt]"));
static ID_REFS: LazyLock<Selector> = LazyLock::new(|| {
    sel("label[for], [aria-labelledby], [aria-describedby], [aria-controls], [headers]")
});
static ARIA_HIDDEN: LazyLock<Selector> = LazyLock::new(|| sel("[aria-hidden=true]"));
static FOCUSABLE: LazyLock<Selector> = LazyLock::new(|| {
    sel(
        "a[href], button, input:not([type=hidden]), select, textarea, [tabindex]:not([tabindex='-1'])",
    )
});
static PERSONAL_INPUTS: LazyLock<Selector> = LazyLock::new(|| sel("input"));
static STYLE: LazyLock<Selector> = LazyLock::new(|| sel("style"));
static STYLE_ATTR: LazyLock<Selector> = LazyLock::new(|| sel("[style]"));
static OUTLINE_NONE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?i)outline\s*:\s*(?:none|0(?:px)?)\b").unwrap());
static FILENAME_ALT: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?i)(?:\.(?:jpe?g|png|gif|webp|avif|svg|bmp)$|^(?:img|image|dsc|screenshot|photo)[_\- ]?\d+)").unwrap()
});
static PERSONAL_HINT: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?i)(?:name|email|e-mail|phone|tel|mobile|address|zip|postal|이름|성명|이메일|전화|휴대|연락처|주소|우편)").unwrap()
});

/// 목적을 알 수 없는 링크 텍스트 (소문자, 공백 제거 비교).
const VAGUE_LINK_TEXTS: &[&str] = &[
    "더보기",
    "더보기+",
    "자세히",
    "자세히보기",
    "바로가기",
    "여기",
    "클릭",
    "링크",
    "보기",
    "clickhere",
    "here",
    "readmore",
    "more",
    "learnmore",
    "link",
    "click",
];

fn non_empty(el: &ElementRef, name: &str) -> bool {
    el.value().attr(name).is_some_and(|v| !v.trim().is_empty())
}

/// 요소의 접근 가능한 이름이 있는지 (텍스트, aria-label, aria-labelledby, title, 이름 있는 이미지).
fn has_accessible_name(el: &ElementRef) -> bool {
    non_empty(el, "aria-label")
        || non_empty(el, "aria-labelledby")
        || non_empty(el, "title")
        || el.text().any(|t| !t.trim().is_empty())
        || el.select(&NAMED_IMG).any(|i| {
            i.value().name() != "img" || i.value().attr("alt").is_some_and(|a| !a.trim().is_empty())
        })
}

fn describe(el: &ElementRef) -> String {
    let v = el.value();
    let key = ["id", "name", "type"]
        .iter()
        .find_map(|k| {
            v.attr(k)
                .filter(|s| !s.is_empty())
                .map(|s| format!("[{k}={s}]"))
        })
        .unwrap_or_default();
    format!("{}{key}", v.name())
}

pub fn extract(doc: &Html) -> A11yFacts {
    let mut f = A11yFacts::default();

    // ===== 입력 요소 레이블 =====
    let label_targets: HashSet<&str> = doc
        .select(&LABEL_FOR)
        .filter_map(|l| l.value().attr("for"))
        .collect();
    for c in doc.select(&CONTROLS) {
        let ty = c
            .value()
            .attr("type")
            .unwrap_or("text")
            .trim()
            .to_ascii_lowercase();
        if c.value().name() == "input"
            && matches!(
                ty.as_str(),
                "hidden" | "submit" | "button" | "reset" | "image"
            )
        {
            continue;
        }
        f.form_controls += 1;
        let by_for = c
            .value()
            .attr("id")
            .is_some_and(|id| label_targets.contains(id));
        let wrapped = c
            .ancestors()
            .any(|a| a.value().as_element().is_some_and(|e| e.name() == "label"));
        let aria = non_empty(&c, "aria-label")
            || non_empty(&c, "aria-labelledby")
            || non_empty(&c, "title");
        if !(by_for || wrapped || aria) {
            f.unlabeled_controls.push(describe(&c));
        }
    }

    // ===== 버튼 이름 =====
    for b in doc.select(&BUTTONS) {
        f.buttons += 1;
        let v = b.value();
        let named = if v.name() == "input" {
            match v
                .attr("type")
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str()
            {
                // submit/reset 은 브라우저 기본 이름이 있다
                "submit" | "reset" => true,
                "image" => {
                    non_empty(&b, "alt") || non_empty(&b, "aria-label") || non_empty(&b, "title")
                }
                _ => {
                    non_empty(&b, "value") || non_empty(&b, "aria-label") || non_empty(&b, "title")
                }
            }
        } else {
            has_accessible_name(&b)
        };
        if !named {
            f.unnamed_buttons += 1;
        }
    }

    // ===== 링크 =====
    for a in doc.select(&LINKS) {
        f.links += 1;
        if !has_accessible_name(&a) {
            f.empty_links += 1;
        }
    }
    f.new_window_links = doc.select(&TARGET_BLANK).count();

    // ===== 랜드마크 =====
    f.has_main = doc.select(&MAIN).next().is_some();
    f.has_nav = doc.select(&NAV).next().is_some();
    f.has_header = doc.select(&HEADER).next().is_some();
    f.has_footer = doc.select(&FOOTER).next().is_some();

    // ===== id 중복 / 건너뛰기 링크 =====
    let mut id_count: HashMap<&str, usize> = HashMap::new();
    for e in doc.select(&IDS) {
        if let Some(id) = e
            .value()
            .attr("id")
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            *id_count.entry(id).or_default() += 1;
        }
    }
    let mut dups: Vec<String> = id_count
        .iter()
        .filter(|(_, n)| **n > 1)
        .map(|(id, _)| id.to_string())
        .collect();
    dups.sort();
    f.duplicate_ids = dups;
    f.skip_link = doc
        .select(&BODY_LINKS)
        .take(3)
        .filter_map(|a| a.value().attr("href").map(str::trim))
        .find(|h| h.len() > 1 && h.starts_with('#') && id_count.contains_key(&h[1..]))
        .map(str::to_string);

    // ===== 기타 =====
    for i in doc.select(&IFRAMES) {
        f.iframes += 1;
        if !non_empty(&i, "title") && !non_empty(&i, "aria-label") {
            f.iframes_without_title += 1;
        }
    }
    for t in doc.select(&TABLES) {
        let presentational = t
            .value()
            .attr("role")
            .is_some_and(|r| r == "presentation" || r == "none");
        if presentational {
            continue;
        }
        f.tables += 1;
        if t.select(&TH).next().is_none() {
            f.tables_without_headers += 1;
        }
    }
    f.autoplay_media = doc.select(&AUTOPLAY).count();
    f.positive_tabindex = doc
        .select(&TABINDEX)
        .filter(|e| {
            e.value()
                .attr("tabindex")
                .and_then(|v| v.trim().parse::<i32>().ok())
                .is_some_and(|n| n > 0)
        })
        .count();
    f.obsolete_elements = doc.select(&OBSOLETE).count();
    // ===== 링크 목적 =====
    for a in doc.select(&LINKS) {
        let text: String = a.text().collect::<String>().split_whitespace().collect();
        let key = text.to_lowercase();
        if VAGUE_LINK_TEXTS.contains(&key.as_str())
            && !non_empty(&a, "aria-label")
            && !non_empty(&a, "title")
        {
            let t = a
                .text()
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if !f.vague_link_texts.contains(&t) {
                f.vague_link_texts.push(t);
            }
        }
    }

    // ===== 대체 텍스트 품질 =====
    for img in doc.select(&IMG_ALT) {
        let alt = img.value().attr("alt").unwrap_or_default().trim();
        if FILENAME_ALT.is_match(alt) {
            f.filename_alts.push(alt.to_string());
        }
        if alt.chars().count() > 125 {
            f.long_alts += 1;
        }
    }

    // ===== id 참조 / aria-hidden =====
    for el in doc.select(&ID_REFS) {
        for attr_name in [
            "for",
            "aria-labelledby",
            "aria-describedby",
            "aria-controls",
            "headers",
        ] {
            if attr_name == "for" && el.value().name() != "label" {
                continue;
            }
            for id in el
                .value()
                .attr(attr_name)
                .unwrap_or_default()
                .split_ascii_whitespace()
            {
                if !id_count.contains_key(id) {
                    let s = format!("{attr_name}={id}");
                    if !f.broken_id_refs.contains(&s) {
                        f.broken_id_refs.push(s);
                    }
                }
            }
        }
    }
    f.focusable_aria_hidden = doc
        .select(&ARIA_HIDDEN)
        .filter(|e| {
            let self_focusable = FOCUSABLE.matches(e);
            self_focusable || e.select(&FOCUSABLE).next().is_some()
        })
        .count();

    // ===== 빈 헤딩 =====
    for h in doc.select(&HEADINGS) {
        f.headings += 1;
        if !has_accessible_name(&h) {
            f.empty_headings += 1;
        }
    }

    // ===== 개인정보 입력 autocomplete =====
    for i in doc.select(&PERSONAL_INPUTS) {
        let v = i.value();
        let ty = v.attr("type").unwrap_or("text").to_ascii_lowercase();
        if matches!(
            ty.as_str(),
            "hidden"
                | "submit"
                | "button"
                | "reset"
                | "image"
                | "checkbox"
                | "radio"
                | "file"
                | "password"
                | "search"
        ) {
            continue;
        }
        let hint = format!(
            "{} {} {}",
            v.attr("name").unwrap_or_default(),
            v.attr("id").unwrap_or_default(),
            v.attr("placeholder").unwrap_or_default()
        );
        let personal = matches!(ty.as_str(), "email" | "tel") || PERSONAL_HINT.is_match(&hint);
        if personal && !non_empty(&i, "autocomplete") {
            f.autocomplete_missing.push(describe(&i));
        }
    }

    // ===== 초점 표시 제거 =====
    let mut css = String::new();
    for s in doc.select(&STYLE) {
        css.extend(s.text());
        css.push('\n');
    }
    for e in doc.select(&STYLE_ATTR) {
        css.push_str(e.value().attr("style").unwrap_or_default());
        css.push('\n');
    }
    f.outline_removed = OUTLINE_NONE.find_iter(&css).count();
    f.has_focus_visible = css.contains(":focus-visible");

    f
}

/// html lang 이 BCP 47 형태인지 (예: ko, en-US, zh-Hant-TW).
pub fn valid_lang(lang: &str) -> bool {
    static RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^[a-zA-Z]{2,3}(?:-[a-zA-Z0-9]{2,8})*$").unwrap());
    RE.is_match(lang.trim())
}

/// viewport 메타가 확대를 막는지 (`user-scalable=no` 또는 `maximum-scale` < 2).
pub fn zoom_disabled(viewport: &str) -> bool {
    let v = viewport.to_ascii_lowercase().replace(' ', "");
    v.split([',', ';']).any(|kv| match kv.split_once('=') {
        Some(("user-scalable", val)) => val == "no" || val == "0",
        Some(("maximum-scale", val)) => val.parse::<f32>().is_ok_and(|n| n < 2.0),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(body: &str) -> A11yFacts {
        extract(&Html::parse_document(&format!(
            "<html><body>{body}</body></html>"
        )))
    }

    #[test]
    fn forms_and_buttons() {
        let f = run(r#"
            <label for="a">이름</label><input id="a">
            <label>이메일 <input name="email"></label>
            <input name="q" placeholder="검색">
            <input type="hidden" name="t"><select aria-label="정렬"></select><textarea name="memo"></textarea>
            <button>확인</button><button><svg aria-label="닫기"></svg></button><button><i class="icon"></i></button>
            <input type="image" src="x.png"><input type="submit">"#);
        assert_eq!(f.form_controls, 5);
        assert_eq!(
            f.unlabeled_controls,
            vec!["input[name=q]", "textarea[name=memo]"]
        );
        assert_eq!((f.buttons, f.unnamed_buttons), (5, 2));
    }

    #[test]
    fn links_landmarks_ids() {
        let f = run(
            r##"<a href="#content">본문 바로가기</a><header></header><nav></nav>
            <main id="content"><a href="/x"><img src="a.png" alt=""></a><a href="/y"><img src="b.png" alt="로고"></a>
            <a href="/z" target="_blank">새 창</a><div id="d"></div><div id="d"></div></main>"##,
        );
        assert_eq!((f.links, f.empty_links), (4, 1));
        assert!(f.has_main && f.has_nav && f.has_header && !f.has_footer);
        assert_eq!(f.skip_link.as_deref(), Some("#content"));
        assert_eq!(f.duplicate_ids, vec!["d"]);
        assert_eq!(f.new_window_links, 1);
    }

    #[test]
    fn misc_elements() {
        let f = run(
            r#"<iframe src="a"></iframe><iframe src="b" title="지도"></iframe>
            <table><tr><td>1</td></tr></table><table><tr><th>h</th></tr></table><table role="presentation"></table>
            <video autoplay></video><div tabindex="1"></div><div tabindex="0"></div><center>x</center>"#,
        );
        assert_eq!((f.iframes, f.iframes_without_title), (2, 1));
        assert_eq!((f.tables, f.tables_without_headers), (2, 1));
        assert_eq!(
            (f.autoplay_media, f.positive_tabindex, f.obsolete_elements),
            (1, 1, 1)
        );
    }

    #[test]
    fn deeper_checks() {
        let f = run(r##"<style>a:focus{outline:none} .x:focus-visible{}</style>
            <a href="/a">더보기</a><a href="/b">자세히 보기</a><a href="/c" aria-label="공지 더보기">더보기</a>
            <img src="a.jpg" alt="IMG_1234.jpg"><img src="b.jpg" alt="DSC0001">
            <label for="nope">x</label><div aria-labelledby="ghost"></div>
            <div aria-hidden="true"><a href="/x">x</a></div><span aria-hidden="true">deco</span>
            <h2></h2><h3><img src="a.png" alt="제목"></h3>
            <input name="user_email"><input type="tel" autocomplete="tel"><input name="search_q">
            <div style="outline: 0"></div>"##);
        assert_eq!(f.vague_link_texts, vec!["더보기", "자세히 보기"]);
        assert_eq!(f.filename_alts, vec!["IMG_1234.jpg", "DSC0001"]);
        assert_eq!(f.broken_id_refs, vec!["for=nope", "aria-labelledby=ghost"]);
        assert_eq!(f.focusable_aria_hidden, 1);
        assert_eq!((f.headings, f.empty_headings), (2, 1));
        assert_eq!(f.autocomplete_missing, vec!["input[name=user_email]"]);
        assert_eq!(f.outline_removed, 2);
        assert!(f.has_focus_visible);
        assert!(
            valid_lang("ko")
                && valid_lang("en-US")
                && !valid_lang("korean")
                && !valid_lang("ko_KR")
        );
    }

    #[test]
    fn viewport_zoom() {
        assert!(zoom_disabled("width=device-width, user-scalable=no"));
        assert!(zoom_disabled("width=device-width, maximum-scale=1"));
        assert!(!zoom_disabled("width=device-width, initial-scale=1"));
    }
}
