mod controller;
mod domain;
#[cfg(any(test, feature = "test-support"))]
mod fake;
mod session;
mod transport;
mod trust;

#[cfg(feature = "test-support")]
pub use fake::{FakeAgentCatalogRow, FakeHerdrHandle, FakeSpaceCatalogRow, fake_controller};

pub use controller::{
    AttachResult, AttachTicket, AttachmentConflict, AttachmentUnavailable, CreateAgentFailure,
    HerdrController, MutationFailure, WritableAttachment,
};
pub use domain::{
    AgentCommand, AgentId, AgentKey, AgentPhase, Confirmed, ConflictId, RequestId, SpaceId,
};
pub use session::{
    AgentCatalogEntry, CatalogMirror, CatalogRevision, CatalogSnapshot, ConnectionState,
    ControlOwnership, HerdrEvent, SessionError, SpaceCatalogEntry,
};
pub use trust::{
    BUNDLED_HERDR_SERVICE_LOCATION, DOCUMENTED_HERDR_INSTALL_LOCATIONS, HerdrServiceLocation,
    TrustRejection, TrustRejectionReason, TrustedLocation,
};
