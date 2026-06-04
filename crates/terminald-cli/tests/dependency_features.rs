#[test]
fn websocket_dependency_enables_rustls_with_system_roots() {
    let manifest = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));

    assert!(
        manifest.contains("tokio-tungstenite") && manifest.contains("rustls-tls-native-roots"),
        "terminald-cli must enable tokio-tungstenite rustls TLS with system certificate roots"
    );
    assert!(
        manifest.contains("rustls") && manifest.contains("ring"),
        "terminald-cli must select a rustls crypto provider"
    );
}

#[test]
fn rustls_crypto_provider_is_available() {
    let _ = rustls::ClientConfig::builder();
}
