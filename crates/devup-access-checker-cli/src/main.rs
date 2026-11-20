use std::io::Write;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, ValueEnum};
use devup_access_checker::{Category, Checker, Options, Presence, Report};

#[derive(Copy, Clone, ValueEnum)]
enum Format {
    Text,
    Json,
}

/// URL 의 SEO / GEO 구성 요소 존재 여부를 점검합니다.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// 점검할 URL (스킴 생략 시 https)
    #[arg(required = true)]
    urls: Vec<String>,
    /// 출력 형식
    #[arg(short, long, value_enum, default_value_t = Format::Text)]
    format: Format,
    /// 누락(missing) 항목만 출력 (text 형식)
    #[arg(long)]
    only_missing: bool,
    /// 링크 접근성 검증 생략
    #[arg(long)]
    no_links: bool,
    /// AI·검색 크롤러 User-Agent 요청 생략
    #[arg(long)]
    no_ai_ua: bool,
    /// HTTP→HTTPS 리다이렉트·압축 확인 생략
    #[arg(long)]
    no_transport: bool,
    /// 외부 CSS·JS·이미지 분석 생략
    #[arg(long)]
    no_resources: bool,
    /// 여러 페이지 표본 수 (0 = 생략)
    #[arg(long, default_value_t = 10)]
    pages: usize,
    /// IndexNow 키 (지정하면 키 파일을 확인)
    #[arg(long)]
    indexnow_key: Option<String>,
    /// 대상 페이지 timeout (초)
    #[arg(long, default_value_t = 30)]
    timeout: u64,
}

fn mark(p: Presence) -> &'static str {
    match p {
        Presence::Present => "✓",
        Presence::Missing => "✗",
        Presence::Unknown => "?",
        Presence::NotApplicable => "-",
    }
}

fn short_value(v: &serde_json::Value) -> String {
    let s = match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    if s.chars().count() > 80 {
        format!("{}…", s.chars().take(80).collect::<String>())
    } else {
        s
    }
}

fn print_text(out: &mut impl Write, r: &Report, only_missing: bool) -> std::io::Result<()> {
    writeln!(
        out,
        "{} → {} (HTTP {}, {}ms)",
        r.url, r.final_url, r.status_code, r.duration_ms
    )?;
    let mut last: Option<Category> = None;
    for c in &r.checks {
        if only_missing && c.presence != Presence::Missing {
            continue;
        }
        if last != Some(c.category) {
            writeln!(out, "\n[{}]", c.category.label())?;
            last = Some(c.category);
        }
        let mut line = format!("  {} {:<36} {}", mark(c.presence), c.label, c.id);
        if let Some(v) = &c.value {
            line.push_str(&format!("  = {}", short_value(v)));
        }
        writeln!(out, "{line}")?;
        if let Some(n) = &c.note {
            writeln!(out, "      {n}")?;
        }
    }
    let count = |p| r.checks.iter().filter(|c| c.presence == p).count();
    writeln!(
        out,
        "\n있음 {} · 없음 {} · 판별 불가 {} · 해당 없음 {}",
        count(Presence::Present),
        count(Presence::Missing),
        count(Presence::Unknown),
        count(Presence::NotApplicable)
    )
}

/// 점검을 실행하고 결과를 쓴다. 반환값은 종료 코드 (0 성공, 2 일부 URL 실패).
async fn run(cli: Cli, out: &mut impl Write, err: &mut impl Write) -> std::io::Result<u8> {
    let mut opts = Options::default();
    opts.page_timeout = Duration::from_secs(cli.timeout);
    opts.check_links = !cli.no_links;
    opts.check_ai_user_agents = !cli.no_ai_ua;
    opts.check_transport = !cli.no_transport;
    opts.page_samples = cli.pages;
    opts.check_resources = !cli.no_resources;
    opts.indexnow_key = cli.indexnow_key.clone();
    let checker = Checker::new(opts);

    // 여러 URL 은 동시에 점검하고, 출력은 입력 순서를 따른다
    let results = futures::future::join_all(cli.urls.iter().map(|u| checker.check(u))).await;
    let mut reports = Vec::new();
    let mut failed = false;
    for result in results {
        match result {
            Ok(r) => {
                if let Format::Text = cli.format {
                    print_text(out, &r, cli.only_missing)?;
                    writeln!(out)?;
                }
                reports.push(r);
            }
            Err(e) => {
                writeln!(err, "error: {e}")?;
                failed = true;
            }
        }
    }
    if let Format::Json = cli.format {
        let json = if reports.len() == 1 && cli.urls.len() == 1 {
            serde_json::to_string_pretty(&reports[0])
        } else {
            serde_json::to_string_pretty(&reports)
        }
        .map_err(std::io::Error::other)?;
        writeln!(out, "{json}")?;
    }
    Ok(if failed { 2 } else { 0 })
}

#[tokio::main]
async fn main() -> ExitCode {
    let code = run(
        Cli::parse(),
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    )
    .await;
    ExitCode::from(code.unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn server() -> MockServer {
        let s = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html")
                    .set_body_string(format!(
                        "<!doctype html><html><head><title>t</title><meta name=description content=\"{}\"></head><body><h1>h</h1></body></html>",
                        "긴 설명 ".repeat(30)
                    )),
            )
            .mount(&s)
            .await;
        s
    }

    async fn exec(args: &[&str]) -> (u8, String, String) {
        let cli = Cli::parse_from(
            [
                "devup-access-checker",
                "--no-ai-ua",
                "--no-transport",
                "--pages",
                "0",
            ]
            .iter()
            .chain(args),
        );
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run(cli, &mut out, &mut err).await.unwrap();
        (
            code,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    #[tokio::test]
    async fn text_output_lists_categories_values_and_notes() {
        let s = server().await;
        let (code, out, _) = exec(&[&s.uri()]).await;
        assert_eq!(code, 0);
        assert!(out.contains("[기본 메타]"));
        assert!(out.contains("✓") && out.contains("✗") && out.contains("?") && out.contains("-"));
        assert!(out.contains("…"), "긴 값은 줄인다");
        assert!(out.contains("있음 "));
    }

    #[tokio::test]
    async fn only_missing_and_json_formats() {
        let s = server().await;
        let (_, out, _) = exec(&["--only-missing", &s.uri()]).await;
        assert!(!out.contains(" ✓ "));
        let (_, out, _) = exec(&["-f", "json", &s.uri()]).await;
        assert!(
            serde_json::from_str::<serde_json::Value>(&out)
                .unwrap()
                .is_object()
        );
        let (_, out, _) = exec(&["-f", "json", &s.uri(), &s.uri()]).await;
        assert!(
            serde_json::from_str::<serde_json::Value>(&out)
                .unwrap()
                .is_array()
        );
    }

    #[tokio::test]
    async fn unreachable_url_exits_with_2() {
        let s = server().await;
        let (code, _, err) = exec(&[
            "--no-links",
            "--no-resources",
            "--timeout",
            "5",
            &s.uri(),
            "http://127.0.0.1:9",
        ])
        .await;
        assert_eq!(code, 2);
        assert!(err.contains("error:"));
    }
}
