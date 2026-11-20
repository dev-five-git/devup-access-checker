//! llms.txt 형식 분석 (<https://llmstxt.org>). 네트워크 접근 없음.
//!
//! 스펙 구조: `# 제목` (필수) → `> 요약` (선택) → 본문 → `## 섹션` 아래 `- [이름](url): 설명` 링크 목록.

use std::sync::LazyLock;

use regex::Regex;

static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[^\]]+\]\([^)\s]+\)").unwrap());
/// 섹션 안 목록 항목의 규격: `- [이름](url)` 뒤에 선택적으로 `: 설명`
static LINK_ITEM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[-*]\s+\[[^\]]+\]\([^)\s]+\)(?:\s*:\s*.*)?$").unwrap());

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
    /// `## ` 섹션 안에서 `- [이름](url): 설명` 형식이 아닌 목록 항목
    pub malformed_items: Vec<String>,
    /// 규격 밖 헤딩 (두 번째 `# `, 첫 `## ` 앞의 `###` 이하)
    pub extra_headings: Vec<String>,
    /// `## Optional` 섹션
    pub optional_section: bool,
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
    let mut malformed_items = Vec::new();
    let mut extra_headings = Vec::new();
    let mut optional_section = false;
    let mut h1 = 0;
    let mut in_section = false;
    for l in body.lines().map(str::trim) {
        if l.starts_with("# ") {
            h1 += 1;
            if h1 > 1 {
                extra_headings.push(l.to_string());
            }
        } else if let Some(name) = l.strip_prefix("## ") {
            in_section = true;
            optional_section |= name.trim().eq_ignore_ascii_case("optional");
        } else if l.starts_with("###") && !in_section {
            extra_headings.push(l.to_string());
        } else if in_section
            && (l.starts_with("- ") || l.starts_with("* "))
            && !LINK_ITEM.is_match(l)
        {
            malformed_items.push(l.to_string());
        }
    }
    LlmsTxt {
        malformed_items,
        extra_headings,
        optional_section,
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
                links: 3,
                optional_section: true,
                ..Default::default()
            }
        );
        let bad = parse(
            "# A\n### 소제목\n# B\n## Docs\n- 링크 없음\n* [x](u) 설명만\n- [y](https://a.com/y)\n### 섹션 안 소제목\n",
        );
        assert_eq!(bad.extra_headings, ["### 소제목", "# B"]);
        assert_eq!(bad.malformed_items, ["- 링크 없음", "* [x](u) 설명만"]);
        assert!(!bad.optional_section);
    }

    #[test]
    fn plain_text_has_no_structure() {
        assert_eq!(parse("hello\nworld"), LlmsTxt::default());
        assert_eq!(parse("## only section").title, None);
    }
}
