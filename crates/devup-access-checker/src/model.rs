use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 항목의 존재 여부.
///
/// 이 라이브러리는 "있다/없다" 만 판정한다. 좋고 나쁨(점수·등급)은 판단하지 않는다.
/// `Present` 는 항목 이름이 가리키는 대상이 **존재한다**는 뜻이다.
/// 예를 들어 `transport.mixed_content` 가 `Present` 면 혼합 콘텐츠가 발견되었다는 의미다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Presence {
    /// 대상이 존재함
    Present,
    /// 대상이 없음
    Missing,
    /// 판별할 수 없음 (네트워크 오류, CSR 셸 등)
    Unknown,
    /// 전제 조건이 없어 검사 대상이 아님 (예: 링크가 없어 접근성 검증 불가)
    NotApplicable,
}

impl Presence {
    pub fn from_bool(b: bool) -> Self {
        if b { Self::Present } else { Self::Missing }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// 기본 메타
    Meta,
    /// 색인 제어
    Indexing,
    /// Open Graph
    OpenGraph,
    /// Twitter Card
    Twitter,
    /// Facebook
    Facebook,
    /// 구조화 데이터
    StructuredData,
    /// 본문 구조
    Content,
    /// 접근성
    Accessibility,
    /// 분석 도구
    Analytics,
    /// 성능
    Performance,
    /// 전송
    Transport,
    /// 보안 헤더
    Security,
    /// 신뢰·법적 정보
    Trust,
    /// 여러 페이지 표본
    Site,
    /// 크롤링
    Crawling,
    /// 생성형 검색(GEO)
    Geo,
    /// 404 페이지
    NotFoundPage,
    /// 링크 검증
    Links,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Self::Meta => "기본 메타",
            Self::Indexing => "색인 제어",
            Self::OpenGraph => "Open Graph",
            Self::Twitter => "Twitter Card",
            Self::Facebook => "Facebook",
            Self::StructuredData => "구조화 데이터",
            Self::Content => "본문 구조",
            Self::Accessibility => "접근성",
            Self::Analytics => "분석 도구",
            Self::Performance => "성능",
            Self::Transport => "전송",
            Self::Security => "보안 헤더",
            Self::Trust => "신뢰·법적 정보",
            Self::Site => "여러 페이지 표본",
            Self::Crawling => "크롤링",
            Self::Geo => "생성형 검색(GEO)",
            Self::NotFoundPage => "404 페이지",
            Self::Links => "링크 검증",
        }
    }
}

/// 개별 점검 항목 결과.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Check {
    /// 안정적인 식별자 (예: `meta.title`). 버전이 바뀌어도 의미가 유지된다.
    pub id: String,
    pub category: Category,
    /// 사람이 읽는 항목명
    pub label: String,
    pub presence: Presence,
    /// 수집한 원시 값 (문자열, 숫자, 목록, 객체)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    /// 사실 기반 설명 (판단·권고 문구 없음)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// 하나의 URL 에 대한 전체 점검 결과.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    /// 정규화된 요청 URL
    pub url: String,
    /// 리다이렉트 후 최종 URL
    pub final_url: String,
    pub status_code: u16,
    pub duration_ms: u64,
    /// 대상 페이지가 오류 응답(비 2xx)이거나 봇 차단 페이지인 경우 그 사유.
    /// 이때 페이지 HTML·헤더에서 나온 항목은 모두 `unknown` 이다.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<String>,
    pub checks: Vec<Check>,
}

impl Report {
    pub fn get(&self, id: &str) -> Option<&Check> {
        self.checks.iter().find(|c| c.id == id)
    }

    pub fn missing(&self) -> impl Iterator<Item = &Check> {
        self.checks
            .iter()
            .filter(|c| c.presence == Presence::Missing)
    }
}
