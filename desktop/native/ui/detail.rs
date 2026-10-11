use super::*;
use crate::ui_style::{collapse_card, control, scroll_mask};

const DETAIL_SECTION_GAP: f32 = 16.;
const DETAIL_FIELD_GAP: f32 = 12.;
const DETAIL_VALUE_GAP: f32 = 6.;

impl Workspace {
    pub(super) fn detail_pane(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let surface = cx.theme().background;
        // Reserve the sidebar, outer inset, and (on wide windows) the task list.
        let pane_width = window.viewport_size().width
            - px(208. + 12.)
            - if window.viewport_size().width >= px(1020.) {
                px(384. + 12.)
            } else {
                px(0.)
            };
        // Source text has its own font size and reserves space for the copy target.
        let source_width = pane_width - px(48. + 2. + 12. + 4. + 8. + 28.);
        let Some(detail) = &self.detail else {
            return v_flex()
                .h_full()
                .flex_1()
                .rounded_xl()
                .overflow_hidden()
                .bg(surface)
                .child(empty(
                    "info",
                    self.tr("task.empty.title"),
                    self.tr("task.empty.copy"),
                    cx,
                ))
                .into_any_element();
        };
        let task = &detail.task;
        let id = task.id;
        let color = state_color(&task.state, cx);
        let rate = self.store.speeds.get(&id).copied().unwrap_or_default();
        let progress = percent(task.downloaded_bytes, task.total_bytes);
        let trash = self.preferences.trash.contains(&id);
        let name = task
            .file_name
            .clone()
            .unwrap_or_else(|| self.tr("new.resolving"));
        let protocol = match task.kind.download_kind() {
            DownloadKind::Http => "HTTP",
            DownloadKind::Ftp => "FTP",
            DownloadKind::Sftp => "SFTP",
            DownloadKind::Bt => "BT",
        };
        let size = bytes(task.total_bytes);
        let text_width = |text: &str, font_size, weight| {
            let mut run = window.text_style().to_run(text.len());
            run.font = font(".SystemUIFont");
            run.font.weight = weight;
            window
                .text_system()
                .shape_line(text.to_owned().into(), font_size, &[run], None)
                .width
                .ceil()
        };
        let size_width = text_width(&size, px(13.), FontWeight::NORMAL);
        let tag_width = text_width(protocol, px(11.), FontWeight::MEDIUM) + px(16.);
        // Reserve the body insets, icon, gaps, size and tag before truncating
        // the name, so metadata stays on the same line even in a narrow pane.
        let name_width =
            (pane_width - px(48. + 32. + 24. + 8.) - size_width - tag_width).max(px(0.));
        let mut name_font = font(".SystemUIFont");
        name_font.weight = FontWeight::SEMIBOLD;
        let visible_name = cx
            .text_system()
            .line_wrapper(name_font, px(18.))
            .truncate_line(name.clone().into(), name_width, "…", &mut Vec::new());
        let mut body = v_flex()
            .w_full()
            .flex_shrink_0()
            .gap(px(DETAIL_SECTION_GAP))
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .gap_3()
                    .child(
                        div()
                            .size(px(32.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                icon_view(file_icon(&name))
                                    .size(px(28.))
                                    .text_color(cx.theme().muted_foreground),
                            ),
                    )
                    .child(
                        h_flex()
                            .min_w_0()
                            .flex_1()
                            .gap_2()
                            .child(
                                div()
                                    .max_w(name_width)
                                    .min_w_0()
                                    .text_size(px(18.))
                                    .font_semibold()
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .line_height(relative(1.25))
                                    .child(visible_name),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(13.))
                                    .whitespace_nowrap()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(size),
                            ),
                    )
                    .child(
                        h_flex()
                            .w(tag_width)
                            .h(px(22.))
                            .flex_shrink_0()
                            .justify_center()
                            .rounded(px(5.))
                            .bg(inset_color(cx))
                            .text_size(px(11.))
                            .font_medium()
                            .text_color(cx.theme().muted_foreground)
                            .child(protocol),
                    ),
            )
            .child(
                v_flex()
                    .gap(px(8.))
                    .child(
                        h_flex()
                            .items_end()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(24.))
                                    .font_semibold()
                                    .child(format!("{:.1}%", progress * 100.)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_medium()
                                    .px_3()
                                    .py_1()
                                    .rounded_full()
                                    .bg(color.opacity(0.10))
                                    .text_color(color)
                                    .child(self.tr(&format!("state.{:?}", task.state))),
                            ),
                    )
                    .child(progress_bar(progress, color, cx))
                    .child(
                        h_flex()
                            .justify_between()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "{} / {}",
                                bytes(Some(task.downloaded_bytes)),
                                bytes(task.total_bytes)
                            ))
                            .child(self.tr("native.transferred")),
                    ),
            )
            .child(
                h_flex()
                    .gap_4()
                    .px_4()
                    .py_3()
                    .rounded_lg()
                    .bg(inset_color(cx))
                    .child(metric(self.tr("detail.downSpeed"), speed(rate.0), cx))
                    .child(metric(self.tr("detail.upSpeed"), speed(rate.1), cx))
                    .child(metric(
                        self.tr("detail.remaining"),
                        eta(task.downloaded_bytes, task.total_bytes, rate.0),
                        cx,
                    )),
            );
        let mut actions = h_flex().flex_shrink_0().gap_2();
        if trash {
            actions = actions.child(
                self.action_button("restore", "undo-2", "native.restore")
                    .primary()
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.act(vec![id], TaskAction::Restore, window, cx)
                    })),
            );
        } else {
            if can_start(&task.state) {
                actions = actions.child(
                    self.action_button(
                        "start",
                        "play",
                        if task.state == TaskState::Failed {
                            "list.action.retry"
                        } else {
                            "list.action.start"
                        },
                    )
                    .primary()
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.act(vec![id], TaskAction::Start, window, cx)
                    })),
                );
            }
            if active(&task.state) {
                actions = actions.child(
                    self.action_button("pause", "pause", "list.action.pause")
                        .primary()
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.act(vec![id], TaskAction::Pause, window, cx)
                        })),
                );
            }
            if task.state == TaskState::Completed {
                actions = actions
                    .child(
                        self.action_button("open", "external-link", "detail.openFile")
                            .primary()
                            .disabled(self.busy)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.act(vec![id], TaskAction::Open, window, cx)
                            })),
                    )
                    .child(
                        self.icon_button("reveal", "folder-open", "detail.revealFinder")
                            .disabled(self.busy)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.act(vec![id], TaskAction::Reveal, window, cx)
                            })),
                    );
            }
            if can_stop(&task.state) {
                actions = actions.child(
                    self.action_button("stop", "square", "list.action.stop")
                        .ghost()
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.act(vec![id], TaskAction::Stop, window, cx)
                        })),
                );
            }
        }
        actions = actions.child(
            self.icon_button(
                "trash",
                "trash-2",
                if trash {
                    "list.action.deletePermanent"
                } else {
                    "confirm.moveTrash"
                },
            )
            .text_color(cx.theme().muted_foreground)
            .disabled(self.busy)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.act(
                    vec![id],
                    if trash {
                        TaskAction::Delete(false)
                    } else {
                        TaskAction::Trash
                    },
                    window,
                    cx,
                )
            })),
        );
        body = v_flex()
            .w_full()
            .flex_shrink_0()
            .px(px(24.))
            .pt(px(12.))
            .pb(px(24.))
            .gap(px(DETAIL_SECTION_GAP))
            .child(body);
        if let Some(error) = &task.error {
            body = body.child(
                v_flex()
                    .gap_2()
                    .p_4()
                    .rounded_lg()
                    .bg(cx.theme().danger.opacity(0.08))
                    .text_color(cx.theme().danger)
                    .child(self.tr("detail.failed"))
                    .child(div().text_sm().child(error.clone()))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(self.tr("detail.failedHint")),
                    ),
            );
        }
        let source = match &task.kind {
            TaskKind::Http(c) => c.url.to_string(),
            TaskKind::Ftp(c) => c.url.to_string(),
            TaskKind::Sftp(c) => c.url.to_string(),
            TaskKind::Bt(c) => match &c.source {
                BtSource::Magnet(s) => s.clone(),
                BtSource::TorrentFile(p) => p.to_string_lossy().into_owned(),
            },
        };
        // GPUI's flex text measurement can cache an unconstrained width. Shape
        // the visible source against the pane width while retaining the full copy.
        let visible_source = cx
            .text_system()
            .line_wrapper(font(".SystemUIFont"), px(13.))
            .truncate_line(source.clone().into(), source_width, "…", &mut Vec::new());
        let visible_directory = cx
            .text_system()
            .line_wrapper(font(".SystemUIFont"), px(14.))
            .truncate_line(
                task.save_dir.to_string_lossy().into_owned().into(),
                (pane_width - px(48. + 12. + 28.)).max(px(0.)),
                "…",
                &mut Vec::new(),
            );
        body = body.child(
            v_flex()
                .gap(px(DETAIL_FIELD_GAP))
                .child(section(self.tr("native.fileDetails"), cx))
                .child(
                    v_flex()
                        .gap(px(DETAIL_VALUE_GAP))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(self.tr("new.saveTo")),
                        )
                        .child(
                            h_flex()
                                .w_full()
                                .min_w_0()
                                .min_h(px(28.))
                                .gap_3()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_size(px(14.))
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .child(visible_directory),
                                )
                                .child(
                                    control("open-save-directory")
                                        .ghost()
                                        .size(px(28.))
                                        .p_0()
                                        .flex_shrink_0()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(icon_view("folder-open").size(px(16.)))
                                        .tooltip(self.tr("native.openFolder"))
                                        .disabled(self.busy)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.act(vec![id], TaskAction::OpenFolder, window, cx)
                                        })),
                                ),
                        ),
                )
                .child(
                    v_flex()
                        .gap(px(DETAIL_VALUE_GAP))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(self.tr("detail.source")),
                        )
                        .child(
                            h_flex()
                                .w_full()
                                .min_w_0()
                                .h(px(36.))
                                .pl_3()
                                .pr_1()
                                .gap_2()
                                .rounded(px(6.))
                                .border_1()
                                .border_color(cx.theme().border.opacity(0.6))
                                .bg(inset_color(cx))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_size(px(13.))
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .child(visible_source),
                                )
                                .child(
                                    crate::ui_style::control("copy-source")
                                        .ghost()
                                        .size(px(28.))
                                        .p_0()
                                        .flex_shrink_0()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(icon_view("copy").size(px(16.)))
                                        .tooltip(self.tr("native.copySource"))
                                        .on_click({
                                            move |_, _, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    source.clone(),
                                                ))
                                            }
                                        }),
                                ),
                        ),
                )
                .child(
                    h_flex()
                        .gap_3()
                        .flex_wrap()
                        .child(
                            div()
                                .flex_1()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(self.tr("detail.created")),
                        )
                        .child(
                            div().text_size(px(13.)).child(
                                task.created_at
                                    .with_timezone(&chrono::Local)
                                    .format(&self.tr("detail.dateTimeFormat"))
                                    .to_string(),
                            ),
                        ),
                ),
        );
        let mut limits = v_flex().gap(px(DETAIL_FIELD_GAP)).child(
            h_flex()
                .gap_4()
                .child(metric(
                    self.tr("detail.downloadLimit"),
                    task.limits
                        .download_bytes_per_second
                        .map(speed)
                        .unwrap_or_else(|| self.tr("common.unlimited")),
                    cx,
                ))
                .child(metric(
                    self.tr("detail.uploadLimit"),
                    task.limits
                        .upload_bytes_per_second
                        .map(speed)
                        .unwrap_or_else(|| self.tr("common.unlimited")),
                    cx,
                )),
        );
        if let TaskKind::Http(config) = &task.kind {
            limits = limits.child(
                info(
                    self.tr("new.maxConnections"),
                    config.max_connections.unwrap_or(16).to_string(),
                    cx,
                )
                .gap(px(DETAIL_VALUE_GAP)),
            );
        }
        let header = control("detail-limits-toggle")
            .ghost()
            .h(px(40.))
            .justify_between()
            .child(
                div()
                    .flex_1()
                    .text_left()
                    .font_medium()
                    .child(self.tr("detail.rateLimits")),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.detail_limits.toggle();
                cx.notify();
            }));
        let edit = self
            .action_button("edit-limits", "pencil", "native.edit")
            .ghost()
            .h(px(28.))
            .disabled(self.busy)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_form(FormKind::Limits(id), window, cx)
            }));
        body = body.child(collapse_card(
            &mut self.detail_limits,
            header,
            Some(edit),
            limits,
            window,
            cx,
        ));
        if let TaskKind::Http(config) = &task.kind {
            if !config.headers.is_empty() || !detail.credentials.headers.is_empty() {
                let mut headers = v_flex()
                    .gap(px(DETAIL_FIELD_GAP))
                    .child(section(self.tr("credentials.requestHeaders"), cx));
                for header in config
                    .headers
                    .iter()
                    .chain(detail.credentials.headers.iter())
                {
                    headers = headers.child(info(header.name.clone(), header.value.clone(), cx));
                }
                body = body.child(headers);
            }
        }
        if let Some(username) = &detail.credentials.username {
            body = body.child(info(self.tr("credentials.username"), username.clone(), cx));
        }
        if let TaskKind::Bt(config) = &task.kind {
            if let Some(bt) = &self.bt {
                let have = (0..bt.total_pieces)
                    .filter(|i| {
                        bt.piece_haves
                            .get((*i / 8) as usize)
                            .is_some_and(|byte| byte & (1 << (7 - i % 8)) != 0)
                    })
                    .count();
                let mut pieces = h_flex().flex_wrap().gap(px(3.));
                // Each cell aggregates an even range, so very large torrents remain cheap to render.
                let group = bt.total_pieces.max(1).div_ceil(240);
                for start in (0..bt.total_pieces).step_by(group as usize) {
                    let end = (start + group).min(bt.total_pieces);
                    let complete = (start..end).all(|i| {
                        bt.piece_haves
                            .get((i / 8) as usize)
                            .is_some_and(|byte| byte & (1 << (7 - i % 8)) != 0)
                    });
                    pieces = pieces.child(div().size(px(7.)).rounded(px(2.)).bg(if complete {
                        color
                    } else {
                        cx.theme().border
                    }));
                }
                let mut files = v_flex().gap_3();
                for file in &bt.files {
                    files = files.child(
                        v_flex()
                            .gap_1()
                            .child(div().text_sm().child(file.name.clone()))
                            .child(progress_bar(
                                percent(file.downloaded, Some(file.size)),
                                color,
                                cx,
                            ))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!(
                                        "{} / {}",
                                        bytes(Some(file.downloaded)),
                                        bytes(Some(file.size))
                                    )),
                            ),
                    );
                }
                body = body
                    .child(section(
                        format!("{} · {have}/{}", self.tr("bt.pieces"), bt.total_pieces),
                        cx,
                    ))
                    .child(pieces)
                    .child(info(
                        self.tr("fields.shareRatio"),
                        format!("{:.2}", bt.seed_ratio.unwrap_or(0.)),
                        cx,
                    ))
                    .child(files);
            } else {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(self.tr("bt.unavailable")),
                );
            }
            body = body.child(info(
                self.tr("fields.seedAfter"),
                self.tr(if config.enable_seeding {
                    "native.on"
                } else {
                    "native.off"
                }),
                cx,
            ));
        }
        v_flex()
            .h_full()
            .min_w_0()
            .flex_1()
            .rounded_xl()
            .overflow_hidden()
            .bg(surface)
            .child(
                h_flex()
                    .h(px(60.))
                    .flex_shrink_0()
                    .pl_6()
                    // The 20 px glyph sits inside a 36 px hit target. Offset
                    // its 8 px inset so it aligns with the body's right edge.
                    .pr_4()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(self.tr("native.inspector")),
                    )
                    .child(actions)
                    .child(
                        div()
                            .w(px(1.))
                            .h(px(16.))
                            .flex_shrink_0()
                            .bg(cx.theme().border.opacity(0.7)),
                    )
                    .child(
                        self.icon_button("close-detail", "panel-right-close", "common.close")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.preferences.detail_open = false;
                                this.persist(cx);
                            })),
                    ),
            )
            .child(
                scroll_mask(
                    div()
                        .id("detail-scroll")
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&self.detail_scroll)
                        .child(body),
                    &self.detail_scroll,
                    surface,
                )
                .flex_1(),
            )
            .into_any_element()
    }
}
