# 점검 기준 대조표

공신력 있는 점검 기준을 하나씩 대조해, 기준마다 어떤 항목 id 로 확인하는지 또는 왜 확인할 수 없는지를 적은 표입니다. 이 라이브러리는 헤드리스 브라우저와 외부 API 를 쓰지 않으므로, **렌더링 결과(계산된 색·크기·배치), 스크립트 실행, 사람의 의미 판단**이 필요한 기준은 확인할 수 없습니다. 그런 기준은 "확인 불가"와 이유를 적었습니다.

표의 항목 id 는 모두 [`CATALOG`](../crates/devup-access-checker/src/catalog.rs)에 있으며, 테스트(`coverage_doc_ids_exist`)가 이를 확인합니다.

axe-core 규칙과 Lighthouse 감사 목록은 `python scripts/check_coverage.py` 로 공식 저장소의 최신 목록과 다시 대조할 수 있습니다. 새 규칙이 생기면 대조표에 없는 이름을 출력합니다.

- [KWCAG 2.2 (33개 검사항목)](#kwcag-22)
- [axe-core 규칙](#axe-core-규칙)
- [Lighthouse](#lighthouse)
- [Google 검색 센터](#google-검색-센터)
- [Google 리치 결과 (구조화 데이터)](#google-리치-결과)
- [보안 헤더 (OWASP Secure Headers)](#보안-헤더)
- [생성형 검색(GEO)](#생성형-검색geo)
- [확인할 수 없는 것 요약](#확인할-수-없는-것-요약)

## KWCAG 2.2

한국형 웹 콘텐츠 접근성 지침 2.2 의 33개 검사항목입니다.

| 검사항목 | 항목 id | 확인할 수 없는 부분 |
|---|---|---|
| 1.1.1 적절한 대체 텍스트 제공 | `content.img_alt_missing` `accessibility.filename_alt` `accessibility.long_alt` `accessibility.redundant_alt` `accessibility.area_alt_missing` `accessibility.object_alt_missing` `accessibility.role_img_unnamed` `accessibility.unnamed_buttons` | 대체 텍스트가 이미지 내용과 맞는지 (의미 판단) |
| 1.2.1 자막 제공 | `accessibility.video_captions_missing` | 영상에 입힌 자막, 오디오 대본, 수어 |
| 1.3.1 표의 구성 | `accessibility.table_headers_missing` `accessibility.empty_table_headers` `accessibility.scope_invalid` `accessibility.table_caption_missing` `accessibility.broken_id_refs` | — |
| 1.3.2 콘텐츠의 선형구조 | `content.heading_skip` `accessibility.list_structure_invalid` `accessibility.definition_list_invalid` | CSS 배치로 바뀐 실제 읽기 순서 (렌더링) |
| 1.3.3 명확한 지시사항 제공 | — | 확인 불가: 색·모양·위치만으로 지시하는지는 의미 판단 |
| 1.4.1 색에 무관한 콘텐츠 인식 | — | 확인 불가: 계산된 색 필요 (렌더링) |
| 1.4.2 자동 재생 금지 | `accessibility.autoplay_media` | 스크립트로 재생하는 소리 |
| 1.4.3 텍스트 콘텐츠의 명도 대비 | — | 확인 불가: 계산된 글자·배경 색 필요 (렌더링) |
| 1.4.4 콘텐츠 간의 구분 | — | 확인 불가: 테두리·간격 등 시각적 구분 (렌더링) |
| 2.1.1 키보드 사용 보장 | `accessibility.custom_control_unfocusable` `accessibility.click_without_keyboard` `accessibility.focusable_aria_hidden` `accessibility.nested_interactive` `accessibility.presentation_focusable` | 스크립트로 붙인 이벤트, 키보드 트랩 (실행 필요) |
| 2.1.2 초점 이동과 표시 | `accessibility.outline_removed` `accessibility.positive_tabindex` | 실제 초점 순서와 표시 모양 (렌더링) |
| 2.1.3 조작 가능 | — | 확인 불가: 컨트롤 크기 (렌더링) |
| 2.1.4 문자 단축키 | `accessibility.duplicate_accesskeys` | 스크립트로 구현한 단축키 |
| 2.2.1 응답시간 조절 | `accessibility.timed_refresh` | 스크립트 타이머·세션 만료 |
| 2.2.2 정지 기능 제공 | `accessibility.autoplay_media` `accessibility.obsolete_elements` (WHATWG 폐기 요소 전체) | 스크립트 자동 슬라이드 |
| 2.3.1 깜빡임과 번쩍임 사용 제한 | — | 확인 불가: 프레임 분석 필요 |
| 2.4.1 반복 영역 건너뛰기 | `accessibility.skip_link` `accessibility.landmark_main` `accessibility.landmark_nav` `accessibility.landmark_header` `accessibility.landmark_footer` | 건너뛰기 링크가 실제로 동작하는지 (실행) |
| 2.4.2 제목 제공 | `meta.title` `accessibility.page_title_generic` `accessibility.iframe_title_missing` `accessibility.iframe_title_duplicate` `accessibility.empty_headings` `content.h1` | 제목이 내용을 잘 설명하는지 (의미 판단) |
| 2.4.3 적절한 링크 텍스트 | `accessibility.empty_links` `accessibility.vague_link_text` `accessibility.identical_link_text` | 링크 목적이 문맥상 분명한지 (의미 판단) |
| 2.4.4 고정된 참조 위치 정보 | — | 해당 없음: 전자출판물 대상 |
| 2.5.1 단일 포인터 입력 지원 | — | 확인 불가: 스크립트 제스처 |
| 2.5.2 포인터 입력 취소 | — | 확인 불가: 스크립트 이벤트 |
| 2.5.3 레이블과 네임 | `accessibility.label_name_mismatch` | CSS 로 숨긴 텍스트 구분 |
| 2.5.4 동작기반 작동 | — | 확인 불가: 기기 동작 센서 스크립트 |
| 3.1.1 기본 언어 표시 | `meta.html_lang` `accessibility.html_lang_valid` `accessibility.element_lang_invalid` `accessibility.html_xml_lang_mismatch` | 실제 본문 언어와 맞는지 |
| 3.2.1 사용자 요구에 따른 실행 | `accessibility.new_window_links` `accessibility.timed_refresh` `transport.meta_refresh_redirect` | onchange 자동 이동 등 스크립트 |
| 3.2.2 찾기 쉬운 도움 정보 | — | 확인 불가: 페이지 간 배치 일관성 |
| 3.3.1 오류 정정 | — | 확인 불가: 제출 후 동작 (실행) |
| 3.3.2 레이블 제공 | `accessibility.unlabeled_form_controls` `accessibility.label_title_only` `accessibility.multiple_labels` `accessibility.fieldset_legend_missing` `accessibility.radio_group_ungrouped` | — |
| 3.3.3 접근 가능한 인증 | `accessibility.paste_blocked` `accessibility.autocomplete_invalid` | CAPTCHA 대체 수단 |
| 3.3.4 반복 입력 정보 | `accessibility.autocomplete_missing` `accessibility.autocomplete_invalid` | 단계 간 자동 채움 (실행) |
| 4.1.1 마크업 오류 방지 | `accessibility.markup_errors` `accessibility.duplicate_ids` | — |
| 4.1.2 웹 애플리케이션 접근성 준수 | `accessibility.invalid_aria_role` `accessibility.invalid_aria_attr` `accessibility.aria_invalid_value` `accessibility.aria_required_attr_missing` `accessibility.aria_required_children` `accessibility.aria_required_parent` `accessibility.aria_name_prohibited` `accessibility.aria_deprecated_role` `accessibility.custom_control_unnamed` `accessibility.dialog_unnamed` `accessibility.status_widget_unnamed` | 상태 변화가 보조기기에 전달되는지 (실행) |
| (정보통신접근성 품질인증) | `accessibility.cert_mark` | 인증 유효 여부 (발급기관 조회 필요) |

## axe-core 규칙

[axe-core](https://github.com/dequelabs/axe-core/blob/develop/doc/rule-descriptions.md) 4.x 규칙입니다.

| 규칙 | 항목 id / 확인 불가 이유 |
|---|---|
| area-alt | `accessibility.area_alt_missing` |
| aria-allowed-attr | 일부: `accessibility.invalid_aria_attr` (역할별 허용 속성 표는 미적용) |
| aria-allowed-role | 미적용: 요소별 허용 역할 표가 방대하고 오탐이 많음 |
| aria-command-name, aria-input-field-name, aria-toggle-field-name, aria-treeitem-name, aria-tab-name | `accessibility.custom_control_unnamed` |
| accesskeys | `accessibility.duplicate_accesskeys` |
| aria-deprecated-role | `accessibility.aria_deprecated_role` |
| aria-dialog-name | `accessibility.dialog_unnamed` |
| aria-hidden-body | `accessibility.aria_hidden_root` |
| aria-hidden-focus | `accessibility.focusable_aria_hidden` |
| aria-meter-name, aria-progressbar-name, aria-tooltip-name | `accessibility.status_widget_unnamed` |
| aria-prohibited-attr | `accessibility.aria_name_prohibited` |
| aria-required-attr | `accessibility.aria_required_attr_missing` |
| aria-required-children | `accessibility.aria_required_children` |
| aria-required-parent | `accessibility.aria_required_parent` |
| aria-roles | `accessibility.invalid_aria_role` |
| aria-valid-attr | `accessibility.invalid_aria_attr` |
| aria-valid-attr-value | `accessibility.aria_invalid_value` |
| autocomplete-valid | `accessibility.autocomplete_invalid` |
| avoid-inline-spacing | `accessibility.inline_spacing_important` |
| blink, marquee | `accessibility.obsolete_elements` |
| button-name, input-button-name, input-image-alt | `accessibility.unnamed_buttons` |
| bypass, skip-link | `accessibility.skip_link` |
| color-contrast, color-contrast-enhanced, link-in-text-block | 확인 불가: 계산된 색 (렌더링) |
| css-orientation-lock | `accessibility.orientation_lock` |
| definition-list, dlitem | `accessibility.definition_list_invalid` |
| document-title | `meta.title` `accessibility.page_title_generic` |
| duplicate-id, duplicate-id-aria | `accessibility.duplicate_ids` |
| empty-heading | `accessibility.empty_headings` |
| empty-table-header | `accessibility.empty_table_headers` |
| form-field-multiple-labels | `accessibility.multiple_labels` |
| frame-title | `accessibility.iframe_title_missing` |
| frame-title-unique | `accessibility.iframe_title_duplicate` |
| frame-focusable-content, scrollable-region-focusable, target-size | 확인 불가: 렌더링 크기·스크롤 영역 |
| heading-order | `content.heading_skip` |
| html-has-lang | `meta.html_lang` |
| html-lang-valid | `accessibility.html_lang_valid` |
| html-xml-lang-mismatch | `accessibility.html_xml_lang_mismatch` |
| identical-links-same-purpose | `accessibility.identical_link_text` |
| image-alt | `content.img_alt_missing` |
| image-redundant-alt | `accessibility.redundant_alt` (alt 안의 "이미지"·"image of" 등) |
| label | `accessibility.unlabeled_form_controls` |
| label-content-name-mismatch | `accessibility.label_name_mismatch` |
| label-title-only | `accessibility.label_title_only` |
| landmark-banner-is-top-level, landmark-contentinfo-is-top-level, landmark-main-is-top-level, landmark-complementary-is-top-level | `accessibility.landmark_not_top_level` (complementary 제외) |
| landmark-no-duplicate-banner | `accessibility.duplicate_banner` |
| landmark-no-duplicate-contentinfo | `accessibility.duplicate_contentinfo` |
| landmark-no-duplicate-main | `accessibility.multiple_main` |
| landmark-one-main | `accessibility.landmark_main` |
| landmark-unique | `accessibility.unnamed_repeated_landmarks` |
| link-name | `accessibility.empty_links` |
| list, listitem | `accessibility.list_structure_invalid` |
| meta-refresh, meta-refresh-no-exceptions | `accessibility.timed_refresh` |
| meta-viewport, meta-viewport-large | `accessibility.zoom_disabled` |
| nested-interactive | `accessibility.nested_interactive` |
| no-autoplay-audio | `accessibility.autoplay_media` |
| object-alt | `accessibility.object_alt_missing` |
| p-as-heading | 확인 불가: 굵기·크기로 제목처럼 보이는지 (렌더링) |
| page-has-heading-one | `content.h1` |
| presentation-role-conflict | `accessibility.presentation_focusable` |
| region | `accessibility.content_outside_landmarks` |
| role-img-alt, svg-img-alt | `accessibility.role_img_unnamed` |
| scope-attr-valid | `accessibility.scope_invalid` |
| select-name | `accessibility.unlabeled_form_controls` |
| server-side-image-map | `accessibility.server_side_image_map` |
| summary-name | `accessibility.empty_summary` |
| tabindex | `accessibility.positive_tabindex` |
| table-duplicate-name, table-fake-caption, td-has-header, th-has-data-cells | 미적용: 표 셀 의미 관계는 구조만으로 판정 시 오탐이 많음. 제목은 `accessibility.table_caption_missing` |
| td-headers-attr | `accessibility.broken_id_refs` |
| valid-lang | `accessibility.element_lang_invalid` |
| video-caption | `accessibility.video_captions_missing` |
| aria-braille-equivalent, aria-conditional-attr, aria-roledescription, aria-text, focus-order-semantics, hidden-content | 미적용: 실험적·보조 규칙 |
| frame-tested | 해당 없음: axe 가 iframe 안을 검사했는지 알리는 내부 규칙 |
| audio-caption, duplicate-id-active | 해당 없음: axe 에서 폐기된 규칙 (duplicate-id 계열은 `accessibility.duplicate_ids`) |

## Lighthouse

[Lighthouse 기본 설정](https://github.com/GoogleChrome/lighthouse/blob/main/core/config/default-config.js)의 감사 항목입니다.

### 접근성

Lighthouse 접근성 범주는 axe-core 규칙을 그대로 씁니다. 대응 항목은 [axe-core 규칙](#axe-core-규칙) 표와 같습니다.

| 감사 | 항목 id / 확인 불가 이유 |
|---|---|
| accesskeys, aria-allowed-attr, aria-allowed-role, aria-command-name, aria-conditional-attr, aria-deprecated-role, aria-dialog-name, aria-hidden-body, aria-hidden-focus, aria-input-field-name, aria-meter-name, aria-progressbar-name, aria-prohibited-attr, aria-required-attr, aria-required-children, aria-required-parent, aria-roles, aria-text, aria-toggle-field-name, aria-tooltip-name, aria-treeitem-name, aria-valid-attr, aria-valid-attr-value, autocomplete-valid, button-name, bypass, color-contrast, definition-list, dlitem, duplicate-id-aria, empty-heading, form-field-multiple-labels, frame-title, heading-order, html-has-lang, html-lang-valid, html-xml-lang-mismatch, identical-links-same-purpose, image-redundant-alt, input-button-name, input-image-alt, label, label-content-name-mismatch, landmark-one-main, link-in-text-block, link-name, list, listitem, meta-refresh, meta-viewport, object-alt, presentation-role-conflict, select-name, skip-link, svg-img-alt, tabindex, table-duplicate-name, table-fake-caption, target-size, td-has-header, td-headers-attr, th-has-data-cells, valid-lang, video-caption | axe-core 규칙 표 참조 |
| custom-controls-labels, custom-controls-roles, focus-traps, focusable-controls, interactive-element-affordance, logical-tab-order, managed-focus, offscreen-content-hidden, use-landmarks, visual-order-follows-dom | Lighthouse 도 자동 판정하지 않는 수동 점검 항목. 일부 단서: `accessibility.custom_control_unfocusable` `accessibility.custom_control_unnamed` `accessibility.landmark_main` |

### AI 에이전트 (Agentic Browsing)

| 감사 | 항목 id / 확인 불가 이유 |
|---|---|
| llms-txt | `geo.llms_txt` `geo.llms_txt_title` `geo.llms_txt_links` (Lighthouse 와 같이 H1 제목과 링크 유무) |
| ard-schema | `geo.ard_manifest` `geo.ai_catalog_legacy` (위치·JSON 형식까지. ARD JSON 스키마 검증은 미적용) |
| webmcp-form-coverage | `geo.webmcp_forms` |
| webmcp-registered-tools | 일부: `geo.webmcp_script` (인라인 스크립트의 navigator.modelContext). 실제 등록된 도구는 실행 필요 |
| webmcp-schema-validity | 확인 불가: 실행 중 등록된 도구의 스키마 |
| agent-accessibility-tree | 확인 불가: 브라우저가 만든 접근성 트리 필요 |

### SEO

| 감사 | 항목 id / 확인 불가 이유 |
|---|---|
| is-crawlable | `indexing.meta_noindex` `indexing.header_noindex` `crawling.robots_allows.googlebot` |
| document-title | `meta.title` |
| meta-description | `meta.description` |
| http-status-code | `transport.http_status` |
| link-text | `accessibility.vague_link_text` |
| crawlable-anchors | `content.uncrawlable_links` |
| robots-txt | `crawling.robots_txt` `crawling.robots_invalid_lines` |
| image-alt | `content.img_alt_missing` |
| hreflang | `structured_data.hreflang` `structured_data.hreflang_self` `structured_data.hreflang_x_default` `structured_data.hreflang_reciprocal` |
| canonical | `meta.canonical` `meta.canonical_host_mismatch` `meta.canonical_target_mismatch` `meta.canonical_target_noindex` `links.canonical` |
| structured-data | `structured_data.json_ld` `structured_data.json_ld_invalid` `structured_data.required.organization` 등 |
| viewport | `meta.viewport` |
| plugins | `content.plugins` |
| font-size, tap-targets | 확인 불가: 렌더링 크기 |

### 모범 사례

| 감사 | 항목 id / 확인 불가 이유 |
|---|---|
| is-on-https | `transport.https` `transport.mixed_content` |
| redirects-http | `transport.http_to_https_redirect` |
| has-hsts | `transport.hsts` |
| csp-xss | `security.csp` (값만 기록, 정책 강도는 평가하지 않음) |
| origin-isolation | `security.cross_origin_opener_policy` |
| clickjacking-mitigation | `security.x_frame_options` `security.csp` |
| paste-preventing-inputs | `accessibility.paste_blocked` |
| doctype | `meta.doctype` |
| charset | `meta.charset` |
| bf-cache | `performance.bfcache_blocked` |
| trusted-types-xss | `security.trusted_types` |
| js-libraries | `meta.framework` (프레임워크 식별) |
| baseline | 확인 불가: 페이지가 쓰는 웹 기능의 브라우저 지원 데이터와 실행 필요 |
| image-aspect-ratio, image-size-responsive | 확인 불가: 렌더링 크기 |
| geolocation-on-start, notification-on-start | 확인 불가: 스크립트 실행 |
| errors-in-console, deprecations, inspector-issues, valid-source-maps, third-party-cookies | 확인 불가: 브라우저 실행 |
| no-vulnerable-libraries | 확인 불가: 취약점 DB 필요 (외부 API). 프레임워크 식별은 `meta.framework` |

### 성능

| 감사 | 항목 id / 확인 불가 이유 |
|---|---|
| FCP·LCP·TBT·CLS·Speed Index 지표 | 확인 불가: 렌더링 측정 |
| render-blocking-resources | `performance.render_blocking_scripts` `performance.head_stylesheets` |
| critical-request-chains | `performance.css_import` |
| uses-text-compression | `performance.compression` |
| uses-long-cache-ttl | `performance.static_asset_cache` `performance.cache_control` |
| total-byte-weight | `performance.page_weight` `performance.html_bytes` |
| dom-size | `performance.dom_size` |
| uses-rel-preconnect | `performance.preconnect` |
| font-display | `performance.font_face_without_display` |
| modern-image-formats | `performance.modern_image_formats` |
| offscreen-images | `performance.img_lazy` |
| lcp-lazy-loaded | `performance.lcp_image_lazy` |
| prioritize-lcp-image | `performance.fetchpriority_high` `performance.image_preload` |
| uses-responsive-images | `performance.img_srcset` |
| unsized-images | `performance.img_dimensions_missing` |
| efficient-animated-content | `performance.large_gifs` |
| unminified-css | `performance.unminified_css` |
| unminified-javascript | 미적용: JS 본문을 보관하지 않음. 큰 파일은 `performance.large_scripts` |
| duplicated-javascript | `performance.duplicate_scripts` (같은 src 기준) |
| no-document-write | `performance.document_write` |
| server-response-time | `performance.response_time` |
| redirects | `transport.redirect` |
| third-party-summary | `performance.third_party_scripts` |
| uses-http2 | `transport.http2` |
| unused-css-rules, unused-javascript, legacy-javascript, bootup-time, mainthread-work-breakdown, uses-passive-event-listeners, layout-shifts | 확인 불가: 스크립트 실행·렌더링 |
| first-contentful-paint, largest-contentful-paint, cumulative-layout-shift, total-blocking-time, speed-index, interactive, max-potential-fid, interaction-to-next-paint | 확인 불가: 렌더링 측정 지표 |
| cache-insight | `performance.static_asset_cache` `performance.cache_control` |
| document-latency-insight | `transport.redirect` `performance.response_time` `performance.compression` |
| dom-size-insight | `performance.dom_size` |
| duplicated-javascript-insight | `performance.duplicate_scripts` |
| font-display-insight | `performance.font_face_without_display` |
| image-delivery-insight | `performance.modern_image_formats` `performance.large_images` `performance.img_srcset` |
| lcp-discovery-insight | `performance.lcp_image_lazy` `performance.fetchpriority_high` `performance.image_preload` |
| modern-http-insight | `transport.http2` |
| network-dependency-tree-insight | `performance.css_import` `performance.preconnect` |
| render-blocking-insight | `performance.render_blocking_scripts` `performance.head_stylesheets` |
| third-parties-insight | `performance.third_party_scripts` |
| viewport-insight | `meta.viewport` |
| cls-culprits-insight, forced-reflow-insight, inp-breakdown-insight, lcp-breakdown-insight, legacy-javascript-insight | 확인 불가: 렌더링·스크립트 실행 |
| diagnostics, final-screenshot, long-tasks, main-thread-tasks, metrics, network-requests, network-rtt, network-server-latency, non-composited-animations, resource-summary, screenshot-thumbnails, script-treemap-data, user-timings | 해당 없음: 판정이 아닌 진단용 원자료 (브라우저 실행 결과) |

## Google 검색 센터

| 요건 | 항목 id |
|---|---|
| robots.txt | `crawling.robots_txt` `crawling.robots_allows.googlebot` `crawling.robots_blocks_resources` `crawling.robots_oversize` |
| sitemap | `crawling.sitemap_xml` `crawling.sitemap_any` `crawling.sitemap_entries` `crawling.sitemap_lastmod` `crawling.sitemap_broken_urls` `crawling.sitemap_noindex_pages` `crawling.sitemap_canonical_mismatch` `crawling.sitemap_oversize` |
| 색인 제어 | `indexing.meta_robots` `indexing.meta_noindex` `indexing.header_noindex` `indexing.x_robots_tag` |
| 스니펫 제어 | `indexing.snippet_controls` |
| 표준 URL | `meta.canonical` `meta.canonical_target_mismatch` `transport.url_variant.trailing_slash` `transport.host_variant_duplicate` |
| 다국어 | `structured_data.hreflang` `structured_data.hreflang_reciprocal` `meta.lang_locale_mismatch` |
| 구조화 데이터 | `structured_data.json_ld` `structured_data.required.article` 등 |
| 제목·설명 | `meta.title` `meta.description` `site.duplicate_titles` `site.duplicate_descriptions` |
| 크롤링 가능한 링크 | `content.uncrawlable_links` `content.broken_internal_links` |
| 이미지 | `content.img_alt_missing` `performance.img_srcset` |
| JavaScript SEO | `content.csr_shell` `geo.noscript_content` |
| soft 404 | `not_found_page.returns_404` |
| HTTPS | `transport.https` `transport.http_to_https_redirect` |
| 모바일 | `meta.viewport` |
| 파비콘 | `meta.favicon` `links.favicon` |
| Core Web Vitals | 확인 불가: 렌더링 측정 |

## Google 리치 결과

[Google 검색 구조화 데이터 갤러리](https://developers.google.com/search/docs/appearance/structured-data/search-gallery)의 기능별 문서에 있는 "Required properties" 를 기준으로 합니다. 최상위 속성만 보며, 해당 유형이 없으면 `not_applicable` 입니다.

| 기능 문서 | 항목 id / 비고 |
|---|---|
| article | `structured_data.required.article` (Google 은 필수 속성 없음, 권장 속성 기준) |
| breadcrumb | `structured_data.required.breadcrumb_list` |
| book | `structured_data.required.book` |
| carousel | `structured_data.required.item_list` |
| course | `structured_data.required.course` |
| dataset | `structured_data.required.dataset` |
| discussion-forum | `structured_data.required.discussion_forum` |
| education-qa | `structured_data.required.education_qa` |
| employer-rating | `structured_data.required.employer_rating` |
| event | `structured_data.required.event` |
| factcheck | `structured_data.required.claim_review` |
| image-license-metadata | `structured_data.required.image_license` |
| job-posting | `structured_data.required.job_posting` |
| local-business | `structured_data.required.local_business` |
| loyalty-program | `structured_data.required.loyalty_program` |
| math-solvers | `structured_data.required.math_solver` |
| product, merchant-listing, product-snippet | `structured_data.required.product` `structured_data.type.product` |
| product-variants | `structured_data.required.product_group` |
| movie | `structured_data.required.movie` |
| organization | `structured_data.required.organization` (Google 은 필수 속성 없음, 권장 속성 기준) |
| profile-page | `structured_data.required.profile_page` |
| qapage | `structured_data.required.qa_page` |
| recipe | `structured_data.required.recipe` |
| return-policy | `structured_data.required.merchant_return_policy` |
| software-app | `structured_data.required.software_app` |
| speakable | `structured_data.speakable` |
| vacation-rental | `structured_data.required.vacation_rental` |
| video | `structured_data.required.video` |
| review-snippet | 미적용: Review·AggregateRating 은 대개 Product 등에 중첩되어 itemReviewed 를 생략하므로 단독 판정 시 오탐 |
| shipping-policy | 미적용: 판매자 수준 배송 설정(Merchant Center 연동)이라 페이지 단위 판정이 어려움 |
| paywalled-content | 미적용: 유료 콘텐츠 여부를 페이지만으로 알 수 없음 |
| carousels-beta | 해당 없음: 일부 지역 베타 |
| intro-structured-data, sd-policies, search-gallery, generate-structured-data-with-javascript | 해당 없음: 안내 문서 |

## 보안 헤더

[OWASP Secure Headers Project](https://owasp.org/www-project-secure-headers/)의 [권장 헤더](https://github.com/OWASP/www-project-secure-headers/blob/master/ci/headers_add.json)와 [제거 권장 헤더](https://github.com/OWASP/www-project-secure-headers/blob/master/ci/headers_remove.json) 목록입니다.

| 헤더 | 항목 id |
|---|---|
| Strict-Transport-Security | `transport.hsts` |
| Content-Security-Policy | `security.csp` |
| X-Frame-Options | `security.x_frame_options` |
| X-Content-Type-Options | `security.x_content_type_options` |
| Referrer-Policy | `security.referrer_policy` |
| Permissions-Policy | `security.permissions_policy` |
| Cross-Origin-Opener-Policy | `security.cross_origin_opener_policy` |
| Cross-Origin-Embedder-Policy | `security.cross_origin_embedder_policy` |
| Cross-Origin-Resource-Policy | `security.cross_origin_resource_policy` |
| X-DNS-Prefetch-Control | `security.x_dns_prefetch_control` |
| X-Permitted-Cross-Domain-Policies | `security.x_permitted_cross_domain_policies` |
| Cache-Control | `performance.cache_control` |
| Clear-Site-Data | 해당 없음: 로그아웃 응답용 헤더라 일반 페이지에는 쓰지 않음 |
| 제거 권장 헤더 (88개) | `security.server_header` `security.x_powered_by` `security.info_disclosure_headers` (나머지 86개 전체) |
| 노출하지 말아야 할 헤더 (Server, X-Powered-By) | `security.server_header` `security.x_powered_by` |
| 쿠키 Secure·HttpOnly | `security.insecure_cookies` |
| 하위 리소스 무결성 (SRI) | `security.sri_missing` |
| 안전한 폼 전송 | `security.insecure_form_action` |
| 새 창 링크 opener | `security.target_blank_noopener` |
| security.txt | `transport.security_txt` `transport.security_txt_fields` (RFC 9116 필수 Contact·Expires) |
| Trusted Types (CSP) | `security.trusted_types` |
| X-XSS-Protection | 의도적으로 제외: 폐기된 헤더 (OWASP 도 사용 중단 권고) |

## 생성형 검색(GEO)

| 신호 | 항목 id |
|---|---|
| llms.txt 규격 | `geo.llms_txt` `geo.llms_txt_title` `geo.llms_txt_summary` `geo.llms_txt_sections` `geo.llms_txt_links` `geo.llms_full_txt` |
| AI 크롤러 robots 허용 | `geo.robots_allows.gptbot` 외 13개 |
| AI 크롤러 실제 접근 | `geo.ua_access.gptbot` `geo.ua_access.claudebot` `geo.ua_access.perplexitybot` |
| AI 학습 거부 표시 | `geo.meta_noai` `geo.tdm_reservation` `geo.ai_txt` |
| 스니펫·요약 제한 | `indexing.snippet_controls` |
| 마크다운 제공 | `geo.markdown_version` `geo.markdown_alternate` |
| 저자·날짜 | `geo.author` `geo.published_date` `geo.modified_date` |
| 인용하기 쉬운 구조 | `geo.question_headings` `geo.lists` `geo.tables` `structured_data.type.faq_page` `structured_data.speakable` |
| 정적 HTML 로 읽히는지 | `content.csr_shell` `geo.noscript_content` |
| AI 에이전트용 리소스 목록 (ARD) | `geo.ard_manifest` `geo.ai_catalog_legacy` |
| AI 에이전트가 쓸 수 있는 폼·도구 (WebMCP) | `geo.webmcp_forms` `geo.webmcp_script` |

## 확인할 수 없는 것 요약

헤드리스 브라우저·외부 API 없이 정적 HTML 과 응답 헤더만 보기 때문에 아래는 원리상 확인할 수 없습니다.

- **렌더링이 필요한 것:** 명도 대비, 색만으로 전달하는 정보, 컨트롤 크기, 실제 초점 순서와 표시, CSS 배치 후 읽기 순서, 이미지 비율, 글자 크기, Core Web Vitals.
- **스크립트 실행이 필요한 것:** 스크립트로 붙인 키보드·포인터 이벤트, 키보드 트랩, 타이머, 오류 정정, 동적 상태 알림, 콘솔 오류, 사용하지 않는 CSS·JS.
- **사람의 의미 판단이 필요한 것:** 대체 텍스트·제목·링크 텍스트의 적절성, 명확한 지시사항, 도움 정보의 일관성.
- **외부 데이터가 필요한 것:** 라이브러리 취약점(취약점 DB), 접근성 인증의 유효성(발급기관 조회).
