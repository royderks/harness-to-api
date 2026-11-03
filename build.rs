fn main() {
    println!("cargo:rerun-if-env-changed=H2A_VERSION");
    let version = std::env::var("H2A_VERSION")
        .unwrap_or_else(|_| std::env::var("CARGO_PKG_VERSION").unwrap());
    assert!(
        version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    );
    println!("cargo:rustc-env=H2A_VERSION={version}");
}
