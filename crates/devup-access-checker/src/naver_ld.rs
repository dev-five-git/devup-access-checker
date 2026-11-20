//! 네이버 서치어드바이저 구조화 데이터 가이드의 유형별 필수 속성 (Google 기준과 다르다).
//! 각 가이드의 "필수" 표를 그대로 옮겼다. 네트워크 접근 없음.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value, json};

type Obj = Map<String, Value>;

static DATETIME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?(?:Z|[+-]\d\d:\d\d)$").unwrap()
});
static DURATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^P(?:\d+Y)?(?:\d+M)?(?:\d+W)?(?:\d+D)?(?:T(?:\d+H)?(?:\d+M)?(?:\d+(?:\.\d+)?S)?)?$",
    )
    .unwrap()
});

/// 네이버가 지원하는 연관 채널의 호스트 (가이드의 채널 이름을 도메인으로 옮김).
pub const NAVER_CHANNEL_HOSTS: &[&str] = &[
    "blog.naver.com",
    "smartstore.naver.com",
    "kin.naver.com",
    "chzzk.naver.com",
    "clip.naver.com",
    "daangn.com",
    "threads.net",
    "threads.com",
    "instagram.com",
    "youtube.com",
    "story.kakao.com",
    "pf.kakao.com",
    "tistory.com",
    "tiktok.com",
    "facebook.com",
    "x.com",
    "twitter.com",
];

fn types(o: &Obj) -> Vec<&str> {
    match o.get("@type") {
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    }
}

fn is(o: &Obj, ts: &[&str]) -> bool {
    types(o).iter().any(|t| ts.contains(t))
}

fn walk<'a>(v: &'a Value, out: &mut Vec<&'a Obj>) {
    if let Value::Object(m) = v {
        out.push(m);
    }
    match v {
        Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
        Value::Object(m) => m.values().for_each(|x| walk(x, out)),
        _ => {}
    }
}

fn filled(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::String(s)) => !s.trim().is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
        Some(_) => true,
    }
}

/// 점으로 이은 경로 (`creator.name`). 배열이면 첫 원소를 따른다.
fn path<'a>(o: &'a Obj, p: &str) -> Option<&'a Value> {
    let mut cur: Option<&Value> = None;
    for (i, k) in p.split('.').enumerate() {
        let map = if i == 0 {
            o
        } else {
            match cur? {
                Value::Array(a) => a.first()?.as_object()?,
                v => v.as_object()?,
            }
        };
        cur = map.get(k);
    }
    cur
}

/// `a|b` 는 둘 중 하나만 있어도 된다.
fn need(o: &Obj, keys: &[&str]) -> Vec<String> {
    keys.iter()
        .filter(|k| !k.split('|').any(|p| filled(path(o, p))))
        .map(|k| k.to_string())
        .collect()
}

fn num(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) if !s.contains(['e', 'E']) => s.trim().parse().ok(),
        _ => None,
    }
}

fn list_items(o: &Obj) -> Vec<&Obj> {
    match o.get("itemListElement") {
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_object).collect(),
        Some(Value::Object(m)) => vec![m],
        _ => Vec::new(),
    }
}

fn every_item(o: &Obj, key: &str) -> Vec<String> {
    let items = list_items(o);
    if items.is_empty() {
        return vec!["itemListElement".into()];
    }
    let alt = format!("{key}|item.{key}");
    if items.iter().all(|i| need(i, &[alt.as_str()]).is_empty()) {
        Vec::new()
    } else {
        vec![format!("itemListElement[].{key}")]
    }
}

fn rating(o: &Obj, value_key: &str) -> Vec<String> {
    let mut bad = Vec::new();
    for k in [value_key, "bestRating", "ratingCount", "reviewCount"] {
        if o.get(k).is_some() && num(o.get(k)).is_none_or(|n| n < 0.0) {
            bad.push(format!("{k} 형식"));
        }
    }
    if let (Some(v), Some(b)) = (num(o.get(value_key)), num(o.get("bestRating")))
        && b < v
    {
        bad.push("bestRating < ratingValue".into());
    }
    bad
}

type Rule = fn(&Obj) -> Vec<String>;

