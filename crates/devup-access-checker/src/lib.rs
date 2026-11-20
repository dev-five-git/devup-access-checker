//! URL 하나에 대해 SEO / GEO 구성 요소의 **존재 여부**를 점검한다.
//!
//! 점수나 등급은 산출하지 않는다. 각 항목은 안정적인 `id` 와 [`Presence`] 및 원시 값을 가지며,
//! 점수화는 이 결과를 소비하는 쪽의 몫이다.
//!
//! ```no_run
//! # async fn run() -> Result<(), devup_access_checker::Error> {
//! let report = devup_access_checker::Checker::default().check("example.com").await?;
//! for c in report.missing() {
//!     println!("{} ({})", c.label, c.id);
//! }
//! # Ok(()) }
//! ```

mod a11y;
mod catalog;
mod checks;
mod css;
mod extra;
mod fetch;
mod html;
mod llms;
mod model;
mod more;
mod naver_ld;
mod robots;
mod rules;
mod schema_types;
mod search;
mod signals;
mod sitemap;
mod spec;
mod trust;

use std::time::{Duration, Instant};

use futures::future::join_all;
use futures::stream::{self, StreamExt};
use url::Url;

pub use catalog::CATALOG;
pub use model::{Category, Check, Presence, Report};

pub const DEFAULT_USER_AGENT: &str = concat!(
    "Mozilla/5.0 (compatible; DevupAccessChecker/",
    env!("CARGO_PKG_VERSION"),
    ")"
);

/// 외부 리소스 분석 상한
const MAX_CSS_FILES: usize = 30;
const MAX_JS_FILES: usize = 40;
const MAX_IMAGES: usize = 80;

/// 존재하지 않을 경로 (404 페이지 확인용)
const NOT_FOUND_PATH: &str = "/__devup_access_checker_404_probe__";

/// 대상 페이지 자체를 점검할 수 없을 때의 오류. 부속 요청 실패는 오류가 아니라 `unknown` 항목으로 보고한다.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("invalid url: {0}")]
    InvalidUrl(String),
    #[error("failed to fetch {url}: {reason}")]
    Fetch { url: String, reason: String },
}

/// 점검 옵션. `Options::default()` 로 만든 뒤 필요한 필드만 바꿔 쓴다.
///
/// ```
/// let mut opts = devup_access_checker::Options::default();
/// opts.check_ai_user_agents = false;
/// opts.page_samples = 0;
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Options {
    pub user_agent: String,
    /// 대상 페이지 요청 timeout
    pub page_timeout: Duration,
    /// 부속 파일·링크 요청 timeout
    pub probe_timeout: Duration,
    /// favicon / og:image 등 링크 접근 가능 여부 검증
    pub check_links: bool,
    /// AI·검색 크롤러(Yeti·Bingbot) User-Agent 로 페이지를 요청해 응답 확인
    pub check_ai_user_agents: bool,
    /// http:// 버전 리다이렉트, 리다이렉트 체인, www 변형, 압축 여부 확인
    pub check_transport: bool,
    /// sitemap 에서 표본으로 요청해 볼 URL 수 (0 = 생략)
    pub sitemap_samples: usize,
    /// 페이지 내부 링크 중 표본으로 요청해 볼 수 (0 = 생략, `check_links` 가 꺼져 있어도 생략)
    pub internal_link_samples: usize,
    /// 외부 CSS·JS·이미지를 받아 분석 (초점 표시, font-display, 페이지 무게)
    pub check_resources: bool,
    /// 여러 페이지 표본 크롤링 페이지 수 (0 = 생략). sitemap 을 우선 쓰고 부족하면 내부 링크로 채운다.
    pub page_samples: usize,
    /// IndexNow 키. 지정하면 `https://호스트/{키}.txt` 키 파일을 확인한다 (키 파일 이름은 비밀이라 찾아낼 수 없음)
    pub indexnow_key: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            user_agent: DEFAULT_USER_AGENT.to_string(),
            page_timeout: Duration::from_secs(30),
            probe_timeout: Duration::from_secs(10),
            check_links: true,
            check_ai_user_agents: true,
            check_transport: true,
            sitemap_samples: 5,
            internal_link_samples: 10,
            page_samples: 10,
            check_resources: true,
            indexnow_key: None,
        }
    }
}

/// 입력 문자열을 URL 로 정규화한다. 스킴이 없으면 https 를 붙인다.
pub fn normalize_url(input: &str) -> Result<Url, Error> {
    let t = input.trim();
    if t.is_empty() {
        return Err(Error::InvalidUrl("empty".into()));
    }
    let with_scheme = if t.contains("://") {
        t.to_string()
    } else {
        format!("https://{t}")
    };
    let url = Url::parse(&with_scheme).map_err(|e| Error::InvalidUrl(format!("{t}: {e}")))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(Error::InvalidUrl(t.to_string()));
    }
    Ok(url)
}

/// URL 점검기.
#[derive(Debug, Clone, Default)]
pub struct Checker {
    options: Options,
}

impl Checker {
    pub fn new(options: Options) -> Self {
        Self { options }
    }

