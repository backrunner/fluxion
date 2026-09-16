use crate::{backend::Command, i18n::t, model::bytes};
use fluxion_core::*;
use gpui::{prelude::*, *};
use gpui_component::{
    button::*,
    checkbox::Checkbox,
    input::{Input, InputEvent, InputState},
    switch::Switch,
    *,
};
use std::{collections::BTreeMap, path::PathBuf};

const INPUT_FIELDS: &[(&str, &str, &str, bool, bool)] = &[
    ("source", "new.linkPlaceholder", "", false, false),
    ("directory", "new.chooseFolder", "", false, false),
    ("filename", "new.autoDetected", "", false, false),
    ("connections", "new.maxConnections", "16", false, false),
    ("down", "common.unlimited", "", false, false),
    ("up", "common.unlimited", "", false, false),
    ("headers", "fields.customHeaders", "", true, true),
    ("cookie", "fields.cookie", "", false, true),
    ("referer", "fields.referer", "", false, false),
    ("agent", "fields.userAgent", "", false, false),
    ("username", "credentials.username", "", false, false),
    ("password", "credentials.password", "", false, true),
    ("privatekey", "fields.privateKeyPath", "", false, false),
    ("passphrase", "credentials.keyPassphrase", "", false, true),
    ("trackers", "common.onePerLine", "", true, true),
    ("ratio", "fields.noLimit", "", false, false),
    ("allow", "common.onePerLine", "", true, false),
    ("deny", "common.onePerLine", "", true, false),
];

pub enum FormEvent {
    Submit(Command),
    Cancel,
}
pub enum FormKind {
    Create,
    Settings(SettingsSnapshot),
    Limits(TaskId),
}

pub struct Form {
    pub kind: FormKind,
    pub locale: String,
    pub inputs: BTreeMap<&'static str, Entity<InputState>>,
    pub error: String,
    pub busy: bool,
    advanced: bool,
    proxy: bool,
    seeding: bool,
    pub preview: Option<MagnetPreview>,
    pub preview_generation: u64,
    pub resolving: bool,
    subscriptions: Vec<Subscription>,
    revealed: std::collections::BTreeSet<&'static str>,
}
impl EventEmitter<FormEvent> for Form {}

