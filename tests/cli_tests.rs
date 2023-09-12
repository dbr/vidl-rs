#[test]
fn cli_tests() {
    let tmp = tempfile::tempdir().unwrap();
    trycmd::TestCases::new()
        .case("tests/cli_tests.md")
        .env("VIDL_CONFIG_DIR", tmp.path().to_str().unwrap())
        ;
}
