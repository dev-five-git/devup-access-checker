# devup-access-checker

URL 하나를 받아 SEO · GEO(생성형 검색) 구성 요소의 **존재 여부**를 점검하는 Rust 라이브러리와 CLI입니다.

이 도구는 점수나 등급을 매기지 않습니다. 각 항목에는 고정된 `id`, 존재 여부(`presence`), 원시 값(`value`)만 담기며, 점수화는 결과를 받아 쓰는 쪽에서 합니다.

## CLI

```sh
cargo run -p devup-access-checker-cli -- devfive.kr                 # 텍스트 출력
cargo run -p devup-access-checker-cli -- devfive.kr -f json         # JSON
cargo run -p devup-access-checker-cli -- a.com b.com --only-missing
```

| 옵션 | 설명 |
|---|---|
| `-f, --format text\|json` | 출력 형식 |
| `--only-missing` | 없는 항목만 출력 (text) |
| `--no-links` | favicon · og:image 등 링크 접근, og:image 크기, 내부 링크 표본 검증 생략 |
| `--no-ai-ua` | AI 크롤러 User-Agent 요청 생략 |
| `--no-transport` | 리다이렉트 체인 · HTTP→HTTPS · www/URL 변형 · TLS 인증서 · 압축 확인 생략 |
| `--no-resources` | 외부 CSS·JS·이미지 분석(초점 표시, font-display, 페이지 무게) 생략 |
| `--pages <수>` | 여러 페이지 표본 수 (기본 10, 0이면 생략) |
| `--timeout <초>` | 대상 페이지 timeout (기본 30) |

종료 코드: `0` 성공, `2` 하나 이상의 URL을 가져오지 못함.

## 라이브러리

```rust
use devup_access_checker::{Checker, Presence};

let report = Checker::default().check("devfive.kr").await?;
if report.get("geo.llms_txt").map(|c| c.presence) == Some(Presence::Missing) { /* ... */ }
```

`html::extract`, `robots::Robots`, `sitemap::summarize`, `signals::*`는 네트워크 없이 단독으로도 쓸 수 있습니다.

## 대상 페이지가 차단되거나 리다이렉트될 때

- **클라이언트 측 리다이렉트:** 지연 5초 이하의 meta refresh와, 본문이 거의 없는 페이지의 `location.href = "..."` · `location.replace("...")`는 검색엔진처럼 따라가서 최종 페이지를 분석합니다(최대 3회). 따라간 기록은 `transport.meta_refresh_redirect` · `transport.js_redirect`에 남습니다.
- **차단·오류 페이지:** 대상 페이지가 2xx가 아니거나 봇 차단 페이지(Cloudflare, Akamai, F5, Imperva 등)이면 리포트의 `blocked`에 사유가 들어갑니다. 이때 페이지 HTML·헤더에서 나온 항목은 모두 `unknown`이 됩니다. 실제 페이지를 보지 못했는데 "없음"으로 판정하지 않기 위해서입니다. robots.txt·sitemap 같은 사이트 파일 항목은 그대로 판정합니다.
- **부속 요청의 차단:** robots.txt·llms.txt·표본 페이지 등이 봇 차단 페이지로 돌아오면 "없음"이 아니라 `unknown`입니다. AI 크롤러 User-Agent 요청이 차단 페이지를 받으면 `geo.ua_access.*`는 `missing`이고, 값의 `challenge`에 공급자가 남습니다.
- **요청 예절:** 같은 호스트에는 동시에 최대 6개 요청만 보내고, timeout·연결 실패는 한 번 재시도합니다.

## Presence

| 값 | 의미 |
|---|---|
| `present` | 항목 이름이 가리키는 대상이 **있음** |
| `missing` | 없음 |
| `unknown` | 판별 불가 (요청 실패, CSR 셸 등) |
| `not_applicable` | 전제가 없어 검사 대상 아님 (예: 링크가 없어서 접근 검증 불가) |