    pub fn options(&self) -> &Options {
        &self.options
    }

    pub async fn check(&self, input: &str) -> Result<Report, Error> {
        let started = Instant::now();
        let target = normalize_url(input)?;
        let opts = &self.options;
        let client = fetch::build_page_client(&opts.user_agent, opts.page_timeout);
        let probe = fetch::build_client(&opts.user_agent, opts.probe_timeout);

        let mut page = fetch::fetch_page(&client, &target).await?;
        let mut facts = html::extract(&page.body, &page.final_url);
        // 지연 5초 이하 meta refresh 는 검색엔진처럼 리다이렉트로 보고 따라간다 (최대 3회)
        // JavaScript 를 거의 담지 않은 빈 페이지의 location 이동도 따라간다 (Googlebot 은 JS 를 실행해 따라감)
        let mut refresh_hops: Vec<checks::ClientRedirect> = Vec::new();
        for _ in 0..3 {
            let next = match facts
                .meta_refresh
                .as_deref()
                .and_then(signals::parse_refresh)
            {
                Some((delay, url)) if delay <= 5.0 => Some(("meta_refresh", url, Some(delay))),
                _ if facts.body_text_len < 200 => {
                    signals::js_redirect(&page.body).map(|u| ("javascript", u, None))
                }
                _ => None,
            };
            let Some((kind, next, delay)) = next else {
                break;
            };
            let Ok(next_url) = page.final_url.join(&next) else {
                break;
            };
            if next_url == page.final_url || !matches!(next_url.scheme(), "http" | "https") {
                break;
            }
            let Ok(next_page) = fetch::fetch_page(&client, &next_url).await else {
                break;
            };
            refresh_hops.push(checks::ClientRedirect {
                kind,
                from: page.final_url.to_string(),
                to: next_url.to_string(),
                delay,
            });
            facts = html::extract(&next_page.body, &next_page.final_url);
            page = next_page;
        }
        let base = page.final_url.clone();
        let challenge = signals::bot_challenge(&page.body);
        let blocked = match (challenge, page.status) {
            (Some(vendor), status) => Some(format!("봇 차단 페이지({vendor}, HTTP {status})")),
            (None, s) if !(200..300).contains(&s) => Some(format!("HTTP {s} 오류 응답")),
            _ => None,
        };
        let origin = base.join("/").expect("origin");
        let at = |p: &str| origin.join(p).expect("join path");

        let links_fut = async {
            if !opts.check_links {
                return None;
            }
            let one = |raw: Option<String>| {
                let probe = &probe;
                let base = facts.link_base(&base);
                async move {
                    match raw {
                        Some(r) => Some(fetch::probe_link(probe, &r, base).await),
                        None => None,
                    }
                }
            };
            let (favicon, apple_touch_icon, canonical, og_image, twitter_image, manifest, feed) = tokio::join!(
                one(facts.favicon.clone()),
                one(facts.apple_touch_icon.clone()),
                one(facts.canonical.clone()),
                one(facts.meta("og:image").map(str::to_string)),
                one(facts.meta("twitter:image").map(str::to_string)),
                one(facts.manifest.clone()),
                one(facts.feeds.first().cloned()),
            );
            Some(checks::LinkProbes {
                favicon,
                apple_touch_icon,
                canonical,
                og_image,
                twitter_image,
                manifest,
                feed,
            })
        };

        let transport_fut = async {
            if !opts.check_transport {
                return (None, None, None, None, Vec::new());
            }
            let mut http_url = base.clone();
            // 대상은 항상 http(s) 이므로 스킴 변경은 실패하지 않는다
            let _ = http_url.set_scheme("http");
            let http_fut = async {
                // 80 포트를 막아 둔 사이트가 많아 짧게 본다
                let quick = fetch::build_client(
                    &opts.user_agent,
                    opts.probe_timeout.min(Duration::from_secs(5)),
                );
                fetch::probe_status(&quick, &http_url).await
            };
            let enc_fut = probe_encoding(&opts.user_agent, opts.probe_timeout, &base);
            let chain_fut = async {
                let chain =
                    fetch::redirect_chain(&opts.user_agent, opts.probe_timeout, &target, 10).await;
                // 최종 호스트가 아닌 경유 호스트의 robots.txt (최대 3곳)
                let mut hosts: Vec<Url> = Vec::new();
                for h in chain.iter().flatten() {
                    if let Ok(u) = Url::parse(&h.url)
                        && u.host_str() != base.host_str()
                        && !hosts.iter().any(|x| x.host_str() == u.host_str())
                        && hosts.len() < 3
                    {
                        hosts.push(u.join("/robots.txt").expect("join path"));
                    }
                }
                let robots = join_all(hosts.iter().map(|u| {
                    let probe = &probe;
                    async move {
                        let host = u.host_str().unwrap_or_default().to_string();
                        (
                            host,
                            fetch::probe_file(probe, u, fetch::Expected::PlainText).await,
                        )
                    }
                }))
                .await;
                (chain, robots)
            };
            let variant_fut = probe_host_variant(&probe, host_variant(&base));
            let (h, e, (c, r), v) = tokio::join!(http_fut, enc_fut, chain_fut, variant_fut);
            (Some(h), Some(e), Some(c), v, r)
        };

        let ai_fut = async {
            if !opts.check_ai_user_agents {
                return Vec::new();
            }
            join_all(
                checks::AI_USER_AGENTS
                    .iter()
                    .chain(checks::SEARCH_USER_AGENTS)
                    .map(|(id, ua)| {
                        let c = fetch::build_client(ua, opts.probe_timeout);
                        let base = &base;
                        async move { (*id, fetch::probe_ua(&c, base).await) }
                    }),
            )
            .await
        };

        let urls = [
            "/sitemap.xml",
            "/robots.txt",
            "/llms.txt",
            "/llms-full.txt",
            "/ai.txt",
            NOT_FOUND_PATH,
            "/favicon.ico",
            "/apple-touch-icon.png",
            "/.well-known/security.txt",
            "/.well-known/tdmrep.json",
            "/.well-known/apple-app-site-association",
            "/.well-known/assetlinks.json",
            "/ads.txt",
            "/.well-known/ard.json",
            "/.well-known/ai-catalog.json",
        ]
        .map(at);
        let [
            sitemap_url,
            robots_url,
            llms_url,
            llms_full_url,
            ai_url,
            not_found_url,
            favicon_url,
            apple_url,
            security_url,
            tdm_url,
            aasa_url,
            assetlinks_url,
            ads_url,
            ard_url,
            ai_catalog_url,
        ] = &urls;
        use fetch::Expected::{Binary, Json, PlainText, Xml};

        // ── sitemap 체인: robots/sitemap → 선언 sitemap → (index 면 첫 하위) → 표본 페이지 (순서 의존) ──
        let sitemap_chain = async {
            let (sitemap, robots) = tokio::join!(
                fetch::probe_file(&probe, sitemap_url, Xml),
                fetch::probe_file(&probe, robots_url, PlainText),
            );
            let mut declared_urls: Vec<Url> = Vec::new();
            if robots.exists {
                for raw in
                    robots::Robots::parse(robots.body.as_deref().unwrap_or_default()).sitemaps
                {
                    if let Ok(u) = origin.join(&raw)
                        && u != *sitemap_url
                        && !declared_urls.contains(&u)
                        && declared_urls.len() < 5
                    {
                        declared_urls.push(u);
                    }
                }
            }
            let declared = join_all(
                declared_urls
                    .iter()
                    .map(|u| fetch::probe_file(&probe, u, Xml)),
            )
            .await;

            let summary = checks::pick_sitemap(&sitemap, &declared)
                .filter(|sm| !checks::is_gz(sm))
                .map(|sm| sitemap::summarize(sm.body.as_deref().unwrap_or_default()));
            // 페이지 URL 목록: index 면 첫 하위 sitemap 의 URL 을 쓴다
            let mut sample_note: Option<String> =
                (opts.sitemap_samples == 0).then(|| "점검하지 않음 (옵션으로 생략)".to_string());
            let page_locs: Vec<String> = match &summary {
                Some(s) if s.is_index && (opts.sitemap_samples > 0 || opts.page_samples > 0) => {
                    match s.locs.first().and_then(|u| Url::parse(u).ok()) {
                        Some(child) => match fetch::fetch_sitemap_locs(&probe, &child).await {
                            Ok(locs) => locs,
                            Err(e) => {
                                sample_note =
                                    Some(format!("첫 하위 sitemap({child}) 읽기 실패: {e}"));
                                Vec::new()
                            }
                        },
                        None => Vec::new(),
                    }
                }
                Some(s) => s.locs.clone(),
                None => Vec::new(),
            };
            // sitemap 점검용 표본: sitemap 에 실린 페이지 앞에서부터
            let sitemap_sample_urls: Vec<String> = page_locs
                .iter()
                .take(opts.sitemap_samples)
                .cloned()
                .collect();
            let (page_urls, from_sitemap, from_links) = if opts.page_samples > 0 {
                pick_page_samples(
                    &page_locs,
                    &facts.extra.internal_link_urls,
                    &base,
                    opts.page_samples,
                )
            } else {
                (Vec::new(), 0, 0)
            };
            // 두 표본의 합집합을 한 번에 요청
            let mut all: Vec<String> = sitemap_sample_urls.clone();
            for u in &page_urls {
                if !all.contains(u) {
                    all.push(u.clone());
                }
            }
            let fetched: Vec<checks::SampleResult> = join_all(all.into_iter().map(|raw| {
                let probe = &probe;
                async move {
                    let result = match Url::parse(&raw) {
                        Ok(u) => fetch::probe_page(probe, &u).await,
                        Err(e) => Err(format!("invalid url: {e}")),
                    };
                    (raw, result)
                }
            }))
            .await;
            let pick = |urls: &[String]| -> Vec<checks::SampleResult> {
                urls.iter()
                    .filter_map(|u| fetched.iter().find(|(x, _)| x == u).cloned())
                    .collect()
            };
            let samples = pick(&sitemap_sample_urls);
            let site = (opts.page_samples > 0).then(|| checks::SiteSample {
                source: match (from_sitemap > 0, from_links > 0) {
                    (true, true) => "sitemap+internal_links",
                    (true, false) => "sitemap",
                    _ => "internal_links",
                },
                pages: pick(&page_urls),
            });
            (
                sitemap,
                robots,
                checks::SitemapDetail {
                    declared,
                    samples,
                    sample_note,
                },
                site,
            )
        };

        // ── 사이트 루트 파일 ──
        let md_url = markdown_url(&base);
        let files_fut = async {
            tokio::join!(
                fetch::probe_file(&probe, llms_url, PlainText),
                fetch::probe_file(&probe, llms_full_url, PlainText),
                fetch::probe_file(&probe, ai_url, PlainText),
                fetch::probe_file(&probe, favicon_url, Binary),
                fetch::probe_file(&probe, apple_url, Binary),
                fetch::probe_file(&probe, security_url, PlainText),
                fetch::probe_file(&probe, tdm_url, Json),
                fetch::probe_file(&probe, aasa_url, Json),
                fetch::probe_file(&probe, assetlinks_url, Json),
                fetch::probe_file(&probe, ads_url, PlainText),
                fetch::probe_file(&probe, &md_url, PlainText),
                fetch::probe_file(&probe, ard_url, Json),
                fetch::probe_file(&probe, ai_catalog_url, Json),
            )
        };

        // B 그룹: TLS, URL 변형, og:image 파일, 내부 링크 표본, .md 버전
        let tls_fut = async {
            if !opts.check_transport {
                return None;
            }
            // 대상 페이지를 받을 때 함께 얻은 인증서를 쓴다 (추가 요청 없음)
            Some(tls_result(base.scheme(), page.certificate.as_ref()))
        };
        let variants_fut = async {
            if !opts.check_transport {
                return Vec::new();
            }
            join_all(url_variants(&base).into_iter().map(|(kind, url)| {
                let probe = &probe;
                async move {
                    let result = fetch::probe_status(probe, &url).await;
                    checks::UrlVariant { kind, url, result }
                }
            }))
            .await
        };
        let og_fut = async {
            let raw = facts.meta("og:image").filter(|_| opts.check_links)?;
            let Ok(url) = facts.link_base(&base).join(raw) else {
                return Some(Err(format!("invalid url: {raw}")));
            };
            Some(fetch::fetch_bytes(&probe, &url, 8 * 1024 * 1024).await.map(
                |(status, content_type, bytes, truncated)| {
                    let size = imagesize::blob_size(&bytes).ok();
                    checks::ImageInfo {
                        url: url.to_string(),
                        status,
                        content_type,
                        bytes: bytes.len(),
                        truncated,
                        width: size.map(|s| s.width),
                        height: size.map(|s| s.height),
                    }
                },
            ))
        };
        let internal_fut = async {
            if !opts.check_links || opts.internal_link_samples == 0 {
                return None;
            }
            // 내부 링크 URL 은 이미 절대 URL 로 정규화되어 있다
            let urls: Vec<Url> = facts
                .extra
                .internal_link_urls
                .iter()
                .take(opts.internal_link_samples)
                .filter_map(|raw| Url::parse(raw).ok())
                .collect();
            Some(
                join_all(urls.into_iter().map(|u| {
                    let probe = &probe;
                    async move { (u.to_string(), fetch::probe_status(probe, &u).await) }
                }))
                .await,
            )
        };
        let manifest_fut = async {
            let raw = facts.manifest.as_deref().filter(|_| opts.check_links)?;
            let url = facts.link_base(&base).join(raw).ok()?;
            Some(fetch::fetch_bytes(&probe, &url, 1024 * 1024).await.map(
                |(status, _, bytes, _)| {
                    (
                        status,
                        serde_json::from_slice::<serde_json::Value>(&bytes).ok(),
                    )
                },
            ))
        };
        let favicon_fut = async {
            if !opts.check_links {
                return None;
            }
            let url = match facts
                .favicon
                .as_deref()
                .and_then(|f| facts.link_base(&base).join(f).ok())
            {
                Some(u) => u,
                None => favicon_url.clone(),
            };
            Some(fetch::fetch_bytes(&probe, &url, 2 * 1024 * 1024).await.map(
                |(status, content_type, bytes, truncated)| {
                    let size = imagesize::blob_size(&bytes).ok();
                    checks::ImageInfo {
                        url: url.to_string(),
                        status,
                        content_type,
                        bytes: bytes.len(),
                        truncated,
                        width: size.map(|s| s.width),
                        height: size.map(|s| s.height),
                    }
                },
            ))
        };
        let asset_fut = async {
            let path = facts.extra.same_origin_resources.first()?;
            let url = origin.join(path).ok()?;
            Some((url.to_string(), fetch::probe_headers(&probe, &url).await))
        };
        // canonical 대상 / hreflang 대상 / 외부 CSS / 페이지 무게
        let canonical_fut = async {
            let c = facts
                .canonical
                .as_deref()
                .and_then(|c| facts.link_base(&base).join(c).ok())?;
            if c.as_str().trim_end_matches('/') == base.as_str().trim_end_matches('/') {
                return None;
            }
            Some(fetch::probe_page(&probe, &c).await)
        };
        let hreflang_fut = async {
            let targets: Vec<(String, Url)> = facts
                .hreflang
                .iter()
                .filter_map(|h| {
                    facts
                        .link_base(&base)
                        .join(&h.href)
                        .ok()
                        .map(|u| (h.lang.clone(), u))
                })
                .filter(|(_, u)| {
                    u.as_str().trim_end_matches('/') != base.as_str().trim_end_matches('/')
                })
                .take(10)
                .collect();
            join_all(targets.into_iter().map(|(lang, u)| {
                let probe = &probe;
                async move { (lang, u.to_string(), fetch::probe_page(probe, &u).await) }
            }))
            .await
        };
        let css_fut = async {
            if !opts.check_resources {
                return None;
            }
            let urls = &facts.extra.stylesheet_urls;
            let results: Vec<Result<String, String>> = stream::iter(sample(urls, MAX_CSS_FILES))
                .map(|u| {
                    let probe = &probe;
                    async move {
                        let url = Url::parse(u).map_err(|e| e.to_string())?;
                        let (status, _, bytes, _) =
                            fetch::fetch_bytes(probe, &url, 5 * 1024 * 1024).await?;
                        if !(200..300).contains(&status) {
                            return Err(format!("HTTP {status}"));
                        }
                        Ok(String::from_utf8_lossy(&bytes).into_owned())
                    }
                })
                .buffer_unordered(8)
                .collect()
                .await;
            let mut out = checks::ExternalCss {
                total: urls.len(),
                ..Default::default()
            };
            for r in results {
                match r {
                    Ok(text) => {
                        out.checked += 1;
                        out.bytes += text.len();
                        out.facts.merge(&css::analyze(&text));
                    }
                    Err(_) => out.failed += 1,
                }
            }
            Some(out)
        };
        let weight_fut = async {
            if !opts.check_resources {
                return None;
            }
            let x = &facts.extra;
            let js: Vec<(String, usize)> = stream::iter(sample(&x.script_urls, MAX_JS_FILES))
                .map(|u| {
                    let probe = &probe;
                    async move {
                        let url = Url::parse(u).ok()?;
                        let n = fetch::count_bytes(probe, &url, 20 * 1024 * 1024)
                            .await
                            .ok()?;
                        Some((u.clone(), n))
                    }
                })
                .buffer_unordered(8)
                .filter_map(|x| async move { x })
                .collect()
                .await;
            let imgs: Vec<(String, Option<u64>)> = stream::iter(sample(&x.image_urls, MAX_IMAGES))
                .map(|u| {
                    let probe = &probe;
                    async move {
                        let url = Url::parse(u).ok()?;
                        let len = fetch::head_length(probe, &url)
                            .await
                            .ok()
                            .and_then(|(_, len)| len);
                        Some((u.clone(), len))
                    }
                })
                .buffer_unordered(8)
                .filter_map(|x| async move { x })
                .collect()
                .await;
            // 크기를 알아낸 이미지
            let sized: Vec<(String, u64)> = imgs
                .iter()
                .filter_map(|(u, l)| l.map(|l| (u.clone(), l)))
                .collect();
            Some(checks::PageWeight {
                js_total: x.script_urls.len(),
                js_checked: js.len(),
                js_bytes: js.iter().map(|(_, n)| n).sum(),
                images_total: x.image_urls.len(),
                images_checked: imgs.len(),
                images_unknown: imgs.iter().filter(|(_, l)| l.is_none()).count(),
                image_bytes: imgs.iter().filter_map(|(_, l)| *l).sum(),
                large_scripts: checks::largest(
                    js.iter().map(|(u, n)| (u.clone(), *n as u64)),
                    checks::LARGE_SCRIPT_BYTES,
                ),
                large_images: checks::largest(sized.iter().cloned(), checks::LARGE_IMAGE_BYTES),
                large_gifs: checks::largest(
                    sized.iter().filter(|(u, _)| is_gif(u)).cloned(),
                    checks::LARGE_GIF_BYTES,
                ),
            })
        };

        // 검색엔진 가이드: 피드 본문, Bing 인증 파일, 없는 경로 리다이렉트, IndexNow 키 파일
        let search_fut = async {
            let feed = async {
                let url = facts
                    .feeds
                    .first()
                    .and_then(|f| facts.link_base(&base).join(f).ok())
                    .filter(|_| opts.check_links)?;
                Some(fetch::probe_file(&probe, &url, Xml).await)
            };
            let bing_url = at("/BingSiteAuth.xml");
            let bing = fetch::probe_file(&probe, &bing_url, Xml);
            let not_found_chain = async {
                if !opts.check_transport {
                    return None;
                }
                Some(
                    fetch::redirect_chain(&opts.user_agent, opts.probe_timeout, not_found_url, 5)
                        .await,
                )
            };
            let indexnow = async {
                let key = opts.indexnow_key.as_deref()?.trim().to_string();
                let valid = (8..=128).contains(&key.len())
                    && key.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-');
                if !valid {
                    return Some((key, None));
                }
                let url = at(&format!("/{key}.txt"));
                Some((key, Some(fetch::probe_file(&probe, &url, PlainText).await)))
            };
            let llms_subpath = async {
                let path = base.path();
                let dir = &path[..=path.rfind('/').unwrap_or(0)];
                if dir == "/" {
                    return None;
                }
                Some(fetch::probe_file(&probe, &at(&format!("{dir}llms.txt")), PlainText).await)
            };
            let card_url = at("/.well-known/agent-card.json");
            let agent_card = fetch::probe_file(&probe, &card_url, Json);
            let robots_chain = async {
                if !opts.check_transport {
                    return None;
                }
                Some(
                    fetch::redirect_chain(&opts.user_agent, opts.probe_timeout, robots_url, 6)
                        .await,
                )
            };
            let ((feed, bing, not_found_chain, indexnow), (llms_subpath, agent_card, robots_chain)) = tokio::join!(
                async { tokio::join!(feed, bing, not_found_chain, indexnow) },
                async { tokio::join!(llms_subpath, agent_card, robots_chain) },
            );
            checks::SearchProbes {
                llms_subpath,
                agent_card: Some(agent_card),
                robots_chain,
                feed,
                bing_site_auth: Some(bing),
                not_found_chain,
                chain_robots: Vec::new(),
                indexnow,
            }
        };

        // ── 모든 독립 작업을 한 번에 ──
        let (
            (sitemap, robots, sitemap_detail, site),
            (
                llms,
                llms_full,
                ai_txt,
                favicon_ico,
                apple_touch_icon,
                security_txt,
                tdmrep,
                apple_app_site_association,
                assetlinks,
                ads_txt,
                markdown_version,
                ard,
                ai_catalog,
            ),
            not_found,
            links,
            (http_version, encoding, redirect_chain, host_variant, chain_robots),
            ai_ua,
            (tls, url_variants, og_image, internal_samples),
            (manifest, favicon_image, static_asset),
            (canonical_target, hreflang_targets, external_css, page_weight),
            mut search,
        ) = tokio::join!(
            sitemap_chain,
            files_fut,
            fetch::probe_status_body(&probe, not_found_url),
            links_fut,
            transport_fut,
            ai_fut,
            async { tokio::join!(tls_fut, variants_fut, og_fut, internal_fut) },
            async { tokio::join!(manifest_fut, favicon_fut, asset_fut) },
            async { tokio::join!(canonical_fut, hreflang_fut, css_fut, weight_fut) },
            search_fut,
        );
        search.chain_robots = chain_robots;
        let well_known = checks::WellKnown {
            favicon_ico,
            apple_touch_icon,
            security_txt,
            tdmrep,
            apple_app_site_association,
            assetlinks,
            ads_txt,
            ard,
            ai_catalog,
        };

        let extended = checks::Extended {
            canonical_target,
            hreflang_targets,
            external_css,
            page_weight,
            tls,
            url_variants,
            og_image,
            internal_samples,
            markdown_version: Some(markdown_version),
            manifest,
            favicon_image,
            static_asset,
            search,
            now: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or_default(),
        };

        let checks = checks::build(&checks::Inputs {
            page: &page,
            facts: &facts,
            sitemap: &sitemap,
            robots: &robots,
            llms: &llms,
            llms_full: &llms_full,
            ai_txt: &ai_txt,
            not_found: &not_found,
            http_version: http_version.as_ref(),
            content_encoding: encoding.as_ref(),
            links: links.as_ref(),
            ai_ua: &ai_ua,
            well_known: &well_known,
            sitemap_detail: &sitemap_detail,
            redirect_chain: redirect_chain.as_ref(),
            host_variant: host_variant.as_ref(),
            extended: &extended,
            site: site.as_ref(),
            refresh_hops: &refresh_hops,
            blocked: blocked.as_deref(),
            challenge,
        });

        Ok(Report {
            url: target.to_string(),
            final_url: base.to_string(),
            status_code: page.status,
            duration_ms: started.elapsed().as_millis() as u64,
            blocked,
            checks,
        })
    }
}

