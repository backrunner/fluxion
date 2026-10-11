use super::*;
use crate::ui_style::{
    control, preference_row, scroll_mask, segment, segments, separator, settings_group,
};
use gpui_component::switch::Switch;

fn settings_link(id: &'static str, label: String, cx: &App) -> Button {
    control(id).ghost().p_0().h(px(28.)).border_0().child(
        h_flex()
            .gap_1p5()
            .text_size(px(13.))
            .text_color(cx.theme().primary)
            .child(label)
            .child(icon_view("external-link").size(px(13.))),
    )
}

impl Workspace {
    pub(super) fn settings_pane(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        // Resolve the form width before laying out percentage-sized groups.
        // GPUI otherwise measures nested percentage widths from their content.
        // Sidebar (208), outer gutter (12), and equal content insets (28 each).
        let content_width = window.viewport_size().width - px(208. + 12. + 28. * 2.);
        let mut appearance = segments(cx);
        for (id, name) in [
            ("system", "native.system"),
            ("light", "native.light"),
            ("dark", "native.dark"),
        ] {
            appearance = appearance.child(
                segment(id, self.tr(name), self.preferences.appearance == id, cx).on_click(
                    cx.listener(move |this, _, window, cx| {
                        this.preferences.appearance = id.into();
                        crate::apply_theme(id, Some(window), cx);
                        this.persist(cx);
                    }),
                ),
            );
        }
        let mut languages = segments(cx).flex_wrap();
        for (id, label) in [
            ("en", "English"),
            ("zh-CN", "简体中文"),
            ("ja", "日本語"),
            ("ko", "한국어"),
        ] {
            languages = languages.child(
                segment(id, label.into(), self.preferences.locale == id, cx).on_click(cx.listener(
                    move |this, _, window, cx| {
                        this.preferences.locale = id.into();
                        if let Some(form) = &this.settings_form {
                            form.update(cx, |form, cx| form.set_locale(id, window, cx));
                        }
                        this.search.update(cx, |input, cx| {
                            input.set_placeholder(t(id, "list.search"), window, cx)
                        });
                        this.persist(cx);
                    },
                )),
            );
        }
        let mut channels = segments(cx);
        for (channel, label) in [(Channel::Stable, "Stable"), (Channel::Beta, "Beta")] {
            channels = channels.child(
                segment(
                    channel.as_str(),
                    label.into(),
                    self.preferences.update_channel == channel,
                    cx,
                )
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
            .w_full()
            .gap_3()
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(13.))
                            .child(self.tr("native.updateChannel")),
                    )
                    .child(channels),
            )
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
            .child(separator(cx))
            .child(preference_row(
                self.tr("native.autoUpdates"),
                Switch::new("startup-update-checks")
                    .small()
                    .tooltip(self.tr("native.autoUpdates"))
                    .checked(self.preferences.auto_check_updates)
                    .on_click(cx.listener(|this, value, _, cx| {
                        this.preferences.auto_check_updates = *value;
                        this.persist(cx);
                    })),
            ))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.tr("native.autoUpdatesHint")),
            )
            .child(separator(cx))
            .child(
                h_flex()
                    .w_full()
                    .gap_3()
                    .flex_wrap()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w(px(160.))
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .child(format!("Fluxion {}", crate::updater::VERSION)),
                            )
                            .when(!status.is_empty(), |view| {
                                view.child(
                                    div()
                                        .text_xs()
                                        .text_color(
                                            if matches!(self.update_status, UpdateStatus::Error(_))
                                            {
                                                cx.theme().danger
                                            } else {
                                                cx.theme().muted_foreground
                                            },
                                        )
                                        .child(status),
                                )
                            })
                            .when_some(self.update_checked_at, |view, time| {
                                view.child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(
                                            self.tr("native.lastChecked").replace(
                                                "{time}",
                                                &time.format("%H:%M").to_string(),
                                            ),
                                        ),
                                )
                            }),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .flex_wrap()
                            .child(
                                control("check-update")
                                    .label(self.tr("settings.update.check"))
                                    .disabled(
                                        self.update_status.busy() || self.update_prepared.is_some(),
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.check_for_updates(true, cx)
                                    })),
                            )
                            .when(
                                self.update.is_some() && self.update_prepared.is_none(),
                                |row| {
                                    row.child(
                                        control("download-update")
                                            .primary()
                                            .label(self.tr("native.downloadUpdate"))
                                            .disabled(self.update_status.busy())
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                if let Some(release) = this.update.clone() {
                                                    this.update_status =
                                                        UpdateStatus::Downloading(0, None);
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
                                    control("install-update")
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
                    ),
            )
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
                    .border_b_1()
                    .border_color(cx.theme().border.opacity(0.7))
                    .text_size(px(17.))
                    .font_semibold()
                    .justify_between()
                    .child(self.tr("settings.title"))
                    .child(
                        control("save-transfer-settings")
                            .primary()
                            .label(self.tr(if self.busy { "common.saving" } else { "native.saveTransferSettings" }))
                            .disabled(self.busy || self.settings_form.is_none())
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(form) = &this.settings_form {
                                    form.update(cx, |form, cx| form.submit(cx));
                                }
                            })),
                    ),
            )
            .child(
                scroll_mask(
                div()
                    .id("settings-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.settings_scroll)
                    .px_7()
                    .pt_5()
                    .pb_7()
                    .child(
                        v_flex()
                            .w(content_width)
                            .flex_shrink_0()
                            .gap_6()
                            .child(settings_group(
                                self.tr("settings.appearance.title"),
                                v_flex().w_full().gap_2()
                                    .child(preference_row(self.tr("native.theme"), appearance))
                                    .child(separator(cx))
                                    .child(preference_row(self.tr("native.language"), languages)),
                                cx,
                            ))
                            .when(!cfg!(feature = "app-store"), |view| view.child(settings_group(
                                self.tr("settings.update.title"),
                                updates,
                                cx,
                            )))
                            .when(cfg!(feature = "app-store"), |view| {
                                view.child(
                                    v_flex()
                                        .gap_2()
                                        .child(section(
                                            format!("Fluxion {}", crate::updater::VERSION),
                                            cx,
                                        ))
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(self.tr("native.storeUpdates")),
                                        ),
                                )
                            })
                            .child(
                                v_flex()
                                    .w_full()
                                    .gap_2()
                                    .child(section(self.tr("native.privacyTitle"), cx))
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .w_full()
                                            .child(self.tr("native.privacySummary")),
                                    )
                                    .child(
                                        h_flex()
                                            .gap_x_5()
                                            .gap_y_1()
                                            .flex_wrap()
                                            .child(
                                                settings_link("privacy-policy", self.tr("native.privacyPolicy"), cx)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.send(Command::Document(true), cx)
                                                    })),
                                            )
                                            .child(
                                                settings_link("third-party-notices", self.tr("native.thirdPartyNotices"), cx)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.send(Command::Document(false), cx)
                                                    })),
                                            )
                                            .child(
                                                settings_link("support", self.tr("native.support"), cx)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        cx.open_url(if this.preferences.locale == "zh-CN" {
                                                            "https://fluxion.alkinum.com/zh/support/"
                                                        } else {
                                                            "https://fluxion.alkinum.com/support/"
                                                        });
                                                    })),
                                            ),
                                    ),
                            )
                            .children(self.settings_form.clone()),
                    ),
                    &self.settings_scroll,
                    cx.theme().background,
                )
                .flex_1(),
            )
            .into_any_element()
    }
}
