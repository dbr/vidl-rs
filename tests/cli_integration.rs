#[test]
fn cli_integration() {
    let tmp = tempfile::tempdir().unwrap();
    trycmd::TestCases::new()
        .case("tests/cli_integration.md")
        .env("VIDL_CONFIG_DIR", tmp.path().to_str().unwrap());
}