impl Form {
    pub fn new(
        kind: FormKind,
        locale: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut inputs = BTreeMap::new();
        for &(key, placeholder, value, multiline, secret) in INPUT_FIELDS {
            inputs.insert(
                key,
                cx.new(|cx| {
                    let mut input = InputState::new(window, cx)
                        .placeholder(t(&locale, placeholder))
                        .multi_line(multiline);
                    if !multiline {
                        input = input.masked(secret);
                    }
                    input.set_value(value, window, cx);
                    input
                }),
            );
        }
        let mut form = Self {
            kind,
            locale,
            inputs,
            error: String::new(),
            busy: false,
            advanced: false,
            proxy: true,
            seeding: true,
            preview: None,
            preview_generation: 0,
            resolving: false,
            subscriptions: vec![],
            revealed: Default::default(),
        };
        let directory = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join("Downloads");
        form.set(
            "directory",
            directory.to_string_lossy().into_owned(),
            window,
            cx,
        );
        if let FormKind::Settings(settings) = &form.kind {
            let settings = settings.clone();
            form.proxy = settings.use_system_proxy;
            form.set("down", limit_text(settings.download_limit), window, cx);
            form.set("up", limit_text(settings.upload_limit), window, cx);
            form.set(
                "trackers",
                settings
                    .bt_trackers
                    .iter()
                    .map(|u| u.as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
                window,
                cx,
            );
            form.set("allow", settings.bt_ip_allow.join("\n"), window, cx);
            form.set("deny", settings.bt_ip_deny.join("\n"), window, cx);
        }
        let source = form.inputs["source"].clone();
        form.subscriptions
            .push(cx.subscribe(&source, |this, _, event, cx| {
                if matches!(event, InputEvent::Change) {
                    this.preview = None;
                    this.preview_generation += 1;
                    this.resolving = false;
                    cx.notify();
                }
            }));
        form
    }
    pub fn set_locale(&mut self, locale: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.locale = locale.into();
        for &(key, placeholder, ..) in INPUT_FIELDS {
            self.inputs[key].update(cx, |input, cx| {
                input.set_placeholder(t(locale, placeholder), window, cx);
            });
        }
        cx.notify();
    }
    pub fn set(
        &self,
        key: &str,
        value: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.inputs[key].update(cx, |input, cx| input.set_value(value, window, cx));
    }
    fn value(&self, key: &str, cx: &App) -> String {
        self.inputs[key].read(cx).value().to_string()
    }
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        let field = if matches!(self.kind, FormKind::Create) {
            "source"
        } else {
            "down"
        };
        self.inputs[field].update(cx, |input, cx| input.focus(window, cx));
    }
    fn field(&self, key: &'static str, label: &str, cx: &Context<Self>) -> AnyElement {
        if matches!(key, "headers" | "trackers") && !self.revealed.contains(key) {
            return v_flex()
                .gap_1p5()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(t(&self.locale, label)),
                )
                .child(
                    Button::new(SharedString::from(format!("reveal-field-{key}")))
                        .icon(IconName::Eye)
                        .label(t(&self.locale, "native.editProtected"))
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.revealed.insert(key);
                            cx.notify();
                        })),
                )
                .into_any_element();
        }
        v_flex()
            .gap_1p5()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(t(&self.locale, label)),
            )
            .child(Input::new(&self.inputs[key]).disabled(self.busy).when(
                matches!(key, "headers" | "trackers" | "allow" | "deny"),
                |input| input.h(px(88.)),
            ))
            .into_any_element()
    }
    fn pick(&mut self, folder: bool, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: !folder,
            directories: folder,
            multiple: false,
            prompt: Some(
                t(
                    &self.locale,
                    if folder {
                        "new.browseFolder"
                    } else {
                        "new.chooseTorrent"
                    },
                )
                .into(),
            ),
        });
        cx.spawn_in(window, async move |view, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.first()
            {
                let value = path.to_string_lossy().into_owned();
                let _ = view.update_in(cx, |this, window, cx| {
                    this.set(
                        if folder { "directory" } else { "source" },
                        value,
                        window,
                        cx,
                    );
                    cx.notify();
                });
            }
        })
        .detach();
    }
    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let result = self.command(cx);
        match result {
            Ok(command) => {
                self.error.clear();
                self.busy = true;
                cx.emit(FormEvent::Submit(command));
            }
            Err(error) => self.error = validation_message(&self.locale, &error.to_string()),
        }
        cx.notify();
    }
    fn command(&self, cx: &App) -> anyhow::Result<Command> {
        let down = parse_limit(&self.value("down", cx))?;
        let up = parse_limit(&self.value("up", cx))?;
        let limits = TaskRateLimit {
            download_bytes_per_second: down,
            upload_bytes_per_second: up,
        };
        if let FormKind::Limits(id) = self.kind {
            return Ok(Command::Limits(id, limits));
        }
        let trackers = parse_trackers(&self.value("trackers", cx))?;
        if let FormKind::Settings(original) = &self.kind {
            let allow = lines(&self.value("allow", cx));
            let deny = lines(&self.value("deny", cx));
            for rule in allow.iter().chain(deny.iter()) {
                anyhow::ensure!(
                    cidr_contains(rule, "127.0.0.1".parse().unwrap()).is_some()
                        || cidr_contains(rule, "::1".parse().unwrap()).is_some(),
                    "Enter valid IP addresses or CIDR ranges"
                );
            }
            return Ok(Command::Settings(SettingsSnapshot {
                download_limit: down,
                upload_limit: up,
                use_system_proxy: self.proxy,
                bt_trackers: trackers,
                bt_ip_allow: allow,
                bt_ip_deny: deny,
                ..original.clone()
            }));
        }
        let values = self
            .inputs
            .iter()
            .map(|(key, _)| (*key, self.value(key, cx)))
            .collect();
        let selection = self.preview.as_ref().map(|p| {
            p.files
                .iter()
                .filter(|f| f.selected)
                .map(|f| f.index)
                .collect::<Vec<_>>()
        });
        if let Some(selection) = &selection {
            anyhow::ensure!(!selection.is_empty(), "Select at least one file");
        }
        Ok(Command::Create(build_input(
            &values,
            limits,
            trackers,
            self.proxy,
            self.seeding,
            selection.unwrap_or_default(),
        )?))
    }
}

