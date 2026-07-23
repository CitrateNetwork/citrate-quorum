//! WP-S1.4 — the compile-fail proof. A domain call that omits the
//! `TenantContext` must be a build error, not a review comment. `trybuild`
//! compiles each `tests/ui/*.rs` and asserts it fails with the expected message.
#[test]
fn tenant_less_domain_call_is_rejected() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/missing_tenant_context.rs");
}
