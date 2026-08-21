use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex, MutexGuard},
};

#[cfg(test)]
use crate::AttachmentConflict;
use crate::{
    AgentKey, CatalogSnapshot, CreateAgentFailure, RequestId,
    transport::{
        AttachmentId, AttachmentOutput, CreateAgentRequest, HerdrTransport, TransportAttachResult,
        TransportCommand, TransportError, TransportNotification,
    },
};

#[derive(Default)]
struct FakeState {
    incoming: VecDeque<TransportNotification>,
    catalog: CatalogSnapshot,
    create_outcomes: HashMap<RequestId, Result<AgentKey, CreateAgentFailure>>,
    create_results: HashMap<RequestId, Result<AgentKey, CreateAgentFailure>>,
    create_requests: Vec<CreateAgentRequest>,
    create_effects: usize,
    attach_outcomes: HashMap<AgentKey, TransportAttachResult>,
    attach_results: HashMap<RequestId, TransportAttachResult>,
    attach_requests: usize,
    attach_effects: usize,
    input_by_attachment: BTreeMap<AttachmentId, Vec<Vec<u8>>>,
    detached_requests: HashSet<RequestId>,
    detach_effects: usize,
    closed_requests: HashSet<RequestId>,
    close_effects: usize,
    fail_next_send: bool,
    fail_next_recv: bool,
}

pub(crate) struct FakeHerdrTransport {
    state: Arc<Mutex<FakeState>>,
}

#[derive(Clone)]
pub struct FakeHerdrHandle {
    state: Arc<Mutex<FakeState>>,
}

#[cfg(feature = "test-support")]
pub type FakeAgentCatalogRow = (crate::AgentId, crate::AgentPhase, crate::ControlOwnership);

#[cfg(feature = "test-support")]
pub type FakeSpaceCatalogRow = (crate::SpaceId, Vec<FakeAgentCatalogRow>);

#[cfg(feature = "test-support")]
pub fn fake_controller() -> (crate::HerdrController, FakeHerdrHandle) {
    let (transport, handle) = FakeHerdrTransport::new();
    (crate::HerdrController::with_transport(transport), handle)
}

impl FakeHerdrTransport {
    pub(crate) fn new() -> (Self, FakeHerdrHandle) {
        let state = Arc::new(Mutex::new(FakeState::default()));
        (
            Self {
                state: Arc::clone(&state),
            },
            FakeHerdrHandle { state },
        )
    }

    fn state(&self) -> MutexGuard<'_, FakeState> {
        self.state.lock().expect("lock fake Herdr state")
    }

    #[cfg(test)]
    pub(crate) fn push_notification(&self, notification: TransportNotification) {
        self.state().incoming.push_back(notification);
    }

    #[cfg(test)]
    pub(crate) fn inputs_for_id(&self, attachment: AttachmentId) -> Vec<Vec<u8>> {
        self.state()
            .input_by_attachment
            .get(&attachment)
            .cloned()
            .unwrap_or_default()
    }
}

impl Default for FakeHerdrTransport {
    fn default() -> Self {
        Self::new().0
    }
}

