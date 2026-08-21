use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, Sender},
    },
};

use crate::{
    AgentCommand, AgentKey, CatalogMirror, Confirmed, HerdrEvent, RequestId, SpaceId,
    session::{SessionState, SessionUpdate},
    transport::{
        AttachAgentRequest, AttachmentId, AttachmentInput, CloseAgentRequest, CreateAgentRequest,
        DetachAgentRequest, HerdrTransport, TransportAttachResult, TransportCommand,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CreateAgentFailure {
    Rejected(String),
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttachmentUnavailable;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MutationFailure {
    RequestIdConflict,
    Cancelled,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentConflict {
    id: crate::ConflictId,
    holder: String,
}

impl AttachmentConflict {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the real transport constructs conflicts in P9")
    )]
    pub(crate) fn new(id: crate::ConflictId, holder: impl Into<String>) -> Self {
        Self {
            id,
            holder: holder.into(),
        }
    }

    pub fn id(&self) -> &crate::ConflictId {
        &self.id
    }

    pub fn holder(&self) -> &str {
        &self.holder
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[must_use]
pub struct AttachTicket {
    request_id: RequestId,
}

impl AttachTicket {
    pub fn request_id(self) -> RequestId {
        self.request_id
    }
}

#[must_use]
pub struct WritableAttachment {
    attachment: AttachmentId,
    active: Arc<AtomicBool>,
    input: Sender<AttachmentInput>,
}

impl WritableAttachment {
    pub fn write_input(&self, bytes: Vec<u8>) -> Result<(), AttachmentUnavailable> {
        if !self.active.load(Ordering::Acquire) {
            return Err(AttachmentUnavailable);
        }
        self.input
            .send(AttachmentInput {
                attachment: self.attachment,
                bytes,
            })
            .map_err(|_| AttachmentUnavailable)
    }
}

#[must_use]
pub enum AttachResult {
    Writable(WritableAttachment),
    Conflict(AttachmentConflict),
    Unavailable,
}

struct AttachmentBinding {
    agent: AgentKey,
    active: Arc<AtomicBool>,
    requests: HashSet<RequestId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum MutationRequest {
    Create {
        space: SpaceId,
        command: AgentCommand,
    },
    Attach(AgentKey),
    Detach(AgentKey),
    Close(AgentKey),
}

pub struct HerdrController {
    transport: Box<dyn HerdrTransport>,
    session: SessionState,
    input_tx: Sender<AttachmentInput>,
    input_rx: Receiver<AttachmentInput>,
    bindings: BTreeMap<AttachmentId, AttachmentBinding>,
    attach_agents: HashMap<RequestId, AgentKey>,
    attach_results: HashMap<RequestId, AttachResult>,
    cancelled_attachments: HashSet<RequestId>,
    requests: HashMap<RequestId, MutationRequest>,
}

impl HerdrController {
    #[cfg(test)]
    pub(crate) fn with_transport(transport: impl HerdrTransport + 'static) -> Self {
        let (input_tx, input_rx) = std::sync::mpsc::channel();
        Self {
            transport: Box::new(transport),
            session: SessionState::default(),
            input_tx,
            input_rx,
            bindings: BTreeMap::new(),
            attach_agents: HashMap::new(),
            attach_results: HashMap::new(),
            cancelled_attachments: HashSet::new(),
            requests: HashMap::new(),
        }
    }

    pub fn catalog(&self) -> &CatalogMirror {
        self.session.catalog()
    }

    pub fn create_agent(
        &mut self,
        request_id: RequestId,
        space: SpaceId,
        command: AgentCommand,
    ) -> Result<(), MutationFailure> {
        self.register_request(
            request_id,
            MutationRequest::Create {
                space: space.clone(),
                command: command.clone(),
            },
        )?;
        self.transport
            .send(TransportCommand::CreateAgent(CreateAgentRequest {
                request_id,
                space,
                command,
            }))
            .map_err(|_| MutationFailure::Unavailable)
    }

    pub fn request_attach(
        &mut self,
        request_id: RequestId,
        agent: AgentKey,
    ) -> Result<AttachTicket, MutationFailure> {
        if self.cancelled_attachments.contains(&request_id) {
            return Err(MutationFailure::Cancelled);
        }
        self.register_request(request_id, MutationRequest::Attach(agent.clone()))?;
        self.attach_agents
            .entry(request_id)
            .or_insert_with(|| agent.clone());
        if self
            .transport
            .send(TransportCommand::RequestAttach(AttachAgentRequest {
                request_id,
                agent,
            }))
            .is_err()
        {
            self.attach_agents.remove(&request_id);
            return Err(MutationFailure::Unavailable);
        }
        Ok(AttachTicket { request_id })
    }

    pub fn take_attachment(&mut self, ticket: AttachTicket) -> Option<AttachResult> {
        self.attach_results.remove(&ticket.request_id)
    }

    pub fn detach(
        &mut self,
        request_id: RequestId,
        agent: AgentKey,
    ) -> Result<(), MutationFailure> {
        self.register_request(request_id, MutationRequest::Detach(agent.clone()))?;
        self.transport
            .send(TransportCommand::Detach(DetachAgentRequest {
                request_id,
                agent: agent.clone(),
            }))
            .map_err(|_| MutationFailure::Unavailable)?;
        self.revoke_bindings(&agent);
        Ok(())
    }

    pub fn close_agent(
        &mut self,
        request_id: RequestId,
        agent: AgentKey,
        _confirmation: &Confirmed,
    ) -> Result<(), MutationFailure> {
        self.register_request(request_id, MutationRequest::Close(agent.clone()))?;
        self.transport
            .send(TransportCommand::CloseAgent(CloseAgentRequest {
                request_id,
                agent: agent.clone(),
            }))
            .map_err(|_| MutationFailure::Unavailable)?;
        self.revoke_bindings(&agent);
        Ok(())
    }

    pub fn poll(&mut self) -> Vec<HerdrEvent> {
        let mut input_failed = false;
        while let Ok(input) = self.input_rx.try_recv() {
            if self.bindings.contains_key(&input.attachment) {
                input_failed |= self
                    .transport
                    .send(TransportCommand::AttachmentInput(input))
                    .is_err();
            }
        }

        let drain = self.session.drain(self.transport.as_mut());
        let mut events: Vec<_> = drain
            .updates
            .into_iter()
            .filter_map(|update| self.apply(update))
            .collect();
        if let Some(error) = drain.error {
            events.push(HerdrEvent::SessionFailed(error));
        }
        if input_failed {
            events.push(HerdrEvent::SessionFailed(
                crate::SessionError::TransportUnavailable,
            ));
        }
        events
    }

    fn apply(&mut self, update: SessionUpdate) -> Option<HerdrEvent> {
        match update {
            SessionUpdate::Event(event) => Some(event),
            SessionUpdate::AttachmentOutput(output) => {
                self.bindings
                    .get(&output.attachment)
                    .map(|binding| HerdrEvent::AttachmentOutput {
                        agent: binding.agent.clone(),
                        bytes: output.bytes,
                    })
            }
            SessionUpdate::AttachResolved { request_id, result } => {
                let agent = self.attach_agents.remove(&request_id)?;
                let result = match result {
                    TransportAttachResult::Writable(attachment) => {
                        let binding =
                            self.bindings
                                .entry(attachment)
                                .or_insert_with(|| AttachmentBinding {
                                    agent,
                                    active: Arc::new(AtomicBool::new(true)),
                                    requests: HashSet::new(),
                                });
                        binding.requests.insert(request_id);
                        let active = binding.active.clone();
                        AttachResult::Writable(WritableAttachment {
                            attachment,
                            active,
                            input: self.input_tx.clone(),
                        })
                    }
                    TransportAttachResult::Conflict(conflict) => AttachResult::Conflict(conflict),
                    TransportAttachResult::Unavailable => AttachResult::Unavailable,
                };
                self.attach_results.insert(request_id, result);
                Some(HerdrEvent::AttachmentChanged(request_id))
            }
        }
    }

    fn revoke_bindings(&mut self, agent: &AgentKey) {
        let mut cancelled = Vec::new();
        self.bindings.retain(|_, binding| {
            if binding.agent == *agent {
                binding.active.store(false, Ordering::Release);
                cancelled.extend(binding.requests.iter().copied());
                false
            } else {
                true
            }
        });
        cancelled.extend(
            self.attach_agents
                .iter()
                .filter_map(|(request_id, requested_agent)| {
                    (requested_agent == agent).then_some(*request_id)
                }),
        );
        for request_id in cancelled {
            self.attach_agents.remove(&request_id);
            self.attach_results.remove(&request_id);
            self.cancelled_attachments.insert(request_id);
        }
    }

    fn register_request(
        &mut self,
        request_id: RequestId,
        request: MutationRequest,
    ) -> Result<(), MutationFailure> {
        match self.requests.get(&request_id) {
            Some(existing) if existing != &request => Err(MutationFailure::RequestIdConflict),
            Some(_) => Ok(()),
            None => {
                self.requests.insert(request_id, request);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        AgentCatalogEntry, AgentId, AgentPhase, CatalogSnapshot, ConflictId, ControlOwnership,
        SpaceCatalogEntry,
        fake::{FakeHerdrHandle, FakeHerdrTransport},
    };

    use super::*;

    fn key(space: &str, agent: &str) -> AgentKey {
        AgentKey {
            space: SpaceId::new(space),
            agent: AgentId::new(agent),
        }
    }

    fn snapshot(space: &str, agents: &[&str]) -> CatalogSnapshot {
        CatalogSnapshot::new(vec![SpaceCatalogEntry::new(
            SpaceId::new(space),
            agents
                .iter()
                .map(|agent| {
                    AgentCatalogEntry::new(
                        AgentId::new(*agent),
                        AgentPhase::Running,
                        ControlOwnership::Unowned,
                    )
                })
                .collect(),
        )])
    }

    fn controller() -> (HerdrController, FakeHerdrHandle) {
        let (transport, handle) = FakeHerdrTransport::new();
        (HerdrController::with_transport(transport), handle)
    }

    #[test]
    fn controller_create_is_idempotent_for_a_request_id() {
        let (mut controller, handle) = controller();
        let request_id = RequestId::new();
        let agent = key("space", "agent");
        let command = AgentCommand {
            program: "printf".to_string(),
            argv: vec!["a b".to_string(), "; echo x".to_string()],
        };
        handle.set_create_outcome(request_id, Ok(agent.clone()));

        controller
            .create_agent(request_id, SpaceId::new("space"), command.clone())
            .expect("create agent");
        controller
            .create_agent(request_id, SpaceId::new("space"), command)
            .expect("retry create agent");

        assert_eq!(handle.create_effects(), 1);
        assert_eq!(handle.create_requests().len(), 2);
        assert_eq!(
            controller.poll(),
            [
                HerdrEvent::CreateResolved {
                    request_id,
                    result: Ok(agent.clone()),
                },
                HerdrEvent::CreateResolved {
                    request_id,
                    result: Ok(agent),
                },
            ]
        );
        assert!(controller.poll().is_empty());
    }

    #[test]
    fn controller_create_failure_leaves_the_catalog_unchanged() {
        let (mut controller, handle) = controller();
        handle.seed_catalog(snapshot("space", &["existing"]));
        controller.poll();
        let before = controller.catalog().clone();
        let request_id = RequestId::new();
        let failure = CreateAgentFailure::Rejected("invalid command".to_string());
        handle.set_create_outcome(request_id, Err(failure.clone()));

        controller
            .create_agent(
                request_id,
                SpaceId::new("space"),
                AgentCommand {
                    program: "missing".to_string(),
                    argv: Vec::new(),
                },
            )
            .expect("queue failed create");

        assert_eq!(
            controller.poll(),
            [HerdrEvent::CreateResolved {
                request_id,
                result: Err(failure),
            }]
        );
        assert_eq!(controller.catalog(), &before);
    }

    #[test]
    fn controller_attach_transitions_from_requested_to_typed_resolution() {
        let (mut controller, handle) = controller();
        let writable_agent = key("space", "writable");
        let conflict_agent = key("space", "conflict");
        let unavailable_agent = key("space", "unavailable");
        handle.set_writable(writable_agent.clone(), 41);
        handle.set_conflict(
            conflict_agent.clone(),
            AttachmentConflict::new(ConflictId::new("conflict-1"), "other-client"),
        );

        let writable_ticket = controller
            .request_attach(RequestId::new(), writable_agent)
            .expect("request writable attachment");
        let conflict_ticket = controller
            .request_attach(RequestId::new(), conflict_agent)
            .expect("request conflicting attachment");
        let unavailable_ticket = controller
            .request_attach(RequestId::new(), unavailable_agent)
            .expect("request unavailable attachment");
        assert!(controller.take_attachment(writable_ticket).is_none());
        controller.poll();

        let Some(AttachResult::Writable(writable)) = controller.take_attachment(writable_ticket)
        else {
            panic!("expected writable attachment");
        };
        writable
            .write_input(vec![0, 0x1b, 0xff, b'\r', b'\n'])
            .expect("queue attachment input");
        controller.poll();
        assert_eq!(handle.inputs_for(41), [vec![0, 0x1b, 0xff, b'\r', b'\n']]);

        let Some(AttachResult::Conflict(conflict)) = controller.take_attachment(conflict_ticket)
        else {
            panic!("expected conflict");
        };
        assert_eq!(conflict.id(), &ConflictId::new("conflict-1"));
        assert_eq!(conflict.holder(), "other-client");
        assert!(matches!(
            controller.take_attachment(unavailable_ticket),
            Some(AttachResult::Unavailable)
        ));
    }

    #[test]
    fn controller_attach_is_idempotent_for_a_request_id() {
        let (mut controller, handle) = controller();
        let request_id = RequestId::new();
        let agent = key("space", "agent");
        handle.set_writable(agent.clone(), 17);

        let _ = controller
            .request_attach(request_id, agent.clone())
            .expect("request attachment");
        let _ = controller
            .request_attach(request_id, agent)
            .expect("retry attachment");
        controller.poll();

        assert_eq!(handle.attach_effects(), 1);
        assert_eq!(handle.attach_requests(), 2);
    }

    #[test]
    fn controller_rejects_request_id_reuse_with_a_different_payload() {
        let (mut controller, handle) = controller();
        let request_id = RequestId::new();
        let first_agent = key("space", "first");
        let second_agent = key("space", "second");
        handle.set_writable(first_agent.clone(), 18);

        let _ = controller
            .request_attach(request_id, first_agent)
            .expect("request attachment");
        assert!(matches!(
            controller.request_attach(request_id, second_agent),
            Err(MutationFailure::RequestIdConflict)
        ));
        assert_eq!(handle.attach_requests(), 1);
    }

    #[test]
    fn controller_detach_and_close_revoke_writable_capabilities() {
        let (mut controller, handle) = controller();
        let detached_agent = key("space", "detached");
        let closed_agent = key("space", "closed");
        handle.seed_catalog(snapshot("space", &["detached", "closed"]));
        handle.set_writable(detached_agent.clone(), 21);
        handle.set_writable(closed_agent.clone(), 22);
        controller.poll();

        let detached_ticket = controller
            .request_attach(RequestId::new(), detached_agent.clone())
            .expect("request detached attachment");
        let closed_ticket = controller
            .request_attach(RequestId::new(), closed_agent.clone())
            .expect("request closed attachment");
        controller.poll();
        let Some(AttachResult::Writable(detached)) = controller.take_attachment(detached_ticket)
        else {
            panic!("expected detached writable attachment");
        };
        let Some(AttachResult::Writable(closed)) = controller.take_attachment(closed_ticket) else {
            panic!("expected closed writable attachment");
        };

        detached
            .write_input(b"queued before detach".to_vec())
            .unwrap();
        let detach_request = RequestId::new();
        controller
            .detach(detach_request, detached_agent.clone())
            .expect("detach agent");
        controller
            .detach(detach_request, detached_agent)
            .expect("retry detach agent");
        assert_eq!(
            detached.write_input(b"after detach".to_vec()),
            Err(AttachmentUnavailable)
        );

        closed.write_input(b"queued before close".to_vec()).unwrap();
        let close_request = RequestId::new();
        let confirmation = Confirmed::after_user_confirmation();
        controller
            .close_agent(close_request, closed_agent.clone(), &confirmation)
            .expect("close agent");
        controller
            .close_agent(close_request, closed_agent, &confirmation)
            .expect("retry close agent");
        controller.poll();

        assert_eq!(
            closed.write_input(b"after close".to_vec()),
            Err(AttachmentUnavailable)
        );
        assert!(handle.inputs_for(21).is_empty());
        assert!(handle.inputs_for(22).is_empty());
        assert_eq!(handle.detach_effects(), 1);
        assert_eq!(handle.close_effects(), 1);
        assert_eq!(
            controller.catalog().snapshot().spaces()[0].agents().len(),
            1
        );
    }

    #[test]
    fn controller_detach_and_close_cancel_pending_attachments() {
        let (mut controller, handle) = controller();
        let detached_agent = key("space", "detached");
        let closed_agent = key("space", "closed");
        handle.set_writable(detached_agent.clone(), 31);
        handle.set_writable(closed_agent.clone(), 32);

        let detached_ticket = controller
            .request_attach(RequestId::new(), detached_agent.clone())
            .expect("request pending detached attachment");
        let closed_ticket = controller
            .request_attach(RequestId::new(), closed_agent.clone())
            .expect("request pending closed attachment");
        controller
            .detach(RequestId::new(), detached_agent)
            .expect("detach pending attachment");
        controller
            .close_agent(
                RequestId::new(),
                closed_agent,
                &Confirmed::after_user_confirmation(),
            )
            .expect("close pending attachment");
        controller.poll();

        assert!(controller.take_attachment(detached_ticket).is_none());
        assert!(controller.take_attachment(closed_ticket).is_none());
        assert!(handle.inputs_for(31).is_empty());
        assert!(handle.inputs_for(32).is_empty());
    }

    #[test]
    fn controller_preserves_capability_when_detach_cannot_be_enqueued() {
        let (mut controller, handle) = controller();
        let agent = key("space", "agent");
        handle.set_writable(agent.clone(), 51);
        let ticket = controller
            .request_attach(RequestId::new(), agent.clone())
            .expect("request attachment");
        controller.poll();
        let Some(AttachResult::Writable(writable)) = controller.take_attachment(ticket) else {
            panic!("expected writable attachment");
        };

        handle.fail_next_send();
        assert_eq!(
            controller.detach(RequestId::new(), agent),
            Err(MutationFailure::Unavailable)
        );
        writable
            .write_input(b"still writable".to_vec())
            .expect("write after failed detach");
        controller.poll();

        assert_eq!(handle.inputs_for(51), [b"still writable".to_vec()]);
    }

    #[test]
    fn controller_can_retry_an_attach_that_was_not_enqueued() {
        let (mut controller, handle) = controller();
        let request_id = RequestId::new();
        let agent = key("space", "agent");
        handle.set_writable(agent.clone(), 52);
        handle.fail_next_send();

        assert!(matches!(
            controller.request_attach(request_id, agent.clone()),
            Err(MutationFailure::Unavailable)
        ));
        let ticket = controller
            .request_attach(request_id, agent)
            .expect("retry attachment");
        controller.poll();

        assert!(matches!(
            controller.take_attachment(ticket),
            Some(AttachResult::Writable(_))
        ));
        assert_eq!(handle.attach_effects(), 1);
    }

    #[test]
    fn controller_poll_reports_transport_failure() {
        let (mut controller, handle) = controller();
        handle.fail_next_recv();

        assert_eq!(
            controller.poll(),
            [HerdrEvent::SessionFailed(
                crate::SessionError::TransportUnavailable
            )]
        );
    }

    #[test]
    fn controller_poll_drains_output_once() {
        let (mut controller, handle) = controller();
        let agent = key("space", "agent");
        handle.set_writable(agent.clone(), 77);
        let ticket = controller
            .request_attach(RequestId::new(), agent.clone())
            .expect("request attachment");
        controller.poll();
        assert!(matches!(
            controller.take_attachment(ticket),
            Some(AttachResult::Writable(_))
        ));
        handle.push_output(77, b"replay then live".to_vec());

        assert_eq!(
            controller.poll(),
            [HerdrEvent::AttachmentOutput {
                agent,
                bytes: b"replay then live".to_vec(),
            }]
        );
        assert!(controller.poll().is_empty());
    }
}