/// (id, label, 대상 @type, 규칙)
const RULES: &[(&str, &str, &[&str], Rule)] = &[
    (
        "postal_address",
        "네이버 주소 필수 속성",
        &["PostalAddress"],
        |o| {
            let mut m = need(o, &["streetAddress"]);
            if let Some(code) = o.get("postalCode").and_then(Value::as_str)
                && !code.chars().all(|c| c.is_ascii_digit() || c == '-')
            {
                m.push("postalCode 형식".into());
            }
            m
        },
    ),
    (
        "breadcrumb_list",
        "네이버 이동 경로 필수 속성",
        &["BreadcrumbList"],
        |o| every_item(o, "name"),
    ),
    (
        "carousel",
        "네이버 캐러셀 필수 속성",
        &["ItemList"],
        |o| every_item(o, "image"),
    ),
    (
        "channel",
        "네이버 연관 채널 필수 속성",
        &["Person", "Organization"],
        |o| need(o, &["name", "url", "sameAs"]),
    ),
    ("movie", "네이버 영화 필수 속성", &["Movie"], |o| {
        need(o, &["name", "actor"])
    }),
    (
        "aggregate_rating",
        "네이버 평점 필수 속성",
        &["AggregateRating"],
        |o| {
            let mut m = need(o, &["ratingValue", "ratingCount|reviewCount"]);
            m.extend(rating(o, "ratingValue"));
            m
        },
    ),
    (
        "recipe",
        "네이버 레시피 필수 속성",
        &["Recipe"],
        |o| need(o, &["recipeInstructions|recipeInstruction"]),
    ),
    (
        "restaurant",
        "네이버 식당 필수 속성",
        &["Restaurant"],
        |o| need(o, &["name", "openingHours"]),
    ),
    (
        "review",
        "네이버 리뷰 필수 속성",
        &["Review"],
        |o| {
            let mut m = need(o, &["reviewBody"]);
            if let Some(Value::Object(r)) = o.get("reviewRating") {
                m.extend(rating(r, "ratingValue"));
            }
            m
        },
    ),
    (
        "software_app",
        "네이버 소프트웨어 필수 속성",
        &["SoftwareApplication"],
        |o| need(o, &["name", "applicationCategory"]),
    ),
    (
        "tv_series",
        "네이버 TV 시리즈 필수 속성",
        &["TVSeries"],
        |o| need(o, &["name", "actor"]),
    ),
];

/// 결과: (id, label, None = 해당 유형 없음 / Some(빠진 것, 추가 값))
pub type Outcome = (&'static str, &'static str, Option<(Vec<String>, Value)>);

fn best(objs: &[&Obj], rule: impl Fn(&Obj) -> Vec<String>) -> Option<Vec<String>> {
    objs.iter().map(|o| rule(o)).min_by_key(Vec::len)
}

fn video(o: &Obj) -> Vec<String> {
    let mut m = need(
        o,
        &[
            "name",
            "description",
            "url",
            "thumbnailUrl",
            "uploadDate",
            "duration",
            "cid",
            "creator.name",
            "creator.url",
        ],
    );
    let creator = o.get("creator").and_then(|c| match c {
        Value::Array(a) => a.first(),
        v => Some(v),
    });
    if creator
        .and_then(Value::as_object)
        .is_some_and(|c| !is(c, &["Person"]))
    {
        m.push("creator @type Person".into());
    }
    if let Some(d) = o.get("uploadDate").and_then(Value::as_str)
        && !DATETIME.is_match(d)
    {
        m.push("uploadDate 형식".into());
    }
    if let Some(d) = o.get("duration").and_then(Value::as_str)
        && !DURATION.is_match(d)
    {
        m.push("duration 형식".into());
    }
    if let Some(r) = o.get("contentRating")
        && !matches!(r.as_str(), Some("12+" | "15+" | "18+"))
    {
        m.push("contentRating 값".into());
    }
    for k in ["channelName", "channelUrl"] {
        if o.contains_key(k) {
            m.push(format!("{k} 는 최상위에 둘 수 없음"));
        }
    }
    if let Some(Value::Object(c)) = o.get("creator") {
        for k in ["cid", "mobileUrl", "mobileChannelUrl"] {
            if c.contains_key(k) {
                m.push(format!("creator.{k} 는 최상위에 두어야 함"));
            }
        }
    }
    if o.get("interactionType").is_some_and(Value::is_string) {
        m.push("interactionType 은 객체".into());
    }
    m
}

