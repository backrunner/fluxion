use super::*;
use crate::ui_style::{control, scroll_mask, segment, segments};
use gpui_component::menu::{DropdownMenu, PopupMenuItem};
use gpui_component::tooltip::Tooltip;

const LIST_CONTENT_INSET: f32 = 24.;
const LIST_HEADER_HEIGHT: f32 = 60.;
const LIST_SEARCH_HEIGHT: f32 = 32.;
const LIST_SEARCH_BOTTOM_INSET: f32 = 12.;
const LIST_FILTER_HEIGHT: f32 = 32.;
const LIST_BATCH_HEIGHT: f32 = 60.;

fn ellipsize_task_name(name: &str, width: Pixels, cx: &App) -> SharedString {
    let mut name_font = font(".SystemUIFont");
    name_font.weight = FontWeight::MEDIUM;
    cx.text_system()
        .line_wrapper(name_font, px(14.))
        .truncate_line(
            name.to_owned().into(),
            width.max(px(0.)),
            "…",
            &mut Vec::new(),
        )
}

fn batch_targets(
    tasks: &[TaskSummary],
    selection_mode: bool,
    checked: &BTreeSet<TaskId>,
) -> Vec<TaskId> {
    tasks
        .iter()
        .filter(|task| !selection_mode || checked.contains(&task.id))
        .map(|task| task.id)
        .collect()
}

