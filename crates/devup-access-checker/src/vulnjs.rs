//! 알려진 취약 버전의 JS 라이브러리 찾기. retire.js 저장소(`data/retire.json`, `scripts/update_retire.py` 로 갱신)의
//! URL·파일명·파일 내용 정규식으로 버전을 알아내고 취약 범위와 비교한다. 네트워크 접근 없음.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;

/// retire.js 의 버전 자리표시자를 대신하는 패턴.
const VERSION: &str = r"[0-9][0-9.a-z_\-]+";

#[derive(Deserialize)]
struct RawLib {
    uri: Vec<String>,
    filename: Vec<String>,
    filecontent: Vec<String>,
    vulnerabilities: Vec<Vuln>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Vuln {
    #[serde(rename = "atOrAbove")]
    pub at_or_above: Option<String>,
    pub below: Option<String>,
    pub severity: String,
    pub ids: Vec<String>,
}

struct Lib {
    name: String,
    uri: Vec<Regex>,
    filename: Vec<Regex>,
    filecontent: Vec<Regex>,
    vulnerabilities: Vec<Vuln>,
}

fn compile(patterns: &[String]) -> Vec<Regex> {
    // Rust regex 가 지원하지 않는 역참조·전후방 탐색 패턴은 건너뛴다
    patterns
        .iter()
        .filter_map(|p| Regex::new(&p.replace("§§version§§", VERSION)).ok())
        .collect()
}

static LIBS: LazyLock<Vec<Lib>> = LazyLock::new(|| {
    let raw: BTreeMap<String, RawLib> =
        serde_json::from_str(include_str!("../data/retire.json")).expect("valid retire.json");
    raw.into_iter()
        .map(|(name, l)| Lib {
            name,
            uri: compile(&l.uri),
            filename: compile(&l.filename),
            filecontent: compile(&l.filecontent),
            vulnerabilities: l.vulnerabilities,
        })
        .collect()
});

/// retire.js 방식의 버전 비교 (점·대시로 나눈 조각을 숫자면 숫자로, 아니면 문자열로).
pub fn compare(a: &str, b: &str) -> Ordering {
    let split = |s: &str| s.split(['.', '-']).map(str::to_string).collect::<Vec<_>>();
    let (x, y) = (split(a), split(b));
    for i in 0..x.len().max(y.len()) {
        let ord = match (x.get(i), y.get(i)) {
            (Some(p), Some(q)) => match (p.parse::<u64>(), q.parse::<u64>()) {
                (Ok(m), Ok(n)) => m.cmp(&n),
                // 숫자 조각이 접미사(rc·beta)보다 나중 버전
                (Ok(_), Err(_)) => Ordering::Greater,
                (Err(_), Ok(_)) => Ordering::Less,
                (Err(_), Err(_)) => p.cmp(q),
            },
            // 1.0 과 1.0.0 은 같고, 1.0-rc 는 1.0 보다 이르다
            (Some(p), None) => match p.parse::<u64>() {
                Ok(0) => Ordering::Equal,
                Ok(_) => Ordering::Greater,
                Err(_) => Ordering::Less,
            },
            (None, q) => match q.map(|q| q.parse::<u64>()) {
                Some(Ok(0)) | None => Ordering::Equal,
                Some(Ok(_)) => Ordering::Less,
                Some(Err(_)) => Ordering::Greater,
            },
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    Ordering::Equal
}

fn affected(v: &Vuln, version: &str) -> bool {
    v.at_or_above
        .as_deref()
        .is_none_or(|a| compare(version, a) != Ordering::Less)
        && v.below
            .as_deref()
            .is_some_and(|b| compare(version, b) == Ordering::Less)
}

/// 찾은 라이브러리 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub library: String,
    pub version: String,
    pub url: String,
    pub vulnerabilities: Vec<Vuln>,
}

fn first_version(res: &[Regex], text: &str) -> Option<String> {
    res.iter().find_map(|r| {
        let c = r.captures(text)?;
        c.iter()
            .skip(1)
            .flatten()
            .map(|m| {
                // 탐욕적 버전 패턴이 붙여 가져온 `.min`·`.slim` 등을 뗀다
                let mut v = m.as_str();
                for suffix in [".min", "-min", ".slim", ".js", "."] {
                    v = v.strip_suffix(suffix).unwrap_or(v);
                }
                v.to_string()
            })
            .find(|s| s.starts_with(|ch: char| ch.is_ascii_digit()))
    })
}

/// 스크립트 URL 과 (있으면) 본문으로 라이브러리·버전을 알아내고 취약점을 찾는다.
pub fn scan(url: &str, body: Option<&str>) -> Vec<Finding> {
    let filename = url
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit('/')
        .next()
        .unwrap_or_default();
    LIBS.iter()
        .filter_map(|lib| {
            let version = first_version(&lib.uri, url)
                .or_else(|| first_version(&lib.filename, filename))
                .or_else(|| body.and_then(|b| first_version(&lib.filecontent, b)))?;
            let vulnerabilities: Vec<Vuln> = lib
                .vulnerabilities
                .iter()
                .filter(|v| affected(v, &version))
                .cloned()
                .collect();
            Some(Finding {
                library: lib.name.clone(),
                version,
                url: url.to_string(),
                vulnerabilities,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(compare("1.12.4", "1.12.4"), Ordering::Equal);
        assert_eq!(compare("1.9.1", "1.12.0"), Ordering::Less);
        assert_eq!(compare("3.5.0", "3.4.99"), Ordering::Greater);
        assert_eq!(compare("1.0.0-rc1", "1.0.0"), Ordering::Less);
        assert_eq!(compare("1.0.0", "1.0.0-beta"), Ordering::Greater);
        assert_eq!(compare("2.0", "2.0.1"), Ordering::Less);
        assert_eq!(compare("2.0.1", "2.0"), Ordering::Greater);
        assert_eq!(compare("1.0-a", "1.0-b"), Ordering::Less);
        assert_eq!(compare("1.0a", "1.0"), Ordering::Less);
        assert_eq!(compare("1.0.0", "1.0"), Ordering::Equal);
        assert_eq!(compare("1.0", "1.0.0"), Ordering::Equal);
        assert_eq!(compare("1.0", "1.0.rc"), Ordering::Greater);
        assert_eq!(compare("1.1", "1.rc"), Ordering::Greater);
    }

    #[test]
    fn finds_vulnerable_libraries() {
        let f = scan("https://code.jquery.com/jquery-1.12.4.min.js", None);
        let jq = f.iter().find(|x| x.library == "jquery").expect("jquery");
        assert_eq!(jq.version, "1.12.4");
        assert!(!jq.vulnerabilities.is_empty());
        let f = scan(
            "https://cdn.a.com/app.js",
            Some("/*! jQuery v3.7.1 | (c) OpenJS Foundation */"),
        );
        let jq = f
            .iter()
            .find(|x| x.library == "jquery")
            .expect("jquery by content");
        assert_eq!(jq.version, "3.7.1");
        assert!(jq.vulnerabilities.is_empty());
        assert!(scan("https://a.com/main.js", Some("console.log(1)")).is_empty());
        assert!(scan("https://a.com/main.js", None).is_empty());
        let v = Vuln {
            at_or_above: Some("2.0".into()),
            below: Some("3.0".into()),
            severity: "low".into(),
            ids: vec![],
        };
        assert!(affected(&v, "2.5") && !affected(&v, "1.9") && !affected(&v, "3.0"));
        let open = Vuln { below: None, ..v };
        assert!(!affected(&open, "2.5"));
    }
}
