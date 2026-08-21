use super::super::*;
use super::render_palette::TabStripPalette;
use crate::terminal_view::herdr::{
    HerdrCatalogRenderInput, HerdrSidebarRenderInput, ServiceOrigin, SidebarContent, SidebarView,
};
use termy_herdr_core::{AgentPhase, ConnectionState, ControlOwnership};
use termy_ui::SegmentedControl;

impl TerminalView {
    pub(crate) fn render_sidebar(
        &mut self,
        colors: &TerminalColors,
        font_family: &SharedString,
        sidebar_bg: gpui::Rgba,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sidebar_width = if self.workspace_sidebar_overlay_visible() {
            self.workspace_sidebar_width
        } else {
            self.workspace_sidebar_width()
        };
        let selected = self.selected_sidebar_view();
        let selector = SegmentedControl::new("sidebar-view-selector")
            .width(px((sidebar_width - 16.0).max(0.0)))
            .option("Workspaces", selected == SidebarView::Workspaces)
            .on_click(cx.listener(|this, _event, _window, cx| {
                this.select_sidebar_view(SidebarView::Workspaces, cx);
            }))
            .option("Herdr", selected == SidebarView::Herdr)
            .on_click(cx.listener(|this, _event, _window, cx| {
                this.select_sidebar_view(SidebarView::Herdr, cx);
            }));
        let content = match self.selected_sidebar_content() {
            SidebarContent::Workspaces => {
                self.render_workspace_sidebar(colors, font_family, sidebar_bg, cx)
            }
            SidebarContent::Herdr(input) => {
                self.render_herdr_sidebar(input, colors, font_family, sidebar_bg)
            }
        };

        div()
            .id("sidebar")
            .relative()
            .flex_none()
            .w(px(sidebar_width))
            .h_full()
            .flex()
            .flex_col()
            .bg(sidebar_bg)
            .child(
                div()
                    .id("sidebar-view-selector-row")
                    .flex_none()
                    .w_full()
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(selector),
            )
            .child(div().flex_1().min_h(px(0.0)).w_full().child(content))
            .into_any_element()
    }

    pub(super) fn workspace_sidebar_actions_visible(&self) -> bool {
        self.workspace_sidebar_visible() && self.selected_sidebar_view() == SidebarView::Workspaces
    }

    fn render_herdr_sidebar(
        &self,
        input: HerdrSidebarRenderInput,
        colors: &TerminalColors,
        font_family: &SharedString,
        sidebar_bg: gpui::Rgba,
    ) -> AnyElement {
        let palette = self.resolve_tab_strip_palette(colors, sidebar_bg);
        let content = match input {
            HerdrSidebarRenderInput::Disabled => {
                self.render_herdr_message("Herdr integration is disabled", font_family, &palette)
            }
            HerdrSidebarRenderInput::AwaitingController => self.render_herdr_message(
                "Herdr service connection is unavailable",
                font_family,
                &palette,
            ),
            HerdrSidebarRenderInput::Catalog(input) => {
                self.render_herdr_catalog(input, colors, font_family, &palette)
            }
        };

        div()
            .id("herdr-sidebar")
            .relative()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(palette.tab_stroke_color)
            .child(content)
            .into_any_element()
    }

