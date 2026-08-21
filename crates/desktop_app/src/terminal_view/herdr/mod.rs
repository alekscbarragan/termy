use std::collections::{BTreeMap, HashMap};

use termy_herdr_core::{
    AgentCommand, AgentKey, AgentPhase, AttachResult, AttachTicket, CatalogMirror, ConnectionState,
    ControlOwnership, CreateAgentFailure, HerdrController, HerdrEvent, RequestId, SpaceId,
    WritableAttachment,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum SidebarView {
    #[default]
    Workspaces,
    Herdr,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ServiceOrigin {
    StartedByTermy,
}

impl super::TerminalView {
    pub(super) fn selected_sidebar_view(&self) -> SidebarView {
        self.herdr_sidebar_view
    }

    pub(super) fn select_sidebar_view(&mut self, view: SidebarView, cx: &mut gpui::Context<Self>) {
        if self.herdr_sidebar_view == view {
            return;
        }
        self.herdr_sidebar_view = view;
        self.mark_tab_strip_layout_dirty();
        cx.notify();
    }

    pub(super) fn selected_sidebar_content(&self) -> SidebarContent {
        sidebar_content(self.herdr_sidebar_view, self.herdr.as_ref())
    }

    pub(in crate::terminal_view) fn process_herdr_events(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(runtime) = self.herdr.as_mut() else {
            return false;
        };
        let drain = runtime.drain();
        for ready in drain.ready_tabs {
            let size = self
                .active_terminal()
                .map(super::Terminal::size)
                .unwrap_or_default();
            let terminal =
                super::Terminal::new_herdr_agent(size, self.terminal_runtime.term_options());
            let pane_id = self.insert_herdr_agent_tab(terminal, size, cx);
            if let Some(terminal) = self.herdr_agent_terminal_by_pane_id(&pane_id) {
                for bytes in ready.initial_output {
                    terminal.feed_output(&bytes);
                }
            }
            if let Some(runtime) = self.herdr.as_mut() {
                runtime.register_agent_tab(ready.key, pane_id, ready.attachment);
            }
        }
        for output in drain.output {
            if let Some(terminal) = self.herdr_agent_terminal_by_pane_id(&output.pane_id) {
                terminal.feed_output(&output.bytes);
            }
        }
        for failure in drain.open_failures {
            crate::ui::toast::error(failure);
            self.notify_overlay(cx);
        }
        drain.should_redraw
    }

    fn herdr_agent_terminal_by_pane_id(&self, pane_id: &str) -> Option<&super::Terminal> {
        self.pane_terminal_by_id(pane_id).or_else(|| {
            self.session
                .workspaces
                .iter()
                .flat_map(|workspace| workspace.tabs.iter())
                .flat_map(|tab| tab.panes.iter())
                .find(|pane| pane.id == pane_id)
                .map(super::TerminalPane::terminal)
        })
    }

    pub(super) fn begin_herdr_create(&mut self, space: SpaceId, cx: &mut gpui::Context<Self>) {
        if let Some(runtime) = self.herdr.as_mut() {
            runtime.begin_create(space);
            cx.notify();
        }
    }

    pub(super) fn cancel_herdr_create(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(runtime) = self.herdr.as_mut() {
            runtime.cancel_create();
            cx.notify();
        }
    }

    pub(super) fn update_herdr_create_program(
        &mut self,
        program: String,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(runtime) = self.herdr.as_mut() {
            runtime.update_create_program(program);
            cx.notify();
        }
    }

    pub(super) fn update_herdr_create_argument(
        &mut self,
        index: usize,
        argument: String,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(runtime) = self.herdr.as_mut() {
            runtime.update_create_argument(index, argument);
            cx.notify();
        }
    }

    pub(super) fn add_herdr_create_argument(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(runtime) = self.herdr.as_mut() {
            runtime.add_create_argument();
            cx.notify();
        }
    }

    pub(super) fn remove_herdr_create_argument(
        &mut self,
        index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(runtime) = self.herdr.as_mut() {
            runtime.remove_create_argument(index);
            cx.notify();
        }
    }

    pub(super) fn submit_herdr_create(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(runtime) = self.herdr.as_mut() {
            runtime.submit_create();
            cx.notify();
        }
    }

    pub(super) fn open_or_focus_agent(&mut self, key: AgentKey, cx: &mut gpui::Context<Self>) {
        #[cfg(test)]
        let runtime_kind = self
            .agent_open_runtime_kind_override
            .unwrap_or_else(|| self.runtime_kind());
        #[cfg(not(test))]
        let runtime_kind = self.runtime_kind();
        if let Err(message) = agent_open_gate(runtime_kind) {
            crate::ui::toast::info(message);
            self.notify_overlay(cx);
            return;
        }

        let pane_id = self
            .herdr
            .as_ref()
            .and_then(|runtime| runtime.agent_pane_id(&key));
        if let Some(pane_id) = pane_id {
            match find_agent_tab(&self.session, &pane_id) {
                Some(AgentTabLocation::Active(tab)) => self.switch_tab(tab, cx),
                Some(AgentTabLocation::Stashed { workspace, .. }) => {
                    self.switch_workspace(workspace, cx);
                    if let Some(AgentTabLocation::Active(tab)) =
                        find_agent_tab(&self.session, &pane_id)
                    {
                        self.switch_tab(tab, cx);
                    }
                }
                None => {
                    if let Some(runtime) = self.herdr.as_mut() {
                        runtime.remove_agent_tab(&key);
                    }
                }
            }
            if find_agent_tab(&self.session, &pane_id).is_some() {
                return;
            }
        }

        let result = self
            .herdr
            .as_mut()
            .ok_or("Herdr integration is disabled")
            .and_then(|runtime| runtime.request_open(key));
        if let Err(message) = result {
            crate::ui::toast::error(message);
            self.notify_overlay(cx);
        }
    }
}

fn agent_open_gate(runtime_kind: super::RuntimeKind) -> Result<(), &'static str> {
    match runtime_kind {
        super::RuntimeKind::Native => Ok(()),
        super::RuntimeKind::Tmux => Err("Switch to the native runtime to open Herdr Agents"),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AgentTabLocation {
    Active(usize),
    Stashed { workspace: usize, tab: usize },
}

fn find_agent_tab(session: &super::SessionState, pane_id: &str) -> Option<AgentTabLocation> {
    if let Some(tab) = session
        .tabs
        .iter()
        .position(|tab| tab.panes.iter().any(|pane| pane.id == pane_id))
    {
        return Some(AgentTabLocation::Active(tab));
    }
    session
        .workspaces
        .iter()
        .enumerate()
        .find_map(|(workspace, entry)| {
            entry
                .tabs
                .iter()
                .position(|tab| tab.panes.iter().any(|pane| pane.id == pane_id))
                .map(|tab| AgentTabLocation::Stashed { workspace, tab })
        })
}

pub(super) fn route_agent_input(
    terminal: &super::Terminal,
    runtime: Option<&HerdrRuntime>,
    pane_id: &str,
    bytes: &[u8],
) -> Option<bool> {
    matches!(terminal, super::Terminal::HerdrAgent(_)).then(|| {
        runtime.is_some_and(|runtime| runtime.write_input_to_pane(pane_id, bytes.to_vec()))
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct HerdrAgentRenderInput {
    pub(super) key: AgentKey,
    pub(super) phase: AgentPhase,
    pub(super) control: ControlOwnership,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct HerdrSpaceRenderInput {
    pub(super) id: SpaceId,
    pub(super) agents: Vec<HerdrAgentRenderInput>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct HerdrCatalogRenderInput {
    pub(super) connection: ConnectionState,
    pub(super) origin: Option<ServiceOrigin>,
    pub(super) spaces: Vec<HerdrSpaceRenderInput>,
}

impl HerdrCatalogRenderInput {
    fn from_mirror(mirror: &CatalogMirror, origin: Option<ServiceOrigin>) -> Self {
        Self {
            connection: mirror.connection(),
            origin,
            spaces: mirror
                .snapshot()
                .spaces()
                .iter()
                .map(|space| HerdrSpaceRenderInput {
                    id: space.id().clone(),
                    agents: space
                        .agents()
                        .iter()
                        .map(|agent| HerdrAgentRenderInput {
                            key: AgentKey {
                                space: space.id().clone(),
                                agent: agent.id().clone(),
                            },
                            phase: agent.phase(),
                            control: agent.control(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum HerdrSidebarRenderInput {
    Disabled,
    AwaitingController,
    Catalog(HerdrCatalogRenderInput),
}

pub(super) enum SidebarContent {
    Workspaces,
    Herdr(HerdrSidebarRenderInput),
}

pub(super) struct AttachedAgentOutput {
    pub(super) pane_id: String,
    pub(super) bytes: Vec<u8>,
}

pub(super) struct ReadyAgentTab {
    pub(super) key: AgentKey,
    pub(super) attachment: WritableAttachment,
    pub(super) initial_output: Vec<Vec<u8>>,
}

#[derive(Default)]
pub(super) struct HerdrDrain {
    pub(super) should_redraw: bool,
    pub(super) output: Vec<AttachedAgentOutput>,
    pub(super) ready_tabs: Vec<ReadyAgentTab>,
    pub(super) open_failures: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CreateAgentFormRenderInput {
    pub(super) space: SpaceId,
    pub(super) program: String,
    pub(super) argv: Vec<String>,
    pub(super) pending: bool,
    pub(super) error: Option<String>,
}

struct CreateAgentFormState {
    space: SpaceId,
    program: String,
    argv: Vec<String>,
    pending: Option<RequestId>,
    error: Option<String>,
}

impl CreateAgentFormState {
    fn render_input(&self) -> CreateAgentFormRenderInput {
        CreateAgentFormRenderInput {
            space: self.space.clone(),
            program: self.program.clone(),
            argv: self.argv.clone(),
            pending: self.pending.is_some(),
            error: self.error.clone(),
        }
    }
}

struct AgentPane {
    pane_id: String,
    attachment: WritableAttachment,
}

struct PendingAgentOpen {
    key: AgentKey,
    ticket: AttachTicket,
}

pub(super) struct HerdrRuntime {
    controller: Option<HerdrController>,
    agent_panes: BTreeMap<AgentKey, AgentPane>,
    pending_opens: HashMap<RequestId, PendingAgentOpen>,
    create_form: Option<CreateAgentFormState>,
    origin: Option<ServiceOrigin>,
}

impl HerdrRuntime {
    pub(super) fn from_saved_preference(preference: Option<bool>) -> Option<Self> {
        matches!(preference, Some(true)).then(Self::awaiting_controller)
    }

    fn awaiting_controller() -> Self {
        Self {
            controller: None,
            agent_panes: BTreeMap::new(),
            pending_opens: HashMap::new(),
            create_form: None,
            origin: None,
        }
    }

    pub(super) fn sync_saved_preference(
        runtime: &mut Option<Self>,
        preference: Option<bool>,
    ) -> bool {
        let enabled = matches!(preference, Some(true));
        match (enabled, runtime.is_some()) {
            (true, false) => {
                *runtime = Some(Self::awaiting_controller());
                true
            }
            (false, true) => {
                *runtime = None;
                true
            }
            _ => false,
        }
    }

    pub(super) fn sidebar_input(&self) -> HerdrSidebarRenderInput {
        let Some(controller) = self.controller.as_ref() else {
            return HerdrSidebarRenderInput::AwaitingController;
        };
        HerdrSidebarRenderInput::Catalog(HerdrCatalogRenderInput::from_mirror(
            controller.catalog(),
            self.origin,
        ))
    }

    pub(super) fn create_form_input(&self) -> Option<CreateAgentFormRenderInput> {
        self.create_form
            .as_ref()
            .map(CreateAgentFormState::render_input)
    }

    pub(super) fn begin_create(&mut self, space: SpaceId) {
        self.create_form = Some(CreateAgentFormState {
            space,
            program: String::new(),
            argv: vec![String::new()],
            pending: None,
            error: None,
        });
    }

    pub(super) fn cancel_create(&mut self) {
        if self
            .create_form
            .as_ref()
            .is_some_and(|form| form.pending.is_none())
        {
            self.create_form = None;
        }
    }

    pub(super) fn update_create_program(&mut self, program: String) {
        if let Some(form) = self
            .create_form
            .as_mut()
            .filter(|form| form.pending.is_none())
        {
            form.program = program;
            form.error = None;
        }
    }

    pub(super) fn update_create_argument(&mut self, index: usize, argument: String) {
        let Some(form) = self
            .create_form
            .as_mut()
            .filter(|form| form.pending.is_none())
        else {
            return;
        };
        if let Some(slot) = form.argv.get_mut(index) {
            *slot = argument;
            form.error = None;
        }
    }

    pub(super) fn add_create_argument(&mut self) {
        if let Some(form) = self
            .create_form
            .as_mut()
            .filter(|form| form.pending.is_none())
        {
            form.argv.push(String::new());
        }
    }

    pub(super) fn remove_create_argument(&mut self, index: usize) {
        let Some(form) = self
            .create_form
            .as_mut()
            .filter(|form| form.pending.is_none())
        else {
            return;
        };
        if index < form.argv.len() {
            form.argv.remove(index);
        }
    }

    pub(super) fn submit_create(&mut self) {
        self.submit_create_with_request_id(RequestId::new());
    }

    fn submit_create_with_request_id(&mut self, request_id: RequestId) {
        let Some(form) = self
            .create_form
            .as_mut()
            .filter(|form| form.pending.is_none())
        else {
            return;
        };
        if form.program.is_empty() {
            form.error = Some("Program is required".to_string());
            return;
        }
        let Some(controller) = self.controller.as_mut() else {
            form.error = Some("Herdr service connection is unavailable".to_string());
            return;
        };
        let command = AgentCommand {
            program: form.program.clone(),
            argv: form.argv.clone(),
        };
        match controller.create_agent(request_id, form.space.clone(), command) {
            Ok(()) => form.pending = Some(request_id),
            Err(_) => form.error = Some("Could not create Agent".to_string()),
        }
    }

    fn request_open(&mut self, key: AgentKey) -> Result<(), &'static str> {
        self.request_open_with_id(RequestId::new(), key)
    }

    fn request_open_with_id(
        &mut self,
        request_id: RequestId,
        key: AgentKey,
    ) -> Result<(), &'static str> {
        if self.agent_panes.contains_key(&key)
            || self
                .pending_opens
                .values()
                .any(|pending| pending.key == key)
        {
            return Ok(());
        }
        let Some(controller) = self.controller.as_mut() else {
            return Err("Herdr service connection is unavailable");
        };
        let ticket = controller
            .request_attach(request_id, key.clone())
            .map_err(|_| "Could not attach to Agent")?;
        self.pending_opens
            .insert(request_id, PendingAgentOpen { key, ticket });
        Ok(())
    }

    fn register_agent_tab(
        &mut self,
        key: AgentKey,
        pane_id: String,
        attachment: WritableAttachment,
    ) {
        self.agent_panes.insert(
            key,
            AgentPane {
                pane_id,
                attachment,
            },
        );
    }

    pub(super) fn write_input_to_pane(&self, pane_id: &str, bytes: Vec<u8>) -> bool {
        self.agent_panes
            .values()
            .find(|pane| pane.pane_id == pane_id)
            .is_some_and(|pane| pane.attachment.write_input(bytes).is_ok())
    }

    fn agent_pane_id(&self, key: &AgentKey) -> Option<String> {
        self.agent_panes.get(key).map(|pane| pane.pane_id.clone())
    }

    fn remove_agent_tab(&mut self, key: &AgentKey) {
        self.agent_panes.remove(key);
    }

    pub(super) fn drain(&mut self) -> HerdrDrain {
        let Some(controller) = self.controller.as_mut() else {
            return HerdrDrain::default();
        };

        let mut drain = HerdrDrain::default();
        for event in controller.poll() {
            match event {
                HerdrEvent::AttachmentOutput { agent, bytes } => {
                    if let Some(pane) = self.agent_panes.get(&agent) {
                        drain.output.push(AttachedAgentOutput {
                            pane_id: pane.pane_id.clone(),
                            bytes,
                        });
                    } else if let Some(ready) =
                        drain.ready_tabs.iter_mut().find(|ready| ready.key == agent)
                    {
                        ready.initial_output.push(bytes);
                    }
                }
                HerdrEvent::CreateResolved { request_id, result } => {
                    let matches_pending = self
                        .create_form
                        .as_ref()
                        .is_some_and(|form| form.pending == Some(request_id));
                    if matches_pending {
                        match result {
                            Ok(_) => self.create_form = None,
                            Err(failure) => {
                                if let Some(form) = self.create_form.as_mut() {
                                    form.pending = None;
                                    form.error = Some(create_failure_message(failure));
                                }
                            }
                        }
                    }
                    drain.should_redraw = true;
                }
                HerdrEvent::AttachmentChanged(request_id) => {
                    let Some(pending) = self.pending_opens.remove(&request_id) else {
                        continue;
                    };
                    match controller.take_attachment(pending.ticket) {
                        Some(AttachResult::Writable(attachment)) => {
                            drain.ready_tabs.push(ReadyAgentTab {
                                key: pending.key,
                                attachment,
                                initial_output: Vec::new(),
                            });
                        }
                        Some(AttachResult::Conflict(conflict)) => drain
                            .open_failures
                            .push(format!("Agent is controlled by {}", conflict.holder())),
                        Some(AttachResult::Unavailable) | None => drain
                            .open_failures
                            .push("Could not attach to Agent".to_string()),
                    }
                    drain.should_redraw = true;
                }
                HerdrEvent::CatalogChanged(_)
                | HerdrEvent::ControlChanged { .. }
                | HerdrEvent::ConnectionChanged(_)
                | HerdrEvent::SessionFailed(_) => drain.should_redraw = true,
            }
        }
        drain.should_redraw |= !drain.output.is_empty();
        drain
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "Service supervision supplies the origin in P9")
    )]
    pub(super) fn set_service_origin(&mut self, origin: ServiceOrigin) {
        self.origin = Some(origin);
    }

    #[cfg(test)]
    fn with_controller(controller: HerdrController) -> Self {
        Self {
            controller: Some(controller),
            agent_panes: BTreeMap::new(),
            pending_opens: HashMap::new(),
            create_form: None,
            origin: None,
        }
    }
}

fn create_failure_message(failure: CreateAgentFailure) -> String {
    match failure {
        CreateAgentFailure::Rejected(message) => message,
        CreateAgentFailure::Unavailable => "Could not create Agent".to_string(),
    }
}

pub(super) fn sidebar_content(view: SidebarView, runtime: Option<&HerdrRuntime>) -> SidebarContent {
    match view {
        SidebarView::Workspaces => SidebarContent::Workspaces,
        SidebarView::Herdr => SidebarContent::Herdr(runtime.map_or(
            HerdrSidebarRenderInput::Disabled,
            HerdrRuntime::sidebar_input,
        )),
    }
}

#[cfg(test)]
mod tests {
    use crate::config::AppConfig;
    use crate::workspace_store::{StoredTab, StoredWorkspace};
    use gpui::{AppContext, TestAppContext, WindowOptions};
    use termy_core::{TerminalOptions, TerminalSize};
    use termy_herdr_core::{
        AgentId, AgentKey, AgentPhase, ControlOwnership, RequestId, SpaceId, fake_controller,
    };

    use super::*;

    fn open_test_terminal_view(
        cx: &mut TestAppContext,
    ) -> gpui::WindowHandle<super::super::TerminalView> {
        cx.update(|app| {
            app.open_window(WindowOptions::default(), |window, cx| {
                cx.new(|cx| {
                    super::super::TerminalView::new(
                        window,
                        cx,
                        AppConfig {
                            native_tab_persistence: false,
                            herdr_enabled: Some(true),
                            ..AppConfig::default()
                        },
                    )
                })
            })
            .expect("test terminal window should open");
        });
        cx.windows()
            .into_iter()
            .find_map(|handle| handle.downcast::<super::super::TerminalView>())
            .expect("test terminal window should exist")
    }

    fn native_test_tab(tab_id: super::super::TabId) -> super::super::TerminalTab {
        super::super::TerminalView::create_native_tab(
            tab_id,
            super::super::Terminal::new_test_display(TerminalSize::default()),
            80,
            24,
            None,
        )
    }

    fn agent_test_tab(tab_id: super::super::TabId) -> super::super::TerminalTab {
        super::super::TerminalView::create_native_tab(
            tab_id,
            super::super::Terminal::new_herdr_agent(
                TerminalSize::default(),
                TerminalOptions::default(),
            ),
            80,
            24,
            None,
        )
    }

    #[test]
    fn herdr_sidebar_views_are_mutually_exclusive() {
        assert!(matches!(
            sidebar_content(SidebarView::Workspaces, None),
            SidebarContent::Workspaces
        ));
        assert!(matches!(
            sidebar_content(SidebarView::Herdr, None),
            SidebarContent::Herdr(HerdrSidebarRenderInput::Disabled)
        ));
    }

    #[test]
    fn herdr_workspaces_arm_preserves_workspace_renderer() {
        let runtime = HerdrRuntime::from_saved_preference(Some(true));
        assert!(matches!(
            sidebar_content(SidebarView::Workspaces, runtime.as_ref()),
            SidebarContent::Workspaces
        ));
    }

    #[test]
    fn herdr_runtime_exists_only_for_an_explicit_saved_enablement() {
        assert!(HerdrRuntime::from_saved_preference(None).is_none());
        assert!(HerdrRuntime::from_saved_preference(Some(false)).is_none());
        assert!(HerdrRuntime::from_saved_preference(Some(true)).is_some());
    }

    #[test]
    fn herdr_live_status_is_derived_from_the_drained_catalog_mirror() {
        let (controller, handle) = fake_controller();
        let mut runtime = HerdrRuntime::with_controller(controller);
        runtime.set_service_origin(ServiceOrigin::StartedByTermy);

        handle.seed_catalog_rows(vec![(
            SpaceId::new("space-a"),
            vec![(
                AgentId::new("agent-a"),
                AgentPhase::Starting,
                ControlOwnership::AnotherClient,
            )],
        )]);
        assert!(runtime.drain().should_redraw);
        assert_eq!(agent_phase(&runtime.sidebar_input()), AgentPhase::Starting);

        handle.seed_catalog_rows(vec![(
            SpaceId::new("space-a"),
            vec![(
                AgentId::new("agent-a"),
                AgentPhase::Running,
                ControlOwnership::AnotherClient,
            )],
        )]);
        assert!(runtime.drain().should_redraw);
        assert_eq!(agent_phase(&runtime.sidebar_input()), AgentPhase::Running);

        let HerdrSidebarRenderInput::Catalog(input) = runtime.sidebar_input() else {
            panic!("expected catalog render input");
        };
        assert_eq!(input.origin, Some(ServiceOrigin::StartedByTermy));
        assert_eq!(input.connection, ConnectionState::Disconnected);
        assert_eq!(
            input.spaces[0].agents[0].control,
            ControlOwnership::AnotherClient
        );
    }

    #[test]
    fn create_form_preserves_exact_argument_tokens() {
        let request_id = RequestId::new();
        let space = SpaceId::new("space-a");
        let key = AgentKey {
            space: space.clone(),
            agent: AgentId::new("agent-a"),
        };
        let (controller, handle) = fake_controller();
        handle.set_create_outcome(request_id, Ok(key));
        let mut runtime = HerdrRuntime::with_controller(controller);
        runtime.begin_create(space.clone());
        runtime.update_create_program("agent-program".to_string());
        for (index, token) in ["a b", "; echo x", "$HOME", "\"quoted\""]
            .into_iter()
            .enumerate()
        {
            if index > 0 {
                runtime.add_create_argument();
            }
            runtime.update_create_argument(index, token.to_string());
        }

        runtime.submit_create_with_request_id(request_id);

        assert_eq!(
            handle.created_commands(),
            vec![(
                space,
                AgentCommand {
                    program: "agent-program".to_string(),
                    argv: vec![
                        "a b".to_string(),
                        "; echo x".to_string(),
                        "$HOME".to_string(),
                        "\"quoted\"".to_string(),
                    ],
                },
            )]
        );
        assert!(runtime.drain().should_redraw);
        assert!(runtime.create_form_input().is_none());
    }

    #[test]
    fn failed_create_keeps_the_form_and_catalog_unchanged() {
        let request_id = RequestId::new();
        let space = SpaceId::new("space-a");
        let (controller, handle) = fake_controller();
        handle.set_create_outcome(
            request_id,
            Err(CreateAgentFailure::Rejected("invalid program".to_string())),
        );
        let mut runtime = HerdrRuntime::with_controller(controller);
        runtime.begin_create(space);
        runtime.update_create_program("bad-program".to_string());

        runtime.submit_create_with_request_id(request_id);
        assert!(runtime.drain().should_redraw);

        let form = runtime.create_form_input().expect("failed form stays open");
        assert!(!form.pending);
        assert_eq!(form.error.as_deref(), Some("invalid program"));
        let HerdrSidebarRenderInput::Catalog(catalog) = runtime.sidebar_input() else {
            panic!("expected catalog");
        };
        assert!(catalog.spaces.is_empty());
    }

    #[test]
    fn agent_binding_routes_keyboard_and_mouse_bytes_and_output() {
        let key = AgentKey {
            space: SpaceId::new("space-a"),
            agent: AgentId::new("agent-a"),
        };
        let request_id = RequestId::new();
        let (controller, handle) = fake_controller();
        handle.set_writable(key.clone(), 7);
        let mut runtime = HerdrRuntime::with_controller(controller);
        runtime
            .request_open_with_id(request_id, key.clone())
            .expect("request fake attach");
        let mut attached = runtime.drain();
        assert_eq!(attached.ready_tabs.len(), 1);
        let ready = attached.ready_tabs.remove(0);
        let terminal = super::super::Terminal::new_herdr_agent(
            TerminalSize::default(),
            TerminalOptions::default(),
        );
        let native = super::super::Terminal::new_test_display(TerminalSize::default());
        assert_eq!(
            route_agent_input(&native, Some(&runtime), "native-pane", b"native"),
            None
        );
        assert_eq!(
            route_agent_input(
                &terminal,
                Some(&runtime),
                "pane-without-binding",
                b"discarded",
            ),
            Some(false)
        );
        runtime.register_agent_tab(ready.key, "pane-a".to_string(), ready.attachment);

        assert_eq!(
            route_agent_input(&terminal, Some(&runtime), "pane-a", b"keyboard"),
            Some(true)
        );
        assert_eq!(
            route_agent_input(&terminal, Some(&runtime), "pane-a", b"mouse-packet",),
            Some(true)
        );
        runtime.drain();
        assert_eq!(
            handle.inputs_for(7),
            vec![b"keyboard".to_vec(), b"mouse-packet".to_vec()]
        );

        handle.push_output(7, b"live output".to_vec());
        let drain = runtime.drain();
        assert!(drain.should_redraw);
        assert_eq!(drain.output.len(), 1);
        assert_eq!(drain.output[0].pane_id, "pane-a");
        assert_eq!(drain.output[0].bytes, b"live output");
    }

    #[test]
    fn agent_tab_lookup_covers_active_and_stashed_workspaces() {
        let terminal = super::super::Terminal::new_herdr_agent(
            TerminalSize::default(),
            TerminalOptions::default(),
        );
        let tab = super::super::TerminalView::create_native_tab(7, terminal, 80, 24, None);
        let pane_id = tab.active_pane_id.clone();
        let mut session = super::super::SessionState::new();
        session.tabs.push(tab);
        assert_eq!(
            find_agent_tab(&session, &pane_id),
            Some(AgentTabLocation::Active(0))
        );

        let tab = session.tabs.remove(0);
        session
            .workspaces
            .push(super::super::workspaces::WorkspaceEntry::new(2));
        session.workspaces[1].tabs.push(tab);
        assert_eq!(
            find_agent_tab(&session, &pane_id),
            Some(AgentTabLocation::Stashed {
                workspace: 1,
                tab: 0,
            })
        );
    }

    #[test]
    fn repeated_open_while_attach_is_pending_does_not_enqueue_a_second_attach() {
        let key = AgentKey {
            space: SpaceId::new("space-a"),
            agent: AgentId::new("agent-a"),
        };
        let (controller, handle) = fake_controller();
        handle.set_writable(key.clone(), 7);
        let mut runtime = HerdrRuntime::with_controller(controller);

        runtime
            .request_open_with_id(RequestId::new(), key.clone())
            .expect("first attach");
        runtime
            .request_open_with_id(RequestId::new(), key)
            .expect("deduplicated attach");

        assert_eq!(runtime.pending_opens.len(), 1);
    }

    #[gpui::test]
    fn tmux_open_entry_rejects_before_touching_herdr_or_session_state(cx: &mut TestAppContext) {
        let key = AgentKey {
            space: SpaceId::new("space-a"),
            agent: AgentId::new("agent-a"),
        };
        let (controller, handle) = fake_controller();
        handle.seed_catalog_rows(vec![(
            key.space.clone(),
            vec![(
                key.agent.clone(),
                AgentPhase::Running,
                ControlOwnership::ThisClient,
            )],
        )]);
        handle.set_writable(key.clone(), 7);
        let terminal_view = open_test_terminal_view(cx);

        terminal_view
            .update(cx, |view, _window, cx| {
                let mut runtime = HerdrRuntime::with_controller(controller);
                assert!(runtime.drain().should_redraw);
                view.herdr = Some(runtime);
                view.agent_open_runtime_kind_override = Some(super::super::RuntimeKind::Tmux);
                let session_before = (
                    view.session.active_workspace,
                    view.session.active_tab,
                    view.session.tabs.len(),
                    view.session.workspaces.len(),
                );
                let catalog_before = view.herdr.as_ref().unwrap().sidebar_input();

                view.open_or_focus_agent(key, cx);

                assert_eq!(
                    (
                        view.session.active_workspace,
                        view.session.active_tab,
                        view.session.tabs.len(),
                        view.session.workspaces.len(),
                    ),
                    session_before
                );
                let runtime = view.herdr.as_ref().unwrap();
                assert_eq!(runtime.sidebar_input(), catalog_before);
                assert!(runtime.agent_panes.is_empty());
                assert!(runtime.pending_opens.is_empty());
            })
            .expect("update test terminal view");

        assert_eq!(handle.attach_requests(), 0);
        assert_eq!(handle.attach_effects(), 0);
        assert_eq!(handle.create_effects(), 0);
        assert_eq!(handle.detach_effects(), 0);
        assert_eq!(handle.close_effects(), 0);
    }

    #[gpui::test]
    fn stashed_agent_open_entry_switches_to_its_workspace_and_tab(cx: &mut TestAppContext) {
        let key = AgentKey {
            space: SpaceId::new("space-a"),
            agent: AgentId::new("agent-a"),
        };
        let (controller, handle) = fake_controller();
        handle.set_writable(key.clone(), 7);
        let terminal_view = open_test_terminal_view(cx);

        terminal_view
            .update(cx, |view, _window, cx| {
                let mut runtime = HerdrRuntime::with_controller(controller);
                runtime
                    .request_open_with_id(RequestId::new(), key.clone())
                    .expect("request fake attach");
                let ready = runtime.drain().ready_tabs.remove(0);
                let agent_tab = agent_test_tab(12);
                let pane_id = agent_tab.active_pane_id.clone();
                runtime.register_agent_tab(ready.key, pane_id.clone(), ready.attachment);
                view.herdr = Some(runtime);
                view.session.tabs = vec![native_test_tab(10), native_test_tab(11)];
                view.session.active_tab = 0;
                view.session.workspaces = vec![
                    super::super::workspaces::WorkspaceEntry::new(1),
                    super::super::workspaces::WorkspaceEntry::new(2),
                ];
                view.session.workspaces[1].tabs = vec![native_test_tab(13), agent_tab];
                view.session.workspaces[1].active_tab = 0;

                view.open_or_focus_agent(key, cx);

                assert_eq!(view.session.active_workspace, 1);
                assert_eq!(view.session.active_tab, 1);
                assert_eq!(view.active_pane_id(), Some(pane_id.as_str()));
            })
            .expect("update test terminal view");

        assert_eq!(handle.attach_effects(), 1);
    }

    #[gpui::test]
    fn failed_stashed_workspace_switch_does_not_focus_a_current_workspace_tab(
        cx: &mut TestAppContext,
    ) {
        let key = AgentKey {
            space: SpaceId::new("space-a"),
            agent: AgentId::new("agent-a"),
        };
        let (controller, handle) = fake_controller();
        handle.set_writable(key.clone(), 7);
        let terminal_view = open_test_terminal_view(cx);

        terminal_view
            .update(cx, |view, _window, cx| {
                let mut runtime = HerdrRuntime::with_controller(controller);
                runtime
                    .request_open_with_id(RequestId::new(), key.clone())
                    .expect("request fake attach");
                let ready = runtime.drain().ready_tabs.remove(0);
                let agent_tab = agent_test_tab(22);
                let pane_id = agent_tab.active_pane_id.clone();
                runtime.register_agent_tab(ready.key, pane_id, ready.attachment);
                view.herdr = Some(runtime);
                view.session.tabs = vec![native_test_tab(20), native_test_tab(21)];
                view.session.active_tab = 0;
                let active_pane_before = view.active_pane_id().unwrap().to_string();
                view.session.workspaces = vec![
                    super::super::workspaces::WorkspaceEntry::new(1),
                    super::super::workspaces::WorkspaceEntry::new(2),
                ];
                view.session.workspaces[1].tabs = vec![native_test_tab(23), agent_tab];
                view.session.workspaces[1].pending_restore = Some(StoredWorkspace {
                    name: "broken".to_string(),
                    pinned: false,
                    active_tab: 0,
                    tabs: vec![StoredTab {
                        pinned: false,
                        manual_title: None,
                        active_pane: 0,
                        layout_tree_json: None,
                        panes: Vec::new(),
                    }],
                });

                view.open_or_focus_agent(key, cx);

                assert_eq!(view.session.active_workspace, 0);
                assert_eq!(view.session.active_tab, 0);
                assert_eq!(view.active_pane_id(), Some(active_pane_before.as_str()));
            })
            .expect("update test terminal view");

        assert_eq!(handle.attach_effects(), 1);
    }

    fn agent_phase(input: &HerdrSidebarRenderInput) -> AgentPhase {
        let HerdrSidebarRenderInput::Catalog(input) = input else {
            panic!("expected catalog render input");
        };
        input.spaces[0].agents[0].phase
    }
}
