mod detail;
mod list;
mod settings;

use crate::{
    backend::{Action as TaskAction, Backend, Command, Message},
    form::{Form, FormEvent, FormKind, limit_text},
    i18n::t,
    model::*,
    updater::{Channel, PreparedUpdate, Status as UpdateStatus},
};
use fluxion_core::*;
use gpui::{prelude::*, *};
use gpui_component::{
    button::*,
    checkbox::Checkbox,
    dialog::DialogButtonProps,
    input::{Input, InputEvent, InputState},
    *,
};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

pub struct Workspace {
    backend: Backend,
    store: crate::model::TaskStore,
    preferences: Preferences,
    settings: SettingsSnapshot,
    filter: Filter,
    settings_open: bool,
    selected: Option<TaskId>,
    checked: BTreeSet<TaskId>,
    detail: Option<TaskDetail>,
    bt: Option<BtStateSnapshot>,
    search: Entity<InputState>,
    focus: FocusHandle,
    sort: u8,
    update: Option<crate::updater::Release>,
    update_status: UpdateStatus,
    update_prepared: Option<PreparedUpdate>,
    update_checked_at: Option<chrono::DateTime<chrono::Local>>,
    loading: bool,
    busy: bool,
    error: String,
    form: Option<Entity<Form>>,
    settings_form: Option<Entity<Form>>,
    subscriptions: Vec<Subscription>,
    last_bt: Instant,
    list_scroll: UniformListScrollHandle,
}

