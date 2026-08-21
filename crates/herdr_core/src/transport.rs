use crate::{
    AgentCommand, AgentKey, CatalogSnapshot, ConnectionState, ControlOwnership, RequestId, SpaceId,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AttachmentId(u64);

impl AttachmentId {
    pub fn new(value: u64) -> Self {
        Self(value)
    }
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
    CreateAgent {
        request_id: RequestId,
        space: SpaceId,
        command: AgentCommand,
    },
    Attach {
        request_id: RequestId,
        agent: AgentKey,
    },
    WriteInput(AttachmentInput),
    Detach {
        request_id: RequestId,
        agent: AgentKey,
    },
    CloseAgent {
        request_id: RequestId,
        agent: AgentKey,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransportNotification {
    CatalogSnapshot(CatalogSnapshot),
    AttachmentOutput(AttachmentOutput),
    ControlChanged {
        agent: AgentKey,
        ownership: ControlOwnership,
    },
    ConnectionChanged(ConnectionState),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransportError;

pub trait HerdrTransport: Send {
    fn send(&mut self, command: TransportCommand) -> Result<(), TransportError>;

    fn try_recv(&mut self) -> Result<Option<TransportNotification>, TransportError>;
}
