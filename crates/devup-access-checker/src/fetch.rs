//! HTTP 요청과 부속 리소스 probe.

use std::time::{Duration, Instant};

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use reqwest::{Client, Method, Response, header::HeaderMap, redirect::Policy};
use tokio::sync::Semaphore;
use url::Url;

use crate::Error;

/// 같은 호스트에 동시에 보내는 요청 수 상한 (순간 폭주로 인한 차단·timeout 방지).
const PER_HOST_CONCURRENCY: usize = 8;

static HOST_LIMITS: LazyLock<Mutex<HashMap<String, Arc<Semaphore>>>> =
    LazyLock::new(Default::default);

fn host_limit(url: &str) -> Arc<Semaphore> {
    let host = Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_default();
    let mut map = HOST_LIMITS.lock().unwrap_or_else(|e| e.into_inner());
    // 아무도 쓰지 않는 항목은 정리한다 (장시간 실행되는 서비스에서 맵이 무한히 커지지 않도록)
    if map.len() > 256 {
        map.retain(|_, s| Arc::strong_count(s) > 1);
    }
    map.entry(host)
        .or_insert_with(|| Arc::new(Semaphore::new(PER_HOST_CONCURRENCY)))
        .clone()
}

/// `DEVUP_ACCESS_CHECKER_TRACE=1` 이면 요청마다 대기·소요 시간을 stderr 로 출력한다.
static TRACE: LazyLock<bool> =
    LazyLock::new(|| std::env::var_os("DEVUP_ACCESS_CHECKER_TRACE").is_some());

/// 호스트별 동시 요청 수를 제한하고, 연결 실패는 한 번 재시도한다 (timeout·DNS 실패는 재시도하지 않음).
async fn send(client: &Client, method: Method, url: &str) -> Result<Response, reqwest::Error> {
    let limit = host_limit(url);
    let waited = Instant::now();
    let _permit = limit.acquire_owned().await.ok();
    let started = Instant::now();
    let mut retried = false;
    loop {
        let result = client.request(method.clone(), url).send().await;
        if *TRACE {
            eprintln!(
                "[devup-access-checker] {method} {url} wait={}ms took={}ms {}",
                (started - waited).as_millis(),
                started.elapsed().as_millis(),
                match &result {
                    Ok(r) => r.status().as_u16().to_string(),
                    Err(e) => describe(e),
                }
            );
        }
        match result {
            Err(e) if !retried && e.is_connect() && !e.is_timeout() && !is_dns_error(&e) => {
                retried = true;
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            r => return r,
        }
    }
}

/// DNS 조회 실패는 다시 시도해도 같은 결과라 재시도하지 않는다.
fn is_dns_error(e: &reqwest::Error) -> bool {
    let mut source: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(e);
    while let Some(err) = source {
        let text = err.to_string().to_ascii_lowercase();
        if text.contains("dns error")
            || text.contains("failed to lookup")
            || text.contains("no such host")
            || text.contains("name or service not known")
        {
            return true;
        }
        source = err.source();
    }
    false
}

/// `map_err` 에 함수로 넘기기 위한 [`describe`].
pub(crate) fn describe_owned(e: reqwest::Error) -> String {
    describe(&e)
}

pub(crate) async fn send_get(client: &Client, url: &str) -> Result<Response, reqwest::Error> {
    send(client, Method::GET, url).await
}

pub(crate) async fn send_head(client: &Client, url: &str) -> Result<Response, reqwest::Error> {
    send(client, Method::HEAD, url).await
}

/// 부속 파일 본문은 이 크기까지만 보관한다.
const MAX_PROBE_BODY: usize = 512 * 1024;
/// 보관 상한을 넘은 본문은 크기만 이 값까지 센다.
const COUNT_LIMIT: usize = 64 * 1024 * 1024;
/// sitemap 은 표준 한도(50MB) 판별을 위해 더 크게 받는다.
const MAX_SITEMAP_BODY: usize = 51 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Page {
    pub status: u16,
    pub final_url: Url,
    pub headers: HeaderMap,
    pub body: String,
    pub elapsed: Duration,
    /// 압축 해제 전 전송 크기는 알 수 없으므로 본문 바이트 수를 기록한다.
    pub body_bytes: usize,
    /// 본문이 [`MAX_PAGE_BYTES`] 를 넘어 잘림
    pub truncated: bool,
    /// 협상된 HTTP 버전 (예: "HTTP/2.0")
    pub version: String,
    /// 최종 연결의 TLS 서버 인증서 (https 일 때)
    pub certificate: Option<CertInfo>,
}

/// 대상 페이지용 클라이언트: 인증서 확인을 위해 TLS 정보를 함께 받는다.
pub fn build_page_client(user_agent: &str, timeout: Duration) -> Client {
    Client::builder()
        .user_agent(user_agent)
        .timeout(timeout)
        .redirect(Policy::limited(10))
        .gzip(true)
        .brotli(true)
        .tls_info(true)
        .build()
        .expect("reqwest client build")
}

pub fn build_client(user_agent: &str, timeout: Duration) -> Client {
    Client::builder()
        .user_agent(user_agent)
        .timeout(timeout)
        .redirect(Policy::limited(10))
        .gzip(true)
        .brotli(true)
        .build()
        .expect("reqwest client build")
}

fn describe(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "timeout".into()
    } else if e.is_connect() {
        format!("connect: {e}")
    } else if e.is_redirect() {
        "too many redirects".into()
    } else {
        e.to_string()
    }
}