impl Workspace {
    pub fn new(
        backend: Backend,
        preferences: Preferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t(&preferences.locale, "list.search"))
        });
        if preferences.auto_check_updates {
            let _ = backend
                .commands
                .try_send(Command::CheckUpdate(preferences.update_channel, false));
        }
        let messages = backend.messages.clone();
        cx.defer_in(window, move |_, window, cx| {
            cx.spawn_in(window, async move |view, cx| {
                while let Ok(message) = messages.recv().await {
                    if view
                        .update_in(cx, |this, window, cx| this.receive(message, window, cx))
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .detach();
            cx.spawn_in(window, async move |view, cx| {
                loop {
                    cx.background_executor()
                        .timer(crate::updater::CHECK_INTERVAL)
                        .await;
                    if view
                        .update_in(cx, |this, _, cx| {
                            if this.preferences.auto_check_updates
                                && !this.update_status.busy()
                                && this.update_prepared.is_none()
                            {
                                this.check_for_updates(false, cx);
                            }
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .detach();
        });
        let subscriptions = vec![
            cx.subscribe(&search, |_, _, event, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.observe_window_appearance(window, |this, window, cx| {
                if this.preferences.appearance == "system" {
                    crate::apply_theme("system", Some(window), cx);
                    cx.notify();
                }
            }),
        ];
        let focus = cx.focus_handle();
        window.focus(&focus);
        let update_status = if std::env::args().any(|arg| arg == "--fluxion-update-failed") {
            UpdateStatus::Error(t(&preferences.locale, "native.updateRolledBack"))
        } else {
            UpdateStatus::Idle
        };
        Self {
            backend,
            store: Default::default(),
            preferences,
            settings: Default::default(),
            filter: Filter::All,
            settings_open: false,
            selected: None,
            checked: Default::default(),
            detail: None,
            bt: None,
            search,
            focus,
            sort: 0,
            update: None,
            update_status,
            update_prepared: None,
            update_checked_at: None,
            loading: true,
            busy: false,
            error: String::new(),
            form: None,
            settings_form: None,
            subscriptions,
            last_bt: Instant::now(),
            list_scroll: UniformListScrollHandle::new(),
        }
    }
    pub fn observe_root(&mut self, root: &Entity<Root>, cx: &mut Context<Self>) {
        // The window root does not exist during Workspace construction. Attach
        // after Root is created so dialog and notification changes redraw us.
        self.subscriptions
            .push(cx.observe(root, |_, _, cx| cx.notify()));
    }
    fn tr(&self, key: &str) -> String {
        t(&self.preferences.locale, key)
    }
    fn send(&mut self, command: Command, cx: &mut Context<Self>) {
        if self.backend.commands.try_send(command).is_err() {
            self.error = self.tr("native.unavailable");
            self.busy = false;
            self.update_status = UpdateStatus::Error(self.tr("native.unavailable"));
            for form in [&self.form, &self.settings_form].into_iter().flatten() {
                form.update(cx, |form, cx| {
                    form.busy = false;
                    cx.notify();
                });
            }
        }
        cx.notify();
    }
    fn persist(&mut self, cx: &mut Context<Self>) {
        self.send(Command::Preferences(self.preferences.clone()), cx);
    }
    fn check_for_updates(&mut self, manual: bool, cx: &mut Context<Self>) {
        if self.update_status.installing() || self.update_prepared.is_some() {
            return;
        }
        self.update_status = UpdateStatus::Checking;
        self.send(
            Command::CheckUpdate(self.preferences.update_channel, manual),
            cx,
        );
    }
    fn change_update_channel(&mut self, channel: Channel, cx: &mut Context<Self>) {
        if self.update_status.installing() || self.preferences.update_channel == channel {
            return;
        }
        self.preferences.update_channel = channel;
        self.update = None;
        self.update_prepared = None;
        self.update_checked_at = None;
        self.persist(cx);
        self.check_for_updates(true, cx);
    }
    fn receive(&mut self, message: Message, window: &mut Window, cx: &mut Context<Self>) {
        match message {
            Message::UpdateChecked(channel, manual, result) => {
                if channel != self.preferences.update_channel
                    || self.update_status.installing()
                    || self.update_prepared.is_some()
                {
                    return;
                }
                match result {
                    Ok(release) => {
                        self.update = release;
                        self.update_checked_at = Some(chrono::Local::now());
                        self.update_status = if self.update.is_some() {
                            UpdateStatus::Available
                        } else {
                            UpdateStatus::Current
                        };
                    }
                    Err(_) => {
                        self.update_status = if manual {
                            UpdateStatus::Error(self.tr("native.updateCheckFailed"))
                        } else if self.update.is_some() {
                            UpdateStatus::Available
                        } else {
                            UpdateStatus::Idle
                        };
                    }
                }
            }
            Message::UpdateProgress(done, total) => {
                self.update_status = UpdateStatus::Downloading(done, total);
            }
            Message::UpdateVerifying => self.update_status = UpdateStatus::Verifying,
            Message::UpdateReady(prepared) => {
                self.update_status = UpdateStatus::Ready;
                self.update_prepared = Some(prepared);
            }
            Message::UpdateError(error) => {
                self.update_status = UpdateStatus::Error(error);
            }
            Message::Restart => crate::request_shutdown(cx),
            Message::Snapshot(tasks, settings) => {
                self.store.tasks = tasks;
                self.settings = settings;
                self.loading = false;
                self.store.speeds.retain(|id, _| {
                    self.store
                        .tasks
                        .iter()
                        .any(|t| t.id == *id && active(&t.state))
                });
                self.checked
                    .retain(|id| self.store.tasks.iter().any(|t| t.id == *id));
                if let Some(id) = self.selected {
                    self.send(Command::Detail(id), cx);
                }
            }
            Message::Event(event) => {
                if let CoreEvent::SettingsChanged(settings) = &event {
                    self.settings = settings.clone();
                }
                self.store.apply(&event);
                if let CoreEvent::TaskStateChanged { task_id, .. }
                | CoreEvent::TaskCompleted { task_id, .. } = &event
                {
                    if Some(*task_id) != self.selected {
                        self.send(Command::Detail(*task_id), cx);
                    }
                }

                if let Some(detail) = &mut self.detail
                    && let Some(task) = self
                        .store
                        .tasks
                        .iter()
                        .find(|task| task.id == detail.task.id)
                {
                    detail.task.state = task.state.clone();
                    detail.task.downloaded_bytes = task.downloaded_bytes;
                    detail.task.uploaded_bytes = task.uploaded_bytes;
                    detail.task.total_bytes = task.total_bytes;
                    detail.task.error = task.error.clone();
                }
                if let Some(id) = self.selected {
                    if matches!(event,CoreEvent::TaskStateChanged{task_id,..}|CoreEvent::TaskCompleted{task_id,..} if task_id==id)
                    {
                        self.send(Command::Detail(id), cx);
                    }
                    if self.last_bt.elapsed() > Duration::from_secs(1)
                        && self
                            .store
                            .tasks
                            .iter()
                            .any(|t| t.id == id && t.kind == DownloadKind::Bt && active(&t.state))
                    {
                        self.last_bt = Instant::now();
                        self.send(Command::Bt(id), cx);
                    }
                }
            }
            Message::Detail(id, detail) => {
                if let Some(detail) = &detail
                    && let Some(task) = self.store.tasks.iter_mut().find(|t| t.id == id)
                {
                    *task = TaskSummary::from(&detail.task);
                }
                if self.selected == Some(id) {
                    self.detail = detail;
                }
            }
            Message::Bt(id, state) => {
                if self.selected == Some(id) {
                    self.bt = state;
                }
            }
            Message::Created(id) => {
                self.form = None;
                window.close_dialog(cx);
                self.filter = Filter::All;
                self.settings_open = false;
                self.select(id, window, cx);
                self.send(Command::Refresh, cx);
            }
            Message::Acted(id, action) => {
                match action {
                    TaskAction::Trash => {
                        self.preferences.trash.insert(id);
                        self.checked.remove(&id);
                        self.persist(cx);
                    }
                    TaskAction::Restore | TaskAction::Start => {
                        if self.preferences.trash.remove(&id) {
                            self.persist(cx);
                        }
                    }
                    TaskAction::Delete(_) => {
                        self.store.tasks.retain(|t| t.id != id);
                        self.store.speeds.remove(&id);
                        self.preferences.trash.remove(&id);
                        self.checked.remove(&id);
                        self.persist(cx);
                    }
                    _ => (),
                }
                if self.selected == Some(id) {
                    if matches!(
                        action,
                        TaskAction::Delete(_) | TaskAction::Trash | TaskAction::Restore
                    ) {
                        self.selected = None;
                        self.detail = None;
                        self.bt = None;
                    } else {
                        self.send(Command::Detail(id), cx);
                    }
                }
            }
            Message::Saved => {
                if self.form.is_some() {
                    self.form = None;
                    window.close_dialog(cx);
                }
                window.push_notification(self.tr("detail.limitsSaved"), cx);
            }
            Message::Preview(generation, result) => {
                if let Some(form) = &self.form {
                    form.update(cx, |form, cx| {
                        if form.preview_generation == generation {
                            form.resolving = false;
                            match result {
                                Ok(preview) => form.preview = Some(preview),
                                Err(error) => form.error = error,
                            }
                            cx.notify();
                        }
                    });
                }
            }
            Message::Error(error) => {
                self.loading = false;
                self.error = error.clone();
                if let Some(form) = self.form.as_ref().or(self.settings_form.as_ref()) {
                    form.update(cx, |form, cx| {
                        form.error = error;
                        form.busy = false;
                        cx.notify();
                    });
                }
            }
            Message::Finished => {
                self.busy = false;
                for form in [&self.form, &self.settings_form].into_iter().flatten() {
                    form.update(cx, |form, cx| {
                        form.busy = false;
                        cx.notify();
                    });
                }
            }
        }
        cx.notify();
    }
    fn visible(&self, cx: &App) -> Vec<TaskSummary> {
        let search = self.search.read(cx).value().to_lowercase();
        let mut tasks = self
            .store
            .tasks
            .iter()
            .filter(|task| {
                self.filter.matches(task, &self.preferences.trash)
                    && task
                        .file_name
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&search)
            })
            .cloned()
            .collect::<Vec<_>>();
        match self.sort {
            1 => tasks.sort_by(|a, b| a.file_name.cmp(&b.file_name)),
            2 => tasks.sort_by_key(|t| {
                std::cmp::Reverse(self.store.speeds.get(&t.id).copied().unwrap_or_default().0)
            }),
            _ => tasks.sort_by_key(|t| std::cmp::Reverse(t.created_at)),
        };
        tasks
    }
    fn select(&mut self, id: TaskId, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected != Some(id) {
            self.detail = None;
            self.bt = None;
        }
        self.selected = Some(id);
        self.preferences.detail_open = true;
        self.send(Command::Detail(id), cx);
        if self
            .store
            .tasks
            .iter()
            .any(|t| t.id == id && t.kind == DownloadKind::Bt)
        {
            self.send(Command::Bt(id), cx);
        }
        window.focus(&self.focus);
        cx.notify();
    }
    fn select_filter(&mut self, filter: Filter, cx: &mut Context<Self>) {
        self.filter = filter;
        self.settings_open = false;
        self.checked.clear();
        if let Some(id) = self.selected
            && !self
                .store
                .tasks
                .iter()
                .any(|t| t.id == id && filter.matches(t, &self.preferences.trash))
        {
            self.selected = None;
            self.detail = None;
            self.bt = None;
        }
        cx.notify();
    }
    fn act(
        &mut self,
        ids: Vec<TaskId>,
        action: TaskAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || window.has_active_dialog(cx) {
            return;
        }
        let ids: Vec<_> = ids
            .into_iter()
            .filter(|id| {
                self.store
                    .tasks
                    .iter()
                    .find(|t| t.id == *id)
                    .is_some_and(|t| match action {
                        TaskAction::Start => can_start(&t.state),
                        TaskAction::Pause => active(&t.state),
                        TaskAction::Stop => can_stop(&t.state),
                        TaskAction::Open | TaskAction::Reveal => t.state == TaskState::Completed,
                        _ => true,
                    })
            })
            .collect();
        if ids.is_empty() {
            return;
        }
        if matches!(
            action,
            TaskAction::Stop | TaskAction::Trash | TaskAction::Delete(_)
        ) {
            let label = self.tr(match action {
                TaskAction::Stop => "list.action.stop",
                TaskAction::Trash => "confirm.moveTrash",
                _ => "common.delete",
            });
            let message = self.tr(match action {
                TaskAction::Stop => "confirm.stopMessage",
                TaskAction::Trash => "confirm.trashMessage",
                _ => "confirm.deleteMessage",
            });
            let delete_label = self.tr("confirm.deleteFiles");
            let cancel = self.tr("common.cancel");
            let selection = self
                .tr("list.selected")
                .replace("{count}", &ids.len().to_string());
            let files = std::rc::Rc::new(std::cell::Cell::new(false));
            let view = cx.entity().downgrade();
            window.open_dialog(cx, move |dialog, _, _| {
                let checked = files.clone();
                let selected = files.clone();
                let view = view.clone();
                let ids = ids.clone();
                dialog
                    .title(label.clone())
                    .child(
                        v_flex()
                            .gap_2()
                            .child(div().font_medium().child(selection.clone()))
                            .child(message.clone()),
                    )
                    .confirm()
                    .button_props(
                        DialogButtonProps::default()
                            .ok_text(label.clone())
                            .cancel_text(cancel.clone()),
                    )
                    .overlay_closable(false)
                    .when(matches!(action, TaskAction::Delete(_)), |d| {
                        d.child(
                            Checkbox::new("delete-files")
                                .label(delete_label.clone())
                                .checked(files.get())
                                .on_click(move |value, _, cx| {
                                    checked.set(*value);
                                    cx.refresh_windows();
                                }),
                        )
                    })
                    .on_ok(move |_, _, cx| {
                        let _ = view.update(cx, |this, cx| {
                            if !this.busy {
                                this.busy = true;
                                this.send(
                                    Command::Act(
                                        ids.clone(),
                                        if matches!(action, TaskAction::Delete(_)) {
                                            TaskAction::Delete(selected.get())
                                        } else {
                                            action
                                        },
                                    ),
                                    cx,
                                );
                            }
                        });
                        true
                    })
            });
        } else {
            self.busy = true;
            self.error.clear();
            self.send(Command::Act(ids, action), cx);
        }
    }
    fn open_form(&mut self, kind: FormKind, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || window.has_active_dialog(cx) {
            return;
        }
        let title = self.tr(if matches!(kind, FormKind::Create) {
            "new.title"
        } else {
            "detail.rateLimits"
        });
        let form = cx.new(|cx| Form::new(kind, self.preferences.locale.clone(), window, cx));
        if let Some(detail) = &self.detail
            && matches!(form.read(cx).kind, FormKind::Limits(_))
        {
            form.update(cx, |form, cx| {
                form.set(
                    "down",
                    limit_text(detail.task.limits.download_bytes_per_second),
                    window,
                    cx,
                );
                form.set(
                    "up",
                    limit_text(detail.task.limits.upload_bytes_per_second),
                    window,
                    cx,
                );
            });
        }
        self.subscriptions.push(
            cx.subscribe_in(&form, window, |this, _, event, window, cx| {
                this.form_event(event, window, cx)
            }),
        );
        self.form = Some(form.clone());
        let weak = cx.entity().downgrade();
        let focus = form.clone();
        window.open_dialog(cx, move |dialog, _, cx| {
            dialog
                .title(title.clone())
                .w(px(600.))
                .margin_top(px(60.))
                .keyboard(!form.read(cx).busy)
                .close_button(false)
                .overlay_closable(false)
                .child(form.clone())
                .on_close({
                    let weak = weak.clone();
                    move |_, _, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            this.form = None;
                            cx.notify();
                        });
                    }
                })
        });
        focus.update(cx, |form, cx| form.focus(window, cx));
        cx.notify();
    }
    fn form_event(&mut self, event: &FormEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            FormEvent::Cancel => {
                self.form = None;
                window.close_dialog(cx);
            }
            FormEvent::Submit(command) => {
                let command = match command {
                    Command::Create(input) => Command::Create(input.clone()),
                    Command::Settings(s) => Command::Settings(s.clone()),
                    Command::Limits(id, limits) => Command::Limits(*id, limits.clone()),
                    Command::Preview(g, s) => Command::Preview(*g, s.clone()),
                    _ => return,
                };
                if !matches!(command, Command::Preview(..)) {
                    self.busy = true;
                }
                self.send(command, cx);
            }
        }
    }
    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        if self.settings_form.is_none() {
            let form = cx.new(|cx| {
                Form::new(
                    FormKind::Settings(self.settings.clone()),
                    self.preferences.locale.clone(),
                    window,
                    cx,
                )
            });
            self.subscriptions.push(cx.subscribe_in(
                &form,
                window,
                |this, _, event, window, cx| this.form_event(event, window, cx),
            ));
            self.settings_form = Some(form);
        }
        self.settings_open = true;
        cx.notify();
    }
    fn move_selection(&mut self, direction: isize, window: &mut Window, cx: &mut Context<Self>) {
        let tasks = self.visible(cx);
        if tasks.is_empty() {
            return;
        }
        let index = self
            .selected
            .and_then(|id| tasks.iter().position(|t| t.id == id))
            .map(|i| (i as isize + direction).clamp(0, tasks.len() as isize - 1) as usize)
            .unwrap_or(0);
        self.select(tasks[index].id, window, cx);
        self.list_scroll.scroll_to_item(index, ScrollStrategy::Top);
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let modal = Root::render_dialog_layer(window, cx);
        let notifications = Root::render_notification_layer(window, cx);
        let narrow = window.viewport_size().width < px(1020.);
        div()
            .id("workspace")
            .key_context("Downloads")
            .track_focus(&self.focus)
            .size_full()
            .font_family(".SystemUIFont")
            .text_size(px(14.))
            .text_color(cx.theme().foreground)
            .relative()
            .bg(canvas_color(cx))
            .on_action(cx.listener(|this, _: &crate::NewDownload, window, cx| {
                this.open_form(FormKind::Create, window, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::Search, window, cx| {
                if !window.has_active_dialog(cx) {
                    this.search.update(cx, |input, cx| input.focus(window, cx));
                }
            }))
            .on_action(
                cx.listener(|this, _: &crate::Settings, window, cx| this.open_settings(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &crate::Refresh, _, cx| this.send(Command::Refresh, cx)),
            )
            .on_action(cx.listener(|this, _: &crate::ToggleDetail, _, cx| {
                this.preferences.detail_open = !this.preferences.detail_open;
                this.persist(cx);
            }))
            .on_action(cx.listener(|this, _: &crate::SelectAll, window, cx| {
                if !window.has_focused_input(cx) {
                    this.checked = this.visible(cx).iter().map(|t| t.id).collect();
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &crate::Remove, window, cx| {
                if !window.has_focused_input(cx) {
                    let ids = if this.checked.is_empty() {
                        this.selected.into_iter().collect()
                    } else {
                        this.checked.iter().copied().collect()
                    };
                    this.act(
                        ids,
                        if this.filter == Filter::Trash {
                            TaskAction::Delete(false)
                        } else {
                            TaskAction::Trash
                        },
                        window,
                        cx,
                    );
                }
            }))
            .on_action(cx.listener(|this, _: &crate::NextTask, window, cx| {
                if !window.has_focused_input(cx) {
                    this.move_selection(1, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &crate::PreviousTask, window, cx| {
                if !window.has_focused_input(cx) {
                    this.move_selection(-1, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &crate::ToggleTask, window, cx| {
                if !window.has_focused_input(cx)
                    && let Some(id) = this.selected
                    && let Some(task) = this.store.tasks.iter().find(|t| t.id == id)
                {
                    let action = if active(&task.state) {
                        TaskAction::Pause
                    } else {
                        TaskAction::Start
                    };
                    this.act(vec![id], action, window, cx);
                }
            }))
            .child(
                h_flex()
                    .size_full()
                    .items_start()
                    .child(self.sidebar(cx))
                    .child(
                        h_flex()
                            .h_full()
                            .min_w_0()
                            .flex_1()
                            .py_3()
                            .pr_3()
                            .gap_3()
                            .child(if self.settings_open {
                                self.settings_pane(cx)
                            } else {
                                h_flex()
                                    .h_full()
                                    .min_w_0()
                                    .flex_1()
                                    .gap_3()
                                    .child(self.task_list(window, cx))
                                    .when(self.preferences.detail_open && !narrow, |v| {
                                        v.child(self.detail_pane(cx))
                                    })
                                    .into_any_element()
                            }),
                    ),
            )
            .when(
                narrow
                    && !self.settings_open
                    && self.preferences.detail_open
                    && self.selected.is_some(),
                |v| {
                    v.child(
                        div()
                            .occlude()
                            .absolute()
                            .top_3()
                            .bottom_3()
                            .left(px(208.))
                            .right_3()
                            .rounded_xl()
                            .bg(cx.theme().background)
                            .shadow_lg()
                            .child(self.detail_pane(cx)),
                    )
                },
            )
            .when(!self.error.is_empty() && self.form.is_none(), |v| {
                v.child(
                    h_flex()
                        .absolute()
                        .bottom_4()
                        .left(px(220.))
                        .right_4()
                        .gap_3()
                        .px_4()
                        .py_3()
                        .rounded_lg()
                        .bg(cx.theme().background)
                        .border_1()
                        .border_color(cx.theme().danger)
                        .shadow_lg()
                        .child(icon_view("triangle-alert").text_color(cx.theme().danger))
                        .child(div().flex_1().min_w_0().text_sm().child(self.error.clone()))
                        .child(
                            self.icon_button("dismiss", "close", "common.dismiss")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.error.clear();
                                    cx.notify();
                                })),
                        ),
                )
            })
            .children(modal)
            .children(notifications)
    }
}

// Large surfaces are separated by space and tone, not outlines or bevels.
fn canvas_color(cx: &App) -> Hsla {
    rgb(if cx.theme().is_dark() {
        0x13161c
    } else {
        0xeff2f6
    })
    .into()
}
fn inset_color(cx: &App) -> Hsla {
    rgb(if cx.theme().is_dark() {
        0x252a32
    } else {
        0xf3f5f8
    })
    .into()
}
fn selected_color(cx: &App) -> Hsla {
    rgb(if cx.theme().is_dark() {
        0x303540
    } else {
        0xe9eef5
    })
    .into()
}

fn icon_view(name: &str) -> Icon {
    Icon::default()
        .path(SharedString::from(format!("icons/{name}.svg")))
        .size(px(20.))
}
fn file_icon(name: &str) -> &'static str {
    match name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "zip" | "dmg" | "tar" | "gz" | "iso" | "7z" => "file-archive",
        "mp4" | "mov" | "mkv" => "file-video",
        "mp3" | "wav" | "flac" => "file-audio",
        "png" | "jpg" | "webp" => "file-image",
        _ => "file",
    }
}
fn state_color(state: &TaskState, cx: &App) -> Hsla {
    match state {
        TaskState::Downloading | TaskState::Seeding => rgb(if cx.theme().is_dark() {
            0x94c8ac
        } else {
            0x2a7954
        })
        .into(),
        TaskState::Completed => rgb(if cx.theme().is_dark() {
            0x9badc3
        } else {
            0x546e8b
        })
        .into(),
        TaskState::Failed => cx.theme().danger,
        TaskState::Resolving | TaskState::Verifying => cx.theme().primary,
        _ => cx.theme().muted_foreground,
    }
}
fn progress_bar(progress: f32, color: Hsla, cx: &App) -> Div {
    div()
        .w_full()
        .h(px(4.))
        .flex_shrink_0()
        .rounded_full()
        .bg(inset_color(cx))
        .overflow_hidden()
        .child(
            div()
                .h_full()
                .w(relative(progress))
                .bg(color)
                .rounded_full(),
        )
}
fn metric(label: String, value: String, cx: &App) -> Div {
    v_flex()
        .flex_1()
        .min_w_0()
        .gap_2()
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(div().text_base().font_medium().child(value))
}
fn section(label: String, cx: &App) -> Div {
    div()
        .text_size(px(13.))
        .font_semibold()
        .text_color(cx.theme().muted_foreground)
        .child(label)
}
fn info(label: String, value: String, cx: &App) -> Div {
    v_flex()
        .gap_1()
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(div().text_sm().overflow_hidden().child(value))
}
fn empty(icon: &str, title: String, copy: String, cx: &App) -> AnyElement {
    v_flex()
        .flex_1()
        .size_full()
        .items_center()
        .justify_center()
        .px_8()
        .gap_3()
        .child(
            icon_view(icon)
                .size(px(28.))
                .text_color(cx.theme().muted_foreground),
        )
        .child(
            div()
                .mt_2()
                .text_base()
                .font_medium()
                .text_center()
                .child(title),
        )
        .child(
            div()
                .text_sm()
                .text_center()
                .text_color(cx.theme().muted_foreground)
                .max_w(px(260.))
                .child(copy),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{Backend, Message, Preferences, Root, SettingsSnapshot, Workspace};
    use gpui::{AppContext, WindowOptions};
    use gpui_component::WindowExt;
    #[gpui::test]
    fn snapshot_queued_before_window_creation_is_applied(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            crate::apply_theme("dark", None, cx);
        });
        let (commands, _rx) = async_channel::bounded(32);
        let (tx, messages) = async_channel::bounded(32);
        let (_done, stopped) = async_channel::bounded(1);
        tx.try_send(Message::Snapshot(vec![], SettingsSnapshot::default()))
            .unwrap();
        let mut workspace = None;
        cx.add_window(|window, cx| {
            let view = cx.new(|cx| {
                Workspace::new(
                    Backend {
                        commands,
                        messages,
                        stopped,
                    },
                    Preferences::default(),
                    window,
                    cx,
                )
            });
            workspace = Some(view.clone());
            Root::new(view, window, cx)
        });
        cx.run_until_parked();
        workspace.unwrap().update(cx, |view, _| {
            assert!(
                !view.loading,
                "A snapshot queued before the first draw must not be dropped"
            )
        });
        drop(tx);
        cx.run_until_parked();
    }
    #[gpui::test]
    fn root_dialog_changes_redraw_workspace(cx: &mut gpui::TestAppContext) {
        use std::{cell::Cell, rc::Rc};
        cx.update(|cx| {
            gpui_component::init(cx);
            crate::apply_theme("dark", None, cx);
        });
        let (commands, _rx) = async_channel::bounded(32);
        let (_tx, messages) = async_channel::bounded(32);
        let (_done, stopped) = async_channel::bounded(1);
        let mut workspace = None;
        let window = cx.update(|cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let view = cx.new(|cx| {
                    Workspace::new(
                        Backend {
                            commands,
                            messages,
                            stopped,
                        },
                        Preferences::default(),
                        window,
                        cx,
                    )
                });
                workspace = Some(view.clone());
                let root = cx.new(|cx| Root::new(view.clone(), window, cx));
                view.update(cx, |view, cx| view.observe_root(&root, cx));
                root
            })
            .unwrap()
        });
        cx.run_until_parked();
        let redraws = Rc::new(Cell::new(0));
        let count = redraws.clone();
        let _subscription = cx.update(|cx| {
            cx.observe(workspace.as_ref().unwrap(), move |_, _| {
                count.set(count.get() + 1)
            })
        });
        window
            .update(cx, |_, window, cx| {
                window.defer(cx, |window, cx| {
                    window.open_dialog(cx, |dialog, _, _| dialog.title("Confirm"));
                });
            })
            .unwrap();
        cx.run_until_parked();
        let opened = redraws.get();
        assert!(
            opened > 0,
            "Opening a root dialog must invalidate the workspace layer"
        );
        window
            .update(cx, |_, window, cx| {
                window.defer(cx, |window, cx| window.close_dialog(cx));
            })
            .unwrap();
        cx.run_until_parked();
        assert!(
            redraws.get() > opened,
            "Closing a root dialog must remove the workspace overlay"
        );
    }
    #[gpui::test]
    fn channel_switch_ignores_stale_results_and_background_checks_obey_preferences(
        cx: &mut gpui::TestAppContext,
    ) {
        use super::{Channel, Command, UpdateStatus};
        cx.update(|cx| {
            gpui_component::init(cx);
            crate::apply_theme("dark", None, cx);
        });
        let (commands, rx) = async_channel::bounded(32);
        let (_tx, messages) = async_channel::bounded(32);
        let (_done, stopped) = async_channel::bounded(1);
        let mut view = None;
        let window = cx.add_window(|window, cx| {
            let workspace = cx.new(|cx| {
                Workspace::new(
                    Backend {
                        commands,
                        messages,
                        stopped,
                    },
                    Preferences {
                        update_channel: Channel::Stable,
                        auto_check_updates: false,
                        ..Default::default()
                    },
                    window,
                    cx,
                )
            });
            view = Some(workspace.clone());
            Root::new(workspace, window, cx)
        });
        cx.run_until_parked();
        assert!(
            rx.try_recv().is_err(),
            "disabled automatic checks must not contact a server on launch"
        );
        let view = view.unwrap();
        window
            .update(cx, |_, window, cx| {
                view.update(cx, |view, cx| {
                    view.change_update_channel(Channel::Beta, cx);
                    view.receive(
                        Message::UpdateChecked(Channel::Stable, true, Ok(None)),
                        window,
                        cx,
                    );
                    assert!(matches!(view.update_status, UpdateStatus::Checking));
                    view.receive(
                        Message::UpdateChecked(Channel::Beta, true, Ok(None)),
                        window,
                        cx,
                    );
                    assert!(matches!(view.update_status, UpdateStatus::Current));
                    view.preferences.auto_check_updates = true;
                })
            })
            .unwrap();
        assert!(matches!(rx.try_recv().unwrap(), Command::Preferences(_)));
        assert!(matches!(
            rx.try_recv().unwrap(),
            Command::CheckUpdate(Channel::Beta, true)
        ));
        cx.executor().advance_clock(crate::updater::CHECK_INTERVAL);
        cx.run_until_parked();
        assert!(matches!(
            rx.try_recv().unwrap(),
            Command::CheckUpdate(Channel::Beta, false)
        ));
        view.update(cx, |view, _| {
            view.preferences.auto_check_updates = false;
            view.update_status = UpdateStatus::Idle;
        });
        cx.executor().advance_clock(crate::updater::CHECK_INTERVAL);
        cx.run_until_parked();
        assert!(rx.try_recv().is_err());
        // Exercise the settings composition with real GPUI layout, including
        // narrow windows and long localized controls. This is not a screenshot test.
        let visual = gpui::VisualTestContext::from_window(*window, cx);
        for width in [760., 1180.] {
            visual.simulate_resize(gpui::size(gpui::px(width), gpui::px(760.)));
            for appearance in ["light", "dark"] {
                for locale in ["en", "zh-CN", "ja", "ko"] {
                    window
                        .update(cx, |_, window, cx| {
                            let view = view.clone();
                            window.defer(cx, move |window, cx| {
                                crate::apply_theme(appearance, Some(window), cx);
                                view.update(cx, |view, cx| {
                                    view.preferences.locale = locale.into();
                                    view.open_settings(window, cx);
                                    view.update = Some(crate::updater::Release {
                                        version: "1.1.0-beta.1".into(),
                                        channel: Some(Channel::Beta),
                                        notes: "Release notes".into(),
                                        platforms: Default::default(),
                                    });
                                    view.update_status = UpdateStatus::Available;
                                });
                            });
                        })
                        .unwrap();
                    cx.run_until_parked();
                }
            }
        }
    }
}
