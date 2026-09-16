use fluxion_core::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashMap},
    path::Path,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub locale: String,
    pub appearance: String,
    pub trash: BTreeSet<TaskId>,
    pub detail_open: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            locale: "en".into(),
            appearance: "system".into(),
            trash: BTreeSet::new(),
            detail_open: true,
        }
    }
}
impl Preferences {
    pub async fn load_with_legacy(data: &Path) -> Self {
        if data.join("native-ui.json").exists() || std::env::var_os("FLUXION_DATA_DIR").is_some() {
            return Self::load(data);
        }
        let Some(home) = std::env::var_os("HOME") else {
            return Self::default();
        };
        let root = std::path::PathBuf::from(home)
            .join("Library/WebKit/top.backrunner.fluxion/WebsiteData");
        let mut pending = vec![(root, 0)];
        let mut result = Self::default();
        while let Some((directory, depth)) = pending.pop() {
            if depth > 7 {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_dir() {
                    pending.push((entry.path(), depth + 1));
                } else if kind.is_file() && entry.file_name() == "localstorage.sqlite3" {
                    if let Ok(values) = read_legacy_preferences(&entry.path()).await {
                        for (key, value) in values {
                            result.import_legacy(&key, &value);
                        }
                    }
                }
            }
        }
        // Keep WebKit data intact. Only copy the three known, non-secret UI preferences.
        let _ = result.save(data).await;
        result
    }
    fn import_legacy(&mut self, key: &str, value: &str) {
        match key {
            "fluxion.locale" if ["en", "zh-CN", "ja", "ko"].contains(&value) => {
                self.locale = value.into()
            }
            "fluxion.theme" if ["light", "dark", "system"].contains(&value) => {
                self.appearance = value.into()
            }
            "fluxion.trashTaskIds.v1" => {
                if let Ok(ids) = serde_json::from_str::<Vec<TaskId>>(value) {
                    self.trash.extend(ids);
                }
            }
            _ => (),
        }
    }
    pub fn load(data: &Path) -> Self {
        std::fs::read(data.join("native-ui.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }
    pub async fn save(&self, data: &Path) -> anyhow::Result<()> {
        tokio::fs::create_dir_all(data).await?;
        let temp = data.join("native-ui.json.tmp");
        tokio::fs::write(&temp, serde_json::to_vec_pretty(self)?).await?;
        tokio::fs::rename(temp, data.join("native-ui.json")).await?;
        Ok(())
    }
}

async fn read_legacy_preferences(path: &Path) -> anyhow::Result<Vec<(String, String)>> {
    use sqlx::{Connection, Row};
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(path)
        .read_only(true);
    let mut connection = sqlx::SqliteConnection::connect_with(&options).await?;
    let rows=sqlx::query("SELECT key,value FROM ItemTable WHERE key IN ('fluxion.locale','fluxion.theme','fluxion.trashTaskIds.v1') AND length(value) < 1048576").fetch_all(&mut connection).await?;
    rows.into_iter()
        .map(|row| {
            let key: String = row.try_get("key")?;
            let bytes: Vec<u8> = row.try_get("value")?;
            let units = bytes
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>();
            Ok((key, String::from_utf16(&units)?))
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Active,
    Completed,
    Failed,
    Paused,
    Trash,
}
impl Filter {
    pub fn matches(self, task: &TaskSummary, trash: &BTreeSet<TaskId>) -> bool {
        if self == Self::Trash {
            return trash.contains(&task.id);
        }
        if trash.contains(&task.id) {
            return false;
        }
        match self {
            Self::All => true,
            Self::Active => active(&task.state) || task.state == TaskState::Queued,
            Self::Completed => task.state == TaskState::Completed,
            Self::Failed => task.state == TaskState::Failed,
            Self::Paused => matches!(task.state, TaskState::Paused | TaskState::Stopped),
            Self::Trash => false,
        }
    }
}
pub fn active(s: &TaskState) -> bool {
    matches!(
        s,
        TaskState::Downloading | TaskState::Resolving | TaskState::Verifying | TaskState::Seeding
    )
}
pub fn can_start(s: &TaskState) -> bool {
    matches!(
        s,
        TaskState::Queued | TaskState::Paused | TaskState::Stopped | TaskState::Failed
    )
}
pub fn can_stop(s: &TaskState) -> bool {
    active(s) || matches!(s, TaskState::Queued | TaskState::Paused)
}
pub fn bytes(value: Option<u64>) -> String {
    let Some(value) = value else {
        return "—".into();
    };
    let mut amount = value as f64;
    let mut unit = 0;
    while amount >= 1024. && unit < 4 {
        amount /= 1024.;
        unit += 1;
    }
    if unit == 0 {
        format!("{value} B")
    } else {
        format!("{amount:.1} {}", ["B", "KB", "MB", "GB", "TB"][unit])
    }
}
pub fn speed(value: u64) -> String {
    format!("{}/s", bytes(Some(value)))
}
pub fn percent(done: u64, total: Option<u64>) -> f32 {
    total
        .filter(|n| *n > 0)
        .map(|n| (done as f64 / n as f64).clamp(0., 1.) as f32)
        .unwrap_or(0.)
}
pub fn eta(done: u64, total: Option<u64>, speed: u64) -> String {
    let Some(total) = total.filter(|_| speed > 0) else {
        return "—".into();
    };
    let seconds = total.saturating_sub(done).div_ceil(speed);
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h {}m", seconds / 3600, seconds % 3600 / 60)
    }
}

#[derive(Default)]
pub struct TaskStore {
    pub tasks: Vec<TaskSummary>,
    pub speeds: HashMap<TaskId, (u64, u64)>,
}
impl TaskStore {
    pub fn apply(&mut self, event: &CoreEvent) {
        match event {
            CoreEvent::TaskCreated(task) => {
                if let Some(old) = self.tasks.iter_mut().find(|t| t.id == task.id) {
                    *old = task.clone();
                } else {
                    self.tasks.insert(0, task.clone());
                }
            }
            CoreEvent::TaskSpeed(s) => {
                if self
                    .tasks
                    .iter()
                    .any(|t| t.id == s.task_id && active(&t.state))
                {
                    self.speeds.insert(
                        s.task_id,
                        (s.download_bytes_per_second, s.upload_bytes_per_second),
                    );
                }
            }
            CoreEvent::TaskStateChanged { task_id, state } => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == *task_id) {
                    t.state = state.clone();
                    if active(state) || *state == TaskState::Queued {
                        t.error = None;
                    }
                }
                if !active(state) {
                    self.speeds.remove(task_id);
                }
            }
            CoreEvent::TaskProgress(p) => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == p.task_id) {
                    t.downloaded_bytes = p.downloaded_bytes;
                    t.uploaded_bytes = p.uploaded_bytes;
                    t.total_bytes = p.total_bytes.or(t.total_bytes);
                }
            }
            CoreEvent::TaskError(e) => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == e.task_id) {
                    t.state = TaskState::Failed;
                    t.error = Some(e.message.clone());
                }
                self.speeds.remove(&e.task_id);
            }
            CoreEvent::TaskCompleted { task_id, .. } => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == *task_id) {
                    t.state = TaskState::Completed;
                    t.downloaded_bytes = t.total_bytes.unwrap_or(t.downloaded_bytes);
                }
                self.speeds.remove(task_id);
            }
            CoreEvent::SettingsChanged(_) => (),
        }
    }
}
