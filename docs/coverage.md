# 점검 기준 대조표

공신력 있는 점검 기준을 하나씩 대조해, 기준마다 어떤 항목 id 로 확인하는지 또는 왜 확인할 수 없는지를 적은 표입니다. 이 라이브러리는 헤드리스 브라우저와 외부 API 를 쓰지 않으므로, **렌더링 결과(계산된 색·크기·배치), 스크립트 실행, 사람의 의미 판단**이 필요한 기준은 확인할 수 없습니다. 그런 기준은 "확인 불가"와 이유를 적었습니다.

표의 항목 id 는 모두 [`CATALOG`](../crates/devup-access-checker/src/catalog.rs)에 있으며, 테스트(`coverage_doc_ids_exist`)가 이를 확인합니다.

`python scripts/check_coverage.py` 는 아래 기준의 최신 원문을 받아 대조표·내장 목록과 다시 대조하고, 빠진 이름이 있으면 출력합니다. axe-core 규칙, Lighthouse 감사, Google 구조화 데이터 문서, OWASP 권장 헤더, WCAG 2.2 성공 기준, Open Graph 속성, Web App Manifest 멤버, WHATWG HTML 폐기 속성, schema.org 타입, MDN 폐기·비표준 헤더가 대상입니다. Observatory 테스트, webhint hint, HTMLHint 규칙도 대상입니다. KWCAG 2.2, Google 검색 센터 문서, X Cards, 네이버 서치어드바이저, Bing, Yandex, Daum, Apple, EN 301 549, IBM Equal Access, HTML_CodeSniffer, ARIA in HTML, W3C i18n checker, WAVE, Nu HTML Checker, AI·GEO 프로토콜 명세는 사람이 대조했습니다 (규칙 수는 각 절에 적었습니다).

