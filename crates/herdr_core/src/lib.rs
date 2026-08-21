mod domain;
#[cfg(test)]
mod fake;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the P3 session seam has no production caller until P4"
    )
)]
mod session;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the P3 transport seam has no production caller until P4"
    )
)]
mod transport;
mod trust;

pub use domain::{
    AgentCommand, AgentId, AgentKey, AgentPhase, Confirmed, ConflictId, RequestId, SpaceId,
};
pub use session::{
    AgentCatalogEntry, CatalogMirror, CatalogRevision, CatalogSnapshot, ConnectionState,
    ControlOwnership, SpaceCatalogEntry,
};
pub use trust::{
    BUNDLED_HERDR_SERVICE_LOCATION, DOCUMENTED_HERDR_INSTALL_LOCATIONS, HerdrServiceLocation,
    TrustRejection, TrustRejectionReason, TrustedLocation,
};
