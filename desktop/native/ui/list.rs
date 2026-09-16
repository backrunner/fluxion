use super::*;
use gpui_component::menu::{DropdownMenu, PopupMenuItem};

const LIST_CONTENT_INSET: f32 = 24.;

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
        Button::new(id).h(px(36.)).px_3().child(
            h_flex()
                .gap_2()
                .text_size(px(14.))
                .child(icon_view(icon))
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
        h_flex()
            .id(id)
            .h(px(44.))
            .px_3()
            .gap_3()
            .rounded_xl()
            .cursor_pointer()
            .when(selected, |v| v.bg(selected_color(cx)))
            .hover(|v| v.bg(inset_color(cx)))
            .child(icon_view(icon).text_color(if selected { cx.theme().primary } else { color }))
            .child(
                div()
                    .flex_1()
                    .text_size(px(14.))
                    .font_medium()
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
                    .h(px(64.))
                    .window_control_area(WindowControlArea::Drag),
            )
            .child(
                v_flex()
                    .px_3()
                    .gap_2()
                    .child(
                        div()
                            .px_3()
                            .pb_2()
                            .text_xs()
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
                .gap_3()
                .child(
                    div()
                        .size(px(32.))
                        .rounded_lg()
                        .bg(tint.opacity(0.10))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icon_view(icon).text_color(tint)),
                )
                .child(
                    div()
                        .flex_1()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(label),
                )
                .child(div().text_size(px(14.)).font_medium().child(speed(value)))
        };
        v_flex()
            .mx_3()
            .mb_3()
            .p_3()
            .gap_3()
            .rounded_2xl()
            .bg(cx.theme().background)
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
    pub(super) fn row(&self, task: &TaskSummary, cx: &mut Context<Self>) -> AnyElement {
        let id = task.id;
        let selected = self.selected == Some(id);
        let color = state_color(&task.state, cx);
        let rate = self.store.speeds.get(&id).copied().unwrap_or_default();
        let progress = percent(task.downloaded_bytes, task.total_bytes);
        let name = task
            .file_name
            .clone()
            .unwrap_or_else(|| self.tr("new.resolving"));
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
                    .h(px(24.))
                    .gap_2()
                    .child(icon_view(file_icon(&name)).text_color(if selected {
                        cx.theme().primary
                    } else {
                        cx.theme().muted_foreground
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(14.))
                            .font_medium()
                            .truncate()
                            .child(name),
                    )
                    .child(
                        h_flex()
                            .size(px(24.))
                            .flex_shrink_0()
                            .justify_center()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(
                                Checkbox::new(("check", id.as_u128() as usize))
                                    .checked(self.checked.contains(&id))
                                    .on_click(cx.listener(move |this, checked, _, cx| {
                                        if *checked {
                                            this.checked.insert(id);
                                        } else {
                                            this.checked.remove(&id);
                                        }
                                        cx.notify();
                                    })),
                            ),
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
                                    speed(rate.0)
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
                    .px_3()
                    .py(px(10.))
                    .gap_3()
                    .rounded_lg()
                    .bg(if selected {
                        selected_color(cx)
                    } else {
                        transparent_black()
                    })
                    .hover(|v| {
                        v.bg(if selected {
                            selected_color(cx)
                        } else {
                            inset_color(cx)
                        })
                    })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        if event.modifiers().platform && !this.checked.insert(id) {
                            this.checked.remove(&id);
                        }
                        this.select(id, window, cx);
                    }))
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
                                    .disabled(self.busy || self.filter == Filter::Trash)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.act(vec![id], primary, window, cx)
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }
    pub(super) fn task_list(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let tasks = self.visible(cx);
        let count = tasks.len();
        let trash = self.filter == Filter::Trash;
        let ids = if self.checked.is_empty() {
            tasks.iter().map(|t| t.id).collect::<Vec<_>>()
        } else {
            tasks
                .iter()
                .filter(|t| self.checked.contains(&t.id))
                .map(|t| t.id)
                .collect::<Vec<_>>()
        };
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
            .rounded_2xl()
            .overflow_hidden()
            .bg(cx.theme().background);
        if self.preferences.detail_open && window.viewport_size().width >= px(1020.) {
            pane = pane.w(px(384.)).flex_shrink_0();
        } else {
            pane = pane.flex_1();
        }
        pane = pane
            .child(
                h_flex()
                    .h(px(60.))
                    .px(px(LIST_CONTENT_INSET))
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .text_lg()
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
                    .child(
                        self.icon_button("refresh", "rotate-ccw", "common.refresh")
                            .disabled(self.busy || self.loading)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.loading = true;
                                this.send(Command::Refresh, cx);
                            })),
                    )
                    .child(
                        self.icon_button("detail", "panel-right", "list.toggleDetail")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.preferences.detail_open = !this.preferences.detail_open;
                                this.persist(cx);
                            })),
                    ),
            )
            .child(
                h_flex()
                    .px_4()
                    .pb_2()
                    .gap_2()
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&self.search)
                                .prefix(icon_view("search"))
                                .h(px(36.))
                                .appearance(false)
                                .rounded_lg()
                                .bg(inset_color(cx))
                                .cleanable(true),
                        ),
                    )
                    .child(
                        Button::new("sort")
                            .ghost()
                            .h(px(36.))
                            .px_2()
                            .flex_shrink_0()
                            .tooltip(self.tr("list.sortTasks"))
                            .child(
                                h_flex()
                                    .gap_1()
                                    .text_size(px(13.))
                                    .child(sort_options[selected_sort as usize].1.clone())
                                    .child(icon_view("chevrons-up-down")),
                            )
                            .dropdown_menu_with_anchor(Corner::TopRight, move |mut menu, _, _| {
                                for (value, label) in &sort_options {
                                    let value = *value;
                                    let view = sort_view.clone();
                                    menu = menu.item(
                                        PopupMenuItem::new(label.clone())
                                            .checked(value == selected_sort)
                                            .on_click(move |_, _, cx| {
                                                let _ = view.update(cx, |this, cx| {
                                                    this.sort = value;
                                                    this.list_scroll
                                                        .scroll_to_item(0, ScrollStrategy::Top);
                                                    cx.notify();
                                                });
                                            }),
                                    );
                                }
                                menu
                            }),
                    ),
            );
        if !trash {
            let mut tabs = h_flex()
                .mx_4()
                .p_1()
                .gap_1()
                .rounded_lg()
                .bg(inset_color(cx));
            for (index, filter, key) in [
                (0, Filter::All, "list.filter.all"),
                (1, Filter::Active, "list.filter.active"),
                (2, Filter::Completed, "list.filter.completed"),
                (3, Filter::Failed, "list.filter.failed"),
                (4, Filter::Paused, "state.Paused"),
            ] {
                tabs = tabs.child(
                    Button::new(("filter", index as usize))
                        .ghost()
                        .small()
                        .compact()
                        .h(px(28.))
                        .flex_1()
                        .when(self.filter == filter, |b| {
                            b.bg(cx.theme().background)
                                .text_color(cx.theme().foreground)
                        })
                        .label(self.tr(key))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.select_filter(filter, cx)),
                        ),
                );
            }
            pane = pane.child(tabs);
        }
        pane = pane
            .child(
                h_flex()
                    .h(px(44.))
                    .px(px(LIST_CONTENT_INSET))
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(if self.checked.is_empty() {
                                self.tr("native.taskCount")
                                    .replace("{count}", &count.to_string())
                            } else {
                                self.tr("list.selected")
                                    .replace("{count}", &self.checked.len().to_string())
                            }),
                    )
                    .when(!trash, |r| {
                        r.child(
                            h_flex()
                                .h(px(36.))
                                .px(px(2.))
                                .rounded_lg()
                                .bg(inset_color(cx))
                                .child(
                                    self.icon_button("start-batch", "play", "native.startSelected")
                                        .size(px(32.))
                                        .disabled(
                                            self.busy
                                                || !tasks.iter().any(|t| {
                                                    ids.contains(&t.id) && can_start(&t.state)
                                                }),
                                        )
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.act(start.clone(), TaskAction::Start, window, cx)
                                        })),
                                )
                                .child(
                                    self.icon_button("pause-batch", "pause", "list.pauseSelected")
                                        .size(px(32.))
                                        .disabled(
                                            self.busy
                                                || !tasks.iter().any(|t| {
                                                    ids.contains(&t.id) && active(&t.state)
                                                }),
                                        )
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.act(pause.clone(), TaskAction::Pause, window, cx)
                                        })),
                                )
                                .child(
                                    self.icon_button("stop-batch", "square", "list.action.stop")
                                        .size(px(32.))
                                        .disabled(
                                            self.busy
                                                || !tasks.iter().any(|t| {
                                                    ids.contains(&t.id) && can_stop(&t.state)
                                                }),
                                        )
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.act(stop.clone(), TaskAction::Stop, window, cx)
                                        })),
                                ),
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
            )
            .child(if self.loading {
                empty(
                    "loader-circle",
                    self.tr("native.loading"),
                    self.tr("native.loadingCopy"),
                    cx,
                )
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
            } else {
                let view = cx.entity();
                uniform_list("tasks", tasks.len(), move |range, _, cx| {
                    view.update(cx, |this, cx| {
                        range
                            .map(|index| this.row(&tasks[index], cx))
                            .collect::<Vec<_>>()
                    })
                })
                .flex_1()
                .track_scroll(self.list_scroll.clone())
                .into_any_element()
            });
        pane.into_any_element()
    }
}
