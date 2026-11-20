//! 보안 스캐너(Mozilla HTTP Observatory, webhint, hstspreload.org, RFC 9116)와 대조해 추가한 해석기.
//! CSP·쿠키·HSTS·security.txt 값과 HTML 신호를 해석한다. 네트워크 접근 없음.

use std::sync::LazyLock;

use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use url::Url;

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("valid selector")
}

static ALL: LazyLock<Selector> = LazyLock::new(|| sel("*"));
static SRI_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(sha256|sha384|sha512)-[A-Za-z0-9+/_-]+={0,2}(\?.*)?$").unwrap()
});

// ===== CSP =====

/// 지나치게 넓은 출처 (Observatory DANGEROUSLY_BROAD).
const BROAD: &[&str] = &[
    "*",
    "http:",
    "https:",
    "ftp:",
    "http://*",
    "http://*.*",
    "https://*",
    "https://*.*",
];

/// 정책 하나: (지시어, 출처 목록). 지시어 이름은 소문자.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Csp {
    pub directives: Vec<(String, Vec<String>)>,
    /// 두 번 이상 나온 지시어 (report-uri·report-to 제외)
    pub duplicates: Vec<String>,
}

pub fn parse_csp(policy: &str) -> Csp {
    let mut csp = Csp::default();
    for part in policy.split(';') {
        let mut it = part.split_ascii_whitespace();
        let Some(name) = it.next() else { continue };
        let name = name.to_ascii_lowercase();
        if csp.directives.iter().any(|(n, _)| *n == name) {
            if !matches!(name.as_str(), "report-uri" | "report-to")
                && !csp.duplicates.contains(&name)
            {
                csp.duplicates.push(name);
            }
            continue;
        }
        csp.directives
            .push((name, it.map(str::to_ascii_lowercase).collect()));
    }
    csp
}

impl Csp {
    pub fn get(&self, d: &str) -> Option<&[String]> {
        self.directives
            .iter()
            .find(|(n, _)| n == d)
            .map(|(_, v)| v.as_slice())
    }

    /// 지시어가 없으면 default-src 로 대체한다 (None = 제한 없음).
    pub fn effective(&self, d: &str) -> Option<&[String]> {
        self.get(d).or_else(|| self.get("default-src"))
    }
}

fn has_nonce_or_hash(srcs: &[String]) -> bool {
    srcs.iter().any(|s| {
        s.starts_with("'nonce-")
            || s.starts_with("'sha256-")
            || s.starts_with("'sha384-")
            || s.starts_with("'sha512-")
    })
}

fn is_broad(srcs: Option<&[String]>) -> bool {
    srcs.is_none_or(|s| s.iter().any(|x| BROAD.contains(&x.as_str())))
}

fn allows_inline(srcs: Option<&[String]>) -> bool {
    srcs.is_none_or(|s| {
        s.iter().any(|x| x == "'unsafe-inline'" || x == "data:") && !has_nonce_or_hash(s)
    })
}

impl Csp {
    pub fn unsafe_inline(&self) -> bool {
        allows_inline(self.effective("script-src"))
    }
    pub fn unsafe_eval(&self) -> bool {
        ["script-src", "style-src"].iter().any(|d| {
            self.effective(d)
                .is_some_and(|s| s.iter().any(|x| x == "'unsafe-eval'"))
        })
    }
    pub fn broad_script(&self) -> bool {
        let s = self.effective("script-src");
        // strict-dynamic + nonce/hash 이면 호스트·스킴 출처는 무시된다
        if s.is_some_and(|s| s.iter().any(|x| x == "'strict-dynamic'") && has_nonce_or_hash(s)) {
            return false;
        }
        is_broad(s)
    }
    pub fn object_unrestricted(&self) -> bool {
        is_broad(self.effective("object-src"))
    }
    pub fn unsafe_inline_style(&self) -> bool {
        let s = self.effective("style-src");
        allows_inline(s) || is_broad(s)
    }
    pub fn insecure_scheme(&self) -> bool {
        self.directives
            .iter()
            .flat_map(|(_, v)| v)
            .any(|x| x.starts_with("http:") || x.starts_with("ftp:"))
    }
    pub fn base_uri_unrestricted(&self) -> bool {
        let s = self.get("base-uri");
        is_broad(s) || s.is_some_and(|s| s.iter().any(|x| x == "'unsafe-inline'" || x == "data:"))
    }
    pub fn form_action_unrestricted(&self) -> bool {
        is_broad(self.get("form-action"))
    }
    pub fn strict_dynamic_without_nonce(&self) -> bool {
        self.get("script-src")
            .is_some_and(|s| s.iter().any(|x| x == "'strict-dynamic'") && !has_nonce_or_hash(s))
    }
    pub fn frame_ancestors_restricted(&self) -> bool {
        self.get("frame-ancestors")
            .is_some_and(|s| !is_broad(Some(s)))
    }
}

