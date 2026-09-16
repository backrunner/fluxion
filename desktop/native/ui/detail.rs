use super::*;

impl Workspace {
    pub(super) fn detail_pane(&self, cx: &mut Context<Self>) -> AnyElement {
        let surface = cx.theme().background;
        let Some(detail) = &self.detail else {
            return v_flex()
                .h_full()
                .flex_1()
                .rounded_2xl()
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
        let mut body = v_flex()
            .px_7()
            .pt_3()
            .pb_7()
            .gap_7()
            .child(
                h_flex()
                    .gap_4()
                    .child(
                        div()
                            .size(px(56.))
                            .flex_shrink_0()
                            .rounded_xl()
                            .bg(cx.theme().primary.opacity(0.10))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                icon_view(file_icon(&name))
                                    .size(px(28.))
                                    .text_color(cx.theme().primary),
                            ),
                    )
                    .child(
                        v_flex()
                            .min_w_0()
                            .gap_2()
                            .child(
                                div()
                                    .text_xl()
                                    .font_semibold()
                                    .line_height(relative(1.25))
                                    .child(name),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(match task.kind.download_kind() {
                                        DownloadKind::Http => "HTTP",
                                        DownloadKind::Ftp => "FTP",
                                        DownloadKind::Sftp => "SFTP",
                                        DownloadKind::Bt => "BitTorrent",
                                    })
                                    .child("·")
                                    .child(bytes(task.total_bytes)),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .gap_3()
                    .child(
                        h_flex()
                            .items_end()
                            .justify_between()
                            .child(
                                div()
                                    .text_3xl()
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
                    .p_4()
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
        body = body.child(
            v_flex()
                .gap_4()
                .child(section(self.tr("native.fileDetails"), cx))
                .child(info(
                    self.tr("new.saveTo"),
                    task.save_dir.to_string_lossy().into_owned(),
                    cx,
                ))
                .child(info(self.tr("new.link"), source.clone(), cx))
                .child(
                    self.action_button("copy-source", "copy", "native.copySource")
                        .ghost()
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(source.clone()))
                        }),
                )
                .child(info(
                    self.tr("detail.created"),
                    task.created_at
                        .with_timezone(&chrono::Local)
                        .format("%b %d, %Y · %H:%M")
                        .to_string(),
                    cx,
                )),
        );
        body = body.child(
            v_flex()
                .gap_4()
                .pt_2()
                .child(
                    h_flex()
                        .justify_between()
                        .child(section(self.tr("detail.rateLimits"), cx))
                        .child(
                            Button::new("edit-limits")
                                .ghost()
                                .small()
                                .label(self.tr("native.edit"))
                                .disabled(self.busy)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_form(FormKind::Limits(id), window, cx)
                                })),
                        ),
                )
                .child(
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
                ),
        );
        if let TaskKind::Http(config) = &task.kind {
            body = body.child(info(
                self.tr("new.maxConnections"),
                config.max_connections.unwrap_or(16).to_string(),
                cx,
            ));
            if !config.headers.is_empty() || !detail.credentials.headers.is_empty() {
                let mut headers = v_flex()
                    .gap_3()
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
            .rounded_2xl()
            .overflow_hidden()
            .bg(surface)
            .child(
                h_flex()
                    .h(px(72.))
                    .flex_shrink_0()
                    .px_7()
                    .gap_3()
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
                        self.icon_button("close-detail", "panel-right-close", "common.close")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.preferences.detail_open = false;
                                this.persist(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .id("detail-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(body),
            )
            .into_any_element()
    }
}