`present`는 "좋다"는 뜻이 아니라 "있다"는 뜻입니다. 그래서 `indexing.meta_noindex`, `transport.mixed_content`, `content.img_alt_missing`처럼 부정적인 항목은 `present`가 곧 문제가 발견되었다는 의미입니다.

## 점검 항목

옵션과 상황에 따라 일부 항목이 빠질 수 있습니다.

- `crawling.sitemap_entries` · `sitemap_lastmod` · `sitemap_foreign_host` · `sitemap_broken_urls` · `sitemap_noindex_pages` · `sitemap_canonical_mismatch` · `sitemap_oversize`는 sitemap(기본 경로 또는 robots.txt 선언)을 찾았을 때만 나옵니다. sitemap index이면 `sitemap_noindex_pages`와 `sitemap_canonical_mismatch`는 나오지 않습니다.
- `transport.host_variant_duplicate`는 도메인 대상(IP·localhost 제외)일 때만 나옵니다. `transport.url_variant.*`는 경로 모양에 따라 해당하는 변형만 나옵니다.
- `--no-links`를 쓰면 `links.*`, `open_graph.image_file`, `content.broken_internal_links`, `meta.favicon_size`가 빠지고 `meta.manifest_*`는 `not_applicable`이 됩니다.
- `performance.static_asset_cache`는 같은 호스트의 script·stylesheet가 있을 때만 나옵니다.
- `--no-ai-ua`를 쓰면 `geo.ua_access.*`가 빠집니다.
- `--pages 0`을 쓰면 `site.*`가 빠집니다. 표본은 sitemap(index이면 첫 하위 sitemap)에서 먼저 고르고, 부족하면 페이지 안 내부 링크로 채웁니다. 대상 페이지와 다른 호스트의 URL은 제외합니다.
- `--no-transport`를 쓰면 `transport.http_to_https_redirect` · `redirect` · `temporary_redirect` · `host_variant_duplicate` · `tls_certificate` · `url_variant.*`와 `performance.compression`이 빠집니다.

웹 접근성(`accessibility.*`)은 정적 HTML만으로 판별할 수 있는 항목만 다룹니다. 색 대비, 키보드 초점 이동, 화면 낭독 순서처럼 렌더링이 필요한 항목은 포함하지 않습니다.

