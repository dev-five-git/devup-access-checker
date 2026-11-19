use devup_access_checker::{Checker, Options, Presence, Report};
use wiremock::matchers::{header_regex, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PAGE: &str = r#"<!doctype html><html lang="ko"><head>
<meta charset="utf-8"><title>데브파이브 홈페이지 테스트</title>
<meta name="description" content="설명">
<meta property="og:image" content="/og.png">
<link rel="icon" href="/favicon.ico">
<link rel="canonical" href="/">
<script type="application/ld+json">{"@context":"https://schema.org","@type":"Organization","name":"x"}</script>
</head><body><h1>제목</h1><img src="/a.png"><p>본문</p></body></html>"#;

async fn site() -> MockServer {
    let s = MockServer::start().await;
    let html = |body: &str| {
        ResponseTemplate::new(200)
            .insert_header("content-type", "text/html")
            .set_body_string(body)
    };
    // GPTBot UA 는 방화벽에 막힌 것으로 가정
    Mock::given(method("GET"))
        .and(path("/"))
        .and(header_regex("user-agent", "GPTBot"))
        .respond_with(ResponseTemplate::new(403))
        .with_priority(1)
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(html(PAGE))
        .mount(&s)
        .await;
    Mock::given(method("GET")).and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "User-agent: *\nAllow: /\n\nUser-agent: GPTBot\nDisallow: /\n\nSitemap: http://x/sitemap.xml\n"))
        .mount(&s).await;
    Mock::given(method("GET"))
        .and(path("/sitemap.xml"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/xml")
                .set_body_string("<?xml version=\"1.0\"?><urlset><url><loc>a</loc></url></urlset>"),
        )
        .mount(&s)
        .await;
    // llms.txt 는 SPA 가 HTML 로 응답 → 없음으로 판정되어야 함
    Mock::given(method("GET"))
        .and(path("/llms.txt"))
        .respond_with(html(PAGE))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/favicon.ico"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&s)
        .await;
    // og.png, ai.txt, llms-full.txt, 404 probe 는 wiremock 기본 404
    s
}

fn presence(r: &Report, id: &str) -> Presence {
    r.get(id)
        .unwrap_or_else(|| panic!("check {id} missing"))
        .presence
}

#[tokio::test]
async fn reports_presence_for_mock_site() {
    let s = site().await;
    let r = Checker::new(Options {
        check_transport: false,
        ..Default::default()
    })
    .check(&s.uri())
    .await
    .unwrap();

    assert_eq!(r.status_code, 200);
    let ids: std::collections::HashSet<_> = r.checks.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids.len(), r.checks.len(), "ids must be unique");

    assert_eq!(presence(&r, "meta.title"), Presence::Present);
    assert_eq!(
        r.get("meta.title").unwrap().value.as_ref().unwrap()["length"],
        14
    );
    assert_eq!(presence(&r, "meta.keywords"), Presence::Missing);
    assert_eq!(presence(&r, "meta.canonical"), Presence::Present);
    assert_eq!(
        r.get("meta.canonical").unwrap().value.as_ref().unwrap()["selfReferencing"],
        true
    );
    assert_eq!(
        presence(&r, "structured_data.type.organization"),
        Presence::Present
    );
    assert_eq!(
        presence(&r, "structured_data.type.faq_page"),
        Presence::Missing
    );
    assert_eq!(presence(&r, "content.img_alt_missing"), Presence::Present);

    assert_eq!(presence(&r, "crawling.sitemap_xml"), Presence::Present);
    assert_eq!(
        r.get("crawling.sitemap_entries")
            .unwrap()
            .value
            .as_ref()
            .unwrap()["entries"],
        1
    );
    assert_eq!(presence(&r, "crawling.robots_txt"), Presence::Present);
    assert_eq!(
        presence(&r, "crawling.robots_sitemap_directive"),
        Presence::Present
    );
    assert_eq!(
        presence(&r, "crawling.robots_allows.googlebot"),
        Presence::Present
    );
    assert_eq!(presence(&r, "geo.robots_allows.gptbot"), Presence::Missing);
    assert_eq!(
        presence(&r, "geo.robots_allows.claudebot"),
        Presence::Present
    );
    assert_eq!(
        presence(&r, "geo.llms_txt"),
        Presence::Missing,
        "HTML fallback is not llms.txt"
    );
    assert_eq!(presence(&r, "geo.ai_txt"), Presence::Missing);
    assert_eq!(presence(&r, "geo.ua_access.gptbot"), Presence::Missing);
    assert_eq!(presence(&r, "geo.ua_access.claudebot"), Presence::Present);

    assert_eq!(
        presence(&r, "not_found_page.returns_404"),
        Presence::Present
    );
    assert_eq!(presence(&r, "links.favicon"), Presence::Present);
    assert_eq!(presence(&r, "links.og_image"), Presence::Missing);
    assert_eq!(presence(&r, "links.manifest"), Presence::NotApplicable);
    assert_eq!(
        presence(&r, "transport.http_to_https_redirect"),
        Presence::Unknown,
        "transport checks disabled"
    );
}

