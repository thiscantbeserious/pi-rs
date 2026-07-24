//! Integration tests. The CI coverage job runs `--lib --test integration`.

#[test]
fn library_links() {
    assert_eq!(pi_rs::version(), env!("CARGO_PKG_VERSION"));
}