| id | 항목 |
|---|---|
| `meta.title` | title |
| `meta.description` | meta description |
| `meta.keywords` | meta keywords |
| `meta.viewport` | meta viewport |
| `meta.charset` | charset |
| `meta.html_lang` | html lang |
| `meta.canonical` | canonical link |
| `meta.theme_color` | theme-color |
| `meta.author` | meta author |
| `meta.favicon` | favicon |
| `meta.apple_touch_icon` | apple-touch-icon |
| `meta.manifest` | web app manifest |
| `meta.favicon_ico_file` | /favicon.ico 파일 |
| `meta.apple_touch_icon_file` | /apple-touch-icon.png 파일 |
| `meta.canonical_host_mismatch` | canonical 호스트 불일치 |
| `meta.google_site_verification` | Google Search Console 인증 |
| `meta.naver_site_verification` | 네이버 서치어드바이저 인증 |
| `indexing.meta_robots` | meta robots |
| `indexing.meta_noindex` | meta noindex 지시 |
| `indexing.meta_nofollow` | meta nofollow 지시 |
| `indexing.x_robots_tag` | X-Robots-Tag 헤더 |
| `indexing.header_noindex` | X-Robots-Tag noindex |
| `indexing.meta_refresh` | meta refresh 리다이렉트 |
| `open_graph.title` | og:title |
| `open_graph.description` | og:description |
| `open_graph.image` | og:image |
| `open_graph.url` | og:url |
| `open_graph.type` | og:type |
| `open_graph.site_name` | og:site_name |
| `open_graph.locale` | og:locale |
| `open_graph.url_host_mismatch` | og:url 호스트 불일치 |
| `twitter.card` | twitter:card |
| `twitter.title` | twitter:title |
| `twitter.description` | twitter:description |
| `twitter.image` | twitter:image |
| `facebook.app_id` | fb:app_id |
| `facebook.pages` | fb:pages |
| `facebook.article_publisher` | article:publisher |
| `structured_data.json_ld` | JSON-LD |
| `structured_data.json_ld_invalid` | JSON 파싱 불가 JSON-LD 블록 |
| `structured_data.type.organization` | Organization 스키마 |
| `structured_data.type.website` | WebSite 스키마 |
| `structured_data.type.breadcrumb_list` | BreadcrumbList 스키마 |
| `structured_data.type.faq_page` | FAQPage 스키마 |
| `structured_data.type.article` | Article 스키마 |
| `structured_data.type.product` | Product 스키마 |
| `structured_data.same_as` | sameAs (외부 프로필 연결) |
| `structured_data.microdata` | Microdata (itemscope) |
| `structured_data.hreflang` | hreflang 링크 |
| `content.h1` | h1 태그 |
| `content.h2` | h2 태그 |
| `content.img_alt_missing` | alt 누락 이미지 |
| `content.body_text` | 정적 HTML body 텍스트 |
| `content.internal_links` | 내부 링크 |
| `content.external_links` | 외부 링크 |
| `accessibility.cert_mark` | 정보통신접근성 품질인증 마크 |
| `analytics.google` | Google Analytics / GTM |
| `analytics.naver` | 네이버 애널리틱스 |
| `performance.font_preload` | font preload |
| `performance.image_preload` | image preload |
| `performance.img_dimensions_missing` | width/height 누락 이미지 |
| `performance.img_lazy` | loading=lazy 이미지 |
| `performance.response_time` | HTML 응답 시간(ms) |
| `performance.html_bytes` | HTML 크기(byte, 압축 해제) |
| `performance.compression` | 응답 압축 (Content-Encoding) |
| `performance.cache_control` | Cache-Control 헤더 |
| `transport.https` | HTTPS |
| `transport.http_status` | HTTP 2xx 응답 |
| `transport.http_to_https_redirect` | HTTP → HTTPS 리다이렉트 |
| `transport.redirect` | 요청 URL 리다이렉트 |
| `transport.temporary_redirect` | 임시 리다이렉트 (302/303/307) |
| `transport.host_variant_duplicate` | www/non-www 중복 노출 |
| `transport.security_txt` | /.well-known/security.txt |
| `transport.hsts` | HSTS 헤더 |
| `transport.mixed_content` | 혼합 콘텐츠 (https 페이지의 http 리소스) |
| `crawling.sitemap_xml` | sitemap.xml |
| `crawling.sitemap_any` | sitemap (기본 경로 또는 robots 선언) |
| `crawling.sitemap_entries` | sitemap 항목 수 |
| `crawling.sitemap_lastmod` | sitemap lastmod |
| `crawling.sitemap_foreign_host` | sitemap 내 다른 호스트 URL |
| `crawling.sitemap_broken_urls` | sitemap 샘플 URL 중 비정상 응답 |
| `crawling.sitemap_noindex_pages` | sitemap 표본 중 noindex 페이지 |
| `crawling.sitemap_canonical_mismatch` | sitemap 표본 중 canonical 이 다른 URL 인 페이지 |
| `crawling.robots_txt` | robots.txt |
| `crawling.robots_sitemap_directive` | robots.txt Sitemap 지시자 |
| `crawling.robots_declared_sitemaps` | robots.txt 선언 sitemap 접근 |
| `crawling.feed_link` | RSS/Atom 피드 링크 |
| `crawling.robots_allows.all` | robots.txt 기본(*) 허용 |
| `crawling.robots_allows.googlebot` | Google 크롤러 허용 (Googlebot) |
| `crawling.robots_allows.bingbot` | Bing 크롤러 허용 (Bingbot) |
| `crawling.robots_allows.yeti` | Naver 크롤러 허용 (Yeti) |
| `crawling.robots_allows.daumoa` | Daum 크롤러 허용 (Daumoa) |
| `geo.llms_txt` | llms.txt |
| `geo.llms_full_txt` | llms-full.txt |
| `geo.ai_txt` | ai.txt |
| `geo.llms_txt_title` | llms.txt # 제목 |
| `geo.llms_txt_summary` | llms.txt > 요약 |
| `geo.llms_txt_sections` | llms.txt ## 섹션 |
| `geo.llms_txt_links` | llms.txt 링크 목록 |
| `geo.tdm_reservation` | TDM 예약 (tdmrep.json / 헤더 / 메타) |
| `geo.robots_allows.gptbot` | OpenAI 학습 크롤러 허용 (GPTBot) |
| `geo.robots_allows.oai_searchbot` | ChatGPT 검색 크롤러 허용 (OAI-SearchBot) |
| `geo.robots_allows.chatgpt_user` | ChatGPT 사용자 요청 크롤러 허용 (ChatGPT-User) |
| `geo.robots_allows.claudebot` | Anthropic 학습 크롤러 허용 (ClaudeBot) |
| `geo.robots_allows.claude_searchbot` | Claude 검색 크롤러 허용 (Claude-SearchBot) |
| `geo.robots_allows.claude_user` | Claude 사용자 요청 크롤러 허용 (Claude-User) |
| `geo.robots_allows.perplexitybot` | Perplexity 검색 크롤러 허용 (PerplexityBot) |
| `geo.robots_allows.perplexity_user` | Perplexity 사용자 요청 크롤러 허용 (Perplexity-User) |
| `geo.robots_allows.google_extended` | Gemini 학습 크롤러 허용 (Google-Extended) |
| `geo.robots_allows.applebot_extended` | Apple AI 학습 크롤러 허용 (Applebot-Extended) |
| `geo.robots_allows.meta_externalagent` | Meta AI 크롤러 허용 (meta-externalagent) |
| `geo.robots_allows.amazonbot` | Amazon 크롤러 허용 (Amazonbot) |
| `geo.robots_allows.bytespider` | ByteDance 크롤러 허용 (Bytespider) |
| `geo.robots_allows.ccbot` | Common Crawl 크롤러 허용 (CCBot) |
| `geo.ua_access.gptbot` | AI UA 응답 (gptbot) |
| `geo.ua_access.claudebot` | AI UA 응답 (claudebot) |
| `geo.ua_access.perplexitybot` | AI UA 응답 (perplexitybot) |
| `geo.meta_noai` | meta noai / noimageai 지시 |
| `not_found_page.returns_404` | 없는 경로에 404 응답 |
| `not_found_page.custom` | 커스텀 404 페이지 |
| `links.favicon` | favicon 접근 |
| `links.apple_touch_icon` | apple-touch-icon 접근 |
| `links.canonical` | canonical 접근 |
| `links.og_image` | og:image 접근 |
| `links.twitter_image` | twitter:image 접근 |
| `links.manifest` | manifest 접근 |
| `links.feed` | 피드 접근 |
| `meta.duplicate_tags` | 중복 태그 (title·description·canonical·viewport·robots) |
| `meta.lang_locale_mismatch` | html lang ↔ og:locale 언어 불일치 |
| `meta.generator` | meta generator |
| `meta.framework` | 프레임워크 / CMS 흔적 |
| `open_graph.image_width` | og:image:width |
| `open_graph.image_height` | og:image:height |
| `open_graph.image_alt` | og:image:alt |
| `open_graph.image_file` | og:image 실제 파일 |
| `twitter.site` | twitter:site |
| `twitter.creator` | twitter:creator |
| `structured_data.hreflang_x_default` | hreflang x-default |
| `structured_data.hreflang_self` | hreflang 자기 참조 |
| `structured_data.required.organization` | Organization 스키마 필수 속성 |
| `structured_data.required.website` | WebSite 스키마 필수 속성 |
| `structured_data.required.article` | Article 스키마 필수 속성 |
| `structured_data.required.product` | Product 스키마 필수 속성 |
| `structured_data.required.breadcrumb_list` | BreadcrumbList 스키마 필수 속성 |
| `structured_data.required.faq_page` | FAQPage 스키마 필수 속성 |
| `structured_data.required.local_business` | LocalBusiness 스키마 필수 속성 |
| `content.heading_skip` | 헤딩 레벨 건너뜀 |
| `content.internal_nofollow` | 내부 링크 nofollow |
| `content.broken_internal_links` | 내부 링크 표본 중 비정상 응답 |
| `performance.render_blocking_scripts` | head 렌더링 차단 스크립트 (async/defer 없음) |
| `performance.head_stylesheets` | head 외부 스타일시트 |
| `performance.preconnect` | preconnect / dns-prefetch |
| `performance.modern_image_formats` | WebP / AVIF 이미지 |
| `performance.img_srcset` | srcset 이미지 |
| `transport.http2` | HTTP/2 이상 |
| `transport.http3_advertised` | HTTP/3 광고 (Alt-Svc h3) |
| `transport.tls_certificate` | TLS 인증서 |
| `transport.url_variant.trailing_slash` | 끝 슬래시 변형 중복 노출 |
| `transport.url_variant.uppercase` | 대문자 경로 변형 중복 노출 |
| `transport.url_variant.index_html` | index.html 변형 중복 노출 |
| `security.csp` | Content-Security-Policy |
| `security.x_content_type_options` | X-Content-Type-Options |
| `security.x_frame_options` | X-Frame-Options |
| `security.referrer_policy` | Referrer-Policy |
| `security.permissions_policy` | Permissions-Policy |
| `crawling.robots_blocks_resources` | robots.txt 가 CSS/JS 차단 |
| `geo.author` | 작성자 (meta / JSON-LD) |
| `geo.published_date` | 게시일 (article:published_time / datePublished) |
| `geo.modified_date` | 수정일 (article:modified_time / dateModified) |
| `geo.markdown_alternate` | 마크다운 대체 링크 (rel=alternate text/markdown) |
| `geo.markdown_version` | 페이지 .md 버전 |
| `accessibility.unlabeled_form_controls` | 레이블 없는 입력 요소 |
| `accessibility.unnamed_buttons` | 이름 없는 버튼 |
| `accessibility.empty_links` | 이름(텍스트) 없는 링크 |
| `accessibility.landmark_main` | main 랜드마크 |
| `accessibility.landmark_nav` | nav 랜드마크 |
| `accessibility.landmark_header` | header(banner) 랜드마크 |
| `accessibility.landmark_footer` | footer(contentinfo) 랜드마크 |
| `accessibility.skip_link` | 건너뛰기 링크 |
| `accessibility.iframe_title_missing` | title 없는 iframe |
| `accessibility.duplicate_ids` | 중복 id |
| `accessibility.table_headers_missing` | 제목 셀(th) 없는 표 |
| `accessibility.autoplay_media` | 자동 재생 미디어 |
| `accessibility.positive_tabindex` | 양수 tabindex |
| `accessibility.obsolete_elements` | 폐기된 표현 요소 (marquee·blink·font·center) |
| `accessibility.new_window_links` | 새 창 링크 (target=_blank) |
| `accessibility.zoom_disabled` | 확대 금지 viewport |
| `accessibility.vague_link_text` | 목적을 알 수 없는 링크 텍스트 |
| `accessibility.filename_alt` | 파일명 형태의 alt |
| `accessibility.long_alt` | 125자 초과 alt |
| `accessibility.broken_id_refs` | 없는 id 를 가리키는 참조 (for·aria-*) |
| `accessibility.focusable_aria_hidden` | aria-hidden 안의 초점 가능 요소 |
| `accessibility.empty_headings` | 빈 헤딩 |
| `accessibility.autocomplete_missing` | 개인정보 입력란 autocomplete 누락 |
| `accessibility.html_lang_valid` | html lang 형식 (BCP 47) |
| `accessibility.outline_removed` | 초점 표시 제거 (outline: none) |
| `trust.privacy_policy` | 개인정보처리방침 링크 |
| `trust.terms` | 이용약관 링크 |
| `trust.phone` | 전화번호 링크 (tel:) |
| `trust.email` | 이메일 링크 (mailto:) |
| `trust.address` | 주소 (<address> 또는 스키마 address) |
| `trust.social_links` | SNS 링크 |
| `trust.business_number` | 사업자등록번호 표기 |
| `trust.copyright_year` | 저작권 연도 표기 |
| `meta.bing_site_verification` | Bing 웹마스터 인증 |
| `meta.amphtml` | AMP 링크 (rel=amphtml) |
| `meta.app_link_meta` | 앱 연결 메타 (apple-itunes-app·al:*) |
| `meta.apple_app_site_association` | /.well-known/apple-app-site-association |
| `meta.assetlinks` | /.well-known/assetlinks.json |
| `meta.manifest_content` | manifest 내용 |
| `meta.manifest_icons_192_512` | manifest 192·512px 아이콘 |
| `meta.favicon_size` | favicon 실제 크기 |
| `crawling.etag` | ETag 헤더 |
| `crawling.last_modified` | Last-Modified 헤더 |
| `crawling.vary_user_agent` | Vary: User-Agent |
| `security.server_header` | Server 헤더 노출 |
| `security.x_powered_by` | X-Powered-By 헤더 노출 |
| `security.insecure_cookies` | Secure·HttpOnly·SameSite 누락 쿠키 |
| `performance.static_asset_cache` | 정적 리소스 Cache-Control |
| `performance.dom_size` | DOM 크기 |
| `performance.third_party_scripts` | 서드파티 스크립트 호스트 |
| `crawling.ads_txt` | ads.txt |
| `crawling.robots_oversize` | robots.txt 500KiB 초과 |
| `crawling.robots_crawl_delay` | robots.txt Crawl-delay |
| `crawling.robots_invalid_lines` | robots.txt 해석 불가 줄 |
| `crawling.sitemap_oversize` | sitemap 한도 초과 (50MB 또는 5만 URL) |
| `geo.question_headings` | 질문형 헤딩 (h2~h4) |
| `geo.lists` | 목록 (항목 2개 이상 ul/ol, dl) |
| `geo.tables` | 표 |
| `geo.noscript_content` | noscript 대체 콘텐츠 |
| `structured_data.speakable` | speakable 속성 |
| `meta.canonical_target_noindex` | canonical 대상 페이지 noindex |
| `meta.canonical_target_mismatch` | canonical 대상이 2xx 아님 또는 다른 canonical |
| `structured_data.hreflang_reciprocal` | hreflang 상호 참조 |
| `performance.external_css` | 외부 CSS 분석 |
| `performance.font_face_without_display` | font-display 없는 @font-face |
| `performance.page_weight` | 페이지 리소스 무게 |
| `site.pages_sampled` | 표본 페이지 수집 |
| `site.failed_pages` | 표본 중 2xx 아닌 페이지 |
| `site.duplicate_titles` | 페이지 간 중복 title |
| `site.duplicate_descriptions` | 페이지 간 중복 description |
| `site.missing_titles` | title 없는 페이지 |
| `site.missing_descriptions` | description 없는 페이지 |
| `site.missing_h1` | h1 없는 페이지 |
| `site.multiple_h1` | h1 이 2개 이상인 페이지 |
| `site.noindex_pages` | noindex 페이지 |
| `site.img_alt_missing_pages` | alt 누락 이미지가 있는 페이지 |
| `site.img_alt_missing_total` | 표본 전체 alt 누락 이미지 |
| `site.body_text_lengths` | 표본 페이지 정적 body 텍스트 길이 |
| `site.lang_variants` | 페이지마다 다른 html lang |
| `transport.bot_challenge` | 봇 차단·챌린지 페이지 |
| `transport.meta_refresh_redirect` | meta refresh 리다이렉트 (5초 이하, 따라감) |
| `transport.js_redirect` | JavaScript 리다이렉트 (빈 페이지의 location 이동, 따라감) |
| `transport.client_redirect_total` | 클라이언트 측 리다이렉트 수 |
