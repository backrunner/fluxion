use std::sync::Arc;

use fluxion_core::{CoreEvent, FluxionCore, FluxionError};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

pub mod commands;

#[derive(Clone)]
pub struct CoreState {
    core: Arc<FluxionCore>,
}

impl CoreState {
    pub fn new(core: Arc<FluxionCore>) -> Self {
        Self { core }
    }

    pub fn core(&self) -> Arc<FluxionCore> {
        self.core.clone()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AppError {
    pub message: String,
    pub kind: String,
}

impl From<FluxionError> for AppError {
    fn from(error: FluxionError) -> Self {
        Self {
            kind: error.kind().to_string(),
            message: error.to_string(),
        }
    }
}

impl From<uuid::Error> for AppError {
    fn from(error: uuid::Error) -> Self {
        Self {
            kind: "InvalidConfig".to_string(),
            message: error.to_string(),
        }
    }
}

impl AppError {
    pub fn invalid_config(message: impl Into<String>) -> Self {
        Self {
            kind: "InvalidConfig".to_string(),
            message: message.into(),
        }
    }
}

pub type CommandResult<T> = std::result::Result<T, AppError>;

pub fn register_events<R: Runtime>(app: &AppHandle<R>) {
    let core = app.state::<CoreState>().core();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut events = core.subscribe_events();
        loop {
            match events.recv().await {
                Ok(event) => emit_event(&app, event),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

fn emit_event<R: Runtime>(app: &AppHandle<R>, event: CoreEvent) {
    let _ = app.emit("fluxion://event", event);
}

#[macro_export]
macro_rules! generate_handler {
    () => {
        tauri::generate_handler![
            $crate::commands::create_task,
            $crate::commands::start_task,
            $crate::commands::pause_task,
            $crate::commands::stop_task,
            $crate::commands::delete_task,
            $crate::commands::list_tasks,
            $crate::commands::get_task,
            $crate::commands::get_settings,
            $crate::commands::update_settings,
            $crate::commands::pause_all,
            $crate::commands::start_all,
            $crate::commands::clear_finished,
            $crate::commands::update_task_limits,
            $crate::commands::get_bt_state,
            $crate::commands::resolve_magnet_preview,
            $crate::commands::open_task_file,
            $crate::commands::reveal_task_file
        ]
    };
}
