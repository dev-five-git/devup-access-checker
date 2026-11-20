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
    ("applebot", "Applebot", "Apple 검색(Siri·Spotlight)"),
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
    (
        "google_cloudvertexbot",
        "Google-CloudVertexBot",
        "Vertex AI 에이전트",
    ),
    ("googleother", "GoogleOther", "Google 범용(연구)"),
    ("meta_webindexer", "meta-webindexer", "Meta AI 검색 인용"),
    ("oai_adsbot", "OAI-AdsBot", "ChatGPT 광고 검증"),
    ("amzn_searchbot", "Amzn-SearchBot", "Amazon 검색"),
    ("amzn_user", "Amzn-User", "Amazon 사용자 요청"),
    ("mistralai_user", "MistralAI-User", "Mistral 사용자 요청"),
    ("mistralai_index", "MistralAI-Index", "Mistral 검색 색인"),
    ("mistralai_training", "MistralAI-Training", "Mistral 학습"),
    ("duckassistbot", "DuckAssistBot", "DuckDuckGo AI 답변"),
    ("ai2bot", "AI2Bot", "Ai2 학습"),
    (
        "webzio_extended",
        "Webzio-extended",
        "Webz.io AI 학습 데이터",
    ),
];

/// 실제 User-Agent 로 페이지를 요청해 볼 검색엔진 크롤러 (가이드에 적힌 UA 문자열)
pub const SEARCH_USER_AGENTS: &[(&str, &str)] = &[
    (
        "yeti",
        "Mozilla/5.0 (compatible; Yeti/1.1; +https://naver.me/spd)",
    ),
    (
        "bingbot",
        "Mozilla/5.0 (compatible; bingbot/2.0; +http://www.bing.com/bingbot.htm)",
    ),
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

/// 검색엔진 가이드 대조로 추가 요청한 결과.
#[derive(Debug, Default)]
pub struct SearchProbes {
    /// 페이지 경로의 디렉터리 llms.txt (예: /docs/llms.txt). None = 루트 페이지
    pub llms_subpath: Option<FileProbe>,
    /// `/.well-known/agent-card.json` (A2A). None = 수행 안 함
    pub agent_card: Option<FileProbe>,
    /// robots.txt 요청의 리다이렉트 체인. None = 수행 안 함
    pub robots_chain: Option<Result<Vec<Hop>, String>>,
    /// `/.well-known/change-password` 첫 응답 (리다이렉트 1단계까지). None = 수행 안 함
    pub change_password: Option<Result<Vec<Hop>, String>>,
    /// `/.well-known/gpc.json`. None = 수행 안 함
    pub gpc: Option<FileProbe>,
    /// 첫 RSS/Atom 피드 본문. None = 피드 링크 없음 또는 수행 안 함
    pub feed: Option<FileProbe>,
    /// `/BingSiteAuth.xml`. None = 수행 안 함
    pub bing_site_auth: Option<FileProbe>,
    /// 없는 경로 요청의 리다이렉트 체인. None = 수행 안 함
    pub not_found_chain: Option<Result<Vec<Hop>, String>>,
    /// 리다이렉트 경유 호스트(최종 호스트 제외)의 robots.txt
    pub chain_robots: Vec<(String, FileProbe)>,
    /// (IndexNow 키, 키 파일 probe). None = 키를 지정하지 않음
    pub indexnow: Option<(String, Option<FileProbe>)>,
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
    pub search: SearchProbes,
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
pub type AiUaResult = (&'static str, Result<crate::fetch::UaProbe, String>);

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
    let directives: Vec<String> = [f.meta("robots"), f.meta("googlebot"), f.meta("bingbot")]
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
        "TDM 예약 선언 (tdmrep.json / 헤더 / 메타)",
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
        let search = SEARCH_USER_AGENTS.iter().any(|(s, _)| s == id);
        let (item, cat, label) = if search {
            (
                format!("crawling.ua_access.{id}"),
                Crawling,
                format!("검색 크롤러 UA 응답 ({id})"),
            )
        } else {
            (
                format!("geo.ua_access.{id}"),
                Geo,
                format!("AI UA 응답 ({id})"),
            )
        };
        match result {
            Ok((status, final_url, challenge, title)) => o.push(
                item,
                cat,
                label,
                Presence::from_bool((200..300).contains(status) && challenge.is_none()),
                Some(json!({
                    "statusCode": status, "url": final_url.as_str(), "challenge": challenge, "title": title,
                    // 일반 요청과 제목이 다르면 UA 별로 다른 내용을 주는 것(클로킹)일 수 있다
                    "sameTitleAsPage": title.as_deref().map(str::trim) == i.facts.title.as_deref().map(str::trim),
                })),
                Some("User-Agent 문자열만 바꾼 요청 결과. 크롤러 IP 검증을 하는 방화벽에서는 실제 크롤러와 결과가 다를 수 있음".into()),
            ),
            Err(e) => o.push(item, cat, label, Presence::Unknown, None, Some(format!("요청 실패: {e}"))),
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
    spec_checks(&mut o, i);
    search_checks(&mut o, i, &robots);
    search_network_checks(&mut o, i, &robots);
    geo_spec_checks(&mut o, i, &robots);
    security_spec_checks(&mut o, i);
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
                    "members": json.as_ref().and_then(Value::as_object).map(|m| m.keys().collect::<Vec<_>>()),
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
        let sm_url = Url::parse(&sm.url).unwrap_or_else(|_| page.final_url.clone());
        let issues = sitemap::validate(sm.body.as_deref().unwrap_or_default(), &sm_url);
        let problem = |v: &[String]| {
            (
                Presence::from_bool(!v.is_empty()),
                (!v.is_empty()).then(|| json!(v)),
            )
        };
        for (id, label, (p, v)) in [
            (
                "crawling.sitemap_invalid_priority",
                "sitemap priority 범위 밖",
                problem(&issues.invalid_priority),
            ),
            (
                "crawling.sitemap_invalid_changefreq",
                "sitemap changefreq 허용값 밖",
                problem(&issues.invalid_changefreq),
            ),
            (
                "crawling.sitemap_invalid_lastmod",
                "sitemap lastmod W3C Datetime 형식 오류",
                problem(&issues.invalid_lastmod),
            ),
            (
                "crawling.sitemap_invalid_loc",
                "sitemap loc 형식 오류",
                problem(&issues.invalid_loc),
            ),
            (
                "crawling.sitemap_out_of_scope",
                "sitemap 위치 밖 URL",
                problem(&issues.out_of_scope),
            ),
        ] {
            o.push(
                id,
                Crawling,
                label,
                p,
                v,
                Some("sitemaps.org 프로토콜".into()),
            );
        }
        o.push(
            "crawling.sitemap_namespace",
            Crawling,
            "sitemap 네임스페이스",
            Presence::from_bool(issues.namespace),
            None,
            Some("http://www.sitemaps.org/schemas/sitemap/0.9".into()),
        );
        o.push(
            "crawling.sitemap_unescaped_entities",
            Crawling,
            "sitemap 엔티티 미이스케이프 (&)",
            Presence::from_bool(issues.unescaped_ampersands > 0),
            Some(json!(issues.unescaped_ampersands)),
            None,
        );
        o.push(
            "crawling.sitemap_encoding",
            Crawling,
            "sitemap UTF-8",
            Presence::from_bool(sm.utf8),
            None,
            None,
        );
        o.push(
            "crawling.sitemap_over_10mb",
            Crawling,
            "sitemap 10MB 이상 (네이버 제출 한도)",
            Presence::from_bool(size >= 10 * 1024 * 1024),
            Some(json!({ "url": sm.url, "bytes": size })),
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
        i.facts.meta("bingbot").map(str::to_string),
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
    if i.facts.search.data_snippet > 0 {
        snippet.push("data-snippet".into());
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
    // webhint no-disallowed-headers
    "x-runtime",
    "x-version",
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
        wk.exists.then(|| {
            let mut v = json!({ "contact": contact, "expires": expires });
            for k in [
                "encryption",
                "acknowledgments",
                "preferred-languages",
                "canonical",
                "policy",
                "hiring",
                "csaf",
                "bug-bounty",
            ] {
                v[k] = json!(field(k));
            }
            v
        }),
        Some("RFC 9116".into()),
    );
}

/// MDN(browser-compat-data)이 폐기(deprecated)로 표시한 응답 헤더.
pub const DEPRECATED_HEADERS: &[&str] = &[
    "attribution-reporting-register-source",
    "attribution-reporting-register-trigger",
    "content-dpr",
    "expect-ct",
    "feature-policy",
    "observe-browsing-topics",
    "pragma",
    "report-to",
    "warning",
    "x-xss-protection",
];

/// Open Graph 프로토콜(ogp.me)이 정의한 og:type 값.
const OG_TYPES: &[&str] = &[
    "website",
    "article",
    "book",
    "profile",
    "music.song",
    "music.album",
    "music.playlist",
    "music.radio_station",
    "video.movie",
    "video.episode",
    "video.tv_show",
    "video.other",
];

/// 명세 대조로 추가한 항목: WCAG 2.2, Open Graph·X Cards, Web App Manifest, HTML 폐기 속성, schema.org, MDN 폐기 헤더.
fn spec_checks(o: &mut Out, i: &Inputs) {
    use Category::*;
    let f = i.facts;
    let s = &f.spec;
    let ext_css = i.extended.external_css.as_ref();
    let css_count = |g: fn(&crate::css::CssFacts) -> usize| {
        g(&f.extra.inline_css) + ext_css.map_or(0, |c| g(&c.facts))
    };

    // ===== WCAG 2.2 =====
    let viewport = f.meta("viewport");
    let fixed = viewport.and_then(|v| {
        v.split([',', ';']).find_map(|p| {
            let (k, w) = p.split_once('=')?;
            (k.trim().eq_ignore_ascii_case("width") && w.trim().parse::<f64>().is_ok())
                .then(|| p.trim().to_string())
        })
    });
    o.push(
        "accessibility.viewport_fixed_width",
        Accessibility,
        "고정 너비 viewport (리플로 불가)",
        if viewport.is_some() {
            Presence::from_bool(fixed.is_some())
        } else {
            Presence::NotApplicable
        },
        fixed.map(Value::from),
        Some("WCAG 2.2 1.4.10 리플로".into()),
    );
    o.push(
        "accessibility.site_search",
        Accessibility,
        "사이트 검색 (search 요소·role=search·type=search)",
        Presence::from_bool(s.site_search),
        None,
        Some("WCAG 2.2 2.4.5 여러 경로".into()),
    );
    let breadcrumb = s.breadcrumb || f.json_ld.types.iter().any(|t| t == "BreadcrumbList");
    o.push(
        "accessibility.breadcrumb_nav",
        Accessibility,
        "이동 경로(브레드크럼) 표시",
        Presence::from_bool(breadcrumb),
        None,
        Some("WCAG 2.2 2.4.8 위치".into()),
    );
    o.push(
        "accessibility.abbr_without_title",
        Accessibility,
        "title 없는 abbr (약어 설명 없음)",
        count_presence(s.abbr_without_title, s.abbr_total),
        (s.abbr_total > 0).then(|| json!({ "count": s.abbr_without_title, "total": s.abbr_total })),
        Some("WCAG 2.2 3.1.4 약어".into()),
    );
    o.push(
        "accessibility.onchange_navigation",
        Accessibility,
        "값만 바꿔도 이동·제출하는 onchange",
        Presence::from_bool(s.onchange_navigation > 0),
        Some(json!(s.onchange_navigation)),
        Some("WCAG 2.2 3.2.2 입력 시".into()),
    );
    o.push(
        "accessibility.onfocus_navigation",
        Accessibility,
        "초점만 받아도 이동·제출하는 onfocus",
        Presence::from_bool(s.onfocus_navigation > 0),
        Some(json!(s.onfocus_navigation)),
        Some("WCAG 2.2 3.2.1 초점 시".into()),
    );
    o.push(
        "accessibility.video_audio_description_missing",
        Accessibility,
        "음성 해설 트랙 없는 영상",
        count_presence(s.videos_without_descriptions, s.videos),
        (s.videos > 0).then(|| json!({ "count": s.videos_without_descriptions, "total": s.videos })),
        Some("WCAG 2.2 1.2.5 음성 해설. track kind=descriptions 기준 (영상에 직접 넣은 해설은 알 수 없음)".into()),
    );
    o.push(
        "accessibility.password_autocomplete_off",
        Accessibility,
        "자동 완성을 끈 비밀번호 칸",
        count_presence(s.password_autocomplete_off, s.password_fields),
        (s.password_fields > 0)
            .then(|| json!({ "count": s.password_autocomplete_off, "total": s.password_fields })),
        Some("WCAG 2.2 3.3.8 접근 가능한 인증 (비밀번호 관리자 사용 방해)".into()),
    );
    let animations = css_count(|c| c.animations);
    let reduced =
        f.extra.inline_css.reduced_motion || ext_css.is_some_and(|c| c.facts.reduced_motion);
    o.push(
        "accessibility.reduced_motion",
        Accessibility,
        "움직임 줄이기 설정 대응 (prefers-reduced-motion)",
        if animations == 0 {
            Presence::NotApplicable
        } else {
            Presence::from_bool(reduced)
        },
        Some(json!({ "animations": animations })),
        Some("WCAG 2.2 2.3.3 상호작용 애니메이션. 내려받은 CSS 기준".into()),
    );
    let justify = s.inline_justify + css_count(|c| c.justify);
    o.push(
        "accessibility.justified_text",
        Accessibility,
        "양쪽 정렬 텍스트 (text-align: justify)",
        Presence::from_bool(justify > 0),
        Some(json!(justify)),
        Some("WCAG 2.2 1.4.8 시각적 표현".into()),
    );
    o.push(
        "accessibility.obsolete_attributes",
        Accessibility,
        "폐기된 HTML 속성 (align·bgcolor·border 등)",
        Presence::from_bool(s.obsolete_attrs > 0),
        (s.obsolete_attrs > 0)
            .then(|| json!({ "count": s.obsolete_attrs, "samples": s.obsolete_attr_samples })),
        Some("WHATWG HTML 명세 폐기 속성 목록 기준".into()),
    );

    // ===== Open Graph =====
    for (id, key) in [
        ("image_type", "og:image:type"),
        ("image_secure_url", "og:image:secure_url"),
        ("locale_alternate", "og:locale:alternate"),
        ("video", "og:video"),
        ("audio", "og:audio"),
        ("determiner", "og:determiner"),
    ] {
        o.opt(&format!("open_graph.{id}"), OpenGraph, key, f.meta(key));
    }
    let og_type = f.meta("og:type").map(|t| t.trim().to_ascii_lowercase());
    o.push(
        "open_graph.type_unknown",
        OpenGraph,
        "명세에 없는 og:type 값",
        match &og_type {
            Some(t) => Presence::from_bool(!OG_TYPES.contains(&t.as_str())),
            None => Presence::NotApplicable,
        },
        og_type.clone().map(Value::from),
        Some("ogp.me 의 객체 유형 기준".into()),
    );
    let type_ns = og_type
        .as_deref()
        .filter(|t| *t != "website" && OG_TYPES.contains(t))
        .map(|t| format!("{}:", t.split('.').next().unwrap_or(t)));
    let mut props: Vec<&str> = f
        .meta_property
        .keys()
        .chain(f.meta_name.keys())
        .filter(|k| {
            type_ns
                .as_ref()
                .is_some_and(|ns| k.starts_with(ns.as_str()))
        })
        .map(String::as_str)
        .collect();
    props.sort_unstable();
    props.dedup();
    o.push(
        "open_graph.type_properties",
        OpenGraph,
        "og:type 별 속성 (article:published_time 등)",
        if type_ns.is_some() {
            Presence::from_bool(!props.is_empty())
        } else {
            Presence::NotApplicable
        },
        (!props.is_empty()).then(|| json!(props)),
        None,
    );

    // ===== X(Twitter) Cards =====
    o.opt(
        "twitter.image_alt",
        Twitter,
        "twitter:image:alt",
        f.meta("twitter:image:alt"),
    );
    o.opt(
        "twitter.player",
        Twitter,
        "twitter:player",
        f.meta("twitter:player"),
    );
    let mut app: Vec<&str> = f
        .meta_name
        .keys()
        .chain(f.meta_property.keys())
        .filter(|k| k.starts_with("twitter:app:"))
        .map(String::as_str)
        .collect();
    app.sort_unstable();
    app.dedup();
    o.push(
        "twitter.app",
        Twitter,
        "twitter:app:* (앱 카드)",
        Presence::from_bool(!app.is_empty()),
        (!app.is_empty()).then(|| json!(app)),
        None,
    );
    let card = f
        .meta("twitter:card")
        .map(|c| c.trim().to_ascii_lowercase());
    o.push(
        "twitter.card_unknown",
        Twitter,
        "명세에 없는 twitter:card 값",
        match &card {
            Some(c) => Presence::from_bool(!matches!(
                c.as_str(),
                "summary" | "summary_large_image" | "app" | "player"
            )),
            None => Presence::NotApplicable,
        },
        card.map(Value::from),
        Some("summary·summary_large_image·app·player".into()),
    );

    // ===== Web App Manifest =====
    let manifest = match &i.extended.manifest {
        None => Err((Presence::NotApplicable, None)),
        Some(Err(e)) => Err((Presence::Unknown, Some(format!("요청 실패: {e}")))),
        Some(Ok((_, None))) => Err((
            Presence::NotApplicable,
            Some("JSON 으로 해석할 수 없음".to_string()),
        )),
        Some(Ok((_, Some(j)))) => Ok(j),
    };
    let has = |k: &str| {
        manifest
            .as_ref()
            .ok()
            .and_then(|j| j.get(k))
            .filter(|v| match v {
                Value::String(x) => !x.trim().is_empty(),
                Value::Array(a) => !a.is_empty(),
                Value::Null => false,
                _ => true,
            })
    };
    let maskable = has("icons").and_then(Value::as_array).is_some_and(|icons| {
        icons.iter().any(|icon| {
            icon.get("purpose")
                .and_then(Value::as_str)
                .is_some_and(|p| p.split_ascii_whitespace().any(|x| x == "maskable"))
        })
    });
    let display = has("display").and_then(Value::as_str);
    let len = |k: &str| has(k).and_then(Value::as_array).map(|a| json!(a.len()));
    let items = [
        (
            "meta.manifest_id",
            "manifest id",
            has("id").is_some(),
            has("id").cloned(),
        ),
        (
            "meta.manifest_scope",
            "manifest scope",
            has("scope").is_some(),
            has("scope").cloned(),
        ),
        (
            "meta.manifest_description",
            "manifest description",
            has("description").is_some(),
            has("description").cloned(),
        ),
        (
            "meta.manifest_screenshots",
            "manifest screenshots",
            has("screenshots").is_some(),
            len("screenshots"),
        ),
        (
            "meta.manifest_shortcuts",
            "manifest shortcuts",
            has("shortcuts").is_some(),
            len("shortcuts"),
        ),
        (
            "meta.manifest_maskable_icon",
            "manifest maskable 아이콘",
            maskable,
            None,
        ),
        (
            "meta.manifest_colors",
            "manifest theme_color·background_color",
            has("theme_color").is_some() && has("background_color").is_some(),
            Some(
                json!({ "themeColor": has("theme_color"), "backgroundColor": has("background_color") }),
            ),
        ),
        (
            "meta.manifest_display_valid",
            "manifest display 값 (fullscreen·standalone·minimal-ui·browser)",
            display.is_some_and(|d| {
                matches!(d, "fullscreen" | "standalone" | "minimal-ui" | "browser")
            }),
            display.map(Value::from),
        ),
    ];
    for (id, label, ok, value) in items {
        match &manifest {
            Ok(_) => o.push(id, Meta, label, Presence::from_bool(ok), value, None),
            Err((p, note)) => o.push(id, Meta, label, *p, None, note.clone()),
        }
    }

    // ===== schema.org =====
    let unknown: Vec<&str> = f
        .json_ld
        .types
        .iter()
        .map(String::as_str)
        .filter(|t| {
            let n = t
                .trim_start_matches("https://schema.org/")
                .trim_start_matches("http://schema.org/")
                .trim_start_matches("schema:");
            !n.contains([':', '/']) && crate::schema_types::TYPES.binary_search(&n).is_err()
        })
        .collect();
    o.push(
        "structured_data.unknown_types",
        StructuredData,
        "schema.org 에 없는 @type",
        if f.json_ld.types.is_empty() {
            Presence::NotApplicable
        } else {
            Presence::from_bool(!unknown.is_empty())
        },
        (!unknown.is_empty()).then(|| json!(unknown)),
        Some("schema.org 최신 어휘 기준 (다른 어휘 접두사는 제외)".into()),
    );

    // ===== MDN 폐기 헤더 =====
    let deprecated: Vec<String> = DEPRECATED_HEADERS
        .iter()
        .filter_map(|h| header(&i.page.headers, h).map(|v| format!("{h}: {v}")))
        .collect();
    o.push(
        "security.deprecated_headers",
        Security,
        "폐기된 응답 헤더 (X-XSS-Protection·Expect-CT 등)",
        Presence::from_bool(!deprecated.is_empty()),
        (!deprecated.is_empty()).then(|| json!(deprecated)),
        Some("MDN browser-compat-data 의 deprecated 표시 기준".into()),
    );
}

fn is_absolute_http(h: &str) -> bool {
    let l = h.trim().to_ascii_lowercase();
    l.starts_with("http://") || l.starts_with("https://")
}

/// 네이버 서치어드바이저·Bing 웹마스터 가이드라인 대조로 추가한 항목.
fn search_checks(o: &mut Out, i: &Inputs, robots: &Result<Robots, String>) {
    use Category::*;
    let f = i.facts;
    let s = &f.search;
    let base = &i.page.final_url;
    let link_base = f.link_base(base);
    let host = base.host_str().unwrap_or_default();
    let list = |v: &[String]| (!v.is_empty()).then(|| json!(v));
    let x_robots = header(&i.page.headers, "x-robots-tag");

    // ===== 마크업 (네이버) =====
    o.push(
        "meta.canonical_relative",
        Meta,
        "canonical 상대 경로",
        match &s.canonical_raw {
            Some(h) => Presence::from_bool(!is_absolute_http(h)),
            None => Presence::NotApplicable,
        },
        s.canonical_raw.clone().map(Value::from),
        Some("네이버: canonical 은 절대 경로".into()),
    );
    let relative: Vec<&str> = s
        .favicons
        .iter()
        .filter(|(_, h)| !is_absolute_http(h))
        .map(|(_, h)| h.as_str())
        .collect();
    let mut dup: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for (r, _) in &s.favicons {
        *dup.entry(r.as_str()).or_default() += 1;
    }
    dup.retain(|_, n| *n > 1);
    let no_favicon = s.favicons.is_empty();
    o.push(
        "meta.favicon_relative_href",
        Meta,
        "favicon 상대 경로",
        if no_favicon {
            Presence::NotApplicable
        } else {
            Presence::from_bool(!relative.is_empty())
        },
        (!relative.is_empty()).then(|| json!(relative)),
        Some("네이버 파비콘 가이드: 절대 경로".into()),
    );
    o.push(
        "meta.favicon_duplicate_rel",
        Meta,
        "같은 rel 의 favicon 중복",
        if no_favicon {
            Presence::NotApplicable
        } else {
            Presence::from_bool(!dup.is_empty())
        },
        (!dup.is_empty()).then(|| json!(dup)),
        Some("네이버 파비콘 가이드: 같은 rel 은 하나만".into()),
    );
    let header_charset = header(&i.page.headers, "content-type").and_then(|ct| {
        ct.split(';').find_map(|p| {
            let (k, v) = p.split_once('=')?;
            k.trim()
                .eq_ignore_ascii_case("charset")
                .then(|| v.trim().trim_matches('"').to_string())
        })
    });
    let norm = |l: &str| {
        encoding_rs::Encoding::for_label(l.trim().as_bytes())
            .map_or_else(|| l.trim().to_ascii_lowercase(), |e| e.name().to_string())
    };
    o.push(
        "meta.charset_mismatch",
        Meta,
        "헤더·meta charset 불일치",
        match (&header_charset, &f.charset) {
            (Some(h), Some(m)) => Presence::from_bool(norm(h) != norm(m)),
            _ => Presence::NotApplicable,
        },
        Some(json!({ "header": header_charset, "meta": f.charset })),
        Some("네이버 SEO 리포트: 서버 인코딩과 문서 인코딩 일치".into()),
    );
    o.push(
        "meta.head_elements_in_body",
        Meta,
        "body 안에 있는 head 전용 요소",
        Presence::from_bool(!s.head_in_body.is_empty()),
        list(&s.head_in_body),
        Some("title·description·robots·인증 meta·canonical 은 head 안에 있어야 인식".into()),
    );
    let squash = |t: &str| {
        t.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let title = f.title.as_deref();
    let desc = f.meta("description");
    o.push(
        "meta.description_same_as_title",
        Meta,
        "description 이 title 과 동일",
        match (title, desc) {
            (Some(t), Some(d)) => Presence::from_bool(squash(t) == squash(d)),
            _ => Presence::NotApplicable,
        },
        None,
        Some("네이버 HTML 마크업 가이드".into()),
    );
    let terms = |t: Option<&str>| t.map(crate::search::repeated_terms).unwrap_or_default();
    let (rt_title, rt_desc) = (terms(title), terms(desc));
    o.push(
        "meta.repeated_terms",
        Meta,
        "title·description 반복 단어",
        if title.is_none() && desc.is_none() {
            Presence::NotApplicable
        } else {
            Presence::from_bool(!rt_title.is_empty() || !rt_desc.is_empty())
        },
        Some(json!({ "title": rt_title, "description": rt_desc })),
        Some("네이버: 같은 단어 2회 이상 반복 (조사는 구분하지 않음)".into()),
    );
    let mobile = matches!(host.split('.').next(), Some("m" | "mobile"));
    let canonical_host = f
        .canonical
        .as_deref()
        .and_then(|c| link_base.join(c).ok())
        .and_then(|u| u.host_str().map(str::to_string));
    o.push(
        "meta.mobile_site_canonical",
        Meta,
        "모바일 사이트 → 데스크톱 canonical",
        if mobile {
            Presence::from_bool(canonical_host.as_deref().is_some_and(|h| h != host))
        } else {
            Presence::NotApplicable
        },
        canonical_host.map(Value::from),
        Some("네이버 모바일 가이드: 별도 모바일 URL 은 데스크톱 URL 을 canonical 로".into()),
    );
    o.opt(
        "twitter.domain",
        Twitter,
        "twitter:domain",
        f.meta("twitter:domain"),
    );

    // ===== 지시어 =====
    let robots_values: Vec<String> = [f.meta("robots").map(str::to_string), x_robots.clone()]
        .into_iter()
        .flatten()
        .map(|v| v.to_ascii_lowercase())
        .collect();
    let mut naver: Vec<String> = Vec::new();
    if robots_values.iter().any(|v| v.contains("nosourceinfo")) {
        naver.push("nosourceinfo".into());
    }
    if s.naver_meta
        .as_deref()
        .is_some_and(|v| v.to_ascii_lowercase().contains("nosublinks"))
    {
        naver.push("meta naver nosublinks".into());
    }
    if s.nosublink_links > 0 {
        naver.push(format!("a[rel=nosublink] {}", s.nosublink_links));
    }
    o.push(
        "indexing.naver_directives",
        Indexing,
        "네이버 전용 지시어 (nosourceinfo·nosublinks·nosublink)",
        Presence::from_bool(!naver.is_empty()),
        list(&naver),
        None,
    );
    let bing_header: Vec<String> = x_robots
        .iter()
        .flat_map(|v| v.split(','))
        .filter_map(|seg| {
            let seg = seg.trim();
            let (ua, d) = seg.split_once(':')?;
            ua.trim()
                .eq_ignore_ascii_case("bingbot")
                .then(|| d.trim().to_ascii_lowercase())
        })
        .collect();
    let bing_meta = f.meta("bingbot").map(|v| v.to_ascii_lowercase());
    let mut bing: Vec<String> = bing_meta.iter().cloned().collect();
    bing.extend(bing_header.iter().cloned());
    o.push(
        "indexing.bingbot_meta",
        Indexing,
        "Bing 전용 robots 지시 (meta bingbot·X-Robots-Tag bingbot:)",
        Presence::from_bool(!bing.is_empty()),
        list(&bing),
        None,
    );
    let all_directives: Vec<String> = robots_values.iter().chain(bing.iter()).cloned().collect();
    let copilot: Vec<&str> = ["nocache", "noarchive"]
        .into_iter()
        .filter(|d| {
            all_directives
                .iter()
                .any(|v| v.split([',', ' ']).any(|t| t.trim() == *d))
        })
        .collect();
    o.push(
        "geo.copilot_restrictions",
        Geo,
        "Copilot 인용 제한 (noarchive·nocache)",
        Presence::from_bool(!copilot.is_empty()),
        (!copilot.is_empty()).then(|| json!(copilot)),
        Some("Bing: nocache 는 제목·URL 만, noarchive 는 인용 제외".into()),
    );
    let mut ai_pref: Vec<String> = bing
        .iter()
        .flat_map(|v| v.split([',', ' ']))
        .map(str::trim)
        .filter(|t| t.starts_with("ai-training:") || t.starts_with("ai-grounding:"))
        .map(str::to_string)
        .collect();
    if i.robots.exists
        && i.robots
            .body
            .as_deref()
            .unwrap_or_default()
            .lines()
            .any(|l| {
                let l = l.trim().to_ascii_lowercase();
                l.starts_with("user-agent:") && l.contains("__usage_ai-")
            })
    {
        ai_pref.push("robots.txt __usage_ai-* 그룹".into());
    }
    o.push(
        "geo.bing_ai_preferences",
        Geo,
        "Bing AI 학습·그라운딩 선호 표시",
        Presence::from_bool(!ai_pref.is_empty()),
        list(&ai_pref),
        Some("Bing AI Preferences (ai-training·ai-grounding)".into()),
    );
    let noindex = all_directives
        .iter()
        .chain(f.meta("googlebot").map(str::to_ascii_lowercase).iter())
        .any(|v| {
            v.split([',', ' '])
                .any(|t| matches!(t.trim(), "noindex" | "none"))
        });
    let blocked_noindex = match (noindex, robots) {
        (false, _) => Err(Presence::NotApplicable),
        (true, Err(_)) => Err(Presence::Unknown),
        (true, Ok(r)) => Ok(["Googlebot", "Bingbot", "Yeti"]
            .into_iter()
            .filter(|b| !r.is_allowed(b, base.path()))
            .collect::<Vec<_>>()),
    };
    o.push(
        "indexing.noindex_blocked_by_robots",
        Indexing,
        "robots.txt 로 막혀 noindex 를 볼 수 없음",
        blocked_noindex
            .as_ref()
            .map_or_else(|p| *p, |b| Presence::from_bool(!b.is_empty())),
        blocked_noindex
            .ok()
            .filter(|b| !b.is_empty())
            .map(|b| json!(b)),
        Some("검색엔진은 수집이 허용되어야 noindex 를 읽는다".into()),
    );

    // ===== 링크·리소스 =====
    o.push(
        "content.fragment_route_links",
        Content,
        "해시(#!·#/) 라우트 링크",
        count_presence(s.fragment_route_count, f.internal_links),
        (s.fragment_route_count > 0)
            .then(|| json!({ "count": s.fragment_route_count, "samples": s.fragment_routes })),
        Some("네이버: fragment(#) URL 은 수집하지 않음".into()),
    );
    o.push(
        "content.pseudo_js_links",
        Content,
        "의사 자바스크립트 링크 (href=\"fn()\"·href=\"#\"+onclick)",
        Presence::from_bool(s.pseudo_js_count > 0),
        (s.pseudo_js_count > 0)
            .then(|| json!({ "count": s.pseudo_js_count, "samples": s.pseudo_js_links })),
        None,
    );
    o.push(
        "crawling.script_cache_busting",
        Crawling,
        "JS URL 의 타임스탬프·해시 쿼리",
        Presence::from_bool(!s.cache_busting_scripts.is_empty()),
        Some(json!({ "urls": s.cache_busting_scripts, "hashedFiles": s.hashed_script_files })),
        Some("네이버 자바스크립트 가이드: 매번 바뀌는 JS URL 은 수집 비용을 늘림".into()),
    );
    let x = &f.extra;
    for (id, ua) in [
        ("crawling.robots_blocks_resources_yeti", "Yeti"),
        ("crawling.robots_blocks_resources_bingbot", "Bingbot"),
    ] {
        let label = format!("robots.txt 가 CSS/JS 차단 ({ua})");
        match robots {
            Err(e) => o.push(
                id,
                Crawling,
                label,
                Presence::Unknown,
                None,
                Some(e.clone()),
            ),
            Ok(_) if x.same_origin_resources.is_empty() => {
                o.push(id, Crawling, label, Presence::NotApplicable, None, None)
            }
            Ok(r) => {
                let blocked: Vec<&String> = x
                    .same_origin_resources
                    .iter()
                    .filter(|p| !r.is_allowed(ua, p))
                    .collect();
                o.push(
                    id,
                    Crawling,
                    label,
                    Presence::from_bool(!blocked.is_empty()),
                    (!blocked.is_empty()).then(|| json!(blocked)),
                    None,
                );
            }
        }
    }
    let mut fav_paths = vec!["/favicon.ico".to_string()];
    for (_, h) in &s.favicons {
        if let Ok(u) = link_base.join(h)
            && u.host_str() == Some(host)
            && !fav_paths.contains(&u.path().to_string())
        {
            fav_paths.push(u.path().to_string());
        }
    }
    match robots {
        Err(e) => o.push(
            "crawling.robots_blocks_favicon",
            Crawling,
            "robots.txt 가 favicon 차단",
            Presence::Unknown,
            None,
            Some(e.clone()),
        ),
        Ok(r) => {
            let blocked: Vec<&String> = fav_paths
                .iter()
                .filter(|p| !r.is_allowed("Yeti", p))
                .collect();
            o.push(
                "crawling.robots_blocks_favicon",
                Crawling,
                "robots.txt 가 favicon 차단",
                Presence::from_bool(!blocked.is_empty()),
                (!blocked.is_empty()).then(|| json!(blocked)),
                Some("Yeti 기준 (그룹이 없으면 * 그룹)".into()),
            );
        }
    }
    o.push(
        "content.img_generic_filenames",
        Content,
        "의미 없는 이미지 파일명 (IMG_1234·숫자·해시)",
        count_presence(s.generic_image_count, s.images),
        (s.generic_image_count > 0)
            .then(|| json!({ "count": s.generic_image_count, "samples": s.generic_image_names })),
        Some("Bing 웹마스터 가이드라인: 설명적인 파일명".into()),
    );
    o.push(
        "content.parameter_heavy_links",
        Content,
        "파라미터가 많은 내부 링크 (3개 이상)",
        count_presence(s.param_heavy_count, f.internal_links),
        Some(json!({
            "count": s.param_heavy_count, "maxParams": s.max_link_params,
            "pageParams": base.query_pairs().count(), "samples": s.param_heavy_links,
        })),
        Some("Bing·네이버: URL 파라미터 최소화".into()),
    );

    // ===== 구조화 데이터 =====
    o.push(
        "structured_data.rdfa",
        StructuredData,
        "RDFa",
        Presence::from_bool(s.rdfa),
        None,
        None,
    );
    o.push(
        "structured_data.microformats",
        StructuredData,
        "Microformats (hCard·hRecipe·hReview 등)",
        Presence::from_bool(!s.microformats.is_empty()),
        list(&s.microformats),
        None,
    );
    o.push(
        "open_graph.image_socialonly",
        OpenGraph,
        "og:image socialonly (Bing 뉴스)",
        Presence::from_bool(s.og_image_socialonly),
        None,
        None,
    );
    for (id, label, result) in crate::naver_ld::evaluate(&x.ld_roots, f.meta("og:image").is_some())
    {
        let id = format!("structured_data.naver.{id}");
        match result {
            None => o.push(
                id,
                StructuredData,
                label,
                Presence::NotApplicable,
                None,
                None,
            ),
            Some((missing, mut extra)) => {
                extra["missing"] = json!(missing);
                o.push(
                    id,
                    StructuredData,
                    label,
                    Presence::from_bool(missing.is_empty()),
                    Some(extra),
                    Some("네이버 서치어드바이저 구조화 데이터 가이드의 필수 속성".into()),
                );
            }
        }
    }

    // ===== 전송 =====
    let invalid_labels = base.domain().map(crate::search::invalid_host_labels);
    o.push(
        "transport.hostname_invalid",
        Transport,
        "표준에 어긋난 호스트명 (밑줄 등)",
        invalid_labels
            .as_ref()
            .map_or(Presence::NotApplicable, |l| {
                Presence::from_bool(!l.is_empty())
            }),
        invalid_labels.filter(|l| !l.is_empty()).map(|l| json!(l)),
        Some("네이버: RFC 952·1123 호스트명 (IDN 은 punycode 기준)".into()),
    );
    let suffix = base.domain().and_then(|d| psl::suffix(d.as_bytes()));
    let private = suffix
        .filter(|sf| sf.typ() == Some(psl::Type::Private))
        .map(|sf| String::from_utf8_lossy(sf.as_bytes()).into_owned());
    o.push(
        "transport.hosting_subdomain",
        Transport,
        "호스팅 제공 기본 도메인 (github.io 등)",
        Presence::from_bool(private.is_some()),
        private.map(Value::from),
        Some("공개 접미사 목록(PSL)의 PRIVATE 구역 기준".into()),
    );
    let hops = i
        .redirect_chain
        .map(|r| r.as_ref().map(|h| h.len().saturating_sub(1)));
    let looped = i
        .redirect_chain
        .and_then(|r| r.as_ref().ok())
        .is_some_and(|h| {
            h.iter()
                .enumerate()
                .any(|(n, a)| h[..n].iter().any(|b| b.url == a.url))
        });
    o.push(
        "transport.redirect_hops_excess",
        Transport,
        "리다이렉트 5회 초과·루프",
        match hops {
            None => Presence::NotApplicable,
            Some(Err(_)) => Presence::Unknown,
            Some(Ok(0)) => Presence::NotApplicable,
            Some(Ok(n)) => Presence::from_bool(n > 5 || looped),
        },
        hops.and_then(Result::ok)
            .map(|n| json!({ "hops": n, "loop": looped })),
        Some("네이버: 리다이렉트는 5회 이하".into()),
    );
    match i.not_found {
        Ok((_, body)) => {
            let hosts = crate::search::third_party_hosts(body, base);
            o.push(
                "not_found_page.third_party_calls",
                NotFoundPage,
                "404 페이지의 외부 호출",
                Presence::from_bool(!hosts.is_empty()),
                list(&hosts),
                Some("Bing 404 모범 사례: 정적 HTML, 외부 호출 없음".into()),
            );
        }
        Err(e) => o.push(
            "not_found_page.third_party_calls",
            NotFoundPage,
            "404 페이지의 외부 호출",
            Presence::Unknown,
            None,
            Some(e.clone()),
        ),
    }
}

/// 검색엔진 가이드 대조 항목 중 추가 요청이 필요한 것.
fn search_network_checks(o: &mut Out, i: &Inputs, robots: &Result<Robots, String>) {
    use Category::*;
    let sp = &i.extended.search;
    let base = &i.page.final_url;
    let host = base.host_str().unwrap_or_default();

    // ===== 피드 (네이버 RSS 가이드) =====
    let feed_items = [
        ("crawling.feed_valid", "피드 형식 유효 (RSS·Atom)"),
        ("crawling.feed_items", "피드 item 수"),
        (
            "crawling.feed_foreign_host",
            "피드 안 다른 호스트·상대 경로 URL",
        ),
        ("crawling.feed_oversize", "피드 10MB 이상"),
        ("crawling.feed_full_content", "피드 본문 전체 포함"),
    ];
    match &sp.feed {
        None => {
            for (id, label) in feed_items {
                o.push(id, Crawling, label, Presence::NotApplicable, None, None);
            }
        }
        Some(p) if p.error.is_some() || p.body.is_none() => {
            for (id, label) in feed_items {
                o.push(
                    id,
                    Crawling,
                    label,
                    Presence::Unknown,
                    None,
                    p.error.clone(),
                );
            }
        }
        Some(p) => {
            let feed = crate::search::parse_feed(p.body.as_deref().unwrap_or_default());
            let valid = feed.valid && (200..300).contains(&p.status_code.unwrap_or_default());
            let na = |b: bool| {
                if valid {
                    Presence::from_bool(b)
                } else {
                    Presence::NotApplicable
                }
            };
            let foreign: Vec<&String> = feed
                .links
                .iter()
                .filter(|l| {
                    Url::parse(l)
                        .ok()
                        .and_then(|u| u.host_str().map(str::to_string))
                        .as_deref()
                        != Some(host)
                })
                .take(20)
                .collect();
            let size = p.size.unwrap_or_default();
            o.push(
                "crawling.feed_valid",
                Crawling,
                "피드 형식 유효 (RSS·Atom)",
                Presence::from_bool(valid),
                Some(json!({ "url": p.url, "statusCode": p.status_code, "format": feed.format })),
                None,
            );
            o.push(
                "crawling.feed_items",
                Crawling,
                "피드 item 수",
                na(feed.items > 0),
                Some(json!(feed.items)),
                None,
            );
            o.push(
                "crawling.feed_foreign_host",
                Crawling,
                "피드 안 다른 호스트·상대 경로 URL",
                na(!foreign.is_empty()),
                (!foreign.is_empty()).then(|| json!(foreign)),
                Some("네이버: 피드의 모든 URL 은 사이트와 같은 호스트의 절대 경로".into()),
            );
            o.push(
                "crawling.feed_oversize",
                Crawling,
                "피드 10MB 이상",
                na(size >= 10 * 1024 * 1024),
                Some(json!({ "bytes": size, "truncated": p.truncated })),
                None,
            );
            o.push(
                "crawling.feed_full_content",
                Crawling,
                "피드 본문 전체 포함",
                if valid && feed.items > 0 {
                    Presence::from_bool(feed.full_content_items >= feed.items)
                } else {
                    Presence::NotApplicable
                },
                Some(json!({ "items": feed.items, "withContent": feed.full_content_items })),
                Some("item 마다 content:encoded(RSS) 또는 content(Atom) 가 있는지. 본문이 잘렸는지는 판단하지 않음".into()),
            );
        }
    }

    // ===== Bing 사이트 인증 파일 =====
    let (p, n) = match &sp.bing_site_auth {
        None => (Presence::NotApplicable, None),
        Some(f) if f.error.is_some() => (Presence::Unknown, f.error.clone()),
        Some(f) => (
            Presence::from_bool(
                f.exists && f.body.as_deref().is_some_and(|b| b.contains("<user>")),
            ),
            None,
        ),
    };
    o.push(
        "meta.bing_site_auth_file",
        Meta,
        "Bing 사이트 인증 파일 (/BingSiteAuth.xml)",
        p,
        sp.bing_site_auth.as_ref().map(file_value),
        n,
    );

    // ===== robots.txt 인코딩 =====
    o.push(
        "crawling.robots_encoding",
        Crawling,
        "robots.txt UTF-8 인코딩",
        if i.robots.error.is_some() {
            Presence::Unknown
        } else if i.robots.exists {
            Presence::from_bool(i.robots.utf8)
        } else {
            Presence::NotApplicable
        },
        None,
        Some("Bing robots.txt 가이드: UTF-8 또는 ASCII".into()),
    );

    // ===== sitemap 표본 중 리다이렉트 =====
    let samples = &i.sitemap_detail.samples;
    let redirected: Vec<&str> = samples
        .iter()
        .filter(|(u, r)| {
            r.as_ref().is_ok_and(|p| {
                p.final_url.as_str().trim_end_matches('/') != u.trim_end_matches('/')
            })
        })
        .map(|(u, _)| u.as_str())
        .collect();
    o.push(
        "crawling.sitemap_redirected_urls",
        Crawling,
        "sitemap 표본 중 리다이렉트되는 URL",
        count_presence(redirected.len(), samples.len()),
        (!redirected.is_empty()).then(|| json!(redirected)),
        Some("Bing: sitemap 에서 리다이렉트·삭제된 URL 은 빼야 함".into()),
    );

    // ===== 없는 경로 리다이렉트 =====
    let (p, v, n) = match &sp.not_found_chain {
        None => (Presence::NotApplicable, None, None),
        Some(Err(e)) => (Presence::Unknown, None, Some(e.clone())),
        Some(Ok(hops)) => (
            Presence::from_bool(hops.len() > 1),
            Some(json!(
                hops.iter()
                    .map(|h| json!({ "url": h.url, "status": h.status }))
                    .collect::<Vec<_>>()
            )),
            None,
        ),
    };
    o.push(
        "not_found_page.redirects",
        NotFoundPage,
        "없는 경로가 리다이렉트됨",
        p,
        v,
        n.or_else(|| Some("네이버: 오류 페이지로 리다이렉트하지 말 것".into())),
    );

    // ===== 리다이렉트 경유 URL 의 robots 차단 =====
    let chain_label = "리다이렉트 경유 URL robots 차단";
    match i.redirect_chain {
        None => o.push(
            "transport.redirect_chain_robots_blocked",
            Transport,
            chain_label,
            Presence::NotApplicable,
            None,
            None,
        ),
        Some(Err(e)) => o.push(
            "transport.redirect_chain_robots_blocked",
            Transport,
            chain_label,
            Presence::Unknown,
            None,
            Some(e.clone()),
        ),
        Some(Ok(hops)) if hops.len() < 2 => o.push(
            "transport.redirect_chain_robots_blocked",
            Transport,
            chain_label,
            Presence::NotApplicable,
            None,
            None,
        ),
        Some(Ok(hops)) => {
            let mut blocked: Vec<String> = Vec::new();
            let mut unknown = false;
            for h in hops {
                let Ok(u) = Url::parse(&h.url) else { continue };
                let rules = if u.host_str() == Some(host) {
                    robots.clone()
                } else {
                    match sp
                        .chain_robots
                        .iter()
                        .find(|(hh, _)| Some(hh.as_str()) == u.host_str())
                    {
                        Some((_, f)) if f.exists => {
                            Ok(Robots::parse(f.body.as_deref().unwrap_or_default()))
                        }
                        Some((_, f)) if f.error.is_none() => Ok(Robots::parse("")),
                        _ => Err(String::new()),
                    }
                };
                match rules {
                    Ok(r) => {
                        for bot in ["Googlebot", "Bingbot", "Yeti"] {
                            if !r.is_allowed(bot, u.path()) {
                                blocked.push(format!("{bot}: {}", h.url));
                            }
                        }
                    }
                    Err(_) => unknown = true,
                }
            }
            o.push(
                "transport.redirect_chain_robots_blocked",
                Transport,
                chain_label,
                if blocked.is_empty() && unknown {
                    Presence::Unknown
                } else {
                    Presence::from_bool(!blocked.is_empty())
                },
                (!blocked.is_empty()).then(|| json!(blocked)),
                Some("네이버: 리다이렉트 과정의 모든 URL 이 수집 가능해야 함".into()),
            );
        }
    }

    // ===== IndexNow 키 파일 =====
    let label = "IndexNow 키 파일";
    match &sp.indexnow {
        None => o.push(
            "crawling.indexnow_key_file",
            Crawling,
            label,
            Presence::NotApplicable,
            None,
            Some(
                "키 파일 이름은 비밀이라 키를 지정해야 확인할 수 있음 (Options::indexnow_key)"
                    .into(),
            ),
        ),
        Some((key, None)) => o.push(
            "crawling.indexnow_key_file",
            Crawling,
            label,
            Presence::Missing,
            Some(json!({ "key": key })),
            Some("키 형식이 올바르지 않음 (16진수·대시 8~128자)".into()),
        ),
        Some((key, Some(f))) => {
            let ok = (200..300).contains(&f.status_code.unwrap_or_default())
                && f.body.as_deref().is_some_and(|b| b.trim() == key);
            o.push(
                "crawling.indexnow_key_file",
                Crawling,
                label,
                if f.error.is_some() {
                    Presence::Unknown
                } else {
                    Presence::from_bool(ok)
                },
                Some(json!({ "key": key, "url": f.url, "statusCode": f.status_code })),
                f.error.clone(),
            );
        }
    }
}

/// HTTP `Link` 헤더에서 rel(과 선택적으로 type)이 맞는 대상 URL.
fn link_header_targets(headers: &HeaderMap, rel: &str, typ: Option<&str>) -> Vec<String> {
    let raw = header(headers, "link").unwrap_or_default();
    raw.split(',')
        .filter_map(|part| {
            let mut it = part.split(';');
            let target = it
                .next()?
                .trim()
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_string();
            let params: Vec<(String, String)> = it
                .filter_map(|p| {
                    let (k, v) = p.split_once('=')?;
                    Some((
                        k.trim().to_ascii_lowercase(),
                        v.trim().trim_matches('"').to_ascii_lowercase(),
                    ))
                })
                .collect();
            let has = |k: &str, want: &str| {
                params
                    .iter()
                    .any(|(pk, pv)| pk == k && pv.split_whitespace().any(|x| x == want))
            };
            (has("rel", rel) && typ.is_none_or(|t| has("type", t))).then_some(target)
        })
        .collect()
}

/// A2A 에이전트 카드 필수 필드 (a2a-protocol.org 명세 §8.2).
const AGENT_CARD_FIELDS: &[&str] = &[
    "name",
    "description",
    "version",
    "capabilities",
    "skills",
    "supportedInterfaces",
    "defaultInputModes",
    "defaultOutputModes",
];

/// AI·프로토콜 명세(llms.txt, IETF aipref, Content Signals, RSL, TDMRep, A2A, RFC 9309, sitemaps.org, GEO 논문) 대조 항목.
fn geo_spec_checks(o: &mut Out, i: &Inputs, robots: &Result<Robots, String>) {
    use Category::*;
    let f = i.facts;
    let s = &f.search;
    let sp = &i.extended.search;
    let headers = &i.page.headers;
    let list = |v: &[String]| (!v.is_empty()).then(|| json!(v));

    // ===== llms.txt =====
    let llms = i
        .llms
        .exists
        .then(|| llms::parse(i.llms.body.as_deref().unwrap_or_default()));
    let na = |b: Option<bool>| b.map_or(Presence::NotApplicable, Presence::from_bool);
    o.push(
        "geo.llms_txt_link_format",
        Geo,
        "llms.txt 링크 항목 형식 오류",
        na(llms.as_ref().map(|d| !d.malformed_items.is_empty())),
        llms.as_ref().and_then(|d| list(&d.malformed_items)),
        Some("llmstxt.org: 섹션 목록은 `- [이름](url): 설명`".into()),
    );
    o.push(
        "geo.llms_txt_extra_headings",
        Geo,
        "llms.txt 규격 밖 헤딩",
        na(llms.as_ref().map(|d| !d.extra_headings.is_empty())),
        llms.as_ref().and_then(|d| list(&d.extra_headings)),
        None,
    );
    o.push(
        "geo.llms_txt_optional_section",
        Geo,
        "llms.txt Optional 섹션",
        na(llms.as_ref().map(|d| d.optional_section)),
        None,
        None,
    );
    let mut described: Vec<String> = s.llms_describedby.clone();
    described.extend(
        link_header_targets(headers, "describedby", None)
            .into_iter()
            .filter(|u| {
                u.split(['?', '#'])
                    .next()
                    .unwrap_or_default()
                    .ends_with("llms.txt")
            }),
    );
    o.push(
        "geo.llms_txt_describedby",
        Geo,
        "llms.txt 연결 (rel=describedby)",
        Presence::from_bool(!described.is_empty()),
        list(&described),
        Some("llmstxt.org: HTML link 또는 Link 헤더의 rel=describedby".into()),
    );
    let (p, n) = match &sp.llms_subpath {
        None => (Presence::NotApplicable, None),
        Some(p) if p.error.is_some() => (Presence::Unknown, p.error.clone()),
        Some(p) => (Presence::from_bool(p.exists), None),
    };
    o.push(
        "geo.llms_txt_subpath",
        Geo,
        "하위 경로 llms.txt",
        p,
        sp.llms_subpath.as_ref().map(file_value),
        n,
    );

    // ===== AI 사용 선호·라이선스 =====
    let parsed = robots.as_ref().ok().filter(|_| i.robots.exists);
    let pairs = |v: &[(String, String)]| {
        json!(
            v.iter()
                .map(|(a, x)| json!({ "userAgents": a, "value": x }))
                .collect::<Vec<_>>()
        )
    };
    o.push(
        "geo.content_signals",
        Geo,
        "Content-Signal (robots.txt)",
        match (robots, parsed) {
            (Err(_), _) => Presence::Unknown,
            (_, None) => Presence::NotApplicable,
            (_, Some(r)) => Presence::from_bool(!r.content_signals.is_empty()),
        },
        parsed
            .filter(|r| !r.content_signals.is_empty())
            .map(|r| pairs(&r.content_signals)),
        Some("search·ai-input·ai-train = yes|no".into()),
    );
    let usage_header = header(headers, "content-usage");
    let usage_robots = parsed.map(|r| r.content_usage.clone()).unwrap_or_default();
    o.push(
        "geo.content_usage",
        Geo,
        "AI 사용 선호 (IETF aipref Content-Usage)",
        if usage_header.is_some() || !usage_robots.is_empty() {
            Presence::Present
        } else if robots.is_err() {
            Presence::Unknown
        } else {
            Presence::Missing
        },
        Some(json!({ "header": usage_header, "robots": pairs(&usage_robots) })),
        Some("train-ai·ai-use·search = y|n".into()),
    );
    let mut rsl: Vec<String> = parsed.map(|r| r.licenses.clone()).unwrap_or_default();
    rsl.extend(link_header_targets(
        headers,
        "license",
        Some("application/rsl+xml"),
    ));
    rsl.extend(s.rsl_links.iter().cloned());
    if s.rsl_script {
        rsl.push("<script type=\"application/rsl+xml\">".into());
    }
    o.push(
        "geo.rsl_license",
        Geo,
        "RSL 라이선스 선언",
        Presence::from_bool(!rsl.is_empty()),
        list(&rsl),
        Some("robots.txt License·Link 헤더·link rel=license·인라인 RSL".into()),
    );
    let wk = &i.well_known;
    let tdm_header = header(headers, "tdm-reservation");
    let tdm_meta = f.meta("tdm-reservation").map(str::to_string);
    let tdm_json: Option<Value> = wk
        .tdmrep
        .exists
        .then(|| serde_json::from_str(wk.tdmrep.body.as_deref().unwrap_or_default()).ok())
        .flatten();
    let declared = tdm_header.is_some() || tdm_meta.is_some() || wk.tdmrep.exists;
    let policy: Vec<String> = [
        header(headers, "tdm-policy"),
        f.meta("tdm-policy").map(str::to_string),
    ]
    .into_iter()
    .flatten()
    .chain(
        tdm_json
            .iter()
            .filter_map(Value::as_array)
            .flatten()
            .filter_map(|e| {
                e.get("tdm-policy")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }),
    )
    .collect();
    o.push(
        "geo.tdm_policy",
        Geo,
        "TDM 정책 URL (tdm-policy)",
        if declared {
            Presence::from_bool(!policy.is_empty())
        } else {
            Presence::NotApplicable
        },
        list(&policy),
        Some("W3C TDMRep".into()),
    );
    let bit = |v: &str| matches!(v.trim(), "0" | "1");
    let mut bad: Vec<String> = [tdm_header.clone(), tdm_meta.clone()]
        .into_iter()
        .flatten()
        .filter(|v| !bit(v))
        .collect();
    if wk.tdmrep.exists {
        let ok = tdm_json
            .as_ref()
            .and_then(Value::as_array)
            .is_some_and(|a| {
                a.iter().all(|e| {
                    e.get("location").is_some_and(Value::is_string)
                        && e.get("tdm-reservation")
                            .and_then(Value::as_u64)
                            .is_some_and(|n| n <= 1)
                })
            });
        if !ok {
            bad.push("tdmrep.json".into());
        }
    }
    o.push(
        "geo.tdm_reservation_invalid",
        Geo,
        "TDM 예약 값 오류",
        if declared {
            Presence::from_bool(!bad.is_empty())
        } else {
            Presence::NotApplicable
        },
        list(&bad),
        Some(
            "tdm-reservation 은 0 또는 1, tdmrep.json 은 location·tdm-reservation 객체 배열".into(),
        ),
    );
    let (p, v, n) = match &sp.agent_card {
        None => (Presence::NotApplicable, None, None),
        Some(c) if c.error.is_some() => (Presence::Unknown, None, c.error.clone()),
        Some(c) if !c.exists => (Presence::Missing, None, None),
        Some(c) => {
            let card: Value =
                serde_json::from_str(c.body.as_deref().unwrap_or_default()).unwrap_or_default();
            let missing: Vec<&str> = AGENT_CARD_FIELDS
                .iter()
                .copied()
                .filter(|k| card.get(k).is_none())
                .collect();
            (
                Presence::from_bool(missing.is_empty()),
                Some(json!({ "url": c.url, "missing": missing })),
                None,
            )
        }
    };
    o.push("geo.a2a_agent_card", Geo, "A2A 에이전트 카드", p, v, n);

    // ===== RFC 9309 =====
    let rb = i.robots;
    o.push(
        "crawling.robots_content_type",
        Crawling,
        "robots.txt text/plain",
        if rb.exists {
            Presence::from_bool(
                rb.content_type
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .to_ascii_lowercase()
                    .starts_with("text/plain"),
            )
        } else {
            Presence::NotApplicable
        },
        rb.content_type.clone().map(Value::from),
        Some("RFC 9309 §2.3".into()),
    );
    o.push(
        "crawling.robots_orphan_rules",
        Crawling,
        "그룹 밖 Allow/Disallow",
        parsed.map_or(Presence::NotApplicable, |r| {
            Presence::from_bool(r.orphan_rules > 0)
        }),
        parsed.map(|r| json!(r.orphan_rules)),
        Some("RFC 9309 §2.2.2: User-agent 앞의 규칙은 무시됨".into()),
    );
    let unreachable = rb.error.is_some() || rb.status_code.is_some_and(|c| c >= 500);
    o.push(
        "crawling.robots_unreachable",
        Crawling,
        "robots.txt 5xx·네트워크 오류 (전체 차단으로 간주)",
        Presence::from_bool(unreachable),
        Some(json!({ "statusCode": rb.status_code, "error": rb.error })),
        Some("RFC 9309 §2.3.1.4: 접근 불가면 크롤러는 전체 차단으로 가정".into()),
    );
    let (p, v, n) = match &sp.robots_chain {
        None => (Presence::NotApplicable, None, None),
        Some(Err(e)) => (Presence::Unknown, None, Some(e.clone())),
        Some(Ok(h)) => {
            let hops = h.len().saturating_sub(1);
            (Presence::from_bool(hops > 5), Some(json!(hops)), None)
        }
    };
    o.push(
        "crawling.robots_redirect_hops",
        Crawling,
        "robots.txt 리다이렉트 5회 초과",
        p,
        v,
        n.or_else(|| Some("RFC 9309 §2.3.1.2".into())),
    );

    // ===== GEO 논문 신호 =====
    o.push(
        "geo.quotations",
        Geo,
        "인용문 (blockquote·q)",
        Presence::from_bool(s.quotations > 0),
        Some(json!(s.quotations)),
        Some("GEO 논문(Aggarwal 외, KDD 2024): 인용문 추가".into()),
    );
    o.push(
        "geo.statistics",
        Geo,
        "수치·통계 표기",
        Presence::from_bool(s.statistics > 0),
        Some(json!(s.statistics)),
        Some("숫자+단위 표기 수 (휴리스틱). GEO 논문: 통계 추가".into()),
    );
}

/// 응답 헤더의 모든 값 (여러 번 온 헤더는 각각).
fn header_values(h: &HeaderMap, name: &str) -> Vec<String> {
    h.get_all(name)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(str::to_string)
        .collect()
}

/// (항목 id, 표시명, CSP 정책 하나에 대한 판정)
type CspRule = (&'static str, &'static str, fn(&crate::sec::Csp) -> bool);

/// 보안 스캐너(Observatory·webhint·hstspreload·RFC 9116) 대조 항목.
fn security_spec_checks(o: &mut Out, i: &Inputs) {
    use crate::sec;
    use Category::*;
    let f = i.facts;
    let s = &f.sec;
    let h = &i.page.headers;
    let base = &i.page.final_url;
    let https = base.scheme() == "https";
    let list = |v: &[String]| (!v.is_empty()).then(|| json!(v));
    let flag = |b: bool, applicable: bool| {
        if applicable {
            Presence::from_bool(b)
        } else {
            Presence::NotApplicable
        }
    };

    // ===== CSP =====
    let header_csp: Vec<sec::Csp> = header_values(h, "content-security-policy")
        .iter()
        .flat_map(|v| v.split(','))
        .map(sec::parse_csp)
        .collect();
    let mut all_csp = header_csp.clone();
    all_csp.extend(s.meta_csp.iter().map(|v| sec::parse_csp(v)));
    let has_csp = !all_csp.is_empty();
    // 여러 정책은 모두 통과해야 하므로, 모든 정책이 허용할 때만 문제로 본다
    let every = |pred: fn(&sec::Csp) -> bool| has_csp && all_csp.iter().all(pred);
    let meta_ignored: Vec<&str> = s
        .meta_csp
        .iter()
        .flat_map(|m| {
            let c = sec::parse_csp(m);
            ["frame-ancestors", "report-uri", "sandbox"]
                .into_iter()
                .filter(move |d| c.get(d).is_some())
        })
        .collect();
    o.push(
        "security.csp_meta",
        Security,
        "meta 태그 CSP",
        Presence::from_bool(!s.meta_csp.is_empty()),
        (!s.meta_csp.is_empty())
            .then(|| json!({ "policies": s.meta_csp, "ignoredDirectives": meta_ignored })),
        Some("meta CSP 는 frame-ancestors·report-uri·sandbox 를 무시함".into()),
    );
    let csp_items: [CspRule; 10] = [
        (
            "security.csp_unsafe_inline",
            "CSP script-src 'unsafe-inline' 허용",
            sec::Csp::unsafe_inline,
        ),
        (
            "security.csp_unsafe_eval",
            "CSP 'unsafe-eval' 허용",
            sec::Csp::unsafe_eval,
        ),
        (
            "security.csp_broad_script_source",
            "CSP script-src 과다 허용 (*·https: 등)",
            sec::Csp::broad_script,
        ),
        (
            "security.csp_object_src_unrestricted",
            "CSP object-src 미제한",
            sec::Csp::object_unrestricted,
        ),
        (
            "security.csp_unsafe_inline_style",
            "CSP style-src 'unsafe-inline' 허용",
            sec::Csp::unsafe_inline_style,
        ),
        (
            "security.csp_base_uri_unrestricted",
            "CSP base-uri 미제한",
            sec::Csp::base_uri_unrestricted,
        ),
        (
            "security.csp_form_action_unrestricted",
            "CSP form-action 미제한",
            sec::Csp::form_action_unrestricted,
        ),
        (
            "security.csp_default_src_missing",
            "CSP default-src 없음",
            |c| c.get("default-src").is_none(),
        ),
        (
            "security.csp_strict_dynamic_without_nonce",
            "CSP strict-dynamic 에 nonce·hash 없음",
            sec::Csp::strict_dynamic_without_nonce,
        ),
        (
            "security.csp_duplicate_directives",
            "CSP 지시어 중복",
            |c| !c.duplicates.is_empty(),
        ),
    ];
    for (id, label, pred) in csp_items {
        let any = has_csp && all_csp.iter().any(pred);
        let value = if matches!(
            id,
            "security.csp_strict_dynamic_without_nonce" | "security.csp_duplicate_directives"
        ) {
            any
        } else {
            every(pred)
        };
        o.push(
            id,
            Security,
            label,
            flag(value, has_csp),
            None,
            Some("Mozilla HTTP Observatory 기준".into()),
        );
    }
    o.push(
        "security.csp_insecure_scheme",
        Security,
        "CSP 에 http: 출처",
        flag(
            all_csp.iter().any(sec::Csp::insecure_scheme),
            has_csp && https,
        ),
        None,
        None,
    );
    let report_only = header(h, "content-security-policy-report-only");
    o.push(
        "security.csp_report_only_only",
        Security,
        "CSP 보고 전용만 설정",
        flag(report_only.is_some(), !has_csp),
        report_only.map(Value::from),
        None,
    );
    let xfo = header(h, "x-frame-options").map(|v| v.trim().to_ascii_lowercase());
    let xfo_ok = xfo
        .as_deref()
        .is_some_and(|v| matches!(v, "deny" | "sameorigin"));
    o.push(
        "security.clickjacking_unprotected",
        Security,
        "클릭재킹 방어 없음 (X-Frame-Options·frame-ancestors)",
        Presence::from_bool(
            !xfo_ok && !header_csp.iter().any(sec::Csp::frame_ancestors_restricted),
        ),
        None,
        Some("meta CSP 의 frame-ancestors 는 무시됨".into()),
    );

    // ===== 헤더 값 =====
    o.push(
        "security.x_frame_options_invalid",
        Security,
        "X-Frame-Options 값 오류",
        flag(!xfo_ok, xfo.is_some()),
        xfo.clone().map(Value::from),
        Some("DENY·SAMEORIGIN 만 유효 (ALLOW-FROM 은 무시됨)".into()),
    );
    let xcto = header(h, "x-content-type-options");
    o.push(
        "security.x_content_type_options_invalid",
        Security,
        "X-Content-Type-Options 값 오류",
        flag(
            !xcto
                .as_deref()
                .is_some_and(|v| v.trim().eq_ignore_ascii_case("nosniff")),
            xcto.is_some(),
        ),
        xcto.map(Value::from),
        None,
    );
    const REFERRER: &[&str] = &[
        "no-referrer",
        "no-referrer-when-downgrade",
        "same-origin",
        "origin",
        "strict-origin",
        "origin-when-cross-origin",
        "strict-origin-when-cross-origin",
        "unsafe-url",
    ];
    let referrer_raw: Vec<String> = header_values(h, "referrer-policy")
        .into_iter()
        .chain(f.meta("referrer").map(str::to_string))
        .collect();
    let effective = referrer_raw
        .iter()
        .flat_map(|v| v.split(','))
        .map(|t| t.trim().to_ascii_lowercase())
        .rfind(|t| REFERRER.contains(&t.as_str()));
    o.push(
        "security.referrer_policy_unsafe",
        Security,
        "안전하지 않은 Referrer-Policy",
        flag(
            effective.as_deref().is_none_or(|t| {
                matches!(
                    t,
                    "origin"
                        | "origin-when-cross-origin"
                        | "unsafe-url"
                        | "no-referrer-when-downgrade"
                )
            }),
            !referrer_raw.is_empty(),
        ),
        (!referrer_raw.is_empty())
            .then(|| json!({ "values": referrer_raw, "effective": effective })),
        Some("헤더와 meta referrer 중 마지막 유효 값 기준 (유효 값이 없으면 문제)".into()),
    );
    let mut co_bad: Vec<String> = Vec::new();
    for (name, allowed) in [
        (
            "cross-origin-opener-policy",
            &[
                "same-origin",
                "same-origin-allow-popups",
                "noopener-allow-popups",
                "unsafe-none",
            ][..],
        ),
        (
            "cross-origin-embedder-policy",
            &["require-corp", "credentialless", "unsafe-none"][..],
        ),
        (
            "cross-origin-resource-policy",
            &["same-site", "same-origin", "cross-origin"][..],
        ),
    ] {
        let vals = header_values(h, name);
        let token = |v: &str| {
            v.split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase()
        };
        if vals.len() > 1 || vals.iter().any(|v| !allowed.contains(&token(v).as_str())) {
            co_bad.push(format!("{name}: {}", vals.join(", ")));
        }
    }
    let co_present = [
        "cross-origin-opener-policy",
        "cross-origin-embedder-policy",
        "cross-origin-resource-policy",
    ]
    .iter()
    .any(|n| h.contains_key(*n));
    o.push(
        "security.cross_origin_policy_invalid",
        Security,
        "COOP·COEP·CORP 값 오류",
        flag(!co_bad.is_empty(), co_present),
        list(&co_bad),
        None,
    );
    let acao = header(h, "access-control-allow-origin");
    o.push(
        "security.cors_wildcard",
        Security,
        "CORS 전체 허용 (Access-Control-Allow-Origin: *)",
        Presence::from_bool(acao.as_deref().is_some_and(|v| v.trim() == "*")),
        acao.map(Value::from),
        None,
    );
    let hpkp: Vec<String> = ["public-key-pins", "public-key-pins-report-only"]
        .into_iter()
        .filter(|n| h.contains_key(*n))
        .map(str::to_string)
        .collect();
    o.push(
        "security.hpkp",
        Security,
        "폐기된 HPKP 헤더",
        Presence::from_bool(!hpkp.is_empty()),
        list(&hpkp),
        None,
    );
    o.push(
        "security.p3p",
        Security,
        "폐기된 P3P 정책",
        Presence::from_bool(h.contains_key("p3p") || s.p3p_link),
        header(h, "p3p").map(Value::from),
        None,
    );
    let xua =
        h.contains_key("x-ua-compatible") || s.http_equiv.iter().any(|e| e == "x-ua-compatible");
    o.push(
        "meta.x_ua_compatible",
        Meta,
        "X-UA-Compatible (IE 전용)",
        Presence::from_bool(xua),
        None,
        None,
    );

    // ===== HSTS =====
    let hsts_raw = header(h, "strict-transport-security");
    let hsts = hsts_raw.as_deref().map(sec::parse_hsts);
    o.push(
        "transport.hsts_invalid",
        Transport,
        "HSTS 헤더 형식 오류",
        flag(
            hsts.as_ref().is_some_and(|x| !x.errors.is_empty()),
            hsts.is_some(),
        ),
        hsts.as_ref().and_then(|x| list(&x.errors)),
        None,
    );
    let hsts_value = hsts.as_ref().map(|x| {
        json!({ "maxAge": x.max_age, "includeSubDomains": x.include_subdomains, "preload": x.preload })
    });
    o.push(
        "transport.hsts_max_age_short",
        Transport,
        "HSTS max-age 6개월 미만",
        hsts.as_ref()
            .and_then(|x| x.max_age)
            .map_or(Presence::NotApplicable, |m| {
                Presence::from_bool(m < 15_552_000)
            }),
        hsts_value.clone(),
        Some("Observatory 기준 6개월(15552000초). 값으로 preload 조건(1년)도 판단 가능".into()),
    );
    let mut preload_fail: Vec<&str> = Vec::new();
    if let Some(x) = hsts.as_ref().filter(|x| x.preload) {
        if x.max_age.is_none_or(|m| m < 31_536_000) {
            preload_fail.push("max-age 1년 미만");
        }
        if !x.include_subdomains {
            preload_fail.push("includeSubDomains 없음");
        }
        let same_host_https = i
            .http_version
            .and_then(|r| r.as_ref().ok())
            .is_some_and(|(_, u)| u.scheme() == "https" && u.host_str() == base.host_str());
        if !same_host_https {
            preload_fail.push("http 가 같은 호스트 https 로 리다이렉트되지 않음 (또는 확인 못 함)");
        }
    }
    o.push(
        "transport.hsts_preload_ineligible",
        Transport,
        "HSTS preload 요건 미충족",
        flag(
            !preload_fail.is_empty(),
            hsts.as_ref().is_some_and(|x| x.preload),
        ),
        (!preload_fail.is_empty()).then(|| json!(preload_fail)),
        Some("hstspreload.org 제출 요건".into()),
    );
    let downgrade: Vec<String> = i
        .redirect_chain
        .and_then(|r| r.as_ref().ok())
        .map(|hops| {
            hops.windows(2)
                .filter(|w| w[0].url.starts_with("https://") && w[1].url.starts_with("http://"))
                .map(|w| format!("{} → {}", w[0].url, w[1].url))
                .collect()
        })
        .unwrap_or_default();
    o.push(
        "transport.https_to_http_redirect",
        Transport,
        "https 에서 http 로 하향 리다이렉트",
        if i.redirect_chain.is_some_and(|r| r.is_ok()) {
            Presence::from_bool(!downgrade.is_empty())
        } else {
            Presence::NotApplicable
        },
        list(&downgrade),
        None,
    );

    // ===== 쿠키 =====
    let cookies = sec::check_cookies(&header_values(h, "set-cookie"), https);
    let has_cookies = cookies.cookies > 0;
    for (id, label, v) in [
        (
            "security.cookie_samesite_invalid",
            "쿠키 SameSite 값 오류",
            &cookies.samesite_invalid,
        ),
        (
            "security.session_cookie_flags",
            "세션·CSRF 쿠키 보호 속성 누락",
            &cookies.session_flags,
        ),
        (
            "security.cookie_prefix_invalid",
            "__Host-·__Secure- 쿠키 접두사 오류",
            &cookies.prefix_invalid,
        ),
        (
            "security.cookie_syntax_invalid",
            "Set-Cookie 문법 오류",
            &cookies.syntax_invalid,
        ),
    ] {
        o.push(
            id,
            Security,
            label,
            flag(!v.is_empty(), has_cookies),
            list(v),
            None,
        );
    }

    // ===== SRI·URL =====
    o.push(
        "security.sri_invalid",
        Security,
        "integrity 속성 오류",
        flag(!s.sri_issues.is_empty(), s.sri_elements > 0),
        list(&s.sri_issues),
        Some("형식·sha384 미만·crossorigin 없음·http 로드".into()),
    );
    o.push(
        "security.protocol_relative_urls",
        Security,
        "프로토콜 상대 URL (//)",
        Presence::from_bool(s.protocol_relative_count > 0),
        (s.protocol_relative_count > 0)
            .then(|| json!({ "count": s.protocol_relative_count, "samples": s.protocol_relative })),
        None,
    );

    // ===== 문자 인코딩 =====
    o.push(
        "meta.bom",
        Meta,
        "BOM 으로 시작하는 문서",
        Presence::from_bool(i.page.bom),
        None,
        None,
    );
    let header_cs = header(h, "content-type").and_then(|ct| {
        ct.split(';').find_map(|p| {
            let (k, v) = p.split_once('=')?;
            k.trim()
                .eq_ignore_ascii_case("charset")
                .then(|| v.trim().trim_matches('"').to_string())
        })
    });
    let declared = header_cs.clone().or_else(|| f.charset.clone());
    let enc = declared.as_deref().map(|l| {
        encoding_rs::Encoding::for_label(l.trim().as_bytes())
            .map_or_else(|| l.trim().to_ascii_lowercase(), |e| e.name().to_string())
    });
    o.push(
        "meta.charset_not_utf8",
        Meta,
        "UTF-8 이 아닌 문자 인코딩",
        enc.as_deref().map_or(Presence::NotApplicable, |e| {
            Presence::from_bool(e != "UTF-8")
        }),
        enc.map(Value::from),
        None,
    );
    let offset = i.page.meta_charset_offset;
    o.push(
        "meta.charset_late",
        Meta,
        "늦게 선언된 charset (1024바이트 밖·head 첫 요소 아님)",
        if s.charset_metas == 0 {
            Presence::NotApplicable
        } else {
            Presence::from_bool(offset.is_none_or(|n| n > 1024) || !s.charset_first_in_head)
        },
        Some(json!({ "byteOffset": offset, "firstInHead": s.charset_first_in_head })),
        None,
    );
    o.push(
        "meta.charset_duplicate",
        Meta,
        "charset 선언 중복",
        flag(s.charset_metas > 1, s.charset_metas > 0),
        Some(json!(s.charset_metas)),
        None,
    );
    let ct = header(h, "content-type");
    o.push(
        "transport.content_type_invalid",
        Transport,
        "Content-Type 헤더 누락·오류",
        Presence::from_bool(!ct.as_deref().is_some_and(|v| {
            let m = v
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();
            m == "text/html" || m == "application/xhtml+xml"
        })),
        ct.map(Value::from),
        None,
    );

    // ===== 마크업·메타 값 =====
    o.push(
        "content.button_type_missing",
        Content,
        "폼 안의 type 없는 button (submit 으로 동작)",
        count_presence(s.buttons_without_type, s.buttons_in_forms),
        Some(json!(s.buttons_without_type)),
        None,
    );
    const VIEWPORT_KEYS: &[&str] = &[
        "width",
        "height",
        "initial-scale",
        "minimum-scale",
        "maximum-scale",
        "user-scalable",
        "viewport-fit",
        "interactive-widget",
    ];
    let mut vp_bad: Vec<String> = Vec::new();
    if let Some(v) = f.meta("viewport") {
        let pairs: Vec<(String, String)> = v
            .split([',', ';'])
            .filter_map(|p| {
                let (k, val) = p.split_once('=')?;
                Some((
                    k.trim().to_ascii_lowercase(),
                    val.trim().to_ascii_lowercase(),
                ))
            })
            .collect();
        for (k, val) in &pairs {
            if !VIEWPORT_KEYS.contains(&k.as_str()) {
                vp_bad.push(format!("알 수 없는 키: {k}"));
            }
            if k == "width" && val != "device-width" && val.parse::<f64>().is_err() {
                vp_bad.push(format!("width 값 오류: {val}"));
            }
        }
        match pairs.iter().find(|(k, _)| k == "initial-scale") {
            None => vp_bad.push("initial-scale 없음".into()),
            Some((_, val)) if val.parse::<f64>().ok() != Some(1.0) => {
                vp_bad.push(format!("initial-scale={val}"))
            }
            _ => {}
        }
        if s.viewports > 1 {
            vp_bad.push("viewport 중복".into());
        }
    }
    o.push(
        "meta.viewport_invalid",
        Meta,
        "viewport 값 오류",
        flag(!vp_bad.is_empty(), s.viewports > 0),
        list(&vp_bad),
        Some("webhint meta-viewport (확대 차단은 accessibility.zoom_disabled)".into()),
    );
    let bad_colors: Vec<String> = s
        .theme_colors
        .iter()
        .filter(|c| !sec::is_css_color(c))
        .cloned()
        .collect();
    o.push(
        "meta.theme_color_invalid",
        Meta,
        "theme-color 값 오류",
        flag(!bad_colors.is_empty(), !s.theme_colors.is_empty()),
        list(&bad_colors),
        None,
    );
    let cc = header(h, "cache-control").map(|v| v.to_ascii_lowercase());
    let mut cc_bad: Vec<String> = Vec::new();
    if let Some(v) = &cc {
        const KNOWN: &[&str] = &[
            "max-age",
            "s-maxage",
            "no-cache",
            "no-store",
            "no-transform",
            "must-revalidate",
            "proxy-revalidate",
            "must-understand",
            "private",
            "public",
            "immutable",
            "stale-while-revalidate",
            "stale-if-error",
        ];
        let ds: Vec<(&str, Option<&str>)> = v
            .split(',')
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .map(|d| {
                d.split_once('=')
                    .map_or((d, None), |(k, x)| (k.trim(), Some(x.trim())))
            })
            .collect();
        for (k, x) in &ds {
            if !KNOWN.contains(k) {
                cc_bad.push(format!("알 수 없는 지시어: {k}"));
            }
            if matches!(*k, "max-age" | "s-maxage")
                && !x.is_some_and(|x| x.trim_matches('"').parse::<u64>().is_ok())
            {
                cc_bad.push(format!("{k} 값 오류"));
            }
        }
        let has = |k: &str| ds.iter().any(|(d, _)| *d == k);
        if (has("max-age") || has("s-maxage")) && (has("no-cache") || has("no-store")) {
            cc_bad.push("max-age 와 no-cache·no-store 동시 사용".into());
        }
    }
    o.push(
        "performance.cache_control_conflict",
        Performance,
        "Cache-Control 지시어 충돌·오류",
        flag(!cc_bad.is_empty(), cc.is_some()),
        list(&cc_bad),
        None,
    );
    let compressed = i
        .content_encoding
        .and_then(|r| r.as_ref().ok())
        .and_then(|e| e.as_ref());
    let vary_ok = header(h, "vary").is_some_and(|v| {
        v.split(',')
            .any(|t| t.trim().eq_ignore_ascii_case("accept-encoding") || t.trim() == "*")
    });
    o.push(
        "performance.vary_accept_encoding_missing",
        Performance,
        "압축 응답에 Vary: Accept-Encoding 없음",
        flag(!vary_ok, compressed.is_some()),
        compressed.map(|e| json!(e)),
        None,
    );

    // ===== manifest 세부 (webhint) =====
    let manifest = i
        .extended
        .manifest
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .and_then(|(_, j)| j.as_ref());
    let mstr = |k: &str| {
        manifest
            .and_then(|j| j.get(k))
            .and_then(Value::as_str)
            .map(str::trim)
    };
    let (name, short) = (mstr("name"), mstr("short_name"));
    let len = |s: Option<&str>| s.map_or(0, |x| x.chars().count());
    let mut name_bad: Vec<&str> = Vec::new();
    if name.is_none_or(str::is_empty) {
        name_bad.push("name 없음");
    } else if len(name) > 30 {
        name_bad.push("name 30자 초과");
    }
    if len(short) > 12 || (short.is_none_or(str::is_empty) && len(name) > 12) {
        name_bad.push("short_name 12자 초과 또는 없음");
    }
    o.push(
        "meta.manifest_name_length",
        Meta,
        "manifest name·short_name 길이",
        flag(!name_bad.is_empty(), manifest.is_some()),
        (!name_bad.is_empty()).then(|| json!(name_bad)),
        Some("webhint manifest-app-name (name ≤30, short_name ≤12)".into()),
    );
    o.push(
        "meta.manifest_file_extension",
        Meta,
        "manifest 확장자 (.webmanifest)",
        f.manifest.as_deref().map_or(Presence::NotApplicable, |m| {
            Presence::from_bool(
                m.split(['?', '#'])
                    .next()
                    .unwrap_or_default()
                    .ends_with(".webmanifest"),
            )
        }),
        f.manifest.clone().map(Value::from),
        None,
    );
    let manifest_url = f
        .manifest
        .as_deref()
        .and_then(|m| f.link_base(base).join(m).ok());
    let scope_bad = manifest.zip(manifest_url.as_ref()).map(|(j, mu)| {
        let resolve = |k: &str| {
            j.get(k)
                .and_then(Value::as_str)
                .and_then(|v| mu.join(v).ok())
        };
        let start = resolve("start_url");
        let scope = resolve("scope").unwrap_or_else(|| start.clone().unwrap_or_else(|| mu.clone()));
        let scope_dir = scope.as_str()[..=scope.as_str().rfind('/').unwrap_or(0)].to_string();
        match start {
            None => Some("start_url 없음"),
            Some(s) if s.origin() != base.origin() => Some("start_url 이 다른 출처"),
            Some(s) if !s.as_str().starts_with(&scope_dir) => Some("start_url 이 scope 밖"),
            Some(_) => None,
        }
    });
    o.push(
        "meta.manifest_start_url_scope",
        Meta,
        "manifest start_url 범위 오류",
        scope_bad.map_or(Presence::NotApplicable, |b| {
            Presence::from_bool(b.is_some())
        }),
        scope_bad.flatten().map(Value::from),
        None,
    );

    // ===== security.txt =====
    let wk = &i.well_known.security_txt;
    let txt = wk
        .exists
        .then(|| sec::parse_security_txt(wk.body.as_deref().unwrap_or_default()));
    let expires = txt.as_ref().and_then(|t| {
        t.values("expires")
            .first()
            .and_then(|v| sec::rfc3339_to_unix(v))
    });
    o.push(
        "transport.security_txt_expired",
        Transport,
        "security.txt 만료",
        expires.map_or(Presence::NotApplicable, |e| {
            Presence::from_bool(e < i.extended.now)
        }),
        expires.map(
            |e| json!({ "expiresUnix": e, "moreThanYearAhead": e - i.extended.now > 366 * 86_400 }),
        ),
        Some("RFC 9116 §2.5.5".into()),
    );
    let mut txt_bad: Vec<String> = txt.as_ref().map(|t| t.errors.clone()).unwrap_or_default();
    if wk.exists {
        let ct = wk
            .content_type
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .replace(' ', "");
        if !ct.starts_with("text/plain") || !ct.contains("charset=utf-8") {
            txt_bad.push(format!("Content-Type: {ct}"));
        }
        if !wk.url.starts_with("https://") {
            txt_bad.push("https 가 아님".into());
        }
    }
    o.push(
        "transport.security_txt_invalid",
        Transport,
        "security.txt 형식 위반",
        flag(!txt_bad.is_empty(), wk.exists),
        list(&txt_bad),
        Some("RFC 9116 §2·§3".into()),
    );
    o.push(
        "transport.security_txt_signed",
        Transport,
        "security.txt 전자서명",
        txt.as_ref()
            .map_or(Presence::NotApplicable, |t| Presence::from_bool(t.signed)),
        None,
        Some("RFC 9116 §2.3 (권장)".into()),
    );
    let canonical: Vec<&str> = txt
        .as_ref()
        .map(|t| t.values("canonical"))
        .unwrap_or_default();
    let fetched = wk.final_url.as_deref().unwrap_or(&wk.url);
    o.push(
        "transport.security_txt_canonical_mismatch",
        Transport,
        "security.txt Canonical 에 실제 URL 없음",
        flag(
            !canonical.contains(&fetched) && !canonical.contains(&wk.url.as_str()),
            !canonical.is_empty(),
        ),
        (!canonical.is_empty()).then(|| json!({ "canonical": canonical, "fetched": fetched })),
        None,
    );

    // ===== well-known =====
    let sp = &i.extended.search;
    let soft404 = i
        .not_found
        .as_ref()
        .is_ok_and(|(code, _)| (200..300).contains(code));
    let (p, v) = match &sp.change_password {
        None => (Presence::NotApplicable, None),
        Some(Err(e)) => (Presence::Unknown, Some(json!(e))),
        Some(Ok(hops)) => {
            let first = hops.first().map_or(0, |h| h.status);
            let ok = (300..400).contains(&first) || ((200..300).contains(&first) && !soft404);
            (
                Presence::from_bool(ok),
                Some(json!({ "status": first, "softNotFound": soft404 })),
            )
        }
    };
    o.push(
        "security.change_password_url",
        Security,
        "비밀번호 변경 URL (/.well-known/change-password)",
        p,
        v,
        Some(
            "W3C Change Password URL. 없는 경로도 200 을 주는 사이트는 200 만으로 인정하지 않음"
                .into(),
        ),
    );
    let (p, n) = match &sp.gpc {
        None => (Presence::NotApplicable, None),
        Some(g) if g.error.is_some() => (Presence::Unknown, g.error.clone()),
        Some(g) => (
            Presence::from_bool(
                g.exists
                    && serde_json::from_str::<Value>(g.body.as_deref().unwrap_or_default())
                        .ok()
                        .and_then(|j| j.get("gpc").and_then(Value::as_bool))
                        == Some(true),
            ),
            None,
        ),
    };
    o.push(
        "trust.gpc",
        Trust,
        "Global Privacy Control (gpc.json)",
        p,
        None,
        n,
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
            bom: false,
            meta_charset_offset: None,
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
        llms: FileProbe,
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
                llms: failed("l"),
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
                llms: &self.llms,
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

    #[test]
    fn spec_items_wcag_og_twitter_manifest_schema_headers() {
        let mut fx = Fx::failing();
        let html = r##"<html><head><meta name="viewport" content="width=1024, initial-scale=1, shrink">
<meta property="og:type" content="article"><meta property="article:published_time" content="2026-01-01">
<meta property="article:tag" content="a"><meta property="og:image:type" content="image/png">
<meta name="twitter:card" content="gallery"><meta name="twitter:app:name:iphone" content="앱">
<meta name="twitter:image:alt" content="설명"><style>a{transition: color 1s}</style>
<script type="application/ld+json">[{"@type":"Product"},{"@type":"schema:Offer"},{"@type":"https://schema.org/FakeThing"},
{"@type":"foaf:Person"},{"@type":"Widget"}]</script></head>
<body bgcolor="#fff"><abbr>WWW</abbr><select onchange="this.form.submit()"></select>
<input type="password" autocomplete="off"><p style="text-align:justify">x</p>
<a href="/" onfocus="location.href='/'">a</a><video src="v.mp4"></video></body></html>"##;
        fx.facts = crate::html::extract(html, &fx.page.final_url);
        fx.page
            .headers
            .insert("x-xss-protection", "1; mode=block".parse().unwrap());
        fx.extended.manifest = Some(Ok((
            200,
            Some(json!({
                "id": "/", "scope": "/", "description": "앱", "screenshots": [{}, {}], "shortcuts": [{}],
                "icons": [{"src": "a.png", "purpose": "any maskable"}],
                "theme_color": "#fff", "background_color": "#000", "display": "standalone"
            })),
        )));
        let c = fx.run();
        let p = |id: &str| get(&c, id).presence;
        assert_eq!(p("accessibility.viewport_fixed_width"), Presence::Present);
        assert_eq!(
            get(&c, "accessibility.viewport_fixed_width").value,
            Some(json!("width=1024"))
        );
        assert_eq!(p("accessibility.site_search"), Presence::Missing);
        assert_eq!(p("accessibility.breadcrumb_nav"), Presence::Missing);
        assert_eq!(p("accessibility.abbr_without_title"), Presence::Present);
        assert_eq!(p("accessibility.onchange_navigation"), Presence::Present);
        assert_eq!(p("accessibility.onfocus_navigation"), Presence::Present);
        assert_eq!(
            p("accessibility.video_audio_description_missing"),
            Presence::Present
        );
        assert_eq!(
            p("accessibility.password_autocomplete_off"),
            Presence::Present
        );
        assert_eq!(p("accessibility.reduced_motion"), Presence::Missing);
        assert_eq!(p("accessibility.justified_text"), Presence::Present);
        assert_eq!(p("accessibility.obsolete_attributes"), Presence::Present);
        assert_eq!(p("open_graph.image_type"), Presence::Present);
        assert_eq!(p("open_graph.video"), Presence::Missing);
        assert_eq!(p("open_graph.type_unknown"), Presence::Missing);
        assert_eq!(
            get(&c, "open_graph.type_properties").value,
            Some(json!(["article:published_time", "article:tag"]))
        );
        assert_eq!(p("twitter.image_alt"), Presence::Present);
        assert_eq!(p("twitter.player"), Presence::Missing);
        assert_eq!(
            get(&c, "twitter.app").value,
            Some(json!(["twitter:app:name:iphone"]))
        );
        assert_eq!(p("twitter.card_unknown"), Presence::Present);
        for id in [
            "meta.manifest_id",
            "meta.manifest_scope",
            "meta.manifest_description",
            "meta.manifest_screenshots",
            "meta.manifest_shortcuts",
            "meta.manifest_maskable_icon",
            "meta.manifest_colors",
            "meta.manifest_display_valid",
        ] {
            assert_eq!(p(id), Presence::Present, "{id}");
        }
        assert_eq!(get(&c, "meta.manifest_screenshots").value, Some(json!(2)));
        assert!(get(&c, "meta.manifest_content").value.as_ref().unwrap()["members"].is_array());
        assert_eq!(
            get(&c, "structured_data.unknown_types").value,
            Some(json!(["https://schema.org/FakeThing", "Widget"]))
        );
        assert_eq!(
            get(&c, "security.deprecated_headers").value,
            Some(json!(["x-xss-protection: 1; mode=block"]))
        );

        // 반대 상황: 대상 없음 / 정상 값 / 외부 CSS
        let html = r#"<html><head><meta name="viewport" content="width=device-width, initial-scale=1">
<meta property="og:type" content="music.song"><meta property="music:duration" content="200">
<meta name="twitter:card" content="summary_large_image">
<script type="application/ld+json">{"@type":"BreadcrumbList"}</script></head>
<body><form role="search"></form><abbr title="x">x</abbr><input type="password"></body></html>"#;
        fx.facts = crate::html::extract(html, &fx.page.final_url);
        fx.page.headers.clear();
        fx.extended.external_css = Some(ExternalCss {
            total: 1,
            checked: 1,
            failed: 0,
            bytes: 10,
            facts: crate::css::analyze(
                "a{animation-name:spin} @media (prefers-reduced-motion: reduce){a{animation:none}}",
            ),
        });
        fx.extended.manifest = Some(Ok((
            200,
            Some(json!({
                "id": "", "scope": null, "screenshots": [], "shortcuts": [{}], "icons": [{"purpose": "any"}],
                "theme_color": "#fff", "background_color": 1, "display": "tabs"
            })),
        )));
        let c = fx.run();
        let p = |id: &str| get(&c, id).presence;
        assert_eq!(p("accessibility.viewport_fixed_width"), Presence::Missing);
        assert_eq!(p("accessibility.site_search"), Presence::Present);
        assert_eq!(p("accessibility.breadcrumb_nav"), Presence::Present);
        assert_eq!(p("accessibility.abbr_without_title"), Presence::Missing);
        assert_eq!(
            p("accessibility.password_autocomplete_off"),
            Presence::Missing
        );
        assert_eq!(p("accessibility.reduced_motion"), Presence::Present);
        assert_eq!(p("open_graph.type_properties"), Presence::Present);
        assert_eq!(p("twitter.card_unknown"), Presence::Missing);
        assert_eq!(p("twitter.app"), Presence::Missing);
        assert_eq!(p("structured_data.unknown_types"), Presence::Missing);
        assert_eq!(p("security.deprecated_headers"), Presence::Missing);
        for (id, want) in [
            ("meta.manifest_id", Presence::Missing),
            ("meta.manifest_scope", Presence::Missing),
            ("meta.manifest_description", Presence::Missing),
            ("meta.manifest_screenshots", Presence::Missing),
            ("meta.manifest_shortcuts", Presence::Present),
            ("meta.manifest_maskable_icon", Presence::Missing),
            ("meta.manifest_colors", Presence::Present),
            ("meta.manifest_display_valid", Presence::Missing),
        ] {
            assert_eq!(p(id), want, "{id}");
        }

        // og:type website / 명세에 없는 값, 대상 없음, manifest 해석 불가·없음·실패
        let html =
            r#"<html><head><meta property="og:type" content="website"></head><body></body></html>"#;
        fx.facts = crate::html::extract(html, &fx.page.final_url);
        fx.extended.external_css = None;
        fx.extended.manifest = Some(Ok((200, None)));
        let c = fx.run();
        let p = |id: &str| get(&c, id).presence;
        assert_eq!(p("open_graph.type_unknown"), Presence::Missing);
        assert_eq!(p("open_graph.type_properties"), Presence::NotApplicable);
        assert_eq!(
            p("accessibility.viewport_fixed_width"),
            Presence::NotApplicable
        );
        assert_eq!(
            p("accessibility.abbr_without_title"),
            Presence::NotApplicable
        );
        assert_eq!(
            p("accessibility.video_audio_description_missing"),
            Presence::NotApplicable
        );
        assert_eq!(p("accessibility.onfocus_navigation"), Presence::Missing);
        assert_eq!(p("accessibility.reduced_motion"), Presence::NotApplicable);
        assert_eq!(p("twitter.card_unknown"), Presence::NotApplicable);
        assert_eq!(p("structured_data.unknown_types"), Presence::NotApplicable);
        assert_eq!(p("meta.manifest_id"), Presence::NotApplicable);
        assert!(get(&c, "meta.manifest_id").note.is_some());
        let html =
            r#"<html><head><meta property="og:type" content="blog"></head><body></body></html>"#;
        fx.facts = crate::html::extract(html, &fx.page.final_url);
        fx.extended.manifest = None;
        let c = fx.run();
        assert_eq!(
            get(&c, "open_graph.type_unknown").presence,
            Presence::Present
        );
        assert_eq!(
            get(&c, "open_graph.type_properties").presence,
            Presence::NotApplicable
        );
        assert_eq!(
            get(&c, "meta.manifest_colors").presence,
            Presence::NotApplicable
        );
        fx.extended.manifest = Some(Err("timeout".into()));
        let c = fx.run();
        assert_eq!(get(&c, "meta.manifest_colors").presence, Presence::Unknown);
    }

    #[test]
    fn search_engine_guide_items() {
        let mut fx = Fx::failing();
        let html = r##"<html><head><title>서울 맛집 서울 맛집</title><meta name="description" content="서울 맛집  서울 맛집">
<meta charset="euc-kr"><link rel="canonical" href="/x"><link rel="icon" href="/f.png"><link rel="icon" href="https://a.com/g.png">
<meta name="robots" content="noindex, nosourceinfo, noarchive"><meta name="bingbot" content="nocache, ai-training:disallow">
<meta name="twitter:domain" content="a.com"><meta name="naver" content="NOSUBLINKS"><script src="/app.js?t=1712345678"></script><link rel="stylesheet" href="/s.css">
<script type="application/ld+json">[{"@type":"Person"},{"@type":"Movie","name":"m","actor":"a"},
{"@type":"VideoObject","name":"v"}]</script></head>
<body><link rel="canonical" href="/y"><a href="#!/x">h</a><a href="/q?a=1&b=2&c=3">q</a><a href="fn()">f</a><img src="IMG_0001.jpg">
<div class="h-entry"></div><span typeof="Thing"></span><a href="/n" rel="nosublink">n</a><p data-snippet>s</p></body></html>"##;
        fx.page = page("https://m.my_site.github.io/", html);
        fx.page
            .headers
            .insert("content-type", "text/html; charset=UTF-8".parse().unwrap());
        fx.page.headers.insert(
            "x-robots-tag",
            "bingbot: noarchive, otherbot: noindex".parse().unwrap(),
        );
        fx.facts = crate::html::extract(html, &fx.page.final_url);
        fx.robots = found(
            "https://a.com/robots.txt",
            "User-agent: *\nDisallow: /\n\nUser-agent: __usage_ai-training\nDisallow: /\n",
        );
        fx.not_found = Ok((
            404,
            r#"<script src="https://ads.example/a.js"></script>"#.into(),
        ));
        let hops = |n: usize| {
            (0..=n)
                .map(|k| Hop {
                    url: format!("https://a.com/{k}"),
                    status: 301,
                })
                .collect::<Vec<_>>()
        };
        fx.redirect_chain = Some(Ok(hops(6)));
        let c = fx.run();
        let p = |id: &str| get(&c, id).presence;
        for id in [
            "meta.canonical_relative",
            "meta.favicon_relative_href",
            "meta.charset_mismatch",
            "meta.head_elements_in_body",
            "meta.description_same_as_title",
            "meta.repeated_terms",
            "twitter.domain",
            "indexing.naver_directives",
            "indexing.bingbot_meta",
            "geo.copilot_restrictions",
            "geo.bing_ai_preferences",
            "indexing.noindex_blocked_by_robots",
            "content.fragment_route_links",
            "content.pseudo_js_links",
            "content.img_generic_filenames",
            "content.parameter_heavy_links",
            "crawling.script_cache_busting",
            "crawling.robots_blocks_resources_yeti",
            "crawling.robots_blocks_resources_bingbot",
            "crawling.robots_blocks_favicon",
            "structured_data.rdfa",
            "structured_data.microformats",
            "structured_data.naver.movie",
            "transport.hostname_invalid",
            "transport.hosting_subdomain",
            "transport.redirect_hops_excess",
            "not_found_page.third_party_calls",
            "indexing.snippet_controls",
            "meta.favicon_duplicate_rel",
        ] {
            assert_eq!(p(id), Presence::Present, "{id}");
        }
        for id in [
            "meta.mobile_site_canonical",
            "structured_data.required.person",
            "structured_data.naver.video",
            "open_graph.image_socialonly",
        ] {
            assert_eq!(p(id), Presence::Missing, "{id}");
        }
        assert_eq!(p("structured_data.naver.recipe"), Presence::NotApplicable);
        assert_eq!(
            get(&c, "geo.copilot_restrictions").value,
            Some(json!(["nocache", "noarchive"]))
        );
        assert_eq!(
            get(&c, "transport.hosting_subdomain").value,
            Some(json!("github.io"))
        );
        assert_eq!(
            get(&c, "indexing.noindex_blocked_by_robots").value,
            Some(json!(["Googlebot", "Bingbot", "Yeti"]))
        );

        // 반대 상황
        let html = r#"<html><head><title>제목</title><meta name="description" content="다른 설명">
<link rel="canonical" href="https://www.a.com/"><link rel="icon" href="https://a.com/f.png"><link rel="icon" href="https://a.com/g.png">
<meta property="og:image" content="o.jpg" socialonly></head><body><a href="/a">a</a><img src="제품.jpg"></body></html>"#;
        fx.page = page("https://m.a.com/", html);
        fx.page.headers.clear();
        fx.facts = crate::html::extract(html, &fx.page.final_url);
        fx.robots = found("https://a.com/robots.txt", "User-agent: *\nAllow: /\n");
        fx.not_found = Ok((404, "<p>없음</p>".into()));
        fx.redirect_chain = Some(Ok(vec![
            Hop {
                url: "https://a.com/".into(),
                status: 301,
            },
            Hop {
                url: "https://a.com/x".into(),
                status: 301,
            },
            Hop {
                url: "https://a.com/".into(),
                status: 200,
            },
        ]));
        let c = fx.run();
        let p = |id: &str| get(&c, id).presence;
        for id in [
            "meta.canonical_relative",
            "meta.favicon_relative_href",
            "meta.description_same_as_title",
            "meta.repeated_terms",
            "meta.head_elements_in_body",
            "indexing.naver_directives",
            "indexing.bingbot_meta",
            "geo.copilot_restrictions",
            "geo.bing_ai_preferences",
            "content.fragment_route_links",
            "content.pseudo_js_links",
            "content.img_generic_filenames",
            "content.parameter_heavy_links",
            "crawling.script_cache_busting",
            "crawling.robots_blocks_favicon",
            "transport.hostname_invalid",
            "transport.hosting_subdomain",
            "not_found_page.third_party_calls",
        ] {
            assert_eq!(p(id), Presence::Missing, "{id}");
        }
        for id in [
            "meta.favicon_duplicate_rel",
            "meta.mobile_site_canonical",
            "open_graph.image_socialonly",
            "transport.redirect_hops_excess",
        ] {
            assert_eq!(p(id), Presence::Present, "{id}");
        }
        for id in [
            "meta.charset_mismatch",
            "indexing.noindex_blocked_by_robots",
            "crawling.robots_blocks_resources_yeti",
            "structured_data.naver.video",
        ] {
            assert_eq!(p(id), Presence::NotApplicable, "{id}");
        }

        // 대상 없음·요청 실패
        let html = r#"<html><head><meta name="robots" content="none"></head><body></body></html>"#;
        fx.page = page("http://127.0.0.1/", html);
        fx.facts = crate::html::extract(html, &fx.page.final_url);
        fx.robots = failed("https://a.com/robots.txt");
        fx.not_found = err();
        fx.redirect_chain = Some(err());
        let c = fx.run();
        let p = |id: &str| get(&c, id).presence;
        for id in [
            "meta.canonical_relative",
            "meta.favicon_relative_href",
            "meta.favicon_duplicate_rel",
            "meta.description_same_as_title",
            "meta.repeated_terms",
            "content.fragment_route_links",
            "content.img_generic_filenames",
            "transport.hostname_invalid",
            "meta.mobile_site_canonical",
        ] {
            assert_eq!(p(id), Presence::NotApplicable, "{id}");
        }
        for id in [
            "indexing.noindex_blocked_by_robots",
            "crawling.robots_blocks_resources_bingbot",
            "crawling.robots_blocks_favicon",
            "transport.redirect_hops_excess",
            "not_found_page.third_party_calls",
        ] {
            assert_eq!(p(id), Presence::Unknown, "{id}");
        }
        fx.redirect_chain = None;
        let c = fx.run();
        assert_eq!(
            get(&c, "transport.redirect_hops_excess").presence,
            Presence::NotApplicable
        );
        fx.redirect_chain = Some(Ok(vec![Hop {
            url: "https://a.com/".into(),
            status: 200,
        }]));
        let c = fx.run();
        assert_eq!(
            get(&c, "transport.redirect_hops_excess").presence,
            Presence::NotApplicable
        );
    }

    #[test]
    fn search_network_items() {
        let mut fx = Fx::failing();
        let p = |c: &[Check], id: &str| get(c, id).presence;
        // 수행 안 함 / 요청 실패
        let c = fx.run();
        for id in [
            "crawling.feed_valid",
            "crawling.feed_full_content",
            "meta.bing_site_auth_file",
            "not_found_page.redirects",
            "crawling.indexnow_key_file",
            "crawling.sitemap_redirected_urls",
        ] {
            assert_eq!(p(&c, id), Presence::NotApplicable, "{id}");
        }
        assert_eq!(p(&c, "crawling.robots_encoding"), Presence::Unknown);
        assert_eq!(
            p(&c, "transport.redirect_chain_robots_blocked"),
            Presence::Unknown
        );
        fx.redirect_chain = None;
        let c = fx.run();
        assert_eq!(
            p(&c, "transport.redirect_chain_robots_blocked"),
            Presence::NotApplicable
        );
        fx.extended.search = SearchProbes {
            llms_subpath: None,
            agent_card: None,
            robots_chain: None,
            change_password: None,
            gpc: None,
            feed: Some(failed("https://a.com/feed")),
            bing_site_auth: Some(failed("https://a.com/BingSiteAuth.xml")),
            not_found_chain: Some(err()),
            chain_robots: vec![("b.com".into(), failed("https://b.com/robots.txt"))],
            indexnow: Some((
                "0123456789abcdef".into(),
                Some(failed("https://a.com/k.txt")),
            )),
        };
        fx.redirect_chain = Some(Ok(vec![
            Hop {
                url: "https://b.com/".into(),
                status: 301,
            },
            Hop {
                url: "https://a.com/".into(),
                status: 200,
            },
        ]));
        fx.robots = found("https://a.com/robots.txt", "User-agent: *\nAllow: /\n");
        let c = fx.run();
        for id in [
            "crawling.feed_items",
            "meta.bing_site_auth_file",
            "not_found_page.redirects",
            "crawling.indexnow_key_file",
            "transport.redirect_chain_robots_blocked",
        ] {
            assert_eq!(p(&c, id), Presence::Unknown, "{id}");
        }
        assert_eq!(
            p(&c, "crawling.robots_encoding"),
            Presence::Missing,
            "utf8 플래그 없음"
        );

        // 응답은 있지만 조건 불충족
        let mut feed = found("https://a.com/feed", "<html>피드 아님</html>");
        feed.size = Some(11 * 1024 * 1024);
        let mut big = found(
            "https://a.com/feed",
            "<rss><channel><item><link>https://a.com/1</link></item></channel></rss>",
        );
        big.size = Some(11 * 1024 * 1024);
        fx.extended.search = SearchProbes {
            llms_subpath: None,
            agent_card: None,
            robots_chain: None,
            change_password: None,
            gpc: None,
            feed: Some(feed),
            bing_site_auth: Some(found("https://a.com/BingSiteAuth.xml", "<users></users>")),
            not_found_chain: Some(Ok(vec![
                Hop {
                    url: "https://a.com/nope".into(),
                    status: 302,
                },
                Hop {
                    url: "https://a.com/".into(),
                    status: 200,
                },
            ])),
            chain_robots: vec![(
                "b.com".into(),
                FileProbe {
                    status_code: Some(404),
                    ..Default::default()
                },
            )],
            indexnow: Some((
                "0123456789abcdef".into(),
                Some(found("https://a.com/k.txt", "다른 키")),
            )),
        };
        fx.robots.utf8 = true;
        fx.sitemap_detail.samples = vec![
            ("https://a.com/p/".into(), Ok(sample_page(200))),
            ("https://a.com/old".into(), Ok(sample_page(200))),
            ("https://a.com/x".into(), err()),
        ];
        fx.redirect_chain = Some(Ok(vec![
            Hop {
                url: "not a url".into(),
                status: 301,
            },
            Hop {
                url: "https://b.com/".into(),
                status: 301,
            },
            Hop {
                url: "https://a.com/".into(),
                status: 200,
            },
        ]));
        let c = fx.run();
        assert_eq!(p(&c, "crawling.feed_valid"), Presence::Missing);
        for id in [
            "crawling.feed_items",
            "crawling.feed_oversize",
            "crawling.feed_full_content",
        ] {
            assert_eq!(p(&c, id), Presence::NotApplicable, "{id}");
        }
        for id in [
            "meta.bing_site_auth_file",
            "crawling.indexnow_key_file",
            "transport.redirect_chain_robots_blocked",
        ] {
            assert_eq!(p(&c, id), Presence::Missing, "{id}");
        }
        assert_eq!(p(&c, "crawling.robots_encoding"), Presence::Present);
        assert_eq!(p(&c, "not_found_page.redirects"), Presence::Present);
        assert_eq!(
            get(&c, "crawling.sitemap_redirected_urls").value,
            Some(json!(["https://a.com/old"]))
        );

        fx.extended.search.feed = Some(big);
        fx.extended.search.indexnow = Some(("zz".into(), None));
        fx.robots = FileProbe {
            status_code: Some(404),
            ..Default::default()
        };
        let c = fx.run();
        assert_eq!(p(&c, "crawling.feed_oversize"), Presence::Present);
        assert_eq!(p(&c, "crawling.feed_full_content"), Presence::Missing);
        assert_eq!(p(&c, "crawling.feed_foreign_host"), Presence::Missing);
        assert_eq!(p(&c, "crawling.indexnow_key_file"), Presence::Missing);
        assert_eq!(p(&c, "crawling.robots_encoding"), Presence::NotApplicable);
        fx.redirect_chain = Some(Ok(vec![Hop {
            url: "https://a.com/".into(),
            status: 200,
        }]));
        let c = fx.run();
        assert_eq!(
            p(&c, "transport.redirect_chain_robots_blocked"),
            Presence::NotApplicable
        );
    }

    #[test]
    fn geo_and_protocol_spec_items() {
        let mut fx = Fx::failing();
        let p = |c: &[Check], id: &str| get(c, id).presence;
        // 요청 실패·대상 없음
        let c = fx.run();
        for (id, want) in [
            ("geo.llms_txt_link_format", Presence::NotApplicable),
            ("geo.llms_txt_describedby", Presence::Missing),
            ("geo.llms_txt_subpath", Presence::NotApplicable),
            ("geo.content_signals", Presence::Unknown),
            ("geo.content_usage", Presence::Unknown),
            ("geo.rsl_license", Presence::Missing),
            ("geo.tdm_policy", Presence::NotApplicable),
            ("geo.tdm_reservation_invalid", Presence::NotApplicable),
            ("geo.a2a_agent_card", Presence::NotApplicable),
            ("crawling.robots_content_type", Presence::NotApplicable),
            ("crawling.robots_orphan_rules", Presence::NotApplicable),
            ("crawling.robots_unreachable", Presence::Present),
            ("crawling.robots_redirect_hops", Presence::NotApplicable),
            ("geo.quotations", Presence::Missing),
        ] {
            assert_eq!(p(&c, id), want, "{id}");
        }

        // 모두 갖춘 경우
        let html = r#"<html><head><link rel="describedby" href="/llms.txt"><meta name="tdm-reservation" content="1">
<meta name="tdm-policy" content="https://a.com/policy.json"><script type="application/rsl+xml"><rsl/></script></head><body><blockquote>인용</blockquote><p>30% 증가</p></body></html>"#;
        fx.facts = crate::html::extract(html, &fx.page.final_url);
        fx.page
            .headers
            .insert("content-usage", "train-ai=n".parse().unwrap());
        fx.page.headers.insert(
            "link",
            r#"</l.xml>; rel="license"; type="application/rsl+xml", </docs/llms.txt>; rel=describedby, <x>"#.parse().unwrap(),
        );
        fx.page
            .headers
            .insert("tdm-reservation", "1".parse().unwrap());
        let mut robots = found(
            "https://a.com/robots.txt",
            "Disallow: /x\nUser-agent: *\nContent-Signal: ai-train=no\nContent-Usage: train-ai=n\nLicense: https://a.com/l.xml\nAllow: /\n",
        );
        robots.content_type = Some("text/plain; charset=utf-8".into());
        fx.robots = robots;
        fx.llms = found(
            "https://a.com/llms.txt",
            "# A\n### x\n## Docs\n- bad\n## Optional\n- [a](https://a.com)\n",
        );
        fx.well_known.tdmrep = found(
            "https://a.com/.well-known/tdmrep.json",
            r#"[{"location": "/", "tdm-reservation": 1, "tdm-policy": "https://a.com/p.json"}]"#,
        );
        fx.extended.search.llms_subpath = Some(found("https://a.com/docs/llms.txt", "# Docs"));
        fx.extended.search.agent_card = Some(found(
            "https://a.com/.well-known/agent-card.json",
            r#"{"name":"a","description":"d","version":"1","capabilities":{},"skills":[],"supportedInterfaces":[],
"defaultInputModes":[],"defaultOutputModes":[]}"#,
        ));
        let hops = |n: usize| {
            (0..=n)
                .map(|k| Hop {
                    url: format!("https://a.com/{k}"),
                    status: 301,
                })
                .collect::<Vec<_>>()
        };
        fx.extended.search.robots_chain = Some(Ok(hops(6)));
        let c = fx.run();
        for id in [
            "geo.llms_txt_link_format",
            "geo.llms_txt_extra_headings",
            "geo.llms_txt_optional_section",
            "geo.llms_txt_describedby",
            "geo.llms_txt_subpath",
            "geo.content_signals",
            "geo.content_usage",
            "geo.rsl_license",
            "geo.tdm_policy",
            "geo.a2a_agent_card",
            "crawling.robots_content_type",
            "crawling.robots_orphan_rules",
            "crawling.robots_redirect_hops",
            "geo.quotations",
            "geo.statistics",
        ] {
            assert_eq!(p(&c, id), Presence::Present, "{id}");
        }
        assert_eq!(p(&c, "geo.tdm_reservation_invalid"), Presence::Missing);
        assert_eq!(p(&c, "crawling.robots_unreachable"), Presence::Missing);
        assert_eq!(
            get(&c, "geo.llms_txt_describedby").value,
            Some(json!(["/llms.txt", "/docs/llms.txt"]))
        );

        // 값 오류·불충족
        fx.facts = crate::html::extract("<html><body></body></html>", &fx.page.final_url);
        fx.page.headers.clear();
        fx.page
            .headers
            .insert("tdm-reservation", "yes".parse().unwrap());
        fx.robots = found("https://a.com/robots.txt", "User-agent: *\nAllow: /\n");
        fx.well_known.tdmrep = found(
            "https://a.com/.well-known/tdmrep.json",
            r#"[{"location": "/", "tdm-reservation": 2}]"#,
        );
        fx.extended.search.llms_subpath = Some(failed("https://a.com/docs/llms.txt"));
        fx.extended.search.agent_card = Some(found(
            "https://a.com/.well-known/agent-card.json",
            r#"{"name":"a"}"#,
        ));
        fx.extended.search.robots_chain = Some(err());
        let c = fx.run();
        for (id, want) in [
            ("geo.content_signals", Presence::Missing),
            ("geo.content_usage", Presence::Missing),
            ("geo.tdm_policy", Presence::Missing),
            ("geo.tdm_reservation_invalid", Presence::Present),
            ("geo.a2a_agent_card", Presence::Missing),
            ("geo.llms_txt_subpath", Presence::Unknown),
            ("crawling.robots_redirect_hops", Presence::Unknown),
            ("crawling.robots_content_type", Presence::Missing),
            ("crawling.robots_orphan_rules", Presence::Missing),
        ] {
            assert_eq!(p(&c, id), want, "{id}");
        }
        assert_eq!(
            get(&c, "geo.tdm_reservation_invalid").value,
            Some(json!(["yes", "tdmrep.json"]))
        );
        fx.extended.search.agent_card = Some(failed("https://a.com/.well-known/agent-card.json"));
        fx.extended.search.robots_chain = Some(Ok(hops(1)));
        fx.robots = FileProbe {
            status_code: Some(503),
            ..Default::default()
        };
        let c = fx.run();
        assert_eq!(p(&c, "geo.a2a_agent_card"), Presence::Unknown);
        assert_eq!(p(&c, "crawling.robots_redirect_hops"), Presence::Missing);
        assert_eq!(p(&c, "crawling.robots_unreachable"), Presence::Present);
        fx.extended.search.agent_card = Some(FileProbe {
            status_code: Some(404),
            ..Default::default()
        });
        let c = fx.run();
        assert_eq!(p(&c, "geo.a2a_agent_card"), Presence::Missing);
    }

    #[test]
    fn sitemap_protocol_items() {
        let mut fx = Fx::failing();
        fx.sitemap = found(
            "https://a.com/sitemap.xml",
            "<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\"><url><loc>https://a.com/a?x=1&y=2</loc>\
             <priority>2</priority><changefreq>often</changefreq><lastmod>어제</lastmod></url><url><loc>/rel</loc></url></urlset>",
        );
        fx.sitemap.utf8 = true;
        let c = fx.run();
        for id in [
            "crawling.sitemap_namespace",
            "crawling.sitemap_invalid_priority",
            "crawling.sitemap_invalid_changefreq",
            "crawling.sitemap_invalid_lastmod",
            "crawling.sitemap_unescaped_entities",
            "crawling.sitemap_invalid_loc",
            "crawling.sitemap_encoding",
        ] {
            assert_eq!(get(&c, id).presence, Presence::Present, "{id}");
        }
        assert_eq!(
            get(&c, "crawling.sitemap_out_of_scope").presence,
            Presence::Missing
        );
    }

    #[test]
    fn security_scanner_items() {
        let mut fx = Fx::failing();
        let p = |c: &[Check], id: &str| get(c, id).presence;
        let html = r##"<html><head><meta charset="euc-kr"><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src * 'unsafe-inline' 'unsafe-eval'; frame-ancestors 'none'">
<meta http-equiv="X-UA-Compatible" content="IE=edge"><meta name="referrer" content="unsafe-url">
<meta name="viewport" content="width=abc, foo=1, initial-scale=2"><meta name="viewport" content="width=device-width">
<meta name="theme-color" content="not-a-color"><link rel="manifest" href="/m.json">
<script src="//cdn.x.com/a.js" integrity="sha256-abc"></script></head>
<body><form><button>b</button></form></body></html>"##;
        fx.page = page("https://a.com/", html);
        fx.page.bom = true;
        fx.page.meta_charset_offset = Some(2000);
        fx.facts = crate::html::extract(html, &fx.page.final_url);
        let h = &mut fx.page.headers;
        h.clear();
        h.insert("content-security-policy", "script-src 'self' 'unsafe-inline' https:; style-src 'unsafe-eval' 'unsafe-inline' http://x; script-src y".parse().unwrap());
        h.insert(
            "x-frame-options",
            "ALLOW-FROM https://b.com".parse().unwrap(),
        );
        h.insert("x-content-type-options", "yes".parse().unwrap());
        h.insert("cross-origin-opener-policy", "bogus".parse().unwrap());
        h.insert("access-control-allow-origin", "*".parse().unwrap());
        h.insert("public-key-pins", "pin".parse().unwrap());
        h.insert("p3p", "CP=x".parse().unwrap());
        h.insert(
            "strict-transport-security",
            "max-age=60; preload".parse().unwrap(),
        );
        h.append("set-cookie", "sessionid=1; SameSite=None".parse().unwrap());
        h.append("set-cookie", "__Secure-a=1".parse().unwrap());
        h.append("set-cookie", "bad name=1".parse().unwrap());
        h.insert(
            "content-type",
            "text/plain; charset=euc-kr".parse().unwrap(),
        );
        h.insert(
            "cache-control",
            "max-age=60, no-store, bogus, s-maxage=x".parse().unwrap(),
        );
        h.insert("x-runtime", "0.1".parse().unwrap());
        fx.content_encoding = Some(Ok(Some("br".into())));
        fx.redirect_chain = Some(Ok(vec![
            Hop {
                url: "https://a.com/x".into(),
                status: 301,
            },
            Hop {
                url: "http://a.com/y".into(),
                status: 200,
            },
        ]));
        fx.http_version = Some(Ok((301, u("https://www.b.com/"))));
        fx.extended.manifest = Some(Ok((
            200,
            Some(
                json!({"name": "아주 아주 아주 긴 이름을 가진 웹 애플리케이션 서비스 이름", "start_url": "https://other.com/"}),
            ),
        )));
        fx.well_known.security_txt = found(
            "http://a.com/.well-known/security.txt",
            "Contact: http://a.com/c\nExpires: 2000-01-01T00:00:00Z\nCanonical: https://a.com/other.txt\n",
        );
        fx.extended.now = 1_800_000_000;
        fx.not_found = Ok((200, "soft".into()));
        fx.extended.search.change_password = Some(Ok(vec![Hop {
            url: "https://a.com/.well-known/change-password".into(),
            status: 200,
        }]));
        fx.extended.search.gpc = Some(found(
            "https://a.com/.well-known/gpc.json",
            r#"{"gpc": false}"#,
        ));
        let c = fx.run();
        for id in [
            "security.csp_meta",
            "security.csp_unsafe_inline",
            "security.csp_unsafe_eval",
            "security.csp_object_src_unrestricted",
            "security.csp_unsafe_inline_style",
            "security.csp_base_uri_unrestricted",
            "security.csp_form_action_unrestricted",
            "security.csp_broad_script_source",
            "security.clickjacking_unprotected",
            "security.csp_duplicate_directives",
            "security.csp_insecure_scheme",
            "security.x_frame_options_invalid",
            "security.x_content_type_options_invalid",
            "security.referrer_policy_unsafe",
            "security.cross_origin_policy_invalid",
            "security.cors_wildcard",
            "security.hpkp",
            "security.p3p",
            "meta.x_ua_compatible",
            "transport.hsts_max_age_short",
            "transport.hsts_preload_ineligible",
            "transport.https_to_http_redirect",
            "security.cookie_samesite_invalid",
            "security.session_cookie_flags",
            "security.cookie_prefix_invalid",
            "security.cookie_syntax_invalid",
            "security.sri_invalid",
            "security.protocol_relative_urls",
            "meta.bom",
            "meta.charset_not_utf8",
            "meta.charset_late",
            "meta.charset_duplicate",
            "transport.content_type_invalid",
            "content.button_type_missing",
            "meta.viewport_invalid",
            "meta.theme_color_invalid",
            "performance.cache_control_conflict",
            "performance.vary_accept_encoding_missing",
            "meta.manifest_name_length",
            "meta.manifest_start_url_scope",
            "transport.security_txt_expired",
            "transport.security_txt_invalid",
            "transport.security_txt_canonical_mismatch",
            "security.info_disclosure_headers",
        ] {
            assert_eq!(p(&c, id), Presence::Present, "{id}");
        }
        for id in [
            "security.csp_default_src_missing",
            "transport.hsts_invalid",
            "meta.manifest_file_extension",
            "transport.security_txt_signed",
            "security.change_password_url",
            "trust.gpc",
            "security.csp_strict_dynamic_without_nonce",
        ] {
            assert_eq!(p(&c, id), Presence::Missing, "{id}");
        }
        assert_eq!(
            p(&c, "security.csp_report_only_only"),
            Presence::NotApplicable
        );

        // 깨끗한 응답
        let html = r#"<html><head><meta charset="utf-8"><link rel="manifest" href="/app.webmanifest"></head><body></body></html>"#;
        fx.page = page("https://a.com/", html);
        fx.page.meta_charset_offset = Some(40);
        fx.facts = crate::html::extract(html, &fx.page.final_url);
        let h = &mut fx.page.headers;
        h.clear();
        h.insert(
            "content-security-policy",
            "default-src 'none'; script-src 'nonce-a' 'strict-dynamic'; style-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'self'".parse().unwrap(),
        );
        h.insert("x-frame-options", "DENY".parse().unwrap());
        h.insert("x-content-type-options", "nosniff".parse().unwrap());
        h.insert(
            "referrer-policy",
            "no-referrer, strict-origin-when-cross-origin"
                .parse()
                .unwrap(),
        );
        h.insert(
            "strict-transport-security",
            "max-age=63072000; includeSubDomains; preload"
                .parse()
                .unwrap(),
        );
        h.insert(
            "set-cookie",
            "__Host-s=1; Secure; HttpOnly; Path=/; SameSite=Lax"
                .parse()
                .unwrap(),
        );
        h.insert("content-type", "text/html; charset=utf-8".parse().unwrap());
        h.insert("cache-control", "public, max-age=60".parse().unwrap());
        h.insert("vary", "Accept-Encoding".parse().unwrap());
        fx.redirect_chain = Some(Ok(vec![Hop {
            url: "https://a.com/".into(),
            status: 200,
        }]));
        fx.http_version = Some(Ok((301, u("https://a.com/"))));
        fx.extended.manifest = Some(Ok((
            200,
            Some(json!({"name": "앱", "short_name": "앱", "start_url": "/app/", "scope": "/"})),
        )));
        fx.well_known.security_txt = found(
            "https://a.com/.well-known/security.txt",
            "-----BEGIN PGP SIGNED MESSAGE-----\nContact: mailto:a@a.com\nExpires: 2030-01-01T00:00:00Z\nCanonical: https://a.com/.well-known/security.txt\n",
        );
        fx.well_known.security_txt.content_type = Some("text/plain; charset=utf-8".into());
        fx.not_found = Ok((404, "none".into()));
        fx.extended.search.change_password = Some(Ok(vec![Hop {
            url: "x".into(),
            status: 302,
        }]));
        fx.extended.search.gpc = Some(found(
            "https://a.com/.well-known/gpc.json",
            r#"{"gpc": true}"#,
        ));
        let c = fx.run();
        for id in [
            "security.csp_unsafe_inline",
            "security.csp_unsafe_eval",
            "security.csp_broad_script_source",
            "security.csp_object_src_unrestricted",
            "security.csp_unsafe_inline_style",
            "security.csp_base_uri_unrestricted",
            "security.csp_form_action_unrestricted",
            "security.csp_default_src_missing",
            "security.csp_insecure_scheme",
            "security.clickjacking_unprotected",
            "security.x_frame_options_invalid",
            "security.x_content_type_options_invalid",
            "security.referrer_policy_unsafe",
            "transport.hsts_invalid",
            "transport.hsts_max_age_short",
            "transport.hsts_preload_ineligible",
            "transport.https_to_http_redirect",
            "security.cookie_samesite_invalid",
            "security.session_cookie_flags",
            "security.cookie_prefix_invalid",
            "security.cookie_syntax_invalid",
            "meta.charset_not_utf8",
            "meta.charset_late",
            "meta.charset_duplicate",
            "transport.content_type_invalid",
            "performance.cache_control_conflict",
            "performance.vary_accept_encoding_missing",
            "meta.manifest_name_length",
            "meta.manifest_start_url_scope",
            "transport.security_txt_expired",
            "transport.security_txt_invalid",
            "transport.security_txt_canonical_mismatch",
            "security.cors_wildcard",
            "security.protocol_relative_urls",
            "security.csp_meta",
        ] {
            assert_eq!(p(&c, id), Presence::Missing, "{id}");
        }
        for id in [
            "meta.viewport_invalid",
            "meta.theme_color_invalid",
            "security.sri_invalid",
            "content.button_type_missing",
            "security.cross_origin_policy_invalid",
        ] {
            assert_eq!(p(&c, id), Presence::NotApplicable, "{id}");
        }
        for id in [
            "meta.manifest_file_extension",
            "transport.security_txt_signed",
            "security.change_password_url",
            "trust.gpc",
        ] {
            assert_eq!(p(&c, id), Presence::Present, "{id}");
        }

        // 대상 없음·요청 실패·기타 분기
        let html = r#"<html><head><link rel="manifest" href="/m.json"></head><body></body></html>"#;
        fx.page = page("http://a.com/", html);
        fx.facts = crate::html::extract(html, &fx.page.final_url);
        fx.page.headers.clear();
        fx.page.headers.insert(
            "content-security-policy-report-only",
            "default-src 'self'".parse().unwrap(),
        );
        fx.page.headers.insert(
            "strict-transport-security",
            "includeSubDomains".parse().unwrap(),
        );
        fx.page
            .headers
            .insert("x-ua-compatible", "IE=edge".parse().unwrap());
        fx.redirect_chain = Some(err());
        fx.extended.manifest = Some(Ok((
            200,
            Some(json!({"start_url": "/x/", "scope": "/app/"})),
        )));
        fx.extended.search.change_password = Some(err());
        fx.extended.search.gpc = Some(failed("g"));
        fx.well_known.security_txt = failed("s");
        let c = fx.run();
        assert_eq!(p(&c, "security.csp_report_only_only"), Presence::Present);
        assert_eq!(p(&c, "security.csp_unsafe_inline"), Presence::NotApplicable);
        assert_eq!(p(&c, "transport.hsts_invalid"), Presence::Present);
        assert_eq!(
            p(&c, "transport.hsts_max_age_short"),
            Presence::NotApplicable
        );
        assert_eq!(
            p(&c, "transport.https_to_http_redirect"),
            Presence::NotApplicable
        );
        assert_eq!(p(&c, "meta.charset_late"), Presence::NotApplicable);
        assert_eq!(p(&c, "meta.manifest_start_url_scope"), Presence::Present);
        assert_eq!(p(&c, "meta.manifest_name_length"), Presence::Present);
        assert_eq!(p(&c, "meta.manifest_file_extension"), Presence::Missing);
        assert_eq!(p(&c, "security.change_password_url"), Presence::Unknown);
        assert_eq!(p(&c, "trust.gpc"), Presence::Unknown);
        assert_eq!(p(&c, "meta.x_ua_compatible"), Presence::Present);
        assert_eq!(
            p(&c, "transport.security_txt_expired"),
            Presence::NotApplicable
        );
        fx.extended.manifest = Some(Ok((200, Some(json!({"name": "a"})))));
        fx.facts = crate::html::extract(
            r#"<html><head><link rel="manifest" href="/m.json"></head></html>"#,
            &fx.page.final_url,
        );
        fx.extended.search.change_password = None;
        fx.extended.search.gpc = None;
        let c = fx.run();
        assert_eq!(
            get(&c, "meta.manifest_start_url_scope").value,
            Some(json!("start_url 없음"))
        );
        assert_eq!(
            p(&c, "security.change_password_url"),
            Presence::NotApplicable
        );
        assert_eq!(p(&c, "trust.gpc"), Presence::NotApplicable);
    }
}
