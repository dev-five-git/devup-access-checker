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
    /// AI 크롤러 User-Agent 요청 생략
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

fn print_text(r: &Report, only_missing: bool) {
    println!(
        "{} → {} (HTTP {}, {}ms)",
        r.url, r.final_url, r.status_code, r.duration_ms
    );
    let mut last: Option<Category> = None;
    for c in &r.checks {
        if only_missing && c.presence != Presence::Missing {
            continue;
        }
        if last != Some(c.category) {
            println!("\n[{}]", c.category.label());
            last = Some(c.category);
        }
        let mut line = format!("  {} {:<36} {}", mark(c.presence), c.label, c.id);
        if let Some(v) = &c.value {
            line.push_str(&format!("  = {}", short_value(v)));
        }
        println!("{line}");
        if let Some(n) = &c.note {
            println!("      {n}");
        }
    }
    let count = |p| r.checks.iter().filter(|c| c.presence == p).count();
    println!(
        "\n있음 {} · 없음 {} · 판별 불가 {} · 해당 없음 {}",
        count(Presence::Present),
        count(Presence::Missing),
        count(Presence::Unknown),
        count(Presence::NotApplicable)
    );
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let checker = Checker::new(Options {
        page_timeout: Duration::from_secs(cli.timeout),
        check_links: !cli.no_links,
        check_ai_user_agents: !cli.no_ai_ua,
        check_transport: !cli.no_transport,
        page_samples: cli.pages,
        check_resources: !cli.no_resources,
        ..Default::default()
    });

    let mut reports = Vec::new();
    let mut failed = false;
    for url in &cli.urls {
        match checker.check(url).await {
            Ok(r) => {
                if let Format::Text = cli.format {
                    print_text(&r, cli.only_missing);
                    println!();
                }
                reports.push(r);
            }
            Err(e) => {
                eprintln!("error: {e}");
                failed = true;
            }
        }
    }
    if let Format::Json = cli.format {
        let out = if reports.len() == 1 && cli.urls.len() == 1 {
            serde_json::to_string_pretty(&reports[0])
        } else {
            serde_json::to_string_pretty(&reports)
        };
        println!("{}", out.expect("serialize report"));
    }
    if failed {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}
