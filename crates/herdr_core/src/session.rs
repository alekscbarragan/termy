use crate::{AgentId, AgentKey, AgentPhase, SpaceId};

use crate::transport::{AttachmentOutput, HerdrTransport, TransportError, TransportNotification};

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
pub struct AgentCatalogEntry {
    id: AgentId,
    phase: AgentPhase,
    control: ControlOwnership,
}

impl AgentCatalogEntry {
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
    pub(crate) fn new(spaces: Vec<SpaceCatalogEntry>) -> Self {
        Self { spaces }
    }

    pub fn spaces(&self) -> &[SpaceCatalogEntry] {
        &self.spaces
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
    Transport(TransportError),
    RevisionExhausted,
}

impl From<TransportError> for SessionError {
    fn from(error: TransportError) -> Self {
        Self::Transport(error)
    }
}

#[derive(Default)]
pub struct SessionState {
    mirror: CatalogMirror,
}

impl SessionState {
    pub fn catalog(&self) -> &CatalogMirror {
        &self.mirror
    }

    pub fn drain(
        &mut self,
        transport: &mut dyn HerdrTransport,
    ) -> Result<Vec<AttachmentOutput>, SessionError> {
        let mut output = Vec::new();
        while let Some(notification) = transport.try_recv()? {
            if let Some(chunk) = self.apply(notification)? {
                output.push(chunk);
            }
        }
        Ok(output)
    }

    fn apply(
        &mut self,
        notification: TransportNotification,
    ) -> Result<Option<AttachmentOutput>, SessionError> {
        match notification {
            TransportNotification::CatalogSnapshot(snapshot) => {
                let revision = self.next_revision()?;
                self.mirror.snapshot = snapshot;
                self.mirror.revision = revision;
                Ok(None)
            }
            TransportNotification::AttachmentOutput(output) => Ok(Some(output)),
            TransportNotification::ControlChanged { agent, ownership } => {
                self.update_control(&agent, ownership)?;
                Ok(None)
            }
            TransportNotification::ConnectionChanged(connection) => {
                self.mirror.connection = connection;
                Ok(None)
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
    use crate::{AgentCommand, AgentId, RequestId};

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

        let output = session.drain(&mut transport).expect("drain snapshots");

        assert!(output.is_empty());
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

        session.drain(&mut transport).expect("drain state changes");

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

        let output = session.drain(&mut transport).expect("drain output");

        assert_eq!(
            output,
            [
                AttachmentOutput {
                    attachment,
                    bytes: b"replay\r\n".to_vec(),
                },
                AttachmentOutput {
                    attachment,
                    bytes: b"live\0\xff".to_vec(),
                },
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
            .send(TransportCommand::WriteInput(AttachmentInput {
                attachment,
                bytes: bytes.clone(),
            }))
            .expect("record input");
        transport
            .send(TransportCommand::WriteInput(AttachmentInput {
                attachment: other_attachment,
                bytes: b"other".to_vec(),
            }))
            .expect("record other input");

        assert_eq!(transport.inputs_for(attachment), [bytes]);
        assert_eq!(transport.inputs_for(other_attachment), [b"other".to_vec()]);
    }

    #[test]
    fn session_transport_commands_keep_typed_payloads() {
        let request_id = RequestId::new();
        let agent = key("space", "agent");
        let attachment = AttachmentId::new(11);
        let commands = [
            TransportCommand::CreateAgent {
                request_id,
                space: SpaceId::new("space"),
                command: AgentCommand {
                    program: "printf".to_string(),
                    argv: vec!["a b".to_string()],
                },
            },
            TransportCommand::Attach {
                request_id,
                agent: agent.clone(),
            },
            TransportCommand::WriteInput(AttachmentInput {
                attachment,
                bytes: vec![0, 0xff],
            }),
            TransportCommand::Detach {
                request_id,
                agent: agent.clone(),
            },
            TransportCommand::CloseAgent {
                request_id,
                agent: agent.clone(),
            },
        ];

        for command in commands {
            match command {
                TransportCommand::CreateAgent {
                    request_id: actual_request,
                    space,
                    command,
                } => {
                    assert_eq!(actual_request, request_id);
                    assert_eq!(space, SpaceId::new("space"));
                    assert_eq!(command.program, "printf");
                    assert_eq!(command.argv, ["a b"]);
                }
                TransportCommand::Attach {
                    request_id: actual_request,
                    agent: actual_agent,
                }
                | TransportCommand::Detach {
                    request_id: actual_request,
                    agent: actual_agent,
                }
                | TransportCommand::CloseAgent {
                    request_id: actual_request,
                    agent: actual_agent,
                } => {
                    assert_eq!(actual_request, request_id);
                    assert_eq!(actual_agent, agent);
                }
                TransportCommand::WriteInput(input) => {
                    assert_eq!(input.attachment, attachment);
                    assert_eq!(input.bytes, [0, 0xff]);
                }
            }
        }
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

        let error = session
            .drain(&mut transport)
            .expect_err("revision exhaustion must fail");

        assert_eq!(error, SessionError::RevisionExhausted);
        assert_eq!(session.catalog().snapshot(), &original);
        assert_eq!(session.catalog().revision(), CatalogRevision(u64::MAX));
    }
}
