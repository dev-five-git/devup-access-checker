//! CSS 텍스트 분석 (초점 표시 제거, @font-face font-display). 네트워크 접근 없음.

use std::sync::LazyLock;

use regex::Regex;

static OUTLINE_NONE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)outline\s*:\s*(?:none|0(?:px)?)\s*(?:!important)?\s*[;}]").unwrap()
});
static FONT_FACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)@font-face\s*\{([^}]*)\}").unwrap());
static COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)/\*.*?\*/").unwrap());

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CssFacts {
    /// `outline: none|0` 선언 수
    pub outline_removed: usize,
    pub has_focus_visible: bool,
    pub font_faces: usize,
    /// font-display 가 없는 @font-face 수
    pub font_faces_without_display: usize,
}

impl CssFacts {
    pub fn merge(&mut self, other: &CssFacts) {
        self.outline_removed += other.outline_removed;
        self.has_focus_visible |= other.has_focus_visible;
        self.font_faces += other.font_faces;
        self.font_faces_without_display += other.font_faces_without_display;
    }
}

pub fn analyze(css: &str) -> CssFacts {
    let css = COMMENT.replace_all(css, "");
    let mut f = CssFacts {
        outline_removed: OUTLINE_NONE.find_iter(&css).count(),
        has_focus_visible: css.contains(":focus-visible"),
        ..Default::default()
    };
    for c in FONT_FACE.captures_iter(&css) {
        f.font_faces += 1;
        if !c[1].to_ascii_lowercase().contains("font-display") {
            f.font_faces_without_display += 1;
        }
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyzes_css() {
        let f = analyze(
            "/* outline:none; */ a:focus{outline:none} b{outline: 0 !important;} c{outline:1px solid}\
             .x:focus-visible{} @font-face{font-family:A;src:url(a.woff2)} @font-face{font-family:B;font-display:swap}",
        );
        assert_eq!(
            f,
            CssFacts {
                outline_removed: 2,
                has_focus_visible: true,
                font_faces: 2,
                font_faces_without_display: 1
            }
        );
    }
}
