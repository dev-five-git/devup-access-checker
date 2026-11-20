//! CSS 텍스트 분석 (초점 표시 제거, @font-face font-display). 네트워크 접근 없음.

use std::sync::LazyLock;

use regex::Regex;

static OUTLINE_NONE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)outline\s*:\s*(?:none|0(?:px)?)\s*(?:!important)?\s*[;}]").unwrap()
});
static FONT_FACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)@font-face\s*\{([^}]*)\}").unwrap());
static IMPORT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)@import\s").unwrap());
static WHITESPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static ORIENTATION_LOCK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)@media[^{]*orientation\s*:\s*(?:portrait|landscape)[^{]*\{[^@]*?rotate\(\s*-?(?:90|270)deg").unwrap()
});
static MOTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[;{\s])(?:animation|transition)(?:-name|-duration)?\s*:\s*([^;}]*)")
        .unwrap()
});
static JUSTIFY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)text-align\s*:\s*justify").unwrap());
static COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)/\*.*?\*/").unwrap());

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CssFacts {
    /// `outline: none|0` 선언 수
    pub outline_removed: usize,
    pub has_focus_visible: bool,
    pub font_faces: usize,
    /// font-display 가 없는 @font-face 수
    pub font_faces_without_display: usize,
    /// `@import` 규칙 수 (스타일시트를 직렬로 더 받게 한다)
    pub imports: usize,
    /// 화면 방향 미디어 쿼리 안에서 90도 회전시키는 규칙 (방향 고정)
    pub orientation_lock: usize,
    /// 분석한 CSS 바이트
    pub bytes: usize,
    /// 주석·불필요한 공백을 지우면 줄어드는 바이트 (압축 안 된 CSS 추정)
    pub minifiable_bytes: usize,
    /// 움직임을 만드는 animation·transition 선언 수 (none·0s 제외)
    pub animations: usize,
    /// `prefers-reduced-motion` 미디어 쿼리 사용
    pub reduced_motion: bool,
    /// `text-align: justify` 선언 수
    pub justify: usize,
    /// `text-decoration: blink` 선언 수
    pub blink: usize,
}

impl CssFacts {
    pub fn merge(&mut self, other: &CssFacts) {
        self.outline_removed += other.outline_removed;
        self.has_focus_visible |= other.has_focus_visible;
        self.font_faces += other.font_faces;
        self.font_faces_without_display += other.font_faces_without_display;
        self.imports += other.imports;
        self.orientation_lock += other.orientation_lock;
        self.bytes += other.bytes;
        self.minifiable_bytes += other.minifiable_bytes;
        self.animations += other.animations;
        self.reduced_motion |= other.reduced_motion;
        self.justify += other.justify;
        self.blink += other.blink;
    }
}

pub fn analyze(css: &str) -> CssFacts {
    let original = css.len();
    let css = COMMENT.replace_all(css, "");
    let collapsed = WHITESPACE.replace_all(&css, " ").len();
    let mut f = CssFacts {
        outline_removed: OUTLINE_NONE.find_iter(&css).count(),
        has_focus_visible: css.contains(":focus-visible"),
        imports: IMPORT.find_iter(&css).count(),
        orientation_lock: ORIENTATION_LOCK.find_iter(&css).count(),
        bytes: original,
        minifiable_bytes: original - collapsed,
        animations: MOTION
            .captures_iter(&css)
            .filter(|c| !matches!(c[1].trim(), "none" | "0s" | "0" | "all 0s"))
            .count(),
        reduced_motion: css.to_ascii_lowercase().contains("prefers-reduced-motion"),
        justify: JUSTIFY.find_iter(&css).count(),
        blink: crate::aria::css_blink(&css),
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
            "@import url(a.css); /* @import x; */ /* outline:none; */ a:focus{outline:none} b{outline: 0 !important;} c{outline:1px solid}\
             .x:focus-visible{} @font-face{font-family:A;src:url(a.woff2)} @font-face{font-family:B;font-display:swap}",
        );
        assert_eq!(
            f,
            CssFacts {
                outline_removed: 2,
                has_focus_visible: true,
                font_faces: 2,
                font_faces_without_display: 1,
                imports: 1,
                orientation_lock: 0,
                bytes: f.bytes,
                minifiable_bytes: f.minifiable_bytes,
                animations: 0,
                reduced_motion: false,
                justify: 0,
                blink: 0,
            }
        );
    }

    #[test]
    fn orientation_lock_and_minify_estimate() {
        let f = analyze(
            "@media (orientation: portrait) {
  body { transform: rotate(90deg); }
}
/* 설명 */
",
        );
        assert_eq!(f.orientation_lock, 1);
        assert!(f.minifiable_bytes > 10 && f.bytes > f.minifiable_bytes);
        assert_eq!(analyze("a{b:c}").minifiable_bytes, 0);
    }

    #[test]
    fn motion_and_justify() {
        let f = analyze(
            "a{transition: color .2s} b{animation-name: spin} c{transition:none} d{animation-duration: 0s}             @media (prefers-reduced-motion: reduce){a{transition:none}} p{text-align: justify}",
        );
        assert_eq!((f.animations, f.reduced_motion, f.justify), (2, true, 1));
        let mut g = analyze("x{TEXT-ALIGN:JUSTIFY}");
        g.merge(&f);
        assert_eq!((g.animations, g.reduced_motion, g.justify), (2, true, 2));
    }
}
