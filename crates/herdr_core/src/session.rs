use crate::{AgentId, AgentKey, AgentPhase, SpaceId};

use crate::transport::{
    AttachmentOutput, HerdrTransport, TransportAttachResult, TransportNotification,
};

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct CatalogRevision(u64);

impl CatalogRevision {
    pub fn get(self) -> u64 {
        self.0
    }

    fn checked_next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlOwnership {
    Unowned,
    ThisClient,
    AnotherClient,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ConnectionState {
    Connected,
    #[default]
    Disconnected,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HerdrEvent {
    CatalogChanged(CatalogRevision),
    AttachmentOutput {
        agent: AgentKey,
        bytes: Vec<u8>,
    },
    ControlChanged {
        agent: AgentKey,
        ownership: ControlOwnership,
    },
    ConnectionChanged(ConnectionState),
    CreateResolved {
        request_id: crate::RequestId,
        result: Result<AgentKey, crate::CreateAgentFailure>,
    },
    AttachmentChanged(crate::RequestId),
    SessionFailed(SessionError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentCatalogEntry {
    id: AgentId,
    phase: AgentPhase,
    control: ControlOwnership,
}

impl AgentCatalogEntry {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the real transport constructs catalog entries in P9"
        )
    )]
    pub(crate) fn new(id: AgentId, phase: AgentPhase, control: ControlOwnership) -> Self {
        Self { id, phase, control }
    }

    pub fn id(&self) -> &AgentId {
        &self.id
    }

    pub fn phase(&self) -> AgentPhase {
        self.phase
    }

    pub fn control(&self) -> ControlOwnership {
        self.control
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpaceCatalogEntry {
    id: SpaceId,
    agents: Vec<AgentCatalogEntry>,
}

impl SpaceCatalogEntry {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the real transport constructs catalog entries in P9"
        )
    )]
    pub(crate) fn new(id: SpaceId, agents: Vec<AgentCatalogEntry>) -> Self {
        Self { id, agents }
    }

    pub fn id(&self) -> &SpaceId {
        &self.id
    }

    pub fn agents(&self) -> &[AgentCatalogEntry] {
        &self.agents
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CatalogSnapshot {
    spaces: Vec<SpaceCatalogEntry>,
}

impl CatalogSnapshot {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the real transport constructs catalog snapshots in P9"
        )
    )]
    pub(crate) fn new(spaces: Vec<SpaceCatalogEntry>) -> Self {
        Self { spaces }
    }

    pub fn spaces(&self) -> &[SpaceCatalogEntry] {
        &self.spaces
    }

