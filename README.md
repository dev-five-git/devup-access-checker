# devup-access-checker

URL 하나를 받아 SEO · GEO(생성형 검색) 구성 요소의 **존재 여부**를 점검하는 Rust 라이브러리와 CLI입니다.

이 도구는 점수나 등급을 매기지 않습니다. 각 항목에는 고정된 `id`, 존재 여부(`presence`), 원시 값(`value`)만 담기며, 점수화는 결과를 받아 쓰는 쪽에서 합니다.

## 설치

```sh
# CLI
cargo install devup-access-checker-cli

# 라이브러리
cargo add devup-access-checker
```

CLI 바이너리는 [GitHub Releases](https://github.com/dev-five-git/devup-access-checker/releases)에서도 받을 수 있습니다(Linux x86_64·aarch64, macOS x86_64·aarch64, Windows x86_64).

## CLI

```sh
devup-access-checker devfive.kr                 # 텍스트 출력
devup-access-checker devfive.kr -f json         # JSON
devup-access-checker a.com b.com --only-missing
```

| 옵션 | 설명 |
|---|---|
| `-f, --format text\|json` | 출력 형식 |
| `--only-missing` | 없는 항목만 출력 (text) |
| `--no-links` | favicon · og:image 등 링크 접근, og:image 크기, 내부 링크 표본 검증 생략 |
| `--no-ai-ua` | AI·검색 크롤러(Yeti·Bingbot) User-Agent 요청 생략 |
| `--no-transport` | 리다이렉트 체인 · HTTP→HTTPS · www/URL 변형 · TLS 인증서 · 압축 확인 생략 |
| `--no-resources` | 외부 CSS·JS·이미지 분석(초점 표시, font-display, 페이지 무게) 생략 |
| `--pages <수>` | 여러 페이지 표본 수 (기본 10, 0이면 생략) |
| `--timeout <초>` | 대상 페이지 timeout (기본 30) |
| `--indexnow-key <키>` | IndexNow 키 파일(`/{키}.txt`) 확인. 키 파일 이름은 비밀이라 지정해야만 확인합니다 |

URL을 여러 개 주면 동시에 점검하고, 결과는 입력 순서대로 출력합니다. 종료 코드: `0` 성공, `2` 하나 이상의 URL을 가져오지 못함.

옵션으로 생략한 항목도 결과에서 빠지지 않고 `unknown`으로 남습니다. 결과의 항목 구성은 옵션과 대상에 관계없이 항상 같습니다.

## 라이브러리

```rust
use devup_access_checker::{Checker, Options, Presence};

let mut opts = Options::default();
opts.check_ai_user_agents = false; // 필요한 옵션만 바꾼다

let report = Checker::new(opts).check("devfive.kr").await?;
if report.get("geo.llms_txt").map(|c| c.presence) == Some(Presence::Missing) { /* ... */ }
```

`report.checks`는 항상 [`CATALOG`](crates/devup-access-checker/src/catalog.rs)와 같은 id를 같은 순서로 담습니다. 점검을 실행하지 않고 전체 항목 목록(id, 분류, 항목명)이 필요하면 `devup_access_checker::CATALOG`를 쓰면 됩니다. 항목 id는 버전이 바뀌어도 의미가 유지됩니다.

공개 API는 `Checker`, `Options`, `Report`, `Check`, `Presence`, `Category`, `Error`, `CATALOG`, `normalize_url`입니다. 파싱·요청 모듈은 내부 구현이라 공개하지 않습니다. `Options`·`Report`·`Check`·`Category`·`Error`는 `#[non_exhaustive]`라서, 이후 버전에서 필드나 분류가 추가되어도 호환성이 깨지지 않습니다. `Options`는 `Options::default()`로 만든 뒤 필드를 바꿔 씁니다.

## 대상 페이지가 차단되거나 리다이렉트될 때

- **클라이언트 측 리다이렉트:** 지연 5초 이하의 meta refresh와, 본문이 거의 없는 페이지의 `location.href = "..."` · `location.replace("...")`는 검색엔진처럼 따라가서 최종 페이지를 분석합니다(최대 3회). 따라간 기록은 `transport.meta_refresh_redirect` · `transport.js_redirect`에 남습니다.
- **차단·오류 페이지:** 대상 페이지가 2xx가 아니거나 봇 차단 페이지(Cloudflare, Akamai, F5, Imperva 등)이면 리포트의 `blocked`에 사유가 들어갑니다. 이때 페이지 HTML·헤더에서 나온 항목은 모두 `unknown`이 됩니다. 실제 페이지를 보지 못했는데 "없음"으로 판정하지 않기 위해서입니다. robots.txt·sitemap 같은 사이트 파일 항목은 그대로 판정합니다.
- **부속 요청의 차단:** robots.txt·llms.txt·표본 페이지 등이 봇 차단 페이지로 돌아오면 "없음"이 아니라 `unknown`입니다. AI 크롤러 User-Agent 요청이 차단 페이지를 받으면 `geo.ua_access.*`는 `missing`이고, 값의 `challenge`에 공급자가 남습니다.
- **해석 규칙:** 상대 링크는 `<base href>`가 있으면 그것을 기준으로 해석합니다. 문자 인코딩은 BOM → `Content-Type` charset → `<meta charset>` 순으로 정하므로 EUC-KR 페이지도 그대로 읽습니다. robots.txt 규칙은 경로와 쿼리를 함께 보고, 한글 경로는 퍼센트 인코딩해 비교합니다. sitemap index는 첫 하위 sitemap(`.xml.gz` 포함)의 페이지를 표본으로 씁니다.
- **요청 예절과 속도:** 같은 호스트에는 동시에 최대 8개 요청만 보내고, 연결 실패는 한 번 재시도합니다(timeout·DNS 실패는 재시도하지 않음). 대상 페이지 본문은 15MB까지만 받습니다. 서로 의존하지 않는 요청은 모두 동시에 보내며, `http://` 확인은 5초 안에 끝냅니다. `DEVUP_ACCESS_CHECKER_TRACE=1` 환경 변수를 주면 요청별 대기·소요 시간을 stderr로 출력합니다.

## Presence

| 값 | 의미 |
|---|---|
| `present` | 항목 이름이 가리키는 대상이 **있음** |
| `missing` | 없음 |
| `unknown` | 판별 불가 (요청 실패, CSR 셸 등) |
| `not_applicable` | 전제가 없어 검사 대상 아님 (예: 링크가 없어서 접근 검증 불가) |

`present`는 "좋다"는 뜻이 아니라 "있다"는 뜻입니다. 그래서 `indexing.meta_noindex`, `transport.mixed_content`, `content.img_alt_missing`처럼 부정적인 항목은 `present`가 곧 문제가 발견되었다는 의미입니다.

## 테스트

`cargo test --workspace`로 실행합니다. 모든 테스트는 로컬 mock 서버만 쓰고 외부 네트워크에 의존하지 않습니다. CI는 [cargo-tarpaulin](https://github.com/xd009642/tarpaulin)으로 라인 커버리지 100%를 확인하며, 미달이면 실패합니다.

```sh
cargo tarpaulin --workspace --engine llvm --fail-under 100
```

## 지원 Rust 버전

Rust 1.88 이상 (CI에서 확인).

## 배포

[changepacks](https://github.com/changepacks/changepacks)로 배포합니다. 변경 사항과 함께 `.changepacks/` 에 changepack 로그를 추가해 main 에 머지하면 CI가 버전을 올리고 crates.io 에 배포한 뒤, CLI 바이너리를 GitHub Release 에 올립니다. 저장소 시크릿 `CARGO_REGISTRY_TOKEN`이 필요합니다.

## 점검 기준 대조표

KWCAG 2.2 의 33개 검사항목, axe-core 규칙, Lighthouse(SEO·모범 사례·성능), Google 검색 센터, OWASP 보안 헤더를 하나씩 대조했습니다. 기준마다 어떤 항목으로 확인하는지, 정적 HTML 로는 왜 확인할 수 없는지(렌더링·스크립트 실행·의미 판단 필요)는 [docs/coverage.md](docs/coverage.md)에 있습니다.

## 점검 항목

옵션과 상황에 따라 일부 항목이 빠질 수 있습니다.

- `crawling.sitemap_entries` · `sitemap_lastmod` · `sitemap_foreign_host` · `sitemap_broken_urls` · `sitemap_noindex_pages` · `sitemap_canonical_mismatch` · `sitemap_oversize` · `sitemap_over_10mb`는 sitemap(기본 경로 또는 robots.txt 선언)을 찾았을 때만 나옵니다. sitemap index이면 `sitemap_noindex_pages`와 `sitemap_canonical_mismatch`는 나오지 않습니다.
- `transport.host_variant_duplicate`는 도메인 대상(IP·localhost 제외)일 때만 나옵니다. `transport.url_variant.*`는 경로 모양에 따라 해당하는 변형만 나옵니다.
- `--no-links`를 쓰면 `links.*`, `open_graph.image_file`, `content.broken_internal_links`, `meta.favicon_size`가 빠지고 `meta.manifest_*`와 `crawling.feed_*`는 `not_applicable`이 됩니다.
- `performance.static_asset_cache`는 같은 호스트의 script·stylesheet가 있을 때만 나옵니다.
- `--no-ai-ua`를 쓰면 `geo.ua_access.*`와 `crawling.ua_access.*`가 빠집니다.
- `--pages 0`을 쓰면 `site.*`가 빠집니다. 표본은 sitemap(index이면 첫 하위 sitemap)에서 먼저 고르고, 부족하면 페이지 안 내부 링크로 채웁니다. 대상 페이지와 다른 호스트의 URL은 제외합니다.
- `--no-transport`를 쓰면 `transport.http_to_https_redirect` · `redirect` · `temporary_redirect` · `host_variant_duplicate` · `tls_certificate` · `url_variant.*`와 `performance.compression`이 빠지고, `transport.redirect_hops_excess` · `redirect_chain_robots_blocked`와 `not_found_page.redirects`는 `not_applicable`이 됩니다.
- `crawling.indexnow_key_file`은 `--indexnow-key`(`Options::indexnow_key`)를 주었을 때만 확인하고, 주지 않으면 `not_applicable`입니다.

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
| `indexing.snippet_controls` | 스니펫 제한 지시어 (nosnippet·max-snippet 등) |
| `indexing.naver_directives` | 네이버 전용 지시어 (nosourceinfo·nosublinks·nosublink) |
| `indexing.bingbot_meta` | Bing 전용 robots 지시 (meta bingbot·X-Robots-Tag bingbot:) |
| `indexing.noindex_blocked_by_robots` | robots.txt 로 막혀 noindex 를 볼 수 없음 |
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
| `content.csr_shell` | CSR 셸 추정 (정적 HTML 본문이 거의 없음). present 면 본문·접근성 항목이 실제 화면과 다를 수 있음 |
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
| `crawling.robots_allows.applebot` | Apple 검색(Siri·Spotlight) 크롤러 허용 (Applebot) |
| `geo.llms_txt` | llms.txt |
| `geo.llms_full_txt` | llms-full.txt |
| `geo.ai_txt` | ai.txt |
| `geo.llms_txt_title` | llms.txt # 제목 |
| `geo.llms_txt_summary` | llms.txt > 요약 |
| `geo.llms_txt_sections` | llms.txt ## 섹션 |
| `geo.llms_txt_links` | llms.txt 링크 목록 |
| `geo.tdm_reservation` | TDM 예약 선언 (tdmrep.json / 헤더 / 메타) |
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
| `geo.robots_allows.google_cloudvertexbot` | Vertex AI 에이전트 크롤러 허용 (Google-CloudVertexBot) |
| `geo.robots_allows.googleother` | Google 범용(연구) 크롤러 허용 (GoogleOther) |
| `geo.robots_allows.meta_webindexer` | Meta AI 검색 인용 크롤러 허용 (meta-webindexer) |
| `geo.robots_allows.oai_adsbot` | ChatGPT 광고 검증 크롤러 허용 (OAI-AdsBot) |
| `geo.robots_allows.amzn_searchbot` | Amazon 검색 크롤러 허용 (Amzn-SearchBot) |
| `geo.robots_allows.amzn_user` | Amazon 사용자 요청 크롤러 허용 (Amzn-User) |
| `geo.robots_allows.mistralai_user` | Mistral 사용자 요청 크롤러 허용 (MistralAI-User) |
| `geo.robots_allows.mistralai_index` | Mistral 검색 색인 크롤러 허용 (MistralAI-Index) |
| `geo.robots_allows.mistralai_training` | Mistral 학습 크롤러 허용 (MistralAI-Training) |
| `geo.robots_allows.duckassistbot` | DuckDuckGo AI 답변 크롤러 허용 (DuckAssistBot) |
| `geo.robots_allows.ai2bot` | Ai2 학습 크롤러 허용 (AI2Bot) |
| `geo.robots_allows.webzio_extended` | Webz.io AI 학습 데이터 크롤러 허용 (Webzio-extended) |
| `geo.ua_access.gptbot` | AI UA 응답 (gptbot) |
| `geo.ua_access.claudebot` | AI UA 응답 (claudebot) |
| `geo.ua_access.perplexitybot` | AI UA 응답 (perplexitybot) |
| `geo.meta_noai` | meta noai / noimageai 지시 |
| `not_found_page.returns_404` | 없는 경로에 404 응답 |
| `not_found_page.custom` | 커스텀 404 페이지 |
| `not_found_page.third_party_calls` | 404 페이지의 외부 호출 |
| `not_found_page.redirects` | 없는 경로가 리다이렉트됨 |
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
| `open_graph.image_type` | og:image:type |
| `open_graph.image_secure_url` | og:image:secure_url |
| `open_graph.locale_alternate` | og:locale:alternate |
| `open_graph.video` | og:video |
| `open_graph.audio` | og:audio |
| `open_graph.determiner` | og:determiner |
| `open_graph.type_unknown` | 명세에 없는 og:type 값 |
| `open_graph.type_properties` | og:type 별 속성 (article:published_time 등) |
| `open_graph.image_socialonly` | og:image socialonly (Bing 뉴스) |
| `twitter.site` | twitter:site |
| `twitter.creator` | twitter:creator |
| `twitter.image_alt` | twitter:image:alt |
| `twitter.player` | twitter:player |
| `twitter.app` | twitter:app:* (앱 카드) |
| `twitter.card_unknown` | 명세에 없는 twitter:card 값 |
| `twitter.domain` | twitter:domain |
| `structured_data.hreflang_x_default` | hreflang x-default |
| `structured_data.hreflang_self` | hreflang 자기 참조 |
| `structured_data.required.organization` | Organization 스키마 필수 속성 |
| `structured_data.required.website` | WebSite 스키마 필수 속성 |
| `structured_data.required.article` | Article 스키마 필수 속성 |
| `structured_data.required.product` | Product 스키마 필수 속성 |
| `structured_data.required.breadcrumb_list` | BreadcrumbList 스키마 필수 속성 |
| `structured_data.required.faq_page` | FAQPage 스키마 필수 속성 |
| `structured_data.required.local_business` | LocalBusiness 스키마 필수 속성 |
| `structured_data.required.book` | Book 스키마 필수 속성 |
| `structured_data.required.course` | Course 스키마 필수 속성 |
| `structured_data.required.dataset` | Dataset 스키마 필수 속성 |
| `structured_data.required.discussion_forum` | DiscussionForumPosting 스키마 필수 속성 |
| `structured_data.required.education_qa` | Quiz 스키마 필수 속성 |
| `structured_data.required.employer_rating` | EmployerAggregateRating 스키마 필수 속성 |
| `structured_data.required.event` | Event 스키마 필수 속성 |
| `structured_data.required.claim_review` | ClaimReview 스키마 필수 속성 |
| `structured_data.required.image_license` | ImageObject 스키마 필수 속성 |
| `structured_data.required.job_posting` | JobPosting 스키마 필수 속성 |
| `structured_data.required.loyalty_program` | MemberProgram 스키마 필수 속성 |
| `structured_data.required.math_solver` | MathSolver 스키마 필수 속성 |
| `structured_data.required.movie` | Movie 스키마 필수 속성 |
| `structured_data.required.product_group` | ProductGroup 스키마 필수 속성 |
| `structured_data.required.profile_page` | ProfilePage 스키마 필수 속성 |
| `structured_data.required.qa_page` | QAPage 스키마 필수 속성 |
| `structured_data.required.recipe` | Recipe 스키마 필수 속성 |
| `structured_data.required.software_app` | SoftwareApplication 스키마 필수 속성 |
| `structured_data.required.vacation_rental` | VacationRental 스키마 필수 속성 |
| `structured_data.required.video` | VideoObject 스키마 필수 속성 |
| `structured_data.required.item_list` | ItemList 스키마 필수 속성 |
| `structured_data.required.merchant_return_policy` | MerchantReturnPolicy 스키마 필수 속성 |
| `structured_data.required.person` | Person 스키마 필수 속성 |
| `content.heading_skip` | 헤딩 레벨 건너뜀 |
| `content.internal_nofollow` | 내부 링크 nofollow |
| `content.broken_internal_links` | 내부 링크 표본 중 비정상 응답 |
| `content.uncrawlable_links` | 검색엔진이 따라갈 수 없는 링크 (href 없음·javascript:) |
| `content.plugins` | 플러그인 콘텐츠 (Flash·Java 등) |
| `content.fragment_route_links` | 해시(#!·#/) 라우트 링크 |
| `content.pseudo_js_links` | 의사 자바스크립트 링크 (href="fn()"·href="#"+onclick) |
| `content.img_generic_filenames` | 의미 없는 이미지 파일명 (IMG_1234·숫자·해시) |
| `content.parameter_heavy_links` | 파라미터가 많은 내부 링크 (3개 이상) |
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
| `accessibility.obsolete_elements` | 폐기된 HTML 요소 (marquee·font·center 등, WHATWG 폐기 목록) |
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
| `accessibility.invalid_aria_role` | ARIA 명세에 없는 role |
| `accessibility.invalid_aria_attr` | ARIA 명세에 없는 aria-* 속성 |
| `accessibility.role_img_unnamed` | 이름 없는 role="img" |
| `accessibility.video_captions_missing` | 자막 트랙 없는 video |
| `accessibility.area_alt_missing` | alt 없는 이미지맵 area |
| `accessibility.object_alt_missing` | 대체 텍스트 없는 object/embed |
| `accessibility.element_lang_invalid` | 본문 요소의 잘못된 lang 값 |
| `accessibility.nested_interactive` | 링크·버튼 안의 인터랙티브 요소 |
| `accessibility.list_structure_invalid` | 잘못된 목록 구조 (ul/ol 밖 li 등) |
| `accessibility.aria_hidden_root` | html/body 에 aria-hidden="true" |
| `accessibility.timed_refresh` | 시간 제한 자동 새로고침 (meta refresh 지연) |
| `accessibility.duplicate_accesskeys` | 중복 accesskey |
| `accessibility.presentation_focusable` | 초점을 받는 요소의 role=presentation/none |
| `accessibility.custom_control_unfocusable` | 초점을 받을 수 없는 커스텀 컨트롤 (role=button 등에 tabindex 없음) |
| `accessibility.custom_control_unnamed` | 이름 없는 커스텀 컨트롤 |
| `accessibility.click_without_keyboard` | 키보드로 쓸 수 없는 onclick 요소 (div·span 등) |
| `accessibility.aria_required_attr_missing` | 역할에 필요한 ARIA 속성 누락 |
| `accessibility.dialog_unnamed` | 이름 없는 대화상자 (dialog) |
| `accessibility.fieldset_legend_missing` | legend 없는 fieldset |
| `accessibility.radio_group_ungrouped` | fieldset·radiogroup 으로 묶이지 않은 라디오 그룹 |
| `accessibility.autocomplete_invalid` | 잘못된 autocomplete 값 |
| `accessibility.empty_table_headers` | 비어 있는 표 제목 셀 (th) |
| `accessibility.scope_invalid` | 잘못된 scope 속성 |
| `accessibility.multiple_main` | main 랜드마크 중복 |
| `accessibility.unnamed_repeated_landmarks` | 이름 없이 반복되는 nav·aside 랜드마크 |
| `accessibility.redundant_alt` | 중복 표현이 든 alt ("이미지", "image of" 등) |
| `accessibility.empty_summary` | 비어 있는 details summary |
| `accessibility.page_title_generic` | 기본값 그대로인 페이지 제목 ("React App" 등) |
| `accessibility.markup_errors` | HTML 마크업 오류 (파서 오류) |
| `accessibility.aria_invalid_value` | 허용되지 않는 ARIA 속성 값 |
| `accessibility.aria_name_prohibited` | 이름을 붙일 수 없는 요소의 aria-label |
| `accessibility.aria_required_children` | 필수 자식 역할이 없는 ARIA 컨테이너 |
| `accessibility.aria_required_parent` | 필수 부모 역할이 없는 ARIA 요소 |
| `accessibility.aria_deprecated_role` | 폐기된 ARIA 역할 |
| `accessibility.status_widget_unnamed` | 이름 없는 progress·meter·tooltip |
| `accessibility.label_title_only` | title 속성만으로 이름이 붙은 입력 요소 |
| `accessibility.multiple_labels` | label 이 둘 이상 연결된 입력 요소 |
| `accessibility.label_name_mismatch` | 보이는 텍스트를 포함하지 않는 aria-label |
| `accessibility.landmark_not_top_level` | 다른 랜드마크 안에 든 main·banner·contentinfo |
| `accessibility.duplicate_banner` | banner(최상위 header) 중복 |
| `accessibility.duplicate_contentinfo` | contentinfo(최상위 footer) 중복 |
| `accessibility.content_outside_landmarks` | 랜드마크 밖의 본문 텍스트 |
| `accessibility.iframe_title_duplicate` | 같은 title 을 쓰는 iframe |
| `accessibility.html_xml_lang_mismatch` | html 의 lang 과 xml:lang 불일치 |
| `accessibility.server_side_image_map` | 서버 측 이미지맵 (img ismap) |
| `accessibility.definition_list_invalid` | 잘못된 정의 목록 구조 (dl·dt·dd) |
| `accessibility.table_caption_missing` | 제목(caption) 없는 데이터 표 |
| `accessibility.inline_spacing_important` | 글자·줄 간격을 !important 로 고정한 인라인 스타일 |
| `accessibility.paste_blocked` | 붙여넣기를 막은 입력 요소 |
| `accessibility.identical_link_text` | 같은 텍스트인데 목적지가 다른 링크 |
| `accessibility.orientation_lock` | 화면 방향 고정 (orientation 미디어 쿼리 + 회전) |
| `accessibility.viewport_fixed_width` | 고정 너비 viewport (리플로 불가) |
| `accessibility.site_search` | 사이트 검색 (search 요소·role=search·type=search) |
| `accessibility.breadcrumb_nav` | 이동 경로(브레드크럼) 표시 |
| `accessibility.abbr_without_title` | title 없는 abbr (약어 설명 없음) |
| `accessibility.onchange_navigation` | 값만 바꿔도 이동·제출하는 onchange |
| `accessibility.password_autocomplete_off` | 자동 완성을 끈 비밀번호 칸 |
| `accessibility.reduced_motion` | 움직임 줄이기 설정 대응 (prefers-reduced-motion) |
| `accessibility.justified_text` | 양쪽 정렬 텍스트 (text-align: justify) |
| `accessibility.obsolete_attributes` | 폐기된 HTML 속성 (align·bgcolor·border 등) |
| `accessibility.onfocus_navigation` | 초점만 받아도 이동·제출하는 onfocus |
| `accessibility.video_audio_description_missing` | 음성 해설 트랙 없는 영상 |
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
| `security.cross_origin_opener_policy` | Cross-Origin-Opener-Policy |
| `security.cross_origin_embedder_policy` | Cross-Origin-Embedder-Policy |
| `security.cross_origin_resource_policy` | Cross-Origin-Resource-Policy |
| `security.sri_missing` | SRI(integrity) 없는 외부 스크립트·스타일시트 |
| `security.insecure_form_action` | https 페이지에서 http 로 보내는 폼 |
| `security.target_blank_noopener` | rel=noopener 없는 새 창 링크 |
| `security.trusted_types` | Trusted Types (CSP require-trusted-types-for) |
| `security.x_dns_prefetch_control` | X-DNS-Prefetch-Control |
| `security.x_permitted_cross_domain_policies` | X-Permitted-Cross-Domain-Policies |
| `security.info_disclosure_headers` | 버전·내부 정보를 노출하는 헤더 (OWASP 제거 권장) |
| `security.deprecated_headers` | 폐기된 응답 헤더 (X-XSS-Protection·Expect-CT 등) |
| `performance.static_asset_cache` | 정적 리소스 Cache-Control |
| `performance.dom_size` | DOM 크기 |
| `performance.third_party_scripts` | 서드파티 스크립트 호스트 |
| `crawling.ads_txt` | ads.txt |
| `crawling.robots_oversize` | robots.txt 500KiB 초과 |
| `crawling.robots_crawl_delay` | robots.txt Crawl-delay |
| `crawling.robots_invalid_lines` | robots.txt 해석 불가 줄 |
| `crawling.sitemap_oversize` | sitemap 한도 초과 (50MB 또는 5만 URL) |
| `crawling.script_cache_busting` | JS URL 의 타임스탬프·해시 쿼리 |
| `crawling.robots_blocks_resources_yeti` | robots.txt 가 CSS/JS 차단 (Yeti) |
| `crawling.robots_blocks_resources_bingbot` | robots.txt 가 CSS/JS 차단 (Bingbot) |
| `crawling.robots_blocks_favicon` | robots.txt 가 favicon 차단 |
| `crawling.sitemap_over_10mb` | sitemap 10MB 이상 (네이버 제출 한도) |
| `crawling.ua_access.yeti` | 검색 크롤러 UA 응답 (yeti) |
| `crawling.ua_access.bingbot` | 검색 크롤러 UA 응답 (bingbot) |
| `crawling.feed_valid` | 피드 형식 유효 (RSS·Atom) |
| `crawling.feed_items` | 피드 item 수 |
| `crawling.feed_foreign_host` | 피드 안 다른 호스트·상대 경로 URL |
| `crawling.feed_oversize` | 피드 10MB 이상 |
| `crawling.feed_full_content` | 피드 본문 전체 포함 |
| `crawling.robots_encoding` | robots.txt UTF-8 인코딩 |
| `crawling.sitemap_redirected_urls` | sitemap 표본 중 리다이렉트되는 URL |
| `crawling.indexnow_key_file` | IndexNow 키 파일 |
| `crawling.robots_content_type` | robots.txt text/plain |
| `crawling.robots_orphan_rules` | 그룹 밖 Allow/Disallow |
| `crawling.robots_unreachable` | robots.txt 5xx·네트워크 오류 (전체 차단으로 간주) |
| `crawling.robots_redirect_hops` | robots.txt 리다이렉트 5회 초과 |
| `crawling.sitemap_namespace` | sitemap 네임스페이스 |
| `crawling.sitemap_invalid_priority` | sitemap priority 범위 밖 |
| `crawling.sitemap_invalid_changefreq` | sitemap changefreq 허용값 밖 |
| `crawling.sitemap_invalid_lastmod` | sitemap lastmod W3C Datetime 형식 오류 |
| `crawling.sitemap_unescaped_entities` | sitemap 엔티티 미이스케이프 (&) |
| `crawling.sitemap_invalid_loc` | sitemap loc 형식 오류 |
| `crawling.sitemap_out_of_scope` | sitemap 위치 밖 URL |
| `crawling.sitemap_encoding` | sitemap UTF-8 |
| `geo.question_headings` | 질문형 헤딩 (h2~h4) |
| `geo.lists` | 목록 (항목 2개 이상 ul/ol, dl) |
| `geo.tables` | 표 |
| `geo.noscript_content` | noscript 대체 콘텐츠 |
| `geo.ard_manifest` | ARD 매니페스트 (AI 에이전트용 리소스 목록) |
| `geo.ai_catalog_legacy` | ARD 이전 위치 (ai-catalog) |
| `geo.webmcp_forms` | WebMCP 선언형 주석이 있는 폼 (toolname) |
| `geo.webmcp_script` | WebMCP 명령형 API 사용 흔적 (navigator.modelContext) |
| `geo.copilot_restrictions` | Copilot 인용 제한 (noarchive·nocache) |
| `geo.bing_ai_preferences` | Bing AI 학습·그라운딩 선호 표시 |
| `geo.llms_txt_link_format` | llms.txt 링크 항목 형식 오류 |
| `geo.llms_txt_extra_headings` | llms.txt 규격 밖 헤딩 |
| `geo.llms_txt_optional_section` | llms.txt Optional 섹션 |
| `geo.llms_txt_describedby` | llms.txt 연결 (rel=describedby) |
| `geo.llms_txt_subpath` | 하위 경로 llms.txt |
| `geo.content_signals` | Content-Signal (robots.txt) |
| `geo.content_usage` | AI 사용 선호 (IETF aipref Content-Usage) |
| `geo.rsl_license` | RSL 라이선스 선언 |
| `geo.tdm_policy` | TDM 정책 URL (tdm-policy) |
| `geo.tdm_reservation_invalid` | TDM 예약 값 오류 |
| `geo.a2a_agent_card` | A2A 에이전트 카드 |
| `geo.quotations` | 인용문 (blockquote·q) |
| `geo.statistics` | 수치·통계 표기 |
| `structured_data.speakable` | speakable 속성 |
| `meta.canonical_target_noindex` | canonical 대상 페이지 noindex |
| `meta.canonical_target_mismatch` | canonical 대상이 2xx 아님 또는 다른 canonical |
| `meta.doctype` | HTML5 doctype (<!DOCTYPE html>) |
| `meta.manifest_id` | manifest id |
| `meta.manifest_scope` | manifest scope |
| `meta.manifest_description` | manifest description |
| `meta.manifest_screenshots` | manifest screenshots |
| `meta.manifest_shortcuts` | manifest shortcuts |
| `meta.manifest_maskable_icon` | manifest maskable 아이콘 |
| `meta.manifest_colors` | manifest theme_color·background_color |
| `meta.manifest_display_valid` | manifest display 값 (fullscreen·standalone·minimal-ui·browser) |
| `meta.canonical_relative` | canonical 상대 경로 |
| `meta.favicon_relative_href` | favicon 상대 경로 |
| `meta.favicon_duplicate_rel` | 같은 rel 의 favicon 중복 |
| `meta.charset_mismatch` | 헤더·meta charset 불일치 |
| `meta.head_elements_in_body` | body 안에 있는 head 전용 요소 |
| `meta.description_same_as_title` | description 이 title 과 동일 |
| `meta.repeated_terms` | title·description 반복 단어 |
| `meta.mobile_site_canonical` | 모바일 사이트 → 데스크톱 canonical |
| `meta.bing_site_auth_file` | Bing 사이트 인증 파일 (/BingSiteAuth.xml) |
| `structured_data.hreflang_reciprocal` | hreflang 상호 참조 |
| `structured_data.unknown_types` | schema.org 에 없는 @type |
| `structured_data.rdfa` | RDFa |
| `structured_data.microformats` | Microformats (hCard·hRecipe·hReview 등) |
| `structured_data.naver.postal_address` | 네이버 주소 필수 속성 |
| `structured_data.naver.breadcrumb_list` | 네이버 이동 경로 필수 속성 |
| `structured_data.naver.carousel` | 네이버 캐러셀 필수 속성 |
| `structured_data.naver.channel` | 네이버 연관 채널 필수 속성 |
| `structured_data.naver.movie` | 네이버 영화 필수 속성 |
| `structured_data.naver.aggregate_rating` | 네이버 평점 필수 속성 |
| `structured_data.naver.recipe` | 네이버 레시피 필수 속성 |
| `structured_data.naver.restaurant` | 네이버 식당 필수 속성 |
| `structured_data.naver.review` | 네이버 리뷰 필수 속성 |
| `structured_data.naver.software_app` | 네이버 소프트웨어 필수 속성 |
| `structured_data.naver.tv_series` | 네이버 TV 시리즈 필수 속성 |
| `structured_data.naver.video` | 네이버 동영상 필수 속성 |
| `structured_data.naver.video_channel` | 네이버 동영상 채널 필수 속성 |
| `performance.external_css` | 외부 CSS 분석 |
| `performance.font_face_without_display` | font-display 없는 @font-face |
| `performance.page_weight` | 페이지 리소스 무게 |
| `performance.lcp_image_lazy` | 첫 이미지(LCP 후보) 지연 로딩 |
| `performance.fetchpriority_high` | fetchpriority="high" 지정 |
| `performance.document_write` | 인라인 스크립트의 document.write |
| `performance.css_import` | CSS @import |
| `performance.duplicate_scripts` | 중복으로 넣은 스크립트 |
| `performance.inline_code_bytes` | 인라인 스크립트·스타일 크기 |
| `performance.bfcache_blocked` | bfcache 차단 요인 (no-store, unload) |
| `performance.large_scripts` | 큰 스크립트 파일 (500KB 초과) |
| `performance.large_images` | 큰 이미지 파일 (500KB 초과) |
| `performance.unminified_css` | 압축(minify)되지 않은 CSS |
| `performance.large_gifs` | 큰 GIF 이미지 (100KB 초과) |
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
| `transport.security_txt_fields` | security.txt 필수 필드 (Contact·Expires) |
| `transport.hostname_invalid` | 표준에 어긋난 호스트명 (밑줄 등) |
| `transport.hosting_subdomain` | 호스팅 제공 기본 도메인 (github.io 등) |
| `transport.redirect_hops_excess` | 리다이렉트 5회 초과·루프 |
| `transport.redirect_chain_robots_blocked` | 리다이렉트 경유 URL robots 차단 |
