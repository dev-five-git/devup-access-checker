//! sitemap.xml 요약 (네트워크 접근 없음).

use std::sync::LazyLock;

use regex::Regex;

static LOC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<loc>\s*(?:<!\[CDATA\[)?\s*(.*?)\s*(?:\]\]>)?\s*</loc>").unwrap()
});
static LASTMOD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<lastmod>").unwrap());
static INDEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<sitemapindex[\s>]").unwrap());

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitemapSummary {
    /// `<sitemapindex>` 여부 (하위 sitemap 목록)
    pub is_index: bool,
    /// `<loc>` 개수 (index 면 하위 sitemap 수, 아니면 URL 수)
    pub entries: usize,
    /// `<lastmod>` 개수
    pub with_lastmod: usize,
    /// `<loc>` 값 (XML 엔티티 `&amp;` 복원)
    pub locs: Vec<String>,
}

pub fn summarize(xml: &str) -> SitemapSummary {
    let locs: Vec<String> = LOC
        .captures_iter(xml)
        .map(|c| c[1].replace("&amp;", "&"))
        .collect();
    SitemapSummary {
        is_index: INDEX.is_match(xml),
        entries: locs.len(),
        with_lastmod: LASTMOD.find_iter(xml).count(),
        locs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_urls_and_index() {
        let s = summarize(
            "<?xml version=\"1.0\"?><urlset><url><loc>https://a.com/?x=1&amp;y=2</loc><lastmod>2026-01-01</lastmod></url>\
             <url><LOC> <![CDATA[https://a.com/b]]> </LOC></url></urlset>",
        );
        assert!(!s.is_index);
        assert_eq!((s.entries, s.with_lastmod), (2, 1));
        assert_eq!(s.locs, vec!["https://a.com/?x=1&y=2", "https://a.com/b"]);
        let s =
            summarize("<sitemapindex xmlns=\"x\"><sitemap><loc>a</loc></sitemap></sitemapindex>");
        assert!(s.is_index);
        assert_eq!(s.entries, 1);
    }
}