    #[cfg(test)]
    pub(crate) fn remove_agent(&mut self, key: &AgentKey) -> bool {
        let Some(space) = self.spaces.iter_mut().find(|space| space.id == key.space) else {
            return false;
        };
        let original_len = space.agents.len();
        space.agents.retain(|agent| agent.id != key.agent);
        space.agents.len() != original_len
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CatalogMirror {
    revision: CatalogRevision,
    snapshot: CatalogSnapshot,
    connection: ConnectionState,
}

impl CatalogMirror {
    pub fn revision(&self) -> CatalogRevision {
        self.revision
    }

    pub fn snapshot(&self) -> &CatalogSnapshot {
        &self.snapshot
    }

    pub fn connection(&self) -> ConnectionState {
        self.connection
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionError {
    TransportUnavailable,
    RevisionExhausted,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum SessionUpdate {
    Event(HerdrEvent),
    AttachmentOutput(AttachmentOutput),
    AttachResolved {
        request_id: crate::RequestId,
        result: TransportAttachResult,
    },
}

pub(crate) struct SessionDrain {
    pub updates: Vec<SessionUpdate>,
    pub error: Option<SessionError>,
}

#[derive(Default)]
pub struct SessionState {
    mirror: CatalogMirror,
}

impl SessionState {
    pub fn catalog(&self) -> &CatalogMirror {
        &self.mirror
    }

    pub fn drain(&mut self, transport: &mut dyn HerdrTransport) -> SessionDrain {
        let mut updates = Vec::new();
        loop {
            let notification = match transport.try_recv() {
                Ok(Some(notification)) => notification,
                Ok(None) => break,
                Err(_) => {
                    return SessionDrain {
                        updates,
                        error: Some(SessionError::TransportUnavailable),
                    };
                }
            };
            match self.apply(notification) {
                Ok(update) => updates.push(update),
                Err(error) => {
                    return SessionDrain {
                        updates,
                        error: Some(error),
                    };
                }
            }
        }
        SessionDrain {
            updates,
            error: None,
        }
    }

    fn apply(
        &mut self,
        notification: TransportNotification,
    ) -> Result<SessionUpdate, SessionError> {
        match notification {
            TransportNotification::CatalogSnapshot(snapshot) => {
                let revision = self.next_revision()?;
                self.mirror.snapshot = snapshot;
                self.mirror.revision = revision;
                Ok(SessionUpdate::Event(HerdrEvent::CatalogChanged(revision)))
            }
            TransportNotification::AttachmentOutput(output) => {
                Ok(SessionUpdate::AttachmentOutput(output))
            }
            TransportNotification::ControlChanged { agent, ownership } => {
                self.update_control(&agent, ownership)?;
                Ok(SessionUpdate::Event(HerdrEvent::ControlChanged {
                    agent,
                    ownership,
                }))
            }
            TransportNotification::ConnectionChanged(connection) => {
                self.mirror.connection = connection;
                Ok(SessionUpdate::Event(HerdrEvent::ConnectionChanged(
                    connection,
                )))
            }
            TransportNotification::CreateResolved { request_id, result } => {
                Ok(SessionUpdate::Event(HerdrEvent::CreateResolved {
                    request_id,
                    result,
                }))
            }
            TransportNotification::AttachResolved { request_id, result } => {
                Ok(SessionUpdate::AttachResolved { request_id, result })
            }
        }
    }

    fn update_control(
        &mut self,
        key: &AgentKey,
        ownership: ControlOwnership,
    ) -> Result<(), SessionError> {
        let Some((space_index, agent_index)) = self
            .mirror
            .snapshot
            .spaces
            .iter()
            .enumerate()
            .find(|(_, space)| space.id == key.space)
            .and_then(|(space_index, space)| {
                space
                    .agents
                    .iter()
                    .position(|agent| agent.id == key.agent)
                    .map(|agent_index| (space_index, agent_index))
            })
        else {
            return Ok(());
        };

        if self.mirror.snapshot.spaces[space_index].agents[agent_index].control == ownership {
            return Ok(());
        }
        let revision = self.next_revision()?;
        self.mirror.snapshot.spaces[space_index].agents[agent_index].control = ownership;
        self.mirror.revision = revision;
        Ok(())
    }

    fn next_revision(&self) -> Result<CatalogRevision, SessionError> {
        self.mirror
            .revision
            .checked_next()
            .ok_or(SessionError::RevisionExhausted)
    }
}

#[cfg(test)]
mod tests {
    use crate::AgentId;

    use crate::fake::FakeHerdrTransport;
    use crate::transport::{AttachmentId, AttachmentInput, TransportCommand};

    use super::*;

    fn key(space: &str, agent: &str) -> AgentKey {
        AgentKey {
            space: SpaceId::new(space),
            agent: AgentId::new(agent),
        }
    }

    fn snapshot(space: &str, agent: &str, phase: AgentPhase) -> CatalogSnapshot {
        CatalogSnapshot::new(vec![SpaceCatalogEntry::new(
            SpaceId::new(space),
            vec![AgentCatalogEntry::new(
                AgentId::new(agent),
                phase,
                ControlOwnership::Unowned,
            )],
        )])
    }

    #[test]
    fn session_snapshot_replaces_the_catalog_and_advances_revision() {
        let mut transport = FakeHerdrTransport::default();
        transport.push_notification(TransportNotification::CatalogSnapshot(snapshot(
            "space-a",
            "agent-a",
            AgentPhase::Starting,
        )));
        transport.push_notification(TransportNotification::CatalogSnapshot(snapshot(
            "space-b",
            "agent-b",
            AgentPhase::Running,
        )));
        let mut session = SessionState::default();

        let drain = session.drain(&mut transport);

        assert_eq!(drain.error, None);
        assert_eq!(
            drain.updates,
            [
                SessionUpdate::Event(HerdrEvent::CatalogChanged(CatalogRevision(1))),
                SessionUpdate::Event(HerdrEvent::CatalogChanged(CatalogRevision(2))),
            ]
        );
        assert_eq!(session.catalog().revision().get(), 2);
        assert_eq!(
            session.catalog().snapshot(),
            &snapshot("space-b", "agent-b", AgentPhase::Running)
        );
    }

    #[test]
    fn session_phase_control_and_connection_are_independent_fields() {
        let agent = key("space", "agent");
        let mut transport = FakeHerdrTransport::default();
        transport.push_notification(TransportNotification::ConnectionChanged(
            ConnectionState::Connected,
        ));
        transport.push_notification(TransportNotification::CatalogSnapshot(snapshot(
            "space",
            "agent",
            AgentPhase::WaitingForInput,
        )));
        transport.push_notification(TransportNotification::ControlChanged {
            agent,
            ownership: ControlOwnership::AnotherClient,
        });
        transport.push_notification(TransportNotification::ConnectionChanged(
            ConnectionState::Disconnected,
        ));
        let mut session = SessionState::default();

        assert_eq!(session.drain(&mut transport).error, None);

        let catalog_agent = &session.catalog().snapshot().spaces()[0].agents()[0];
        assert_eq!(catalog_agent.phase(), AgentPhase::WaitingForInput);
        assert_eq!(catalog_agent.control(), ControlOwnership::AnotherClient);
        assert_eq!(
            session.catalog().connection(),
            ConnectionState::Disconnected
        );
    }

    #[test]
    fn session_phase_vocabulary_has_exactly_five_values() {
        fn exhaustive(phase: AgentPhase) -> &'static str {
            match phase {
                AgentPhase::Starting => "starting",
                AgentPhase::Running => "running",
                AgentPhase::WaitingForInput => "waiting-for-input",
                AgentPhase::Succeeded => "succeeded",
                AgentPhase::Failed => "failed",
            }
        }

        let phases = [
            AgentPhase::Starting,
            AgentPhase::Running,
            AgentPhase::WaitingForInput,
            AgentPhase::Succeeded,
            AgentPhase::Failed,
        ];

        assert_eq!(phases.map(exhaustive).len(), 5);
    }

    #[test]
    fn session_preserves_ordered_attachment_output() {
        let attachment = AttachmentId::new(7);
        let mut transport = FakeHerdrTransport::default();
        transport.push_notification(TransportNotification::AttachmentOutput(AttachmentOutput {
            attachment,
            bytes: b"replay\r\n".to_vec(),
        }));
        transport.push_notification(TransportNotification::AttachmentOutput(AttachmentOutput {
            attachment,
            bytes: b"live\0\xff".to_vec(),
        }));
        let mut session = SessionState::default();

        let drain = session.drain(&mut transport);

        assert_eq!(drain.error, None);
        assert_eq!(
            drain.updates,
            [
                SessionUpdate::AttachmentOutput(AttachmentOutput {
                    attachment,
                    bytes: b"replay\r\n".to_vec(),
                }),
                SessionUpdate::AttachmentOutput(AttachmentOutput {
                    attachment,
                    bytes: b"live\0\xff".to_vec(),
                }),
            ]
        );
    }

    #[test]
    fn session_fake_records_verbatim_input_for_one_attachment() {
        let attachment = AttachmentId::new(9);
        let other_attachment = AttachmentId::new(10);
        let bytes = vec![0, 0x1b, 0xff, b'\r', b'\n'];
        let mut transport = FakeHerdrTransport::default();

        transport
            .send(TransportCommand::AttachmentInput(AttachmentInput {
                attachment,
                bytes: bytes.clone(),
            }))
            .expect("record input");
        transport
            .send(TransportCommand::AttachmentInput(AttachmentInput {
                attachment: other_attachment,
                bytes: b"other".to_vec(),
            }))
            .expect("record other input");

        assert_eq!(transport.inputs_for_id(attachment), [bytes]);
        assert_eq!(
            transport.inputs_for_id(other_attachment),
            [b"other".to_vec()]
        );
    }

    #[test]
    fn session_revision_failure_leaves_the_snapshot_unchanged() {
        let mut transport = FakeHerdrTransport::default();
        transport.push_notification(TransportNotification::CatalogSnapshot(snapshot(
            "replacement",
            "replacement",
            AgentPhase::Running,
        )));
        let original = snapshot("original", "original", AgentPhase::Starting);
        let mut session = SessionState {
            mirror: CatalogMirror {
                revision: CatalogRevision(u64::MAX),
                snapshot: original.clone(),
                connection: ConnectionState::Connected,
            },
        };

        let drain = session.drain(&mut transport);

        assert_eq!(drain.error, Some(SessionError::RevisionExhausted));
        assert!(drain.updates.is_empty());
        assert_eq!(session.catalog().snapshot(), &original);
        assert_eq!(session.catalog().revision(), CatalogRevision(u64::MAX));
    }
}