- [KWCAG 2.2 (33개 검사항목)](#kwcag-22)
- [WCAG 2.2 (성공 기준 87개)](#wcag-22)
- [axe-core 규칙](#axe-core-규칙)
- [Lighthouse](#lighthouse)
- [Google 검색 센터](#google-검색-센터)
- [Google 리치 결과 (구조화 데이터)](#google-리치-결과)
- [보안 헤더 (OWASP Secure Headers)](#보안-헤더)
- [Open Graph·X Cards](#open-graphx-cards)
- [Web App Manifest](#web-app-manifest)
- [HTML 폐기 속성](#html-폐기-속성)
- [schema.org](#schemaorg)
- [MDN 폐기 헤더](#mdn-폐기-헤더)
- [네이버 서치어드바이저](#네이버-서치어드바이저)
- [Bing 웹마스터 가이드라인](#bing-웹마스터-가이드라인)
- [검색엔진 문서 (2차)](#검색엔진-문서-2차)
- [AI·GEO 프로토콜](#aigeo-프로토콜)
- [보안 스캐너](#보안-스캐너)
- [접근성 검사기](#접근성-검사기)
- [HTML 검사기·성능](#html-검사기성능)
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

## WCAG 2.2

W3C 웹 콘텐츠 접근성 지침 2.2 의 성공 기준 87개 전체입니다 (A·AA·AAA, 폐지된 4.1.1 포함). 기준 목록은 `scripts/check_coverage.py` 가 W3C 의 `wcag.json` 과 다시 대조합니다.

| 성공 기준 | 등급 | 항목 id | 확인할 수 없는 부분 |
|---|---|---|---|
| 1.1.1 Non-text Content | A | `content.img_alt_missing` `accessibility.filename_alt` `accessibility.long_alt` `accessibility.redundant_alt` `accessibility.area_alt_missing` `accessibility.object_alt_missing` `accessibility.role_img_unnamed` `accessibility.unnamed_buttons` | 대체 텍스트가 이미지 내용과 맞는지 (의미 판단) |
| 1.2.1 Audio-only and Video-only (Prerecorded) | A | — | 확인 불가: 오디오·영상의 대체 대본이 있는지는 의미 판단 |
| 1.2.2 Captions (Prerecorded) | A | `accessibility.video_captions_missing` | 영상에 입힌 자막 |
| 1.2.3 Audio Description or Media Alternative (Prerecorded) | A | `accessibility.video_audio_description_missing` | 대체 대본으로 대신한 경우 (의미 판단) |
| 1.2.4 Captions (Live) | AA | — | 확인 불가: 실시간 방송 자막 (재생 필요) |
| 1.2.5 Audio Description (Prerecorded) | AA | `accessibility.video_audio_description_missing` | 영상 음성에 직접 넣은 해설 |
| 1.2.6 Sign Language (Prerecorded) | AAA | — | 확인 불가: 수어 영상 (영상 분석) |
| 1.2.7 Extended Audio Description (Prerecorded) | AAA | — | 확인 불가: 확장 음성 해설 (영상 분석) |
| 1.2.8 Media Alternative (Prerecorded) | AAA | — | 확인 불가: 대체 대본의 충실도 (의미 판단) |
| 1.2.9 Audio-only (Live) | AAA | — | 확인 불가: 실시간 오디오 대체 (재생 필요) |
| 1.3.1 Info and Relationships | A | `accessibility.table_headers_missing` `accessibility.empty_table_headers` `accessibility.scope_invalid` `accessibility.table_caption_missing` `accessibility.list_structure_invalid` `accessibility.definition_list_invalid` `accessibility.fieldset_legend_missing` `accessibility.radio_group_ungrouped` `accessibility.broken_id_refs` `accessibility.aria_required_children` `accessibility.aria_required_parent` | 시각적 표현과 구조가 맞는지 (렌더링) |
| 1.3.2 Meaningful Sequence | A | `content.heading_skip` `accessibility.list_structure_invalid` `accessibility.positive_tabindex` | CSS 배치로 바뀐 읽기 순서 (렌더링) |
| 1.3.3 Sensory Characteristics | A | — | 확인 불가: 모양·위치만으로 지시하는지는 의미 판단 |
| 1.3.4 Orientation | AA | `accessibility.orientation_lock` | 스크립트로 고정한 방향 |
| 1.3.5 Identify Input Purpose | AA | `accessibility.autocomplete_missing` `accessibility.autocomplete_invalid` | — |
| 1.3.6 Identify Purpose | AAA | `accessibility.landmark_main` `accessibility.landmark_nav` `accessibility.landmark_header` `accessibility.landmark_footer` `accessibility.invalid_aria_role` | 아이콘·컴포넌트 목적 표시 (의미 판단) |
| 1.4.1 Use of Color | A | — | 확인 불가: 계산된 색 필요 (렌더링) |
| 1.4.2 Audio Control | A | `accessibility.autoplay_media` | 스크립트로 재생하는 소리 |
| 1.4.3 Contrast (Minimum) | AA | — | 확인 불가: 계산된 글자·배경 색 필요 (렌더링) |
| 1.4.4 Resize Text | AA | `accessibility.zoom_disabled` | 200% 확대 시 잘림 (렌더링) |
| 1.4.5 Images of Text | AA | — | 확인 불가: 이미지 속 글자 판별 (이미지 분석) |
| 1.4.6 Contrast (Enhanced) | AAA | — | 확인 불가: 계산된 색 필요 (렌더링) |
| 1.4.7 Low or No Background Audio | AAA | — | 확인 불가: 배경음 크기 (오디오 분석) |
| 1.4.8 Visual Presentation | AAA | `accessibility.justified_text` | 색 선택·줄 간격·폭 80자 (렌더링) |
| 1.4.9 Images of Text (No Exception) | AAA | — | 확인 불가: 이미지 속 글자 판별 (이미지 분석) |
| 1.4.10 Reflow | AA | `accessibility.viewport_fixed_width` `meta.viewport` | 320px 폭에서 가로 스크롤 (렌더링) |
| 1.4.11 Non-text Contrast | AA | — | 확인 불가: 계산된 색 필요 (렌더링) |
| 1.4.12 Text Spacing | AA | `accessibility.inline_spacing_important` | 외부 CSS 의 고정 높이로 잘림 (렌더링) |
| 1.4.13 Content on Hover or Focus | AA | — | 확인 불가: 호버·초점 시 나타나는 콘텐츠 (실행) |
| 2.1.1 Keyboard | A | `accessibility.custom_control_unfocusable` `accessibility.click_without_keyboard` `accessibility.focusable_aria_hidden` `accessibility.nested_interactive` `accessibility.presentation_focusable` | 스크립트로 붙인 이벤트 (실행) |
| 2.1.2 No Keyboard Trap | A | — | 확인 불가: 키보드 트랩 (실행) |
| 2.1.3 Keyboard (No Exception) | AAA | `accessibility.custom_control_unfocusable` `accessibility.click_without_keyboard` | 스크립트로 붙인 이벤트 (실행) |
| 2.1.4 Character Key Shortcuts | A | `accessibility.duplicate_accesskeys` | 스크립트로 구현한 단축키 |
| 2.2.1 Timing Adjustable | A | `accessibility.timed_refresh` | 스크립트 타이머·세션 만료 |
| 2.2.2 Pause, Stop, Hide | A | `accessibility.autoplay_media` `accessibility.obsolete_elements` `accessibility.reduced_motion` | 스크립트 자동 슬라이드 |
| 2.2.3 No Timing | AAA | `accessibility.timed_refresh` | 스크립트 타이머 |
| 2.2.4 Interruptions | AAA | — | 확인 불가: 스크립트 알림·팝업 (실행) |
| 2.2.5 Re-authenticating | AAA | — | 확인 불가: 재인증 후 데이터 유지 (로그인 필요) |
| 2.2.6 Timeouts | AAA | — | 확인 불가: 비활동 시간 안내 (의미 판단) |
| 2.3.1 Three Flashes or Below Threshold | A | — | 확인 불가: 프레임 분석 필요 |
| 2.3.2 Three Flashes | AAA | — | 확인 불가: 프레임 분석 필요 |
| 2.3.3 Animation from Interactions | AAA | `accessibility.reduced_motion` | 스크립트 애니메이션 |
| 2.4.1 Bypass Blocks | A | `accessibility.skip_link` `accessibility.landmark_main` `accessibility.landmark_nav` `accessibility.landmark_header` `accessibility.landmark_footer` | 건너뛰기 링크 동작 (실행) |
| 2.4.2 Page Titled | A | `meta.title` `accessibility.page_title_generic` `accessibility.iframe_title_missing` `accessibility.iframe_title_duplicate` | 제목이 내용을 잘 설명하는지 (의미 판단) |
| 2.4.3 Focus Order | A | `accessibility.positive_tabindex` `accessibility.dialog_unnamed` | 실제 초점 순서 (렌더링) |
| 2.4.4 Link Purpose (In Context) | A | `accessibility.empty_links` `accessibility.vague_link_text` `accessibility.identical_link_text` | 문맥상 목적이 분명한지 (의미 판단) |
| 2.4.5 Multiple Ways | AA | `accessibility.site_search` `accessibility.landmark_nav` `accessibility.breadcrumb_nav` | 사이트맵 페이지 등 다른 경로 (의미 판단) |
| 2.4.6 Headings and Labels | AA | `accessibility.empty_headings` `accessibility.unlabeled_form_controls` | 제목·레이블이 설명적인지 (의미 판단) |
| 2.4.7 Focus Visible | AA | `accessibility.outline_removed` | 실제 초점 표시 모양 (렌더링) |
| 2.4.8 Location | AAA | `accessibility.breadcrumb_nav` `structured_data.type.breadcrumb_list` | — |
| 2.4.9 Link Purpose (Link Only) | AAA | `accessibility.vague_link_text` `accessibility.identical_link_text` `accessibility.empty_links` | 링크 텍스트만으로 목적이 분명한지 (의미 판단) |
| 2.4.10 Section Headings | AAA | `content.h2` `content.heading_skip` | 구역마다 제목이 있는지 (의미 판단) |
| 2.4.11 Focus Not Obscured (Minimum) | AA | — | 확인 불가: 고정 헤더에 가려지는지 (렌더링) |
| 2.4.12 Focus Not Obscured (Enhanced) | AAA | — | 확인 불가: 고정 헤더에 가려지는지 (렌더링) |
| 2.4.13 Focus Appearance | AAA | — | 확인 불가: 초점 표시 크기·대비 (렌더링) |
| 2.5.1 Pointer Gestures | A | — | 확인 불가: 스크립트 제스처 |
| 2.5.2 Pointer Cancellation | A | — | 확인 불가: 스크립트 이벤트 |
| 2.5.3 Label in Name | A | `accessibility.label_name_mismatch` | CSS 로 숨긴 텍스트 구분 |
| 2.5.4 Motion Actuation | A | — | 확인 불가: 기기 동작 센서 스크립트 |
| 2.5.5 Target Size (Enhanced) | AAA | — | 확인 불가: 컨트롤 크기 (렌더링) |
| 2.5.6 Concurrent Input Mechanisms | AAA | — | 확인 불가: 입력 수단 제한 스크립트 |
| 2.5.7 Dragging Movements | AA | — | 확인 불가: 드래그 대체 수단 (실행) |
| 2.5.8 Target Size (Minimum) | AA | — | 확인 불가: 컨트롤 크기 (렌더링) |
| 3.1.1 Language of Page | A | `meta.html_lang` `accessibility.html_lang_valid` `accessibility.html_xml_lang_mismatch` | 실제 본문 언어와 맞는지 |
| 3.1.2 Language of Parts | AA | `accessibility.element_lang_invalid` | 외국어 구간에 lang 을 빠뜨렸는지 (언어 판별) |
| 3.1.3 Unusual Words | AAA | — | 확인 불가: 낯선 단어 설명 (의미 판단) |
| 3.1.4 Abbreviations | AAA | `accessibility.abbr_without_title` | abbr 로 표시하지 않은 약어 (의미 판단) |
| 3.1.5 Reading Level | AAA | — | 확인 불가: 읽기 수준 (의미 판단) |
| 3.1.6 Pronunciation | AAA | — | 확인 불가: 발음 표시 필요 여부 (의미 판단) |
| 3.2.1 On Focus | A | `accessibility.onfocus_navigation` | addEventListener 로 붙인 스크립트 (실행) |
| 3.2.2 On Input | A | `accessibility.onchange_navigation` | addEventListener 로 붙인 스크립트 (실행) |
| 3.2.3 Consistent Navigation | AA | — | 확인 불가: 페이지 간 메뉴 순서 비교 (렌더링·의미 판단) |
| 3.2.4 Consistent Identification | AA | — | 확인 불가: 같은 기능의 이름 일관성 (의미 판단) |
| 3.2.5 Change on Request | AAA | `accessibility.new_window_links` `accessibility.timed_refresh` `transport.meta_refresh_redirect` `transport.js_redirect` | 스크립트로 바꾸는 맥락 |
| 3.2.6 Consistent Help | A | — | 확인 불가: 페이지 간 도움 정보 위치 비교 (렌더링) |
| 3.3.1 Error Identification | A | — | 확인 불가: 제출 후 오류 표시 (실행) |
| 3.3.2 Labels or Instructions | A | `accessibility.unlabeled_form_controls` `accessibility.label_title_only` `accessibility.multiple_labels` `accessibility.fieldset_legend_missing` `accessibility.radio_group_ungrouped` | — |
| 3.3.3 Error Suggestion | AA | — | 확인 불가: 오류 수정 제안 (실행) |
| 3.3.4 Error Prevention (Legal, Financial, Data) | AA | — | 확인 불가: 제출 전 확인·취소 절차 (실행) |
| 3.3.5 Help | AAA | — | 확인 불가: 도움말 충실도 (의미 판단) |
| 3.3.6 Error Prevention (All) | AAA | — | 확인 불가: 제출 전 확인·취소 절차 (실행) |
| 3.3.7 Redundant Entry | A | `accessibility.autocomplete_missing` `accessibility.autocomplete_invalid` | 단계 간 자동 채움 (실행) |
| 3.3.8 Accessible Authentication (Minimum) | AA | `accessibility.paste_blocked` `accessibility.password_autocomplete_off` | CAPTCHA·인지 테스트 대체 수단 |
| 3.3.9 Accessible Authentication (Enhanced) | AAA | `accessibility.paste_blocked` `accessibility.password_autocomplete_off` | CAPTCHA·인지 테스트 대체 수단 |
| 4.1.1 Parsing (Obsolete and removed) | 폐지 | `accessibility.markup_errors` `accessibility.duplicate_ids` | WCAG 2.2 에서 폐지됨 (KWCAG 는 유지) |
| 4.1.2 Name, Role, Value | A | `accessibility.invalid_aria_role` `accessibility.invalid_aria_attr` `accessibility.aria_invalid_value` `accessibility.aria_required_attr_missing` `accessibility.aria_name_prohibited` `accessibility.aria_deprecated_role` `accessibility.custom_control_unnamed` `accessibility.dialog_unnamed` `accessibility.unlabeled_form_controls` `accessibility.unnamed_buttons` | 상태 변화 전달 (실행) |
| 4.1.3 Status Messages | AA | `accessibility.status_widget_unnamed` | 스크립트로 넣는 상태 메시지 (실행) |

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

## Open Graph·X Cards

[ogp.me](https://ogp.me/) 가 정의한 속성 전체입니다. 속성 목록은 `scripts/check_coverage.py` 가 ogp.me 원문과 다시 대조합니다.

| 속성 | 항목 id | 비고 |
|---|---|---|
| og:title | `open_graph.title` | — |
| og:type | `open_graph.type` `open_graph.type_unknown` | — |
| og:image | `open_graph.image` `open_graph.image_file` `links.og_image` | — |
| og:url | `open_graph.url` `open_graph.url_host_mismatch` | — |
| og:description | `open_graph.description` | — |
| og:site_name | `open_graph.site_name` | — |
| og:locale | `open_graph.locale` `meta.lang_locale_mismatch` | — |
| og:locale:alternate | `open_graph.locale_alternate` | — |
| og:determiner | `open_graph.determiner` | — |
| og:image:url | `open_graph.image` | og:image 의 별칭이라 og:image 로 확인 |
| og:image:secure_url | `open_graph.image_secure_url` | — |
| og:image:type | `open_graph.image_type` | — |
| og:image:width | `open_graph.image_width` `open_graph.image_file` | — |
| og:image:height | `open_graph.image_height` `open_graph.image_file` | — |
| og:image:alt | `open_graph.image_alt` | — |
| og:video | `open_graph.video` | — |
| og:video:secure_url og:video:type og:video:width og:video:height | `open_graph.video` | 하위 속성은 개별 항목 없이 og:video 존재만 확인 |
| og:audio | `open_graph.audio` | — |
| og:audio:secure_url og:audio:type | `open_graph.audio` | 하위 속성은 개별 항목 없이 og:audio 존재만 확인 |
| article:published_time article:modified_time article:expiration_time article:author article:section article:tag | `open_graph.type_properties` | 값을 목록으로 보고 |
| book:author book:isbn book:release_date book:tag | `open_graph.type_properties` | 값을 목록으로 보고 |
| profile:first_name profile:last_name profile:username profile:gender | `open_graph.type_properties` | 값을 목록으로 보고 |
| music:duration music:album music:album:disc music:album:track music:musician music:song music:song:disc music:song:track music:release_date music:creator | `open_graph.type_properties` | 값을 목록으로 보고 |
| video:actor video:actor:role video:director video:writer video:duration video:release_date video:tag video:series | `open_graph.type_properties` | 값을 목록으로 보고 |
| fb:app_id | `facebook.app_id` | — |

X(Twitter) Cards 마크업 문서의 속성입니다. 공식 문서가 스크립트로 그려져 사람이 대조했습니다.

| 속성 | 항목 id | 비고 |
|---|---|---|
| twitter:card | `twitter.card` `twitter.card_unknown` | — |
| twitter:site twitter:site:id | `twitter.site` | :id 는 개별 항목 없음 |
| twitter:creator twitter:creator:id | `twitter.creator` | :id 는 개별 항목 없음 |
| twitter:title | `twitter.title` | — |
| twitter:description | `twitter.description` | — |
| twitter:image | `twitter.image` `links.twitter_image` | — |
| twitter:image:alt | `twitter.image_alt` | — |
| twitter:player twitter:player:width twitter:player:height twitter:player:stream | `twitter.player` | 하위 속성은 개별 항목 없음 |
| twitter:app:name:* twitter:app:id:* twitter:app:url:* | `twitter.app` | 값을 목록으로 보고 |

## Web App Manifest

W3C Web Application Manifest 와 Manifest App Information 명세의 멤버입니다. `meta.manifest_content` 의 `members` 값에 manifest 의 최상위 멤버 이름을 모두 담습니다. 멤버 목록은 `scripts/check_coverage.py` 가 명세 원문과 다시 대조합니다.

| 멤버 | 항목 id | 비고 |
|---|---|---|
| name short_name | `meta.manifest_content` | 값으로 보고 |
| start_url | `meta.manifest_content` | 값으로 보고 |
| display | `meta.manifest_content` `meta.manifest_display_valid` | — |
| icons | `meta.manifest_icons_192_512` `meta.manifest_maskable_icon` `links.manifest` | — |
| purpose | `meta.manifest_maskable_icon` | 아이콘 purpose |
| id | `meta.manifest_id` | — |
| scope | `meta.manifest_scope` | — |
| theme_color background_color | `meta.manifest_colors` | — |
| description | `meta.manifest_description` | — |
| screenshots | `meta.manifest_screenshots` | — |
| shortcuts | `meta.manifest_shortcuts` | — |
| label form_factor platform url | `meta.manifest_screenshots` `meta.manifest_shortcuts` | screenshots·shortcuts 하위 멤버, 개수만 보고 |
| orientation | `meta.manifest_content` | `members` 값으로만 보고 (방향 고정은 `accessibility.orientation_lock`) |
| lang dir | `meta.manifest_content` | `members` 값으로만 보고 |
| categories iarc_rating_id | `meta.manifest_content` | `members` 값으로만 보고 (앱 스토어용) |
| color_scheme_dark | `meta.manifest_content` | `members` 값으로만 보고 |

## HTML 폐기 속성

WHATWG HTML 명세 "Obsolete features" 의 폐기 속성 목록(143줄, 속성 79종) 전체를 `accessibility.obsolete_attributes` 로 확인합니다. 명세가 "폐기되었지만 적합"으로 허용하는 값(`script charset="utf-8"`, 비어 있지 않은 `a name`, `script language="javascript"`, `style type="text/css"`, `img border="0"`)은 세지 않습니다. 목록은 [`spec.rs`](../crates/devup-access-checker/src/spec.rs) 의 `OBSOLETE_ATTRS` 에 있으며 `scripts/check_coverage.py` 가 명세 원문과 다시 대조합니다. 폐기 요소는 `accessibility.obsolete_elements` 가 확인합니다.

## schema.org

schema.org 는 필수 속성을 정하지 않으므로, 리치 결과 필수 속성은 [Google 리치 결과](#google-리치-결과) 표에서 다룹니다. schema.org 자체와는 타입 이름을 대조합니다. JSON-LD 의 `@type` 이 schema.org 최신 어휘(타입 1,485개)에 없으면 `structured_data.unknown_types` 로 보고합니다. 다른 어휘 접두사(`foaf:` 등)는 제외합니다. 타입 목록은 [`schema_types.rs`](../crates/devup-access-checker/src/schema_types.rs) 에 있으며 `scripts/check_coverage.py` 가 schema.org 최신 목록과 다시 대조합니다.

## MDN 폐기 헤더

MDN browser-compat-data 가 폐기(deprecated) 또는 비표준으로 표시한 HTTP 헤더 전체입니다. 응답 헤더는 `security.deprecated_headers` 로 확인합니다. 목록은 `scripts/check_coverage.py` 가 browser-compat-data 최신판과 다시 대조합니다.

| 헤더 | 항목 id |
|---|---|
| Attribution-Reporting-Register-Source | `security.deprecated_headers` |
| Attribution-Reporting-Register-Trigger | `security.deprecated_headers` |
| Content-DPR | `security.deprecated_headers` |
| Expect-CT | `security.deprecated_headers` |
| Feature-Policy | `security.deprecated_headers` (`security.permissions_policy` 로 대체) |
| Observe-Browsing-Topics | `security.deprecated_headers` |
| Pragma | `security.deprecated_headers` |
| Report-To | `security.deprecated_headers` |
| Warning | `security.deprecated_headers` |
| X-XSS-Protection | `security.deprecated_headers` |
| Attribution-Reporting-Eligible | 해당 없음: 요청 헤더 |
| Attribution-Reporting-Support | 해당 없음: 요청 헤더 |
| DNT | 해당 없음: 요청 헤더 |
| DPR | 해당 없음: 요청 헤더 |
| Device-Memory | 해당 없음: 요청 헤더 |
| Sec-Browsing-Topics | 해당 없음: 요청 헤더 |
| Sec-CH-UA-Full-Version | 해당 없음: 요청 헤더 |
| Viewport-Width | 해당 없음: 요청 헤더 |
| Width | 해당 없음: 요청 헤더 |
| Critical-CH | 비표준이지만 폐기 아님: 대상 아님 |
| Idempotency-Key | 비표준이지만 폐기 아님: 대상 아님 |
| Sec-CH-Device-Memory | 해당 없음: 요청 헤더 |
| Snapshot-Content-Location | 비표준이지만 폐기 아님: 대상 아님 |
| X-DNS-Prefetch-Control | `security.x_dns_prefetch_control` (OWASP 권장 목록) |

## 네이버 서치어드바이저

[웹마스터 가이드](https://searchadvisor.naver.com/guide) 54쪽 전체를 사람이 대조했습니다. 가이드 목록은 사이트의 sitemap.xml 에서 받았습니다. 웹마스터 도구에서만 할 수 있는 일(등록·수집 요청·리포트)은 마지막 줄에 모았습니다.

| 가이드 (쪽) | 요구 사항 | 항목 id |
|---|---|---|
| seo-basic-intro, faq-start-register | 사이트 소유 확인 meta, head 안에 있어야 함 | `meta.naver_site_verification` `meta.head_elements_in_body` |
| seo-basic-intro, seo-basic-robots | Yeti 허용, 파비콘 수집 허용, CSS·JS 수집 허용 | `crawling.robots_allows.yeti` `crawling.robots_blocks_favicon` `crawling.robots_blocks_resources_yeti` |
| seo-basic-robots | robots.txt 위치·형식·크기·Sitemap 선언 | `crawling.robots_txt` `crawling.robots_oversize` `crawling.robots_invalid_lines` `crawling.robots_sitemap_directive` |
| seo-basic-firewall, faq-serpmissing | 방화벽이 Yeti 를 막지 않음 | `crawling.ua_access.yeti` (UA 만 바꾼 요청, IP 검증 방화벽은 재현 못 함) |
| seo-basic-intro, markup-structure | meta refresh·JS 리다이렉트 대신 HTTP 리다이렉트 | `transport.js_redirect` `transport.meta_refresh_redirect` `indexing.meta_refresh` |
| seo-basic-redirect | 리다이렉트 5회 이하, 경유 URL 모두 수집 가능, 301/302 구분 | `transport.redirect_hops_excess` `transport.redirect_chain_robots_blocked` `transport.redirect` `transport.temporary_redirect` |
| seo-basic-create | 표준 호스트명(밑줄 금지), 없는 페이지는 404, 오류 페이지로 리다이렉트 금지 | `transport.hostname_invalid` `not_found_page.returns_404` `not_found_page.redirects` |
| seo-basic-create, seo-advanced-url | 한 호스트로 통일, https, 호스팅 기본 도메인 지양, 파라미터 최소화 | `transport.host_variant_duplicate` `transport.http_to_https_redirect` `transport.https` `transport.hosting_subdomain` `content.parameter_heavy_links` |
| seo-basic-http, report-crawl-refine | HTTP 상태 코드, HTML 4MB·응답 시간 | `transport.http_status` `performance.html_bytes` `performance.response_time` |
| markup-structure | canonical 은 절대 경로, robots meta, 네이버 전용 지시어(nosourceinfo) | `meta.canonical` `meta.canonical_relative` `indexing.meta_robots` `indexing.naver_directives` |
| markup-content | title·description 길이, 제목과 설명이 같음, 단어 반복, Open Graph·Twitter | `meta.title` `meta.description` `meta.description_same_as_title` `meta.repeated_terms` `open_graph.title` `open_graph.description` `open_graph.image` `open_graph.url` `open_graph.type` `twitter.card` `twitter.title` `twitter.description` `twitter.image` `twitter.domain` |
| markup-content | og:image 크기·용량 | `open_graph.image_file` (가로·세로·바이트를 값으로 보고) |
| markup-mobile | viewport, 별도 모바일 URL 은 데스크톱 canonical, UA 별 응답 | `meta.viewport` `meta.mobile_site_canonical` `crawling.vary_user_agent` |
| markup-favicon | favicon 존재·크기, 절대 경로, 같은 rel 은 하나 | `meta.favicon` `meta.favicon_ico_file` `meta.favicon_size` `meta.favicon_relative_href` `meta.favicon_duplicate_rel` |
| report-seo | 서버·문서 인코딩 일치, title 은 head 안, frame 지양, 링크 텍스트 | `meta.charset_mismatch` `meta.head_elements_in_body` `accessibility.obsolete_elements` `accessibility.empty_links` |
| seo-advanced-javascript | 서버 렌더링, fragment(#) URL 금지, JS URL 에 타임스탬프 금지 | `content.csr_shell` `content.fragment_route_links` `crawling.script_cache_busting` |
| resource-and-link, faq-sublink | 크롤링 가능한 링크(함수 호출 href 금지), nosublink | `content.uncrawlable_links` `content.pseudo_js_links` `indexing.naver_directives` |
| request-feed | RSS 형식, item 1개 이상, 같은 호스트 절대 URL, 10MB 미만, 본문 전체 | `crawling.feed_link` `crawling.feed_valid` `crawling.feed_items` `crawling.feed_foreign_host` `crawling.feed_oversize` `crawling.feed_full_content` |
| request-feed | sitemap 형식·5만 URL·10MB 제출 한도 | `crawling.sitemap_xml` `crawling.sitemap_entries` `crawling.sitemap_over_10mb` `crawling.sitemap_foreign_host` |
| seo-advanced-indexing, faq-serpremove | noindex, robots.txt 로 막으면 noindex 를 못 읽음 | `indexing.meta_noindex` `indexing.header_noindex` `indexing.noindex_blocked_by_robots` |
| structured-data-intro | JSON-LD·Microdata, 문법 오류 | `structured_data.json_ld` `structured_data.microdata` `structured_data.json_ld_invalid` |
| structured-data-* (13쪽) | 유형별 필수 속성 (Google 기준과 다름) | `structured_data.naver.postal_address` `structured_data.naver.breadcrumb_list` `structured_data.naver.carousel` `structured_data.naver.channel` `structured_data.naver.movie` `structured_data.naver.aggregate_rating` `structured_data.naver.recipe` `structured_data.naver.restaurant` `structured_data.naver.review` `structured_data.naver.software_app` `structured_data.naver.tv_series` `structured_data.naver.video` `structured_data.naver.video_channel` |
| indexnow-api-key | 키 파일 | `crawling.indexnow_key_file` (키를 지정했을 때만) |
| 확인 불가 | 웹마스터 도구 등록·수집 요청·리포트·URL 검사, IndexNow 전송, 콘텐츠 품질·어뷰징 판단, CSS 로 숨긴 텍스트, 백링크, 사이트 이전·폐쇄 절차, 검색로봇 IP 대역 | 콘솔·로그인·외부 데이터·의미 판단이 필요 |

## Bing 웹마스터 가이드라인

[Webmaster Guidelines](https://www.bing.com/webmasters/help/webmaster-guidelines-30fba23a) 와 관련 도움말 15편(robots meta, AI Preferences, IndexNow, 구조화 데이터, 404, robots.txt, sitemap, 사이트 인증, URL 차단, 크롤러 등)을 사람이 대조했습니다. 도움말은 스크립트로 그려져 Bing 의 도움말 API 응답으로 원문을 읽었습니다.

| 절 | 요구 사항 | 항목 id |
|---|---|---|
| §2·§3 | sitemap, 정식 URL 만, noindex·삭제·리다이렉트 URL 제외, lastmod | `crawling.sitemap_any` `crawling.sitemap_canonical_mismatch` `crawling.sitemap_noindex_pages` `crawling.sitemap_broken_urls` `crawling.sitemap_redirected_urls` `crawling.sitemap_lastmod` |
| §5·§6·§7 | 크롤링 가능한 링크, 중복 URL, 301/302 | `content.uncrawlable_links` `meta.canonical` `transport.url_variant.trailing_slash` `transport.url_variant.uppercase` `transport.url_variant.index_html` `transport.redirect` `transport.temporary_redirect` |
| §8 | Bingbot 허용·렌더링 자원 허용, 파라미터 많은 URL 지양 | `crawling.robots_allows.bingbot` `crawling.robots_blocks_resources_bingbot` `crawling.ua_access.bingbot` `content.parameter_heavy_links` |
| §9, 404 모범 사례 | 삭제된 페이지는 404, 404 페이지는 외부 호출 없이 | `not_found_page.returns_404` `not_found_page.custom` `not_found_page.third_party_calls` |
| §10, robots meta | noindex·nosnippet·max-snippet·data-snippet, Bing 전용 지시(meta bingbot) | `indexing.meta_noindex` `indexing.snippet_controls` `indexing.bingbot_meta` |
| §10 | Copilot 인용 제한 (nocache·noarchive) | `geo.copilot_restrictions` |
| AI Preferences FAQ | ai-training·ai-grounding 선호 표시 (meta·헤더·robots.txt) | `geo.bing_ai_preferences` |
| §12 | 대체 텍스트, 설명적인 이미지 파일명 | `content.img_alt_missing` `content.img_generic_filenames` |
| §13 | title·description, 헤딩 구조, 시맨틱 요소 | `meta.title` `meta.description` `site.duplicate_titles` `content.heading_skip` `accessibility.landmark_main` |
| 구조화 데이터 | JSON-LD·Microdata·RDFa·Microformats·Open Graph, 이름 없는 Person 무시, og:image socialonly | `structured_data.json_ld` `structured_data.microdata` `structured_data.rdfa` `structured_data.microformats` `structured_data.required.person` `open_graph.image_socialonly` |
| robots.txt 만들기 | UTF-8, Crawl-delay, Sitemap | `crawling.robots_encoding` `crawling.robots_crawl_delay` `crawling.robots_sitemap_directive` |
| 사이트 추가·인증 | msvalidate.01 meta, BingSiteAuth.xml | `meta.bing_site_verification` `meta.bing_site_auth_file` |
| IndexNow | 키 파일 | `crawling.indexnow_key_file` (키를 지정했을 때만) |
| 확인 불가 | URL 제출·IndexNow 전송, 콘텐츠 제거 도구, 크롤 속도, Bingbot IP 검증, 콘텐츠 품질·어뷰징 판단, DNS 인증, mRSS 제출, Bingbot TLS 암호군 | 콘솔·외부 호출·의미 판단·TLS 핸드셰이크 열거가 필요 |

## 검색엔진 문서 (2차)

| 출처 | 대조 수 | 새로 넣은 항목 |
|---|---|---|
| Google 검색 센터 문서 전체 | 살아 있는 문서 187쪽 (구조화 데이터 40쪽은 위 표) | `crawling.robots_rule_errors` `crawling.robots_unsupported_directives` `crawling.robots_allows.googlebot_image` `crawling.robots_blocks_favicon_googlebot` `crawling.sitemap_invalid_xml` `crawling.sitemap_future_lastmod` `crawling.sitemap_extensions` `crawling.sitemap_extension_errors` `crawling.sitemap_news_stale` `crawling.sitemap_blocked_by_robots` `meta.canonical_header` `meta.canonical_conflict` `meta.canonical_fragment` `meta.paginated_canonical_to_first` `structured_data.hreflang_invalid` `structured_data.hreflang_other_sources` `structured_data.hreflang_canonical_conflict` `crawling.over_googlebot_size_limit` `crawling.conditional_request_304` `transport.ua_redirect_mismatch` `indexing.robots_unknown_directives` `indexing.unavailable_after_passed` `indexing.robots_conflicting_directives` `indexing.data_nosnippet_invalid_element` `indexing.google_meta` `meta.rating` `meta.favicon_google_eligible` `meta.title_half_empty` `meta.title_script_mismatch` `content.img_missing_src` `content.img_unsupported_format` `open_graph.image_under_1200` `geo.future_dates` `structured_data.website_duplicate` `structured_data.site_name_mismatch` `structured_data.is_accessible_for_free` `content.hidden_text_styles` `content.unencoded_link_urls` `content.outbound_link_qualifiers` `content.underscore_url_paths` `crawling.rel_next_prev` |
| Yandex 웹마스터 도움말 | 176쪽 중 robots·meta·sitemap·canonical·favicon 등 약 25절 | `crawling.robots_allows.yandex` `crawling.robots_clean_param` `indexing.yandex_meta` `indexing.yandex_noindex_tag` `meta.yandex_site_verification` |
| Daum 웹문서 검색 고객센터 | FAQ 10쪽 | `crawling.ua_access.daum`, robots 토큰을 `Daum` 으로 수정한 `crawling.robots_allows.daum` |
| Apple (Applebot·앱 연결 문서) | 5쪽 | `crawling.robots_allows.applebot` `crawling.ua_access.applebot` `indexing.applebot_meta` `meta.apple_app_site_association_valid` `meta.apple_itunes_app_valid` |
| sitemaps.org·Common Crawl | 3쪽 | `crawling.sitemap_namespace` `crawling.sitemap_invalid_priority` `crawling.sitemap_invalid_changefreq` `crawling.sitemap_invalid_lastmod` `crawling.sitemap_unescaped_entities` `crawling.sitemap_invalid_loc` `crawling.sitemap_out_of_scope` `crawling.sitemap_encoding` `geo.ua_access.ccbot` |

robots.txt 해석은 Google 규칙에 맞췄습니다: 토큰 뒤 텍스트 무시(`Googlebot/2.1`), 가장 구체적인 그룹 대체(googlebot-image → googlebot), Crawl-delay 같은 비규칙 필드가 그룹을 끊지 않음, 429 를 5xx 처럼 취급. 별도 모바일 URL 의 메타 일치와 UA 별 동적 제공 비교는 모바일 UA 로 한 번 더 받아야 해 아직 넣지 않았습니다. 콘솔 기능, 스팸 의도 판단, 렌더링 후 상태는 확인할 수 없습니다.

## AI·GEO 프로토콜

| 출처 | 대조 수 | 새로 넣은 항목 |
|---|---|---|
| AI 크롤러 공식 문서 (OpenAI·Google·Meta·Amazon·Mistral·DuckDuckGo·Ai2·Webz.io) | 토큰 13개 | `geo.robots_allows.google_cloudvertexbot` `geo.robots_allows.googleother` `geo.robots_allows.meta_webindexer` `geo.robots_allows.oai_adsbot` `geo.robots_allows.amzn_searchbot` `geo.robots_allows.amzn_user` `geo.robots_allows.mistralai_user` `geo.robots_allows.mistralai_index` `geo.robots_allows.mistralai_training` `geo.robots_allows.duckassistbot` `geo.robots_allows.ai2bot` `geo.robots_allows.webzio_extended` (Applebot 은 검색 크롤러로 분류) |
| ai.robots.txt 공동체 목록 | 209개 | 공식 문서로 확인한 토큰만 넣음 (나머지 192개는 출처 미확인) |
| llmstxt.org | 형식 규칙 전체 | `geo.llms_txt_link_format` `geo.llms_txt_extra_headings` `geo.llms_txt_optional_section` `geo.llms_txt_describedby` `geo.llms_txt_subpath` |
| IETF aipref·Content Signals·RSL·TDMRep·A2A | 5개 명세 | `geo.content_usage` `geo.content_signals` `geo.rsl_license` `geo.tdm_policy` `geo.tdm_reservation_invalid` `geo.a2a_agent_card` |
| RFC 9309 | 규칙 전체 | `crawling.robots_content_type` `crawling.robots_orphan_rules` `crawling.robots_unreachable` `crawling.robots_redirect_hops` |
| GEO 논문 (Aggarwal 외, KDD 2024) | 방법 9개 | `geo.quotations` `geo.statistics` (출처 인용은 `content.external_links`) |

MCP 서버 발견(`/.well-known/mcp.json`)은 표준이 확정되지 않았고, NLWeb 은 공식 발견 방법이 없으며, ChatGPT 플러그인 manifest 는 폐지되어 넣지 않았습니다. C2PA 는 이미지 파일을 받아 해석해야 합니다.

## 보안 스캐너

[Mozilla HTTP Observatory](https://github.com/mdn/mdn-http-observatory) 의 테스트 12개(결과 코드 77개), [webhint](https://github.com/webhintio/hint) 의 hint 50개, RFC 9116 security.txt 필드 10개, IANA well-known URI 107개, hstspreload.org 제출 요건 6개를 대조했습니다. Observatory 와 webhint 목록은 `scripts/check_coverage.py` 가 저장소와 다시 대조합니다. securityheaders.com 은 비공개라 공개 글에 나온 경고 5개만 대조했습니다.

| Observatory 테스트 | 항목 id | 비고 |
|---|---|---|
| cookies | `security.insecure_cookies` `security.cookie_samesite_invalid` `security.session_cookie_flags` `security.cookie_prefix_invalid` `security.cookie_syntax_invalid` | 리다이렉트 경유 응답의 쿠키는 보지 않음 |
| cors | `security.cors_wildcard` | Origin 을 반사하는 자격증명 허용은 별도 요청이 필요해 미구현 |
| cross-origin-embedder-policy | `security.cross_origin_embedder_policy` `security.cross_origin_policy_invalid` | — |
| cross-origin-opener-policy | `security.cross_origin_opener_policy` `security.cross_origin_policy_invalid` | — |
| cross-origin-resource-policy | `security.cross_origin_resource_policy` `security.cross_origin_policy_invalid` | — |
| csp | `security.csp` `security.csp_meta` `security.csp_unsafe_inline` `security.csp_unsafe_eval` `security.csp_broad_script_source` `security.csp_object_src_unrestricted` `security.csp_unsafe_inline_style` `security.csp_insecure_scheme` `security.csp_base_uri_unrestricted` `security.csp_form_action_unrestricted` `security.csp_default_src_missing` `security.csp_strict_dynamic_without_nonce` `security.csp_duplicate_directives` `security.csp_report_only_only` `security.trusted_types` | — |
| redirection | `transport.http_to_https_redirect` `transport.hsts_preload_ineligible` | http 요청의 첫 단계가 같은 호스트 https 인지는 preload 요건 안에서만 확인 |
| referrer-policy | `security.referrer_policy` `security.referrer_policy_unsafe` | — |
| strict-transport-security | `transport.hsts` `transport.hsts_invalid` `transport.hsts_max_age_short` `transport.hsts_preload_ineligible` | Chromium preload 목록 등재 여부 (외부 목록) |
| subresource-integrity | `security.sri_missing` `security.sri_invalid` | — |
| x-content-type-options | `security.x_content_type_options` `security.x_content_type_options_invalid` | — |
| x-frame-options | `security.x_frame_options` `security.x_frame_options_invalid` `security.clickjacking_unprotected` | — |

| webhint hint | 항목 id | 비고 |
|---|---|---|
| apple-touch-icons | `meta.apple_touch_icon` `meta.apple_touch_icon_file` | PNG·180px 확인은 미구현 (이미지 디코드 필요) |
| axe | [axe-core 규칙](#axe-core-규칙) 표 | — |
| babel-config | — | 확인 불가: 빌드 설정 파일 |
| button-type | `content.button_type_missing` | — |
| compat-api | — | 확인 불가: 대상 브라우저 설정과 호환성 데이터 필요 |
| content-type | `transport.content_type_invalid` `transport.asset_content_type_mismatch` | — |
| create-element-svg | — | 확인 불가: JS 소스 린트 |
| css-prefix-order | `performance.css_prefix_order` | — |
| detect-css-reflows | `performance.layout_animations` | — |
| disown-opener | `security.target_blank_noopener` | — |
| doctype | `meta.doctype` | — |
| highest-available-document-mode | `meta.x_ua_compatible` | — |
| html-checker | `accessibility.markup_errors` | 외부 Nu 서비스 (정적 부분은 [HTML 검사기](#html-검사기성능) 표) |
| http-cache | `performance.cache_control` `performance.static_asset_cache` `performance.cache_control_conflict` `crawling.script_cache_busting` | — |
| http-compression | `performance.compression` `performance.asset_compression` `performance.vary_accept_encoding_missing` | — |
| https-only | `transport.https` `transport.mixed_content` `transport.https_to_http_redirect` | — |
| ie-flexbox-compat | — | 해당 없음: IE 전용 |
| image-optimization-cloudinary | — | 확인 불가: 외부 API |
| leading-dot-classlist | — | 확인 불가: JS 소스 린트 |
| manifest-app-name | `meta.manifest_name_length` | — |
| manifest-exists | `meta.manifest` `links.manifest` | — |
| manifest-file-extension | `meta.manifest_file_extension` | — |
| manifest-icons | `meta.manifest_icons_192_512` `meta.manifest_maskable_icon` | 아이콘 실제 크기 (이미지 디코드) |
| manifest-is-valid | `meta.manifest_content` `meta.manifest_display_valid` | — |
| manifest-scoped-urls | `meta.manifest_start_url_scope` | — |
| meta-charset-utf-8 | `meta.charset_not_utf8` `meta.charset_late` `meta.charset_duplicate` | — |
| meta-theme-color | `meta.theme_color` `meta.theme_color_invalid` | — |
| meta-viewport | `meta.viewport` `meta.viewport_invalid` `accessibility.zoom_disabled` | — |
| minified-js | `performance.unminified_js` | — |
| no-bom | `meta.bom` | — |
| no-broken-links | `content.broken_internal_links` `content.broken_resources` | — |
| no-disallowed-headers | `security.server_header` `security.x_powered_by` `security.info_disclosure_headers` `security.hpkp` `security.p3p` | Expires·Host·X-Frame-Options 금지 권고는 OWASP·Observatory 와 충돌해 따르지 않음 |
| no-friendly-error-pages | — | 해당 없음: IE 전용 (`not_found_page.custom` 참고) |
| no-html-only-headers | — | 미구현: 하위 리소스 응답 헤더 표본이 필요 |
| no-http-redirects | `transport.redirect` `performance.asset_redirects` | — |
| no-inline-styles | `performance.inline_style_attrs` `performance.inline_code_bytes` | — |
| no-p3p | `security.p3p` | /w3c/p3p.xml 파일은 요청하지 않음 |
| no-protocol-relative-urls | `security.protocol_relative_urls` | — |
| no-vulnerable-javascript-libraries | — | 보류: 취약점 DB(retire.js 등)를 내장하고 갱신하는 설계가 필요 |
| performance-budget | `performance.page_weight` `performance.html_bytes` | 네트워크 모델 기반 판정은 하지 않음 |
| scoped-svg-styles | — | 확인 불가: CSS 선택자 적용 범위 계산 |
| sri | `security.sri_missing` `security.sri_invalid` | 해시 일치 확인은 하지 않음 |
| ssllabs | — | 확인 불가: 외부 API |
| strict-transport-security | `transport.hsts` `transport.hsts_invalid` `transport.hsts_max_age_short` | — |
| stylesheet-limits | — | 해당 없음: IE 전용 |
| summary-display | `accessibility.summary_display_changed` | — |
| typescript-config | — | 확인 불가: 빌드 설정 파일 |
| validate-set-cookie-header | `security.cookie_syntax_invalid` `security.cookie_prefix_invalid` `security.cookie_samesite_invalid` | — |
| webpack-config | — | 확인 불가: 빌드 설정 파일 |
| x-content-type-options | `security.x_content_type_options` `security.x_content_type_options_invalid` | — |

RFC 9116·IANA: `transport.security_txt` `transport.security_txt_fields`(10개 필드 값) `transport.security_txt_expired` `transport.security_txt_invalid` `transport.security_txt_signed` `transport.security_txt_canonical_mismatch` `security.change_password_url` `trust.gpc`. 서명 검증과 키 신뢰는 확인 불가입니다. IANA 목록의 나머지(openid-configuration·webfinger·mta-sts 등)는 일반 웹사이트의 존재 신호가 아니라서 넣지 않았습니다.

## 접근성 검사기

사람이 대조한 출처와 규칙 수입니다. 새로 넣은 항목은 아래와 같고, 나머지는 기존 항목으로 다루거나 렌더링·실행·의미 판단이 필요합니다.

| 출처 | 대조 수 | 새로 넣은 항목 |
|---|---|---|
| IBM Equal Access | 규칙 170개 | `accessibility.aria_role_redundant` `accessibility.aria_role_not_allowed` `accessibility.aria_attr_not_allowed_for_role` `accessibility.aria_html_attr_conflict` `accessibility.aria_html_attr_redundant` `accessibility.aria_deprecated_attr` `accessibility.presentational_children_roles` `accessibility.duplicate_landmark_names` `accessibility.landmark_role_unnamed` `accessibility.activedescendant_unfocusable` `accessibility.combobox_pattern_invalid` `accessibility.tabbable_non_interactive` `accessibility.iframe_tabindex_negative` `accessibility.empty_labels` `accessibility.placeholder_only_label` `accessibility.form_submit_missing` `accessibility.decorative_img_conflict` `accessibility.figure_unlabeled` `accessibility.canvas_fallback_missing` `accessibility.mouse_only_handlers` `accessibility.css_blink` |
| HTML_CodeSniffer (pa11y) | 메시지 179개 | `accessibility.presentation_semantic_children` `accessibility.label_for_not_control` `accessibility.alt_redundant_with_text` `accessibility.adjacent_duplicate_links` `accessibility.broken_fragment_links` `accessibility.xml_lang_without_lang` `accessibility.ruby_without_rt` |
| ARIA in HTML | 요소 139개·속성 11개 | `accessibility.aria_attr_on_prohibited_element` 및 위 역할·속성 항목 |
| W3C i18n checker | 보고 51종 | `meta.bom_in_content` `meta.xml_declaration` `meta.content_language_meta` `accessibility.dir_invalid` `accessibility.rtl_dir_missing` `accessibility.css_direction` `accessibility.bdo_without_dir` `content.bidi_control_chars` `accessibility.translate_invalid` `meta.charset_not_utf8` `meta.charset_late` |
| WAVE | 항목 110개 | `accessibility.redundant_title_attr` `content.linked_documents` (나머지는 기존 항목) |
| EN 301 549 V3.2.1 | 9장 외 조항 71개 | `accessibility.statement_link` `accessibility.media_controls_missing` `structured_data.accessibility_metadata` (9장은 WCAG 2.2 표) |

EU 웹 접근성 지침 제7조의 피드백 수단(접근성 성명 페이지 안의 연락 수단)은 성명 페이지를 한 번 더 받아야 해 아직 넣지 않았습니다.

## HTML 검사기·성능

[HTMLHint](https://github.com/htmlhint/HTMLHint) 규칙 44개 전체입니다 (`scripts/check_coverage.py` 가 저장소와 다시 대조). Nu HTML Checker 는 `Assertions.java` 의 메시지 251개와 금지 조상 규칙을 사람이 대조했고, 정적으로 값싸게 확인할 수 있는 것을 `meta.http_equiv_invalid` `meta.base_after_resources` `accessibility.invalid_nesting` `accessibility.phrasing_content_violation` `accessibility.form_conformance_errors` `content.invalid_urls` `content.invalid_enum_attrs` `content.time_datetime_invalid` `content.unknown_elements` `content.text_not_nfc` `structured_data.microdata_invalid` `content.section_without_heading` `performance.empty_src` `performance.srcset_sizes_invalid` `performance.preload_invalid` `performance.script_attr_ineffective` `performance.script_json_invalid` 로 넣었습니다. RNC 내용 모델 전체 검증, XHTML 전용 규칙, 암묵 역할 경고(axe 와 같은 이유로 미적용)는 넣지 않았습니다. Chrome DevTools 인사이트와 Lighthouse 에서 빠진 감사는 `performance.excess_preconnect` `performance.preconnect_unused` `performance.preconnect_crossorigin_missing` `performance.third_party_embeds` `performance.legacy_javascript` `performance.unminified_js` `performance.asset_compression` `performance.speculation_rules` `performance.link_header_preload` `performance.server_timing` 로 다룹니다.

| HTMLHint 규칙 | 항목 id | 비고 |
|---|---|---|
| alt-require | `content.img_alt_missing` `accessibility.area_alt_missing` | — |
| attr-no-duplication | `accessibility.markup_errors` | — |
| doctype-first | `meta.doctype` | — |
| doctype-html5 | `meta.doctype` | — |
| frame-title-require | `accessibility.iframe_title_missing` | — |
| h1-require | `content.h1` | — |
| head-script-disabled | `performance.render_blocking_scripts` | — |
| html-lang-require | `meta.html_lang` | — |
| id-unique | `accessibility.duplicate_ids` | — |
| input-requires-label | `accessibility.unlabeled_form_controls` | — |
| link-rel-canonical-require | `meta.canonical` | — |
| main-require | `accessibility.landmark_main` | — |
| meta-charset-require | `meta.charset` | — |
| meta-description-require | `meta.description` | — |
| meta-viewport-require | `meta.viewport` | — |
| tag-no-obsolete | `accessibility.obsolete_elements` | — |
| tag-pair | `accessibility.markup_errors` | — |
| title-require | `meta.title` | — |
| button-type-require | `content.button_type_missing` `content.invalid_enum_attrs` | — |
| form-method-require | `content.form_method_missing` | — |
| src-not-empty | `performance.empty_src` | — |
| inline-style-disabled | `performance.inline_style_attrs` | — |
| inline-script-disabled | `security.inline_event_handlers` `content.pseudo_js_links` | — |
| id-class-ad-disabled | `content.ad_blocker_class_names` | — |
| attr-unsafe-chars | `content.attr_unsafe_chars` | — |
| tagname-specialchars | `content.unknown_elements` | — |
| script-disabled | `performance.inline_code_bytes` | 정책 토글이라 존재만 보고 |
| style-disabled | `performance.inline_code_bytes` | 정책 토글이라 존재만 보고 |
| tags-check | — | 기본 설정이 a·img 에 title 을 요구해 접근성 지침과 충돌 |
| attr-lowercase | — | 해당 없음: 렌더링에 영향 없는 소스 서식 규칙 |
| attr-sorted | — | 해당 없음: 렌더링에 영향 없는 소스 서식 규칙 |
| attr-value-double-quotes | — | 해당 없음: 렌더링에 영향 없는 소스 서식 규칙 |
| attr-value-single-quotes | — | 해당 없음: 렌더링에 영향 없는 소스 서식 규칙 |
| attr-whitespace | — | 해당 없음: 렌더링에 영향 없는 소스 서식 규칙 |
| attr-no-unnecessary-whitespace | — | 해당 없음: 렌더링에 영향 없는 소스 서식 규칙 |
| attr-value-not-empty | — | 해당 없음: 렌더링에 영향 없는 소스 서식 규칙 |
| attr-value-no-duplication | — | 해당 없음: 렌더링에 영향 없는 소스 서식 규칙 |
| empty-tag-not-self-closed | — | 해당 없음: 렌더링에 영향 없는 소스 서식 규칙 |
| tag-self-close | — | 해당 없음: 렌더링에 영향 없는 소스 서식 규칙 |
| tagname-lowercase | — | 해당 없음: 렌더링에 영향 없는 소스 서식 규칙 |
| space-tab-mixed-disabled | — | 해당 없음: 렌더링에 영향 없는 소스 서식 규칙 |
| spec-char-escape | — | 해당 없음: 렌더링에 영향 없는 소스 서식 규칙 |
| href-abs-or-rel | — | 해당 없음: 렌더링에 영향 없는 소스 서식 규칙 |
| id-class-value | — | 해당 없음: 렌더링에 영향 없는 소스 서식 규칙 |

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
