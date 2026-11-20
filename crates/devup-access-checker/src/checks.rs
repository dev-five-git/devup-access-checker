//! 수집한 사실(fact) → `Check` 목록 변환. 네트워크 접근 없음, 점수·판정 없음.

use reqwest::header::HeaderMap;
use serde_json::{Value, json};
use url::Url;

use crate::fetch::{CertInfo, FileProbe, Hop, LinkProbe, Page, SamplePage};
use crate::html::PageFacts;
use crate::model::{Category, Check, Presence};
use crate::robots::Robots;
use crate::signals::{self, CertMarkSignals};
use crate::{a11y, extra, llms, sitemap};

/// body 텍스트가 이보다 짧으면 CSR 셸로 보고 마크 "없음" 을 단정하지 않는다.
pub const CSR_SHELL_TEXT_LEN: usize = 500;

/// 검색엔진 크롤러 (robots.txt 허용 여부 점검 대상): (id 접미사, product token, 표시명)
pub const SEARCH_CRAWLERS: &[(&str, &str, &str)] = &[
    ("googlebot", "Googlebot", "Google"),
    ("bingbot", "Bingbot", "Bing"),
    ("yeti", "Yeti", "Naver"),
    ("daumoa", "Daumoa", "Daum"),
];

/// 생성형 AI 크롤러 (robots.txt 허용 여부 점검 대상).
pub const AI_CRAWLERS: &[(&str, &str, &str)] = &[
    ("gptbot", "GPTBot", "OpenAI 학습"),
    ("oai_searchbot", "OAI-SearchBot", "ChatGPT 검색"),
    ("chatgpt_user", "ChatGPT-User", "ChatGPT 사용자 요청"),
    ("claudebot", "ClaudeBot", "Anthropic 학습"),
    ("claude_searchbot", "Claude-SearchBot", "Claude 검색"),
    ("claude_user", "Claude-User", "Claude 사용자 요청"),
    ("perplexitybot", "PerplexityBot", "Perplexity 검색"),
    (
        "perplexity_user",
        "Perplexity-User",
        "Perplexity 사용자 요청",
    ),
    ("google_extended", "Google-Extended", "Gemini 학습"),
    ("applebot_extended", "Applebot-Extended", "Apple AI 학습"),
    ("meta_externalagent", "meta-externalagent", "Meta AI"),
    ("amazonbot", "Amazonbot", "Amazon"),
    ("bytespider", "Bytespider", "ByteDance"),
    ("ccbot", "CCBot", "Common Crawl"),
];

/// 실제 User-Agent 로 페이지를 요청해 볼 AI 크롤러: (id 접미사, UA 문자열)
pub const AI_USER_AGENTS: &[(&str, &str)] = &[
    (
        "gptbot",
        "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko); compatible; GPTBot/1.2; +https://openai.com/gptbot",
    ),
    (
        "claudebot",
        "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; ClaudeBot/1.0; +claudebot@anthropic.com)",
    ),
    (
        "perplexitybot",
        "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; PerplexityBot/1.0; +https://perplexity.ai/perplexitybot)",
    ),
];

#[derive(Debug, Default)]
pub struct LinkProbes {
    pub favicon: Option<LinkProbe>,
    pub apple_touch_icon: Option<LinkProbe>,
    pub canonical: Option<LinkProbe>,
    pub og_image: Option<LinkProbe>,
    pub twitter_image: Option<LinkProbe>,
    pub manifest: Option<LinkProbe>,
    pub feed: Option<LinkProbe>,
}

/// www <-> non-www 대체 호스트 요청 결과.
#[derive(Debug)]
pub struct HostVariant {
    pub url: Url,
    pub result: Result<(u16, Url), String>,
}

/// 사이트 루트의 관례적 파일 probe 결과.
#[derive(Debug)]
pub struct WellKnown {
    pub favicon_ico: FileProbe,
    pub apple_touch_icon: FileProbe,
    pub security_txt: FileProbe,
    pub tdmrep: FileProbe,
    pub apple_app_site_association: FileProbe,
    pub assetlinks: FileProbe,
    pub ads_txt: FileProbe,
    /// `/.well-known/ard.json` (Agentic Resource Discovery)
    pub ard: FileProbe,
    /// `/.well-known/ai-catalog.json` (ARD 이전 위치)
    pub ai_catalog: FileProbe,
}

/// robots 선언 sitemap 과 sitemap 안 URL 샘플 요청 결과.
#[derive(Debug, Default)]
pub struct SitemapDetail {
    /// robots.txt `Sitemap:` 에 선언된 sitemap probe (기본 경로와 같은 URL 제외)
    pub declared: Vec<FileProbe>,
    /// 샘플 URL 요청 결과
    pub samples: Vec<SampleResult>,
    /// 표본을 얻지 못한 이유 (예: 하위 sitemap 읽기 실패)
    pub sample_note: Option<String>,
}

/// (sitemap 의 URL, 해당 URL 요청 결과)
pub type SampleResult = (String, Result<SamplePage, String>);

/// (hreflang, URL, 해당 페이지 요청 결과)
pub type HreflangTarget = (String, String, Result<SamplePage, String>);

/// 외부 스타일시트 분석 결과.
#[derive(Debug, Clone, Default)]
pub struct ExternalCss {
    pub total: usize,
    pub checked: usize,
    pub failed: usize,
    /// 받은 CSS 바이트 합 (압축 해제 기준)
    pub bytes: usize,
    pub facts: crate::css::CssFacts,
}

/// 페이지 리소스 무게 (압축 해제 기준, 표본 상한 있음).
#[derive(Debug, Clone, Default)]
pub struct PageWeight {
    pub js_total: usize,
    pub js_checked: usize,
    pub js_bytes: usize,
    pub images_total: usize,
    pub images_checked: usize,
    /// Content-Length 를 알 수 없었던 이미지 수
    pub images_unknown: usize,
    pub image_bytes: u64,
    /// [`LARGE_SCRIPT_BYTES`] 를 넘는 스크립트 (URL, 바이트), 큰 순
    pub large_scripts: Vec<(String, u64)>,
    /// [`LARGE_IMAGE_BYTES`] 를 넘는 이미지 (URL, 바이트), 큰 순
    pub large_images: Vec<(String, u64)>,
    /// [`LARGE_GIF_BYTES`] 를 넘는 GIF (움직이는 이미지는 동영상이 훨씬 작다)
    pub large_gifs: Vec<(String, u64)>,
}

/// 큰 GIF 기준 (Lighthouse "효율적인 애니메이션 콘텐츠" 와 같은 100KB)
pub const LARGE_GIF_BYTES: u64 = 100 * 1024;

/// 큰 스크립트 기준 (압축 해제 바이트)
pub const LARGE_SCRIPT_BYTES: u64 = 500 * 1024;
/// 큰 이미지 기준 (전송 바이트)
pub const LARGE_IMAGE_BYTES: u64 = 500 * 1024;

/// 기준을 넘는 항목만 큰 순으로 최대 10개.
pub fn largest(items: impl Iterator<Item = (String, u64)>, limit: u64) -> Vec<(String, u64)> {
    let mut v: Vec<(String, u64)> = items.filter(|(_, n)| *n > limit).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v.truncate(10);
    v
}

/// 클라이언트 측 리다이렉트 한 단계 (meta refresh / JavaScript).
#[derive(Debug, Clone)]
pub struct ClientRedirect {
    /// `meta_refresh` 또는 `javascript`
    pub kind: &'static str,
    pub from: String,
    pub to: String,
    /// meta refresh 지연 초
    pub delay: Option<f64>,
}

/// 여러 페이지 표본 크롤링 결과.
#[derive(Debug, Default)]
pub struct SiteSample {
    /// 후보 URL 출처: `sitemap`, `internal_links`, `sitemap+internal_links`
    pub source: &'static str,
    pub pages: Vec<SampleResult>,
}

/// (정적 리소스 URL, 상태 코드와 응답 헤더)
pub type AssetHeaders = (String, Result<(u16, HeaderMap), String>);

/// (URL, 상태 코드와 최종 URL)
pub type StatusSample = (String, Result<(u16, Url), String>);

/// URL 변형(끝 슬래시, 대문자, index.html) 요청 결과.
#[derive(Debug)]
pub struct UrlVariant {
    /// `trailing_slash` / `uppercase` / `index_html`
    pub kind: &'static str,
    pub url: Url,
    pub result: Result<(u16, Url), String>,
}

/// og:image 실제 파일 정보.
#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub url: String,
    pub status: u16,
    pub content_type: Option<String>,
    /// 받은 바이트 수 (잘렸으면 상한값)
    pub bytes: usize,
    pub truncated: bool,
    pub width: Option<usize>,
    pub height: Option<usize>,
}

/// 네트워크로 추가 수집한 B 그룹 사실.
#[derive(Debug, Default)]
pub struct Extended {
    /// None = 수행 안 함
    pub tls: Option<Result<Option<CertInfo>, String>>,
    pub url_variants: Vec<UrlVariant>,
    /// None = og:image 없음 또는 수행 안 함
    pub og_image: Option<Result<ImageInfo, String>>,
    /// 내부 링크 표본. None = 수행 안 함
    pub internal_samples: Option<Vec<StatusSample>>,
    pub markdown_version: Option<FileProbe>,
    /// 인증서 남은 일수 계산 기준 (UNIX 초)
    pub now: i64,
    /// manifest 응답 (상태 코드, JSON). None = manifest 링크 없음 또는 수행 안 함
    pub manifest: Option<Result<(u16, Option<Value>), String>>,
    /// favicon 이미지 크기. None = 수행 안 함
    pub favicon_image: Option<Result<ImageInfo, String>>,
    /// 정적 리소스 표본 (URL, 상태, 헤더). None = 같은 호스트 리소스 없음 또는 수행 안 함
    pub static_asset: Option<AssetHeaders>,
    /// canonical 대상 페이지 (자기 참조가 아닐 때만). None = 해당 없음
    pub canonical_target: Option<Result<SamplePage, String>>,
    /// hreflang 대상 페이지 (자기 자신 제외)
    pub hreflang_targets: Vec<HreflangTarget>,
    /// None = 수행 안 함
    pub external_css: Option<ExternalCss>,
    /// None = 수행 안 함
    pub page_weight: Option<PageWeight>,
}

/// 기본 경로 또는 robots 선언 중 처음 존재하는 sitemap.
pub fn pick_sitemap<'a>(
    default: &'a FileProbe,
    declared: &'a [FileProbe],
) -> Option<&'a FileProbe> {
    std::iter::once(default).chain(declared).find(|p| p.exists)
}

pub fn is_gz(p: &FileProbe) -> bool {
    p.url.ends_with(".gz")
}

/// 체크 생성에 필요한 모든 입력.
pub struct Inputs<'a> {
    pub page: &'a Page,
    pub facts: &'a PageFacts,
    pub sitemap: &'a FileProbe,
    pub robots: &'a FileProbe,
    pub llms: &'a FileProbe,
    pub llms_full: &'a FileProbe,
    pub ai_txt: &'a FileProbe,
    pub not_found: &'a Result<(u16, String), String>,
    /// http:// 버전 요청 결과. None = 수행 안 함
    pub http_version: Option<&'a Result<(u16, Url), String>>,
    /// 압축 없이 받은 응답의 Content-Encoding. None = 수행 안 함
    pub content_encoding: Option<&'a Result<Option<String>, String>>,
    /// None = 링크 검증 생략
    pub links: Option<&'a LinkProbes>,
    /// AI UA 요청 결과. 빈 목록 = 생략
    pub ai_ua: &'a [AiUaResult],
    pub well_known: &'a WellKnown,
    pub sitemap_detail: &'a SitemapDetail,
    /// None = 수행 안 함
    pub site: Option<&'a SiteSample>,
    /// 따라간 meta refresh (출발 URL, 도착 URL, 지연 초)
    pub refresh_hops: &'a [ClientRedirect],
    /// 오류 응답 / 봇 차단 사유
    pub blocked: Option<&'a str>,
    pub challenge: Option<&'a str>,
    /// None = 수행 안 함
    pub redirect_chain: Option<&'a Result<Vec<Hop>, String>>,
    /// None = 수행 안 함 또는 IP/localhost 대상
    pub host_variant: Option<&'a HostVariant>,
    pub extended: &'a Extended,
}

