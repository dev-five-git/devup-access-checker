//! 원문 HTML 문자열에 대한 패턴 기반 탐지 (분석 도구, 접근성 인증마크, 기본 404 페이지).
//! devfive-center `seo_analysis.rs` 의 탐지 로직을 그대로 옮겼다.

use std::sync::LazyLock;

use regex::Regex;

fn re(p: &str) -> Regex {
    Regex::new(p).expect("valid regex")
}

static GOOGLE_PATTERNS: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    [
        ("GA4", r#"(?i)["'`]G-[A-Z0-9]{8,}["'`]"#),
        (
            "GA4",
            r#"(?i)googletagmanager\.com/gtag/js\?id=G-[A-Z0-9]+"#,
        ),
        (
            "GA4",
            r#"(?i)gtag\s*\(\s*["']config["']\s*,\s*["']G-[A-Z0-9]+["']"#,
        ),
        ("UA", r#"(?i)["'`]UA-[0-9]+-[0-9]+["'`]"#),
        ("UA", r#"(?i)googletagmanager\.com/gtag/js\?id=UA-[0-9-]+"#),
        (
            "UA",
            r#"(?i)gtag\s*\(\s*["']config["']\s*,\s*["']UA-[0-9-]+["']"#,
        ),
        ("UA", r#"(?i)google-analytics\.com/(?:ga|analytics)\.js"#),
        ("GTM", r#"(?i)["'`]GTM-[A-Z0-9]+["'`]"#),
        ("GTM", r#"(?i)googletagmanager\.com/gtm\.js"#),
    ]
    .into_iter()
    .map(|(n, p)| (n, re(p)))
    .collect()
});

/// Google Analytics / GA4 / GTM 추적 코드 종류 (발견 순서, 중복 제거).
pub fn google_tracking(html: &str) -> Vec<&'static str> {
    let mut found = Vec::new();
    for (name, pat) in GOOGLE_PATTERNS.iter() {
        if !found.contains(name) && pat.is_match(html) {
            found.push(*name);
        }
    }
    found
}

static NAVER: LazyLock<Regex> = LazyLock::new(|| re(r#"(?i)(?:/wcslog\.js|wcs_add|wcs\.inflow)"#));

/// 네이버 애널리틱스 (wcslog.js) 추적 코드 존재 여부.
pub fn naver_analytics(html: &str) -> bool {
    NAVER.is_match(html)
}

/// 프레임워크/웹서버 기본 404 페이지 마커 존재 여부.
pub fn is_framework_default_404(body: &str) -> bool {
    const MARKERS: &[&str] = &[
        "this page could not be found",
        "<title>404</title>",
        "default backend - 404",
        "404 not found</title>",
        "<center><h1>404 not found</h1></center>",
        "<h1>not found</h1>",
    ];
    let lower = body.to_lowercase();
    MARKERS.iter().any(|m| lower.contains(m))
}

static FRAMEWORKS: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    [
        (
            "Next.js",
            r#"(?:/_next/static/|__NEXT_DATA__|self\.__next_f)"#,
        ),
        ("Nuxt", r#"(?:__NUXT__|/_nuxt/)"#),
        ("Gatsby", r#"___gatsby"#),
        ("SvelteKit", r#"__sveltekit"#),
        ("Astro", r#"(?:<astro-island|/_astro/)"#),
        ("Remix", r#"__remixContext"#),
        ("Angular", r#"ng-version="#),
        ("Vue", r#"(?:data-v-app|data-server-rendered)"#),
        ("WordPress", r#"/wp-(?:content|includes)/"#),
        ("Wix", r#"static\.wixstatic\.com"#),
        ("Imweb", r#"(?:imweb\.me|cdn\.imweb)"#),
        ("Cafe24", r#"cafe24"#),
        ("Shopify", r#"cdn\.shopify\.com"#),
    ]
    .into_iter()
    .map(|(n, p)| (n, re(p)))
    .collect()
});

/// HTML 흔적으로 추정한 프레임워크 / CMS (발견 순서).
pub fn frameworks(html: &str) -> Vec<&'static str> {
    FRAMEWORKS
        .iter()
        .filter(|(_, r)| r.is_match(html))
        .map(|(n, _)| *n)
        .collect()
}

/// 봇 차단·챌린지 페이지 표식: (공급자, 소문자 표식).
/// 정상 페이지에도 삽입되는 스크립트 경로(예: Cloudflare `/cdn-cgi/challenge-platform/`)는 쓰지 않는다.
const CHALLENGE_MARKERS: &[(&str, &str)] = &[
    ("Cloudflare", "<title>just a moment...</title>"),
    ("Cloudflare", "cf_chl_opt"),
    ("Cloudflare", "attention required! | cloudflare"),
    ("Cloudflare", "verifying your connection"),
    ("F5", "window[\"bobcmn\"]"),
    ("Akamai", "pardon our interruption"),
    ("Akamai", "<title>access denied</title>"),
    ("Imperva", "_incapsula_resource"),
    ("PerimeterX", "px-captcha"),
    ("DataDome", "captcha-delivery.com"),
    ("Kasada", "kpsdk"),
    ("AWS WAF", "awswaf"),
];

/// 봇 차단·챌린지 페이지면 공급자 이름. 본문이 큰(200KB 초과) 정상 페이지는 검사하지 않는다.
pub fn bot_challenge(body: &str) -> Option<&'static str> {
    if body.len() > 200_000 {
        return None;
    }
    let lower = body.to_lowercase();
    CHALLENGE_MARKERS
        .iter()
        .find(|(_, m)| lower.contains(m))
        .map(|(v, _)| *v)
}

/// 작은 HTML 안의 단순 JavaScript 리다이렉트 대상 (`location.href = "..."`, `location.replace("...")`).
/// 100KB 를 넘는 페이지는 실제 콘텐츠 페이지로 보고 검사하지 않는다.
pub fn js_redirect(html: &str) -> Option<String> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?:(?:window|document|top|self)\.)?location(?:\.href)?\s*=\s*["']([^"']+)["']|location\.(?:replace|assign)\(\s*["']([^"']+)["']\s*\)"#,
        )
        .unwrap()
    });
    if html.len() > 100_000 {
        return None;
    }
    let c = RE.captures(html)?;
    c.get(1)
        .or_else(|| c.get(2))
        .map(|m| m.as_str().trim().to_string())
        .filter(|u| !u.is_empty() && !u.starts_with('#'))
}

/// `<meta http-equiv="refresh" content="0; URL=/x">` 해석 → (지연 초, URL).
pub fn parse_refresh(content: &str) -> Option<(f64, String)> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?i)^\s*(\d+(?:\.\d+)?)\s*(?:[;,]\s*(?:url\s*=\s*)?['"]?([^'"]+?)['"]?\s*)?$"#,
        )
        .unwrap()
    });
    let c = RE.captures(content)?;
    let delay = c[1].parse().ok()?;
    let url = c.get(2)?.as_str().trim().to_string();
    (!url.is_empty()).then_some((delay, url))
}

// ===== 정보통신접근성 품질인증 마크 =====
// 발급기관 사이트에 요청을 보내지 않는다. 대상 페이지 HTML 안의 링크/이미지 속성만 읽는다.

static CERT_ISSUERS: LazyLock<Vec<(Regex, &'static str, &'static str)>> = LazyLock::new(|| {
    [
        (
            r#"(?i)href\s*=\s*["'][^"']*(?://|\.)wa\.or\.kr"#,
            "한국정보접근성인증평가원(WA)",
            "https://www.wa.or.kr/board/list.asp?BoardID=0006",
        ),
        (
            r#"(?i)href\s*=\s*["'][^"']*(?://|\.)webwatch\.or\.kr"#,
            "웹와치",
            "http://www.webwatch.or.kr/certification/",
        ),
        (
            r#"(?i)href\s*=\s*["'][^"']*(?://|\.)wamark\.kr"#,
            "웹와치",
            "http://www.webwatch.or.kr/certification/",
        ),
        (
            r#"(?i)href\s*=\s*["'][^"']*(?://|\.)kwacc\.or\.kr"#,
            "한국디지털접근성진흥원",
            "http://www.kwacc.or.kr/CertificationSite/WA/List",
        ),
        (
            r#"(?i)href\s*=\s*["'][^"']*(?://|\.)kdaa\.or\.kr"#,
            "한국디지털접근성진흥원",
            "http://www.kwacc.or.kr/CertificationSite/WA/List",
        ),
    ]
    .into_iter()
    .map(|(p, n, u)| (re(p), n, u))
    .collect()
});

static CERT_TEXT: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"(?i)웹\s*접근성",
        r"(?i)정보통신\s*접근성",
        r"(?i)접근성\s*품질인증",
        r"(?i)접근성\s*인증",
        r"(?i)web\s*accessibility",
    ]
    .into_iter()
    .map(re)
    .collect()
});

