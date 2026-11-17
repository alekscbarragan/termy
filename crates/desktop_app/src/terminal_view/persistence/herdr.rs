use super::*;

impl TerminalView {
    /// Herdr Agent tabs are never restored, so they stay out of the saved
    /// Workspace; the active index moves to the nearest kept tab.
    pub(super) fn persistable_tabs(
        source_tabs: &[TerminalTab],
        active_tab: usize,
    ) -> (Vec<&TerminalTab>, usize) {
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
        (
            persistable_tabs.into_iter().map(|(_, tab)| tab).collect(),
            persisted_active_tab,
        )
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
        let source_tabs = [first, agent, second];

        let (kept, active_tab) = TerminalView::persistable_tabs(&source_tabs, 1);

        assert_eq!(kept.len(), 2);
        assert_eq!(active_tab, 1);
        assert_eq!(kept[0].manual_title.as_deref(), Some("Native A"));
        assert_eq!(kept[1].manual_title.as_deref(), Some("Native B"));
    }
}
