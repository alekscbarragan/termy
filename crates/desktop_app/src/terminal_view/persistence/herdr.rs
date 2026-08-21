use super::*;

impl TerminalView {
    pub(super) fn persisted_workspace_from_tabs(
        source_tabs: &[TerminalTab],
        active_tab: usize,
        layout_trees: &HashMap<TabId, NativePaneLayoutTree>,
        mut persisted_buffer: impl FnMut(&Terminal) -> Option<String>,
    ) -> PersistedNativeWorkspace {
        let persistable_tabs = source_tabs
            .iter()
            .enumerate()
            .filter(|(_, tab)| !Self::tab_contains_herdr_agent(tab))
            .collect::<Vec<_>>();
        let persisted_active_tab = if persistable_tabs.is_empty() {
            0
        } else {
            persistable_tabs
                .iter()
                .position(|(index, _)| *index == active_tab)
                .unwrap_or_else(|| {
                    persistable_tabs
                        .partition_point(|(index, _)| *index < active_tab)
                        .min(persistable_tabs.len() - 1)
                })
        };
        let tabs = persistable_tabs
            .into_iter()
            .map(|(_, tab)| {
                let panes = tab
                    .panes
                    .iter()
                    .map(|pane| PersistedNativePane {
                        left: pane.left,
                        top: pane.top,
                        width: pane.width.max(1),
                        height: pane.height.max(1),
                        buffer: persisted_buffer(&pane.terminal),
                    })
                    .collect::<Vec<_>>();
                let pane_indices = tab
                    .panes
                    .iter()
                    .enumerate()
                    .map(|(index, pane)| (pane.id.clone(), index))
                    .collect::<HashMap<_, _>>();
                let layout_tree = layout_trees
                    .get(&tab.id)
                    .and_then(|tree| {
                        Self::persisted_layout_tree_from_native(&tree.root, &pane_indices)
                    })
                    .or_else(|| {
                        Self::native_layout_tree_from_panes(&tab.panes).and_then(|tree| {
                            Self::persisted_layout_tree_from_native(&tree.root, &pane_indices)
                        })
                    });
                PersistedNativeTab {
                    panes,
                    layout_tree,
                    active_pane: tab.active_pane_index().unwrap_or(0),
                    pinned: tab.pinned,
                    manual_title: tab.manual_title.clone(),
                }
            })
            .collect::<Vec<_>>();
        PersistedNativeWorkspace {
            tabs,
            active_tab: persisted_active_tab,
        }
    }

    fn tab_contains_herdr_agent(tab: &TerminalTab) -> bool {
        tab.panes
            .iter()
            .any(|pane| matches!(pane.terminal(), Terminal::HerdrAgent(_)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use termy_core::TerminalOptions;

    #[test]
    fn agent_tabs_are_excluded_from_persist_restore_without_disturbing_native_tabs() {
        let size = TerminalSize::default();
        let mut first =
            TerminalView::create_native_tab(1, Terminal::new_test_display(size), 80, 24, None);
        let agent = TerminalView::create_native_tab(
            2,
            Terminal::new_herdr_agent(size, TerminalOptions::default()),
            80,
            24,
            Some("Herdr Agent".to_string()),
        );
        let mut second =
            TerminalView::create_native_tab(3, Terminal::new_test_display(size), 80, 24, None);
        first.manual_title = Some("Native A".to_string());
        second.manual_title = Some("Native B".to_string());
        let first_pane_id = first.active_pane_id.clone();
        let mut layout_trees = HashMap::new();
        layout_trees.insert(
            first.id,
            NativePaneLayoutTree {
                root: NativePaneLayoutNode::Leaf {
                    pane_id: first_pane_id,
                },
            },
        );

        let persisted = TerminalView::persisted_workspace_from_tabs(
            &[first, agent, second],
            1,
            &layout_trees,
            |_| None,
        );

        assert_eq!(persisted.tabs.len(), 2);
        assert_eq!(persisted.active_tab, 1);
        assert_eq!(persisted.tabs[0].manual_title.as_deref(), Some("Native A"));
        assert_eq!(persisted.tabs[1].manual_title.as_deref(), Some("Native B"));
        assert!(persisted.tabs[0].layout_tree.is_some());
        let encoded = TerminalView::persisted_workspace_to_value(persisted);
        let restored = TerminalView::parse_persisted_native_workspace_value(&encoded)
            .expect("persisted native-only Workspace should restore");
        assert_eq!(restored.tabs.len(), 2);
        assert_eq!(restored.active_tab, 1);
        assert_eq!(restored.tabs[0].manual_title.as_deref(), Some("Native A"));
        assert_eq!(restored.tabs[1].manual_title.as_deref(), Some("Native B"));
        assert!(restored.tabs[0].layout_tree.is_some());
    }
}