impl Workspace {
    pub(super) fn icon_button(&self, id: &'static str, icon: &str, label: &str) -> Button {
        // Button::icon scales icons down with the button size. An explicit child
        // keeps a 20 px glyph inside a consistent 36 px interaction target.
        Button::new(id)
            .ghost()
            .size(px(36.))
            .p_0()
            .child(icon_view(icon))
            .tooltip(self.tr(label))
    }
    pub(super) fn action_button(&self, id: &'static str, icon: &str, label: &str) -> Button {
        control(id).h(px(32.)).child(
            h_flex()
                .gap_2()
                .text_size(px(13.))
                .child(icon_view(icon).size(px(16.)))
                .child(self.tr(label)),
        )
    }
    fn nav(
        &self,
        id: &'static str,
        icon: &str,
        label: &str,
        count: usize,
        selected: bool,
        cx: &App,
    ) -> Stateful<Div> {
        let color = if selected {
            cx.theme().foreground
        } else {
            cx.theme().muted_foreground
        };
        // Sidebar states need more contrast than inset surfaces on the white
        // content pane. Keep selection visible when the pointer enters it.
        let selected_background = rgb(if cx.theme().is_dark() {
            0x303e50
        } else {
            0xcfdbe9
        });
        let hover_background = rgb(match (cx.theme().is_dark(), selected) {
            (false, false) => 0xdce3ed,
            (false, true) => 0xc1cfe1,
            (true, false) => 0x252e3b,
            (true, true) => 0x3d4e64,
        });
        h_flex()
            .id(id)
            .h(px(36.))
            .px_3()
            .gap_2p5()
            .rounded(px(7.))
            .cursor_pointer()
            .when(selected, |v| v.bg(selected_background))
            .hover(move |v| v.bg(hover_background))
            .child(icon_view(icon).size(px(18.)).text_color(if selected {
                cx.theme().primary
            } else {
                color
            }))
            .child(
                div()
                    .flex_1()
                    .text_size(px(13.))
                    .when(selected, |label| label.font_medium())
                    .text_color(color)
                    .child(self.tr(label)),
            )
            .when(count > 0, |row| {
                row.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(count.to_string()),
                )
            })
    }
    pub(super) fn sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let count = |filter: Filter| {
            self.store
                .tasks
                .iter()
                .filter(|t| filter.matches(t, &self.preferences.trash))
                .count()
        };
        let total = self.store.speeds.values().fold((0u64, 0u64), |a, b| {
            (a.0.saturating_add(b.0), a.1.saturating_add(b.1))
        });
        v_flex()
            .w(px(208.))
            .h_full()
            .flex_shrink_0()
            .child(
                div()
                    .w_full()
                    .h(px(64.))
                    .flex_shrink_0()
                    .window_control_area(WindowControlArea::Drag)
                    .child(
                        h_flex().h(px(50.)).pl(px(96.)).child(
                            div()
                                .text_size(px(14.))
                                .font_semibold()
                                .text_color(cx.theme().muted_foreground)
                                .child("Fluxion"),
                        ),
                    ),
            )
            .child(
                v_flex()
                    .px_3()
                    .gap_1()
                    .child(
                        div()
                            .px_3()
                            .pb_2()
                            .text_xs()
                            .font_medium()
                            .text_color(cx.theme().muted_foreground)
                            .child(self.tr("native.library")),
                    )
                    .child(
                        self.nav(
                            "nav-downloads",
                            "inbox",
                            "nav.downloads",
                            count(Filter::All),
                            !self.settings_open && self.filter != Filter::Trash,
                            cx,
                        )
                        .on_click(
                            cx.listener(|this, _, _, cx| this.select_filter(Filter::All, cx)),
                        ),
                    )
                    .child(
                        self.nav(
                            "nav-trash",
                            "trash-2",
                            "nav.trash",
                            count(Filter::Trash),
                            !self.settings_open && self.filter == Filter::Trash,
                            cx,
                        )
                        .on_click(
                            cx.listener(|this, _, _, cx| this.select_filter(Filter::Trash, cx)),
                        ),
                    ),
            )
            .child(div().flex_1())
            .when(self.update.is_some(), |view| {
                view.child(
                    div().px_3().pb_2().child(
                        Button::new("available-update")
                            .ghost()
                            .w_full()
                            .child(
                                h_flex()
                                    .w_full()
                                    .gap_2()
                                    .text_size(px(13.))
                                    .child(
                                        icon_view("arrow-down-to-line")
                                            .text_color(cx.theme().primary),
                                    )
                                    .child(self.tr(if self.update_prepared.is_some() {
                                        "native.restartUpdate"
                                    } else {
                                        "native.updateAvailable"
                                    })),
                            )
                            .on_click(
                                cx.listener(|this, _, window, cx| this.open_settings(window, cx)),
                            ),
                    ),
                )
            })
            .child(self.transfer_meter(total, cx))
            .child(
                div().px_3().pb_3().child(
                    self.nav(
                        "settings",
                        "settings-2",
                        "nav.settings",
                        0,
                        self.settings_open,
                        cx,
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.open_settings(window, cx))),
                ),
            )
            .into_any_element()
    }
    fn transfer_meter(&self, total: (u64, u64), cx: &App) -> Div {
        let transferring = total.0 > 0 || total.1 > 0;
        let reading = |label: String, value: u64, icon: &str, tint: Hsla| {
            h_flex()
                .gap_2()
                .child(icon_view(icon).size(px(16.)).text_color(tint))
                .child(
                    div()
                        .flex_1()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(label),
                )
                .child(div().text_size(px(13.)).font_medium().child(speed(value)))
        };
        v_flex()
            .mx_3()
            .mb_3()
            .p_3()
            .gap_2p5()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().border.opacity(0.6))
            .bg(cx.theme().background.opacity(0.65))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(self.tr("native.transferSpeed")),
                    )
                    .child(div().size(px(6.)).rounded_full().bg(if transferring {
                        state_color(&TaskState::Downloading, cx)
                    } else {
                        cx.theme().muted_foreground
                    }))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(self.tr(if transferring {
                                "native.live"
                            } else {
                                "native.idle"
                            })),
                    ),
            )
            .child(reading(
                self.tr("nav.down"),
                total.0,
                "arrow-down",
                state_color(&TaskState::Downloading, cx),
            ))
            .child(reading(
                self.tr("nav.up"),
                total.1,
                "arrow-up",
                state_color(&TaskState::Completed, cx),
            ))
    }
    pub(super) fn row(
        &self,
        task: &TaskSummary,
        pane_width: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = task.id;
        let selected = self.selected == Some(id);
        let checked = self.checked.contains(&id);
        let highlighted = selected || checked;
        let color = state_color(&task.state, cx);
        let rate = self.store.speeds.get(&id).copied().unwrap_or_default();
        let progress = percent(task.downloaded_bytes, task.total_bytes);
        let name = task
            .file_name
            .clone()
            .unwrap_or_else(|| self.tr("new.resolving"));
        let file_glyph = file_icon(&name);
        // Reserve both row insets, the action, icon and gaps before shaping.
        // GPUI's flex text can otherwise be clipped before ellipsis is measured.
        let name_width = (pane_width
            - px(48. + 32. + 12. + 20. + 8.)
            - if self.selection_mode { px(36.) } else { px(0.) })
        .max(px(0.));
        let visible_name = ellipsize_task_name(&name, name_width, cx);
        let primary = if active(&task.state) {
            TaskAction::Pause
        } else if task.state == TaskState::Completed {
            TaskAction::Open
        } else {
            TaskAction::Start
        };
        let icon = if active(&task.state) {
            "pause"
        } else if task.state == TaskState::Completed {
            "folder-open"
        } else {
            "play"
        };
        let hint = if active(&task.state) {
            "list.action.pause"
        } else if task.state == TaskState::Completed {
            "detail.openFile"
        } else {
            "list.action.start"
        };
        let show_progress = active(&task.state) || task.downloaded_bytes > 0;
        let information = v_flex()
            .flex_1()
            .min_w_0()
            .gap(px(4.))
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .h(px(24.))
                    .gap_2()
                    .child(
                        icon_view(file_glyph)
                            .size(px(20.))
                            .flex_shrink_0()
                            .text_color(if selected {
                                cx.theme().primary
                            } else {
                                cx.theme().muted_foreground
                            }),
                    )
                    .child(
                        div()
                            .id(("task-name", id.as_u128() as usize))
                            .flex_1()
                            .min_w_0()
                            .max_w(name_width)
                            .text_size(px(14.))
                            .font_medium()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .child(visible_name)
                            .tooltip(move |window, cx| {
                                Tooltip::new(name.clone()).build(window, cx)
                            }),
                    ),
            )
            .child(
                h_flex()
                    .h(px(16.))
                    .gap_2()
                    .text_xs()
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(color)
                            .child(self.tr(&format!("state.{:?}", task.state))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "{} / {}",
                                bytes(Some(task.downloaded_bytes)),
                                bytes(task.total_bytes)
                            )),
                    )
                    .when(show_progress, |row| {
                        row.child(
                            div()
                                .flex_shrink_0()
                                .text_color(cx.theme().muted_foreground)
                                .child(if active(&task.state) {
                                    if task.state == TaskState::Downloading
                                        && rate.0 > 0
                                        && task.total_bytes.is_some()
                                    {
                                        format!(
                                            "{} · {}",
                                            speed(rate.0),
                                            eta(task.downloaded_bytes, task.total_bytes, rate.0)
                                        )
                                    } else {
                                        speed(rate.0)
                                    }
                                } else {
                                    format!("{:.0}%", progress * 100.)
                                }),
                        )
                    }),
            )
            .when(show_progress, |column| {
                column.child(progress_bar(progress, color, cx))
            });
        div()
            .h(px(84.))
            .px_3()
            .py(px(2.))
            .child(
                h_flex()
                    .id(SharedString::from(id.to_string()))
                    .h_full()
                    .items_center()
                    .px_3()
                    .py(px(10.))
                    .gap_3()
                    .rounded_lg()
                    .bg(if highlighted {
                        selected_color(cx)
                    } else {
                        transparent_black()
                    })
                    .hover(|v| {
                        v.bg(if highlighted {
                            selected_color(cx)
                        } else {
                            inset_color(cx)
                        })
                    })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        if this.selection_mode || event.modifiers().platform {
                            this.selection_mode = true;
                            if !this.checked.insert(id) {
                                this.checked.remove(&id);
                            }
                            window.focus(&this.focus);
                            cx.notify();
                        } else {
                            this.select(id, window, cx);
                        }
                    }))
                    .when(self.selection_mode, |row| {
                        row.child(
                            h_flex()
                                .w(px(24.))
                                .h(px(32.))
                                .flex_shrink_0()
                                .justify_center()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .child(
                                    Checkbox::new(("check", id.as_u128() as usize))
                                        .w(px(24.))
                                        .h(px(32.))
                                        .items_center()
                                        .justify_center()
                                        .checked(checked)
                                        .on_click(cx.listener(move |this, checked, _, cx| {
                                            cx.stop_propagation();
                                            if *checked {
                                                this.checked.insert(id);
                                            } else {
                                                this.checked.remove(&id);
                                            }
                                            cx.notify();
                                        })),
                                ),
                        )
                    })
                    .child(information)
                    .child(
                        div()
                            .flex_shrink_0()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(
                                Button::new(("primary", id.as_u128() as usize))
                                    .ghost()
                                    .size(px(32.))
                                    .rounded_full()
                                    .p_0()
                                    .when(selected, |button| button.bg(cx.theme().background))
                                    .child(icon_view(icon))
                                    .tooltip(self.tr(hint))
                                    .disabled(
                                        self.busy
                                            || self.filter == Filter::Trash
                                            || !(active(&task.state)
                                                || can_start(&task.state)
                                                || task.state == TaskState::Completed),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.act(vec![id], primary, window, cx)
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }
    pub(super) fn task_list(
        &self,
        detail_progress: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tasks = self.visible(cx);
        let count = tasks.len();
        let trash = self.filter == Filter::Trash;
        // Center the empty message in the whole pane rather than just the space
        // below its controls. Reserve enough room for wrapped copy in short windows.
        let controls_height = px(LIST_HEADER_HEIGHT
            + LIST_SEARCH_HEIGHT
            + LIST_SEARCH_BOTTOM_INSET
            + if trash { 0. } else { LIST_FILTER_HEIGHT }
            + LIST_BATCH_HEIGHT);
        let empty_bottom_inset = controls_height.min(
            (window.viewport_size().height - px(24.) - controls_height - px(160.)).max(px(0.)),
        );
        let ids = batch_targets(&tasks, self.selection_mode, &self.checked);
        let pause = ids.clone();
        let start = ids.clone();
        let delete = ids.clone();
        let stop = ids.clone();
        let sort_options = [
            (0, self.tr("list.sort.added")),
            (1, self.tr("list.sort.name")),
            (2, self.tr("list.sort.speed")),
        ];
        let selected_sort = self.sort;
        let sort_view = cx.entity().downgrade();
        let mut pane = v_flex()
            .h_full()
            .min_w_0()
            .rounded_xl()
            .overflow_hidden()
            .bg(cx.theme().background);
        let available = window.viewport_size().width - px(208. + 12.);
        let pane_width = if window.viewport_size().width >= px(1020.) {
            available - (available - px(384.)) * detail_progress
        } else {
            available
        };
        if window.viewport_size().width >= px(1020.) {
            pane = pane.w(pane_width).flex_shrink_0();
        } else {
            pane = pane.flex_1();
        }
        pane = pane
            .child(
                h_flex()
                    .h(px(LIST_HEADER_HEIGHT))
                    .px(px(LIST_CONTENT_INSET))
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(17.))
                            .font_semibold()
                            .child(self.tr(if trash { "nav.trash" } else { "nav.downloads" })),
                    )
                    .child(
                        self.action_button("new-download", "plus", "native.newTask")
                            .primary()
                            .tooltip(format!("{} · ⌘N", self.tr("list.newDownload")))
                            .disabled(self.loading)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_form(FormKind::Create, window, cx)
                            })),
                    )
                    .when(self.selected.is_some(), |header| {
                        header.child(
                            self.icon_button("detail", "panel-right", "list.toggleDetail")
                                .size(px(32.))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.preferences.detail_open = !this.preferences.detail_open;
                                    this.persist(cx);
                                })),
                        )
                    }),
            )
            .child(
                h_flex()
                    .px(px(LIST_CONTENT_INSET))
                    .pb(px(LIST_SEARCH_BOTTOM_INSET))
                    .child(
                        h_flex()
                            .w_full()
                            .min_w_0()
                            .h(px(LIST_SEARCH_HEIGHT))
                            .rounded(px(7.))
                            .border_1()
                            .border_color(cx.theme().border.opacity(0.6))
                            .bg(inset_color(cx))
                            .child(
                                div().flex_1().min_w_0().child(
                                    Input::new(&self.search)
                                        .prefix(icon_view("search").size(px(16.)))
                                        .h(px(30.))
                                        .text_size(px(13.))
                                        .appearance(false)
                                        .rounded(px(6.))
                                        .bg(inset_color(cx))
                                        .cleanable(true),
                                ),
                            )
                            .child(
                                div()
                                    .w(px(1.))
                                    .h(px(16.))
                                    .flex_shrink_0()
                                    .bg(cx.theme().border),
                            )
                            .child(
                                control("sort")
                                    .ghost()
                                    .h(px(28.))
                                    .px_2()
                                    .mx(px(2.))
                                    .flex_shrink_0()
                                    .tooltip(self.tr("list.sortTasks"))
                                    .child(
                                        h_flex()
                                            .gap_1p5()
                                            .text_size(px(13.))
                                            .child(sort_options[selected_sort as usize].1.clone())
                                            .child(icon_view("chevrons-up-down").size(px(14.))),
                                    )
                                    .dropdown_menu_with_anchor(
                                        Corner::TopRight,
                                        move |mut menu, _, _| {
                                            for (value, label) in &sort_options {
                                                let value = *value;
                                                let view = sort_view.clone();
                                                menu = menu.item(
                                                    PopupMenuItem::new(label.clone())
                                                        .checked(value == selected_sort)
                                                        .on_click(move |_, _, cx| {
                                                            let _ = view.update(cx, |this, cx| {
                                                                this.sort = value;
                                                                this.list_scroll.scroll_to_item(
                                                                    0,
                                                                    ScrollStrategy::Top,
                                                                );
                                                                cx.notify();
                                                            });
                                                        }),
                                                );
                                            }
                                            menu
                                        },
                                    ),
                            ),
                    ),
            );
        if !trash {
            let mut tabs = segments(cx)
                .h(px(LIST_FILTER_HEIGHT))
                .mx(px(LIST_CONTENT_INSET));
            for (index, filter, key) in [
                (0, Filter::All, "list.filter.all"),
                (1, Filter::Active, "list.filter.active"),
                (2, Filter::Completed, "list.filter.completed"),
                (3, Filter::Failed, "list.filter.failed"),
                (4, Filter::Paused, "state.Paused"),
            ] {
                tabs = tabs.child(
                    segment(
                        ("filter", index as usize),
                        self.tr(key),
                        self.filter == filter,
                        cx,
                    )
                    .compact()
                    .h(px(28.))
                    .flex_1()
                    .on_click(cx.listener(move |this, _, _, cx| this.select_filter(filter, cx))),
                );
            }
            pane = pane.child(tabs);
        }
        pane = pane
            .child(
                h_flex()
                    .h(px(LIST_BATCH_HEIGHT))
                    .px(px(LIST_CONTENT_INSET))
                    .gap_2()
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_2()
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(if self.selection_mode {
                                        self.tr("list.selected")
                                            .replace("{count}", &ids.len().to_string())
                                    } else {
                                        self.tr("native.taskCount")
                                            .replace("{count}", &count.to_string())
                                    }),
                            )
                            .child(
                                control("selection-mode")
                                    .custom(
                                        ButtonCustomVariant::new(cx)
                                            .color(cx.theme().secondary.opacity(0.5))
                                            .border(cx.theme().border.opacity(0.65))
                                            .foreground(cx.theme().foreground)
                                            .hover(cx.theme().secondary_hover.opacity(0.65))
                                            .active(cx.theme().secondary_hover),
                                    )
                                    .h(px(28.))
                                    .px(px(10.))
                                    .flex_shrink_0()
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .font_weight(FontWeight::NORMAL)
                                            .child(self.tr(if self.selection_mode {
                                                "native.finishSelection"
                                            } else {
                                                "native.multiSelect"
                                            })),
                                    )
                                    .disabled(tasks.is_empty() && !self.selection_mode)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.selection_mode = !this.selection_mode;
                                        this.checked.clear();
                                        window.focus(&this.focus);
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        h_flex()
                            .h(px(36.))
                            .px(px(2.))
                            .rounded(px(7.))
                            .bg(inset_color(cx))
                            .when(!trash, |actions| {
                                actions
                                    .child(
                                        self.icon_button(
                                            "start-batch",
                                            "play",
                                            "native.startSelected",
                                        )
                                        .size(px(32.))
                                        .disabled(
                                            self.busy
                                                || !tasks.iter().any(|t| {
                                                    ids.contains(&t.id) && can_start(&t.state)
                                                }),
                                        )
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                this.act(
                                                    start.clone(),
                                                    TaskAction::Start,
                                                    window,
                                                    cx,
                                                )
                                            }),
                                        ),
                                    )
                                    .child(
                                        self.icon_button(
                                            "pause-batch",
                                            "pause",
                                            "list.pauseSelected",
                                        )
                                        .size(px(32.))
                                        .disabled(
                                            self.busy
                                                || !tasks.iter().any(|t| {
                                                    ids.contains(&t.id) && active(&t.state)
                                                }),
                                        )
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                this.act(
                                                    pause.clone(),
                                                    TaskAction::Pause,
                                                    window,
                                                    cx,
                                                )
                                            }),
                                        ),
                                    )
                                    .child(
                                        self.icon_button(
                                            "stop-batch",
                                            "square",
                                            "list.action.stop",
                                        )
                                        .size(px(32.))
                                        .disabled(
                                            self.busy
                                                || !tasks.iter().any(|t| {
                                                    ids.contains(&t.id) && can_stop(&t.state)
                                                }),
                                        )
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                this.act(stop.clone(), TaskAction::Stop, window, cx)
                                            }),
                                        ),
                                    )
                                    .child(
                                        div().w(px(1.)).h(px(16.)).mx(px(4.)).bg(cx.theme().border),
                                    )
                            })
                            .child(
                                self.icon_button(
                                    "delete-batch",
                                    "trash-2",
                                    if trash {
                                        "list.clearTrash"
                                    } else {
                                        "confirm.moveTrash"
                                    },
                                )
                                .size(px(32.))
                                .text_color(cx.theme().muted_foreground)
                                .disabled(self.busy || ids.is_empty())
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.act(
                                            delete.clone(),
                                            if trash {
                                                TaskAction::Delete(false)
                                            } else {
                                                TaskAction::Trash
                                            },
                                            window,
                                            cx,
                                        )
                                    },
                                )),
                            ),
                    ),
            )
            .child(if self.loading {
                empty(
                    "loader-circle",
                    self.tr("native.loading"),
                    self.tr("native.loadingCopy"),
                    cx,
                )
                .pb(empty_bottom_inset)
                .into_any_element()
            } else if tasks.is_empty() {
                empty(
                    if trash { "trash-2" } else { "download" },
                    self.tr(if trash {
                        "list.empty.trash.title"
                    } else if self.store.tasks.is_empty() {
                        "list.empty.downloads.title"
                    } else {
                        "list.empty.matches.title"
                    }),
                    self.tr(if trash {
                        "list.empty.trash.copy"
                    } else {
                        "list.empty.downloads.copy"
                    }),
                    cx,
                )
                .pb(empty_bottom_inset)
                .into_any_element()
            } else {
                let view = cx.entity();
                scroll_mask(
                    uniform_list("tasks", tasks.len(), move |range, _, cx| {
                        view.update(cx, |this, cx| {
                            range
                                .map(|index| this.row(&tasks[index], pane_width, cx))
                                .collect::<Vec<_>>()
                        })
                    })
                    .size_full()
                    .track_scroll(self.list_scroll.clone()),
                    &self.list_scroll.0.borrow().base_handle,
                    cx.theme().background,
                )
                .flex_1()
                .into_any_element()
            });
        pane.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{batch_targets, ellipsize_task_name};
    use fluxion_core::TaskState;
    use std::collections::BTreeSet;

    #[gpui::test]
    fn task_name_ellipsis_recomputes_for_narrower_and_wider_columns(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let name =
                "中文文件名-very-long-filename-with-size-and-protocol-aligned-on-one-line.bin";
            let wide = ellipsize_task_name(name, gpui::px(240.), cx);
            let narrow = ellipsize_task_name(name, gpui::px(100.), cx);
            for visible in [&wide, &narrow] {
                assert!(visible.ends_with('…'));
                assert!(name.starts_with(visible.trim_end_matches('…')));
            }
            assert!(narrow.chars().count() < wide.chars().count());
            assert_eq!(
                ellipsize_task_name(name, gpui::px(4096.), cx).as_ref(),
                name
            );
            assert_eq!(
                ellipsize_task_name("中文.zip", gpui::px(240.), cx).as_ref(),
                "中文.zip"
            );
        });
    }

    #[test]
    fn clearing_last_selection_never_targets_all_tasks() {
        let tasks = vec![
            crate::tests::task(TaskState::Queued),
            crate::tests::task(TaskState::Paused),
        ];
        let mut checked = BTreeSet::new();
        assert_eq!(batch_targets(&tasks, false, &checked).len(), 2);
        assert!(batch_targets(&tasks, true, &checked).is_empty());
        checked.insert(tasks[0].id);
        assert_eq!(batch_targets(&tasks, true, &checked), vec![tasks[0].id]);
        checked.clear();
        assert!(batch_targets(&tasks, true, &checked).is_empty());
        // A checked task outside the current search/filter is not a batch target.
        checked.insert(uuid::Uuid::new_v4());
        assert!(batch_targets(&tasks, true, &checked).is_empty());
    }
}
