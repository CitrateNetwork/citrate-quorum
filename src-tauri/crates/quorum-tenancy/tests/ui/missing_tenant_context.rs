// A domain whose read is correctly scoped: it requires &TenantContext.
use quorum_tenancy::{Scoped, TenantContext, TenantId};

struct RoomsDomain;

impl Scoped for RoomsDomain {}

impl RoomsDomain {
    // The blessed shape: every tenant-scoped call takes the context.
    fn list<'a>(&self, ctx: &'a TenantContext) -> &'a TenantId {
        self.tenant(ctx)
    }
}

fn main() {
    let d = RoomsDomain;
    // ERROR: calling a tenant-scoped method WITHOUT a TenantContext.
    // This is the mistake the type system must reject at compile time.
    let _ = d.list();
}