static CERT_FILENAME: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"(?i)wa[_\-]?mark[^a-z]",
        r"(?i)mark[_\-]?wa[^a-z]",
        r"(?i)web[_\-]?accessibility",
        r"(?i)accessibility[_\-]?mark[^a-z]",
        r"(?i)a11y[_\-]?mark[^a-z]",
        r"(?i)%EC%9B%B9%EC%A0%91%EA%B7%BC%EC%84%B1",
    ]
    .into_iter()
    .map(re)
    .collect()
});

static IMG_TAG: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)<img\b[^>]*>"));

/// 마크 부착 판단 근거로 수집한 신호.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CertMarkSignals {
    /// 발급기관 도메인 링크에서 확인된 기관명
    pub issuer: Option<&'static str>,
    /// 해당 기관의 인증현황 조회 URL
    pub lookup_url: Option<&'static str>,
    /// `<img>` alt/title 에 인증 문구가 있음
    pub text_signal: bool,
    /// `<img>` src 가 마크 이미지 파일명으로 보임
    pub filename_signal: bool,
}

/// 태그에서 지정한 속성 값을 모은다. 값 끝에 공백을 붙여 `[^a-z]` 로 끝나는 패턴이 매칭되게 한다.
fn attr_values(tag: &str, names: &[&str]) -> String {
    let mut out = String::new();
    for name in names {
        let pat = format!(r#"(?i)\s{}\s*=\s*["']([^"']*)["']"#, regex::escape(name));
        if let Some(c) = Regex::new(&pat).ok().and_then(|r| r.captures(tag)) {
            out.push_str(&c[1]);
            out.push(' ');
        }
    }
    out
}

pub fn cert_mark(html: &str) -> CertMarkSignals {
    let mut s = CertMarkSignals::default();
    if let Some((_, issuer, lookup)) = CERT_ISSUERS.iter().find(|(r, _, _)| r.is_match(html)) {
        s.issuer = Some(issuer);
        s.lookup_url = Some(lookup);
    }
    for tag in IMG_TAG.find_iter(html) {
        let tag = tag.as_str();
        if !s.text_signal {
            let text = attr_values(tag, &["alt", "title"]);
            s.text_signal = CERT_TEXT.iter().any(|r| r.is_match(&text));
        }
        if !s.filename_signal {
            let src = attr_values(tag, &["src", "data-src", "srcset"]);
            s.filename_signal = CERT_FILENAME.iter().any(|r| r.is_match(&src));
        }
        if s.text_signal && s.filename_signal {
            break;
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_google_kinds() {
        let html = r#"<script src="https://www.googletagmanager.com/gtag/js?id=G-ABCDEFGH12"></script>
            <script>(function(w,d,s,l,i){})(window,document,'script','dataLayer','GTM-XYZ12');</script>"#;
        assert_eq!(google_tracking(html), vec!["GA4", "GTM"]);
        assert!(google_tracking("<html></html>").is_empty());
    }

    #[test]
    fn detects_frameworks() {
        assert_eq!(
            frameworks(
                r#"<script src="/_next/static/chunks/a.js"></script><link href="/wp-content/x.css">"#
            ),
            vec!["Next.js", "WordPress"]
        );
        assert!(frameworks("<p>plain</p>").is_empty());
    }

    #[test]
    fn detects_challenges_and_refresh() {
        assert_eq!(
            bot_challenge("<title>Just a moment...</title><script>window._cf_chl_opt={}</script>"),
            Some("Cloudflare")
        );
        assert_eq!(
            bot_challenge(r#"<script>window["bobcmn"] = "1";</script>"#),
            Some("F5")
        );
        assert_eq!(
            bot_challenge(
                r#"<script src="/cdn-cgi/challenge-platform/scripts/jsd/main.js"></script><h1>Shop</h1>"#
            ),
            None
        );
        assert_eq!(
            parse_refresh("0; URL=/portal/main"),
            Some((0.0, "/portal/main".into()))
        );
        assert_eq!(
            parse_refresh("5;url='https://a.com/x'"),
            Some((5.0, "https://a.com/x".into()))
        );
        assert_eq!(parse_refresh("360"), None);
        assert_eq!(
            js_redirect(r#"<script>window.location.href = "index.jsp";</script>"#).as_deref(),
            Some("index.jsp")
        );
        assert_eq!(
            js_redirect(r#"<script>location.replace('/ko/')</script>"#).as_deref(),
            Some("/ko/")
        );
        assert_eq!(
            js_redirect(r##"<script>if(x){location.hash = "#top"}</script>"##),
            None
        );
    }

    #[test]
    fn detects_naver() {
        assert!(naver_analytics(
            r#"<script src="//wcs.pstatic.net/wcslog.js"></script>"#
        ));
        assert!(!naver_analytics("<p>naver</p>"));
    }

    #[test]
    fn detects_default_404() {
        assert!(is_framework_default_404(
            "<h1>404</h1><h2>This page could not be found.</h2>"
        ));
        assert!(!is_framework_default_404("<h1>길을 잃으셨나요?</h1>"));
    }

    #[test]
    fn cert_mark_signals() {
        let html = r#"<a href="https://www.wa.or.kr/x"><img src="/img/wa_mark.png" alt="과학기술정보통신부 WA WEB접근성 품질인증 마크"></a>"#;
        let s = cert_mark(html);
        assert_eq!(s.issuer, Some("한국정보접근성인증평가원(WA)"));
        assert!(s.text_signal && s.filename_signal);

        let s = cert_mark(
            r#"<a href="https://iwa.or.kr"><img src="/wa_marketing.png" alt="banner"></a>"#,
        );
        assert_eq!(s, CertMarkSignals::default());
    }
}