pub async fn fetch_page(client: &Client, url: &Url) -> Result<Page, Error> {
    let started = Instant::now();
    let res = send_get(client, url.as_str())
        .await
        .map_err(|e| Error::Fetch {
            url: url.to_string(),
            reason: describe(&e),
        })?;
    let status = res.status().as_u16();
    let final_url = res.url().clone();
    let headers = res.headers().clone();
    let version = format!("{:?}", res.version());
    let certificate = res
        .extensions()
        .get::<reqwest::tls::TlsInfo>()
        .and_then(reqwest::tls::TlsInfo::peer_certificate)
        .and_then(parse_certificate);
    let declared_len = res.content_length();
    let mut res = res;
    let mut buf = Vec::new();
    let mut truncated = false;
    while let Some(chunk) = res.chunk().await.map_err(|e| Error::Fetch {
        url: url.to_string(),
        reason: describe(&e),
    })? {
        buf.extend_from_slice(&chunk);
        if buf.len() >= MAX_PAGE_BYTES {
            buf.truncate(MAX_PAGE_BYTES);
            truncated = true;
            break;
        }
    }
    let body_bytes = if truncated {
        declared_len.map_or(buf.len(), |n| (n as usize).max(buf.len()))
    } else {
        buf.len()
    };
    let body = decode_body(&buf, &headers);
    Ok(Page {
        status,
        final_url,
        headers,
        body_bytes,
        truncated,
        body,
        elapsed: started.elapsed(),
        version,
        certificate,
    })
}

/// 대상 페이지 본문은 이 크기까지만 받는다 (비정상적으로 큰 응답으로 인한 메모리 폭증 방지).
pub const MAX_PAGE_BYTES: usize = 15 * 1024 * 1024;

/// HTML 인코딩 판별 순서를 따라 디코딩한다: BOM → Content-Type charset → 앞 1024 바이트의
/// `<meta charset>` / `http-equiv` → UTF-8.
pub(crate) fn decode_body(bytes: &[u8], headers: &HeaderMap) -> String {
    if let Some((enc, _)) = encoding_rs::Encoding::for_bom(bytes) {
        return enc.decode(bytes).0.into_owned();
    }
    let header_label = headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|ct| {
            ct.split(';')
                .filter_map(|p| p.trim().split_once('='))
                .find(|(k, _)| k.trim().eq_ignore_ascii_case("charset"))
                .map(|(_, v)| v.trim().trim_matches('"').to_string())
        });
    let encoding = header_label
        .and_then(|l| encoding_rs::Encoding::for_label(l.as_bytes()))
        .or_else(|| sniff_meta_charset(&bytes[..bytes.len().min(1024)]))
        .unwrap_or(encoding_rs::UTF_8);
    encoding.decode(bytes).0.into_owned()
}

fn sniff_meta_charset(head: &[u8]) -> Option<&'static encoding_rs::Encoding> {
    static RE: LazyLock<regex::bytes::Regex> = LazyLock::new(|| {
        regex::bytes::Regex::new(r#"(?i)<meta[^>]*?charset\s*=\s*["']?\s*([a-z0-9_:.\-]+)"#)
            .unwrap()
    });
    let label = RE.captures(head)?.get(1)?.as_bytes();
    let enc = encoding_rs::Encoding::for_label(label)?;
    // 명세상 meta 로 UTF-16 을 선언하면 UTF-8 로 본다
    Some(
        if enc == encoding_rs::UTF_16LE || enc == encoding_rs::UTF_16BE {
            encoding_rs::UTF_8
        } else {
            enc
        },
    )
}

