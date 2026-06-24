use std::path::PathBuf;

use fluxion_core::{
    BtStateSnapshot, CreateTaskInput, MagnetPreview, SettingsSnapshot, TaskDetail, TaskFilter,
    TaskId, TaskRateLimit, TaskState, TaskSummary,
};
use tauri::State;
use uuid::Uuid;

use crate::{AppError, CommandResult, CoreState};

#[tauri::command]
pub async fn create_task(
    state: State<'_, CoreState>,
    input: CreateTaskInput,
) -> CommandResult<TaskId> {
    state.core().create_task(input).await.map_err(Into::into)
}

#[tauri::command]
pub async fn start_task(state: State<'_, CoreState>, task_id: String) -> CommandResult<()> {
    state
        .core()
        .start_task(parse_task_id(&task_id)?)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn pause_task(state: State<'_, CoreState>, task_id: String) -> CommandResult<()> {
    state
        .core()
        .pause_task(parse_task_id(&task_id)?)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn stop_task(state: State<'_, CoreState>, task_id: String) -> CommandResult<()> {
    state
        .core()
        .stop_task(parse_task_id(&task_id)?)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn delete_task(
    state: State<'_, CoreState>,
    task_id: String,
    delete_files: bool,
) -> CommandResult<()> {
    state
        .core()
        .delete_task(parse_task_id(&task_id)?, delete_files)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn list_tasks(
    state: State<'_, CoreState>,
    filter: Option<TaskFilter>,
) -> CommandResult<Vec<TaskSummary>> {
    state
        .core()
        .list_tasks(filter.unwrap_or_default())
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn get_task(
    state: State<'_, CoreState>,
    task_id: String,
) -> CommandResult<Option<TaskDetail>> {
    state
        .core()
        .get_task(parse_task_id(&task_id)?)
        .await
        .map(|task| task.map(|detail| detail.redacted()))
        .map_err(Into::into)
}

#[tauri::command]
pub async fn get_settings(state: State<'_, CoreState>) -> CommandResult<SettingsSnapshot> {
    state.core().get_settings().await.map_err(Into::into)
}

#[tauri::command]
pub async fn update_settings(
    state: State<'_, CoreState>,
    settings: SettingsSnapshot,
) -> CommandResult<()> {
    state
        .core()
        .update_settings(settings)
        .await
        .map_err(Into::into)
}

fn parse_task_id(value: &str) -> CommandResult<TaskId> {
    Uuid::parse_str(value).map_err(Into::into)
}

#[tauri::command]
pub async fn pause_all(state: State<'_, CoreState>) -> CommandResult<()> {
    state.core().pause_all().await.map_err(Into::into)
}

#[tauri::command]
pub async fn start_all(state: State<'_, CoreState>) -> CommandResult<()> {
    state.core().start_all().await.map_err(Into::into)
}

#[tauri::command]
pub async fn clear_finished(state: State<'_, CoreState>, delete_files: bool) -> CommandResult<()> {
    state
        .core()
        .clear_finished(delete_files)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn update_task_limits(
    state: State<'_, CoreState>,
    task_id: String,
    limits: TaskRateLimit,
) -> CommandResult<()> {
    state
        .core()
        .update_task_limits(parse_task_id(&task_id)?, limits)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn get_bt_state(
    state: State<'_, CoreState>,
    task_id: String,
) -> CommandResult<Option<BtStateSnapshot>> {
    state
        .core()
        .get_task_state(parse_task_id(&task_id)?)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn resolve_magnet_preview(
    state: State<'_, CoreState>,
    magnet: String,
) -> CommandResult<MagnetPreview> {
    state
        .core()
        .resolve_magnet_preview(magnet)
        .await
        .map_err(Into::into)
}

/// Resolve the on-disk output path for a completed task, guarding against
/// path traversal (the resolved file must live inside the task's save_dir).
async fn resolve_task_output_path(state: &CoreState, task_id: &str) -> CommandResult<PathBuf> {
    let id = parse_task_id(task_id)?;
    let detail: TaskDetail = state
        .core()
        .get_task(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::invalid_config("task not found"))?;
    if detail.task.state != TaskState::Completed {
        return Err(AppError::invalid_config(
            "task is not completed; file may not exist yet",
        ));
    }
    let file_name = detail.task.file_name.ok_or_else(|| {
        AppError::invalid_config("task has no file name; cannot resolve output path")
    })?;
    let save_dir = detail.task.save_dir;
    let file_path = save_dir.join(&file_name);

    // Path-traversal guard: the canonical file path must remain inside the
    // canonical save_dir. If canonicalization fails (file missing), fall back
    // to a lexical containment check on the components.
    use std::path::Component;
    let contained = match (
        std::fs::canonicalize(&file_path),
        std::fs::canonicalize(&save_dir),
    ) {
        (Ok(file_canon), Ok(dir_canon)) => file_canon.starts_with(&dir_canon),
        _ => {
            let no_parent = file_path
                .components()
                .all(|c| !matches!(c, Component::ParentDir));
            no_parent && file_path.starts_with(&save_dir)
        }
    };
    if !contained {
        return Err(AppError::invalid_config(
            "resolved path escapes save directory",
        ));
    }
    Ok(file_path)
}

#[tauri::command]
pub async fn open_task_file(state: State<'_, CoreState>, task_id: String) -> CommandResult<()> {
    let path = resolve_task_output_path(&state, &task_id).await?;
    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|error| AppError {
        kind: "Unknown".to_string(),
        message: error.to_string(),
    })
}

#[tauri::command]
pub async fn reveal_task_file(state: State<'_, CoreState>, task_id: String) -> CommandResult<()> {
    let path = resolve_task_output_path(&state, &task_id).await?;
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|error| AppError {
        kind: "Unknown".to_string(),
        message: error.to_string(),
    })
}
