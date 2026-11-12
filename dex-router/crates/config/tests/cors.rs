use xone_config::load;
#[test]
fn cors_origins_are_explicit_bounded_and_cannot_contain_path_or_credentials() {
    let c = load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap();
    for origin in ["http://127.0.0.1:5173", "https://quotes.example.com"] {
        let mut v = c.config.clone();
        v.api.cors_allowed_origins = vec![origin.into()];
        v.validate("dex-router").unwrap();
    }
    for origin in [
        "*",
        "null",
        "https://example.com/path",
        "http://user:pass@example.com",
        "https://example.com?token=a",
        "https://a\r\nb",
        "file://localhost",
    ] {
        let mut v = c.config.clone();
        v.api.cors_allowed_origins = vec![origin.into()];
        assert!(v.validate("dex-router").is_err(), "{origin}");
    }
    let mut v = c.config;
    v.api.cors_allowed_origins = vec!["https://a.example".into(); 33];
    assert!(v.validate("dex-router").is_err());
}
