use super::*;

impl Workspace {
    pub(super) fn settings_pane(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut appearance = h_flex().gap_2();
        for (id, name) in [
            ("system", "native.system"),
            ("light", "native.light"),
            ("dark", "native.dark"),
        ] {
            appearance = appearance.child(
                Button::new(id)
                    .label(self.tr(name))
                    .when(self.preferences.appearance == id, |b| b.primary())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.preferences.appearance = id.into();
                        crate::apply_theme(id, Some(window), cx);
                        this.persist(cx);
                    })),
            );
        }
        let mut languages = h_flex().gap_2();
        for (id, label) in [
            ("en", "English"),
            ("zh-CN", "简体中文"),
            ("ja", "日本語"),
            ("ko", "한국어"),
        ] {
            languages = languages.child(
                Button::new(id)
                    .small()
                    .label(label)
                    .when(self.preferences.locale == id, |b| b.primary())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.preferences.locale = id.into();
                        if let Some(form) = &this.settings_form {
                            form.update(cx, |form, cx| {
                                form.set_locale(id, window, cx);
                            });
                        }
                        this.search.update(cx, |input, cx| {
                            input.set_placeholder(t(id, "list.search"), window, cx)
                        });
                        this.persist(cx);
                    })),
            );
        }
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .rounded_2xl()
            .overflow_hidden()
            .bg(cx.theme().background)
            .child(
                div()
                    .h(px(80.))
                    .pt_8()
                    .px_8()
                    .text_xl()
                    .font_semibold()
                    .child(self.tr("settings.title")),
            )
            .child(
                v_flex()
                    .id("settings-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_8()
                    .pb_8()
                    .gap_5()
                    .child(section(self.tr("settings.appearance.title"), cx))
                    .child(appearance)
                    .child(languages)
                    .children(self.settings_form.clone())
                    .child(section(self.tr("settings.update.title"), cx))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("check-update")
                                    .label(self.tr("settings.update.check"))
                                    .disabled(self.update_busy)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.update_busy = true;
                                        this.update_status = this.tr("settings.update.checking");
                                        this.send(Command::CheckUpdate(true), cx);
                                    })),
                            )
                            .when(self.update.is_some(), |row| {
                                row.child(
                                    Button::new("install-update")
                                        .primary()
                                        .label(self.tr("update.install"))
                                        .disabled(self.update_busy)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if let Some(release) = this.update.clone() {
                                                this.update_busy = true;
                                                this.send(Command::InstallUpdate(release), cx);
                                            }
                                        })),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(self.update_status.clone()),
                    )
                    .child(
                        div()
                            .pt_5()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("Fluxion {}", crate::updater::VERSION)),
                    ),
            )
            .into_any_element()
    }
}