impl FakeHerdrHandle {
    fn state(&self) -> MutexGuard<'_, FakeState> {
        self.state.lock().expect("lock fake Herdr handle")
    }

    pub(crate) fn seed_catalog(&self, snapshot: CatalogSnapshot) {
        let mut state = self.state();
        state.catalog = snapshot.clone();
        state
            .incoming
            .push_back(TransportNotification::CatalogSnapshot(snapshot));
    }

    #[cfg(feature = "test-support")]
    pub fn seed_catalog_rows(&self, spaces: Vec<FakeSpaceCatalogRow>) {
        self.seed_catalog(CatalogSnapshot::new(
            spaces
                .into_iter()
                .map(|(space, agents)| {
                    crate::SpaceCatalogEntry::new(
                        space,
                        agents
                            .into_iter()
                            .map(|(agent, phase, control)| {
                                crate::AgentCatalogEntry::new(agent, phase, control)
                            })
                            .collect(),
                    )
                })
                .collect(),
        ));
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn set_create_outcome(
        &self,
        request_id: RequestId,
        outcome: Result<AgentKey, CreateAgentFailure>,
    ) {
        self.state().create_outcomes.insert(request_id, outcome);
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn create_effects(&self) -> usize {
        self.state().create_effects
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn created_commands(&self) -> Vec<(crate::SpaceId, crate::AgentCommand)> {
        self.state()
            .create_requests
            .iter()
            .map(|request| (request.space.clone(), request.command.clone()))
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn create_requests(&self) -> Vec<CreateAgentRequest> {
        self.state().create_requests.clone()
    }

    pub fn set_writable(&self, agent: AgentKey, attachment: u64) {
        self.state().attach_outcomes.insert(
            agent,
            TransportAttachResult::Writable(AttachmentId::new(attachment)),
        );
    }

    #[cfg(test)]
    pub(crate) fn set_conflict(&self, agent: AgentKey, conflict: AttachmentConflict) {
        self.state()
            .attach_outcomes
            .insert(agent, TransportAttachResult::Conflict(conflict));
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn attach_requests(&self) -> usize {
        self.state().attach_requests
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn attach_effects(&self) -> usize {
        self.state().attach_effects
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn inputs_for(&self, attachment: u64) -> Vec<Vec<u8>> {
        self.state()
            .input_by_attachment
            .get(&AttachmentId::new(attachment))
            .cloned()
            .unwrap_or_default()
    }

    pub fn push_output(&self, attachment: u64, bytes: Vec<u8>) {
        self.state()
            .incoming
            .push_back(TransportNotification::AttachmentOutput(AttachmentOutput {
                attachment: AttachmentId::new(attachment),
                bytes,
            }));
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn detach_effects(&self) -> usize {
        self.state().detach_effects
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn close_effects(&self) -> usize {
        self.state().close_effects
    }

    #[cfg(test)]
    pub(crate) fn fail_next_send(&self) {
        self.state().fail_next_send = true;
    }

    #[cfg(test)]
    pub(crate) fn fail_next_recv(&self) {
        self.state().fail_next_recv = true;
    }
}

impl HerdrTransport for FakeHerdrTransport {
    fn send(&mut self, command: TransportCommand) -> Result<(), TransportError> {
        let mut state = self.state();
        if std::mem::take(&mut state.fail_next_send) {
            return Err(TransportError);
        }
        match command {
            TransportCommand::CreateAgent(request) => {
                state.create_requests.push(request.clone());
                let result = if let Some(result) = state.create_results.get(&request.request_id) {
                    result.clone()
                } else {
                    let result = state
                        .create_outcomes
                        .remove(&request.request_id)
                        .unwrap_or(Err(CreateAgentFailure::Unavailable));
                    if result.is_ok() {
                        state.create_effects += 1;
                    }
                    state
                        .create_results
                        .insert(request.request_id, result.clone());
                    result
                };
                state
                    .incoming
                    .push_back(TransportNotification::CreateResolved {
                        request_id: request.request_id,
                        result,
                    });
            }
            TransportCommand::RequestAttach(request) => {
                state.attach_requests += 1;
                let result = if let Some(result) = state.attach_results.get(&request.request_id) {
                    result.clone()
                } else {
                    let result = state
                        .attach_outcomes
                        .get(&request.agent)
                        .cloned()
                        .unwrap_or(TransportAttachResult::Unavailable);
                    state.attach_effects += 1;
                    state
                        .attach_results
                        .insert(request.request_id, result.clone());
                    result
                };
                state
                    .incoming
                    .push_back(TransportNotification::AttachResolved {
                        request_id: request.request_id,
                        result,
                    });
            }
            TransportCommand::AttachmentInput(input) => {
                state
                    .input_by_attachment
                    .entry(input.attachment)
                    .or_default()
                    .push(input.bytes);
            }
            TransportCommand::Detach(request) => {
                if state.detached_requests.insert(request.request_id) {
                    state.detach_effects += 1;
                }
            }
            TransportCommand::CloseAgent(request) => {
                if !state.closed_requests.insert(request.request_id) {
                    return Ok(());
                }
                state.close_effects += 1;
                if state.catalog.remove_agent(&request.agent) {
                    let snapshot = state.catalog.clone();
                    state
                        .incoming
                        .push_back(TransportNotification::CatalogSnapshot(snapshot));
                }
            }
        }
        Ok(())
    }

    fn try_recv(&mut self) -> Result<Option<TransportNotification>, TransportError> {
        let mut state = self.state();
        if std::mem::take(&mut state.fail_next_recv) {
            return Err(TransportError);
        }
        Ok(state.incoming.pop_front())
    }
}