/// www 변형 호스트를 요청해 본다. 변형이 없으면(IP·단일 레이블 호스트) None.
async fn probe_host_variant(
    probe: &reqwest::Client,
    url: Option<Url>,
) -> Option<checks::HostVariant> {
    let url = url?;
    let result = fetch::probe_status(probe, &url).await;
    Some(checks::HostVariant { url, result })
}

/// 대상 페이지 연결에서 얻은 인증서로 TLS 항목 입력을 만든다.
fn tls_result(
    scheme: &str,
    cert: Option<&fetch::CertInfo>,
) -> Result<Option<fetch::CertInfo>, String> {
    match (scheme, cert) {
        ("https", Some(c)) => Ok(Some(c.clone())),
        ("https", None) => Err("no peer certificate".to_string()),
        _ => Ok(None),
    }
}

/// URL 경로가 .gif 로 끝나는지 (쿼리·조각 제외).
fn is_gif(url: &str) -> bool {
    url.split(['?', '#'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .ends_with(".gif")
}

/// 분석할 표본을 고른다. 페이지가 순서를 바꿔 내려줘도 같은 결과가 나오도록 정렬·중복 제거 후 앞에서 `n` 개.
fn sample(urls: &[String], n: usize) -> Vec<&String> {
    let mut v: Vec<&String> = urls.iter().collect();
    v.sort();
    v.dedup();
    v.truncate(n);
    v
}

/// www 가 붙어 있으면 뗀, 없으면 붙인 루트 URL. IP·localhost·단일 레이블 호스트는 None.
/// 표본 URL 선택: sitemap 우선, 부족하면 내부 링크. 같은 호스트, 대상 페이지 제외, 중복 제거.
/// 반환: (URL 목록, sitemap 에서 온 수, 내부 링크에서 온 수)
fn pick_page_samples(
    sitemap_locs: &[String],
    links: &[String],
    base: &Url,
    n: usize,
) -> (Vec<String>, usize, usize) {
    let key = |u: &Url| {
        format!(
            "{}{}",
            u.host_str().unwrap_or_default(),
            u.path().trim_end_matches('/')
        )
    };
    let base_key = key(base);
    let mut seen = vec![base_key];
    let mut out = Vec::new();
    let mut counts = [0usize; 2];
    for (src, list) in [sitemap_locs, links].into_iter().enumerate() {
        for raw in list {
            if out.len() >= n {
                break;
            }
            let Ok(u) = Url::parse(raw.trim()) else {
                continue;
            };
            if u.host_str() != base.host_str() || !matches!(u.scheme(), "http" | "https") {
                continue;
            }
            let k = key(&u);
            if seen.contains(&k) {
                continue;
            }
            seen.push(k);
            out.push(raw.trim().to_string());
            counts[src] += 1;
        }
    }
    (out, counts[0], counts[1])
}

/// 같은 페이지를 가리킬 수 있는 URL 변형: 끝 슬래시 토글, 대문자 경로, index.html.
fn url_variants(base: &Url) -> Vec<(&'static str, Url)> {
    let mut clean = base.clone();
    clean.set_query(None);
    clean.set_fragment(None);
    let path = clean.path().to_string();
    let with_path = |p: &str| {
        let mut u = clean.clone();
        u.set_path(p);
        u
    };
    let mut out = Vec::new();
    if path != "/" {
        if let Some(stripped) = path.strip_suffix('/') {
            out.push(("trailing_slash", with_path(stripped)));
        } else if !path.rsplit('/').next().unwrap_or_default().contains('.') {
            out.push(("trailing_slash", with_path(&format!("{path}/"))));
        }
        if path.bytes().any(|b| b.is_ascii_lowercase()) {
            out.push(("uppercase", with_path(&path.to_ascii_uppercase())));
        }
    }
    if path.ends_with('/') {
        out.push(("index_html", with_path(&format!("{path}index.html"))));
    }
    out
}

/// 현재 페이지의 마크다운 버전 관례 경로 (`/a/` → `/a/index.md`, `/a` → `/a.md`).
fn markdown_url(base: &Url) -> Url {
    let mut u = base.clone();
    u.set_query(None);
    u.set_fragment(None);
    let path = u.path().to_string();
    if path.ends_with('/') {
        u.set_path(&format!("{path}index.md"));
    } else {
        u.set_path(&format!("{path}.md"));
    }
    u
}

fn host_variant(base: &Url) -> Option<Url> {
    let host = match base.host()? {
        url::Host::Domain(d) => d,
        _ => return None,
    };
    if !host.contains('.') {
        return None;
    }
    let alt = match host.strip_prefix("www.") {
        Some(rest) => rest.to_string(),
        None => format!("www.{host}"),
    };
    let mut u = base.join("/").ok()?;
    u.set_host(Some(&alt)).ok()?;
    Some(u)
}

/// 자동 압축 해제를 끈 클라이언트로 요청해 서버가 실제로 보낸 Content-Encoding 을 확인한다.
async fn probe_encoding(ua: &str, timeout: Duration, url: &Url) -> Result<Option<String>, String> {
    let client = reqwest::Client::builder()
        .user_agent(ua)
        .timeout(timeout)
        .gzip(false)
        .brotli(false)
        .build()
        .expect("http client");
    let res = client
        .get(url.as_str())
        .header(reqwest::header::ACCEPT_ENCODING, "gzip, br")
        .send()
        .await
        .map_err(fetch::describe_owned)?;
    Ok(res
        .headers()
        .get(reqwest::header::CONTENT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_urls() {
        assert_eq!(
            normalize_url("example.com").unwrap().as_str(),
            "https://example.com/"
        );
        assert_eq!(
            normalize_url(" http://a.com/x ").unwrap().as_str(),
            "http://a.com/x"
        );
        assert!(normalize_url("").is_err());
        assert!(normalize_url("ftp://a.com").is_err());
    }

    #[test]
    fn url_variant_paths() {
        let v = |s: &str| {
            url_variants(&Url::parse(s).unwrap())
                .into_iter()
                .map(|(k, u)| format!("{k}:{}", u.path()))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            v("https://a.com/ko/?x=1"),
            vec![
                "trailing_slash:/ko",
                "uppercase:/KO/",
                "index_html:/ko/index.html"
            ]
        );
        assert_eq!(
            v("https://a.com/about"),
            vec!["trailing_slash:/about/", "uppercase:/ABOUT"]
        );
        assert_eq!(v("https://a.com/"), vec!["index_html:/index.html"]);
        assert_eq!(v("https://a.com/a.pdf"), vec!["uppercase:/A.PDF"]);
        assert_eq!(
            markdown_url(&Url::parse("https://a.com/ko/").unwrap()).path(),
            "/ko/index.md"
        );
        assert_eq!(
            markdown_url(&Url::parse("https://a.com/about").unwrap()).path(),
            "/about.md"
        );
    }

    #[test]
    fn page_sample_selection() {
        let base = Url::parse("https://a.com/ko/").unwrap();
        let locs = [
            "https://a.com/ko",
            "https://a.com/about",
            "https://b.com/x",
            "https://a.com/about/",
        ]
        .map(String::from);
        let links = [
            "https://a.com/blog",
            "https://a.com/about",
            "https://a.com/faq",
        ]
        .map(String::from);
        let (urls, s, l) = pick_page_samples(&locs, &links, &base, 3);
        assert_eq!(
            urls,
            vec![
                "https://a.com/about",
                "https://a.com/blog",
                "https://a.com/faq"
            ]
        );
        assert_eq!((s, l), (1, 2));
    }

    #[test]
    fn host_variants() {
        let v = |s: &str| host_variant(&Url::parse(s).unwrap()).map(|u| u.to_string());
        assert_eq!(
            v("https://devfive.kr/ko/").as_deref(),
            Some("https://www.devfive.kr/")
        );
        assert_eq!(
            v("https://www.devfive.kr/").as_deref(),
            Some("https://devfive.kr/")
        );
        assert_eq!(v("http://127.0.0.1:8080/"), None);
        assert_eq!(v("http://localhost/"), None);
    }

    #[test]
    fn sample_is_order_independent() {
        let a: Vec<String> = ["c", "a", "b", "a"].map(String::from).to_vec();
        let b: Vec<String> = ["b", "a", "c"].map(String::from).to_vec();
        assert_eq!(sample(&a, 2), sample(&b, 2));
        assert_eq!(sample(&a, 10).len(), 3);
    }

    #[test]
    fn gif_detection_ignores_query() {
        assert!(is_gif("https://a.com/x.GIF?v=1"));
        assert!(!is_gif("https://a.com/x.png#gif"));
    }

    #[test]
    fn checker_keeps_options() {
        let o = Options {
            page_samples: 3,
            ..Default::default()
        };
        assert_eq!(Checker::new(o).options().page_samples, 3);
    }

    #[test]
    fn tls_result_by_scheme_and_certificate() {
        let cert = fetch::CertInfo {
            subject: "s".into(),
            issuer: "i".into(),
            not_after: 1,
            not_after_text: "t".into(),
        };
        assert!(tls_result("https", Some(&cert)).unwrap().is_some());
        assert!(tls_result("https", None).is_err());
        assert!(tls_result("http", None).unwrap().is_none());
    }

    #[tokio::test]
    async fn host_variant_probe_runs_only_with_variant() {
        let client = fetch::build_client("t", Duration::from_secs(5));
        assert!(probe_host_variant(&client, None).await.is_none());
        // 닫힌 포트: 결과는 오류지만 변형은 기록된다
        let url = Url::parse("http://127.0.0.1:9/").unwrap();
        let v = probe_host_variant(&client, Some(url)).await.unwrap();
        assert!(v.result.is_err());
    }

    #[test]
    fn page_samples_stop_at_limit() {
        let base = Url::parse("https://a.com/").unwrap();
        let locs: Vec<String> = (0..5).map(|i| format!("https://a.com/s{i}")).collect();
        let links: Vec<String> = (0..5).map(|i| format!("https://a.com/l{i}")).collect();
        let (urls, s, l) = pick_page_samples(&locs, &links, &base, 2);
        assert_eq!((urls.len(), s, l), (2, 2, 0));
    }

    #[tokio::test]
    async fn probe_encoding_reports_header_or_error() {
        use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};
        let s = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).insert_header("content-encoding", "br"))
            .mount(&s)
            .await;
        let url = Url::parse(&s.uri()).unwrap();
        let t = Duration::from_secs(5);
        assert_eq!(
            probe_encoding("t", t, &url).await.unwrap().as_deref(),
            Some("br")
        );
        let closed = Url::parse("http://127.0.0.1:9/").unwrap();
        assert!(probe_encoding("t", t, &closed).await.is_err());
    }
}