// ===== HSTS =====

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hsts {
    pub max_age: Option<u64>,
    pub include_subdomains: bool,
    pub preload: bool,
    /// 형식 오류 사유 (max-age 없음·숫자 아님·지시어 중복·헤더 두 번)
    pub errors: Vec<String>,
}

pub fn parse_hsts(value: &str) -> Hsts {
    let mut h = Hsts::default();
    if value.contains(',') {
        h.errors.push("헤더가 여러 번 전송됨".into());
    }
    let mut seen: Vec<String> = Vec::new();
    for part in value
        .split([';', ','])
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        let (k, v) = part
            .split_once('=')
            .map_or((part, None), |(k, v)| (k, Some(v)));
        let k = k.trim().to_ascii_lowercase();
        if seen.contains(&k) {
            h.errors.push(format!("{k} 중복"));
            continue;
        }
        seen.push(k.clone());
        match k.as_str() {
            "max-age" => match v.map(|v| v.trim().trim_matches('"').parse::<u64>()) {
                Some(Ok(n)) => h.max_age = Some(n),
                _ => h.errors.push("max-age 값 오류".into()),
            },
            "includesubdomains" => h.include_subdomains = true,
            "preload" => h.preload = true,
            _ => {}
        }
    }
    if h.max_age.is_none() && !h.errors.iter().any(|e| e.starts_with("max-age")) {
        h.errors.push("max-age 없음".into());
    }
    h
}

// ===== 쿠키 =====

const COOKIE_ATTRS: &[&str] = &[
    "expires",
    "max-age",
    "domain",
    "path",
    "secure",
    "httponly",
    "samesite",
    "partitioned",
    "priority",
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CookieIssues {
    pub cookies: usize,
    pub samesite_invalid: Vec<String>,
    pub session_flags: Vec<String>,
    pub prefix_invalid: Vec<String>,
    pub syntax_invalid: Vec<String>,
}

fn is_token(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_graphic() && !b"()<>@,;:\\\"/[]?={}".contains(&b))
}

