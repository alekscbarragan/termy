use gpui::TestAppContext;
use termy_herdr_core::{AgentId, AgentKey, RequestId, SpaceId, fake_controller};

use super::*;
use crate::terminal_view::{
    NativePaneLayoutNode, NativePaneLayoutTree, PaneMoveDragState, PaneMoveDropTarget,
    PaneResizeAxis, Terminal, TerminalView,
};

#[gpui::test]
fn native_pane_move_to_agent_tab_is_rejected_without_mutation_or_controller_effect(
    cx: &mut TestAppContext,
) {
    let key = AgentKey {
        space: SpaceId::new("space-a"),
        agent: AgentId::new("agent-a"),
    };
    let (controller, handle) = fake_controller();
    handle.set_writable(key.clone(), 7);
    let mut runtime = HerdrRuntime::with_controller(controller);
    runtime
        .request_open_with_id(RequestId::new(), key)
        .expect("request fake attach");
    let ready = runtime.drain().ready_tabs.remove(0);

    let mut source_tab = TerminalView::native_test_tab(30);
    let source_tab_id = source_tab.id;
    let remaining_pane_id = source_tab.active_pane_id.clone();
    source_tab.panes[0].width = 40;
    let mut extra_tab = TerminalView::native_test_tab(31);
    let mut moved_pane = extra_tab.panes.remove(0);
    moved_pane.left = 40;
    moved_pane.width = 40;
    let moved_pane_id = moved_pane.id.clone();
    source_tab.panes.push(moved_pane);
    source_tab.active_pane_id = moved_pane_id.clone();

    let agent_tab = TerminalView::agent_test_tab(32);
    let agent_tab_id = agent_tab.id;
    let agent_pane_id = agent_tab.active_pane_id.clone();
    runtime.register_agent_tab(ready.key, agent_pane_id.clone(), ready.attachment);

    let terminal_view = TerminalView::open_test_window(cx);
    let effects_before = (handle.detach_effects(), handle.close_effects());
    terminal_view
        .update(cx, |view, _window, cx| {
            view.herdr = Some(runtime);
            view.session.tabs = vec![source_tab, agent_tab];
            view.session.active_tab = 0;
            view.session.native_pane_layout_trees.insert(
                source_tab_id,
                NativePaneLayoutTree {
                    root: NativePaneLayoutNode::Split {
                        axis: PaneResizeAxis::Horizontal,
                        ratio: 0.5,
                        first: Box::new(NativePaneLayoutNode::Leaf {
                            pane_id: remaining_pane_id.clone(),
                        }),
                        second: Box::new(NativePaneLayoutNode::Leaf {
                            pane_id: moved_pane_id.clone(),
                        }),
                    },
                },
            );
            view.session.native_pane_layout_trees.insert(
                agent_tab_id,
                NativePaneLayoutTree {
                    root: NativePaneLayoutNode::Leaf {
                        pane_id: agent_pane_id.clone(),
                    },
                },
            );
            let active_tab_before = view.session.active_tab;
            let layout_trees_before = view.session.native_pane_layout_trees.clone();
            view.pane_move_drag = Some(PaneMoveDragState {
                pane_id: moved_pane_id.clone(),
                start_x: 0.0,
                start_y: 0.0,
                active: true,
                drop_target: Some(PaneMoveDropTarget::Tab {
                    tab_id: agent_tab_id,
                }),
            });

            assert!(view.finish_pane_move_drag(cx));

            assert_eq!(view.session.active_tab, active_tab_before);
            assert_eq!(view.session.native_pane_layout_trees, layout_trees_before);
            let source = &view.session.tabs[0];
            assert_eq!(source.id, source_tab_id);
            assert_eq!(source.active_pane_id, moved_pane_id);
            assert_eq!(source.panes.len(), 2);
            assert_eq!(source.panes[0].id, remaining_pane_id);
            assert_eq!(
                (
                    source.panes[0].left,
                    source.panes[0].top,
                    source.panes[0].width,
                    source.panes[0].height,
                ),
                (0, 0, 40, 24)
            );
            assert_eq!(source.panes[1].id, moved_pane_id);
            assert_eq!(
                (
                    source.panes[1].left,
                    source.panes[1].top,
                    source.panes[1].width,
                    source.panes[1].height,
                ),
                (40, 0, 40, 24)
            );

            let target = &view.session.tabs[1];
            assert_eq!(target.id, agent_tab_id);
            assert_eq!(target.active_pane_id, agent_pane_id);
            assert_eq!(target.panes.len(), 1);
            assert_eq!(target.panes[0].id, agent_pane_id);
            assert!(matches!(
                target.panes[0].terminal(),
                Terminal::HerdrAgent(_)
            ));
        })
        .expect("attempt Native pane move onto Agent tab");

    assert_eq!(
        (handle.detach_effects(), handle.close_effects()),
        effects_before
    );
}
