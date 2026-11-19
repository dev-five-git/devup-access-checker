//! 실제 바이너리의 진입점(인자 파싱, 종료 코드)을 확인한다.
use std::process::Command;

#[test]
fn binary_reports_failure_exit_code() {
    let out = Command::new(env!("CARGO_BIN_EXE_devup-access-checker"))
        .args([
            "--no-ai-ua",
            "--no-transport",
            "--pages",
            "0",
            "--timeout",
            "5",
            "http://127.0.0.1:9",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("error:"));
}
