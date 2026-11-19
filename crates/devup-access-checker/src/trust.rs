//! 신뢰·법적 정보 (개인정보처리방침, 연락처, SNS, 사업자 정보, 저작권 연도). 네트워크 접근 없음.

use std::sync::LazyLock;

use regex::Regex;
use scraper::{Html, Node, Selector};
use url::Url;

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("valid selector")
}

static ANCHOR: LazyLock<Selector> = LazyLock::new(|| sel("a[href]"));
static ADDRESS: LazyLock<Selector> = LazyLock::new(|| sel("address"));
static BODY: LazyLock<Selector> = LazyLock::new(|| sel("body"));
/// 링크 텍스트 기준 (주 판정). 단독 "Privacy" / "Terms" 같은 짧은 라벨도 허용.
static PRIVACY_LABEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(개인\s*정보\s*(?:처리|취급|보호)\s*방침|privacy\s*(?:policy|notice|statement|center|centre))").unwrap()
});
static TERMS_LABEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(이용\s*약관|서비스\s*약관|terms\s*(?:of\s*(?:use|service)|and\s*conditions|&\s*conditions))").unwrap()
});
static PRIVACY_BARE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:privacy|개인정보)$").unwrap());
static TERMS_BARE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^(?:terms|약관)$").unwrap());
/// href 기준 (텍스트가 없거나 아이콘 링크일 때 보조).
static PRIVACY_HREF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)privacy[-_]?(?:policy|notice|statement)").unwrap());
static TERMS_HREF: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)terms[-_]?(?:of[-_]?(?:use|service)|and[-_]?conditions)").unwrap()
});
static BIZ_NO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b\d{3}-\d{2}-\d{5}\b").unwrap());
static COPYRIGHT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:©|ⓒ|\(c\)|copyright)\s*(?:\d{4}\s*[-–~]\s*)?(\d{4})").unwrap()
});

/// (표시명, 호스트 접미사)
pub const SOCIAL_NETWORKS: &[(&str, &str)] = &[
    ("instagram", "instagram.com"),
    ("facebook", "facebook.com"),
    ("x", "x.com"),
    ("twitter", "twitter.com"),
    ("youtube", "youtube.com"),
    ("linkedin", "linkedin.com"),
    ("naver_blog", "blog.naver.com"),
    ("kakao_channel", "pf.kakao.com"),
    ("tiktok", "tiktok.com"),
    ("github", "github.com"),
    ("threads", "threads.net"),
    ("brunch", "brunch.co.kr"),
];

#[derive(Debug, Clone, Default)]
pub struct TrustFacts {
    pub privacy_policy: Option<String>,
    pub terms: Option<String>,
    pub tel: Vec<String>,
    pub mailto: Vec<String>,
    pub has_address_element: bool,
    pub social: Vec<&'static str>,
    /// 사업자등록번호 형식 (000-00-00000) 텍스트
    pub business_number: Option<String>,
    /// 저작권 표기의 마지막 연도
    pub copyright_year: Option<i32>,
}

fn body_text(doc: &Html) -> String {
    let Some(body) = doc.select(&BODY).next() else {
        return String::new();
    };
    let mut out = String::new();
    for node in body.descendants() {
        let Node::Text(t) = node.value() else {
            continue;
        };
        let hidden = node.ancestors().any(|a| {
            a.value()
                .as_element()
                .is_some_and(|e| matches!(e.name(), "script" | "style" | "template"))
        });
        if !hidden {
            out.push_str(t);
            out.push(' ');
        }
    }
    out
}

