use super::super::*;
use super::render_palette::TabStripPalette;
use crate::terminal_view::herdr::{
    CreateAgentFormRenderInput, HerdrCatalogRenderInput, HerdrSidebarRenderInput, ServiceOrigin,
    SidebarContent, SidebarView,
};
use termy_herdr_core::{AgentPhase, ConnectionState, ControlOwnership};
use termy_ui::{Input, SegmentedControl};

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
        let create_form = self
            .herdr
            .as_ref()
            .and_then(crate::terminal_view::herdr::HerdrRuntime::create_form_input);
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
                self.render_herdr_sidebar(input, create_form, colors, font_family, sidebar_bg, cx)
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
        &mut self,
        input: HerdrSidebarRenderInput,
        create_form: Option<CreateAgentFormRenderInput>,
        colors: &TerminalColors,
        font_family: &SharedString,
        sidebar_bg: gpui::Rgba,
        cx: &mut Context<Self>,
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
                self.render_herdr_catalog(input, create_form, colors, font_family, &palette, cx)
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
        &mut self,
        input: HerdrCatalogRenderInput,
        create_form: Option<CreateAgentFormRenderInput>,
        colors: &TerminalColors,
        font_family: &SharedString,
        palette: &TabStripPalette,
        cx: &mut Context<Self>,
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
            let space_id = space.id.clone();
            let create_space = space_id.clone();
            let mut space_rows = div()
                .id(("herdr-space", space_index))
                .w_full()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .mb(px(10.0))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .font_family(font_family.clone())
                        .text_size(px(11.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(palette.active_tab_text)
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(space_id.as_str().to_string()),
                        )
                        .child(
                            div()
                                .id(("herdr-create-agent", space_index))
                                .cursor_pointer()
                                .px(px(6.0))
                                .py(px(3.0))
                                .rounded(px(4.0))
                                .text_size(px(10.0))
                                .text_color(palette.inactive_tab_text)
                                .hover(|style| style.bg(palette.inactive_tab_bg))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.begin_herdr_create(create_space.clone(), cx);
                                }))
                                .child("+ Agent"),
                        ),
                );

            if let Some(form) = create_form
                .as_ref()
                .filter(|form| form.space == space.id)
                .cloned()
            {
                space_rows =
                    space_rows.child(self.render_herdr_create_form(form, font_family, palette, cx));
            }

            for (agent_index, agent) in space.agents.into_iter().enumerate() {
                let agent_key = agent.key.clone();
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
                        .cursor_pointer()
                        .hover(|style| style.bg(palette.active_tab_bg))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_or_focus_agent(agent_key.clone(), cx);
                        }))
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
                                        .child(agent.key.agent.as_str().to_string()),
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

    fn render_herdr_create_form(
        &mut self,
        form: CreateAgentFormRenderInput,
        font_family: &SharedString,
        palette: &TabStripPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let mut fields = div().w_full().flex().flex_col().gap(px(5.0)).child(
            Input::new(format!("herdr-program-{}", form.space.as_str()))
                .placeholder("Program")
                .value(form.program)
                .disabled(form.pending)
                .invalid(form.error.is_some())
                .on_change(move |value, _, cx| {
                    view.update(cx, |this, cx| {
                        this.update_herdr_create_program(value.to_string(), cx);
                    });
                }),
        );

        for (index, argument) in form.argv.into_iter().enumerate() {
            let input_view = cx.entity();
            let remove_view = cx.entity();
            fields = fields.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(
                        div().flex_1().min_w(px(0.0)).child(
                            Input::new(format!("herdr-argument-{}-{index}", form.space.as_str()))
                                .placeholder(format!("Argument {}", index + 1))
                                .value(argument)
                                .disabled(form.pending)
                                .on_change(move |value, _, cx| {
                                    input_view.update(cx, |this, cx| {
                                        this.update_herdr_create_argument(
                                            index,
                                            value.to_string(),
                                            cx,
                                        );
                                    });
                                }),
                        ),
                    )
                    .child(
                        div()
                            .id(("herdr-remove-argument", index))
                            .cursor_pointer()
                            .px(px(5.0))
                            .text_color(palette.inactive_tab_text)
                            .on_click(move |_, _, cx| {
                                remove_view.update(cx, |this, cx| {
                                    this.remove_herdr_create_argument(index, cx);
                                });
                            })
                            .child("−"),
                    ),
            );
        }

        let add_view = cx.entity();
        let cancel_view = cx.entity();
        let submit_view = cx.entity();
        let error = form.error.map(|message| {
            div()
                .font_family(font_family.clone())
                .text_size(px(10.0))
                .text_color(palette.active_tab_text)
                .child(message)
        });
        fields
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .font_family(font_family.clone())
                    .text_size(px(10.0))
                    .text_color(palette.inactive_tab_text)
                    .child(
                        div()
                            .id("herdr-add-argument")
                            .cursor_pointer()
                            .on_click(move |_, _, cx| {
                                add_view.update(cx, |this, cx| {
                                    this.add_herdr_create_argument(cx);
                                });
                            })
                            .child("+ Argument"),
                    )
                    .child(
                        div()
                            .id("herdr-cancel-create")
                            .cursor_pointer()
                            .on_click(move |_, _, cx| {
                                cancel_view.update(cx, |this, cx| {
                                    this.cancel_herdr_create(cx);
                                });
                            })
                            .child("Cancel"),
                    )
                    .child(
                        div()
                            .id("herdr-submit-create")
                            .cursor_pointer()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(palette.active_tab_text)
                            .on_click(move |_, _, cx| {
                                submit_view.update(cx, |this, cx| {
                                    this.submit_herdr_create(cx);
                                });
                            })
                            .child(if form.pending {
                                "Creating…"
                            } else {
                                "Create"
                            }),
                    ),
            )
            .children(error)
            .px(px(4.0))
            .py(px(5.0))
            .into_any_element()
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