/// 본문을 `max` 바이트까지만 받아 디코딩한다. (본문, 잘림 여부)
async fn read_text(mut res: Response, max: usize) -> Result<(String, bool), reqwest::Error> {
    let headers = res.headers().clone();
    let mut buf = Vec::new();
    let mut truncated = false;
    while let Some(chunk) = res.chunk().await? {
        buf.extend_from_slice(&chunk);
        if buf.len() >= max {
            buf.truncate(max);
            truncated = true;
            break;
        }
    }
    Ok((decode_body(&buf, &headers), truncated))
}

/// 표본·대상 확인용 페이지 본문 상한.
const MAX_SAMPLE_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub enum Expected {
    Xml,
    PlainText,
    Json,
    /// 이미지 등 바이너리: HTML 이 아니고 본문이 비어 있지 않음
    Binary,
}

/// 정적 파일(sitemap.xml, robots.txt, llms.txt …) probe 결과.
#[derive(Debug, Clone, Default)]
pub struct FileProbe {
    pub url: String,
    pub status_code: Option<u16>,
    pub final_url: Option<String>,
    /// 요청 경로와 최종 경로가 다름 (보통 홈으로 우회)
    pub redirected: bool,
    pub content_type: Option<String>,
    /// 본문이 기대 형식과 일치 (HTML 이 아님 등)
    pub format_valid: bool,
    /// status 2xx/3xx AND 리다이렉트 우회 없음 AND 형식 일치
    pub exists: bool,
    pub body: Option<String>,
    /// 본문 바이트 수 (보관 상한과 무관한 전체 크기, 최대 64MB 까지 셈)
    pub size: Option<usize>,
    /// 본문이 상한을 넘어 앞부분만 보관함
    pub truncated: bool,
    pub error: Option<String>,
}

/// 본문이 HTML 문서인지. 앞쪽의 BOM·공백·주석·`<?xml ...?>` 선언을 건너뛴 뒤 첫 태그로 판단한다.
pub fn looks_like_html(body: &str) -> bool {
    let mut rest = body.trim_start_matches('\u{feff}').trim_start();
    loop {
        if let Some(r) = rest.strip_prefix("<!--") {
            match r.find("-->") {
                Some(end) => rest = r[end + 3..].trim_start(),
                None => return false,
            }
        } else if rest.starts_with("<?") {
            match rest.find("?>") {
                Some(end) => rest = rest[end + 2..].trim_start(),
                None => return false,
            }
        } else {
            break;
        }
    }
    let head = rest
        .chars()
        .take(16)
        .collect::<String>()
        .to_ascii_lowercase();
    [
        "<!doctype html",
        "<!doctype>",
        "<html",
        "<head",
        "<body",
        "<meta",
        "<title",
        "<link",
        "<script",
    ]
    .iter()
    .any(|p| {
        head.strip_prefix(p).is_some_and(|after| {
            after.is_empty() || after.starts_with([' ', '>', '\t', '\n', '\r', '/'])
        })
    })
}

fn looks_like_xml(body: &str) -> bool {
    let lower = body
        .trim_start_matches('\u{feff}')
        .trim_start()
        .to_ascii_lowercase();
    ["<?xml", "<urlset", "<sitemapindex", "<rss", "<feed"]
        .iter()
        .any(|p| lower.starts_with(p))
}

