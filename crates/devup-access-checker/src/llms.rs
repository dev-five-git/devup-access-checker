//! llms.txt 형식 분석 (https://llmstxt.org). 네트워크 접근 없음.
//!
//! 스펙 구조: `# 제목` (필수) → `> 요약` (선택) → 본문 → `## 섹션` 아래 `- [이름](url): 설명` 링크 목록.

use std::sync::LazyLock;

use regex::Regex;

static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[^\]]+\]\([^)\s]+\)").unwrap());

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LlmsTxt {
    /// 첫 줄(비어 있지 않은)의 `# 제목`
    pub title: Option<String>,
    /// 제목 다음의 `> 요약` 존재
    pub has_summary: bool,
    /// `## ` 섹션 수
    pub sections: usize,
    /// 마크다운 링크 수
    pub links: usize,
}

pub fn parse(body: &str) -> LlmsTxt {
    let body = body.trim_start_matches('\u{feff}');
    let mut lines = body.lines().map(str::trim).filter(|l| !l.is_empty());
    let title = lines
        .next()
        .and_then(|l| l.strip_prefix("# "))
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());
    let has_summary = title.is_some() && lines.next().is_some_and(|l| l.starts_with('>'));
    LlmsTxt {
        title,
        has_summary,
        sections: body
            .lines()
            .filter(|l| l.trim_start().starts_with("## "))
            .count(),
        links: LINK.find_iter(body).count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spec_example() {
        let doc = "# DevFive\n\n> IT 비즈니스 파트너\n\n소개 문단\n\n## Docs\n- [회사 소개](https://a.com/about.md): 설명\n- [포트폴리오](https://a.com/p.md)\n\n## Optional\n- [블로그](https://a.com/blog)\n";
        assert_eq!(
            parse(doc),
            LlmsTxt {
                title: Some("DevFive".into()),
                has_summary: true,
                sections: 2,
                links: 3
            }
        );
    }

    #[test]
    fn plain_text_has_no_structure() {
        assert_eq!(parse("hello\nworld"), LlmsTxt::default());
        assert_eq!(parse("## only section").title, None);
    }
}