/// (AI 크롤러 id, 해당 UA 로 요청한 결과)
pub type AiUaResult = (
    &'static str,
    Result<(u16, Url, Option<&'static str>), String>,
);

struct Out(Vec<Check>);

impl Out {
    fn push(
        &mut self,
        id: impl Into<String>,
        category: Category,
        label: impl Into<String>,
        presence: Presence,
        value: Option<Value>,
        note: Option<String>,
    ) {
        self.0.push(Check {
            id: id.into(),
            category,
            label: label.into(),
            presence,
            value,
            note,
        });
    }

    /// 값이 있으면 Present, 없으면 Missing.
    fn opt(&mut self, id: &str, cat: Category, label: &str, v: Option<&str>) {
        self.push(
            id,
            cat,
            label,
            Presence::from_bool(v.is_some()),
            v.map(|s| json!(s)),
            None,
        );
    }

    fn text_len(&mut self, id: &str, cat: Category, label: &str, v: Option<&str>) {
        let value = v.map(|s| json!({ "text": s, "length": s.chars().count() }));
        self.push(
            id,
            cat,
            label,
            Presence::from_bool(v.is_some()),
            value,
            None,
        );
    }
}

fn header(h: &HeaderMap, name: &str) -> Option<String> {
    let vals: Vec<&str> = h
        .get_all(name)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .collect();
    (!vals.is_empty()).then(|| vals.join(", "))
}

fn file_value(p: &FileProbe) -> Value {
    json!({
        "url": p.url,
        "statusCode": p.status_code,
        "finalUrl": p.final_url,
        "redirected": p.redirected,
        "contentType": p.content_type,
        "formatValid": p.format_valid,
        "bytes": p.size,
        "truncated": p.truncated,
    })
}

fn file_presence(p: &FileProbe) -> (Presence, Option<String>) {
    if let Some(e) = &p.error {
        return (Presence::Unknown, Some(format!("요청 실패: {e}")));
    }
    if p.exists {
        return (Presence::Present, None);
    }
    let note = if p.redirected {
        format!(
            "다른 경로로 리다이렉트됨 ({})",
            p.final_url.as_deref().unwrap_or_default()
        )
    } else if p.status_code.is_some_and(|s| s < 400) && p.body.as_deref() == Some("") {
        "빈 응답 (0 byte)".to_string()
    } else if !p.format_valid && p.status_code.is_some_and(|s| s < 400) {
        "응답이 기대 형식이 아님 (HTML 페이지 등)".to_string()
    } else {
        format!("HTTP {}", p.status_code.unwrap_or_default())
    };
    (Presence::Missing, Some(note))
}

fn link_check(out: &mut Out, id: &str, label: &str, raw: Option<&str>, probe: Option<&LinkProbe>) {
    let Some(raw) = raw else {
        out.push(
            id,
            Category::Links,
            label,
            Presence::NotApplicable,
            None,
            Some("링크가 페이지에 없음".into()),
        );
        return;
    };
    let Some(p) = probe else {
        out.push(
            id,
            Category::Links,
            label,
            Presence::Unknown,
            Some(json!({ "href": raw })),
            Some("검증 결과 없음".into()),
        );
        return;
    };
    let value =
        json!({ "href": raw, "url": p.url, "statusCode": p.status_code, "finalUrl": p.final_url });
    let (presence, note) = match (&p.error, p.status_code) {
        (Some(e), _) => (Presence::Unknown, Some(format!("요청 실패: {e}"))),
        (None, Some(_)) if p.reachable() => (Presence::Present, None),
        (None, Some(s)) => (Presence::Missing, Some(format!("HTTP {s}"))),
        (None, None) => (Presence::Unknown, None),
    };
    out.push(id, Category::Links, label, presence, Some(value), note);
}

fn cert_mark_check(out: &mut Out, s: &CertMarkSignals, body_text_len: usize) {
    let value = json!({
        "issuer": s.issuer,
        "lookupUrl": s.lookup_url,
        "textSignal": s.text_signal,
        "filenameSignal": s.filename_signal,
        "bodyTextLength": body_text_len,
    });
    let (presence, note) = if let Some(issuer) = s.issuer {
        (
            Presence::Present,
            Some(format!(
                "{issuer} 링크 확인. 부착 사실만으로 인증 유효 여부는 알 수 없음"
            )),
        )
    } else if s.text_signal && s.filename_signal {
        (
            Presence::Present,
            Some("마크 이미지(alt 문구 + 파일명) 확인. 발급기관 미특정".into()),
        )
    } else if s.text_signal || s.filename_signal {
        (
            Presence::Unknown,
            Some("인증 관련 표기 일부만 발견되어 확정 불가".into()),
        )
    } else if body_text_len < CSR_SHELL_TEXT_LEN {
        (
            Presence::Unknown,
            Some(format!(
                "정적 HTML body 텍스트 {body_text_len}자 (CSR 셸 추정) — 판별 불가"
            )),
        )
    } else {
        (Presence::Missing, None)
    };
    out.push(
        "accessibility.cert_mark",
        Category::Accessibility,
        "정보통신접근성 품질인증 마크",
        presence,
        Some(value),
        note,
    );
}

const ORG_TYPES: &[&str] = &[
    "Organization",
    "Corporation",
    "LocalBusiness",
    "NGO",
    "EducationalOrganization",
    "GovernmentOrganization",
    "OnlineBusiness",
    "OnlineStore",
    "NewsMediaOrganization",
];
const ARTICLE_TYPES: &[&str] = &[
    "Article",
    "NewsArticle",
    "BlogPosting",
    "TechArticle",
    "Report",
];

pub fn build(i: &Inputs) -> Vec<Check> {
    use Category::*;
    let f = i.facts;
    let page = i.page;
    let base = &page.final_url;
    let mut o = Out(Vec::new());

    // ===== 기본 메타 =====
    o.text_len("meta.title", Meta, "title", f.title.as_deref());
    o.text_len(
        "meta.description",
        Meta,
        "meta description",
        f.meta("description"),
    );
    o.opt("meta.keywords", Meta, "meta keywords", f.meta("keywords"));
    o.opt("meta.viewport", Meta, "meta viewport", f.meta("viewport"));
    let header_charset = header(&page.headers, "content-type").and_then(|ct| {
        ct.to_ascii_lowercase()
            .split("charset=")
            .nth(1)
            .map(|c| c.trim().to_string())
    });
    o.push(
        "meta.charset",
        Meta,
        "charset",
        Presence::from_bool(f.charset.is_some() || header_charset.is_some()),
        Some(json!({ "meta": f.charset, "header": header_charset })),
        None,
    );
    o.opt("meta.html_lang", Meta, "html lang", f.lang.as_deref());
    match &f.canonical {
        Some(href) => {
            let abs = f.link_base(base).join(href).ok();
            let same = abs.as_ref().is_some_and(|a| {
                a.as_str().trim_end_matches('/') == base.as_str().trim_end_matches('/')
            });
            o.push(
                "meta.canonical",
                Meta,
                "canonical link",
                Presence::Present,
                Some(json!({ "href": href, "resolved": abs.map(|u| u.to_string()), "selfReferencing": same })),
                None,
            );
        }
        None => o.push(
            "meta.canonical",
            Meta,
            "canonical link",
            Presence::Missing,
            None,
            None,
        ),
    }
    o.opt(
        "meta.theme_color",
        Meta,
        "theme-color",
        f.meta("theme-color"),
    );
    o.opt("meta.author", Meta, "meta author", f.meta("author"));
    o.opt("meta.favicon", Meta, "favicon", f.favicon.as_deref());
    o.opt(
        "meta.apple_touch_icon",
        Meta,
        "apple-touch-icon",
        f.apple_touch_icon.as_deref(),
    );
    o.opt(
        "meta.manifest",
        Meta,
        "web app manifest",
        f.manifest.as_deref(),
    );
    let wk = i.well_known;
    for (id, label, probe) in [
        (
            "meta.favicon_ico_file",
            "/favicon.ico 파일",
            &wk.favicon_ico,
        ),
        (
            "meta.apple_touch_icon_file",
            "/apple-touch-icon.png 파일",
            &wk.apple_touch_icon,
        ),
    ] {
        let (p, n) = file_presence(probe);
        o.push(id, Meta, label, p, Some(file_value(probe)), n);
    }
    host_mismatch(
        &mut o,
        "meta.canonical_host_mismatch",
        Meta,
        "canonical 호스트 불일치",
        f.canonical.as_deref(),
        base,
        f.link_base(base),
    );
    o.opt(
        "meta.google_site_verification",
        Meta,
        "Google Search Console 인증",
        f.meta("google-site-verification"),
    );
    o.opt(
        "meta.naver_site_verification",
        Meta,
        "네이버 서치어드바이저 인증",
        f.meta("naver-site-verification"),
    );

    // ===== 색인 제어 =====
    let robots_meta = f.meta("robots");
    o.opt("indexing.meta_robots", Indexing, "meta robots", robots_meta);
    let directives: Vec<String> = [f.meta("robots"), f.meta("googlebot")]
        .into_iter()
        .flatten()
        .flat_map(|v| v.split(',').map(|d| d.trim().to_ascii_lowercase()))
        .collect();
    let has_directive = |d: &str| {
        directives
            .iter()
            .any(|x| x == d || (d == "noindex" && x == "none"))
    };
    o.push(
        "indexing.meta_noindex",
        Indexing,
        "meta noindex 지시",
        Presence::from_bool(has_directive("noindex")),
        None,
        None,
    );
    o.push(
        "indexing.meta_nofollow",
        Indexing,
        "meta nofollow 지시",
        Presence::from_bool(has_directive("nofollow") || directives.iter().any(|x| x == "none")),
        None,
        None,
    );
    let x_robots = header(&page.headers, "x-robots-tag");
    o.opt(
        "indexing.x_robots_tag",
        Indexing,
        "X-Robots-Tag 헤더",
        x_robots.as_deref(),
    );
    let header_noindex = x_robots.as_deref().is_some_and(|v| {
        let v = v.to_ascii_lowercase();
        v.contains("noindex") || v.split([',', ':']).any(|t| t.trim() == "none")
    });
    o.push(
        "indexing.header_noindex",
        Indexing,
        "X-Robots-Tag noindex",
        Presence::from_bool(header_noindex),
        None,
        None,
    );
    o.opt(
        "indexing.meta_refresh",
        Indexing,
        "meta refresh 리다이렉트",
        f.meta_refresh.as_deref(),
    );

    // ===== Open Graph / Twitter / Facebook =====
    for (key, label) in [
        ("og:title", "og:title"),
        ("og:description", "og:description"),
        ("og:image", "og:image"),
        ("og:url", "og:url"),
        ("og:type", "og:type"),
        ("og:site_name", "og:site_name"),
        ("og:locale", "og:locale"),
    ] {
        o.opt(
            &format!("open_graph.{}", &key[3..]),
            OpenGraph,
            label,
            f.meta(key),
        );
    }
    host_mismatch(
        &mut o,
        "open_graph.url_host_mismatch",
        OpenGraph,
        "og:url 호스트 불일치",
        f.meta("og:url"),
        base,
        f.link_base(base),
    );
    for key in ["card", "title", "description", "image"] {
        o.opt(
            &format!("twitter.{key}"),
            Twitter,
            &format!("twitter:{key}"),
            f.meta(&format!("twitter:{key}")),
        );
    }
    o.opt(
        "facebook.app_id",
        Facebook,
        "fb:app_id",
        f.meta("fb:app_id"),
    );
    o.opt("facebook.pages", Facebook, "fb:pages", f.meta("fb:pages"));
    o.opt(
        "facebook.article_publisher",
        Facebook,
        "article:publisher",
        f.meta("article:publisher"),
    );

    // ===== 구조화 데이터 =====
    let ld = &f.json_ld;
    o.push(
        "structured_data.json_ld",
        StructuredData,
        "JSON-LD",
        Presence::from_bool(ld.blocks > 0),
        (ld.blocks > 0).then(|| json!({ "blocks": ld.blocks, "invalidBlocks": ld.invalid_blocks, "types": ld.types })),
        None,
    );
    o.push(
        "structured_data.json_ld_invalid",
        StructuredData,
        "JSON 파싱 불가 JSON-LD 블록",
        if ld.blocks == 0 {
            Presence::NotApplicable
        } else {
            Presence::from_bool(ld.invalid_blocks > 0)
        },
        (ld.blocks > 0).then(|| json!(ld.invalid_blocks)),
        None,
    );
    let has_type = |types: &[&str]| {
        ld.types
            .iter()
            .find(|t| types.contains(&t.as_str()))
            .cloned()
    };
    for (id, label, types) in [
        ("organization", "Organization 스키마", ORG_TYPES),
        ("website", "WebSite 스키마", &["WebSite"][..]),
        (
            "breadcrumb_list",
            "BreadcrumbList 스키마",
            &["BreadcrumbList"][..],
        ),
        ("faq_page", "FAQPage 스키마", &["FAQPage"][..]),
        ("article", "Article 스키마", ARTICLE_TYPES),
        ("product", "Product 스키마", &["Product"][..]),
    ] {
        let found = has_type(types);
        o.push(
            format!("structured_data.type.{id}"),
            StructuredData,
            label,
            Presence::from_bool(found.is_some()),
            found.map(Value::from),
            None,
        );
    }
    o.push(
        "structured_data.same_as",
        StructuredData,
        "sameAs (외부 프로필 연결)",
        Presence::from_bool(!ld.same_as.is_empty()),
        (!ld.same_as.is_empty()).then(|| json!(ld.same_as)),
        None,
    );
    o.push(
        "structured_data.microdata",
        StructuredData,
        "Microdata (itemscope)",
        Presence::from_bool(f.has_microdata),
        None,
        None,
    );
    o.push(
        "structured_data.hreflang",
        StructuredData,
        "hreflang 링크",
        Presence::from_bool(!f.hreflang.is_empty()),
        (!f.hreflang.is_empty()).then(|| {
            json!(
                f.hreflang
                    .iter()
                    .map(|h| json!({ "lang": h.lang, "href": h.href }))
                    .collect::<Vec<_>>()
            )
        }),
        None,
    );

    // ===== 본문 구조 =====
    o.push(
        "content.h1",
        Content,
        "h1 태그",
        Presence::from_bool(f.h1_count > 0),
        Some(json!(f.h1_count)),
        None,
    );
    o.push(
        "content.h2",
        Content,
        "h2 태그",
        Presence::from_bool(f.h2_count > 0),
        Some(json!(f.h2_count)),
        None,
    );
    o.push(
        "content.img_alt_missing",
        Content,
        "alt 누락 이미지",
        if f.img_total == 0 {
            Presence::NotApplicable
        } else {
            Presence::from_bool(f.img_without_alt > 0)
        },
        Some(json!({ "total": f.img_total, "missing": f.img_without_alt })),
        None,
    );
    o.push(
        "content.body_text",
        Content,
        "정적 HTML body 텍스트",
        Presence::from_bool(f.body_text_len > 0),
        Some(json!(f.body_text_len)),
        Some("JavaScript 실행 없이 받은 HTML 기준 글자 수".into()),
    );
    o.push(
        "content.internal_links",
        Content,
        "내부 링크",
        Presence::from_bool(f.internal_links > 0),
        Some(json!(f.internal_links)),
        None,
    );
    o.push(
        "content.external_links",
        Content,
        "외부 링크",
        Presence::from_bool(f.external_links > 0),
        Some(json!(f.external_links)),
        None,
    );

    // ===== 접근성 =====
    cert_mark_check(&mut o, &signals::cert_mark(&page.body), f.body_text_len);

    // ===== 분석 도구 =====
    let google = signals::google_tracking(&page.body);
    o.push(
        "analytics.google",
        Analytics,
        "Google Analytics / GTM",
        Presence::from_bool(!google.is_empty()),
        (!google.is_empty()).then(|| json!(google)),
        None,
    );
    o.push(
        "analytics.naver",
        Analytics,
        "네이버 애널리틱스",
        Presence::from_bool(signals::naver_analytics(&page.body)),
        None,
        None,
    );

    // ===== 성능 =====
    o.push(
        "performance.font_preload",
        Performance,
        "font preload",
        Presence::from_bool(f.font_preloads > 0),
        Some(json!(f.font_preloads)),
        None,
    );
    o.push(
        "performance.image_preload",
        Performance,
        "image preload",
        Presence::from_bool(f.image_preloads > 0),
        Some(json!(f.image_preloads)),
        None,
    );
    let img_na = |b: bool| {
        if f.img_total == 0 {
            Presence::NotApplicable
        } else {
            Presence::from_bool(b)
        }
    };
    o.push(
        "performance.img_dimensions_missing",
        Performance,
        "width/height 누락 이미지",
        img_na(f.img_without_dimensions > 0),
        Some(json!({ "total": f.img_total, "missing": f.img_without_dimensions })),
        None,
    );
    o.push(
        "performance.img_lazy",
        Performance,
        "loading=lazy 이미지",
        img_na(f.img_lazy > 0),
        Some(json!({ "total": f.img_total, "lazy": f.img_lazy })),
        None,
    );
    o.push(
        "performance.response_time",
        Performance,
        "HTML 응답 시간(ms)",
        Presence::Present,
        Some(json!(page.elapsed.as_millis() as u64)),
        None,
    );
    o.push(
        "performance.html_bytes",
        Performance,
        "HTML 크기(byte, 압축 해제)",
        Presence::Present,
        Some(json!(page.body_bytes)),
        page.truncated.then(|| {
            format!(
                "본문이 {}MB 를 넘어 앞부분만 분석함",
                crate::fetch::MAX_PAGE_BYTES / 1024 / 1024
            )
        }),
    );
    match i.content_encoding {
        Some(Ok(enc)) => o.opt(
            "performance.compression",
            Performance,
            "응답 압축 (Content-Encoding)",
            enc.as_deref(),
        ),
        Some(Err(e)) => o.push(
            "performance.compression",
            Performance,
            "응답 압축 (Content-Encoding)",
            Presence::Unknown,
            None,
            Some(format!("요청 실패: {e}")),
        ),
        None => {}
    }
    o.opt(
        "performance.cache_control",
        Performance,
        "Cache-Control 헤더",
        header(&page.headers, "cache-control").as_deref(),
    );

    // ===== 전송 =====
    o.push(
        "transport.https",
        Transport,
        "HTTPS",
        Presence::from_bool(base.scheme() == "https"),
        Some(json!(base.scheme())),
        None,
    );
    o.push(
        "transport.http_status",
        Transport,
        "HTTP 2xx 응답",
        Presence::from_bool((200..300).contains(&page.status)),
        Some(json!(page.status)),
        None,
    );
    if let Some(r) = i.http_version {
        let (presence, value, note) = match r {
            Ok((status, final_url)) => (
                Presence::from_bool(final_url.scheme() == "https"),
                Some(json!({ "statusCode": status, "finalUrl": final_url.as_str() })),
                None,
            ),
            Err(e) => (
                Presence::Unknown,
                None,
                Some(format!("http:// 요청 실패: {e}")),
            ),
        };
        o.push(
            "transport.http_to_https_redirect",
            Transport,
            "HTTP → HTTPS 리다이렉트",
            presence,
            value,
            note,
        );
    }
    if let Some(r) = i.redirect_chain {
        match r {
            Ok(hops) => {
                let redirects = &hops[..hops.len().saturating_sub(1)];
                let value = json!(
                    hops.iter()
                        .map(|h| json!({ "url": h.url, "status": h.status }))
                        .collect::<Vec<_>>()
                );
                o.push(
                    "transport.redirect",
                    Transport,
                    "요청 URL 리다이렉트",
                    Presence::from_bool(!redirects.is_empty()),
                    Some(value),
                    None,
                );
                let temporary: Vec<u16> = redirects
                    .iter()
                    .map(|h| h.status)
                    .filter(|s| matches!(s, 302 | 303 | 307))
                    .collect();
                o.push(
                    "transport.temporary_redirect",
                    Transport,
                    "임시 리다이렉트 (302/303/307)",
                    if redirects.is_empty() {
                        Presence::NotApplicable
                    } else {
                        Presence::from_bool(!temporary.is_empty())
                    },
                    (!temporary.is_empty()).then(|| json!(temporary)),
                    None,
                );
            }
            Err(e) => {
                o.push(
                    "transport.redirect",
                    Transport,
                    "요청 URL 리다이렉트",
                    Presence::Unknown,
                    None,
                    Some(e.clone()),
                );
                o.push(
                    "transport.temporary_redirect",
                    Transport,
                    "임시 리다이렉트 (302/303/307)",
                    Presence::Unknown,
                    None,
                    None,
                );
            }
        }
    }
    if let Some(v) = i.host_variant {
        let label = "www/non-www 중복 노출";
        match &v.result {
            Ok((status, final_url)) => {
                let same = final_url.host_str() == base.host_str();
                let ok = (200..300).contains(status);
                o.push(
                    "transport.host_variant_duplicate",
                    Transport,
                    label,
                    if !ok && !same { Presence::NotApplicable } else { Presence::from_bool(ok && !same) },
                    Some(json!({ "variant": v.url.as_str(), "statusCode": status, "finalUrl": final_url.as_str() })),
                    (!ok && !same).then(|| format!("대체 호스트가 HTTP {status} 응답")),
                );
            }
            Err(e) => o.push(
                "transport.host_variant_duplicate",
                Transport,
                label,
                Presence::NotApplicable,
                Some(json!({ "variant": v.url.as_str() })),
                Some(format!("대체 호스트 접속 불가: {e}")),
            ),
        }
    } else if !matches!(base.host(), Some(url::Host::Domain(d)) if d.contains('.')) {
        o.push(
            "transport.host_variant_duplicate",
            Transport,
            "www/non-www 중복 노출",
            Presence::NotApplicable,
            None,
            Some("IP·단일 레이블 호스트는 www 변형이 없음".into()),
        );
    }
    let (p, n) = file_presence(&wk.security_txt);
    o.push(
        "transport.security_txt",
        Transport,
        "/.well-known/security.txt",
        p,
        Some(file_value(&wk.security_txt)),
        n,
    );
    o.opt(
        "transport.hsts",
        Transport,
        "HSTS 헤더",
        header(&page.headers, "strict-transport-security").as_deref(),
    );
    o.push(
        "transport.mixed_content",
        Transport,
        "혼합 콘텐츠 (https 페이지의 http 리소스)",
        if base.scheme() != "https" {
            Presence::NotApplicable
        } else {
            Presence::from_bool(!f.insecure_subresources.is_empty())
        },
        (!f.insecure_subresources.is_empty()).then(|| json!(f.insecure_subresources)),
        None,
    );

    // ===== 크롤링 =====
    let (p, n) = file_presence(i.sitemap);
    o.push(
        "crawling.sitemap_xml",
        Crawling,
        "sitemap.xml",
        p,
        Some(file_value(i.sitemap)),
        n,
    );
    sitemap_checks(&mut o, i, base);
    let (p, n) = file_presence(i.robots);
    o.push(
        "crawling.robots_txt",
        Crawling,
        "robots.txt",
        p,
        Some(file_value(i.robots)),
        n,
    );

    // robots.txt 가 없으면(4xx) 모두 허용. 요청 실패/5xx 는 판별 불가.
    let robots: Result<Robots, String> = if i.robots.exists {
        Ok(Robots::parse(i.robots.body.as_deref().unwrap_or_default()))
    } else if let Some(e) = &i.robots.error {
        Err(format!("robots.txt 요청 실패: {e}"))
    } else if i.robots.status_code.is_some_and(|s| s >= 500) {
        Err(format!(
            "robots.txt HTTP {}",
            i.robots.status_code.unwrap_or_default()
        ))
    } else {
        Ok(Robots::default())
    };
    let robots_note =
        (!i.robots.exists && robots.is_ok()).then(|| "robots.txt 없음 — 기본 허용".to_string());

    match &robots {
        Ok(r) if i.robots.exists => o.push(
            "crawling.robots_sitemap_directive",
            Crawling,
            "robots.txt Sitemap 지시자",
            Presence::from_bool(!r.sitemaps.is_empty()),
            (!r.sitemaps.is_empty()).then(|| json!(r.sitemaps)),
            None,
        ),
        _ => o.push(
            "crawling.robots_sitemap_directive",
            Crawling,
            "robots.txt Sitemap 지시자",
            Presence::NotApplicable,
            None,
            None,
        ),
    }
    // robots 규칙은 경로 + 쿼리에 대해 판정한다 (RFC 9309)
    let path = &base[url::Position::BeforePath..url::Position::AfterQuery];
    let allow_check = |o: &mut Out, id: String, cat: Category, label: String, token: &str| {
        match &robots {
        Ok(r) => o.push(
            id,
            cat,
            label,
            Presence::from_bool(r.is_allowed(token, path)),
            Some(json!({ "token": token, "path": path, "explicitGroup": r.has_group_for(token) })),
            robots_note.clone(),
        ),
        Err(e) => o.push(id, cat, label, Presence::Unknown, None, Some(e.clone())),
    }
    };
    // robots.txt 가 선언한 sitemap 전체를 본다. 기본 경로와 같은 선언은 기본 경로 probe 결과를 쓰고,
    // 요청 상한(5개)을 넘어 확인하지 못한 선언은 unchecked 로 남긴다.
    let declared_raw: Vec<Url> = if i.robots.exists {
        let origin = base.join("/").expect("origin");
        let mut v: Vec<Url> = Vec::new();
        for raw in Robots::parse(i.robots.body.as_deref().unwrap_or_default()).sitemaps {
            if let Ok(u) = origin.join(&raw)
                && !v.contains(&u)
            {
                v.push(u);
            }
        }
        v
    } else {
        Vec::new()
    };
    let entries: Vec<(&Url, Option<&FileProbe>)> = declared_raw
        .iter()
        .map(|u| {
            let probe = if i.sitemap.url == u.as_str() {
                Some(i.sitemap)
            } else {
                i.sitemap_detail
                    .declared
                    .iter()
                    .find(|p| p.url == u.as_str())
            };
            (u, probe)
        })
        .collect();
    let checked: Vec<&FileProbe> = entries.iter().filter_map(|(_, p)| *p).collect();
    let unchecked = entries.len() - checked.len();
    let presence = if entries.is_empty() {
        Presence::NotApplicable
    } else if checked.iter().any(|p| p.error.is_none() && !p.exists) {
        Presence::Missing
    } else if unchecked > 0 || checked.iter().any(|p| p.error.is_some()) {
        Presence::Unknown
    } else {
        Presence::Present
    };
    o.push(
        "crawling.robots_declared_sitemaps",
        Crawling,
        "robots.txt 선언 sitemap 접근",
        presence,
        (!entries.is_empty()).then(|| {
            json!(
                entries
                    .iter()
                    .map(|(u, p)| match p {
                        Some(p) => json!({
                            "url": u.as_str(),
                            "statusCode": p.status_code,
                            "exists": p.exists,
                            "error": p.error,
                        }),
                        None => json!({ "url": u.as_str(), "checked": false }),
                    })
                    .collect::<Vec<_>>()
            )
        }),
        (unchecked > 0).then(|| {
            format!(
                "선언 {}개 중 {unchecked}개는 요청 상한으로 확인하지 않음",
                entries.len()
            )
        }),
    );
    o.push(
        "crawling.feed_link",
        Crawling,
        "RSS/Atom 피드 링크",
        Presence::from_bool(!f.feeds.is_empty()),
        (!f.feeds.is_empty()).then(|| json!(f.feeds)),
        None,
    );
    allow_check(
        &mut o,
        "crawling.robots_allows.all".into(),
        Crawling,
        "robots.txt 기본(*) 허용".into(),
        "*",
    );
    for (id, token, name) in SEARCH_CRAWLERS {
        allow_check(
            &mut o,
            format!("crawling.robots_allows.{id}"),
            Crawling,
            format!("{name} 크롤러 허용 ({token})"),
            token,
        );
    }

    // ===== 생성형 검색(GEO) =====
    for (id, label, probe) in [
        ("llms_txt", "llms.txt", i.llms),
        ("llms_full_txt", "llms-full.txt", i.llms_full),
        ("ai_txt", "ai.txt", i.ai_txt),
    ] {
        let (p, n) = file_presence(probe);
        o.push(
            format!("geo.{id}"),
            Geo,
            label,
            p,
            Some(file_value(probe)),
            n,
        );
    }
    let llms_doc = i
        .llms
        .exists
        .then(|| llms::parse(i.llms.body.as_deref().unwrap_or_default()));
    let na_or = |b: Option<bool>| b.map_or(Presence::NotApplicable, Presence::from_bool);
    o.push(
        "geo.llms_txt_title",
        Geo,
        "llms.txt # 제목",
        na_or(llms_doc.as_ref().map(|d| d.title.is_some())),
        llms_doc
            .as_ref()
            .and_then(|d| d.title.clone())
            .map(Value::from),
        None,
    );
    o.push(
        "geo.llms_txt_summary",
        Geo,
        "llms.txt > 요약",
        na_or(llms_doc.as_ref().map(|d| d.has_summary)),
        None,
        None,
    );
    o.push(
        "geo.llms_txt_sections",
        Geo,
        "llms.txt ## 섹션",
        na_or(llms_doc.as_ref().map(|d| d.sections > 0)),
        llms_doc.as_ref().map(|d| json!(d.sections)),
        None,
    );
    o.push(
        "geo.llms_txt_links",
        Geo,
        "llms.txt 링크 목록",
        na_or(llms_doc.as_ref().map(|d| d.links > 0)),
        llms_doc.as_ref().map(|d| json!(d.links)),
        None,
    );
    let tdm_header = header(&page.headers, "tdm-reservation");
    let tdm_meta = f.meta("tdm-reservation");
    o.push(
        "geo.tdm_reservation",
        Geo,
        "TDM 예약 (tdmrep.json / 헤더 / 메타)",
        if wk.tdmrep.error.is_some() && tdm_header.is_none() && tdm_meta.is_none() {
            Presence::Unknown
        } else {
            Presence::from_bool(wk.tdmrep.exists || tdm_header.is_some() || tdm_meta.is_some())
        },
        Some(json!({ "file": file_value(&wk.tdmrep), "fileExists": wk.tdmrep.exists, "header": tdm_header, "meta": tdm_meta })),
        None,
    );
    for (id, token, name) in AI_CRAWLERS {
        allow_check(
            &mut o,
            format!("geo.robots_allows.{id}"),
            Geo,
            format!("{name} 크롤러 허용 ({token})"),
            token,
        );
    }
    for (id, result) in i.ai_ua {
        let label = format!("AI UA 응답 ({id})");
        match result {
            Ok((status, final_url, challenge)) => o.push(
                format!("geo.ua_access.{id}"),
                Geo,
                label,
                Presence::from_bool((200..300).contains(status) && challenge.is_none()),
                Some(json!({ "statusCode": status, "url": final_url.as_str(), "challenge": challenge })),
                Some("User-Agent 문자열만 바꾼 요청 결과. 크롤러 IP 검증을 하는 방화벽에서는 실제 크롤러와 결과가 다를 수 있음".into()),
            ),
            Err(e) => o.push(format!("geo.ua_access.{id}"), Geo, label, Presence::Unknown, None, Some(format!("요청 실패: {e}"))),
        }
    }
    let noai: Vec<&str> = ["noai", "noimageai"]
        .into_iter()
        .filter(|d| has_directive(d))
        .collect();
    o.push(
        "geo.meta_noai",
        Geo,
        "meta noai / noimageai 지시",
        Presence::from_bool(!noai.is_empty()),
        (!noai.is_empty()).then(|| json!(noai)),
        None,
    );

    // ===== 404 페이지 =====
    match i.not_found {
        Ok((status, body)) => {
            let default = signals::is_framework_default_404(body);
            let is404 = *status == 404;
            o.push(
                "not_found_page.returns_404",
                NotFoundPage,
                "없는 경로에 404 응답",
                Presence::from_bool(is404),
                Some(json!(status)),
                None,
            );
            o.push(
                "not_found_page.custom",
                NotFoundPage,
                "커스텀 404 페이지",
                if is404 {
                    Presence::from_bool(!default && body.len() > 2_048)
                } else {
                    Presence::NotApplicable
                },
                Some(json!({ "bodyBytes": body.len(), "frameworkDefault": default })),
                None,
            );
        }
        Err(e) => {
            o.push(
                "not_found_page.returns_404",
                NotFoundPage,
                "없는 경로에 404 응답",
                Presence::Unknown,
                None,
                Some(format!("요청 실패: {e}")),
            );
            o.push(
                "not_found_page.custom",
                NotFoundPage,
                "커스텀 404 페이지",
                Presence::Unknown,
                None,
                None,
            );
        }
    }

    // ===== 링크 검증 =====
    if let Some(l) = i.links {
        link_check(
            &mut o,
            "links.favicon",
            "favicon 접근",
            f.favicon.as_deref(),
            l.favicon.as_ref(),
        );
        link_check(
            &mut o,
            "links.apple_touch_icon",
            "apple-touch-icon 접근",
            f.apple_touch_icon.as_deref(),
            l.apple_touch_icon.as_ref(),
        );
        link_check(
            &mut o,
            "links.canonical",
            "canonical 접근",
            f.canonical.as_deref(),
            l.canonical.as_ref(),
        );
        link_check(
            &mut o,
            "links.og_image",
            "og:image 접근",
            f.meta("og:image"),
            l.og_image.as_ref(),
        );
        link_check(
            &mut o,
            "links.twitter_image",
            "twitter:image 접근",
            f.meta("twitter:image"),
            l.twitter_image.as_ref(),
        );
        link_check(
            &mut o,
            "links.manifest",
            "manifest 접근",
            f.manifest.as_deref(),
            l.manifest.as_ref(),
        );
        link_check(
            &mut o,
            "links.feed",
            "피드 접근",
            f.feeds.first().map(String::as_str),
            l.feed.as_ref(),
        );
    }

    extended_checks(&mut o, i, &robots);
    more_checks(&mut o, i, &robots);
    canonical_hreflang_weight_checks(&mut o, i);
    deep_checks(&mut o, i);
    rule_checks(&mut o, i);
    agent_checks(&mut o, i);
    owasp_checks(&mut o, i);
    if let Some(site) = i.site {
        site_checks(&mut o, i, site);
    }

    // ===== 대상 페이지 상태 =====
    o.push(
        "transport.bot_challenge",
        Category::Transport,
        "봇 차단·챌린지 페이지",
        Presence::from_bool(i.challenge.is_some()),
        i.challenge.map(Value::from),
        None,
    );
    for (id, kind, label) in [
        (
            "transport.meta_refresh_redirect",
            "meta_refresh",
            "meta refresh 리다이렉트 (5초 이하, 따라감)",
        ),
        (
            "transport.js_redirect",
            "javascript",
            "JavaScript 리다이렉트 (빈 페이지의 location 이동, 따라감)",
        ),
    ] {
        let hops: Vec<Value> = i
            .refresh_hops
            .iter()
            .filter(|h| h.kind == kind)
            .map(|h| json!({ "from": h.from, "to": h.to, "delay": h.delay }))
            .collect();
        o.push(
            id,
            Category::Transport,
            label,
            Presence::from_bool(!hops.is_empty()),
            (!hops.is_empty()).then(|| json!(hops)),
            None,
        );
    }
    o.push(
        "transport.client_redirect_total",
        Category::Transport,
        "클라이언트 측 리다이렉트 수",
        Presence::from_bool(!i.refresh_hops.is_empty()),
        Some(json!(i.refresh_hops.len())),
        None,
    );
    if let Some(reason) = i.blocked {
        for c in o.0.iter_mut().filter(|c| depends_on_page(&c.id)) {
            c.presence = Presence::Unknown;
            c.note = Some(format!(
                "대상 페이지가 {reason} — 실제 페이지 내용으로 판별 불가"
            ));
        }
    }
    fill_catalog(o.0)
}

/// 카탈로그의 모든 id 를 카탈로그 순서로 담는다. 만들어지지 않은 항목(옵션으로 생략 등)은 unknown.
pub fn fill_catalog(checks: Vec<Check>) -> Vec<Check> {
    use crate::catalog::{CATALOG, position};
    let mut slots: Vec<Option<Check>> = vec![None; CATALOG.len()];
    for c in checks {
        let pos = position(&c.id);
        debug_assert!(pos.is_some(), "카탈로그에 없는 id: {}", c.id);
        if let Some(pos) = pos {
            slots[pos] = Some(c);
        }
    }
    // 항목명·분류는 카탈로그가 기준이다
    CATALOG
        .iter()
        .zip(slots)
        .map(|((id, category, label), c)| {
            let mut c = c.unwrap_or_else(|| Check {
                id: (*id).to_string(),
                category: *category,
                label: String::new(),
                presence: Presence::Unknown,
                value: None,
                note: Some("점검하지 않음 (옵션으로 생략했거나 수행 조건이 아님)".into()),
            });
            c.category = *category;
            c.label = (*label).to_string();
            c
        })
        .collect()
}

fn host_mismatch(
    o: &mut Out,
    id: &str,
    cat: Category,
    label: &str,
    raw: Option<&str>,
    base: &Url,
    link_base: &Url,
) {
    let Some(u) = raw.and_then(|r| link_base.join(r).ok()) else {
        o.push(id, cat, label, Presence::NotApplicable, None, None);
        return;
    };
    o.push(
        id,
        cat,
        label,
        Presence::from_bool(u.host_str() != base.host_str()),
        Some(json!({ "host": u.host_str(), "finalHost": base.host_str() })),
        None,
    );
}

fn sitemap_checks(o: &mut Out, i: &Inputs, base: &Url) {
    use Category::Crawling;
    let chosen = pick_sitemap(i.sitemap, &i.sitemap_detail.declared);
    o.push(
        "crawling.sitemap_any",
        Crawling,
        "sitemap (기본 경로 또는 robots 선언)",
        Presence::from_bool(chosen.is_some()),
        chosen.map(|p| json!(p.url)),
        None,
    );
    let Some(chosen) = chosen else { return };
    if is_gz(chosen) {
        o.push(
            "crawling.sitemap_entries",
            Crawling,
            "sitemap 항목 수",
            Presence::Unknown,
            None,
            Some("gzip 압축 sitemap 은 내용 분석 생략".into()),
        );
        return;
    }
    let s = sitemap::summarize(chosen.body.as_deref().unwrap_or_default());
    o.push(
        "crawling.sitemap_entries",
        Crawling,
        "sitemap 항목 수",
        Presence::from_bool(s.entries > 0),
        Some(json!({ "url": chosen.url, "isIndex": s.is_index, "entries": s.entries })),
        None,
    );
    let na = s.entries == 0;
    o.push(
        "crawling.sitemap_lastmod",
        Crawling,
        "sitemap lastmod",
        if na {
            Presence::NotApplicable
        } else {
            Presence::from_bool(s.with_lastmod > 0)
        },
        Some(json!({ "withLastmod": s.with_lastmod, "entries": s.entries })),
        None,
    );
    let mut foreign: Vec<String> = Vec::new();
    for loc in &s.locs {
        let host = Url::parse(loc)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string));
        if host.as_deref() != base.host_str() {
            let h = host.unwrap_or_else(|| loc.clone());
            if !foreign.contains(&h) {
                foreign.push(h);
            }
        }
    }
    o.push(
        "crawling.sitemap_foreign_host",
        Crawling,
        "sitemap 내 다른 호스트 URL",
        if na {
            Presence::NotApplicable
        } else {
            Presence::from_bool(!foreign.is_empty())
        },
        (!foreign.is_empty()).then(|| json!(foreign)),
        None,
    );
    let samples = &i.sitemap_detail.samples;
    if samples.is_empty() {
        let note = i
            .sitemap_detail
            .sample_note
            .clone()
            .unwrap_or_else(|| "sitemap 에서 표본 URL 을 얻지 못함".into());
        for (id, label) in [
            (
                "crawling.sitemap_broken_urls",
                "sitemap 샘플 URL 중 비정상 응답",
            ),
            (
                "crawling.sitemap_noindex_pages",
                "sitemap 표본 중 noindex 페이지",
            ),
            (
                "crawling.sitemap_canonical_mismatch",
                "sitemap 표본 중 canonical 이 다른 URL 인 페이지",
            ),
        ] {
            o.push(
                id,
                Crawling,
                label,
                Presence::Unknown,
                None,
                Some(note.clone()),
            );
        }
        return;
    }
    let broken: Vec<Value> = samples
        .iter()
        .filter_map(|(url, r)| match r {
            Ok(p) if (200..300).contains(&p.status) => None,
            Ok(p) => Some(json!({ "url": url, "statusCode": p.status })),
            Err(e) => Some(json!({ "url": url, "error": e })),
        })
        .collect();
    o.push(
        "crawling.sitemap_broken_urls",
        Crawling,
        "sitemap 샘플 URL 중 비정상 응답",
        Presence::from_bool(!broken.is_empty()),
        Some(json!({ "sampled": samples.len(), "broken": broken })),
        Some(format!(
            "{}개 URL 표본 요청{} (리다이렉트 추적 후 2xx 가 아닌 것)",
            samples.len(),
            if s.is_index {
                ", sitemap index 의 첫 하위 sitemap 에서"
            } else {
                ""
            }
        )),
    );
    let pages: Vec<(&String, &SamplePage)> = samples
        .iter()
        .filter_map(|(u, r)| {
            r.as_ref()
                .ok()
                .filter(|p| (200..300).contains(&p.status))
                .map(|p| (u, p))
        })
        .collect();
    let na = pages.is_empty();
    let noindex: Vec<&String> = pages
        .iter()
        .filter(|(_, p)| p.noindex)
        .map(|(u, _)| *u)
        .collect();
    o.push(
        "crawling.sitemap_noindex_pages",
        Crawling,
        "sitemap 표본 중 noindex 페이지",
        if na {
            Presence::NotApplicable
        } else {
            Presence::from_bool(!noindex.is_empty())
        },
        Some(json!({ "checked": pages.len(), "noindex": noindex })),
        None,
    );
    let mismatched: Vec<Value> = pages
        .iter()
        .filter_map(|(u, p)| {
            let c = p.canonical.as_deref()?;
            let same = c.trim_end_matches('/') == u.trim_end_matches('/')
                || c.trim_end_matches('/') == p.final_url.as_str().trim_end_matches('/');
            (!same).then(|| json!({ "url": u, "canonical": c }))
        })
        .collect();
    o.push(
        "crawling.sitemap_canonical_mismatch",
        Crawling,
        "sitemap 표본 중 canonical 이 다른 URL 인 페이지",
        if na {
            Presence::NotApplicable
        } else {
            Presence::from_bool(!mismatched.is_empty())
        },
        Some(json!({ "checked": pages.len(), "mismatched": mismatched })),
        None,
    );
}