pub async fn probe_file(client: &Client, url: &Url, expected: Expected) -> FileProbe {
    let mut probe = FileProbe {
        url: url.to_string(),
        ..Default::default()
    };
    let res = match send_get(client, url.as_str()).await {
        Ok(r) => r,
        Err(e) => {
            probe.error = Some(describe(&e));
            return probe;
        }
    };
    let status = res.status().as_u16();
    let final_url = res.url().clone();
    probe.content_type = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let max = if matches!(expected, Expected::Xml) {
        MAX_SITEMAP_BODY
    } else {
        MAX_PROBE_BODY
    };
    // 상한까지만 보관하고, 그 뒤는 크기만 센다 (COUNT_LIMIT 에서 멈춤).
    let headers = res.headers().clone();
    let declared_len = res.content_length();
    let mut res = res;
    let mut buf = Vec::new();
    let mut read = 0usize;
    while read < COUNT_LIMIT {
        match res.chunk().await {
            Ok(Some(chunk)) => {
                read += chunk.len();
                if buf.len() <= max {
                    buf.extend_from_slice(&chunk);
                }
            }
            Ok(None) => break,
            Err(e) => {
                probe.status_code = Some(status);
                probe.error = Some(describe(&e));
                return probe;
            }
        }
    }
    if buf.len() > max {
        probe.truncated = true;
        buf.truncate(max);
    }
    let body = decode_body(&buf, &headers);
    // 텍스트 파일 본문에 차단 공급자 이름이 언급된 경우를 오인하지 않도록 HTML 응답만 본다
    if let Some(vendor) = looks_like_html(&body)
        .then(|| crate::signals::bot_challenge(&body))
        .flatten()
    {
        probe.status_code = Some(status);
        probe.final_url = Some(final_url.to_string());
        probe.error = Some(format!("봇 차단 응답({vendor})"));
        return probe;
    }
    probe.redirected = url.path().trim_end_matches('/') != final_url.path().trim_end_matches('/');
    let ct = probe
        .content_type
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let head: String = body.chars().take(2048).collect();
    let is_html = looks_like_html(&body);
    let gz = url.path().ends_with(".gz") && (ct.contains("gzip") || ct.contains("octet-stream"));
    probe.format_valid = match expected {
        Expected::Xml => (gz || ct.contains("xml") || looks_like_xml(&head)) && !is_html,
        Expected::PlainText => !is_html,
        Expected::Binary => !is_html && !body.is_empty(),
        Expected::Json => !is_html && serde_json::from_str::<serde_json::Value>(&body).is_ok(),
    };
    // 다른 파일로의 리다이렉트(예: /sitemap.xml → /crawled-sitemap.xml)는 허용하되,
    // 루트(홈)로 돌려보내는 SPA 폴백은 존재하지 않는 것으로 본다.
    let to_root = probe.redirected && final_url.path().trim_end_matches('/').is_empty();
    probe.exists = (200..400).contains(&status) && !to_root && probe.format_valid;
    probe.status_code = Some(status);
    probe.final_url = Some(final_url.to_string());
    probe.size = Some(declared_len.map_or(read, |n| (n as usize).max(read)));
    probe.body = Some(body);
    probe
}

/// 링크 접근성 probe 결과.
#[derive(Debug, Clone, Default)]
pub struct LinkProbe {
    /// 절대 URL 로 변환된 대상
    pub url: Option<String>,
    pub status_code: Option<u16>,
    pub final_url: Option<String>,
    pub error: Option<String>,
}

impl LinkProbe {
    pub fn reachable(&self) -> bool {
        self.status_code.is_some_and(|s| (200..400).contains(&s))
    }
}

pub async fn probe_link(client: &Client, raw: &str, base: &Url) -> LinkProbe {
    let Ok(abs) = base.join(raw) else {
        return LinkProbe {
            error: Some("invalid url".into()),
            ..Default::default()
        };
    };
    if !matches!(abs.scheme(), "http" | "https") {
        return LinkProbe {
            url: Some(abs.to_string()),
            error: Some(format!("non-http scheme: {}", abs.scheme())),
            ..Default::default()
        };
    }
    match send_get(client, abs.as_str()).await {
        Ok(res) => LinkProbe {
            url: Some(abs.to_string()),
            status_code: Some(res.status().as_u16()),
            final_url: Some(res.url().to_string()),
            error: None,
        },
        Err(e) => LinkProbe {
            url: Some(abs.to_string()),
            error: Some(describe(&e)),
            ..Default::default()
        },
    }
}

/// 상태 코드 + 본문(최대 64KB). 404 페이지 판별용.
pub async fn probe_status_body(client: &Client, url: &Url) -> Result<(u16, String), String> {
    let res = send_get(client, url.as_str())
        .await
        .map_err(describe_owned)?;
    let status = res.status().as_u16();
    let (body, _) = read_text(res, 65_536).await.map_err(describe_owned)?;
    if let Some(vendor) = crate::signals::bot_challenge(&body) {
        return Err(format!("봇 차단 응답({vendor})"));
    }
    Ok((status, body))
}

/// 상태 코드와 최종 URL 만 확인 (본문 무시).
pub async fn probe_status(client: &Client, url: &Url) -> Result<(u16, Url), String> {
    let res = send_get(client, url.as_str())
        .await
        .map_err(describe_owned)?;
    Ok((res.status().as_u16(), res.url().clone()))
}

/// 리다이렉트 한 단계.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hop {
    pub url: String,
    pub status: u16,
}

