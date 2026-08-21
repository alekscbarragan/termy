use crate::{
    AgentCommand, AgentKey, AttachmentConflict, CatalogSnapshot, ConnectionState, ControlOwnership,
    CreateAgentFailure, RequestId, SpaceId,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AttachmentId(u64);

impl AttachmentId {
    #[cfg(any(test, feature = "test-support"))]
    pub fn new(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateAgentRequest {
    pub request_id: RequestId,
    pub space: SpaceId,
    pub command: AgentCommand,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachAgentRequest {
    pub request_id: RequestId,
    pub agent: AgentKey,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetachAgentRequest {
    pub request_id: RequestId,
    pub agent: AgentKey,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloseAgentRequest {
    pub request_id: RequestId,
    pub agent: AgentKey,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentInput {
    pub attachment: AttachmentId,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentOutput {
    pub attachment: AttachmentId,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransportCommand {
    CreateAgent(CreateAgentRequest),
    RequestAttach(AttachAgentRequest),
    AttachmentInput(AttachmentInput),
    Detach(DetachAgentRequest),
    CloseAgent(CloseAgentRequest),
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the real transport constructs attachment results in P9"
    )
)]
pub enum TransportAttachResult {
    Writable(AttachmentId),
    Conflict(AttachmentConflict),
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the real transport constructs notifications in P9"
    )
)]
pub enum TransportNotification {
    CatalogSnapshot(CatalogSnapshot),
    AttachmentOutput(AttachmentOutput),
    ControlChanged {
        agent: AgentKey,
        ownership: ControlOwnership,
    },
    ConnectionChanged(ConnectionState),
    CreateResolved {
        request_id: RequestId,
        result: Result<AgentKey, CreateAgentFailure>,
    },
    AttachResolved {
        request_id: RequestId,
        result: TransportAttachResult,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransportError;

pub trait HerdrTransport: Send {
    fn send(&mut self, command: TransportCommand) -> Result<(), TransportError>;

    fn try_recv(&mut self) -> Result<Option<TransportNotification>, TransportError>;
}