fn video_channel(o: &Obj) -> Vec<String> {
    let mut m = need(
        o,
        &[
            "name",
            "url",
            "cid",
            "description",
            "mainEntityOfPage.dateCreated",
            "mainEntityOfPage.dateModified",
        ],
    );
    for k in [
        "mainEntityOfPage.dateCreated",
        "mainEntityOfPage.dateModified",
    ] {
        if path(o, k)
            .and_then(Value::as_str)
            .is_some_and(|d| !DATETIME.is_match(d))
        {
            m.push(format!("{k} 형식"));
        }
    }
    if let Some(a) = o.get("adultYn")
        && !matches!(a.as_str(), Some("1" | "0"))
    {
        m.push("adultYn 값".into());
    }
    m
}

/// 모든 JSON-LD 블록(`roots`)을 네이버 기준으로 확인한다. `og_image` = 페이지에 og:image 가 있는지.
pub fn evaluate(roots: &[Value], og_image: bool) -> Vec<Outcome> {
    let mut all = Vec::new();
    roots.iter().for_each(|r| walk(r, &mut all));
    let mut out: Vec<Outcome> = RULES
        .iter()
        .map(|(id, label, ts, rule)| {
            let objs: Vec<&Obj> = all.iter().copied().filter(|o| is(o, ts)).collect();
            let extra = if *id == "channel" {
                let hosts: Vec<String> = objs
                    .iter()
                    .filter_map(|o| o.get("sameAs"))
                    .flat_map(|v| match v {
                        Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
                        v => v.as_str().into_iter().collect::<Vec<_>>(),
                    })
                    .filter_map(|u| {
                        url::Url::parse(u).ok()?.host_str().map(|h| {
                            h.trim_start_matches("www.")
                                .trim_start_matches("m.")
                                .to_string()
                        })
                    })
                    .filter(|h| {
                        NAVER_CHANNEL_HOSTS
                            .iter()
                            .any(|n| h == n || h.ends_with(&format!(".{n}")))
                    })
                    .collect();
                json!({ "naverChannels": hosts })
            } else {
                json!({})
            };
            (*id, *label, best(&objs, rule).map(|m| (m, extra)))
        })
        .collect();

    // 동영상: 최상위 객체만 본다 (@graph 는 지원하지 않음)
    let graph = roots.iter().any(|r| r.get("@graph").is_some());
    let top: Vec<&Obj> = roots
        .iter()
        .flat_map(|r| match r {
            Value::Array(a) => a.iter().filter_map(Value::as_object).collect(),
            v => v.as_object().into_iter().collect::<Vec<_>>(),
        })
        .collect();
    let videos: Vec<&Obj> = top
        .iter()
        .copied()
        .filter(|o| is(o, &["VideoObject"]))
        .collect();
    let all_videos = all.iter().any(|o| is(o, &["VideoObject"]));
    let video_result = if videos.is_empty() && !(graph && all_videos) {
        None
    } else {
        let mut m = best(&videos, video).unwrap_or_else(|| vec!["최상위 VideoObject".into()]);
        if videos.len() > 1 {
            m.push("VideoObject 는 한 페이지에 하나".into());
        }
        if graph {
            m.push("@graph 는 지원하지 않음".into());
        }
        let paid = videos.iter().any(|o| o.contains_key("offers"));
        Some((m, json!({ "paid": paid })))
    };
    out.push(("video", "네이버 동영상 필수 속성", video_result));
    let channels: Vec<&Obj> = top
        .iter()
        .copied()
        .filter(|o| is(o, &["Person"]) && o.contains_key("cid"))
        .collect();
    out.push((
        "video_channel",
        "네이버 동영상 채널 필수 속성",
        best(&channels, video_channel).map(|mut m| {
            if !og_image {
                m.push("og:image".into());
            }
            (m, json!({}))
        }),
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(v: Value, og: bool) -> Vec<Outcome> {
        evaluate(&[v], og)
    }

    fn get<'a>(r: &'a [Outcome], id: &str) -> &'a Option<(Vec<String>, Value)> {
        &r.iter().find(|(i, ..)| *i == id).unwrap().2
    }

    fn missing(r: &[Outcome], id: &str) -> Vec<String> {
        get(r, id).as_ref().unwrap().0.clone()
    }

    #[test]
    fn type_rules() {
        let r = run(
            json!([
                {"@type": "PostalAddress", "streetAddress": "강남대로 1", "postalCode": "06000"},
                {"@type": "BreadcrumbList", "itemListElement": [{"@type": "ListItem", "name": "홈"}, {"@type": "ListItem", "item": {"name": "뉴스"}}]},
                {"@type": "ItemList", "itemListElement": {"@type": "ListItem", "image": "a.jpg"}},
                {"@type": ["Organization"], "name": "데브파이브", "url": "https://a.com", "sameAs": ["https://blog.naver.com/x", "https://www.instagram.com/x", "https://example.com", "nope"]},
                {"@type": "Movie", "name": "영화"},
                {"@type": "AggregateRating", "ratingValue": "4.5", "reviewCount": 3, "bestRating": 5},
                {"@type": "Recipe", "recipeInstruction": "끓인다"},
                {"@type": "Restaurant", "name": "식당", "openingHours": "Mo-Fr 09:00-18:00"},
                {"@type": "Review", "reviewBody": "좋다", "reviewRating": {"ratingValue": 5, "bestRating": 4}},
                {"@type": "SoftwareApplication", "name": "앱"},
                {"@type": "TVSeries", "name": "드라마", "actor": [{"name": "배우"}]}
            ]),
            true,
        );
        for id in [
            "postal_address",
            "breadcrumb_list",
            "carousel",
            "channel",
            "aggregate_rating",
            "recipe",
            "restaurant",
            "tv_series",
        ] {
            assert_eq!(missing(&r, id), Vec::<String>::new(), "{id}");
        }
        assert_eq!(
            get(&r, "channel").as_ref().unwrap().1,
            json!({"naverChannels": ["blog.naver.com", "instagram.com"]})
        );
        assert_eq!(missing(&r, "movie"), ["actor"]);
        assert_eq!(missing(&r, "review"), ["bestRating < ratingValue"]);
        assert_eq!(missing(&r, "software_app"), ["applicationCategory"]);
        assert!(get(&r, "video").is_none() && get(&r, "video_channel").is_none());

        let r = run(
            json!([
                {"@type": "PostalAddress", "postalCode": "서울"},
                {"@type": "BreadcrumbList", "itemListElement": [{"name": "홈"}, {"item": "https://a.com/x"}]},
                {"@type": "BreadcrumbList"},
                {"@type": "ItemList", "itemListElement": "x"},
                {"@type": "AggregateRating", "ratingValue": -1, "ratingCount": "1e3"},
                {"@type": "Review", "reviewBody": "", "reviewRating": {"ratingValue": "x"}},
                {"@type": 5}
            ]),
            true,
        );
        assert_eq!(
            missing(&r, "postal_address"),
            ["streetAddress", "postalCode 형식"]
        );
        assert_eq!(missing(&r, "breadcrumb_list"), ["itemListElement[].name"]);
        assert_eq!(missing(&r, "carousel"), ["itemListElement"]);
        assert_eq!(
            missing(&r, "aggregate_rating"),
            ["ratingValue 형식", "ratingCount 형식"]
        );
        assert_eq!(missing(&r, "review"), ["reviewBody", "ratingValue 형식"]);
        assert!(get(&r, "movie").is_none());
    }

    #[test]
    fn object_values_and_single_same_as() {
        let r = run(
            json!([{"@type": "Movie", "name": "m", "actor": {"name": "a"}},
                   {"@type": "Person", "name": "p", "url": "https://a.com", "sameAs": "https://m.blog.naver.com/p"}]),
            true,
        );
        assert_eq!(missing(&r, "movie"), Vec::<String>::new());
        assert_eq!(
            get(&r, "channel").as_ref().unwrap().1,
            json!({"naverChannels": ["blog.naver.com"]})
        );
    }

    #[test]
    fn video_rules() {
        let good = json!({
            "@type": "VideoObject", "name": "영상", "description": "설명", "url": "https://a.com/v",
            "thumbnailUrl": "https://a.com/t.jpg", "uploadDate": "2026-01-01T10:00:00+09:00", "duration": "PT1M30S",
            "cid": "abc", "contentRating": "15+", "creator": {"@type": "Person", "name": "채널", "url": "https://a.com/c"},
            "interactionStatistic": {"interactionType": {"@type": "WatchAction"}}
        });
        let r = run(good.clone(), true);
        assert_eq!(missing(&r, "video"), Vec::<String>::new());
        assert_eq!(get(&r, "video").as_ref().unwrap().1, json!({"paid": false}));

        let bad = json!({
            "@type": "VideoObject", "name": "영상", "uploadDate": "2026-01-01", "duration": "90초",
            "contentRating": "전체", "channelName": "c", "channelUrl": "u", "interactionType": "WatchAction",
            "offers": {}, "creator": {"@type": "Organization", "name": "c", "url": "u", "cid": "x", "mobileUrl": "m", "mobileChannelUrl": "mc"}
        });
        let r = evaluate(&[bad, good.clone()], true);
        let m = missing(&r, "video");
        assert!(
            m.contains(&"VideoObject 는 한 페이지에 하나".to_string()),
            "{m:?}"
        );
        let r = run(
            json!({"@type": "VideoObject", "creator": [{"@type": "Organization"}]}),
            true,
        );
        let m = missing(&r, "video");
        for s in ["description", "creator.name", "creator @type Person"] {
            assert!(m.contains(&s.to_string()), "{s} {m:?}");
        }
        let r = run(
            json!({
                "@type": "VideoObject", "name": "영상", "uploadDate": "2026-01-01", "duration": "90초",
                "contentRating": "전체", "channelName": "c", "channelUrl": "u", "interactionType": "WatchAction",
                "offers": {}, "creator": {"@type": "Organization", "name": "c", "url": "u", "cid": "x", "mobileUrl": "m", "mobileChannelUrl": "mc"}
            }),
            true,
        );
        let (m, v) = get(&r, "video").clone().unwrap();
        for s in [
            "uploadDate 형식",
            "duration 형식",
            "contentRating 값",
            "channelName 는 최상위에 둘 수 없음",
            "channelUrl 는 최상위에 둘 수 없음",
            "creator.cid 는 최상위에 두어야 함",
            "creator.mobileUrl 는 최상위에 두어야 함",
            "creator.mobileChannelUrl 는 최상위에 두어야 함",
            "interactionType 은 객체",
            "creator @type Person",
        ] {
            assert!(m.contains(&s.to_string()), "{s} {m:?}");
        }
        assert_eq!(v, json!({"paid": true}));

        // @graph 안의 동영상
        let r = run(json!({"@graph": [good]}), true);
        assert_eq!(
            missing(&r, "video"),
            ["최상위 VideoObject", "@graph 는 지원하지 않음"]
        );
        let r = run(json!({"@graph": [{"@type": "Thing"}]}), true);
        assert!(get(&r, "video").is_none());
    }

    #[test]
    fn video_channel_rules() {
        let ch = json!({
            "@type": "Person", "name": "채널", "url": "https://a.com/c", "cid": "c1", "description": "설명", "adultYn": "0",
            "mainEntityOfPage": {"dateCreated": "2026-01-01T00:00:00Z", "dateModified": "2026-02-01T00:00:00Z"}
        });
        let r = run(ch, true);
        assert_eq!(missing(&r, "video_channel"), Vec::<String>::new());
        let r = run(
            json!({"@type": "Person", "cid": "c", "adultYn": "Y", "mainEntityOfPage": {"dateCreated": "어제"}}),
            false,
        );
        assert_eq!(
            missing(&r, "video_channel"),
            [
                "name",
                "url",
                "description",
                "mainEntityOfPage.dateModified",
                "mainEntityOfPage.dateCreated 형식",
                "adultYn 값",
                "og:image"
            ]
        );
        // 경로 중간이 배열·객체가 아닌 경우
        let r = run(
            json!({"@type": "Person", "cid": "c", "mainEntityOfPage": "https://a.com"}),
            true,
        );
        assert!(missing(&r, "video_channel").contains(&"mainEntityOfPage.dateCreated".to_string()));
        let r = run(
            json!({"@type": "Person", "cid": "c", "mainEntityOfPage": []}),
            true,
        );
        assert!(missing(&r, "video_channel").contains(&"mainEntityOfPage.dateCreated".to_string()));
        let r = run(
            json!({"@type": "Person", "cid": "c", "mainEntityOfPage": [{"dateCreated": "2026-01-01T00:00:00Z"}]}),
            true,
        );
        assert!(
            !missing(&r, "video_channel").contains(&"mainEntityOfPage.dateCreated".to_string())
        );
    }
}