/// 자동 리다이렉트를 끄고 최대 `max` 단계까지 직접 따라가며 각 단계를 기록한다.
/// 마지막 원소가 최종 응답이다.
pub async fn redirect_chain(
    ua: &str,
    timeout: Duration,
    url: &Url,
    max: usize,
) -> Result<Vec<Hop>, String> {
    let client = Client::builder()
        .user_agent(ua)
        .timeout(timeout)
        .redirect(Policy::none())
        .build()
        .expect("http client");
    let mut hops = Vec::new();
    let mut current = url.clone();
    for _ in 0..=max {
        let res = send_get(&client, current.as_str())
            .await
            .map_err(describe_owned)?;
        let status = res.status().as_u16();
        hops.push(Hop {
            url: current.to_string(),
            status,
        });
        let location = res
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        match location {
            Some(loc) if (300..400).contains(&status) => {
                current = current
                    .join(&loc)
                    .map_err(|e| format!("invalid Location {loc}: {e}"))?;
            }
            _ => return Ok(hops),
        }
    }
    Err(format!("too many redirects (> {max})"))
}

/// TLS 서버 인증서 요약.
#[derive(Debug, Clone)]
pub struct CertInfo {
    pub subject: String,
    pub issuer: String,
    /// 만료 시각 (UNIX 초)
    pub not_after: i64,
    /// RFC 3339 표기 만료 시각
    pub not_after_text: String,
}

/// DER 인증서에서 주체·발급자·만료일을 읽는다.
pub fn parse_certificate(der: &[u8]) -> Option<CertInfo> {
    let (_, cert) = x509_parser::parse_x509_certificate(der).ok()?;
    let not_after = cert.validity().not_after;
    Some(CertInfo {
        subject: cert.subject().to_string(),
        issuer: cert.issuer().to_string(),
        not_after: not_after.timestamp(),
        not_after_text: not_after.to_datetime().to_string(),
    })
}

/// 본문을 최대 `max` 바이트까지 받는다. (status, content-type, bytes, 잘림 여부)
pub async fn fetch_bytes(
    client: &Client,
    url: &Url,
    max: usize,
) -> Result<(u16, Option<String>, Vec<u8>, bool), String> {
    let mut res = send_get(client, url.as_str())
        .await
        .map_err(describe_owned)?;
    let status = res.status().as_u16();
    let ct = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let mut buf = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(describe_owned)? {
        buf.extend_from_slice(&chunk);
        if buf.len() >= max {
            buf.truncate(max);
            return Ok((status, ct, buf, true));
        }
    }
    Ok((status, ct, buf, false))
}

/// 본문을 보관하지 않고 바이트 수만 센다 (`max` 에서 멈춤). 2xx 가 아니면 오류.
pub async fn count_bytes(client: &Client, url: &Url, max: usize) -> Result<usize, String> {
    let mut res = send_get(client, url.as_str())
        .await
        .map_err(describe_owned)?;
    if !res.status().is_success() {
        return Err(format!("HTTP {}", res.status().as_u16()));
    }
    let mut n = 0;
    while let Some(chunk) = res.chunk().await.map_err(describe_owned)? {
        n += chunk.len();
        if n >= max {
            return Ok(max);
        }
    }
    Ok(n)
}

/// 하위 sitemap 을 받아 URL 목록을 돌려준다. `.xml.gz` (gzip) 도 풀어서 읽는다.
pub async fn fetch_sitemap_locs(client: &Client, url: &Url) -> Result<Vec<String>, String> {
    use std::io::Read;
    let (status, _, bytes, _) = fetch_bytes(client, url, MAX_SITEMAP_BODY).await?;
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }
    let xml = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(bytes.as_slice())
            .take(MAX_SITEMAP_BODY as u64)
            .read_to_end(&mut out)
            .map_err(|e| format!("gzip: {e}"))?;
        out
    } else {
        bytes
    };
    let text = String::from_utf8_lossy(&xml);
    if looks_like_html(&text) {
        return Err("HTML 응답".into());
    }
    Ok(crate::sitemap::summarize(&text).locs)
}

/// sitemap 표본 페이지 요약.
#[derive(Debug, Clone)]
pub struct SamplePage {
    pub status: u16,
    pub final_url: Url,
    /// meta robots / googlebot / X-Robots-Tag 중 noindex
    pub noindex: bool,
    /// canonical (절대 URL)
    pub canonical: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub lang: Option<String>,
    pub h1_count: usize,
    pub img_total: usize,
    pub img_without_alt: usize,
    pub body_text_len: usize,
    /// (hreflang, 절대 URL)
    pub hreflang: Vec<(String, String)>,
}