    fn render_herdr_catalog(
        &self,
        input: HerdrCatalogRenderInput,
        colors: &TerminalColors,
        font_family: &SharedString,
        palette: &TabStripPalette,
    ) -> AnyElement {
        let connection = (input.connection == ConnectionState::Disconnected).then(|| {
            self.render_herdr_marker(
                "herdr-connection-lost",
                "Connection lost",
                colors.ansi[1],
                font_family,
            )
        });
        let origin = (input.origin == Some(ServiceOrigin::StartedByTermy)).then(|| {
            self.render_herdr_marker(
                "herdr-service-origin",
                "Service started by Termy",
                colors.ansi[2],
                font_family,
            )
        });

        let mut rows = div()
            .id("herdr-sidebar-rows")
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .overflow_y_scroll()
            .px(px(WORKSPACE_SIDEBAR_PADDING_X))
            .py(px(WORKSPACE_SIDEBAR_PADDING_Y))
            .children(connection)
            .children(origin);

        if input.spaces.is_empty() {
            rows = rows.child(self.render_herdr_message("No Spaces", font_family, palette));
        }

        for (space_index, space) in input.spaces.into_iter().enumerate() {
            let mut space_rows = div()
                .id(("herdr-space", space_index))
                .w_full()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .mb(px(10.0))
                .child(
                    div()
                        .font_family(font_family.clone())
                        .text_size(px(11.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(palette.active_tab_text)
                        .child(space.id),
                );

            for (agent_index, agent) in space.agents.into_iter().enumerate() {
                let phase_color = Self::herdr_phase_color(agent.phase, colors);
                let mut secondary_text = palette.inactive_tab_text;
                secondary_text.a = secondary_text.a.max(0.58);
                space_rows = space_rows.child(
                    div()
                        .id(("herdr-agent", space_index * 10_000 + agent_index))
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .px(px(8.0))
                        .py(px(6.0))
                        .rounded(px(TAB_ITEM_RADIUS))
                        .bg(palette.inactive_tab_bg)
                        .child(
                            div()
                                .w_full()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(div().w(px(6.0)).h(px(6.0)).rounded_full().bg(phase_color))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .font_family(font_family.clone())
                                        .text_size(px(12.0))
                                        .text_color(palette.active_tab_text)
                                        .child(agent.id),
                                )
                                .child(
                                    div()
                                        .font_family(font_family.clone())
                                        .text_size(px(10.0))
                                        .text_color(phase_color)
                                        .child(Self::herdr_phase_label(agent.phase)),
                                ),
                        )
                        .child(
                            div()
                                .font_family(font_family.clone())
                                .text_size(px(10.0))
                                .text_color(secondary_text)
                                .child(Self::herdr_ownership_label(agent.control)),
                        ),
                );
            }
            rows = rows.child(space_rows);
        }

        rows.into_any_element()
    }

    fn render_herdr_marker(
        &self,
        id: &'static str,
        label: &'static str,
        color: gpui::Rgba,
        font_family: &SharedString,
    ) -> AnyElement {
        div()
            .id(id)
            .w_full()
            .mb(px(6.0))
            .font_family(font_family.clone())
            .text_size(px(10.0))
            .text_color(color)
            .child(label)
            .into_any_element()
    }

    fn render_herdr_message(
        &self,
        message: &'static str,
        font_family: &SharedString,
        palette: &TabStripPalette,
    ) -> AnyElement {
        div()
            .w_full()
            .px(px(8.0))
            .py(px(12.0))
            .font_family(font_family.clone())
            .text_size(px(11.0))
            .text_color(palette.inactive_tab_text)
            .child(message)
            .into_any_element()
    }

    const fn herdr_phase_label(phase: AgentPhase) -> &'static str {
        match phase {
            AgentPhase::Starting => "starting",
            AgentPhase::Running => "running",
            AgentPhase::WaitingForInput => "waiting-for-input",
            AgentPhase::Succeeded => "succeeded",
            AgentPhase::Failed => "failed",
        }
    }

    const fn herdr_ownership_label(ownership: ControlOwnership) -> &'static str {
        match ownership {
            ControlOwnership::Unowned => "unowned",
            ControlOwnership::ThisClient => "controlled here",
            ControlOwnership::AnotherClient => "controlled elsewhere",
        }
    }

    fn herdr_phase_color(phase: AgentPhase, colors: &TerminalColors) -> gpui::Rgba {
        match phase {
            AgentPhase::Starting => colors.ansi[6],
            AgentPhase::Running => colors.ansi[2],
            AgentPhase::WaitingForInput => colors.ansi[3],
            AgentPhase::Succeeded => colors.ansi[2],
            AgentPhase::Failed => colors.ansi[1],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn herdr_renderer_names_exactly_five_agent_phases() {
        assert_eq!(
            [
                AgentPhase::Starting,
                AgentPhase::Running,
                AgentPhase::WaitingForInput,
                AgentPhase::Succeeded,
                AgentPhase::Failed,
            ]
            .map(TerminalView::herdr_phase_label),
            [
                "starting",
                "running",
                "waiting-for-input",
                "succeeded",
                "failed",
            ]
        );
    }

    #[test]
    fn herdr_renderer_keeps_ownership_out_of_phase_text() {
        assert_eq!(
            TerminalView::herdr_ownership_label(ControlOwnership::AnotherClient),
            "controlled elsewhere"
        );
        assert_eq!(
            TerminalView::herdr_phase_label(AgentPhase::Running),
            "running"
        );
    }
}
