use super::*;
use gpui_component::switch::Switch;

fn preference_row(label: String, control: impl IntoElement) -> Div {
    h_flex()
        .min_h(px(36.))
        .gap_3()
        .flex_wrap()
        .child(div().w(px(120.)).flex_shrink_0().text_sm().child(label))
        .child(control)
}

impl Workspace {
    pub(super) fn settings_pane(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut appearance = h_flex().gap_1();
        for (id, name) in [
            ("system", "native.system"),
            ("light", "native.light"),
            ("dark", "native.dark"),
        ] {
            appearance = appearance.child(
                Button::new(id)
                    .ghost()
                    .small()
                    .label(self.tr(name))
                    .when(self.preferences.appearance == id, |b| {
                        b.bg(selected_color(cx))
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.preferences.appearance = id.into();
                        crate::apply_theme(id, Some(window), cx);
                        this.persist(cx);
                    })),
            );
        }
        let mut languages = h_flex().flex_wrap().gap_1();
        for (id, label) in [
            ("en", "English"),
            ("zh-CN", "简体中文"),
            ("ja", "日本語"),
            ("ko", "한국어"),
        ] {
            languages = languages.child(
                Button::new(id)
                    .ghost()
                    .small()
                    .label(label)
                    .when(self.preferences.locale == id, |b| b.bg(selected_color(cx)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.preferences.locale = id.into();
                        if let Some(form) = &this.settings_form {
                            form.update(cx, |form, cx| form.set_locale(id, window, cx));
                        }
                        this.search.update(cx, |input, cx| {
                            input.set_placeholder(t(id, "list.search"), window, cx)
                        });
                        this.persist(cx);
                    })),
            );
        }
        let mut channels = h_flex().gap_1();
        for (channel, label) in [(Channel::Stable, "Stable"), (Channel::Beta, "Beta")] {
            channels = channels.child(
                Button::new(channel.as_str())
                    .ghost()
                    .small()
                    .label(label)
                    .when(self.preferences.update_channel == channel, |b| {
                        b.bg(selected_color(cx))
                    })
                    .disabled(self.update_status.installing())
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.change_update_channel(channel, cx)),
                    ),
            );
        }
        let status = match &self.update_status {
            UpdateStatus::Idle => String::new(),
            UpdateStatus::Checking => self.tr("settings.update.checking"),
            UpdateStatus::Current => self.tr("settings.update.current"),
            UpdateStatus::Available => self.tr("settings.update.available").replace(
                "{version}",
                self.update
                    .as_ref()
                    .map(|r| r.version.as_str())
                    .unwrap_or(""),
            ),
            UpdateStatus::Downloading(done, total) => format!(
                "{} · {} / {}",
                self.tr("update.downloading"),
                bytes(Some(*done)),
                bytes(*total)
            ),
            UpdateStatus::Verifying => self.tr("native.updateVerifying"),
            UpdateStatus::Ready => self.tr("native.updateReady"),
            UpdateStatus::Installing => self.tr("native.updateInstalling"),
            UpdateStatus::Error(error) => error.clone(),
        };
        let updates = v_flex()
            .gap_3()
            .child(
                h_flex()
                    .justify_between()
                    .child(section(self.tr("settings.update.title"), cx))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("Fluxion {}", crate::updater::VERSION)),
                    ),
            )
            .child(preference_row(self.tr("native.updateChannel"), channels))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        self.tr(if self.preferences.update_channel == Channel::Stable {
                            "native.stableHint"
                        } else {
                            "native.betaHint"
                        }),
                    ),
            )
            .child(preference_row(
                self.tr("native.autoUpdates"),
                Switch::new("automatic-update-checks")
                    .checked(self.preferences.auto_check_updates)
                    .on_click(cx.listener(|this, value, _, cx| {
                        this.preferences.auto_check_updates = *value;
                        this.persist(cx);
                        if *value {
                            this.check_for_updates(false, cx);
                        }
                    })),
            ))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.tr("native.autoUpdatesHint")),
            )
            .child(
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        Button::new("check-update")
                            .small()
                            .label(self.tr("settings.update.check"))
                            .disabled(self.update_status.busy() || self.update_prepared.is_some())
                            .on_click(
                                cx.listener(|this, _, _, cx| this.check_for_updates(true, cx)),
                            ),
                    )
                    .when(
                        self.update.is_some() && self.update_prepared.is_none(),
                        |row| {
                            row.child(
                                Button::new("download-update")
                                    .small()
                                    .primary()
                                    .label(self.tr("native.downloadUpdate"))
                                    .disabled(self.update_status.busy())
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(release) = this.update.clone() {
                                            this.update_status = UpdateStatus::Downloading(0, None);
                                            this.send(
                                                Command::DownloadUpdate(
                                                    release,
                                                    this.preferences.update_channel,
                                                ),
                                                cx,
                                            );
                                        }
                                    })),
                            )
                        },
                    )
                    .when(self.update_prepared.is_some(), |row| {
                        row.child(
                            Button::new("install-update")
                                .small()
                                .primary()
                                .label(self.tr("update.install"))
                                .disabled(self.update_status.busy())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(prepared) = this.update_prepared.take() {
                                        this.update_status = UpdateStatus::Installing;
                                        this.send(Command::InstallUpdate(prepared), cx);
                                    }
                                })),
                        )
                    }),
            )
            .when(!status.is_empty(), |view| {
                view.child(
                    div()
                        .text_sm()
                        .text_color(if matches!(self.update_status, UpdateStatus::Error(_)) {
                            cx.theme().danger
                        } else {
                            cx.theme().muted_foreground
                        })
                        .child(status),
                )
            })
            .when_some(self.update_checked_at, |view, time| {
                view.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(
                            self.tr("native.lastChecked")
                                .replace("{time}", &time.format("%H:%M").to_string()),
                        ),
                )
            })
            .when_some(
                self.update.as_ref().filter(|r| !r.notes.trim().is_empty()),
                |view, release| {
                    view.child(
                        div()
                            .text_sm()
                            .max_h(px(120.))
                            .overflow_hidden()
                            .child(release.notes.clone()),
                    )
                },
            );
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .rounded_xl()
            .overflow_hidden()
            .bg(cx.theme().background)
            .child(
                h_flex()
                    .h(px(60.))
                    .flex_shrink_0()
                    .px_7()
                    .text_lg()
                    .font_semibold()
                    .child(self.tr("settings.title")),
            )
            .child(
                div()
                    .id("settings-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_7()
                    .pb_7()
                    .child(
                        v_flex()
                            .w_full()
                            .max_w(px(720.))
                            .gap_6()
                            .child(
                                v_flex()
                                    .gap_2()
                                    .child(section(self.tr("settings.appearance.title"), cx))
                                    .child(preference_row(self.tr("native.theme"), appearance))
                                    .child(preference_row(self.tr("native.language"), languages)),
                            )
                            .child(updates)
                            .children(self.settings_form.clone()),
                    ),
            )
            .into_any_element()
    }
}