pub async fn probe_page(client: &Client, url: &Url) -> Result<SamplePage, String> {
    let res = send_get(client, url.as_str())
        .await
        .map_err(describe_owned)?;
    let status = res.status().as_u16();
    let final_url = res.url().clone();
    let header_noindex = res
        .headers()
        .get_all("x-robots-tag")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .any(|v| v.to_ascii_lowercase().contains("noindex"));
    let (body, _) = read_text(res, MAX_SAMPLE_BYTES)
        .await
        .map_err(describe_owned)?;
    if let Some(vendor) = crate::signals::bot_challenge(&body) {
        return Err(format!("봇 차단 응답({vendor})"));
    }
    let facts = crate::html::extract(&body, &final_url);
    let facts_base = facts.link_base(&final_url).clone();
    let meta_noindex = [facts.meta("robots"), facts.meta("googlebot")]
        .into_iter()
        .flatten()
        .flat_map(|v| v.split(','))
        .any(|d| matches!(d.trim().to_ascii_lowercase().as_str(), "noindex" | "none"));
    Ok(SamplePage {
        status,
        canonical: facts
            .canonical
            .as_deref()
            .and_then(|c| facts_base.join(c).ok())
            .map(|u| u.to_string()),
        final_url,
        noindex: header_noindex || meta_noindex,
        description: facts.meta("description").map(str::to_string),
        title: facts.title,
        lang: facts.lang,
        h1_count: facts.h1_count,
        img_total: facts.img_total,
        img_without_alt: facts.img_without_alt,
        body_text_len: facts.body_text_len,
        hreflang: facts
            .hreflang
            .iter()
            .filter_map(|h| {
                facts_base
                    .join(&h.href)
                    .ok()
                    .map(|u| (h.lang.clone(), u.to_string()))
            })
            .collect(),
    })
}

/// 상태 코드와 응답 헤더만 확인 (본문 무시).
pub async fn probe_headers(client: &Client, url: &Url) -> Result<(u16, HeaderMap), String> {
    let res = send_get(client, url.as_str())
        .await
        .map_err(describe_owned)?;
    Ok((res.status().as_u16(), res.headers().clone()))
}

/// HEAD 요청으로 Content-Length 확인. 헤더가 없으면 Ok(None).
pub async fn head_length(client: &Client, url: &Url) -> Result<(u16, Option<u64>), String> {
    let res = send_head(client, url.as_str())
        .await
        .map_err(describe_owned)?;
    let len = res
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());
    Ok((res.status().as_u16(), len))
}