/// `Set-Cookie` 헤더 값들을 확인한다. `https` = 페이지가 https 인지.
pub fn check_cookies(values: &[String], https: bool) -> CookieIssues {
    let mut out = CookieIssues {
        cookies: values.len(),
        ..Default::default()
    };
    for raw in values {
        let mut parts = raw.split(';');
        let first = parts.next().unwrap_or_default().trim();
        let (name, _) = first.split_once('=').unwrap_or((first, ""));
        let name = name.trim();
        let mut problems: Vec<&str> = Vec::new();
        if !first.contains('=') || !is_token(name) {
            problems.push("이름=값 형식 오류");
        }
        if raw.trim_end().ends_with(';') {
            problems.push("끝에 ;");
        }
        let mut attrs: Vec<(String, Option<String>)> = Vec::new();
        for a in parts.map(str::trim).filter(|a| !a.is_empty()) {
            let (k, v) = a
                .split_once('=')
                .map_or((a, None), |(k, v)| (k, Some(v.trim().to_string())));
            let k = k.trim().to_ascii_lowercase();
            if !COOKIE_ATTRS.contains(&k.as_str()) {
                problems.push("알 수 없는 속성");
            }
            if attrs.iter().any(|(x, _)| *x == k) {
                problems.push("속성 중복");
            }
            if k == "expires"
                && !v
                    .as_deref()
                    .is_some_and(|d| d.to_ascii_uppercase().ends_with("GMT"))
            {
                problems.push("Expires 날짜 형식");
            }
            attrs.push((k, v));
        }
        let has = |k: &str| attrs.iter().any(|(x, _)| x == k);
        let secure = has("secure");
        if secure && !https {
            problems.push("http 페이지의 Secure");
        }
        if let Some((_, v)) = attrs.iter().find(|(k, _)| k == "samesite") {
            let v = v.as_deref().unwrap_or_default().to_ascii_lowercase();
            if !matches!(v.as_str(), "lax" | "strict" | "none") || (v == "none" && !secure) {
                out.samesite_invalid.push(name.to_string());
            }
        }
        let lname = name.to_ascii_lowercase();
        if (["sess", "login", "auth"].iter().any(|k| lname.contains(k))
            && (!secure || !has("httponly")))
            || (["csrf", "xsrf"].iter().any(|k| lname.contains(k)) && !has("samesite"))
        {
            out.session_flags.push(name.to_string());
        }
        let path_root = attrs
            .iter()
            .any(|(k, v)| k == "path" && v.as_deref() == Some("/"));
        if (name.starts_with("__Host-") && (has("domain") || !path_root || !secure))
            || (name.starts_with("__Secure-") && !secure)
            || ((name.starts_with("__Host-") || name.starts_with("__Secure-")) && !https)
        {
            out.prefix_invalid.push(name.to_string());
        }
        if !problems.is_empty() {
            out.syntax_invalid
                .push(format!("{name}: {}", problems.join(", ")));
        }
    }
    out
}

// ===== security.txt (RFC 9116) =====

/// RFC 3339 날짜를 UNIX 초로 (형식이 틀리면 None).
pub fn rfc3339_to_unix(s: &str) -> Option<i64> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(\d{4})-(\d{2})-(\d{2})[Tt](\d{2}):(\d{2}):(\d{2})(?:\.\d+)?(?:[Zz]|([+-])(\d{2}):(\d{2}))$").unwrap()
    });
    let c = RE.captures(s.trim())?;
    let n = |i: usize| {
        c.get(i)
            .map_or(0, |m| m.as_str().parse::<i64>().unwrap_or(0))
    };
    let (y, m, d) = (n(1), n(2), n(3));
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // days from civil (Howard Hinnant)
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let offset = (n(8) * 3600 + n(9) * 60)
        * if c.get(7).is_some_and(|s| s.as_str() == "-") {
            -1
        } else {
            1
        };
    Some(days * 86_400 + n(4) * 3600 + n(5) * 60 + n(6) - offset)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SecurityTxt {
    /// (필드 이름 소문자, 값)
    pub fields: Vec<(String, String)>,
    pub signed: bool,
    pub errors: Vec<String>,
}

const URI_FIELDS: &[&str] = &[
    "contact",
    "encryption",
    "acknowledgments",
    "canonical",
    "policy",
    "hiring",
    "csaf",
];

pub fn parse_security_txt(body: &str) -> SecurityTxt {
    let mut t = SecurityTxt {
        signed: body
            .trim_start()
            .starts_with("-----BEGIN PGP SIGNED MESSAGE-----"),
        ..Default::default()
    };
    let mut in_sig = false;
    for line in body.lines().map(str::trim) {
        if line.starts_with("-----BEGIN PGP SIGNATURE") {
            in_sig = true;
        }
        if in_sig
            || line.is_empty()
            || line.starts_with('#')
            || line.starts_with("-----")
            || line.starts_with("Hash:")
        {
            continue;
        }
        let Some((k, v)) = line.split_once(':') else {
            t.errors.push(format!("형식 오류: {line}"));
            continue;
        };
        let k = k.trim().to_ascii_lowercase();
        let v = v.trim().to_string();
        if URI_FIELDS.contains(&k.as_str()) && v.starts_with("http") && !v.starts_with("https://") {
            t.errors.push(format!("{k} 는 https URI 여야 함"));
        }
        if k == "expires" && rfc3339_to_unix(&v).is_none() {
            t.errors.push("Expires 가 RFC 3339 형식이 아님".into());
        }
        t.fields.push((k, v));
    }
    for once in ["expires", "preferred-languages"] {
        if t.fields.iter().filter(|(k, _)| k == once).count() > 1 {
            t.errors.push(format!("{once} 중복"));
        }
    }
    t
}