fn validation_message(locale: &str, error: &str) -> String {
    let key = match error {
        "Enter an HTTP, HTTPS, FTP, SFTP or magnet link" => "validation.link",
        "Choose an absolute download directory" => "validation.directory",
        "Rate limits must be positive numbers (KB/s)" => "validation.rate",
        "Trackers need an HTTP, HTTPS or UDP URL" => "validation.tracker",
        "Enter valid IP addresses or CIDR ranges" => "validation.ip",
        "Select at least one file" => "validation.files",
        "The magnet link needs a BitTorrent info hash" => "validation.magnet",
        "Choose an existing .torrent file" => "validation.torrent",
        "Share ratio must be greater than zero" => "validation.ratio",
        "Connections must be a positive integer" => "validation.connections",
        "Connections must be between 1 and 65535" => "validation.httpConnections",
        "The URL needs a host" => "validation.host",
        "Each request header needs Name: value" => "validation.header",
        "Invalid request header" => "validation.headerInvalid",
        "Request fields cannot contain newlines" => "validation.newline",
        "The transfer URL must point to a file" => "validation.path",
        "FTPS is not supported. Use FTP or SFTP." => "validation.ftps",
        "Invalid tracker URL" => "validation.tracker",
        "Invalid magnet link" => "validation.magnet",
        "Enter a valid share ratio" => "validation.ratio",
        "Use an HTTP, HTTPS, FTP, SFTP or magnet link" => "validation.link",
        _ => return error.into(),
    };
    t(locale, key)
}

pub fn limit_text(value: Option<u64>) -> String {
    value
        .map(|v| format!("{}", v as f64 / 1024.))
        .unwrap_or_default()
}
pub fn parse_limit(value: &str) -> anyhow::Result<Option<u64>> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    let value = value
        .trim()
        .parse::<f64>()
        .map_err(|_| anyhow::anyhow!("Rate limits must be positive numbers (KB/s)"))?;
    anyhow::ensure!(
        value.is_finite() && value > 0. && value <= u64::MAX as f64 / 1024.,
        "Rate limits must be positive numbers (KB/s)"
    );
    Ok(Some((value * 1024.).max(1.) as u64))
}
fn lines(value: &str) -> Vec<String> {
    value
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}
fn parse_trackers(value: &str) -> anyhow::Result<Vec<url::Url>> {
    lines(value)
        .iter()
        .map(|v| {
            let url = url::Url::parse(v).map_err(|_| anyhow::anyhow!("Invalid tracker URL"))?;
            anyhow::ensure!(
                matches!(url.scheme(), "http" | "https" | "udp") && url.host_str().is_some(),
                "Trackers need an HTTP, HTTPS or UDP URL"
            );
            Ok(url)
        })
        .collect()
}