fn count_presence(n: usize, total: usize) -> Presence {
    if total == 0 {
        Presence::NotApplicable
    } else {
        Presence::from_bool(n > 0)
    }
}

fn same_url(a: &Url, b: &Url) -> bool {
    a.as_str().trim_end_matches('/') == b.as_str().trim_end_matches('/')
}

/// 호스트와 경로가 정확히 같은지 (끝 슬래시 차이도 구분, 쿼리·fragment 무시).
fn same_path(a: &Url, b: &Url) -> bool {
    a.host_str() == b.host_str() && a.path() == b.path()
}

fn primary_lang(s: &str) -> String {
    s.split(['-', '_'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// A·B 그룹 추가 항목 + 웹 접근성.
fn extended_checks(o: &mut Out, i: &Inputs, robots: &Result<Robots, String>) {
    use Category::*;
    let f = i.facts;
    let x = &f.extra;
    let page = i.page;
    let base = &page.final_url;
    let ext = i.extended;

    // ===== 기본 메타 =====
    let dups: serde_json::Map<String, Value> = x
        .tag_counts
        .iter()
        .filter(|(_, n)| **n > 1)
        .map(|(k, n)| (k.to_string(), json!(n)))
        .collect();
    o.push(
        "meta.duplicate_tags",
        Meta,
        "중복 태그 (title·description·canonical·viewport·robots)",
        Presence::from_bool(!dups.is_empty()),
        Some(json!(x.tag_counts)),
        Some("정적 HTML 전체 기준 (body 안 스트리밍 메타 태그 포함, SVG title 제외)".into()),
    );
    let locale = f.meta("og:locale");
    o.push(
        "meta.lang_locale_mismatch",
        Meta,
        "html lang ↔ og:locale 언어 불일치",
        match (f.lang.as_deref(), locale) {
            (Some(l), Some(loc)) => Presence::from_bool(primary_lang(l) != primary_lang(loc)),
            _ => Presence::NotApplicable,
        },
        Some(json!({ "lang": f.lang, "ogLocale": locale })),
        None,
    );
    o.opt(
        "meta.generator",
        Meta,
        "meta generator",
        f.meta("generator"),
    );
    let fw = signals::frameworks(&page.body);
    o.push(
        "meta.framework",
        Meta,
        "프레임워크 / CMS 흔적",
        Presence::from_bool(!fw.is_empty()),
        (!fw.is_empty()).then(|| json!(fw)),
        None,
    );

    // ===== Open Graph / Twitter =====
    for (id, key) in [
        ("image_width", "og:image:width"),
        ("image_height", "og:image:height"),
        ("image_alt", "og:image:alt"),
    ] {
        o.opt(&format!("open_graph.{id}"), OpenGraph, key, f.meta(key));
    }
    match &ext.og_image {
        None => o.push(
            "open_graph.image_file",
            OpenGraph,
            "og:image 실제 파일",
            Presence::NotApplicable,
            None,
            None,
        ),
        Some(Err(e)) => o.push(
            "open_graph.image_file",
            OpenGraph,
            "og:image 실제 파일",
            Presence::Unknown,
            None,
            Some(format!("요청 실패: {e}")),
        ),
        Some(Ok(img)) => {
            let decoded = img.width.is_some();
            o.push(
                "open_graph.image_file",
                OpenGraph,
                "og:image 실제 파일",
                Presence::from_bool((200..300).contains(&img.status) && decoded),
                Some(json!({
                    "url": img.url, "statusCode": img.status, "contentType": img.content_type,
                    "bytes": img.bytes, "truncated": img.truncated, "width": img.width, "height": img.height,
                })),
                (!decoded).then(|| "이미지 크기를 읽을 수 없음".to_string()),
            );
        }
    }
    o.opt(
        "twitter.site",
        Twitter,
        "twitter:site",
        f.meta("twitter:site"),
    );
    o.opt(
        "twitter.creator",
        Twitter,
        "twitter:creator",
        f.meta("twitter:creator"),
    );

    // ===== 구조화 데이터 =====
    let has_hreflang = !f.hreflang.is_empty();
    o.push(
        "structured_data.hreflang_x_default",
        StructuredData,
        "hreflang x-default",
        if has_hreflang {
            Presence::from_bool(
                f.hreflang
                    .iter()
                    .any(|h| h.lang.eq_ignore_ascii_case("x-default")),
            )
        } else {
            Presence::NotApplicable
        },
        None,
        None,
    );
    o.push(
        "structured_data.hreflang_self",
        StructuredData,
        "hreflang 자기 참조",
        if has_hreflang {
            Presence::from_bool(f.hreflang.iter().any(|h| {
                f.link_base(base)
                    .join(&h.href)
                    .is_ok_and(|u| same_url(&u, base))
            }))
        } else {
            Presence::NotApplicable
        },
        None,
        None,
    );
    for (id, types, required) in extra::SCHEMA_REQUIRED {
        let objs: Vec<&extra::LdObject> = x
            .ld_objects
            .iter()
            .filter(|ob| ob.types.iter().any(|t| types.contains(&t.as_str())))
            .collect();
        let label = format!("{} 스키마 필수 속성", types[0]);
        if objs.is_empty() {
            o.push(
                format!("structured_data.required.{id}"),
                StructuredData,
                label,
                Presence::NotApplicable,
                None,
                None,
            );
            continue;
        }
        let best: Vec<&str> = objs
            .iter()
            .map(|ob| {
                required
                    .iter()
                    .copied()
                    .filter(|p| !ob.has(p))
                    .collect::<Vec<_>>()
            })
            .min_by_key(Vec::len)
            .unwrap_or_default();
        o.push(
            format!("structured_data.required.{id}"),
            StructuredData,
            label,
            Presence::from_bool(best.is_empty()),
            Some(json!({ "required": required, "missing": best })),
            None,
        );
    }

    // ===== 본문 구조 =====
    o.push(
        "content.heading_skip",
        Content,
        "헤딩 레벨 건너뜀",
        count_presence(x.heading_skips.len(), x.heading_levels.len()),
        (!x.heading_skips.is_empty()).then(|| json!(x.heading_skips)),
        None,
    );
    o.push(
        "content.internal_nofollow",
        Content,
        "내부 링크 nofollow",
        Presence::from_bool(x.internal_nofollow > 0),
        Some(json!(x.internal_nofollow)),
        None,
    );
    if let Some(samples) = &ext.internal_samples {
        let broken: Vec<Value> = samples
            .iter()
            .filter_map(|(u, r)| match r {
                Ok((st, _)) if (200..300).contains(st) => None,
                Ok((st, _)) => Some(json!({ "url": u, "statusCode": st })),
                Err(e) => Some(json!({ "url": u, "error": e })),
            })
            .collect();
        o.push(
            "content.broken_internal_links",
            Content,
            "내부 링크 표본 중 비정상 응답",
            if samples.is_empty() {
                Presence::NotApplicable
            } else {
                Presence::from_bool(!broken.is_empty())
            },
            Some(json!({ "sampled": samples.len(), "broken": broken })),
            None,
        );
    }

    // ===== 성능 =====
    o.push(
        "performance.render_blocking_scripts",
        Performance,
        "head 렌더링 차단 스크립트 (async/defer 없음)",
        Presence::from_bool(!x.render_blocking_scripts.is_empty()),
        (!x.render_blocking_scripts.is_empty()).then(|| json!(x.render_blocking_scripts)),
        None,
    );
    o.push(
        "performance.head_stylesheets",
        Performance,
        "head 외부 스타일시트",
        Presence::from_bool(x.head_stylesheets > 0),
        Some(json!(x.head_stylesheets)),
        None,
    );
    o.push(
        "performance.preconnect",
        Performance,
        "preconnect / dns-prefetch",
        Presence::from_bool(!x.preconnect.is_empty()),
        (!x.preconnect.is_empty()).then(|| json!(x.preconnect)),
        None,
    );
    let modern = x
        .image_formats
        .iter()
        .filter(|(k, _)| matches!(k.as_str(), "webp" | "avif"))
        .map(|(_, n)| n)
        .sum::<usize>();
    o.push(
        "performance.modern_image_formats",
        Performance,
        "WebP / AVIF 이미지",
        count_presence(modern, x.image_formats.values().sum()),
        (!x.image_formats.is_empty()).then(|| json!(x.image_formats)),
        Some(
            "파일 확장자 · <picture> source type 기준. 서버 협상으로 변환되는 경우는 반영되지 않음"
                .into(),
        ),
    );
    o.push(
        "performance.img_srcset",
        Performance,
        "srcset 이미지",
        count_presence(x.img_srcset, f.img_total),
        Some(json!({ "total": f.img_total, "srcset": x.img_srcset })),
        None,
    );

    // ===== 전송 =====
    let v = page.version.as_str();
    o.push(
        "transport.http2",
        Transport,
        "HTTP/2 이상",
        Presence::from_bool(v == "HTTP/2.0" || v == "HTTP/3.0"),
        Some(json!(v)),
        None,
    );
    let alt_svc = header(&page.headers, "alt-svc");
    o.push(
        "transport.http3_advertised",
        Transport,
        "HTTP/3 광고 (Alt-Svc h3)",
        Presence::from_bool(alt_svc.as_deref().is_some_and(|a| a.contains("h3"))),
        alt_svc.map(Value::from),
        None,
    );
    if let Some(tls) = &ext.tls {
        let label = "TLS 인증서";
        match tls {
            Ok(None) => o.push(
                "transport.tls_certificate",
                Transport,
                label,
                Presence::NotApplicable,
                None,
                Some("https 아님".into()),
            ),
            Ok(Some(c)) => o.push(
                "transport.tls_certificate",
                Transport,
                label,
                Presence::Present,
                Some(json!({
                    "subject": c.subject, "issuer": c.issuer, "notAfter": c.not_after_text,
                    "daysRemaining": (c.not_after - ext.now).div_euclid(86_400),
                })),
                None,
            ),
            Err(e) => o.push(
                "transport.tls_certificate",
                Transport,
                label,
                Presence::Unknown,
                None,
                Some(format!("확인 실패: {e}")),
            ),
        }
    }
    for variant in &ext.url_variants {
        let label = match variant.kind {
            "trailing_slash" => "끝 슬래시 변형 중복 노출",
            "uppercase" => "대문자 경로 변형 중복 노출",
            _ => "index.html 변형 중복 노출",
        };
        let id = format!("transport.url_variant.{}", variant.kind);
        match &variant.result {
            Ok((status, final_url)) => o.push(
                id,
                Transport,
                label,
                Presence::from_bool((200..300).contains(status) && !same_path(final_url, base)),
                Some(json!({ "variant": variant.url.as_str(), "statusCode": status, "finalUrl": final_url.as_str() })),
                None,
            ),
            Err(e) => o.push(id, Transport, label, Presence::Unknown, Some(json!({ "variant": variant.url.as_str() })), Some(format!("요청 실패: {e}"))),
        }
    }

    // ===== 보안 헤더 =====
    for (id, name, label) in [
        ("csp", "content-security-policy", "Content-Security-Policy"),
        (
            "x_content_type_options",
            "x-content-type-options",
            "X-Content-Type-Options",
        ),
        ("x_frame_options", "x-frame-options", "X-Frame-Options"),
        ("referrer_policy", "referrer-policy", "Referrer-Policy"),
        (
            "permissions_policy",
            "permissions-policy",
            "Permissions-Policy",
        ),
    ] {
        o.opt(
            &format!("security.{id}"),
            Security,
            label,
            header(&page.headers, name).as_deref(),
        );
    }

    // ===== 크롤링 =====
    match robots {
        Ok(r) if !x.same_origin_resources.is_empty() => {
            let blocked: Vec<&String> = x
                .same_origin_resources
                .iter()
                .filter(|p| !r.is_allowed("Googlebot", p))
                .collect();
            o.push(
                "crawling.robots_blocks_resources",
                Crawling,
                "robots.txt 가 CSS/JS 차단",
                Presence::from_bool(!blocked.is_empty()),
                (!blocked.is_empty()).then(|| json!(blocked)),
                Some("Googlebot 기준, 같은 호스트의 script/stylesheet".into()),
            );
        }
        Ok(_) => o.push(
            "crawling.robots_blocks_resources",
            Crawling,
            "robots.txt 가 CSS/JS 차단",
            Presence::NotApplicable,
            None,
            None,
        ),
        Err(e) => o.push(
            "crawling.robots_blocks_resources",
            Crawling,
            "robots.txt 가 CSS/JS 차단",
            Presence::Unknown,
            None,
            Some(e.clone()),
        ),
    }

    // ===== GEO =====
    o.opt(
        "geo.author",
        Geo,
        "작성자 (meta / JSON-LD)",
        x.author.as_deref(),
    );
    o.opt(
        "geo.published_date",
        Geo,
        "게시일 (article:published_time / datePublished)",
        x.published.as_deref(),
    );
    o.opt(
        "geo.modified_date",
        Geo,
        "수정일 (article:modified_time / dateModified)",
        x.modified.as_deref(),
    );
    o.opt(
        "geo.markdown_alternate",
        Geo,
        "마크다운 대체 링크 (rel=alternate text/markdown)",
        x.markdown_alternate.as_deref(),
    );
    if let Some(md) = &ext.markdown_version {
        let (p, n) = file_presence(md);
        o.push(
            "geo.markdown_version",
            Geo,
            "페이지 .md 버전",
            p,
            Some(file_value(md)),
            n,
        );
    }

    // ===== 웹 접근성 =====
    let a = &f.a11y;
    o.push(
        "accessibility.unlabeled_form_controls",
        Accessibility,
        "레이블 없는 입력 요소",
        count_presence(a.unlabeled_controls.len(), a.form_controls),
        Some(json!({ "total": a.form_controls, "unlabeled": a.unlabeled_controls })),
        None,
    );
    o.push(
        "accessibility.unnamed_buttons",
        Accessibility,
        "이름 없는 버튼",
        count_presence(a.unnamed_buttons, a.buttons),
        Some(json!({ "total": a.buttons, "unnamed": a.unnamed_buttons })),
        None,
    );
    o.push(
        "accessibility.empty_links",
        Accessibility,
        "이름(텍스트) 없는 링크",
        count_presence(a.empty_links, a.links),
        Some(json!({ "total": a.links, "empty": a.empty_links })),
        None,
    );
    for (id, label, has) in [
        ("landmark_main", "main 랜드마크", a.has_main),
        ("landmark_nav", "nav 랜드마크", a.has_nav),
        ("landmark_header", "header(banner) 랜드마크", a.has_header),
        (
            "landmark_footer",
            "footer(contentinfo) 랜드마크",
            a.has_footer,
        ),
    ] {
        o.push(
            format!("accessibility.{id}"),
            Accessibility,
            label,
            Presence::from_bool(has),
            None,
            None,
        );
    }
    o.opt(
        "accessibility.skip_link",
        Accessibility,
        "건너뛰기 링크",
        a.skip_link.as_deref(),
    );
    o.push(
        "accessibility.iframe_title_missing",
        Accessibility,
        "title 없는 iframe",
        count_presence(a.iframes_without_title, a.iframes),
        Some(json!({ "total": a.iframes, "missing": a.iframes_without_title })),
        None,
    );
    o.push(
        "accessibility.duplicate_ids",
        Accessibility,
        "중복 id",
        Presence::from_bool(!a.duplicate_ids.is_empty()),
        (!a.duplicate_ids.is_empty())
            .then(|| json!(a.duplicate_ids.iter().take(20).collect::<Vec<_>>())),
        (a.duplicate_ids.len() > 20)
            .then(|| format!("총 {}개 중 20개만 표시", a.duplicate_ids.len())),
    );
    o.push(
        "accessibility.table_headers_missing",
        Accessibility,
        "제목 셀(th) 없는 표",
        count_presence(a.tables_without_headers, a.tables),
        Some(json!({ "total": a.tables, "missing": a.tables_without_headers })),
        None,
    );
    o.push(
        "accessibility.autoplay_media",
        Accessibility,
        "자동 재생 미디어",
        Presence::from_bool(a.autoplay_media > 0),
        Some(json!(a.autoplay_media)),
        None,
    );
    o.push(
        "accessibility.positive_tabindex",
        Accessibility,
        "양수 tabindex",
        Presence::from_bool(a.positive_tabindex > 0),
        Some(json!(a.positive_tabindex)),
        None,
    );
    o.push(
        "accessibility.obsolete_elements",
        Accessibility,
        "폐기된 HTML 요소 (marquee·font·center 등)",
        Presence::from_bool(a.obsolete_elements > 0),
        Some(json!(a.obsolete_elements)),
        None,
    );
    o.push(
        "accessibility.new_window_links",
        Accessibility,
        "새 창 링크 (target=_blank)",
        Presence::from_bool(a.new_window_links > 0),
        Some(json!(a.new_window_links)),
        None,
    );
    let viewport = f.meta("viewport");
    o.push(
        "accessibility.zoom_disabled",
        Accessibility,
        "확대 금지 viewport",
        viewport.map_or(Presence::NotApplicable, |v| {
            Presence::from_bool(a11y::zoom_disabled(v))
        }),
        viewport.map(Value::from),
        None,
    );
}

/// UNIX 초 → 그레고리력 연도 (UTC).
pub fn year_of(unix: i64) -> i32 {
    // Howard Hinnant, civil_from_days
    let z = unix.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2)) as i32
}

fn list_check(
    o: &mut Out,
    id: &str,
    cat: Category,
    label: &str,
    items: &[String],
    applicable: bool,
) {
    o.push(
        id,
        cat,
        label,
        if applicable {
            Presence::from_bool(!items.is_empty())
        } else {
            Presence::NotApplicable
        },
        (!items.is_empty()).then(|| json!(items.iter().take(20).collect::<Vec<_>>())),
        (items.len() > 20).then(|| format!("총 {}개 중 20개만 표시", items.len())),
    );
}

/// 5차 추가: 접근성 심화, 신뢰 정보, 파일·메타, 헤더·리소스, 크롤링·GEO 세부.
fn more_checks(o: &mut Out, i: &Inputs, robots: &Result<Robots, String>) {
    use Category::*;
    let f = i.facts;
    let x = &f.extra;
    let a = &f.a11y;
    let t = &f.trust;
    let page = i.page;
    let ext = i.extended;
    let wk = i.well_known;

    // ===== 웹 접근성 심화 =====
    list_check(
        o,
        "accessibility.vague_link_text",
        Accessibility,
        "목적을 알 수 없는 링크 텍스트",
        &a.vague_link_texts,
        a.links > 0,
    );
    list_check(
        o,
        "accessibility.filename_alt",
        Accessibility,
        "파일명 형태의 alt",
        &a.filename_alts,
        f.img_total > 0,
    );
    o.push(
        "accessibility.long_alt",
        Accessibility,
        "125자 초과 alt",
        count_presence(a.long_alts, f.img_total),
        Some(json!(a.long_alts)),
        None,
    );
    list_check(
        o,
        "accessibility.broken_id_refs",
        Accessibility,
        "없는 id 를 가리키는 참조 (for·aria-*)",
        &a.broken_id_refs,
        true,
    );
    o.push(
        "accessibility.focusable_aria_hidden",
        Accessibility,
        "aria-hidden 안의 초점 가능 요소",
        Presence::from_bool(a.focusable_aria_hidden > 0),
        Some(json!(a.focusable_aria_hidden)),
        None,
    );
    o.push(
        "accessibility.empty_headings",
        Accessibility,
        "빈 헤딩",
        count_presence(a.empty_headings, a.headings),
        Some(json!({ "total": a.headings, "empty": a.empty_headings })),
        None,
    );
    list_check(
        o,
        "accessibility.autocomplete_missing",
        Accessibility,
        "개인정보 입력란 autocomplete 누락",
        &a.autocomplete_missing,
        a.form_controls > 0,
    );
    o.push(
        "accessibility.html_lang_valid",
        Accessibility,
        "html lang 형식 (BCP 47)",
        f.lang.as_deref().map_or(Presence::NotApplicable, |l| {
            Presence::from_bool(a11y::valid_lang(l))
        }),
        f.lang.as_deref().map(Value::from),
        None,
    );
    o.push(
        "accessibility.outline_removed",
        Accessibility,
        "초점 표시 제거 (outline: none)",
        Presence::from_bool(a.outline_removed + ext.external_css.as_ref().map_or(0, |c| c.facts.outline_removed) > 0),
        Some(json!({
            "inline": a.outline_removed,
            "external": ext.external_css.as_ref().map(|c| c.facts.outline_removed),
            "cssFilesChecked": ext.external_css.as_ref().map(|c| c.checked),
            "hasFocusVisible": a.has_focus_visible || ext.external_css.as_ref().is_some_and(|c| c.facts.has_focus_visible),
        })),
        Some("HTML 안 <style>·style 속성 + 외부 CSS 표본 기준. 선택자 문맥(:focus 대체 스타일 여부)은 보지 않음".into()),
    );

    // ===== 신뢰·법적 정보 =====
    o.opt(
        "trust.privacy_policy",
        Trust,
        "개인정보처리방침 링크",
        t.privacy_policy.as_deref(),
    );
    o.opt("trust.terms", Trust, "이용약관 링크", t.terms.as_deref());
    o.push(
        "trust.phone",
        Trust,
        "전화번호 링크 (tel:)",
        Presence::from_bool(!t.tel.is_empty()),
        (!t.tel.is_empty()).then(|| json!(t.tel)),
        None,
    );
    o.push(
        "trust.email",
        Trust,
        "이메일 링크 (mailto:)",
        Presence::from_bool(!t.mailto.is_empty()),
        (!t.mailto.is_empty()).then(|| json!(t.mailto)),
        None,
    );
    let postal = x
        .ld_objects
        .iter()
        .any(|ob| ob.types.iter().any(|ty| ty == "PostalAddress") || ob.has("address"));
    o.push(
        "trust.address",
        Trust,
        "주소 (<address> 또는 스키마 address)",
        Presence::from_bool(t.has_address_element || postal),
        Some(json!({ "addressElement": t.has_address_element, "schema": postal })),
        None,
    );
    o.push(
        "trust.social_links",
        Trust,
        "SNS 링크",
        Presence::from_bool(!t.social.is_empty()),
        (!t.social.is_empty()).then(|| json!(t.social)),
        None,
    );
    o.opt(
        "trust.business_number",
        Trust,
        "사업자등록번호 표기",
        t.business_number.as_deref(),
    );
    o.push(
        "trust.copyright_year",
        Trust,
        "저작권 연도 표기",
        Presence::from_bool(t.copyright_year.is_some()),
        t.copyright_year
            .map(|y| json!({ "year": y, "currentYear": year_of(ext.now) })),
        None,
    );

    // ===== 파일·메타 =====
    o.opt(
        "meta.bing_site_verification",
        Meta,
        "Bing 웹마스터 인증",
        f.meta("msvalidate.01"),
    );
    o.opt(
        "meta.amphtml",
        Meta,
        "AMP 링크 (rel=amphtml)",
        x.amphtml.as_deref(),
    );
    o.push(
        "meta.app_link_meta",
        Meta,
        "앱 연결 메타 (apple-itunes-app·al:*)",
        Presence::from_bool(!x.app_link_meta.is_empty()),
        (!x.app_link_meta.is_empty()).then(|| json!(x.app_link_meta)),
        None,
    );
    for (id, label, probe) in [
        (
            "meta.apple_app_site_association",
            "/.well-known/apple-app-site-association",
            &wk.apple_app_site_association,
        ),
        (
            "meta.assetlinks",
            "/.well-known/assetlinks.json",
            &wk.assetlinks,
        ),
    ] {
        let (p, n) = file_presence(probe);
        o.push(id, Meta, label, p, Some(file_value(probe)), n);
    }
    match &ext.manifest {
        None => {
            o.push(
                "meta.manifest_content",
                Meta,
                "manifest 내용",
                Presence::NotApplicable,
                None,
                None,
            );
            o.push(
                "meta.manifest_icons_192_512",
                Meta,
                "manifest 192·512px 아이콘",
                Presence::NotApplicable,
                None,
                None,
            );
        }
        Some(Err(e)) => {
            o.push(
                "meta.manifest_content",
                Meta,
                "manifest 내용",
                Presence::Unknown,
                None,
                Some(format!("요청 실패: {e}")),
            );
            o.push(
                "meta.manifest_icons_192_512",
                Meta,
                "manifest 192·512px 아이콘",
                Presence::Unknown,
                None,
                None,
            );
        }
        Some(Ok((status, json))) => {
            let get = |k: &str| {
                json.as_ref()
                    .and_then(|j| j.get(k))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            };
            let sizes: Vec<String> = json
                .as_ref()
                .and_then(|j| j.get("icons"))
                .and_then(Value::as_array)
                .map(|icons| {
                    icons
                        .iter()
                        .filter_map(|i| i.get("sizes").and_then(Value::as_str))
                        .flat_map(|s| s.split_ascii_whitespace().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            o.push(
                "meta.manifest_content",
                Meta,
                "manifest 내용",
                Presence::from_bool(json.is_some()),
                Some(json!({
                    "statusCode": status, "name": get("name"), "shortName": get("short_name"),
                    "startUrl": get("start_url"), "display": get("display"), "iconSizes": sizes,
                })),
                json.is_none()
                    .then(|| "JSON 으로 해석할 수 없음".to_string()),
            );
            let has = |s: &str| sizes.iter().any(|x| x == s);
            o.push(
                "meta.manifest_icons_192_512",
                Meta,
                "manifest 192·512px 아이콘",
                if json.is_some() {
                    Presence::from_bool(has("192x192") && has("512x512"))
                } else {
                    Presence::NotApplicable
                },
                None,
                None,
            );
        }
    }
    match &ext.favicon_image {
        None => {}
        Some(Err(e)) => o.push(
            "meta.favicon_size",
            Meta,
            "favicon 실제 크기",
            Presence::Unknown,
            None,
            Some(format!("요청 실패: {e}")),
        ),
        Some(Ok(img)) => {
            let vector = img
                .content_type
                .as_deref()
                .is_some_and(|c| c.contains("svg"))
                || img.url.to_ascii_lowercase().ends_with(".svg");
            let ok = (200..300).contains(&img.status) && (img.width.is_some() || vector);
            o.push(
                "meta.favicon_size",
                Meta,
                "favicon 실제 크기",
                Presence::from_bool(ok),
                Some(json!({ "url": img.url, "statusCode": img.status, "width": img.width, "height": img.height, "vector": vector, "bytes": img.bytes })),
                (!ok).then(|| "이미지 크기를 읽을 수 없음".to_string()),
            );
        }
    }

    // ===== 응답 헤더·리소스 =====
    let h = &page.headers;
    o.opt(
        "crawling.etag",
        Crawling,
        "ETag 헤더",
        header(h, "etag").as_deref(),
    );
    o.opt(
        "crawling.last_modified",
        Crawling,
        "Last-Modified 헤더",
        header(h, "last-modified").as_deref(),
    );
    let vary = header(h, "vary");
    o.push(
        "crawling.vary_user_agent",
        Crawling,
        "Vary: User-Agent",
        Presence::from_bool(
            vary.as_deref()
                .is_some_and(|v| v.to_ascii_lowercase().contains("user-agent")),
        ),
        vary.map(Value::from),
        None,
    );
    o.opt(
        "security.server_header",
        Security,
        "Server 헤더 노출",
        header(h, "server").as_deref(),
    );
    o.opt(
        "security.x_powered_by",
        Security,
        "X-Powered-By 헤더 노출",
        header(h, "x-powered-by").as_deref(),
    );
    let cookies: Vec<Value> = h
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|c| {
            let mut parts = c.split(';');
            let name = parts.next().unwrap_or_default().split('=').next().unwrap_or_default().trim().to_string();
            let attrs: Vec<String> = parts.map(|p| p.trim().to_ascii_lowercase()).collect();
            json!({
                "name": name,
                "secure": attrs.iter().any(|a| a == "secure"),
                "httpOnly": attrs.iter().any(|a| a == "httponly"),
                "sameSite": attrs.iter().find_map(|a| a.strip_prefix("samesite=").map(str::to_string)),
            })
        })
        .collect();
    let weak = cookies
        .iter()
        .any(|c| c["secure"] == false || c["httpOnly"] == false || c["sameSite"].is_null());
    o.push(
        "security.insecure_cookies",
        Security,
        "Secure·HttpOnly·SameSite 누락 쿠키",
        if cookies.is_empty() {
            Presence::NotApplicable
        } else {
            Presence::from_bool(weak)
        },
        (!cookies.is_empty()).then(|| json!(cookies)),
        None,
    );
    if let Some((url, r)) = &ext.static_asset {
        match r {
            Ok((status, headers)) => {
                let cc = header(headers, "cache-control");
                let max_age = cc.as_deref().and_then(|c| {
                    c.split(',').find_map(|d| {
                        d.trim()
                            .strip_prefix("max-age=")
                            .and_then(|v| v.parse::<u64>().ok())
                    })
                });
                o.push(
                    "performance.static_asset_cache",
                    Performance,
                    "정적 리소스 Cache-Control",
                    Presence::from_bool(cc.is_some()),
                    Some(json!({
                        "url": url, "statusCode": status, "cacheControl": cc, "maxAge": max_age,
                        "immutable": cc.as_deref().is_some_and(|c| c.contains("immutable")),
                    })),
                    None,
                );
            }
            Err(e) => o.push(
                "performance.static_asset_cache",
                Performance,
                "정적 리소스 Cache-Control",
                Presence::Unknown,
                Some(json!({ "url": url })),
                Some(format!("요청 실패: {e}")),
            ),
        }
    }
    o.push(
        "performance.dom_size",
        Performance,
        "DOM 크기",
        Presence::Present,
        Some(json!({ "elements": x.dom_elements, "maxDepth": x.dom_depth })),
        None,
    );
    o.push(
        "performance.third_party_scripts",
        Performance,
        "서드파티 스크립트 호스트",
        Presence::from_bool(!x.third_party_script_hosts.is_empty()),
        (!x.third_party_script_hosts.is_empty()).then(|| json!(x.third_party_script_hosts)),
        None,
    );

    // ===== 크롤링 세부 =====
    let (p, n) = file_presence(&wk.ads_txt);
    o.push(
        "crawling.ads_txt",
        Crawling,
        "ads.txt",
        p,
        Some(file_value(&wk.ads_txt)),
        n,
    );
    if i.robots.exists {
        let size = i.robots.size.unwrap_or_default();
        o.push(
            "crawling.robots_oversize",
            Crawling,
            "robots.txt 500KiB 초과",
            Presence::from_bool(size > 500 * 1024),
            Some(json!(size)),
            None,
        );
    } else {
        o.push(
            "crawling.robots_oversize",
            Crawling,
            "robots.txt 500KiB 초과",
            Presence::NotApplicable,
            None,
            None,
        );
    }
    match robots {
        Ok(r) if i.robots.exists => {
            let delays: Vec<Value> = r
                .crawl_delays
                .iter()
                .map(|(ua, v)| json!({ "userAgent": ua, "value": v }))
                .collect();
            o.push(
                "crawling.robots_crawl_delay",
                Crawling,
                "robots.txt Crawl-delay",
                Presence::from_bool(!delays.is_empty()),
                (!delays.is_empty()).then(|| json!(delays)),
                None,
            );
            list_check(
                o,
                "crawling.robots_invalid_lines",
                Crawling,
                "robots.txt 해석 불가 줄",
                &r.invalid_lines,
                true,
            );
        }
        _ => {
            o.push(
                "crawling.robots_crawl_delay",
                Crawling,
                "robots.txt Crawl-delay",
                Presence::NotApplicable,
                None,
                None,
            );
            o.push(
                "crawling.robots_invalid_lines",
                Crawling,
                "robots.txt 해석 불가 줄",
                Presence::NotApplicable,
                None,
                None,
            );
        }
    }
    if let Some(sm) = pick_sitemap(i.sitemap, &i.sitemap_detail.declared)
        && !is_gz(sm)
    {
        let size = sm.size.unwrap_or_default();
        let entries = sitemap::summarize(sm.body.as_deref().unwrap_or_default()).entries;
        o.push(
            "crawling.sitemap_oversize",
            Crawling,
            "sitemap 한도 초과 (50MB 또는 5만 URL)",
            Presence::from_bool(size > 50 * 1024 * 1024 || entries > 50_000),
            Some(json!({ "url": sm.url, "bytes": size, "entries": entries })),
            None,
        );
    }

    // ===== GEO 세부 =====
    o.push(
        "geo.question_headings",
        Geo,
        "질문형 헤딩 (h2~h4)",
        Presence::from_bool(!x.question_headings.is_empty()),
        (!x.question_headings.is_empty())
            .then(|| json!(x.question_headings.iter().take(20).collect::<Vec<_>>())),
        None,
    );
    o.push(
        "geo.lists",
        Geo,
        "목록 (항목 2개 이상 ul/ol, dl)",
        Presence::from_bool(x.lists + x.definition_lists > 0),
        Some(json!({ "lists": x.lists, "definitionLists": x.definition_lists })),
        None,
    );
    o.push(
        "geo.tables",
        Geo,
        "표",
        Presence::from_bool(x.tables > 0),
        Some(json!(x.tables)),
        None,
    );
    o.push(
        "geo.noscript_content",
        Geo,
        "noscript 대체 콘텐츠",
        Presence::from_bool(x.noscript_text_len > 0),
        Some(json!({ "elements": x.noscript_elements, "textLength": x.noscript_text_len })),
        None,
    );
    let speakable = x.ld_objects.iter().any(|ob| ob.has("speakable"));
    o.push(
        "structured_data.speakable",
        StructuredData,
        "speakable 속성",
        Presence::from_bool(speakable),
        None,
        None,
    );
}

/// 같은 값을 가진 페이지 묶음 (2개 이상인 것만).
fn duplicate_groups<'a>(items: impl Iterator<Item = (&'a str, &'a str)>) -> Vec<Value> {
    let mut groups: Vec<(&str, Vec<&str>)> = Vec::new();
    for (value, url) in items {
        match groups.iter_mut().find(|(v, _)| *v == value) {
            Some((_, urls)) => urls.push(url),
            None => groups.push((value, vec![url])),
        }
    }
    groups
        .into_iter()
        .filter(|(_, urls)| urls.len() > 1)
        .map(|(v, urls)| json!({ "value": v, "urls": urls }))
        .collect()
}

/// 여러 페이지 표본: 대상 페이지 + 표본 페이지들 사이의 중복·누락.
fn site_checks(o: &mut Out, i: &Inputs, site: &SiteSample) {
    use Category::Site;
    let f = i.facts;
    let main_url = i.page.final_url.as_str();
    let ok: Vec<(&str, &SamplePage)> = site
        .pages
        .iter()
        .filter_map(|(u, r)| {
            r.as_ref()
                .ok()
                .filter(|p| (200..300).contains(&p.status))
                .map(|p| (u.as_str(), p))
        })
        .collect();
    let failed: Vec<Value> = site
        .pages
        .iter()
        .filter_map(|(u, r)| match r {
            Ok(p) if (200..300).contains(&p.status) => None,
            Ok(p) => Some(json!({ "url": u, "statusCode": p.status })),
            Err(e) => Some(json!({ "url": u, "error": e })),
        })
        .collect();
    o.push(
        "site.pages_sampled",
        Site,
        "표본 페이지 수집",
        if site.pages.is_empty() {
            Presence::NotApplicable
        } else {
            Presence::from_bool(!ok.is_empty())
        },
        Some(json!({
            "source": site.source,
            "requested": site.pages.len(),
            "ok": ok.len(),
            "urls": site.pages.iter().map(|(u, _)| u).collect::<Vec<_>>(),
        })),
        site.pages
            .is_empty()
            .then(|| "표본으로 쓸 같은 호스트 URL 이 없음".to_string()),
    );
    if site.pages.is_empty() {
        return;
    }
    let na = ok.is_empty();
    let p = |b: bool| {
        if na {
            Presence::NotApplicable
        } else {
            Presence::from_bool(b)
        }
    };
    o.push(
        "site.failed_pages",
        Site,
        "표본 중 2xx 아닌 페이지",
        Presence::from_bool(!failed.is_empty()),
        (!failed.is_empty()).then(|| json!(failed)),
        None,
    );

    // 대상 페이지 포함 비교
    let titles = std::iter::once((f.title.as_deref(), main_url))
        .chain(ok.iter().map(|(u, pg)| (pg.title.as_deref(), *u)));
    let dup_titles = duplicate_groups(titles.clone().filter_map(|(t, u)| t.map(|t| (t, u))));
    o.push(
        "site.duplicate_titles",
        Site,
        "페이지 간 중복 title",
        p(!dup_titles.is_empty()),
        (!dup_titles.is_empty()).then(|| json!(dup_titles)),
        Some("대상 페이지 포함".into()),
    );
    let descs = std::iter::once((f.meta("description"), main_url))
        .chain(ok.iter().map(|(u, pg)| (pg.description.as_deref(), *u)));
    let dup_descs = duplicate_groups(descs.clone().filter_map(|(d, u)| d.map(|d| (d, u))));
    o.push(
        "site.duplicate_descriptions",
        Site,
        "페이지 간 중복 description",
        p(!dup_descs.is_empty()),
        (!dup_descs.is_empty()).then(|| json!(dup_descs)),
        Some("대상 페이지 포함".into()),
    );

    let urls_where = |pred: &dyn Fn(&SamplePage) -> bool| -> Vec<&str> {
        ok.iter()
            .filter(|(_, pg)| pred(pg))
            .map(|(u, _)| *u)
            .collect()
    };
    for (id, label, list) in [
        (
            "site.missing_titles",
            "title 없는 페이지",
            urls_where(&|pg| pg.title.is_none()),
        ),
        (
            "site.missing_descriptions",
            "description 없는 페이지",
            urls_where(&|pg| pg.description.is_none()),
        ),
        (
            "site.missing_h1",
            "h1 없는 페이지",
            urls_where(&|pg| pg.h1_count == 0),
        ),
        (
            "site.multiple_h1",
            "h1 이 2개 이상인 페이지",
            urls_where(&|pg| pg.h1_count > 1),
        ),
        (
            "site.noindex_pages",
            "noindex 페이지",
            urls_where(&|pg| pg.noindex),
        ),
        (
            "site.img_alt_missing_pages",
            "alt 누락 이미지가 있는 페이지",
            urls_where(&|pg| pg.img_without_alt > 0),
        ),
    ] {
        o.push(
            id,
            Site,
            label,
            p(!list.is_empty()),
            Some(json!({ "checked": ok.len(), "urls": list })),
            None,
        );
    }
    let (imgs, missing) = ok.iter().fold((0, 0), |(t, m), (_, pg)| {
        (t + pg.img_total, m + pg.img_without_alt)
    });
    o.push(
        "site.img_alt_missing_total",
        Site,
        "표본 전체 alt 누락 이미지",
        if na || imgs == 0 {
            Presence::NotApplicable
        } else {
            Presence::from_bool(missing > 0)
        },
        Some(json!({ "images": imgs, "missingAlt": missing })),
        None,
    );
    let mut lens: Vec<usize> = ok.iter().map(|(_, pg)| pg.body_text_len).collect();
    lens.sort_unstable();
    o.push(
        "site.body_text_lengths",
        Site,
        "표본 페이지 정적 body 텍스트 길이",
        p(lens.iter().any(|l| *l > 0)),
        (!na).then(|| json!({
            "min": lens.first(), "median": lens.get(lens.len() / 2), "max": lens.last(),
            "pages": ok.iter().map(|(u, pg)| json!({ "url": u, "length": pg.body_text_len })).collect::<Vec<_>>(),
        })),
        None,
    );
    let mut langs: Vec<&str> = std::iter::once(f.lang.as_deref())
        .chain(ok.iter().map(|(_, pg)| pg.lang.as_deref()))
        .flatten()
        .collect();
    langs.sort_unstable();
    langs.dedup();
    o.push(
        "site.lang_variants",
        Site,
        "페이지마다 다른 html lang",
        p(langs.len() > 1),
        (!langs.is_empty()).then(|| json!(langs)),
        Some("다국어 사이트라면 정상일 수 있음".into()),
    );
}

/// 추가 접근성·성능 항목 (HTML 정적 분석 + 이미 받은 리소스 크기).
fn deep_checks(o: &mut Out, i: &Inputs) {
    use Category::*;
    let m = &i.facts.more;
    let list = |v: &[String]| (!v.is_empty()).then(|| json!(v));

    // ===== CSR 셸 =====
    let text_len = i.facts.body_text_len;
    o.push(
        "content.csr_shell",
        Content,
        "CSR 셸 추정 (정적 HTML 본문이 거의 없음)",
        Presence::from_bool(text_len < CSR_SHELL_TEXT_LEN && m.script_srcs > 0),
        Some(json!({ "bodyTextLength": text_len, "scripts": m.script_srcs })),
        Some(format!(
            "본문 텍스트 {CSR_SHELL_TEXT_LEN}자 미만 + 외부 스크립트 기준. present 면 본문·접근성 항목이 브라우저 렌더링 결과와 다를 수 있음"
        )),
    );

    // ===== 접근성 =====
    o.push(
        "accessibility.invalid_aria_role",
        Accessibility,
        "ARIA 명세에 없는 role",
        count_presence(m.invalid_roles.len(), m.role_elements),
        list(&m.invalid_roles),
        None,
    );
    o.push(
        "accessibility.invalid_aria_attr",
        Accessibility,
        "ARIA 명세에 없는 aria-* 속성",
        count_presence(m.invalid_aria_attrs.len(), m.aria_attr_count),
        list(&m.invalid_aria_attrs),
        None,
    );
    o.push(
        "accessibility.role_img_unnamed",
        Accessibility,
        "이름 없는 role=\"img\"",
        count_presence(m.role_img_unnamed, m.role_img),
        Some(json!({ "total": m.role_img, "unnamed": m.role_img_unnamed })),
        None,
    );
    o.push(
        "accessibility.video_captions_missing",
        Accessibility,
        "자막 트랙 없는 video",
        count_presence(m.videos_without_captions, m.videos),
        Some(json!({ "total": m.videos, "missing": m.videos_without_captions })),
        Some("<track kind=captions|subtitles> 기준, 소리 없는(muted) 영상 제외. 영상에 자막이 입혀진(open caption) 경우는 알 수 없음".into()),
    );
    o.push(
        "accessibility.area_alt_missing",
        Accessibility,
        "alt 없는 이미지맵 area",
        count_presence(m.areas_without_alt, m.areas),
        Some(json!({ "total": m.areas, "missing": m.areas_without_alt })),
        None,
    );
    o.push(
        "accessibility.object_alt_missing",
        Accessibility,
        "대체 텍스트 없는 object/embed",
        count_presence(m.objects_without_alt, m.objects),
        Some(json!({ "total": m.objects, "missing": m.objects_without_alt })),
        None,
    );
    o.push(
        "accessibility.element_lang_invalid",
        Accessibility,
        "본문 요소의 잘못된 lang 값",
        count_presence(m.invalid_element_langs.len(), m.element_langs),
        list(&m.invalid_element_langs),
        None,
    );
    o.push(
        "accessibility.nested_interactive",
        Accessibility,
        "링크·버튼 안의 인터랙티브 요소",
        count_presence(m.nested_interactive, m.interactive_containers),
        Some(json!({ "containers": m.interactive_containers, "nested": m.nested_interactive })),
        None,
    );
    o.push(
        "accessibility.list_structure_invalid",
        Accessibility,
        "잘못된 목록 구조 (ul/ol 밖 li 등)",
        count_presence(m.list_violations, m.list_elements),
        Some(json!(m.list_violations)),
        None,
    );
    o.push(
        "accessibility.aria_hidden_root",
        Accessibility,
        "html/body 에 aria-hidden=\"true\"",
        Presence::from_bool(m.aria_hidden_root),
        None,
        None,
    );
    let refresh_delay = i
        .facts
        .meta_refresh
        .as_deref()
        .and_then(signals::refresh_delay);
    o.push(
        "accessibility.timed_refresh",
        Accessibility,
        "시간 제한 자동 새로고침 (meta refresh 지연)",
        Presence::from_bool(refresh_delay.is_some_and(|d| d > 0.0)),
        refresh_delay.map(|d| json!(d)),
        None,
    );
    o.push(
        "accessibility.duplicate_accesskeys",
        Accessibility,
        "중복 accesskey",
        count_presence(m.duplicate_accesskeys.len(), m.accesskeys),
        list(&m.duplicate_accesskeys),
        None,
    );
    o.push(
        "accessibility.presentation_focusable",
        Accessibility,
        "초점을 받는 요소의 role=presentation/none",
        count_presence(m.presentation_focusable, m.presentation_roles),
        Some(json!({ "total": m.presentation_roles, "focusable": m.presentation_focusable })),
        None,
    );

    const GENERIC_TITLES: &[&str] = &[
        "untitled",
        "untitled document",
        "document",
        "react app",
        "vite app",
        "vite + react",
        "vite + react + ts",
        "vite + vue",
        "nuxt app",
        "create next app",
        "next.js",
        "welcome",
        "index",
        "home",
        "new tab",
        "제목 없음",
        "홈",
    ];
    let title_generic = match i.facts.title.as_deref() {
        Some(t) => Presence::from_bool(GENERIC_TITLES.contains(&t.trim().to_lowercase().as_str())),
        None => Presence::NotApplicable,
    };
    // ===== 접근성 (키보드·폼·표·랜드마크) ====
    o.push(
        "accessibility.custom_control_unfocusable",
        Accessibility,
        "초점을 받을 수 없는 커스텀 컨트롤 (role=button 등에 tabindex 없음)",
        count_presence(m.custom_controls_unfocusable, m.custom_controls),
        Some(json!({ "total": m.custom_controls, "unfocusable": m.custom_controls_unfocusable })),
        None,
    );
    o.push(
        "accessibility.custom_control_unnamed",
        Accessibility,
        "이름 없는 커스텀 컨트롤",
        count_presence(m.custom_controls_unnamed, m.custom_controls),
        Some(json!({ "total": m.custom_controls, "unnamed": m.custom_controls_unnamed })),
        None,
    );
    o.push(
        "accessibility.click_without_keyboard",
        Accessibility,
        "키보드로 쓸 수 없는 onclick 요소 (div·span 등)",
        Presence::from_bool(m.click_without_keyboard > 0),
        Some(json!(m.click_without_keyboard)),
        Some("HTML onclick 속성 기준. 스크립트로 붙인 이벤트는 알 수 없음".into()),
    );
    o.push(
        "accessibility.aria_required_attr_missing",
        Accessibility,
        "역할에 필요한 ARIA 속성 누락",
        Presence::from_bool(!m.required_aria_missing.is_empty()),
        list(&m.required_aria_missing),
        None,
    );
    o.push(
        "accessibility.dialog_unnamed",
        Accessibility,
        "이름 없는 대화상자 (dialog)",
        count_presence(m.dialogs_unnamed, m.dialogs),
        Some(json!({ "total": m.dialogs, "unnamed": m.dialogs_unnamed })),
        None,
    );
    o.push(
        "accessibility.fieldset_legend_missing",
        Accessibility,
        "legend 없는 fieldset",
        count_presence(m.fieldsets_without_legend, m.fieldsets),
        Some(json!({ "total": m.fieldsets, "missing": m.fieldsets_without_legend })),
        None,
    );
    o.push(
        "accessibility.radio_group_ungrouped",
        Accessibility,
        "fieldset·radiogroup 으로 묶이지 않은 라디오 그룹",
        count_presence(m.radio_groups_ungrouped.len(), m.radio_groups),
        list(&m.radio_groups_ungrouped),
        None,
    );
    o.push(
        "accessibility.autocomplete_invalid",
        Accessibility,
        "잘못된 autocomplete 값",
        count_presence(m.autocomplete_invalid.len(), m.autocomplete_fields),
        list(&m.autocomplete_invalid),
        None,
    );
    o.push(
        "accessibility.empty_table_headers",
        Accessibility,
        "비어 있는 표 제목 셀 (th)",
        count_presence(m.empty_table_headers, m.table_headers),
        Some(json!({ "total": m.table_headers, "empty": m.empty_table_headers })),
        None,
    );
    o.push(
        "accessibility.scope_invalid",
        Accessibility,
        "잘못된 scope 속성",
        count_presence(m.scope_invalid, m.scope_attrs),
        Some(json!({ "total": m.scope_attrs, "invalid": m.scope_invalid })),
        Some("th 가 아닌 요소의 scope 또는 row·col·rowgroup·colgroup 이 아닌 값".into()),
    );
    o.push(
        "accessibility.multiple_main",
        Accessibility,
        "main 랜드마크 중복",
        count_presence(usize::from(m.main_landmarks > 1), m.main_landmarks),
        Some(json!(m.main_landmarks)),
        None,
    );
    o.push(
        "accessibility.unnamed_repeated_landmarks",
        Accessibility,
        "이름 없이 반복되는 nav·aside 랜드마크",
        Presence::from_bool(!m.unnamed_repeated_landmarks.is_empty()),
        list(&m.unnamed_repeated_landmarks),
        None,
    );
    o.push(
        "accessibility.redundant_alt",
        Accessibility,
        "중복 표현이 든 alt (\"이미지\", \"image of\" 등)",
        count_presence(m.redundant_alts.len(), m.alt_texts),
        list(&m.redundant_alts),
        None,
    );
    o.push(
        "accessibility.empty_summary",
        Accessibility,
        "비어 있는 details summary",
        count_presence(m.empty_summaries, m.summaries),
        Some(json!({ "total": m.summaries, "empty": m.empty_summaries })),
        None,
    );
    o.push(
        "accessibility.page_title_generic",
        Accessibility,
        "기본값 그대로인 페이지 제목 (\"React App\" 등)",
        title_generic,
        i.facts.title.as_deref().map(|t| json!(t)),
        None,
    );

    // ===== 성능 =====
    o.push(
        "performance.lcp_image_lazy",
        Performance,
        "첫 이미지(LCP 후보) 지연 로딩",
        if m.body_images == 0 {
            Presence::NotApplicable
        } else {
            Presence::from_bool(m.first_image_lazy)
        },
        None,
        Some("본문 첫 <img> 기준. 실제 LCP 요소는 렌더링해야 알 수 있음".into()),
    );
    o.push(
        "performance.fetchpriority_high",
        Performance,
        "fetchpriority=\"high\" 지정",
        Presence::from_bool(m.fetchpriority_high > 0),
        Some(json!(m.fetchpriority_high)),
        None,
    );
    o.push(
        "performance.document_write",
        Performance,
        "인라인 스크립트의 document.write",
        Presence::from_bool(m.document_write),
        None,
        None,
    );
    let css_imports = i.facts.extra.inline_css.imports
        + i.extended
            .external_css
            .as_ref()
            .map_or(0, |c| c.facts.imports);
    o.push(
        "performance.css_import",
        Performance,
        "CSS @import",
        Presence::from_bool(css_imports > 0),
        Some(json!({
            "inline": i.facts.extra.inline_css.imports,
            "external": i.extended.external_css.as_ref().map(|c| c.facts.imports),
        })),
        Some("HTML 안 <style> + 외부 CSS 표본 기준".into()),
    );
    o.push(
        "performance.duplicate_scripts",
        Performance,
        "중복으로 넣은 스크립트",
        count_presence(m.duplicate_scripts.len(), m.script_srcs),
        list(&m.duplicate_scripts),
        None,
    );
    o.push(
        "performance.inline_code_bytes",
        Performance,
        "인라인 스크립트·스타일 크기",
        Presence::from_bool(m.inline_script_bytes + m.inline_style_bytes > 0),
        Some(json!({ "script": m.inline_script_bytes, "style": m.inline_style_bytes })),
        Some("JSON 데이터 블록(JSON-LD 등)은 제외".into()),
    );
    let no_store = header(&i.page.headers, "cache-control")
        .is_some_and(|c| c.to_ascii_lowercase().contains("no-store"));
    o.push(
        "performance.bfcache_blocked",
        Performance,
        "bfcache 차단 요인 (no-store, unload)",
        Presence::from_bool(no_store || m.unload_handler),
        Some(json!({ "cacheControlNoStore": no_store, "unloadHandler": m.unload_handler })),
        None,
    );
    if let Some(w) = &i.extended.page_weight {
        let files = |v: &[(String, u64)]| {
            json!(
                v.iter()
                    .map(|(u, n)| json!({ "url": u, "bytes": n }))
                    .collect::<Vec<_>>()
            )
        };
        o.push(
            "performance.large_scripts",
            Performance,
            "큰 스크립트 파일 (500KB 초과)",
            count_presence(w.large_scripts.len(), w.js_checked),
            Some(files(&w.large_scripts)),
            Some(format!("압축 해제 기준, 표본 {}개", w.js_checked)),
        );
        o.push(
            "performance.large_images",
            Performance,
            "큰 이미지 파일 (500KB 초과)",
            count_presence(w.large_images.len(), w.images_checked - w.images_unknown),
            Some(files(&w.large_images)),
            Some(format!(
                "Content-Length 기준, 표본 {}개 중 크기 확인 {}개",
                w.images_checked,
                w.images_checked - w.images_unknown
            )),
        );
    }
}

/// 표준 점검 목록과 대조해 추가한 항목 (docs/coverage.md).
fn rule_checks(o: &mut Out, i: &Inputs) {
    use Category::*;
    let r = &i.facts.rules;
    let list = |v: &[String]| (!v.is_empty()).then(|| json!(v));
    let ext_css = i.extended.external_css.as_ref();
    let orientation =
        i.facts.extra.inline_css.orientation_lock + ext_css.map_or(0, |c| c.facts.orientation_lock);
    let css_bytes = i.facts.extra.inline_css.bytes + ext_css.map_or(0, |c| c.facts.bytes);
    let css_save =
        i.facts.extra.inline_css.minifiable_bytes + ext_css.map_or(0, |c| c.facts.minifiable_bytes);
    let primary = |l: &str| {
        l.split(['-', '_'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
    };
    let lang_mismatch = match (&r.html_lang, &r.xml_lang) {
        (Some(a), Some(b)) => Presence::from_bool(primary(a) != primary(b)),
        _ => Presence::NotApplicable,
    };
    let coop = header(&i.page.headers, "cross-origin-opener-policy");
    let robots_values = [
        i.facts.meta("robots").map(str::to_string),
        i.facts.meta("googlebot").map(str::to_string),
        header(&i.page.headers, "x-robots-tag"),
    ];
    let mut snippet: Vec<String> = Vec::new();
    for d in robots_values.iter().flatten().flat_map(|v| v.split(',')) {
        let d = d.trim().to_ascii_lowercase();
        let d = d
            .rsplit_once(": ")
            .map_or(d.as_str(), |(_, x)| x)
            .to_string();
        let name = d.split(':').next().unwrap_or_default().trim();
        if matches!(
            name,
            "nosnippet"
                | "max-snippet"
                | "max-image-preview"
                | "max-video-preview"
                | "noarchive"
                | "noimageindex"
                | "notranslate"
                | "unavailable_after"
                | "indexifembedded"
        ) && !snippet.contains(&d)
        {
            snippet.push(d);
        }
    }
    o.push(
        "accessibility.markup_errors",
        Accessibility,
        "HTML 마크업 오류 (파서 오류)",
        Presence::from_bool(r.parse_errors > 0),
        Some(json!({ "count": r.parse_errors, "samples": r.parse_error_samples })),
        Some("HTML 파서(html5ever)가 보고한 오류 기준. KWCAG 2.2 마크업 오류 방지".into()),
    );
    o.push(
        "accessibility.aria_invalid_value",
        Accessibility,
        "허용되지 않는 ARIA 속성 값",
        count_presence(r.aria_invalid_values.len(), r.aria_value_attrs),
        list(&r.aria_invalid_values),
        None,
    );
    o.push(
        "accessibility.aria_name_prohibited",
        Accessibility,
        "이름을 붙일 수 없는 요소의 aria-label",
        count_presence(r.aria_name_prohibited.len(), r.labelled_elements),
        list(&r.aria_name_prohibited),
        Some("역할 없는 div·span 이나 presentation·generic 등 이름이 금지된 역할".into()),
    );
    o.push(
        "accessibility.aria_required_children",
        Accessibility,
        "필수 자식 역할이 없는 ARIA 컨테이너",
        count_presence(r.missing_required_children.len(), r.containers),
        list(&r.missing_required_children),
        Some("예: role=list 안에 listitem 없음, role=tablist 안에 tab 없음".into()),
    );
    o.push(
        "accessibility.aria_required_parent",
        Accessibility,
        "필수 부모 역할이 없는 ARIA 요소",
        count_presence(r.missing_required_parent.len(), r.child_roles),
        list(&r.missing_required_parent),
        Some("예: role=tab 이 tablist 밖에 있음".into()),
    );
    o.push(
        "accessibility.aria_deprecated_role",
        Accessibility,
        "폐기된 ARIA 역할",
        Presence::from_bool(!r.deprecated_roles.is_empty()),
        list(&r.deprecated_roles),
        None,
    );
    o.push(
        "accessibility.status_widget_unnamed",
        Accessibility,
        "이름 없는 progress·meter·tooltip",
        count_presence(r.status_widgets_unnamed, r.status_widgets),
        Some(json!({ "total": r.status_widgets, "unnamed": r.status_widgets_unnamed })),
        None,
    );
    o.push(
        "accessibility.label_title_only",
        Accessibility,
        "title 속성만으로 이름이 붙은 입력 요소",
        count_presence(r.title_only_labels, r.form_controls),
        Some(json!(r.title_only_labels)),
        None,
    );
    o.push(
        "accessibility.multiple_labels",
        Accessibility,
        "label 이 둘 이상 연결된 입력 요소",
        count_presence(r.multiple_labels, r.form_controls),
        Some(json!(r.multiple_labels)),
        None,
    );
    o.push(
        "accessibility.label_name_mismatch",
        Accessibility,
        "보이는 텍스트를 포함하지 않는 aria-label",
        count_presence(r.label_name_mismatch.len(), r.labelled_widgets),
        list(&r.label_name_mismatch),
        Some("링크·버튼 기준 (보이는 텍스트 → aria-label), 글자·숫자만 비교. CSS 로 숨긴 텍스트는 구분하지 못함. KWCAG 2.2 레이블과 네임".into()),
    );
    o.push(
        "accessibility.landmark_not_top_level",
        Accessibility,
        "다른 랜드마크 안에 든 main·banner·contentinfo",
        if r.has_landmarks {
            Presence::from_bool(!r.nested_top_landmarks.is_empty())
        } else {
            Presence::NotApplicable
        },
        list(&r.nested_top_landmarks),
        None,
    );
    o.push(
        "accessibility.duplicate_banner",
        Accessibility,
        "banner(최상위 header) 중복",
        count_presence(usize::from(r.banners > 1), r.banners),
        Some(json!(r.banners)),
        None,
    );
    o.push(
        "accessibility.duplicate_contentinfo",
        Accessibility,
        "contentinfo(최상위 footer) 중복",
        count_presence(usize::from(r.contentinfos > 1), r.contentinfos),
        Some(json!(r.contentinfos)),
        None,
    );
    o.push(
        "accessibility.content_outside_landmarks",
        Accessibility,
        "랜드마크 밖의 본문 텍스트",
        Presence::from_bool(r.text_outside_landmarks > 0),
        Some(json!(r.text_outside_landmarks)),
        Some(
            "랜드마크(header·nav·main·footer·aside·이름 있는 section 등) 밖 텍스트 글자 수".into(),
        ),
    );
    o.push(
        "accessibility.iframe_title_duplicate",
        Accessibility,
        "같은 title 을 쓰는 iframe",
        count_presence(r.duplicate_iframe_titles.len(), r.titled_iframes),
        list(&r.duplicate_iframe_titles),
        None,
    );
    o.push(
        "accessibility.html_xml_lang_mismatch",
        Accessibility,
        "html 의 lang 과 xml:lang 불일치",
        lang_mismatch,
        Some(json!({ "lang": r.html_lang, "xmlLang": r.xml_lang })),
        None,
    );
    o.push(
        "accessibility.server_side_image_map",
        Accessibility,
        "서버 측 이미지맵 (img ismap)",
        Presence::from_bool(r.server_side_image_maps > 0),
        Some(json!(r.server_side_image_maps)),
        None,
    );
    o.push(
        "accessibility.definition_list_invalid",
        Accessibility,
        "잘못된 정의 목록 구조 (dl·dt·dd)",
        if r.definition_lists == 0 && r.definition_list_violations == 0 {
            Presence::NotApplicable
        } else {
            Presence::from_bool(r.definition_list_violations > 0)
        },
        Some(json!(r.definition_list_violations)),
        None,
    );
    o.push(
        "accessibility.table_caption_missing",
        Accessibility,
        "제목(caption) 없는 데이터 표",
        count_presence(r.data_tables_without_caption, r.data_tables),
        Some(json!({ "total": r.data_tables, "missing": r.data_tables_without_caption })),
        Some("th 가 있는 표 기준. caption·aria-label·aria-labelledby 를 제목으로 봄 (KWCAG 2.2 표의 구성)".into()),
    );
    o.push(
        "accessibility.inline_spacing_important",
        Accessibility,
        "글자·줄 간격을 !important 로 고정한 인라인 스타일",
        Presence::from_bool(r.inline_spacing_important > 0),
        Some(json!(r.inline_spacing_important)),
        Some("letter-spacing·word-spacing·line-height (사용자 스타일로 조절 불가)".into()),
    );
    o.push(
        "accessibility.paste_blocked",
        Accessibility,
        "붙여넣기를 막은 입력 요소",
        Presence::from_bool(r.paste_blocked > 0),
        Some(json!(r.paste_blocked)),
        Some("onpaste 속성 기준. KWCAG 2.2 접근 가능한 인증".into()),
    );
    o.push(
        "accessibility.identical_link_text",
        Accessibility,
        "같은 텍스트인데 목적지가 다른 링크",
        count_presence(r.same_text_different_href.len(), r.links_with_text),
        list(&r.same_text_different_href),
        None,
    );
    o.push(
        "accessibility.orientation_lock",
        Accessibility,
        "화면 방향 고정 (orientation 미디어 쿼리 + 회전)",
        Presence::from_bool(orientation > 0),
        Some(json!({ "inline": i.facts.extra.inline_css.orientation_lock, "external": ext_css.map(|c| c.facts.orientation_lock) })),
        Some("HTML 안 <style> + 외부 CSS 표본 기준".into()),
    );
    o.push(
        "meta.doctype",
        Meta,
        "HTML5 doctype (<!DOCTYPE html>)",
        Presence::from_bool(r.has_html5_doctype),
        None,
        Some("없으면 브라우저가 호환(quirks) 모드로 그린다".into()),
    );
    o.push(
        "content.uncrawlable_links",
        Content,
        "검색엔진이 따라갈 수 없는 링크 (href 없음·javascript:)",
        count_presence(r.uncrawlable_links, r.anchors),
        Some(json!(r.uncrawlable_links)),
        None,
    );
    o.push(
        "content.plugins",
        Content,
        "플러그인 콘텐츠 (Flash·Java 등)",
        Presence::from_bool(r.plugins > 0),
        Some(json!(r.plugins)),
        None,
    );
    o.push(
        "indexing.snippet_controls",
        Indexing,
        "스니펫 제한 지시어 (nosnippet·max-snippet 등)",
        Presence::from_bool(!snippet.is_empty() || r.data_nosnippet > 0),
        Some(json!({ "directives": snippet, "dataNosnippet": r.data_nosnippet })),
        Some("검색 결과·AI 요약에 쓰일 수 있는 범위를 제한한다 (meta robots·googlebot·X-Robots-Tag·data-nosnippet)".into()),
    );
    o.push(
        "security.cross_origin_opener_policy",
        Security,
        "Cross-Origin-Opener-Policy",
        Presence::from_bool(coop.is_some()),
        coop.map(Value::from),
        None,
    );
    for (id, name, label) in [
        (
            "security.cross_origin_embedder_policy",
            "cross-origin-embedder-policy",
            "Cross-Origin-Embedder-Policy",
        ),
        (
            "security.cross_origin_resource_policy",
            "cross-origin-resource-policy",
            "Cross-Origin-Resource-Policy",
        ),
    ] {
        let v = header(&i.page.headers, name);
        o.push(
            id,
            Security,
            label,
            Presence::from_bool(v.is_some()),
            v.map(Value::from),
            None,
        );
    }
    o.push(
        "security.sri_missing",
        Security,
        "SRI(integrity) 없는 외부 스크립트·스타일시트",
        count_presence(r.third_party_without_sri.len(), r.third_party_resources),
        list(&r.third_party_without_sri),
        Some("다른 호스트에서 받는 script·stylesheet 의 호스트 목록".into()),
    );
    o.push(
        "security.insecure_form_action",
        Security,
        "https 페이지에서 http 로 보내는 폼",
        count_presence(r.insecure_forms.len(), r.forms),
        list(&r.insecure_forms),
        None,
    );
    o.push(
        "security.target_blank_noopener",
        Security,
        "rel=noopener 없는 새 창 링크",
        count_presence(r.blank_without_noopener, r.blank_links),
        Some(json!({ "total": r.blank_links, "withoutNoopener": r.blank_without_noopener })),
        Some("최신 브라우저는 target=_blank 에 noopener 를 기본 적용한다".into()),
    );
    o.push(
        "performance.unminified_css",
        Performance,
        "압축(minify)되지 않은 CSS",
        if css_bytes == 0 {
            Presence::NotApplicable
        } else {
            Presence::from_bool(css_save > 2048 && css_save * 10 > css_bytes)
        },
        Some(json!({ "bytes": css_bytes, "minifiableBytes": css_save })),
        Some("주석·불필요한 공백 기준 추정. 절감 2KB 초과이면서 10% 초과면 present".into()),
    );
    if let Some(w) = &i.extended.page_weight {
        o.push(
            "performance.large_gifs",
            Performance,
            "큰 GIF 이미지 (100KB 초과)",
            count_presence(w.large_gifs.len(), w.images_checked - w.images_unknown),
            Some(json!(
                w.large_gifs
                    .iter()
                    .map(|(u, n)| json!({ "url": u, "bytes": n }))
                    .collect::<Vec<_>>()
            )),
            Some("움직이는 GIF 는 동영상(mp4·webm)으로 바꾸면 크게 줄어든다".into()),
        );
    }
}

/// AI 에이전트 대응 항목 (Lighthouse Agentic Browsing 대조).
fn agent_checks(o: &mut Out, i: &Inputs) {
    use Category::*;
    let r = &i.facts.rules;
    let wk = i.well_known;
    // robots.txt 의 Agentmap: 지시어
    let agentmap = i.robots.body.as_deref().and_then(|b| {
        b.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim()
                .eq_ignore_ascii_case("agentmap")
                .then(|| v.trim().to_string())
        })
    });
    // HTTP Link 헤더의 rel="ard" / rel="ai-catalog"
    let link_header = header(&i.page.headers, "link")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let header_rel = |rel: &str| {
        link_header.split(',').any(|part| {
            part.split(';').skip(1).any(|p| {
                p.trim()
                    .strip_prefix("rel=")
                    .is_some_and(|v| v.trim_matches('"').split_whitespace().any(|x| x == rel))
            })
        })
    };
    let ard_header = header_rel("ard");
    let ard_found = wk.ard.exists || r.ard_link.is_some() || ard_header || agentmap.is_some();
    o.push(
        "geo.ard_manifest",
        Geo,
        "ARD 매니페스트 (AI 에이전트용 리소스 목록)",
        if wk.ard.error.is_some() && !ard_found {
            Presence::Unknown
        } else {
            Presence::from_bool(ard_found)
        },
        Some(json!({
            "wellKnown": file_value(&wk.ard),
            "htmlLink": r.ard_link,
            "linkHeader": ard_header,
            "robotsAgentmap": agentmap,
        })),
        Some("/.well-known/ard.json · <link rel=\"ard\"> · Link 헤더 · robots.txt Agentmap 중 하나. 링크로 알린 주소는 요청하지 않음".into()),
    );
    let legacy = wk.ai_catalog.exists || r.ai_catalog_link.is_some() || header_rel("ai-catalog");
    o.push(
        "geo.ai_catalog_legacy",
        Geo,
        "ARD 이전 위치 (ai-catalog)",
        Presence::from_bool(legacy),
        Some(json!({ "wellKnown": file_value(&wk.ai_catalog), "htmlLink": r.ai_catalog_link })),
        Some("/.well-known/ai-catalog.json 또는 rel=\"ai-catalog\". ARD 소비자는 이 위치를 확인하지 않아도 된다".into()),
    );
    o.push(
        "geo.webmcp_forms",
        Geo,
        "WebMCP 선언형 주석이 있는 폼 (toolname)",
        count_presence(r.webmcp_forms, r.all_forms),
        Some(json!({ "forms": r.all_forms, "annotated": r.webmcp_forms, "described": r.webmcp_forms_described })),
        None,
    );
    o.push(
        "geo.webmcp_script",
        Geo,
        "WebMCP 명령형 API 사용 흔적 (navigator.modelContext)",
        Presence::from_bool(r.webmcp_script),
        None,
        Some("인라인 스크립트 기준. 외부 스크립트에서 등록한 도구는 알 수 없음".into()),
    );
    let csp = header(&i.page.headers, "content-security-policy").unwrap_or_default();
    o.push(
        "security.trusted_types",
        Security,
        "Trusted Types (CSP require-trusted-types-for)",
        Presence::from_bool(
            csp.to_ascii_lowercase()
                .contains("require-trusted-types-for"),
        ),
        None,
        None,
    );
}

/// OWASP Secure Headers "headers_remove" 목록 (Server·X-Powered-By 는 별도 항목). 버전·내부 구조를 노출한다.
pub const INFO_DISCLOSURE_HEADERS: &[&str] = &[
    "$wsep",
    "host-header",
    "k-proxy-request",
    "liferay-portal",
    "oraclecommercecloud-version",
    "pega-host",
    "powered-by",
    "product",
    "sourcemap",
    "teamcity-node-id",
    "x-aspnet-version",
    "x-aspnetmvc-version",
    "x-atmosphere-error",
    "x-atmosphere-first-request",
    "x-atmosphere-tracking-id",
    "x-b3-parentspanid",
    "x-b3-sampled",
    "x-b3-spanid",
    "x-b3-traceid",
    "x-beserver",
    "x-backside-transport",
    "x-cf-powered-by",
    "x-cms",
    "x-calculatedbetarget",
    "x-cocoon-version",
    "x-content-encoded-by",
    "x-datadog-origin",
    "x-datadog-parent-id",
    "x-datadog-sampling-priority",
    "x-datadog-tags",
    "x-datadog-trace-id",
    "x-diaginfo",
    "x-envoy-attempt-count",
    "x-envoy-external-address",
    "x-envoy-internal",
    "x-envoy-original-dst-host",
    "x-envoy-upstream-service-time",
    "x-feserver",
    "x-framework",
    "x-generated-by",
    "x-generator",
    "x-gitlab-meta",
    "x-jitsi-release",
    "x-joomla-version",
    "x-kong-admin-latency",
    "x-kong-client-latency",
    "x-kong-proxy-latency",
    "x-kong-request-id",
    "x-kong-response-latency",
    "x-kong-third-party-latency",
    "x-kong-total-latency",
    "x-kong-upstream-latency",
    "x-kong-upstream-status",
    "x-kubernetes-pf-flowschema-ui",
    "x-kubernetes-pf-prioritylevel-uid",
    "x-litespeed-cache",
    "x-litespeed-purge",
    "x-litespeed-tag",
    "x-litespeed-vary",
    "x-litespeed-cache-control",
    "x-mod-pagespeed",
    "x-nextjs-cache",
    "x-nextjs-matched-path",
    "x-nextjs-page",
    "x-nextjs-redirect",
    "x-owa-version",
    "x-old-content-length",
    "x-oneagent-js-injection",
    "x-page-speed",
    "x-php-version",
    "x-powered-by-plesk",
    "x-powered-cms",
    "x-redirect-by",
    "x-server-powered-by",
    "x-sourcefiles",
    "x-sourcemap",
    "x-turbo-charged-by",
    "x-tyk-trace-id",
    "x-umbraco-version",
    "x-varnish-backend",
    "x-varnish-server",
    "x-woodpecker-version",
    "x-dtagentid",
    "x-dthealthcheck",
    "x-dtinjectedservlet",
    "x-ruxit-js-agent",
];

/// OWASP 권장 헤더·노출 헤더, security.txt 필수 필드.
fn owasp_checks(o: &mut Out, i: &Inputs) {
    use Category::*;
    for (id, name, label) in [
        (
            "security.x_dns_prefetch_control",
            "x-dns-prefetch-control",
            "X-DNS-Prefetch-Control",
        ),
        (
            "security.x_permitted_cross_domain_policies",
            "x-permitted-cross-domain-policies",
            "X-Permitted-Cross-Domain-Policies",
        ),
    ] {
        let v = header(&i.page.headers, name);
        o.push(
            id,
            Security,
            label,
            Presence::from_bool(v.is_some()),
            v.map(Value::from),
            None,
        );
    }
    let exposed: Vec<String> = INFO_DISCLOSURE_HEADERS
        .iter()
        .filter_map(|h| header(&i.page.headers, h).map(|v| format!("{h}: {v}")))
        .collect();
    o.push(
        "security.info_disclosure_headers",
        Security,
        "버전·내부 정보를 노출하는 헤더 (OWASP 제거 권장)",
        Presence::from_bool(!exposed.is_empty()),
        (!exposed.is_empty()).then(|| json!(exposed)),
        Some("OWASP Secure Headers 제거 권장 목록 기준 (Server·X-Powered-By 는 별도 항목)".into()),
    );
    // RFC 9116: Contact 와 Expires 는 필수
    let wk = &i.well_known.security_txt;
    let field = |name: &str| {
        wk.body.as_deref().and_then(|b| {
            b.lines().find_map(|l| {
                let (k, v) = l.split_once(':')?;
                k.trim()
                    .eq_ignore_ascii_case(name)
                    .then(|| v.trim().to_string())
            })
        })
    };
    let (contact, expires) = (field("contact"), field("expires"));
    o.push(
        "transport.security_txt_fields",
        Transport,
        "security.txt 필수 필드 (Contact·Expires)",
        if wk.exists {
            Presence::from_bool(contact.is_some() && expires.is_some())
        } else {
            Presence::NotApplicable
        },
        wk.exists
            .then(|| json!({ "contact": contact, "expires": expires })),
        Some("RFC 9116".into()),
    );
}

fn canonical_hreflang_weight_checks(o: &mut Out, i: &Inputs) {
    use Category::*;
    let ext = i.extended;
    let base = &i.page.final_url;

    // ===== canonical 대상 =====
    let (lbl_noindex, lbl_bad) = (
        "canonical 대상 페이지 noindex",
        "canonical 대상이 2xx 아님 또는 다른 canonical",
    );
    match &ext.canonical_target {
        None => {
            o.push(
                "meta.canonical_target_noindex",
                Meta,
                lbl_noindex,
                Presence::NotApplicable,
                None,
                Some("canonical 없음 또는 자기 참조".into()),
            );
            o.push(
                "meta.canonical_target_mismatch",
                Meta,
                lbl_bad,
                Presence::NotApplicable,
                None,
                None,
            );
        }
        Some(Err(e)) => {
            o.push(
                "meta.canonical_target_noindex",
                Meta,
                lbl_noindex,
                Presence::Unknown,
                None,
                Some(format!("요청 실패: {e}")),
            );
            o.push(
                "meta.canonical_target_mismatch",
                Meta,
                lbl_bad,
                Presence::Unknown,
                None,
                None,
            );
        }
        Some(Ok(t)) => {
            let ok = (200..300).contains(&t.status);
            let own = t.canonical.as_deref();
            let points_elsewhere =
                own.is_some_and(|c| Url::parse(c).map_or(true, |cu| !same_url(&cu, &t.final_url)));
            let value = json!({ "url": t.final_url.as_str(), "statusCode": t.status, "noindex": t.noindex, "canonical": own });
            o.push(
                "meta.canonical_target_noindex",
                Meta,
                lbl_noindex,
                Presence::from_bool(t.noindex),
                Some(value.clone()),
                None,
            );
            o.push(
                "meta.canonical_target_mismatch",
                Meta,
                lbl_bad,
                Presence::from_bool(!ok || points_elsewhere),
                Some(value),
                None,
            );
        }
    }

    // ===== hreflang 상호 참조 =====
    if ext.hreflang_targets.is_empty() {
        o.push(
            "structured_data.hreflang_reciprocal",
            StructuredData,
            "hreflang 상호 참조",
            Presence::NotApplicable,
            None,
            None,
        );
    } else {
        let mut all = true;
        let detail: Vec<Value> = ext
            .hreflang_targets
            .iter()
            .map(|(lang, url, r)| match r {
                Ok(p) => {
                    let back = p
                        .hreflang
                        .iter()
                        .any(|(_, h)| Url::parse(h).is_ok_and(|u| same_url(&u, base)));
                    all &= back && (200..300).contains(&p.status);
                    json!({ "lang": lang, "url": url, "statusCode": p.status, "linksBack": back })
                }
                Err(e) => {
                    all = false;
                    json!({ "lang": lang, "url": url, "error": e })
                }
            })
            .collect();
        o.push(
            "structured_data.hreflang_reciprocal",
            StructuredData,
            "hreflang 상호 참조",
            Presence::from_bool(all),
            Some(json!(detail)),
            Some("각 언어 버전이 이 페이지를 hreflang 으로 되가리키는지".into()),
        );
    }

    // ===== 외부 CSS / 폰트 =====
    let inline = &i.facts.extra.inline_css;
    if let Some(c) = &ext.external_css {
        o.push(
            "performance.external_css",
            Performance,
            "외부 CSS 분석",
            Presence::from_bool(c.total > 0),
            Some(json!({ "total": c.total, "checked": c.checked, "failed": c.failed, "bytes": c.bytes })),
            None,
        );
    }
    let faces = inline.font_faces + ext.external_css.as_ref().map_or(0, |c| c.facts.font_faces);
    let without = inline.font_faces_without_display
        + ext
            .external_css
            .as_ref()
            .map_or(0, |c| c.facts.font_faces_without_display);
    o.push(
        "performance.font_face_without_display",
        Performance,
        "font-display 없는 @font-face",
        count_presence(without, faces),
        Some(json!({ "fontFaces": faces, "withoutDisplay": without })),
        None,
    );

    // ===== 페이지 무게 =====
    if let Some(w) = &ext.page_weight {
        o.push(
            "performance.page_weight",
            Performance,
            "페이지 리소스 무게",
            Presence::Present,
            Some(json!({
                "htmlBytes": i.page.body_bytes,
                "cssBytes": ext.external_css.as_ref().map(|c| c.bytes),
                "js": { "total": w.js_total, "checked": w.js_checked, "bytes": w.js_bytes },
                "images": { "total": w.images_total, "checked": w.images_checked, "unknown": w.images_unknown, "bytes": w.image_bytes },
            })),
            Some("HTML·CSS·JS 는 압축 해제 기준, 이미지는 Content-Length 기준. 표본 상한이 있어 checked < total 이면 일부만 합산".into()),
        );
    }
}

/// 대상 페이지 HTML·응답 헤더와 무관한 항목 (차단 시에도 유효).
const PAGE_INDEPENDENT: &[&str] = &[
    "crawling.sitemap",
    "crawling.robots_txt",
    "crawling.robots_sitemap",
    "crawling.robots_declared",
    "crawling.robots_allows",
    "crawling.robots_oversize",
    "crawling.robots_crawl",
    "crawling.robots_invalid",
    "crawling.ads_txt",
    "transport.https",
    "transport.http_status",
    "transport.http_to_https",
    "transport.redirect",
    "transport.temporary_redirect",
    "transport.tls",
    "transport.security_txt",
    "transport.http2",
    "transport.http3",
    "transport.bot_challenge",
    "transport.meta_refresh",
    "transport.js_redirect",
    "transport.client_redirect",
    "geo.llms",
    "geo.ai_txt",
    "geo.robots_allows",
    "geo.ua_access",
    "geo.markdown_version",
    "meta.favicon_ico_file",
    "meta.apple_touch_icon_file",
    "meta.apple_app_site_association",
    "meta.assetlinks",
    "site.",
    "performance.response_time",
    "performance.compression",
];

/// 대상 페이지가 차단·오류일 때 `unknown` 으로 바꿔야 하는 항목인지.
pub fn depends_on_page(id: &str) -> bool {
    !PAGE_INDEPENDENT.iter().any(|p| id.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_year() {
        assert_eq!(year_of(0), 1970);
        assert_eq!(year_of(1_798_675_200), 2026); // 2026-12-31
        assert_eq!(year_of(1_798_761_600), 2027); // 2027-01-01
    }

    // ===== build() 를 직접 호출하는 입력 픽스처 =====
    use crate::fetch::{CertInfo, Page};
    use std::time::Duration;

    const HTML: &str = r#"<!doctype html><html lang="ko"><head>
<title>제목</title>
<link rel="icon" href="/favicon.ico"><link rel="canonical" href="/">
<link rel="manifest" href="/m.json"><link rel="alternate" hreflang="en" href="https://a.com/en">
<meta property="og:image" content="/og.png"><meta property="og:url" content="https://a.com/">
<script src="http://insecure.example/x.js"></script>
</head><body><h1>h</h1><a href="/about">about</a><img src="/a.png" alt="a"></body></html>"#;

    fn page(url: &str, html: &str) -> Page {
        let mut h = HeaderMap::new();
        h.insert("x-robots-tag", "noindex".parse().unwrap());
        Page {
            status: 200,
            final_url: Url::parse(url).unwrap(),
            headers: h,
            body: html.to_string(),
            elapsed: Duration::from_millis(5),
            body_bytes: html.len(),
            truncated: true,
            version: "HTTP/2.0".into(),
            certificate: None,
        }
    }

    fn failed(url: &str) -> FileProbe {
        FileProbe {
            url: url.into(),
            error: Some("boom".into()),
            ..Default::default()
        }
    }

    fn found(url: &str, body: &str) -> FileProbe {
        FileProbe {
            url: url.into(),
            status_code: Some(200),
            exists: true,
            format_valid: true,
            body: Some(body.into()),
            size: Some(body.len()),
            ..Default::default()
        }
    }

    fn sample_page(status: u16) -> SamplePage {
        SamplePage {
            status,
            final_url: Url::parse("https://a.com/p").unwrap(),
            noindex: false,
            canonical: None,
            title: Some("t".into()),
            description: None,
            lang: None,
            h1_count: 1,
            img_total: 0,
            img_without_alt: 0,
            body_text_len: 10,
            hreflang: Vec::new(),
        }
    }

    fn err<T>() -> Result<T, String> {
        Err("boom".to_string())
    }

    fn u(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    /// 모든 부속 요청이 실패한 상태.
    struct Fx {
        page: Page,
        facts: PageFacts,
        sitemap: FileProbe,
        robots: FileProbe,
        not_found: Result<(u16, String), String>,
        http_version: Option<Result<(u16, Url), String>>,
        content_encoding: Option<Result<Option<String>, String>>,
        links: Option<LinkProbes>,
        ai_ua: Vec<AiUaResult>,
        well_known: WellKnown,
        sitemap_detail: SitemapDetail,
        site: Option<SiteSample>,
        redirect_chain: Option<Result<Vec<Hop>, String>>,
        host_variant: Option<HostVariant>,
        extended: Extended,
    }

    impl Fx {
        fn failing() -> Self {
            let page = page("https://a.com/", HTML);
            let facts = crate::html::extract(&page.body, &page.final_url);
            Fx {
                page,
                facts,
                sitemap: failed("https://a.com/sitemap.xml"),
                robots: failed("https://a.com/robots.txt"),
                not_found: err(),
                http_version: Some(err()),
                content_encoding: Some(err()),
                links: Some(LinkProbes {
                    // 링크는 있는데 검증 결과가 없음 / 상태도 오류도 없음
                    favicon: None,
                    canonical: Some(LinkProbe::default()),
                    ..Default::default()
                }),
                ai_ua: vec![("gptbot", err())],
                well_known: WellKnown {
                    favicon_ico: failed("f"),
                    apple_touch_icon: failed("a"),
                    security_txt: failed("s"),
                    tdmrep: failed("t"),
                    apple_app_site_association: failed("aa"),
                    assetlinks: failed("al"),
                    ads_txt: failed("ads"),
                    ard: failed("ard"),
                    ai_catalog: failed("aic"),
                },
                sitemap_detail: SitemapDetail::default(),
                site: Some(SiteSample {
                    source: "internal_links",
                    pages: vec![("https://a.com/x".into(), err())],
                }),
                redirect_chain: Some(err()),
                host_variant: Some(HostVariant {
                    url: u("https://www.a.com/"),
                    result: err(),
                }),
                extended: Extended {
                    tls: Some(err()),
                    url_variants: vec![UrlVariant {
                        kind: "uppercase",
                        url: u("https://a.com/X"),
                        result: err(),
                    }],
                    og_image: Some(err()),
                    internal_samples: Some(vec![("https://a.com/about".into(), err())]),
                    manifest: Some(err()),
                    favicon_image: Some(err()),
                    static_asset: Some(("https://a.com/a.css".into(), err())),
                    hreflang_targets: vec![("en".into(), "https://a.com/en".into(), err())],
                    ..Default::default()
                },
            }
        }

        fn run(&self) -> Vec<Check> {
            build(&Inputs {
                page: &self.page,
                facts: &self.facts,
                sitemap: &self.sitemap,
                robots: &self.robots,
                llms: &failed("l"),
                llms_full: &failed("lf"),
                ai_txt: &failed("ai"),
                not_found: &self.not_found,
                http_version: self.http_version.as_ref(),
                content_encoding: self.content_encoding.as_ref(),
                links: self.links.as_ref(),
                ai_ua: &self.ai_ua,
                well_known: &self.well_known,
                sitemap_detail: &self.sitemap_detail,
                site: self.site.as_ref(),
                refresh_hops: &[],
                blocked: None,
                challenge: None,
                redirect_chain: self.redirect_chain.as_ref(),
                host_variant: self.host_variant.as_ref(),
                extended: &self.extended,
            })
        }
    }

    fn get<'a>(checks: &'a [Check], id: &str) -> &'a Check {
        checks
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("{id}"))
    }

    #[test]
    fn failed_requests_are_unknown_not_missing() {
        let c = Fx::failing().run();
        for id in [
            "crawling.robots_txt",
            "crawling.robots_allows.googlebot",
            "crawling.robots_blocks_resources",
            "links.favicon",
            "links.canonical",
            "performance.compression",
            "transport.http_to_https_redirect",
            "transport.redirect",
            "transport.temporary_redirect",
            "geo.ua_access.gptbot",
            "geo.tdm_reservation",
            "not_found_page.returns_404",
            "not_found_page.custom",
            "open_graph.image_file",
            "transport.tls_certificate",
            "transport.url_variant.uppercase",
            "meta.manifest_content",
            "meta.manifest_icons_192_512",
            "meta.favicon_size",
            "performance.static_asset_cache",
        ] {
            assert_eq!(get(&c, id).presence, Presence::Unknown, "{id}");
        }
        assert_eq!(
            get(&c, "transport.host_variant_duplicate").presence,
            Presence::NotApplicable
        );
        // 실패한 표본은 '깨짐' 목록에 오류와 함께 남는다
        assert!(
            get(&c, "content.broken_internal_links")
                .value
                .as_ref()
                .unwrap()
                .to_string()
                .contains("boom")
        );
        assert!(
            get(&c, "site.failed_pages")
                .value
                .as_ref()
                .unwrap()
                .to_string()
                .contains("boom")
        );
        assert!(
            get(&c, "structured_data.hreflang_reciprocal")
                .value
                .as_ref()
                .unwrap()
                .to_string()
                .contains("boom")
        );
        // 잘린 본문, 헤더 noindex, https 페이지의 http 리소스
        assert!(get(&c, "performance.html_bytes").note.is_some());
        assert_eq!(
            get(&c, "indexing.header_noindex").presence,
            Presence::Present
        );
        assert_eq!(
            get(&c, "transport.mixed_content").presence,
            Presence::Present
        );
    }

    #[test]
    fn successful_network_results() {
        let mut fx = Fx::failing();
        fx.robots = FileProbe {
            status_code: Some(503),
            ..failed("https://a.com/robots.txt")
        };
        fx.robots.error = None;
        fx.not_found = Ok((200, "x".into()));
        fx.host_variant = Some(HostVariant {
            url: u("https://www.a.com/"),
            result: Ok((503, u("https://www.a.com/"))),
        });
        fx.extended.tls = Some(Ok(Some(CertInfo {
            subject: "CN=a.com".into(),
            issuer: "CN=CA".into(),
            not_after: 86_400 * 10,
            not_after_text: "1970-01-11".into(),
        })));
        fx.extended.manifest = Some(Ok((200, None)));
        let c = fx.run();
        assert!(
            get(&c, "crawling.robots_txt").note.is_some()
                || get(&c, "crawling.robots_allows.googlebot").note.is_some()
        );
        assert_eq!(
            get(&c, "crawling.robots_allows.googlebot").presence,
            Presence::Unknown
        );
        assert_eq!(
            get(&c, "not_found_page.custom").presence,
            Presence::NotApplicable
        );
        assert_eq!(
            get(&c, "transport.host_variant_duplicate").presence,
            Presence::NotApplicable
        );
        assert_eq!(
            get(&c, "transport.tls_certificate").presence,
            Presence::Present
        );
        assert_eq!(
            get(&c, "meta.manifest_icons_192_512").presence,
            Presence::NotApplicable
        );

        fx.host_variant = Some(HostVariant {
            url: u("https://www.a.com/"),
            result: Ok((200, u("https://www.a.com/"))),
        });
        let c = fx.run();
        assert_eq!(
            get(&c, "transport.host_variant_duplicate").presence,
            Presence::Present
        );
    }

    #[test]
    fn declared_sitemaps_and_sitemap_shapes() {
        let mut fx = Fx::failing();
        let index = r#"<?xml version="1.0"?><sitemapindex><sitemap><loc>https://a.com/s1.xml</loc></sitemap></sitemapindex>"#;
        fx.robots = found(
            "https://a.com/robots.txt",
            "User-agent: *\nAllow: /\nSitemap: https://a.com/sitemap.xml\n",
        );
        fx.sitemap = found("https://a.com/sitemap.xml", index);
        fx.sitemap_detail.samples = vec![("https://a.com/p".into(), Ok(sample_page(200)))];
        let c = fx.run();
        assert_eq!(
            get(&c, "crawling.robots_declared_sitemaps").presence,
            Presence::Present
        );
        assert!(
            get(&c, "crawling.sitemap_broken_urls")
                .note
                .as_deref()
                .unwrap()
                .contains("index")
        );

        // 선언했지만 요청 상한으로 확인하지 못한 sitemap
        fx.robots = found(
            "https://a.com/robots.txt",
            "Sitemap: https://a.com/sitemap.xml\nSitemap: https://a.com/other.xml\n",
        );
        let c = fx.run();
        let d = get(&c, "crawling.robots_declared_sitemaps");
        assert_eq!(d.presence, Presence::Unknown);
        assert!(
            d.value
                .as_ref()
                .unwrap()
                .to_string()
                .contains("\"checked\":false")
        );

        // 빈 sitemap 과 gzip sitemap
        fx.sitemap = found(
            "https://a.com/sitemap.xml",
            "<?xml version=\"1.0\"?><urlset></urlset>",
        );
        let c = fx.run();
        assert_eq!(
            get(&c, "crawling.sitemap_lastmod").presence,
            Presence::NotApplicable
        );
        assert_eq!(
            get(&c, "crawling.sitemap_foreign_host").presence,
            Presence::NotApplicable
        );
        fx.sitemap = found("https://a.com/sitemap.xml.gz", "");
        let c = fx.run();
        assert_eq!(
            get(&c, "crawling.sitemap_entries").presence,
            Presence::Unknown
        );
    }

    #[test]
    fn cert_mark_signal_combinations() {
        let run = |s: CertMarkSignals, len: usize| {
            let mut o = Out(Vec::new());
            cert_mark_check(&mut o, &s, len);
            o.0.remove(0)
        };
        let issuer = CertMarkSignals {
            issuer: Some("한국웹접근성인증평가원"),
            ..Default::default()
        };
        assert_eq!(run(issuer, 9_999).presence, Presence::Present);
        let both = CertMarkSignals {
            text_signal: true,
            filename_signal: true,
            ..Default::default()
        };
        assert_eq!(run(both, 9_999).presence, Presence::Present);
        let one = CertMarkSignals {
            text_signal: true,
            ..Default::default()
        };
        assert_eq!(run(one, 9_999).presence, Presence::Unknown);
        assert_eq!(
            run(CertMarkSignals::default(), 9_999).presence,
            Presence::Missing
        );
        assert_eq!(
            run(CertMarkSignals::default(), 0).presence,
            Presence::Unknown
        );
    }

    #[test]
    fn deep_accessibility_and_performance_items() {
        let mut fx = Fx::failing();
        let html = r#"<html><head><meta http-equiv="refresh" content="30">
<style>@import url(a.css);</style></head>
<body><img src="/hero.jpg" loading="lazy"><div role="buton"></div>
<script>document.write("x")</script></body></html>"#;
        fx.page = page("https://a.com/", html);
        fx.page
            .headers
            .insert("cache-control", "no-store".parse().unwrap());
        fx.facts = crate::html::extract(html, &fx.page.final_url);
        fx.extended.page_weight = Some(PageWeight {
            js_total: 2,
            js_checked: 2,
            images_checked: 3,
            images_unknown: 1,
            large_scripts: largest(
                [("a.js".to_string(), 900_000), ("b.js".to_string(), 10)].into_iter(),
                LARGE_SCRIPT_BYTES,
            ),
            large_images: largest(
                [("x.png".into(), 600_000), ("y.png".into(), 700_000)].into_iter(),
                LARGE_IMAGE_BYTES,
            ),
            ..Default::default()
        });
        let c = fx.run();
        for id in [
            "accessibility.invalid_aria_role",
            "accessibility.timed_refresh",
            "performance.lcp_image_lazy",
            "performance.document_write",
            "performance.css_import",
            "performance.inline_code_bytes",
            "performance.bfcache_blocked",
            "performance.large_scripts",
            "performance.large_images",
        ] {
            assert_eq!(get(&c, id).presence, Presence::Present, "{id}");
        }
        let imgs = get(&c, "performance.large_images").value.as_ref().unwrap();
        assert_eq!(imgs[0]["url"], "y.png", "큰 순");
        assert_eq!(
            get(&c, "accessibility.video_captions_missing").presence,
            Presence::NotApplicable
        );
        assert_eq!(
            get(&c, "performance.fetchpriority_high").presence,
            Presence::Missing
        );

        // 이미지가 없는 페이지
        fx.facts = crate::html::extract("<html><body></body></html>", &fx.page.final_url);
        let c = fx.run();
        assert_eq!(
            get(&c, "performance.lcp_image_lazy").presence,
            Presence::NotApplicable
        );
        assert_eq!(get(&c, "content.csr_shell").presence, Presence::Missing);

        // 본문 없이 번들 스크립트만 있는 CSR 셸
        fx.facts = crate::html::extract(
            r#"<html><body><div id="root"></div><script src="/app.js"></script></body></html>"#,
            &fx.page.final_url,
        );
        let c = fx.run();
        assert_eq!(get(&c, "content.csr_shell").presence, Presence::Present);
    }

    /// 헤더·메타 값이 있을 때만 실행되는 판정 경로.
    #[test]
    fn header_and_meta_dependent_values() {
        let mut fx = Fx::failing();
        let dup_ids: String = (0..21)
            .map(|i| format!(r#"<p id="d{i}"></p><p id="d{i}"></p>"#))
            .collect();
        let html = format!(
            r#"<html><head><meta name="robots" content="noindex, nofollow">
<link rel="stylesheet" href="http://insecure.example/a.css"></head>
<body><h2>무엇인가요?</h2><table role="presentation"><tr><td>x</td></tr></table>{dup_ids}</body></html>"#
        );
        fx.page = page("https://a.com/", &html);
        fx.page
            .headers
            .insert("content-type", "text/html; charset=UTF-8".parse().unwrap());
        fx.page
            .headers
            .insert("vary", "User-Agent".parse().unwrap());
        fx.facts = crate::html::extract(&html, &fx.page.final_url);
        let c = fx.run();
        assert_eq!(get(&c, "indexing.meta_noindex").presence, Presence::Present);
        assert_eq!(
            get(&c, "crawling.vary_user_agent").presence,
            Presence::Present
        );
        assert_eq!(get(&c, "geo.question_headings").presence, Presence::Present);
        assert!(
            get(&c, "accessibility.duplicate_ids").note.is_some(),
            "20개 초과"
        );
        assert_eq!(
            get(&c, "transport.mixed_content").presence,
            Presence::Present
        );
        assert_eq!(
            get(&c, "geo.tables").presence,
            Presence::Missing,
            "presentation 표 제외"
        );
        assert!(
            get(&c, "meta.charset")
                .value
                .as_ref()
                .unwrap()
                .to_string()
                .contains("utf-8")
        );
    }

    #[test]
    fn keyboard_and_title_items() {
        let mut fx = Fx::failing();
        let html = r#"<html><head><title>React App</title></head><body>
<div role="button">x</div><main></main><main></main></body></html>"#;
        fx.facts = crate::html::extract(html, &fx.page.final_url);
        let c = fx.run();
        for id in [
            "accessibility.custom_control_unfocusable",
            "accessibility.page_title_generic",
            "accessibility.multiple_main",
        ] {
            assert_eq!(get(&c, id).presence, Presence::Present, "{id}");
        }
        assert_eq!(
            get(&c, "accessibility.dialog_unnamed").presence,
            Presence::NotApplicable
        );
        fx.facts = crate::html::extract("<html><body></body></html>", &fx.page.final_url);
        let c = fx.run();
        assert_eq!(
            get(&c, "accessibility.page_title_generic").presence,
            Presence::NotApplicable
        );
        assert_eq!(
            get(&c, "accessibility.multiple_main").presence,
            Presence::NotApplicable
        );
    }

    #[test]
    fn standard_rule_items() {
        let mut fx = Fx::failing();
        let css = format!(
            "@media (orientation: landscape) {{ body {{ transform: rotate(-90deg) }} }}{}",
            " ".repeat(3000)
        );
        let html = format!(
            r#"<!DOCTYPE html><html lang="ko" xml:lang="en-US"><head>
<meta name="robots" content="max-snippet:50, noarchive, index">
<style>{css}</style></head><body><main>본문</main></body></html>"#
        );
        fx.page = page("https://a.com/", &html);
        fx.page
            .headers
            .insert("x-robots-tag", "googlebot: nosnippet".parse().unwrap());
        fx.page
            .headers
            .insert("cross-origin-opener-policy", "same-origin".parse().unwrap());
        fx.page
            .headers
            .insert("cross-origin-resource-policy", "same-site".parse().unwrap());
        fx.facts = crate::html::extract(&html, &fx.page.final_url);
        fx.extended.page_weight = Some(PageWeight {
            images_checked: 2,
            large_gifs: vec![("a.gif".into(), 200_000)],
            ..Default::default()
        });
        let c = fx.run();
        for id in [
            "meta.doctype",
            "accessibility.html_xml_lang_mismatch",
            "accessibility.orientation_lock",
            "indexing.snippet_controls",
            "security.cross_origin_opener_policy",
            "security.cross_origin_resource_policy",
            "performance.unminified_css",
            "performance.large_gifs",
        ] {
            assert_eq!(get(&c, id).presence, Presence::Present, "{id}");
        }
        let s = get(&c, "indexing.snippet_controls")
            .value
            .as_ref()
            .unwrap()
            .to_string();
        assert!(s.contains("max-snippet:50") && s.contains("noarchive") && s.contains("nosnippet"));
        assert!(!s.contains("\"index\""));

        // lang 이 같은 언어, CSS 없음
        let html = r#"<html lang="en" xml:lang="en-GB"><body></body></html>"#;
        fx.facts = crate::html::extract(html, &fx.page.final_url);
        let c = fx.run();
        assert_eq!(
            get(&c, "accessibility.html_xml_lang_mismatch").presence,
            Presence::Missing
        );
        assert_eq!(
            get(&c, "performance.unminified_css").presence,
            Presence::NotApplicable
        );
        assert_eq!(
            get(&c, "indexing.snippet_controls").presence,
            Presence::Present,
            "헤더 nosnippet"
        );
        assert_eq!(
            get(&c, "accessibility.definition_list_invalid").presence,
            Presence::NotApplicable
        );
        assert_eq!(
            get(&c, "accessibility.landmark_not_top_level").presence,
            Presence::NotApplicable
        );

        // xml:lang 없음, dd 만 있음
        fx.page.headers.remove("x-robots-tag");
        fx.facts = crate::html::extract(
            "<html lang=ko><body><dd>x</dd><main></main></body></html>",
            &fx.page.final_url,
        );
        let c = fx.run();
        assert_eq!(
            get(&c, "accessibility.html_xml_lang_mismatch").presence,
            Presence::NotApplicable
        );
        assert_eq!(
            get(&c, "accessibility.definition_list_invalid").presence,
            Presence::Present
        );
        assert_eq!(
            get(&c, "indexing.snippet_controls").presence,
            Presence::Missing
        );
    }

    #[test]
    fn agent_discovery_items() {
        let mut fx = Fx::failing();
        // 요청 실패 + 알림 없음 → 판별 불가
        let c = fx.run();
        assert_eq!(get(&c, "geo.ard_manifest").presence, Presence::Unknown);
        assert_eq!(get(&c, "geo.ai_catalog_legacy").presence, Presence::Missing);
        assert_eq!(
            get(&c, "geo.webmcp_forms").presence,
            Presence::NotApplicable
        );

        // robots.txt Agentmap
        fx.robots = found(
            "https://a.com/robots.txt",
            "User-agent: *\nAgentmap: https://a.com/ard.json\n",
        );
        let c = fx.run();
        let v = get(&c, "geo.ard_manifest");
        assert_eq!(v.presence, Presence::Present);
        assert!(
            v.value
                .as_ref()
                .unwrap()
                .to_string()
                .contains("https://a.com/ard.json")
        );

        // Link 헤더 rel=ard, well-known 파일, ai-catalog, Trusted Types
        fx.robots = found("https://a.com/robots.txt", "User-agent: *\n");
        fx.page.headers.insert(
            "link",
            r#"</style.css>; rel=preload, </.well-known/ard.json>; rel="ard""#
                .parse()
                .unwrap(),
        );
        fx.page.headers.insert(
            "content-security-policy",
            "require-trusted-types-for 'script'".parse().unwrap(),
        );
        fx.well_known.ai_catalog = found("https://a.com/.well-known/ai-catalog.json", "{}");
        let html = r#"<html><body><form toolname="q"></form><form></form></body></html>"#;
        fx.facts = crate::html::extract(html, &fx.page.final_url);
        let c = fx.run();
        assert_eq!(get(&c, "geo.ard_manifest").presence, Presence::Present);
        assert_eq!(get(&c, "geo.ai_catalog_legacy").presence, Presence::Present);
        assert_eq!(get(&c, "geo.webmcp_forms").presence, Presence::Present);
        assert_eq!(
            get(&c, "security.trusted_types").presence,
            Presence::Present
        );

        // 아무 알림도 없고 파일도 없음
        fx.page
            .headers
            .insert("link", "</a.css>; rel=preload".parse().unwrap());
        fx.well_known.ard = found("https://a.com/.well-known/ard.json", "{}");
        fx.well_known.ard.exists = false;
        let c = fx.run();
        assert_eq!(get(&c, "geo.ard_manifest").presence, Presence::Missing);
    }

    #[test]
    fn owasp_headers_security_txt_and_rich_results() {
        let mut fx = Fx::failing();
        fx.page
            .headers
            .insert("x-dns-prefetch-control", "off".parse().unwrap());
        fx.page
            .headers
            .insert("x-aspnet-version", "4.0.30319".parse().unwrap());
        fx.page
            .headers
            .insert("x-nextjs-cache", "HIT".parse().unwrap());
        fx.well_known.security_txt = found(
            "https://a.com/.well-known/security.txt",
            "Contact: mailto:security@a.com\nExpires: 2030-01-01T00:00:00.000Z\n",
        );
        let html = r#"<html><head><script type="application/ld+json">[
{"@type":"Event","name":"공연","startDate":"2027-01-01"},
{"@type":"Recipe","name":"김치찌개","image":"k.jpg"},
{"@type":"SoftwareApplication","name":"앱","offers":{"price":0},"review":{"@type":"Review"}}
]</script></head><body><tt>고정폭</tt><strike>취소</strike></body></html>"#;
        fx.facts = crate::html::extract(html, &fx.page.final_url);
        let c = fx.run();
        assert_eq!(
            get(&c, "security.x_dns_prefetch_control").presence,
            Presence::Present
        );
        assert_eq!(
            get(&c, "security.x_permitted_cross_domain_policies").presence,
            Presence::Missing
        );
        let leak = get(&c, "security.info_disclosure_headers");
        assert_eq!(leak.presence, Presence::Present);
        assert!(
            leak.value
                .as_ref()
                .unwrap()
                .to_string()
                .contains("x-aspnet-version: 4.0.30319")
        );
        assert_eq!(
            get(&c, "transport.security_txt_fields").presence,
            Presence::Present
        );
        assert_eq!(
            get(&c, "structured_data.required.event").presence,
            Presence::Missing,
            "location 없음"
        );
        assert_eq!(
            get(&c, "structured_data.required.recipe").presence,
            Presence::Present
        );
        assert_eq!(
            get(&c, "structured_data.required.software_app").presence,
            Presence::Present
        );
        assert_eq!(
            get(&c, "structured_data.required.video").presence,
            Presence::NotApplicable
        );
        assert_eq!(
            get(&c, "accessibility.obsolete_elements").value,
            Some(json!(2))
        );

        // Expires 없는 security.txt, 노출 헤더 없음
        fx.page.headers.clear();
        fx.well_known.security_txt = found(
            "https://a.com/.well-known/security.txt",
            "Contact: mailto:a@a.com\n",
        );
        let c = fx.run();
        assert_eq!(
            get(&c, "transport.security_txt_fields").presence,
            Presence::Missing
        );
        assert_eq!(
            get(&c, "security.info_disclosure_headers").presence,
            Presence::Missing
        );
        // security.txt 자체가 없음
        fx.well_known.security_txt = failed("s");
        let c = fx.run();
        assert_eq!(
            get(&c, "transport.security_txt_fields").presence,
            Presence::NotApplicable
        );
    }
}
