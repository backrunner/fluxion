mod backend;
mod form;
mod i18n;
mod model;
#[cfg(test)]
mod tests;
mod ui;
mod updater;

use gpui::*;
use gpui_component::{Root, Theme, ThemeMode};
use rust_embed::RustEmbed;
use std::borrow::Cow;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Assets;

struct Shutdown {
    commands: async_channel::Sender<backend::Command>,
    stopped: async_channel::Receiver<()>,
    pending: bool,
    finished: bool,
}
impl Global for Shutdown {}

pub fn request_shutdown(cx: &mut App) {
    let shutdown = cx.global_mut::<Shutdown>();
    if shutdown.pending {
        return;
    }
    shutdown.pending = true;
    shutdown.commands.close();
    let stopped = shutdown.stopped.clone();
    cx.spawn(async move |cx| {
        let _ = stopped.recv().await;
        let _ = cx.update(|cx| {
            cx.global_mut::<Shutdown>().finished = true;
            cx.quit();
        });
    })
    .detach();
}
impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        Ok(Self::get(path).map(|file| file.data))
    }
    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(Self::iter()
            .filter(|p| p.starts_with(path))
            .map(|p| p.to_string().into())
            .collect())
    }
}
actions!(
    fluxion,
    [
        Quit,
        NewDownload,
        Search,
        ToggleDetail,
        Refresh,
        Settings,
        SelectAll,
        Remove,
        NextTask,
        PreviousTask,
        ToggleTask
    ]
);

fn main() -> anyhow::Result<()> {
    let data = backend::data_dir();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let preferences = runtime.block_on(model::Preferences::load_with_legacy(&data));
    let backend = backend::Backend::start(&runtime, data);
    let commands = backend.commands.clone();
    let stopped = backend.stopped.clone();
    Application::new().with_assets(Assets).run(move |cx| {
        gpui_component::init(cx);
        apply_theme(&preferences.appearance, None, cx);
        cx.set_global(Shutdown {
            commands: backend.commands.clone(),
            stopped: backend.stopped.clone(),
            pending: false,
            finished: false,
        });
        cx.on_action(|_: &Quit, cx| request_shutdown(cx));
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-n", NewDownload, None),
            KeyBinding::new("cmd-f", Search, None),
            KeyBinding::new("cmd-r", Refresh, None),
            KeyBinding::new("cmd-,", Settings, None),
            KeyBinding::new("cmd-i", ToggleDetail, None),
            KeyBinding::new("cmd-a", SelectAll, Some("Downloads")),
            KeyBinding::new("backspace", Remove, Some("Downloads")),
            KeyBinding::new("down", NextTask, Some("Downloads")),
            KeyBinding::new("up", PreviousTask, Some("Downloads")),
            KeyBinding::new("space", ToggleTask, Some("Downloads")),
        ]);
        cx.set_menus(vec![
            Menu {
                name: "Fluxion".into(),
                items: vec![
                    MenuItem::action("Settings…", Settings),
                    MenuItem::separator(),
                    MenuItem::action("Quit Fluxion", Quit),
                ],
            },
            Menu {
                name: "File".into(),
                items: vec![
                    MenuItem::action("New Download…", NewDownload),
                    MenuItem::action("Refresh", Refresh),
                ],
            },
            Menu {
                name: "View".into(),
                items: vec![
                    MenuItem::action("Search", Search),
                    MenuItem::action("Toggle Details", ToggleDetail),
                ],
            },
        ]);
        let bounds = Bounds::centered(None, size(px(1180.), px(760.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(760.), px(520.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Fluxion".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(18.), px(18.))),
                }),
                app_id: Some("top.backrunner.fluxion".into()),
                ..Default::default()
            },
            move |window, cx| {
                let view = cx.new(|cx| ui::Workspace::new(backend, preferences, window, cx));
                window.on_window_should_close(cx, |_, cx| {
                    if cx.global::<Shutdown>().finished {
                        true
                    } else {
                        request_shutdown(cx);
                        false
                    }
                });
                let root = cx.new(|cx| Root::new(view.clone(), window, cx));
                view.update(cx, |view, cx| view.observe_root(&root, cx));
                root
            },
        )
        .expect("open Fluxion window");
        cx.activate(true);
    });
    commands.close();
    let _ = runtime.block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(15), stopped.recv()).await
    });
    // Give Core cancellation, writer flushes and SQLite persistence time to finish.
    runtime.shutdown_timeout(std::time::Duration::from_secs(15));
    Ok(())
}

pub fn apply_theme(appearance: &str, window: Option<&mut Window>, cx: &mut App) {
    let mode = match appearance {
        "dark" => ThemeMode::Dark,
        "light" => ThemeMode::Light,
        _ => cx.window_appearance().into(),
    };
    Theme::change(mode, window, cx);
    let dark = Theme::global(cx).is_dark();
    let theme = Theme::global_mut(cx);
    theme.font_size = px(16.);
    theme.radius = px(8.);
    theme.radius_lg = px(12.);
    theme.shadow = false;
    theme.primary = rgb(if dark { 0xf5a17c } else { 0xb74e2d }).into();
    theme.primary_hover = rgb(if dark { 0xffb597 } else { 0xa44021 }).into();
    theme.primary_active = rgb(0xad4725).into();
    theme.primary_foreground = rgb(if dark { 0x21140e } else { 0xffffff }).into();
    theme.danger = rgb(if dark { 0xf08a84 } else { 0xb63232 }).into();
    theme.ring = theme.primary;
    theme.caret = theme.primary;
    theme.background = rgb(if dark { 0x1c1f25 } else { 0xffffff }).into();
    theme.foreground = rgb(if dark { 0xf0f2f5 } else { 0x242933 }).into();
    theme.muted_foreground = rgb(if dark { 0x9da5b3 } else { 0x697383 }).into();
    theme.border = rgb(if dark { 0x30353e } else { 0xe3e7ed }).into();
    theme.input = theme.border;
    theme.secondary = rgb(if dark { 0x272b33 } else { 0xf3f5f8 }).into();
    theme.secondary_hover = rgb(if dark { 0x333944 } else { 0xe8ecf2 }).into();
    theme.secondary_foreground = theme.foreground;
    theme.sidebar = rgb(if dark { 0x13161c } else { 0xeff2f6 }).into();
    theme.popover = theme.background;
    theme.popover_foreground = theme.foreground;
    theme.accent = theme.secondary_hover;
    theme.accent_foreground = theme.foreground;
    theme.list = theme.background;
    theme.list_hover = theme.secondary;
}