pub fn build_input(
    values: &BTreeMap<&str, String>,
    limits: TaskRateLimit,
    trackers: Vec<url::Url>,
    proxy: bool,
    seeding: bool,
    selected_files: Vec<u32>,
) -> anyhow::Result<CreateTaskInput> {
    let value = |key| values.get(key).map(|s| s.trim()).unwrap_or("");
    let optional = |key| {
        let v = value(key);
        if v.is_empty() {
            None
        } else {
            Some(v.to_owned())
        }
    };
    let source = value("source");
    let directory = PathBuf::from(value("directory"));
    anyhow::ensure!(
        directory.is_absolute(),
        "Choose an absolute download directory"
    );
    let mut credentials = TaskCredentials::default();
    let kind = if source.starts_with("magnet:")
        || (source.ends_with(".torrent") && std::path::Path::new(&source).is_absolute())
    {
        let source = if source.starts_with("magnet:") {
            let url =
                url::Url::parse(source).map_err(|_| anyhow::anyhow!("Invalid magnet link"))?;
            anyhow::ensure!(
                url.query_pairs()
                    .any(|(k, v)| k == "xt" && v.starts_with("urn:btih:")),
                "The magnet link needs a BitTorrent info hash"
            );
            BtSource::Magnet(source.to_owned())
        } else {
            let path = PathBuf::from(source);
            anyhow::ensure!(
                path.is_absolute() && path.is_file(),
                "Choose an existing .torrent file"
            );
            BtSource::TorrentFile(path)
        };
        let ratio = optional("ratio")
            .map(|s| s.parse::<f64>())
            .transpose()
            .map_err(|_| anyhow::anyhow!("Enter a valid share ratio"))?;
        anyhow::ensure!(
            ratio.is_none_or(|v| v.is_finite() && v > 0.),
            "Share ratio must be greater than zero"
        );
        TaskKind::Bt(BtTaskConfig {
            source,
            selected_files,
            trackers,
            max_connections: Some(
                value("connections")
                    .parse::<u32>()
                    .ok()
                    .filter(|v| *v > 0)
                    .ok_or_else(|| anyhow::anyhow!("Connections must be a positive integer"))?,
            ),
            share_ratio_limit: ratio,
            enable_seeding: seeding,
            anti_leech: Default::default(),
            ip_filter: Default::default(),
        })
    } else {
        let mut url = url::Url::parse(source)
            .map_err(|_| anyhow::anyhow!("Enter an HTTP, HTTPS, FTP, SFTP or magnet link"))?;
        anyhow::ensure!(url.host_str().is_some(), "The URL needs a host");
        match url.scheme() {
            "http" | "https" => {
                let mut headers = vec![];
                for line in value("headers")
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                {
                    let (name, value) = line
                        .split_once(':')
                        .ok_or_else(|| anyhow::anyhow!("Each request header needs Name: value"))?;
                    anyhow::ensure!(
                        !name.trim().is_empty()
                            && !value.contains('\r')
                            && name
                                .trim()
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric()
                                    || b"!#$%&'*+-.^_`|~".contains(&b)),
                        "Invalid request header"
                    );
                    headers.push(HeaderPair {
                        name: name.trim().into(),
                        value: value.trim().into(),
                    });
                }
                for (key, name) in [
                    ("cookie", "Cookie"),
                    ("referer", "Referer"),
                    ("agent", "User-Agent"),
                ] {
                    if let Some(v) = optional(key) {
                        anyhow::ensure!(
                            !v.contains(['\r', '\n']),
                            "Request fields cannot contain newlines"
                        );
                        headers.push(HeaderPair {
                            name: name.into(),
                            value: v,
                        });
                    }
                }
                let connections = value("connections")
                    .parse::<u16>()
                    .ok()
                    .filter(|v| *v > 0)
                    .ok_or_else(|| anyhow::anyhow!("Connections must be between 1 and 65535"))?;
                TaskKind::Http(HttpTaskConfig {
                    url,
                    method: HttpMethod::Get,
                    headers,
                    max_connections: Some(connections),
                    min_split_size: None,
                    redirect_limit: 10,
                })
            }
            "ftp" | "sftp" => {
                anyhow::ensure!(
                    url.path() != "/" && !url.path().ends_with('/'),
                    "The transfer URL must point to a file"
                );
                let username = optional("username").or_else(|| {
                    (!url.username().is_empty()).then(|| percent_decode_lossy(url.username()))
                });
                credentials.username = username.clone();
                credentials.password =
                    optional("password").or_else(|| url.password().map(percent_decode_lossy));
                credentials.private_key_passphrase = optional("passphrase");
                let _ = url.set_username("");
                let _ = url.set_password(None);
                if url.scheme() == "ftp" {
                    TaskKind::Ftp(FtpTaskConfig {
                        url,
                        username,
                        passive: true,
                        ftps: false,
                    })
                } else {
                    TaskKind::Sftp(SftpTaskConfig {
                        url,
                        username,
                        private_key_path: optional("privatekey").map(PathBuf::from),
                    })
                }
            }
            "ftps" => anyhow::bail!("FTPS is not supported. Use FTP or SFTP."),
            _ => anyhow::bail!("Use an HTTP, HTTPS, FTP, SFTP or magnet link"),
        }
    };
    let mut input = CreateTaskInput {
        kind,
        save_dir: directory,
        file_name: optional("filename").map(|s| sanitize_file_name(&s)),
        limits,
        proxy: if proxy {
            ProxyPolicy::UseGlobal
        } else {
            ProxyPolicy::Direct
        },
        credentials,
    };
    input.isolate_sensitive_headers();
    Ok(input)
}

