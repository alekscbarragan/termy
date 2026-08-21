use std::collections::BTreeMap;

use termy_herdr_core::{
    AgentKey, AgentPhase, CatalogMirror, ConnectionState, ControlOwnership, HerdrController,
    HerdrEvent,
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

    pub(in crate::terminal_view) fn process_herdr_events(&mut self) -> bool {
        let Some(runtime) = self.herdr.as_mut() else {
            return false;
        };
        let drain = runtime.drain();
        for output in drain.output {
            if let Some(terminal) = self.herdr_agent_terminal_by_pane_id(&output.pane_id) {
                terminal.feed_output(&output.bytes);
            }
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct HerdrAgentRenderInput {
    pub(super) id: String,
    pub(super) phase: AgentPhase,
    pub(super) control: ControlOwnership,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct HerdrSpaceRenderInput {
    pub(super) id: String,
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
                    id: space.id().as_str().to_string(),
                    agents: space
                        .agents()
                        .iter()
                        .map(|agent| HerdrAgentRenderInput {
                            id: agent.id().as_str().to_string(),
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

#[derive(Default)]
pub(super) struct HerdrDrain {
    pub(super) should_redraw: bool,
    pub(super) output: Vec<AttachedAgentOutput>,
}

pub(super) struct HerdrRuntime {
    controller: Option<HerdrController>,
    agent_panes: BTreeMap<AgentKey, String>,
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

    pub(super) fn drain(&mut self) -> HerdrDrain {
        let Some(controller) = self.controller.as_mut() else {
            return HerdrDrain::default();
        };

        let mut drain = HerdrDrain::default();
        for event in controller.poll() {
            match event {
                HerdrEvent::AttachmentOutput { agent, bytes } => {
                    if let Some(pane_id) = self.agent_panes.get(&agent) {
                        drain.output.push(AttachedAgentOutput {
                            pane_id: pane_id.clone(),
                            bytes,
                        });
                    }
                }
                HerdrEvent::CatalogChanged(_)
                | HerdrEvent::ControlChanged { .. }
                | HerdrEvent::ConnectionChanged(_)
                | HerdrEvent::CreateResolved { .. }
                | HerdrEvent::AttachmentChanged(_)
                | HerdrEvent::SessionFailed(_) => drain.should_redraw = true,
            }
        }
        drain.should_redraw |= !drain.output.is_empty();
        drain
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "Agent tabs register their pane mapping in P7")
    )]
    pub(super) fn attach_agent_pane(&mut self, agent: AgentKey, pane_id: String) {
        self.agent_panes.insert(agent, pane_id);
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
            origin: None,
        }
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
    use termy_herdr_core::{
        AgentId, AgentKey, AgentPhase, ControlOwnership, RequestId, SpaceId, fake_controller,
    };

    use super::*;

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
    fn herdr_drain_routes_attached_agent_output_to_its_pane() {
        let key = AgentKey {
            space: SpaceId::new("space-a"),
            agent: AgentId::new("agent-a"),
        };
        let (mut controller, handle) = fake_controller();
        handle.set_writable(key.clone(), 7);
        let _attach_ticket = controller
            .request_attach(RequestId::new(), key.clone())
            .expect("enqueue fake attach");

        let mut runtime = HerdrRuntime::with_controller(controller);
        runtime.attach_agent_pane(key, "pane-a".to_string());
        assert!(runtime.drain().should_redraw);

        handle.push_output(7, b"live output".to_vec());
        let drain = runtime.drain();
        assert!(drain.should_redraw);
        assert_eq!(drain.output.len(), 1);
        assert_eq!(drain.output[0].pane_id, "pane-a");
        assert_eq!(drain.output[0].bytes, b"live output");
    }

    fn agent_phase(input: &HerdrSidebarRenderInput) -> AgentPhase {
        let HerdrSidebarRenderInput::Catalog(input) = input else {
            panic!("expected catalog render input");
        };
        input.spaces[0].agents[0].phase
    }
}