pub fn extract(doc: &Html, base: &Url) -> TrustFacts {
    let mut f = TrustFacts::default();
    let (mut privacy_score, mut terms_score) = (0.0f32, 0.0f32);
    for a in doc.select(&ANCHOR) {
        let href = a.value().attr("href").unwrap_or_default().trim();
        let text: String = a.text().collect();
        let label = format!(
            "{text} {}",
            a.value().attr("aria-label").unwrap_or_default()
        );
        let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
        let real_href = !(href.starts_with("javascript") || href == "#" || href.is_empty());
        // 점수: 라벨에 policy/방침 = 3, href 패턴 = 2, 단독 "Privacy"/"Terms" 라벨 = 1. 실제 URL 이면 +0.5
        let score = |strong: &Regex, bare: &Regex, href_re: &Regex| -> f32 {
            let base = if strong.is_match(&label) {
                3.0
            } else if href_re.is_match(href) {
                2.0
            } else if bare.is_match(&label) {
                1.0
            } else {
                return 0.0;
            };
            base + if real_href { 0.5 } else { 0.0 }
        };
        let ps = score(&PRIVACY_LABEL, &PRIVACY_BARE, &PRIVACY_HREF);
        if ps > privacy_score {
            privacy_score = ps;
            f.privacy_policy = Some(href.to_string());
        }
        let ts = score(&TERMS_LABEL, &TERMS_BARE, &TERMS_HREF);
        if ts > terms_score {
            terms_score = ts;
            f.terms = Some(href.to_string());
        }
        if let Some(t) = href.strip_prefix("tel:") {
            let t = t.trim().to_string();
            if !f.tel.contains(&t) {
                f.tel.push(t);
            }
        } else if let Some(m) = href.strip_prefix("mailto:") {
            let m = m.split('?').next().unwrap_or_default().trim().to_string();
            if !f.mailto.contains(&m) {
                f.mailto.push(m);
            }
        } else if let Ok(u) = base.join(href)
            && let Some(host) = u.host_str()
        {
            for (name, suffix) in SOCIAL_NETWORKS {
                let matches = host == *suffix || host.ends_with(&format!(".{suffix}"));
                if matches && !f.social.contains(name) {
                    f.social.push(name);
                }
            }
        }
    }
    f.has_address_element = doc.select(&ADDRESS).next().is_some();
    let text = body_text(doc);
    f.business_number = BIZ_NO.find(&text).map(|m| m.as_str().to_string());
    f.copyright_year = COPYRIGHT
        .captures_iter(&text)
        .filter_map(|c| c[1].parse().ok())
        .max();
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footer_signals() {
        let html = r#"<html><body><footer>
            <a href="/policy/privacy"><b>개인정보처리방침</b></a><a href="/terms">이용약관</a>
            <a href="tel:02-1234-5678">전화</a><a href="mailto:hello@devfive.kr?subject=hi">메일</a>
            <a href="https://www.instagram.com/devfive">인스타</a><a href="https://blog.naver.com/devfive">블로그</a>
            <address>서울시</address><p>사업자등록번호 123-45-67890</p><p>© 2019-2025 DevFive. All rights reserved.</p>
        </footer></body></html>"#;
        let f = extract(
            &Html::parse_document(html),
            &Url::parse("https://devfive.kr/").unwrap(),
        );
        assert_eq!(f.privacy_policy.as_deref(), Some("/policy/privacy"));
        assert_eq!(f.terms.as_deref(), Some("/terms"));
        assert_eq!(f.tel, vec!["02-1234-5678"]);
        assert_eq!(f.mailto, vec!["hello@devfive.kr"]);
        assert_eq!(f.social, vec!["instagram", "naver_blog"]);
        assert!(f.has_address_element);
        assert_eq!(f.business_number.as_deref(), Some("123-45-67890"));
        assert_eq!(f.copyright_year, Some(2025));

        // 오탐 방지: 카테고리·문서 페이지는 개인정보처리방침이 아니다
        let f = extract(
            &Html::parse_document(r#"<body><a href="/category/privacy/">Privacy</a></body>"#),
            &Url::parse("https://a.com/").unwrap(),
        );
        assert_eq!(
            f.privacy_policy.as_deref(),
            Some("/category/privacy/"),
            "bare 'Privacy' accepted when nothing better"
        );
        let f = extract(
            &Html::parse_document(
                r#"<body><a href="/category/privacy/">Privacy</a><a href="https://legal.x.com/privacy">Privacy Policy</a></body>"#,
            ),
            &Url::parse("https://a.com/").unwrap(),
        );
        assert_eq!(
            f.privacy_policy.as_deref(),
            Some("https://legal.x.com/privacy"),
            "stronger match wins"
        );
        let f = extract(
            &Html::parse_document(
                r#"<body><a href="/docs/Web/Privacy">Privacy on the web</a><a href="/category/privacy/">Privacy news</a>
                <a href="javascript:;">개인정보 처리방침</a><a href="/policy/privacy">개인정보처리방침</a>
                <a href="/legal/terms-of-service"><svg></svg></a></body>"#,
            ),
            &Url::parse("https://a.com/").unwrap(),
        );
        assert_eq!(f.privacy_policy.as_deref(), Some("/policy/privacy"));
        assert_eq!(f.terms.as_deref(), Some("/legal/terms-of-service"));
        let f = extract(
            &Html::parse_document("<body><p>COPYRIGHT ⓒ 2024 데브파이브</p></body>"),
            &Url::parse("https://devfive.kr/").unwrap(),
        );
        assert_eq!(f.copyright_year, Some(2024));
    }
}