impl Render for Form {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let creating = matches!(self.kind, FormKind::Create);
        let settings = matches!(self.kind, FormKind::Settings(_));
        let source = self.value("source", cx);
        let bt = source.starts_with("magnet:")
            || (source.ends_with(".torrent") && std::path::Path::new(&source).is_absolute());
        let ftp = source.starts_with("ftp:") || source.starts_with("sftp:");
        let mut body = v_flex().gap_4();
        if creating {
            body = body
                .child(
                    h_flex()
                        .gap_2()
                        .items_end()
                        .child(self.field("source", "new.link", cx))
                        .child(
                            Button::new("torrent")
                                .icon(IconName::FolderOpen)
                                .label(".torrent")
                                .tooltip(t(&self.locale, "new.chooseTorrent"))
                                .disabled(self.busy)
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.pick(false, window, cx)),
                                ),
                        ),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .items_end()
                        .child(self.field("directory", "new.saveTo", cx))
                        .child(
                            Button::new("folder")
                                .label(t(&self.locale, "new.browse"))
                                .disabled(self.busy)
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.pick(true, window, cx)),
                                ),
                        ),
                )
                .child(self.field("filename", "new.fileName", cx));
            if source.starts_with("magnet:") {
                body = body.child(
                    Button::new("preview")
                        .label(t(
                            &self.locale,
                            if self.resolving {
                                "new.resolving"
                            } else {
                                "new.files"
                            },
                        ))
                        .disabled(self.resolving || self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.resolving = true;
                            this.preview_generation += 1;
                            cx.emit(FormEvent::Submit(Command::Preview(
                                this.preview_generation,
                                this.value("source", cx),
                            )));
                            cx.notify();
                        })),
                );
            }
            if let Some(preview) = &self.preview {
                let mut files = v_flex()
                    .id("preview-files")
                    .max_h(px(150.))
                    .overflow_y_scroll()
                    .gap_2();
                for file in &preview.files {
                    let index = file.index;
                    files = files.child(
                        Checkbox::new(("file", index as usize))
                            .label(format!("{} · {}", file.name, bytes(Some(file.size))))
                            .checked(file.selected)
                            .disabled(self.busy)
                            .on_click(cx.listener(move |this, checked, _, cx| {
                                if let Some(p) = &mut this.preview
                                    && let Some(f) = p.files.iter_mut().find(|f| f.index == index)
                                {
                                    f.selected = *checked;
                                }
                                cx.notify();
                            })),
                    );
                }
                body = body.child(files);
            }
            body = body.child(
                Button::new("advanced")
                    .ghost()
                    .icon(if self.advanced {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .label(t(&self.locale, "new.advanced"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.advanced = !this.advanced;
                        cx.notify();
                    })),
            );
        }
        if !creating || self.advanced {
            if creating {
                body = body.child(self.field("connections", "new.maxConnections", cx));
                if !bt && !ftp {
                    body = body
                        .child(self.field("headers", "fields.customHeaders", cx))
                        .child(self.field("cookie", "fields.cookie", cx))
                        .child(
                            h_flex()
                                .gap_3()
                                .child(self.field("referer", "fields.referer", cx))
                                .child(self.field("agent", "fields.userAgent", cx)),
                        );
                } else if ftp {
                    body = body.child(
                        h_flex()
                            .gap_3()
                            .child(self.field("username", "credentials.username", cx))
                            .child(self.field("password", "credentials.password", cx)),
                    );
                    if source.starts_with("sftp:") {
                        body = body
                            .child(self.field("privatekey", "fields.privateKeyPath", cx))
                            .child(self.field("passphrase", "credentials.keyPassphrase", cx));
                    }
                } else {
                    body = body
                        .child(self.field("trackers", "fields.extraTrackers", cx))
                        .child(self.field("ratio", "fields.shareRatio", cx))
                        .child(
                            Switch::new("seeding")
                                .label(t(&self.locale, "fields.seedAfter"))
                                .checked(self.seeding)
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, value, _, cx| {
                                    this.seeding = *value;
                                    cx.notify();
                                })),
                        );
                }
            }
            if settings {
                body = body
                    .child(
                        div()
                            .font_semibold()
                            .child(t(&self.locale, "settings.transfer.title")),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(t(&self.locale, "settings.transfer.copy")),
                    );
            }
            body = body
                .child(
                    h_flex()
                        .gap_3()
                        .child(self.field("down", "detail.downloadLimit", cx))
                        .child(self.field("up", "detail.uploadLimit", cx)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("KB/s"),
                );
            if creating || settings {
                body = body.child(
                    Switch::new("proxy")
                        .label(t(&self.locale, "settings.proxy.useSystem"))
                        .checked(self.proxy)
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, value, _, cx| {
                            this.proxy = *value;
                            cx.notify();
                        })),
                );
            }
            if settings {
                body = body
                    .child(
                        div()
                            .mt_4()
                            .font_semibold()
                            .child(t(&self.locale, "settings.bt.title")),
                    )
                    .child(self.field("trackers", "settings.bt.trackers", cx))
                    .child(
                        h_flex()
                            .gap_3()
                            .child(self.field("allow", "settings.bt.allow", cx))
                            .child(self.field("deny", "settings.bt.deny", cx)),
                    );
            }
        }
        let height = if settings {
            window.viewport_size().height - px(250.)
        } else {
            window.viewport_size().height - px(245.)
        };
        v_flex()
            .w_full()
            .gap_4()
            .child(
                div()
                    .id("form-scroll")
                    .max_h(height.max(px(180.)))
                    .overflow_y_scroll()
                    .pr_2()
                    .child(body),
            )
            .when(!self.error.is_empty(), |view| {
                view.child(
                    div()
                        .p_3()
                        .rounded_lg()
                        .bg(cx.theme().danger.opacity(0.10))
                        .text_color(cx.theme().danger)
                        .text_sm()
                        .child(self.error.clone()),
                )
            })
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .pt_3()
                    .when(!settings, |row| {
                        row.child(
                            Button::new("cancel")
                                .label(t(&self.locale, "common.cancel"))
                                .disabled(self.busy)
                                .on_click(cx.listener(|_, _, _, cx| cx.emit(FormEvent::Cancel))),
                        )
                    })
                    .child(
                        Button::new("submit")
                            .primary()
                            .label(t(
                                &self.locale,
                                if self.busy {
                                    "common.saving"
                                } else if creating {
                                    "new.create"
                                } else {
                                    "common.save"
                                },
                            ))
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                    ),
            )
    }
}
