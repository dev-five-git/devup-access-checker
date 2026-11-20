//! `DEVUP_ACCESS_CHECKER_TRACE` 가 켜지면 요청마다 로그를 남긴다 (별도 프로세스에서 확인).
use devup_access_checker::{Checker, Options};

/// 기본 옵션에서 일부만 바꾼다 (`Options` 는 `#[non_exhaustive]`).
fn opts(f: impl FnOnce(&mut Options)) -> Options {
    let mut o = Options::default();
    f(&mut o);
    o
}

#[tokio::test]
async fn trace_logging_does_not_change_results() {
    // SAFETY: 이 테스트 바이너리에는 테스트가 하나뿐이라 다른 스레드가 환경 변수를 읽지 않는다.
    unsafe { std::env::set_var("DEVUP_ACCESS_CHECKER_TRACE", "1") };
    let opts = opts(|o| {
        o.check_transport = false;
        o.check_ai_user_agents = false;
        o.check_links = false;
        o.check_resources = false;
        o.page_samples = 0;
    });
    // 성공 응답과 연결 실패 모두 로그 경로를 지난다
    let s = wiremock::MockServer::start().await;
    let r = Checker::new(opts.clone()).check(&s.uri()).await.unwrap();
    assert_eq!(r.status_code, 404);
    assert!(
        Checker::new(opts)
            .check("http://127.0.0.1:9/")
            .await
            .is_err()
    );
}