/// AI 크롤러 UA 요청: (상태, 최종 URL, 챌린지 공급자). 본문은 200KB 까지만 본다.
pub async fn probe_ua(
    client: &Client,
    url: &Url,
) -> Result<(u16, Url, Option<&'static str>), String> {
    let mut res = send_get(client, url.as_str())
        .await
        .map_err(describe_owned)?;
    let status = res.status().as_u16();
    let final_url = res.url().clone();
    let mut buf = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(describe_owned)? {
        buf.extend_from_slice(&chunk);
        if buf.len() >= 200_000 {
            break;
        }
    }
    let challenge = crate::signals::bot_challenge(&String::from_utf8_lossy(&buf));
    Ok((status, final_url, challenge))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_by_meta_charset_when_header_has_none() {
        let (bytes, _, _) =
            encoding_rs::EUC_KR.encode("<html><head><meta charset=\"euc-kr\"><title>한글</title>");
        let mut h = HeaderMap::new();
        h.insert(reqwest::header::CONTENT_TYPE, "text/html".parse().unwrap());
        assert!(decode_body(&bytes, &h).contains("한글"));
        // http-equiv 형식
        let (bytes, _, _) = encoding_rs::EUC_KR
            .encode(r#"<meta http-equiv="Content-Type" content="text/html; charset=EUC-KR">한글"#);
        assert!(decode_body(&bytes, &h).contains("한글"));
        // 헤더 charset 이 meta 보다 우선
        h.insert(
            reqwest::header::CONTENT_TYPE,
            "text/html; charset=utf-8".parse().unwrap(),
        );
        assert!(decode_body("<meta charset=euc-kr>한글".as_bytes(), &h).contains("한글"));
    }

    #[test]
    fn html_detection_skips_prolog_and_comments() {
        assert!(looks_like_html("<!-- generated -->\n<!DOCTYPE html><html>"));
        assert!(looks_like_html(
            "<?xml version=\"1.0\"?>\n<!DOCTYPE html PUBLIC>"
        ));
        assert!(looks_like_html("  <head><meta charset=utf-8>"));
        assert!(looks_like_html("<meta name=x>"));
        assert!(!looks_like_html("# Title\n\n> <head> 태그 설명"));
        assert!(!looks_like_html("<?xml version=\"1.0\"?><urlset>"));
        assert!(!looks_like_html("User-agent: *\nDisallow: /"));
        assert!(!looks_like_html("<headline>"));
        // 닫히지 않은 주석·선언
        assert!(!looks_like_html("<!-- open"));
        assert!(!looks_like_html("<?xml open"));
    }

    // ===== 네트워크 경계 상황 =====
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client() -> Client {
        build_client("test", Duration::from_secs(5))
    }

    async fn serve(p: &str, r: ResponseTemplate) -> (MockServer, Url) {
        let s = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(p))
            .respond_with(r)
            .mount(&s)
            .await;
        let url = Url::parse(&format!("{}{p}", s.uri())).unwrap();
        (s, url)
    }

    /// Content-Length 보다 짧은 본문을 보내고 연결을 끊는 서버 (본문 읽기 오류 재현).
    async fn cut_body_server() -> Url {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let mut buf = [0u8; 1024];
                let _ = sock.read(&mut buf).await;
                let _ = sock
                    .write_all(b"HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: 1000\r\n\r\nshort")
                    .await;
            }
        });
        Url::parse(&format!("http://{addr}/robots.txt")).unwrap()
    }

    const CHALLENGE: &str =
        "<!doctype html><html><head><title>Just a moment...</title></head></html>";

    #[test]
    fn idle_host_limits_are_evicted() {
        for i in 0..300 {
            host_limit(&format!("http://h{i}.example/"));
        }
        assert!(HOST_LIMITS.lock().unwrap().len() <= 257);
    }

    #[test]
    fn decodes_bom_and_utf16_meta_as_utf8() {
        let h = HeaderMap::new();
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend_from_slice("한글".as_bytes());
        assert_eq!(decode_body(&bom, &h), "한글");
        // meta 로 UTF-16 을 선언해도 바이트는 UTF-8 로 해석한다
        assert!(decode_body("<meta charset=utf-16>한글".as_bytes(), &h).contains("한글"));
    }

    #[test]
    fn parses_certificate_fixture() {
        let der = include_bytes!("../tests/fixtures/cert.der");
        let c = parse_certificate(der).unwrap();
        assert!(c.subject.contains("test.local"));
        assert!(c.not_after > 0);
        assert!(parse_certificate(b"not a cert").is_none());
    }

    #[tokio::test]
    async fn describes_errors() {
        // timeout
        let (_s, url) = serve(
            "/slow",
            ResponseTemplate::new(200).set_delay(Duration::from_secs(3)),
        )
        .await;
        let quick = build_client("t", Duration::from_millis(200));
        let e = send_get(&quick, url.as_str()).await.unwrap_err();
        assert_eq!(describe(&e), "timeout");
        // 리다이렉트 반복
        let (s, url) = serve(
            "/loop",
            ResponseTemplate::new(302).insert_header("location", "/loop"),
        )
        .await;
        let e = send_get(
            &build_page_client("t", Duration::from_secs(5)),
            url.as_str(),
        )
        .await
        .unwrap_err();
        assert_eq!(describe(&e), "too many redirects");
        assert!(
            redirect_chain("t", Duration::from_secs(5), &url, 3)
                .await
                .unwrap_err()
                .contains("too many redirects")
        );
        drop(s);
        // 그 외 (잘못된 URL)
        let e = send_get(&client(), "not a url").await.unwrap_err();
        assert!(!describe(&e).is_empty());
    }

    #[tokio::test]
    async fn body_read_errors_and_truncation() {
        let cut = cut_body_server().await;
        assert!(fetch_page(&client(), &cut).await.is_err());
        let p = probe_file(&client(), &cut, Expected::PlainText).await;
        assert!(p.error.is_some());

        // 대상 페이지 상한 초과
        let big = "a".repeat(MAX_PAGE_BYTES + 10);
        let (_s, url) = serve("/big", ResponseTemplate::new(200).set_body_string(big)).await;
        let page = fetch_page(&client(), &url).await.unwrap();
        assert!(page.truncated);
        assert!(page.body_bytes >= MAX_PAGE_BYTES);

        // 사이트 파일 상한 초과 → 실제 크기와 truncated
        let text = "b".repeat(MAX_PROBE_BODY + 10);
        let (_s, url) = serve(
            "/llms.txt",
            ResponseTemplate::new(200).set_body_string(text),
        )
        .await;
        let p = probe_file(&client(), &url, Expected::PlainText).await;
        assert!(p.truncated);
        assert_eq!(p.size, Some(MAX_PROBE_BODY + 10));

        // 404 본문 상한
        let (_s, url) = serve(
            "/404",
            ResponseTemplate::new(404).set_body_string("c".repeat(70_000)),
        )
        .await;
        let (status, body) = probe_status_body(&client(), &url).await.unwrap();
        assert_eq!((status, body.len()), (404, 65_536));

        // fetch_bytes / count_bytes 상한
        let (_s, url) = serve(
            "/bin",
            ResponseTemplate::new(200).set_body_bytes(vec![1u8; 5000]),
        )
        .await;
        let (_, _, bytes, truncated) = fetch_bytes(&client(), &url, 100).await.unwrap();
        assert_eq!((bytes.len(), truncated), (100, true));
        assert_eq!(count_bytes(&client(), &url, 100).await.unwrap(), 100);

        // UA 요청은 앞 200KB 만 본다
        let (_s, url) = serve(
            "/",
            ResponseTemplate::new(200).set_body_string("d".repeat(300_000)),
        )
        .await;
        let (status, _, challenge) = probe_ua(&client(), &url).await.unwrap();
        assert_eq!((status, challenge), (200, None));
    }

    #[tokio::test]
    async fn challenge_pages_are_errors() {
        let (_s, url) = serve(
            "/robots.txt",
            ResponseTemplate::new(200).set_body_string(CHALLENGE),
        )
        .await;
        let p = probe_file(&client(), &url, Expected::PlainText).await;
        assert!(p.error.unwrap().contains("Cloudflare"));
        assert!(probe_status_body(&client(), &url).await.is_err());
        assert!(probe_page(&client(), &url).await.is_err());
    }

    #[tokio::test]
    async fn link_probe_rejects_bad_targets() {
        let base = Url::parse("https://a.com/").unwrap();
        assert_eq!(
            probe_link(&client(), "http://[", &base)
                .await
                .error
                .as_deref(),
            Some("invalid url")
        );
        assert!(
            probe_link(&client(), "mailto:x@a.com", &base)
                .await
                .error
                .unwrap()
                .contains("non-http")
        );
    }

    #[tokio::test]
    async fn sitemap_locs_from_plain_gzip_and_html() {
        use std::io::Write as _;
        let xml = r#"<?xml version="1.0"?><urlset><url><loc>https://a.com/p</loc></url></urlset>"#;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(xml.as_bytes()).unwrap();
        let (_s, url) = serve(
            "/s.xml.gz",
            ResponseTemplate::new(200).set_body_bytes(gz.finish().unwrap()),
        )
        .await;
        assert_eq!(
            fetch_sitemap_locs(&client(), &url).await.unwrap(),
            ["https://a.com/p"]
        );
        let (_s, url) = serve("/s.xml", ResponseTemplate::new(200).set_body_string(xml)).await;
        assert_eq!(fetch_sitemap_locs(&client(), &url).await.unwrap().len(), 1);
        let (_s, url) = serve(
            "/h.xml",
            ResponseTemplate::new(200).set_body_string("<!doctype html><html>"),
        )
        .await;
        assert!(fetch_sitemap_locs(&client(), &url).await.is_err());
        let (_s, url) = serve("/n.xml", ResponseTemplate::new(404)).await;
        assert_eq!(
            fetch_sitemap_locs(&client(), &url).await.unwrap_err(),
            "HTTP 404"
        );
        // gzip 헤더만 있고 내용이 깨짐
        let (_s, url) = serve(
            "/bad.xml.gz",
            ResponseTemplate::new(200).set_body_bytes(vec![0x1f, 0x8b, 0, 1, 2]),
        )
        .await;
        assert!(
            fetch_sitemap_locs(&client(), &url)
                .await
                .unwrap_err()
                .starts_with("gzip")
        );
    }

    #[tokio::test]
    async fn invalid_location_and_header_noindex() {
        let (_s, url) = serve(
            "/r",
            ResponseTemplate::new(301).insert_header("location", "http://["),
        )
        .await;
        assert!(
            redirect_chain("t", Duration::from_secs(5), &url, 3)
                .await
                .unwrap_err()
                .contains("invalid Location")
        );
        let (_s, url) = serve(
            "/p",
            ResponseTemplate::new(200)
                .insert_header("x-robots-tag", "googlebot: NOINDEX")
                .set_body_string("<html><body>x</body></html>"),
        )
        .await;
        assert!(probe_page(&client(), &url).await.unwrap().noindex);
    }
}