impl SecurityTxt {
    pub fn values(&self, name: &str) -> Vec<&str> {
        self.fields
            .iter()
            .filter(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
            .collect()
    }
}

// ===== HTML 신호 =====

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SecFacts {
    /// `<meta http-equiv="Content-Security-Policy">` 값
    pub meta_csp: Vec<String>,
    /// `<meta http-equiv>` 이름 (소문자)
    pub http_equiv: Vec<String>,
    pub p3p_link: bool,
    pub protocol_relative: Vec<String>,
    pub protocol_relative_count: usize,
    /// form 안(또는 form 속성)의 type 없는 button
    pub buttons_without_type: usize,
    pub buttons_in_forms: usize,
    /// integrity 속성 요소 수와 문제
    pub sri_elements: usize,
    pub sri_issues: Vec<String>,
    /// `meta[charset]` + `meta[http-equiv=content-type]` 수
    pub charset_metas: usize,
    /// 첫 charset meta 가 head 의 첫 요소인지
    pub charset_first_in_head: bool,
    /// `meta[name=theme-color]` 값들
    pub theme_colors: Vec<String>,
    /// `meta[name=viewport]` 수
    pub viewports: usize,
}

fn push(v: &mut Vec<String>, s: String) {
    if v.len() < 20 && !v.contains(&s) {
        v.push(s);
    }
}

pub fn extract(doc: &Html, base: &Url) -> SecFacts {
    let mut f = SecFacts::default();
    for el in doc.select(&ALL) {
        let e = el.value();
        let name = e.name();
        let get = |a: &str| e.attr(a).map(str::trim);
        if name == "meta" {
            let equiv = get("http-equiv").map(str::to_ascii_lowercase);
            if let Some(eq) = &equiv {
                push(&mut f.http_equiv, eq.clone());
                if eq == "content-security-policy" {
                    f.meta_csp
                        .push(get("content").unwrap_or_default().to_string());
                }
            }
            if e.attr("charset").is_some() || equiv.as_deref() == Some("content-type") {
                f.charset_metas += 1;
                if f.charset_metas == 1 {
                    f.charset_first_in_head =
                        el.parent().and_then(ElementRef::wrap).is_some_and(|h| {
                            h.value().name() == "head"
                                && h.children()
                                    .filter_map(ElementRef::wrap)
                                    .next()
                                    .is_some_and(|c| c.id() == el.id())
                        });
                }
            }
            match get("name").map(str::to_ascii_lowercase).as_deref() {
                Some("theme-color") => f
                    .theme_colors
                    .push(get("content").unwrap_or_default().to_string()),
                Some("viewport") => f.viewports += 1,
                _ => {}
            }
        }
        if name == "link" && get("rel").is_some_and(|r| r.eq_ignore_ascii_case("p3pv1")) {
            f.p3p_link = true;
        }
        for a in ["src", "href"] {
            if let Some(v) = get(a)
                && v.starts_with("//")
                && matches!(name, "a" | "img" | "link" | "script" | "iframe" | "source")
            {
                f.protocol_relative_count += 1;
                push(&mut f.protocol_relative, v.to_string());
            }
        }
        if name == "button"
            && (e.attr("form").is_some()
                || el
                    .ancestors()
                    .filter_map(ElementRef::wrap)
                    .any(|a| a.value().name() == "form"))
        {
            f.buttons_in_forms += 1;
            f.buttons_without_type += usize::from(e.attr("type").is_none());
        }
        if let Some(integrity) = e.attr("integrity") {
            f.sri_elements += 1;
            let src = get("src").or_else(|| get("href")).unwrap_or_default();
            let tokens: Vec<&str> = integrity.split_ascii_whitespace().collect();
            if tokens.is_empty() || !tokens.iter().all(|t| SRI_TOKEN.is_match(t)) {
                push(&mut f.sri_issues, format!("형식 오류: {src}"));
            } else if !tokens
                .iter()
                .any(|t| t.starts_with("sha384") || t.starts_with("sha512"))
            {
                push(&mut f.sri_issues, format!("sha384 미만: {src}"));
            }
            if let Ok(u) = base.join(src) {
                if u.origin() != base.origin()
                    && !matches!(
                        get("crossorigin").map(str::to_ascii_lowercase).as_deref(),
                        Some("" | "anonymous" | "use-credentials")
                    )
                {
                    push(&mut f.sri_issues, format!("crossorigin 없음: {src}"));
                }
                if u.scheme() == "http" {
                    push(&mut f.sri_issues, format!("http 로 로드: {src}"));
                }
            }
        }
    }
    f
}

// ===== CSS 색 (theme-color) =====

const NAMED_COLORS: &[&str] = &[
    "aliceblue",
    "antiquewhite",
    "aqua",
    "aquamarine",
    "azure",
    "beige",
    "bisque",
    "black",
    "blanchedalmond",
    "blue",
    "blueviolet",
    "brown",
    "burlywood",
    "cadetblue",
    "chartreuse",
    "chocolate",
    "coral",
    "cornflowerblue",
    "cornsilk",
    "crimson",
    "cyan",
    "darkblue",
    "darkcyan",
    "darkgoldenrod",
    "darkgray",
    "darkgreen",
    "darkgrey",
    "darkkhaki",
    "darkmagenta",
    "darkolivegreen",
    "darkorange",
    "darkorchid",
    "darkred",
    "darksalmon",
    "darkseagreen",
    "darkslateblue",
    "darkslategray",
    "darkslategrey",
    "darkturquoise",
    "darkviolet",
    "deeppink",
    "deepskyblue",
    "dimgray",
    "dimgrey",
    "dodgerblue",
    "firebrick",
    "floralwhite",
    "forestgreen",
    "fuchsia",
    "gainsboro",
    "ghostwhite",
    "gold",
    "goldenrod",
    "gray",
    "green",
    "greenyellow",
    "grey",
    "honeydew",
    "hotpink",
    "indianred",
    "indigo",
    "ivory",
    "khaki",
    "lavender",
    "lavenderblush",
    "lawngreen",
    "lemonchiffon",
    "lightblue",
    "lightcoral",
    "lightcyan",
    "lightgoldenrodyellow",
    "lightgray",
    "lightgreen",
    "lightgrey",
    "lightpink",
    "lightsalmon",
    "lightseagreen",
    "lightskyblue",
    "lightslategray",
    "lightslategrey",
    "lightsteelblue",
    "lightyellow",
    "lime",
    "limegreen",
    "linen",
    "magenta",
    "maroon",
    "mediumaquamarine",
    "mediumblue",
    "mediumorchid",
    "mediumpurple",
    "mediumseagreen",
    "mediumslateblue",
    "mediumspringgreen",
    "mediumturquoise",
    "mediumvioletred",
    "midnightblue",
    "mintcream",
    "mistyrose",
    "moccasin",
    "navajowhite",
    "navy",
    "oldlace",
    "olive",
    "olivedrab",
    "orange",
    "orangered",
    "orchid",
    "palegoldenrod",
    "palegreen",
    "paleturquoise",
    "palevioletred",
    "papayawhip",
    "peachpuff",
    "peru",
    "pink",
    "plum",
    "powderblue",
    "purple",
    "rebeccapurple",
    "red",
    "rosybrown",
    "royalblue",
    "saddlebrown",
    "salmon",
    "sandybrown",
    "seagreen",
    "seashell",
    "sienna",
    "silver",
    "skyblue",
    "slateblue",
    "slategray",
    "slategrey",
    "snow",
    "springgreen",
    "steelblue",
    "tan",
    "teal",
    "thistle",
    "tomato",
    "turquoise",
    "violet",
    "wheat",
    "white",
    "whitesmoke",
    "yellow",
    "yellowgreen",
    "transparent",
    "currentcolor",
];

pub fn is_css_color(v: &str) -> bool {
    static HEX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^#(?:[0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$").unwrap());
    static FUNC: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch|color|color-mix|light-dark)\(.+\)$")
            .unwrap()
    });
    let v = v.trim().to_ascii_lowercase();
    HEX.is_match(&v) || FUNC.is_match(&v) || NAMED_COLORS.contains(&v.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csp_rules() {
        let c = parse_csp(
            "default-src 'self'; script-src 'self' 'unsafe-inline' https:; style-src 'self'; object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'; script-src x",
        );
        assert_eq!(c.duplicates, ["script-src"]);
        assert!(c.unsafe_inline() && c.broad_script() && !c.unsafe_eval());
        assert!(!c.object_unrestricted() && !c.unsafe_inline_style() && !c.insecure_scheme());
        assert!(
            !c.base_uri_unrestricted()
                && !c.form_action_unrestricted()
                && c.frame_ancestors_restricted()
        );
        assert!(!c.strict_dynamic_without_nonce());
        let c = parse_csp(
            "script-src 'nonce-abc' 'strict-dynamic' https: 'unsafe-inline'; style-src 'unsafe-eval' 'unsafe-inline' http://cdn; report-uri /r; report-uri /r2;;",
        );
        assert!(c.duplicates.is_empty());
        assert!(!c.unsafe_inline() && !c.broad_script() && c.unsafe_eval());
        assert!(c.object_unrestricted() && c.unsafe_inline_style() && c.insecure_scheme());
        assert!(
            c.base_uri_unrestricted()
                && c.form_action_unrestricted()
                && !c.frame_ancestors_restricted()
        );
        let c = parse_csp("script-src 'strict-dynamic'; base-uri data:; frame-ancestors *");
        assert!(
            c.strict_dynamic_without_nonce()
                && c.base_uri_unrestricted()
                && !c.frame_ancestors_restricted()
        );
        assert!(!c.broad_script());
        assert!(parse_csp("").unsafe_inline());
    }

    #[test]
    fn hsts_rules() {
        let h = parse_hsts("max-age=31536000; includeSubDomains; preload");
        assert_eq!(
            (h.max_age, h.include_subdomains, h.preload, h.errors.len()),
            (Some(31536000), true, true, 0)
        );
        let h = parse_hsts("max-age=abc, max-age=1; max-age=2");
        assert!(h.errors.contains(&"헤더가 여러 번 전송됨".to_string()));
        assert!(h.errors.contains(&"max-age 값 오류".to_string()));
        assert!(h.errors.iter().any(|e| e.contains("중복")));
        assert_eq!(parse_hsts("includeSubDomains").errors, ["max-age 없음"]);
        assert_eq!(parse_hsts("max-age=\"60\"; report-uri=x").max_age, Some(60));
    }

    #[test]
    fn cookie_rules() {
        let v: Vec<String> = [
            "sessionid=1; Path=/; HttpOnly",
            "ok=1; Secure; HttpOnly; SameSite=Lax; Path=/",
            "a=1; SameSite=None",
            "b=1; SameSite=Weird; Secure",
            "csrftoken=1; Secure",
            "__Host-x=1; Secure; Path=/app",
            "__Host-y=1; Secure; Path=/; Domain=a.com",
            "__Secure-z=1",
            "bad name=1; Foo=bar; Path=/; Path=/; Expires=tomorrow;",
            "noequals",
            "e=1; Expires=Wed, 21 Oct 2026 07:28:00 GMT",
        ]
        .map(String::from)
        .to_vec();
        let c = check_cookies(&v, true);
        assert_eq!(c.cookies, 11);
        assert_eq!(c.samesite_invalid, ["a", "b"]);
        assert_eq!(c.session_flags, ["sessionid", "csrftoken"]);
        assert_eq!(c.prefix_invalid, ["__Host-x", "__Host-y", "__Secure-z"]);
        assert_eq!(c.syntax_invalid.len(), 2, "{:?}", c.syntax_invalid);
        let c = check_cookies(&["__Secure-a=1; Secure".to_string()], false);
        assert_eq!(c.prefix_invalid, ["__Secure-a"]);
        assert_eq!(c.syntax_invalid, ["__Secure-a: http 페이지의 Secure"]);
    }

    #[test]
    fn security_txt_rules() {
        assert_eq!(rfc3339_to_unix("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            rfc3339_to_unix("2026-11-20T09:00:00+09:00"),
            Some(1_795_132_800)
        );
        assert_eq!(rfc3339_to_unix("1969-12-31T23:00:00-01:00"), Some(0));
        assert_eq!(rfc3339_to_unix("2026-01-01"), None);
        assert_eq!(rfc3339_to_unix("2026-13-01T00:00:00Z"), None);
        let t = parse_security_txt(
            "-----BEGIN PGP SIGNED MESSAGE-----\nHash: SHA256\n\n# 주석\nContact: mailto:a@a.com\nContact: http://a.com/c\n\
             Expires: 2030-01-01T00:00:00Z\nExpires: 내년\nCanonical: https://a.com/.well-known/security.txt\nbroken line\n\
             -----BEGIN PGP SIGNATURE-----\nabc\n-----END PGP SIGNATURE-----\n",
        );
        assert!(t.signed);
        assert_eq!(t.values("contact"), ["mailto:a@a.com", "http://a.com/c"]);
        assert_eq!(t.errors.len(), 4, "{:?}", t.errors);
    }

    #[test]
    fn html_signals_and_colors() {
        let base = Url::parse("https://a.com/").unwrap();
        let f = extract(
            &Html::parse_document(
                r##"<html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'self'">
<meta http-equiv="X-UA-Compatible" content="IE=edge"><meta http-equiv="content-type" content="text/html; charset=utf-8">
<meta name="theme-color" content="#fff"><meta name="viewport" content="width=device-width"><link rel="P3Pv1" href="/p3p.xml">
<script src="//cdn.x.com/a.js"></script><script src="https://cdn.x.com/b.js" integrity="sha256-abc"></script>
<script src="https://cdn.x.com/c.js" integrity="sha384-abc" crossorigin="anonymous"></script>
<script src="http://a.com/d.js" integrity="md5-x"></script><link rel="stylesheet" href="/s.css" integrity=""></head>
<body><form><button>a</button><button type="submit">b</button></form><button form="f">c</button><button>d</button></body></html>"##,
            ),
            &base,
        );
        assert_eq!(f.meta_csp, ["default-src 'self'"]);
        assert_eq!(
            f.http_equiv,
            ["content-security-policy", "x-ua-compatible", "content-type"]
        );
        assert!(f.p3p_link && f.charset_first_in_head);
        assert_eq!((f.charset_metas, f.viewports), (2, 1));
        assert_eq!(f.theme_colors, ["#fff"]);
        assert_eq!(f.protocol_relative, ["//cdn.x.com/a.js"]);
        assert_eq!((f.buttons_in_forms, f.buttons_without_type), (3, 2));
        assert_eq!(f.sri_elements, 4);
        assert_eq!(f.sri_issues.len(), 6, "{:?}", f.sri_issues);
        let f = extract(
            &Html::parse_document("<html><head><title>t</title><meta charset=utf-8></head></html>"),
            &base,
        );
        assert!(!f.charset_first_in_head);
        for c in [
            "#fff",
            "#FFFFFF80",
            "rgb(0 0 0)",
            "oklch(70% 0.1 200)",
            "RebeccaPurple",
            "transparent",
        ] {
            assert!(is_css_color(c), "{c}");
        }
        for c in ["#ggg", "blu", "rgb", ""] {
            assert!(!is_css_color(c), "{c}");
        }
    }
}