#[tokio::test]
async fn unreachable_target_is_an_error() {
    let err = Checker::default()
        .check("http://127.0.0.1:1")
        .await
        .unwrap_err();
    assert!(matches!(err, devup_access_checker::Error::Fetch { .. }));
}

const KO_PAGE: &str = r#"<!doctype html><html lang="ko"><head><title>t</title>
<link rel="canonical" href="https://www.other-host.kr/ko">
<meta property="og:url" content="/ko/">
<link rel="alternate" type="application/rss+xml" href="/feed.xml">
</head><body><img src="/a.png" width="10" height="10" loading="lazy"></body></html>"#;

#[tokio::test]
async fn redirects_declared_sitemaps_and_site_files() {
    let s = MockServer::start().await;
    let uri = s.uri();
    let ok = |b: String, ct: &str| {
        ResponseTemplate::new(200)
            .insert_header("content-type", ct)
            .set_body_string(b)
    };
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(307).insert_header("location", "/ko/"))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/ko/"))
        .respond_with(ok(KO_PAGE.into(), "text/html"))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ok(
            format!(
                "User-agent: *\nAllow: /\nSitemap: {uri}/sitemap-0.xml\nSitemap: {uri}/gone.xml\n"
            ),
            "text/plain",
        ))
        .mount(&s)
        .await;
    Mock::given(method("GET")).and(path("/sitemap-0.xml"))
        .respond_with(ok(format!(
            "<?xml version=\"1.0\"?><urlset><url><loc>{uri}/ko/</loc><lastmod>2026-11-01</lastmod></url>\
             <url><loc>{uri}/broken</loc></url><url><loc>https://elsewhere.invalid/x</loc></url></urlset>"), "application/xml"))
        .mount(&s).await;
    Mock::given(method("GET"))
        .and(path("/llms.txt"))
        .respond_with(ok(
            "# DevFive\n\n> 요약\n\n## Docs\n- [소개](https://a.com/about.md)\n".into(),
            "text/plain",
        ))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/favicon.ico"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "image/x-icon")
                .set_body_bytes(vec![0u8, 0, 1, 0]),
        )
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/apple-touch-icon.png"))
        .respond_with(ResponseTemplate::new(200).insert_header("content-type", "image/png"))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/.well-known/security.txt"))
        .respond_with(ok(
            "Contact: mailto:security@example.com\n".into(),
            "text/plain",
        ))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/feed.xml"))
        .respond_with(ok("<rss></rss>".into(), "application/rss+xml"))
        .mount(&s)
        .await;

    let r = Checker::new(Options {
        check_ai_user_agents: false,
        sitemap_samples: 10,
        ..Default::default()
    })
    .check(&uri)
    .await
    .unwrap();
    let v = |id: &str| r.get(id).unwrap().value.clone().unwrap();

    // 리다이렉트 체인
    assert_eq!(presence(&r, "transport.redirect"), Presence::Present);
    assert_eq!(v("transport.redirect")[0]["status"], 307);
    assert_eq!(
        presence(&r, "transport.temporary_redirect"),
        Presence::Present
    );
    assert_eq!(
        presence(&r, "transport.host_variant_duplicate"),
        Presence::NotApplicable,
        "IP host has no www variant"
    );

    // 기본 /sitemap.xml 은 없지만 robots 선언 sitemap 으로 찾음
    assert_eq!(presence(&r, "crawling.sitemap_xml"), Presence::Missing);
    assert_eq!(presence(&r, "crawling.sitemap_any"), Presence::Present);
    assert_eq!(
        presence(&r, "crawling.robots_declared_sitemaps"),
        Presence::Missing,
        "gone.xml is 404"
    );
    assert_eq!(v("crawling.sitemap_entries")["entries"], 3);
    assert_eq!(presence(&r, "crawling.sitemap_lastmod"), Presence::Present);
    assert_eq!(
        presence(&r, "crawling.sitemap_foreign_host"),
        Presence::Present
    );
    assert_eq!(
        v("crawling.sitemap_foreign_host"),
        serde_json::json!(["elsewhere.invalid"])
    );
    assert_eq!(
        presence(&r, "crawling.sitemap_broken_urls"),
        Presence::Present
    );
    assert_eq!(
        v("crawling.sitemap_broken_urls")["broken"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    // llms.txt 형식
    assert_eq!(presence(&r, "geo.llms_txt"), Presence::Present);
    assert_eq!(v("geo.llms_txt_title"), "DevFive");
    assert_eq!(presence(&r, "geo.llms_txt_summary"), Presence::Present);
    assert_eq!(v("geo.llms_txt_links"), 1);
    assert_eq!(presence(&r, "geo.tdm_reservation"), Presence::Missing);

    // 도메인 일관성
    assert_eq!(
        presence(&r, "meta.canonical_host_mismatch"),
        Presence::Present
    );
    assert_eq!(
        presence(&r, "open_graph.url_host_mismatch"),
        Presence::Missing
    );

    // 사이트 파일 / 피드
    assert_eq!(presence(&r, "meta.favicon_ico_file"), Presence::Present);
    assert_eq!(
        presence(&r, "meta.apple_touch_icon_file"),
        Presence::Missing,
        "empty body"
    );
    assert_eq!(
        r.get("meta.apple_touch_icon_file").unwrap().note.as_deref(),
        Some("빈 응답 (0 byte)")
    );
    assert_eq!(presence(&r, "transport.security_txt"), Presence::Present);
    assert_eq!(presence(&r, "crawling.feed_link"), Presence::Present);
    assert_eq!(presence(&r, "links.feed"), Presence::Present);

    // 이미지 속성
    assert_eq!(
        presence(&r, "performance.img_dimensions_missing"),
        Presence::Missing
    );
    assert_eq!(presence(&r, "performance.img_lazy"), Presence::Present);
}

/// 1200x630 PNG 헤더 (크기 판별에 필요한 부분만)
fn png_header(w: u32, h: u32) -> Vec<u8> {
    let mut b = vec![
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R',
    ];
    b.extend_from_slice(&w.to_be_bytes());
    b.extend_from_slice(&h.to_be_bytes());
    b.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    b
}

const B_PAGE: &str = r##"<!doctype html><html lang="ko"><head><title>t</title><title>dup</title>
<meta name="viewport" content="width=device-width, maximum-scale=1">
<meta property="og:locale" content="en_US"><meta property="og:image" content="/og.png">
<script src="/_next/static/app.js"></script>
</head><body><a href="#main">본문 바로가기</a><main id="main">
<h1>a</h1><h3>skip</h3>
<a href="/ok">ok</a><a href="/missing"><i></i></a>
<input name="q"><button></button></main></body></html>"##;

#[tokio::test]
async fn extended_network_checks() {
    let s = MockServer::start().await;
    let uri = s.uri();
    let html = |b: &str| {
        ResponseTemplate::new(200)
            .insert_header("content-type", "text/html")
            .set_body_string(b)
    };
    Mock::given(method("GET"))
        .and(path("/ko/"))
        .respond_with(html(B_PAGE).insert_header("x-content-type-options", "nosniff"))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/ko"))
        .respond_with(html(B_PAGE))
        .mount(&s)
        .await; // 슬래시 없는 버전도 200 → 중복
    Mock::given(method("GET"))
        .and(path("/ko/index.html"))
        .respond_with(ResponseTemplate::new(301).insert_header("location", "/ko/"))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/ok"))
        .respond_with(html("<p>ok</p>"))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/og.png"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "image/png")
                .set_body_bytes(png_header(1200, 630)),
        )
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("User-agent: *\nDisallow: /_next/\n"),
        )
        .mount(&s)
        .await;
    Mock::given(method("GET")).and(path("/sitemap.xml"))
        .respond_with(ResponseTemplate::new(200).insert_header("content-type", "application/xml").set_body_string(format!(
            "<urlset><url><loc>{uri}/hidden</loc></url><url><loc>{uri}/dup</loc></url></urlset>")))
        .mount(&s).await;
    Mock::given(method("GET"))
        .and(path("/hidden"))
        .respond_with(html(r#"<meta name="robots" content="noindex">"#))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/dup"))
        .respond_with(html(&format!(r#"<link rel="canonical" href="{uri}/ko/">"#)))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/ko/index.md"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/markdown")
                .set_body_string("# 홈"),
        )
        .mount(&s)
        .await;

    let r = Checker::new(Options {
        check_ai_user_agents: false,
        ..Default::default()
    })
    .check(&format!("{uri}/ko/"))
    .await
    .unwrap();
    let v = |id: &str| {
        r.get(id)
            .unwrap_or_else(|| panic!("{id}"))
            .value
            .clone()
            .unwrap()
    };

    // A: HTML
    assert_eq!(presence(&r, "meta.duplicate_tags"), Presence::Present);
    assert_eq!(v("meta.duplicate_tags")["title"], 2);
    assert_eq!(presence(&r, "meta.lang_locale_mismatch"), Presence::Present);
    assert_eq!(v("meta.framework"), serde_json::json!(["Next.js"]));
    assert_eq!(presence(&r, "content.heading_skip"), Presence::Present);
    assert_eq!(
        presence(&r, "performance.render_blocking_scripts"),
        Presence::Present
    );
    assert_eq!(
        presence(&r, "structured_data.required.organization"),
        Presence::NotApplicable
    );

    // B: 네트워크
    assert_eq!(
        presence(&r, "transport.url_variant.trailing_slash"),
        Presence::Present
    );
    assert_eq!(
        presence(&r, "transport.url_variant.uppercase"),
        Presence::Missing
    );
    assert_eq!(
        presence(&r, "transport.url_variant.index_html"),
        Presence::Missing,
        "redirects to canonical"
    );
    assert_eq!(
        presence(&r, "transport.tls_certificate"),
        Presence::NotApplicable
    );
    assert_eq!(presence(&r, "open_graph.image_file"), Presence::Present);
    assert_eq!(
        (
            v("open_graph.image_file")["width"].clone(),
            v("open_graph.image_file")["height"].clone()
        ),
        (1200.into(), 630.into())
    );
    assert_eq!(
        presence(&r, "content.broken_internal_links"),
        Presence::Present
    );
    assert_eq!(v("content.broken_internal_links")["sampled"], 2);
    assert_eq!(
        presence(&r, "crawling.robots_blocks_resources"),
        Presence::Present
    );
    assert_eq!(
        presence(&r, "crawling.sitemap_noindex_pages"),
        Presence::Present
    );
    assert_eq!(
        presence(&r, "crawling.sitemap_canonical_mismatch"),
        Presence::Present
    );
    assert_eq!(presence(&r, "geo.markdown_version"), Presence::Present);
    assert_eq!(
        presence(&r, "security.x_content_type_options"),
        Presence::Present
    );
    assert_eq!(presence(&r, "security.csp"), Presence::Missing);

    // 웹 접근성
    assert_eq!(presence(&r, "accessibility.skip_link"), Presence::Present);
    assert_eq!(
        presence(&r, "accessibility.landmark_main"),
        Presence::Present
    );
    assert_eq!(
        presence(&r, "accessibility.unlabeled_form_controls"),
        Presence::Present
    );
    assert_eq!(
        presence(&r, "accessibility.unnamed_buttons"),
        Presence::Present
    );
    assert_eq!(v("accessibility.empty_links")["empty"], 1);
    assert_eq!(
        presence(&r, "accessibility.zoom_disabled"),
        Presence::Present
    );
}

const C_PAGE: &str = r#"<!doctype html><html lang="ko"><head><title>t</title>
<link rel="manifest" href="/site.webmanifest"><link rel="icon" href="/icon.png">
<link rel="stylesheet" href="/app.css"><meta name="msvalidate.01" content="ABC">
</head><body><footer><a href="/privacy">개인정보처리방침</a><a href="tel:02-000-0000">전화</a>
<p>© 2020 Test</p></footer></body></html>"#;

#[tokio::test]
async fn files_headers_and_trust_checks() {
    let s = MockServer::start().await;
    let ok = |b: &str, ct: &str| {
        ResponseTemplate::new(200)
            .insert_header("content-type", ct)
            .set_body_string(b)
    };
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ok(C_PAGE, "text/html")
                .insert_header("set-cookie", "sid=1; Path=/; HttpOnly")
                .insert_header("x-powered-by", "Next.js")
                .insert_header("etag", "\"abc\""),
        )
        .mount(&s)
        .await;
    Mock::given(method("GET")).and(path("/site.webmanifest"))
        .respond_with(ok(r#"{"name":"Test","start_url":"/","display":"standalone","icons":[{"src":"/a.png","sizes":"192x192"},{"src":"/b.png","sizes":"512x512"}]}"#, "application/manifest+json"))
        .mount(&s).await;
    Mock::given(method("GET"))
        .and(path("/icon.png"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "image/png")
                .set_body_bytes(png_header(32, 32)),
        )
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/app.css"))
        .respond_with(
            ok("body{}", "text/css")
                .insert_header("cache-control", "public, max-age=31536000, immutable"),
        )
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/.well-known/assetlinks.json"))
        .respond_with(ok(
            r#"[{"relation":["delegate_permission/common.handle_all_urls"]}]"#,
            "application/json",
        ))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/ads.txt"))
        .respond_with(ok(
            "google.com, pub-0000, DIRECT, f08c47fec0942fa0\n",
            "text/plain",
        ))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ok(
            "User-agent: Yeti\nCrawl-delay: 5\nDisalow: /oops\n",
            "text/plain",
        ))
        .mount(&s)
        .await;

    let r = Checker::new(Options {
        check_ai_user_agents: false,
        ..Default::default()
    })
    .check(&s.uri())
    .await
    .unwrap();
    let v = |id: &str| {
        r.get(id)
            .unwrap_or_else(|| panic!("{id}"))
            .value
            .clone()
            .unwrap()
    };

    assert_eq!(presence(&r, "meta.manifest_content"), Presence::Present);
    assert_eq!(v("meta.manifest_content")["name"], "Test");
    assert_eq!(
        presence(&r, "meta.manifest_icons_192_512"),
        Presence::Present
    );
    assert_eq!(presence(&r, "meta.favicon_size"), Presence::Present);
    assert_eq!(v("meta.favicon_size")["width"], 32);
    assert_eq!(
        presence(&r, "meta.bing_site_verification"),
        Presence::Present
    );
    assert_eq!(presence(&r, "meta.assetlinks"), Presence::Present);
    assert_eq!(
        presence(&r, "meta.apple_app_site_association"),
        Presence::Missing
    );
    assert_eq!(presence(&r, "crawling.ads_txt"), Presence::Present);

    assert_eq!(
        presence(&r, "performance.static_asset_cache"),
        Presence::Present
    );
    assert_eq!(v("performance.static_asset_cache")["maxAge"], 31_536_000);
    assert_eq!(v("performance.static_asset_cache")["immutable"], true);
    assert_eq!(presence(&r, "security.x_powered_by"), Presence::Present);
    assert_eq!(
        presence(&r, "security.insecure_cookies"),
        Presence::Present,
        "sid lacks Secure and SameSite"
    );
    assert_eq!(presence(&r, "crawling.etag"), Presence::Present);

    assert_eq!(
        presence(&r, "crawling.robots_crawl_delay"),
        Presence::Present
    );
    assert_eq!(
        v("crawling.robots_invalid_lines"),
        serde_json::json!(["Disalow: /oops"])
    );
    assert_eq!(presence(&r, "crawling.robots_oversize"), Presence::Missing);

    assert_eq!(presence(&r, "trust.privacy_policy"), Presence::Present);
    assert_eq!(presence(&r, "trust.phone"), Presence::Present);
    assert_eq!(v("trust.copyright_year")["year"], 2020);
    assert_eq!(presence(&r, "trust.business_number"), Presence::Missing);

    let ids: std::collections::HashSet<_> = r.checks.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids.len(), r.checks.len(), "ids must be unique");
}

#[tokio::test]
async fn multi_page_sample() {
    let s = MockServer::start().await;
    let uri = s.uri();
    let html = |b: &str| {
        ResponseTemplate::new(200)
            .insert_header("content-type", "text/html")
            .set_body_string(b)
    };
    Mock::given(method("GET")).and(path("/"))
        .respond_with(html(r#"<html lang="ko"><head><title>홈</title><meta name="description" content="공통 설명"></head>
            <body><h1>홈</h1><a href="/extra">extra</a></body></html>"#))
        .mount(&s).await;
    Mock::given(method("GET")).and(path("/sitemap.xml"))
        .respond_with(ResponseTemplate::new(200).insert_header("content-type", "application/xml").set_body_string(format!(
            "<urlset><url><loc>{uri}/</loc></url><url><loc>{uri}/a</loc></url><url><loc>{uri}/b</loc></url><url><loc>{uri}/gone</loc></url></urlset>")))
        .mount(&s).await;
    Mock::given(method("GET")).and(path("/a"))
        .respond_with(html(r#"<html lang="ko"><head><title>홈</title><meta name="description" content="공통 설명"></head><body><h1>A</h1><img src="x.png"></body></html>"#))
        .mount(&s).await;
    Mock::given(method("GET")).and(path("/b"))
        .respond_with(html(r#"<html lang="en"><head><title>B</title></head><body><h1>B</h1><h1>B2</h1></body></html>"#))
        .mount(&s).await;
    Mock::given(method("GET")).and(path("/extra"))
        .respond_with(html(r#"<html lang="ko"><head><title>Extra</title><meta name="description" content="e"></head><body><p>no heading</p></body></html>"#))
        .mount(&s).await;

    let r = Checker::new(Options {
        check_ai_user_agents: false,
        check_transport: false,
        ..Default::default()
    })
    .check(&uri)
    .await
    .unwrap();
    let v = |id: &str| {
        r.get(id)
            .unwrap_or_else(|| panic!("{id}"))
            .value
            .clone()
            .unwrap()
    };

    let sampled = v("site.pages_sampled");
    assert_eq!(sampled["source"], "sitemap+internal_links");
    assert_eq!(
        sampled["requested"], 4,
        "/a, /b, /gone from sitemap (home excluded) + /extra from links"
    );
    assert_eq!(sampled["ok"], 3);
    assert_eq!(presence(&r, "site.failed_pages"), Presence::Present);
    assert_eq!(presence(&r, "site.duplicate_titles"), Presence::Present);
    assert_eq!(v("site.duplicate_titles")[0]["value"], "홈");
    assert_eq!(
        v("site.duplicate_titles")[0]["urls"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        presence(&r, "site.duplicate_descriptions"),
        Presence::Present
    );
    assert_eq!(
        v("site.missing_descriptions")["urls"],
        serde_json::json!([format!("{uri}/b")])
    );
    assert_eq!(
        v("site.missing_h1")["urls"],
        serde_json::json!([format!("{uri}/extra")])
    );
    assert_eq!(
        v("site.multiple_h1")["urls"],
        serde_json::json!([format!("{uri}/b")])
    );
    assert_eq!(v("site.img_alt_missing_total")["missingAlt"], 1);
    assert_eq!(v("site.lang_variants"), serde_json::json!(["en", "ko"]));
    assert_eq!(presence(&r, "site.noindex_pages"), Presence::Missing);
}

#[tokio::test]
async fn canonical_hreflang_css_and_weight() {
    let s = MockServer::start().await;
    let uri = s.uri();
    let html = |b: String| {
        ResponseTemplate::new(200)
            .insert_header("content-type", "text/html")
            .set_body_string(b)
    };
    Mock::given(method("GET")).and(path("/ko/"))
        .respond_with(html(format!(r#"<html lang="ko"><head><title>ko</title>
            <link rel="canonical" href="{uri}/canon/">
            <link rel="alternate" hreflang="ko" href="{uri}/ko/"><link rel="alternate" hreflang="en" href="{uri}/en/">
            <link rel="alternate" hreflang="ja" href="{uri}/ja/">
            <link rel="stylesheet" href="/a.css"><link rel="stylesheet" href="/missing.css">
            <script src="/app.js"></script></head><body><img src="/i.png" alt=""></body></html>"#)))
        .mount(&s).await;
    Mock::given(method("GET"))
        .and(path("/canon/"))
        .respond_with(html(
            r#"<meta name="robots" content="noindex"><link rel="canonical" href="/elsewhere">"#
                .into(),
        ))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/en/"))
        .respond_with(html(format!(
            r#"<link rel="alternate" hreflang="ko" href="{uri}/ko/">"#
        )))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/ja/"))
        .respond_with(html(r#"<p>no hreflang</p>"#.into()))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/a.css"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/css")
                .set_body_string(
                    "button:focus{outline:none}@font-face{font-family:X;src:url(x.woff2)}",
                ),
        )
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/app.js"))
        .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(1000)))
        .mount(&s)
        .await;
    Mock::given(method("HEAD"))
        .and(path("/i.png"))
        .respond_with(ResponseTemplate::new(200).insert_header("content-length", "2048"))
        .mount(&s)
        .await;

    let r = Checker::new(Options {
        check_ai_user_agents: false,
        check_transport: false,
        page_samples: 0,
        ..Default::default()
    })
    .check(&format!("{uri}/ko/"))
    .await
    .unwrap();
    let v = |id: &str| {
        r.get(id)
            .unwrap_or_else(|| panic!("{id}"))
            .value
            .clone()
            .unwrap()
    };

    assert_eq!(
        presence(&r, "meta.canonical_target_noindex"),
        Presence::Present
    );
    assert_eq!(
        presence(&r, "meta.canonical_target_mismatch"),
        Presence::Present,
        "target canonicals elsewhere"
    );

    assert_eq!(
        presence(&r, "structured_data.hreflang_reciprocal"),
        Presence::Missing,
        "ja does not link back"
    );
    let back: Vec<bool> = v("structured_data.hreflang_reciprocal")
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["linksBack"] == true)
        .collect();
    assert_eq!(back, vec![true, false]);

    assert_eq!(v("performance.external_css")["checked"], 1);
    assert_eq!(v("performance.external_css")["failed"], 1);
    assert_eq!(
        presence(&r, "accessibility.outline_removed"),
        Presence::Present
    );
    assert_eq!(v("accessibility.outline_removed")["external"], 1);
    assert_eq!(
        presence(&r, "performance.font_face_without_display"),
        Presence::Present
    );

    let w = v("performance.page_weight");
    assert_eq!(w["js"]["bytes"], 1000);
    assert_eq!(w["images"]["bytes"], 2048);
    // wiremock 은 HEAD 응답의 content-length 헤더를 그대로 돌려준다
}

#[tokio::test]
async fn follows_meta_refresh() {
    let s = MockServer::start().await;
    let html = |b: &str| {
        ResponseTemplate::new(200)
            .insert_header("content-type", "text/html")
            .set_body_string(b)
    };
    Mock::given(method("GET")).and(path("/"))
        .respond_with(html(r#"<html><head><meta http-equiv="refresh" content="0; URL=/portal/main"><title>정부</title></head><body></body></html>"#))
        .mount(&s).await;
    Mock::given(method("GET")).and(path("/portal/main"))
        .respond_with(html(r#"<html lang="ko"><head><title>메인</title><meta name="description" content="d"></head><body><h1>메인</h1></body></html>"#))
        .mount(&s).await;
    let r = Checker::new(Options {
        check_ai_user_agents: false,
        check_transport: false,
        page_samples: 0,
        ..Default::default()
    })
    .check(&s.uri())
    .await
    .unwrap();
    assert!(r.final_url.ends_with("/portal/main"));
    assert_eq!(
        presence(&r, "transport.meta_refresh_redirect"),
        Presence::Present
    );
    assert_eq!(
        r.get("meta.title").unwrap().value.as_ref().unwrap()["text"],
        "메인"
    );
    assert_eq!(presence(&r, "meta.description"), Presence::Present);
    assert!(r.blocked.is_none());
}

#[tokio::test]
async fn blocked_page_makes_page_checks_unknown() {
    let s = MockServer::start().await;
    Mock::given(method("GET")).and(path("/"))
        .respond_with(ResponseTemplate::new(403).insert_header("content-type", "text/html")
            .set_body_string("<html><head><title>Just a moment...</title></head><body>cf_chl_opt</body></html>"))
        .mount(&s).await;
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string("User-agent: *\nAllow: /\n"))
        .mount(&s)
        .await;
    let r = Checker::new(Options {
        check_ai_user_agents: false,
        check_transport: false,
        page_samples: 0,
        ..Default::default()
    })
    .check(&s.uri())
    .await
    .unwrap();
    assert!(r.blocked.as_deref().unwrap().contains("Cloudflare"));
    assert_eq!(presence(&r, "transport.bot_challenge"), Presence::Present);
    assert_eq!(presence(&r, "meta.title"), Presence::Unknown);
    assert_eq!(
        presence(&r, "meta.description"),
        Presence::Unknown,
        "not Missing: we never saw the real page"
    );
    assert_eq!(
        presence(&r, "accessibility.landmark_main"),
        Presence::Unknown
    );
    assert_eq!(
        presence(&r, "crawling.robots_txt"),
        Presence::Present,
        "site files stay valid"
    );
    assert_eq!(presence(&r, "transport.http_status"), Presence::Missing);
}

#[tokio::test]
async fn follows_js_redirect_and_sitemap_file_redirect() {
    let s = MockServer::start().await;
    let html = |b: &str| {
        ResponseTemplate::new(200)
            .insert_header("content-type", "text/html")
            .set_body_string(b)
    };
    Mock::given(method("GET")).and(path("/"))
        .respond_with(html(r#"<html><head><title>t</title></head><body></body><script>window.location.href = "index.jsp";</script></html>"#))
        .mount(&s).await;
    Mock::given(method("GET")).and(path("/index.jsp"))
        .respond_with(html(r#"<html lang="ko"><head><title>본 페이지</title></head><body><h1>본문</h1></body></html>"#))
        .mount(&s).await;
    // /sitemap.xml → 다른 XML 파일로 301 (Vercel 방식): 존재로 판정
    Mock::given(method("GET"))
        .and(path("/sitemap.xml"))
        .respond_with(ResponseTemplate::new(301).insert_header("location", "/crawled-sitemap.xml"))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/crawled-sitemap.xml"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/xml")
                .set_body_string("<urlset><url><loc>x</loc></url></urlset>"),
        )
        .mount(&s)
        .await;
    // /llms.txt → 홈으로 302 (SPA 폴백): 없음으로 판정
    Mock::given(method("GET"))
        .and(path("/llms.txt"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/"))
        .mount(&s)
        .await;

    let r = Checker::new(Options {
        check_ai_user_agents: false,
        check_transport: false,
        page_samples: 0,
        ..Default::default()
    })
    .check(&s.uri())
    .await
    .unwrap();
    assert!(r.final_url.ends_with("/index.jsp"));
    assert_eq!(presence(&r, "transport.js_redirect"), Presence::Present);
    assert_eq!(
        presence(&r, "transport.meta_refresh_redirect"),
        Presence::Missing
    );
    assert_eq!(presence(&r, "content.h1"), Presence::Present);
    assert_eq!(presence(&r, "crawling.sitemap_xml"), Presence::Present);
    assert_eq!(presence(&r, "geo.llms_txt"), Presence::Missing);
}

#[tokio::test]
async fn id_set_is_fixed_regardless_of_options() {
    let s = site().await;
    let ids = |r: &Report| r.checks.iter().map(|c| c.id.clone()).collect::<Vec<_>>();
    let catalog: Vec<String> = devup_access_checker::CATALOG
        .iter()
        .map(|(id, _, _)| (*id).to_string())
        .collect();
    let full = Checker::new(Options::default())
        .check(&s.uri())
        .await
        .unwrap();
    let off = Checker::new(Options {
        check_links: false,
        check_ai_user_agents: false,
        check_transport: false,
        check_resources: false,
        page_samples: 0,
        sitemap_samples: 0,
        ..Default::default()
    })
    .check(&s.uri())
    .await
    .unwrap();
    assert_eq!(ids(&full), catalog);
    assert_eq!(ids(&off), catalog);
    let skipped = off.get("links.canonical").unwrap();
    assert_eq!(skipped.presence, Presence::Unknown);
}

/// README 의 점검 항목 표와 카탈로그가 어긋나지 않게 한다.
#[test]
fn readme_lists_every_catalog_id() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../README.md");
    let Ok(readme) = std::fs::read_to_string(path) else {
        return; // 패키지로 배포된 소스에는 워크스페이스 README 가 없다
    };
    let listed: std::collections::BTreeSet<&str> = readme
        .lines()
        .filter_map(|l| l.strip_prefix("| `")?.split_once("` |").map(|(id, _)| id))
        .filter(|id| id.contains('.'))
        .collect();
    let catalog: std::collections::BTreeSet<&str> = devup_access_checker::CATALOG
        .iter()
        .map(|(id, _, _)| *id)
        .collect();
    assert_eq!(listed, catalog);
}

/// robots.txt 가 기본 경로 sitemap 을 선언했는데 그 파일이 없으면 "선언 sitemap 접근" 은 missing.
#[tokio::test]
async fn declared_default_sitemap_missing_is_reported() {
    let s = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/html")
                .set_body_string(PAGE),
        )
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            "User-agent: *\nDisallow: /*?sort=\n\nSitemap: {}/sitemap.xml\n",
            s.uri()
        )))
        .mount(&s)
        .await;
    let r = Checker::new(Options {
        check_transport: false,
        check_ai_user_agents: false,
        ..Default::default()
    })
    .check(&format!("{}/?sort=1", s.uri()))
    .await
    .unwrap();
    assert_eq!(
        presence(&r, "crawling.robots_declared_sitemaps"),
        Presence::Missing
    );
    // 쿼리까지 포함해 robots 규칙을 판정한다
    assert_eq!(
        presence(&r, "crawling.robots_allows.googlebot"),
        Presence::Missing
    );
}

/// 결과 항목명이 카탈로그와 같아야 한다 (분기마다 다른 항목명을 쓰지 않도록).
#[tokio::test]
async fn labels_match_catalog() {
    let s = site().await;
    for opts in [
        Options::default(),
        Options {
            sitemap_samples: 0,
            page_samples: 0,
            check_links: false,
            ..Default::default()
        },
    ] {
        let r = Checker::new(opts).check(&s.uri()).await.unwrap();
        for (c, (id, cat, label)) in r.checks.iter().zip(devup_access_checker::CATALOG) {
            assert_eq!(
                (c.id.as_str(), c.category, c.label.as_str()),
                (*id, *cat, *label)
            );
        }
    }
}

fn html_page(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/html")
        .set_body_string(body)
}

fn quick() -> Options {
    Options {
        check_transport: false,
        check_ai_user_agents: false,
        check_resources: false,
        ..Default::default()
    }
}

/// meta refresh 를 따라가지 않거나 중단하는 경우들.
#[tokio::test]
async fn client_redirect_edge_cases() {
    let s = MockServer::start().await;
    let long_text = "본문 ".repeat(150);
    for (p, body) in [
        // 지연이 길고 본문도 있어 리다이렉트로 보지 않음
        ("/slow", format!(r#"<html><head><meta http-equiv="refresh" content="30; url=/x"></head><body>{long_text}</body></html>"#)),
        // 해석할 수 없는 대상
        ("/bad", r#"<html><head><meta http-equiv="refresh" content="0; url=http://["></head><body></body></html>"#.to_string()),
        // 자기 자신
        ("/self", r#"<html><head><meta http-equiv="refresh" content="0; url=/self"></head><body></body></html>"#.to_string()),
        // 대상 요청 실패
        ("/dead", r#"<html><head><meta http-equiv="refresh" content="0; url=http://127.0.0.1:9/"></head><body></body></html>"#.to_string()),
    ] {
        Mock::given(method("GET")).and(path(p)).respond_with(html_page(&body)).mount(&s).await;
    }
    for p in ["/slow", "/bad", "/self", "/dead"] {
        let r = Checker::new(quick())
            .check(&format!("{}{p}", s.uri()))
            .await
            .unwrap();
        assert_eq!(
            presence(&r, "transport.meta_refresh_redirect"),
            Presence::Missing,
            "{p}"
        );
    }
}

/// sitemap index: gzip 하위 sitemap 은 풀어서 표본을 얻고, 읽지 못하면 이유를 남긴다.
#[tokio::test]
async fn sitemap_index_children() {
    use std::io::Write as _;
    let s = MockServer::start().await;
    let index = |child: &str| {
        format!(
            r#"<?xml version="1.0"?><sitemapindex><sitemap><loc>{child}</loc></sitemap></sitemapindex>"#
        )
    };
    let child = format!(
        r#"<?xml version="1.0"?><urlset><url><loc>{}/p</loc></url></urlset>"#,
        s.uri()
    );
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(child.as_bytes()).unwrap();
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(html_page(PAGE))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/p"))
        .respond_with(html_page(PAGE))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/c.xml.gz"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(gz.finish().unwrap()))
        .mount(&s)
        .await;
    let sitemap = Mock::given(method("GET"))
        .and(path("/sitemap.xml"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/xml")
                .set_body_string(index(&format!("{}/c.xml.gz", s.uri()))),
        )
        .mount_as_scoped(&s)
        .await;
    let r = Checker::new(quick()).check(&s.uri()).await.unwrap();
    assert_eq!(
        presence(&r, "crawling.sitemap_broken_urls"),
        Presence::Missing
    );
    drop(sitemap);

    // 하위 sitemap 이 없음 → 이유가 노트에 남는다
    let missing = Mock::given(method("GET"))
        .and(path("/sitemap.xml"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/xml")
                .set_body_string(index(&format!("{}/none.xml", s.uri()))),
        )
        .mount_as_scoped(&s)
        .await;
    let r = Checker::new(quick()).check(&s.uri()).await.unwrap();
    let c = r.get("crawling.sitemap_broken_urls").unwrap();
    assert_eq!(c.presence, Presence::Unknown);
    assert!(c.note.as_deref().unwrap().contains("읽기 실패"));
    drop(missing);

    // 하위 sitemap 이 하나도 없는 index
    Mock::given(method("GET"))
        .and(path("/sitemap.xml"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/xml")
                .set_body_string(r#"<?xml version="1.0"?><sitemapindex></sitemapindex>"#),
        )
        .mount(&s)
        .await;
    let r = Checker::new(quick()).check(&s.uri()).await.unwrap();
    assert_eq!(
        presence(&r, "crawling.sitemap_broken_urls"),
        Presence::Unknown
    );
}

/// 해석할 수 없는 og:image 주소는 요청하지 않고 판별 불가.
#[tokio::test]
async fn invalid_og_image_url() {
    let s = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(html_page(
            r#"<html><head><meta property="og:image" content="http://["></head><body></body></html>"#,
        ))
        .mount(&s)
        .await;
    let r = Checker::new(Options {
        check_transport: false,
        check_ai_user_agents: false,
        ..Default::default()
    })
    .check(&s.uri())
    .await
    .unwrap();
    assert_eq!(presence(&r, "open_graph.image_file"), Presence::Unknown);
}
