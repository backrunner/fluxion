use std::{
    path::PathBuf,
    process::{Command as StdCommand, Stdio},
};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::{Args, Parser, Subcommand, ValueEnum};
use fluxion_cli::ipc::{IpcParams, IpcResponse, default_data_dir, default_socket_path, request};
use fluxion_core::{
    AntiLeechConfig, BtSource, BtTaskConfig, CreateTaskInput, DownloadKind, FtpTaskConfig,
    HeaderPair, HttpMethod, HttpTaskConfig, IpFilterConfig, ProxyPolicy, SettingsSnapshot,
    SftpTaskConfig, TaskCredentials, TaskDetail, TaskFilter, TaskKind, TaskRateLimit, TaskState,
    TaskSummary,
};
use serde::Serialize;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    process::Command as TokioCommand,
};
use url::Url;
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "fluxion")]
struct Cli {
    #[arg(long, env = "FLUXION_SOCKET", global = true, value_name = "PATH")]
    socket: Option<PathBuf>,
    #[arg(long, env = "FLUXION_DATA_DIR", global = true, value_name = "DIR")]
    data_dir: Option<PathBuf>,
    #[arg(long, env = "FLUXION_PID", global = true, value_name = "PATH")]
    pid_file: Option<PathBuf>,
    #[command(subcommand)]
    command: CommandGroup,
}

#[derive(Subcommand)]
enum CommandGroup {
    Daemon(DaemonCommand),
    Task(Box<TaskCommand>),
    Settings(SettingsCommand),
    Events(EventsCommand),
    Diagnostics(DiagnosticsCommand),
    #[command(name = "__run-daemon", hide = true)]
    RunDaemon,
}

#[derive(Args)]
struct DaemonCommand {
    #[command(subcommand)]
    command: DaemonSubcommand,
}

#[derive(Subcommand)]
enum DaemonSubcommand {
    Start,
    Stop,
    Status,
}

#[derive(Args)]
struct TaskCommand {
    #[command(subcommand)]
    command: TaskSubcommand,
}

#[derive(Subcommand)]
enum TaskSubcommand {
    Add(Box<AddTaskCommand>),
    List {
        #[arg(long = "state")]
        states: Vec<CliTaskState>,
        #[arg(long = "kind")]
        kinds: Vec<CliDownloadKind>,
    },
    Get {
        task_id: Uuid,
    },
    Start {
        task_id: Uuid,
    },
    Resume {
        task_id: Uuid,
    },
    Pause {
        task_id: Uuid,
    },
    Stop {
        task_id: Uuid,
    },
    Delete {
        task_id: Uuid,
        #[arg(long)]
        delete_files: bool,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "kebab-case")]
enum CliTaskState {
    Queued,
    Resolving,
    Downloading,
    Paused,
    Stopped,
    Completed,
    Seeding,
    Failed,
    Verifying,
}

impl From<CliTaskState> for TaskState {
    fn from(value: CliTaskState) -> Self {
        match value {
            CliTaskState::Queued => TaskState::Queued,
            CliTaskState::Resolving => TaskState::Resolving,
            CliTaskState::Downloading => TaskState::Downloading,
            CliTaskState::Paused => TaskState::Paused,
            CliTaskState::Stopped => TaskState::Stopped,
            CliTaskState::Completed => TaskState::Completed,
            CliTaskState::Seeding => TaskState::Seeding,
            CliTaskState::Failed => TaskState::Failed,
            CliTaskState::Verifying => TaskState::Verifying,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "kebab-case")]
enum CliDownloadKind {
    Http,
    Bt,
    Ftp,
    Sftp,
}

impl From<CliDownloadKind> for DownloadKind {
    fn from(value: CliDownloadKind) -> Self {
        match value {
            CliDownloadKind::Http => DownloadKind::Http,
            CliDownloadKind::Bt => DownloadKind::Bt,
            CliDownloadKind::Ftp => DownloadKind::Ftp,
            CliDownloadKind::Sftp => DownloadKind::Sftp,
        }
    }
}

#[derive(Args)]
struct AddTaskCommand {
    #[command(subcommand)]
    protocol: AddProtocol,
}

#[derive(Subcommand)]
enum AddProtocol {
    Http(HttpAddTask),
    Ftp(FtpAddTask),
    Sftp(SftpAddTask),
    Torrent(TorrentAddTask),
    Magnet(MagnetAddTask),
}

#[derive(Args, Clone)]
struct CommonAddArgs {
    #[arg(long)]
    save_dir: PathBuf,
    #[arg(long)]
    file_name: Option<String>,
    #[arg(long)]
    download_limit: Option<u64>,
    #[arg(long)]
    upload_limit: Option<u64>,
}

#[derive(Args)]
struct HttpAddTask {
    url: Url,
    #[command(flatten)]
    common: CommonAddArgs,
    #[arg(long)]
    max_connections: Option<u16>,
    #[arg(long)]
    min_split_size: Option<u64>,
    #[arg(long = "header")]
    headers: Vec<String>,
}

#[derive(Args)]
struct FtpAddTask {
    url: Url,
    #[command(flatten)]
    common: CommonAddArgs,
    #[arg(long)]
    username: Option<String>,
    #[arg(long)]
    password: Option<String>,
    /// Use active FTP mode (passive mode is the default).
    #[arg(long = "active")]
    active: bool,
    #[arg(long)]
    ftps: bool,
}

#[derive(Args)]
struct SftpAddTask {
    url: Url,
    #[command(flatten)]
    common: CommonAddArgs,
    #[arg(long)]
    username: Option<String>,
    #[arg(long)]
    password: Option<String>,
    #[arg(long)]
    private_key_path: Option<PathBuf>,
    #[arg(long)]
    private_key_passphrase: Option<String>,
}

#[derive(Args)]
struct TorrentAddTask {
    #[arg(long)]
    file: PathBuf,
    #[command(flatten)]
    common: CommonAddArgs,
    #[arg(long = "selected-file")]
    selected_files: Vec<u32>,
    #[arg(long = "tracker")]
    trackers: Vec<Url>,
    #[arg(long)]
    max_connections: Option<u32>,
    #[arg(long)]
    share_ratio_limit: Option<f64>,
    #[arg(long)]
    no_seeding: bool,
}

#[derive(Args)]
struct MagnetAddTask {
    magnet: String,
    #[command(flatten)]
    common: CommonAddArgs,
    #[arg(long = "selected-file")]
    selected_files: Vec<u32>,
    #[arg(long = "tracker")]
    trackers: Vec<Url>,
    #[arg(long)]
    max_connections: Option<u32>,
    #[arg(long)]
    share_ratio_limit: Option<f64>,
    #[arg(long)]
    no_seeding: bool,
}

#[derive(Args)]
struct SettingsCommand {
    #[command(subcommand)]
    command: SettingsSubcommand,
}

#[derive(Subcommand)]
enum SettingsSubcommand {
    Get,
    Set {
        #[arg(long)]
        download_limit: Option<u64>,
        #[arg(long)]
        clear_download_limit: bool,
        #[arg(long)]
        upload_limit: Option<u64>,
        #[arg(long)]
        clear_upload_limit: bool,
        #[arg(long)]
        use_system_proxy: Option<bool>,
    },
}

#[derive(Args)]
struct EventsCommand {
    #[command(subcommand)]
    command: EventsSubcommand,
}

#[derive(Subcommand)]
enum EventsSubcommand {
    Watch {
        #[arg(long)]
        raw: bool,
    },
}

#[derive(Args)]
struct DiagnosticsCommand {
    #[command(subcommand)]
    command: DiagnosticsSubcommand,
}

#[derive(Subcommand)]
enum DiagnosticsSubcommand {
    Export {
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Debug, Clone)]
struct CliContext {
    socket_path: PathBuf,
    data_dir: PathBuf,
    pid_path: PathBuf,
}

impl CliContext {
    fn from_cli(cli: &Cli) -> Self {
        let data_dir = cli.data_dir.clone().unwrap_or_else(default_data_dir);
        Self {
            socket_path: cli.socket.clone().unwrap_or_else(default_socket_path),
            pid_path: cli
                .pid_file
                .clone()
                .unwrap_or_else(|| data_dir.join("fluxiond.pid")),
            data_dir,
        }
    }
}

#[derive(Debug, Serialize)]
struct DiagnosticExport {
    schema_version: u8,
    generated_at: DateTime<Utc>,
    settings: SettingsSnapshot,
    tasks: Vec<TaskSummary>,
    task_details: Vec<TaskDetail>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let context = CliContext::from_cli(&cli);
    match cli.command {
        CommandGroup::Daemon(command) => handle_daemon(command, &context).await,
        CommandGroup::Task(command) => handle_task(*command, &context).await,
        CommandGroup::Settings(command) => handle_settings(command, &context).await,
        CommandGroup::Events(command) => handle_events(command, &context).await,
        CommandGroup::Diagnostics(command) => handle_diagnostics(command, &context).await,
        CommandGroup::RunDaemon => fluxion_cli::daemon_runtime::run().await,
    }
}

async fn handle_daemon(command: DaemonCommand, context: &CliContext) -> Result<()> {
    match command.command {
        DaemonSubcommand::Start => {
            let socket = context.socket_path.clone();
            if request::<String>(socket.clone(), "daemon.status", IpcParams::Empty)
                .await
                .is_ok()
            {
                println!("running");
                return Ok(());
            }
            // The socket check above is the source of truth: if IPC is
            // unreachable the daemon is not serving, so any pid file is stale
            // (possibly a reused pid). Clean up and start fresh.
            let pid_path = context.pid_path.clone();
            if let Some(pid) = read_pid(&pid_path).await?
                && pid_is_alive(pid).await
            {
                eprintln!(
                    "warning: pid file {} points to live process {pid} but IPC is unavailable; assuming stale (pid reuse) and starting a new daemon",
                    pid_path.display()
                );
            }
            let _ = tokio::fs::remove_file(&pid_path).await;
            let _ = tokio::fs::remove_file(&socket).await;
            let exe = std::env::current_exe()?;
            let daemon = exe.with_file_name("fluxiond");
            let mut command = if daemon.exists() {
                StdCommand::new(daemon)
            } else {
                let mut command = StdCommand::new(exe);
                command.arg("__run-daemon");
                command
            };
            detach_daemon_command(&mut command);
            command
                .env("FLUXION_SOCKET", &context.socket_path)
                .env("FLUXION_DATA_DIR", &context.data_dir)
                .env("FLUXION_PID", &context.pid_path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .context("start fluxiond")?;
            wait_for_daemon(socket).await?;
            println!("started fluxiond");
        }
        DaemonSubcommand::Stop => {
            let socket = context.socket_path.clone();
            let status: String = request(socket.clone(), "daemon.stop", IpcParams::Empty).await?;
            wait_for_socket_removal(socket).await?;
            println!("{status}");
        }
        DaemonSubcommand::Status => {
            let socket = context.socket_path.clone();
            match request::<String>(socket, "daemon.status", IpcParams::Empty).await {
                Ok(status) => println!("{status}"),
                Err(error) => {
                    // Socket connectivity is authoritative; the pid is only
                    // informational (it may have been reused by another process).
                    let pid_path = context.pid_path.clone();
                    match read_pid(&pid_path).await? {
                        Some(pid) if pid_is_alive(pid).await => {
                            println!(
                                "not running (IPC unavailable: {error}; pid file has live pid {pid}, possibly reused)"
                            )
                        }
                        Some(pid) => println!("not running (stale pid {pid})"),
                        None => println!("not running"),
                    }
                }
            }
        }
    }
    Ok(())
}

async fn read_pid(path: &std::path::Path) -> Result<Option<u32>> {
    let content = match tokio::fs::read_to_string(path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    Ok(content.trim().parse::<u32>().ok())
}

fn detach_daemon_command(command: &mut StdCommand) {
    #[cfg(unix)]
    {
        command.process_group(0);
    }
}

async fn pid_is_alive(pid: u32) -> bool {
    TokioCommand::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .await
        .map(|status| status.success())
        .unwrap_or(false)
}

async fn wait_for_daemon(socket: PathBuf) -> Result<()> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if request::<String>(socket.clone(), "daemon.status", IpcParams::Empty)
            .await
            .is_ok()
        {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("fluxiond did not become ready");
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

async fn wait_for_socket_removal(socket: PathBuf) -> Result<()> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
    while socket.exists() && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Ok(())
}

async fn handle_task(command: TaskCommand, context: &CliContext) -> Result<()> {
    match command.command {
        TaskSubcommand::Add(input) => {
            let create = build_create_task(*input)?;
            let id: Uuid = request(
                context.socket_path.clone(),
                "task.create",
                IpcParams::CreateTask(Box::new(create)),
            )
            .await?;
            println!("{id}");
        }
        TaskSubcommand::List { states, kinds } => {
            let tasks: Vec<TaskSummary> = request(
                context.socket_path.clone(),
                "task.list",
                IpcParams::TaskFilter(build_task_filter(states, kinds)),
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&tasks)?);
        }
        TaskSubcommand::Get { task_id } => {
            let task: Option<TaskDetail> = request(
                context.socket_path.clone(),
                "task.get",
                IpcParams::TaskId { task_id },
            )
            .await?;
            let redacted = task.map(|detail| detail.redacted());
            println!("{}", serde_json::to_string_pretty(&redacted)?);
        }
        TaskSubcommand::Start { task_id } | TaskSubcommand::Resume { task_id } => {
            let _: serde_json::Value = request(
                context.socket_path.clone(),
                "task.start",
                IpcParams::TaskId { task_id },
            )
            .await?;
        }
        TaskSubcommand::Pause { task_id } => {
            let _: serde_json::Value = request(
                context.socket_path.clone(),
                "task.pause",
                IpcParams::TaskId { task_id },
            )
            .await?;
        }
        TaskSubcommand::Stop { task_id } => {
            let _: serde_json::Value = request(
                context.socket_path.clone(),
                "task.stop",
                IpcParams::TaskId { task_id },
            )
            .await?;
        }
        TaskSubcommand::Delete {
            task_id,
            delete_files,
        } => {
            let _: serde_json::Value = request(
                context.socket_path.clone(),
                "task.delete",
                IpcParams::DeleteTask {
                    task_id,
                    delete_files,
                },
            )
            .await?;
        }
    }
    Ok(())
}

fn build_task_filter(states: Vec<CliTaskState>, kinds: Vec<CliDownloadKind>) -> TaskFilter {
    TaskFilter {
        states: states.into_iter().map(TaskState::from).collect(),
        kinds: kinds.into_iter().map(DownloadKind::from).collect(),
    }
}

async fn handle_settings(command: SettingsCommand, context: &CliContext) -> Result<()> {
    match command.command {
        SettingsSubcommand::Get => {
            let settings: SettingsSnapshot = request(
                context.socket_path.clone(),
                "settings.get",
                IpcParams::Empty,
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&settings)?);
        }
        SettingsSubcommand::Set {
            download_limit,
            clear_download_limit,
            upload_limit,
            clear_upload_limit,
            use_system_proxy,
        } => {
            let mut settings: SettingsSnapshot = request(
                context.socket_path.clone(),
                "settings.get",
                IpcParams::Empty,
            )
            .await?;
            apply_settings_update(
                &mut settings,
                download_limit,
                clear_download_limit,
                upload_limit,
                clear_upload_limit,
                use_system_proxy,
            )?;
            let _: serde_json::Value = request(
                context.socket_path.clone(),
                "settings.update",
                IpcParams::Settings(Box::new(settings)),
            )
            .await?;
        }
    }
    Ok(())
}

fn apply_settings_update(
    settings: &mut SettingsSnapshot,
    download_limit: Option<u64>,
    clear_download_limit: bool,
    upload_limit: Option<u64>,
    clear_upload_limit: bool,
    use_system_proxy: Option<bool>,
) -> Result<()> {
    if download_limit.is_some() && clear_download_limit {
        anyhow::bail!("--download-limit and --clear-download-limit cannot be used together");
    }
    if upload_limit.is_some() && clear_upload_limit {
        anyhow::bail!("--upload-limit and --clear-upload-limit cannot be used together");
    }
    if clear_download_limit {
        settings.download_limit = None;
    } else if download_limit.is_some() {
        settings.download_limit = download_limit;
    }
    if clear_upload_limit {
        settings.upload_limit = None;
    } else if upload_limit.is_some() {
        settings.upload_limit = upload_limit;
    }
    if let Some(use_system_proxy) = use_system_proxy {
        settings.use_system_proxy = use_system_proxy;
    }
    Ok(())
}

async fn handle_events(command: EventsCommand, context: &CliContext) -> Result<()> {
    match command.command {
        EventsSubcommand::Watch { raw } => {
            let mut stream = UnixStream::connect(&context.socket_path).await?;
            let request = fluxion_cli::ipc::IpcRequest {
                id: 1,
                method: "events.subscribe".to_string(),
                params: serde_json::to_value(IpcParams::Empty)?,
            };
            stream
                .write_all(format!("{}\n", serde_json::to_string(&request)?).as_bytes())
                .await?;
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            while reader.read_line(&mut line).await? > 0 {
                print_event_line(&line, raw)?;
                line.clear();
            }
        }
    }
    Ok(())
}

fn print_event_line(line: &str, raw: bool) -> Result<()> {
    if raw {
        print!("{line}");
        return Ok(());
    }
    let response: IpcResponse = serde_json::from_str(line)?;
    if let Some(error) = response.error {
        eprintln!("error: {error}");
    } else if let Some(result) = response.result {
        println!("{}", serde_json::to_string_pretty(&result)?);
    }
    Ok(())
}

async fn handle_diagnostics(command: DiagnosticsCommand, context: &CliContext) -> Result<()> {
    match command.command {
        DiagnosticsSubcommand::Export { output } => {
            let export = build_diagnostic_export(context).await?;
            let payload = serde_json::to_string_pretty(&export)?;
            if let Some(output) = output {
                if let Some(parent) = output.parent()
                    && !parent.as_os_str().is_empty()
                {
                    tokio::fs::create_dir_all(parent).await?;
                }
                tokio::fs::write(&output, payload).await?;
                println!("{}", output.display());
            } else {
                println!("{payload}");
            }
        }
    }
    Ok(())
}

async fn build_diagnostic_export(context: &CliContext) -> Result<DiagnosticExport> {
    let settings: SettingsSnapshot = request(
        context.socket_path.clone(),
        "settings.get",
        IpcParams::Empty,
    )
    .await?;
    let tasks: Vec<TaskSummary> = request(
        context.socket_path.clone(),
        "task.list",
        IpcParams::TaskFilter(TaskFilter::default()),
    )
    .await?;
    let mut task_details = Vec::with_capacity(tasks.len());
    for task in &tasks {
        let detail: Option<TaskDetail> = request(
            context.socket_path.clone(),
            "task.get",
            IpcParams::TaskId { task_id: task.id },
        )
        .await?;
        if let Some(detail) = detail {
            task_details.push(detail.redacted());
        }
    }
    Ok(DiagnosticExport {
        schema_version: 1,
        generated_at: Utc::now(),
        settings,
        tasks,
        task_details,
    })
}

const MIN_SPLIT_SIZE_FLOOR: u64 = 64 * 1024;

fn build_create_task(input: AddTaskCommand) -> Result<CreateTaskInput> {
    match input.protocol {
        AddProtocol::Http(input) => build_http_task(input),
        AddProtocol::Ftp(input) => build_ftp_task(input),
        AddProtocol::Sftp(input) => build_sftp_task(input),
        AddProtocol::Torrent(input) => build_bt_task(
            input.common,
            BtSource::TorrentFile(input.file),
            input.selected_files,
            input.trackers,
            input.max_connections,
            input.share_ratio_limit,
            !input.no_seeding,
        ),
        AddProtocol::Magnet(input) => build_bt_task(
            input.common,
            BtSource::Magnet(input.magnet),
            input.selected_files,
            input.trackers,
            input.max_connections,
            input.share_ratio_limit,
            !input.no_seeding,
        ),
    }
}

fn build_http_task(input: HttpAddTask) -> Result<CreateTaskInput> {
    ensure_scheme(&input.url, &["http", "https"])?;
    if let Some(min_split_size) = input.min_split_size
        && min_split_size < MIN_SPLIT_SIZE_FLOOR
    {
        anyhow::bail!("--min-split-size must be at least {MIN_SPLIT_SIZE_FLOOR} bytes (64 KiB)");
    }
    let headers = parse_headers(&input.headers)?;
    let credentials = TaskCredentials {
        headers: headers
            .iter()
            .filter(|header| fluxion_core::is_sensitive_header(&header.name))
            .cloned()
            .collect(),
        ..Default::default()
    };
    let public_headers = headers
        .into_iter()
        .filter(|header| !fluxion_core::is_sensitive_header(&header.name))
        .collect();
    Ok(create_input(
        input.common,
        TaskKind::Http(HttpTaskConfig {
            url: input.url,
            method: HttpMethod::Get,
            headers: public_headers,
            max_connections: Some(
                input
                    .max_connections
                    .unwrap_or(HttpTaskConfig::DEFAULT_MAX_CONNECTIONS),
            ),
            min_split_size: input.min_split_size,
            redirect_limit: HttpTaskConfig::DEFAULT_REDIRECT_LIMIT,
        }),
        credentials,
    ))
}

fn build_ftp_task(input: FtpAddTask) -> Result<CreateTaskInput> {
    ensure_scheme(&input.url, &["ftp", "ftps"])?;
    let credentials = TaskCredentials {
        username: input.username.clone(),
        password: input.password,
        ..Default::default()
    };
    Ok(create_input(
        input.common,
        TaskKind::Ftp(FtpTaskConfig {
            ftps: input.ftps || input.url.scheme() == "ftps",
            url: input.url,
            username: input.username,
            passive: !input.active,
        }),
        credentials,
    ))
}

fn build_sftp_task(input: SftpAddTask) -> Result<CreateTaskInput> {
    ensure_scheme(&input.url, &["sftp"])?;
    let credentials = TaskCredentials {
        username: input.username.clone(),
        password: input.password,
        private_key_passphrase: input.private_key_passphrase,
        ..Default::default()
    };
    Ok(create_input(
        input.common,
        TaskKind::Sftp(SftpTaskConfig {
            url: input.url,
            username: input.username,
            private_key_path: input.private_key_path,
        }),
        credentials,
    ))
}

fn build_bt_task(
    common: CommonAddArgs,
    source: BtSource,
    selected_files: Vec<u32>,
    trackers: Vec<Url>,
    max_connections: Option<u32>,
    share_ratio_limit: Option<f64>,
    enable_seeding: bool,
) -> Result<CreateTaskInput> {
    Ok(create_input(
        common,
        TaskKind::Bt(BtTaskConfig {
            source,
            selected_files,
            trackers,
            max_connections,
            share_ratio_limit,
            enable_seeding,
            anti_leech: AntiLeechConfig::default(),
            ip_filter: IpFilterConfig::default(),
        }),
        TaskCredentials::default(),
    ))
}

fn create_input(
    common: CommonAddArgs,
    kind: TaskKind,
    credentials: TaskCredentials,
) -> CreateTaskInput {
    CreateTaskInput {
        kind,
        save_dir: common.save_dir,
        file_name: common.file_name,
        limits: TaskRateLimit {
            download_bytes_per_second: common.download_limit,
            upload_bytes_per_second: common.upload_limit,
        },
        proxy: ProxyPolicy::UseGlobal,
        credentials,
    }
}

fn ensure_scheme(url: &Url, allowed: &[&str]) -> Result<()> {
    if allowed.iter().any(|scheme| url.scheme() == *scheme) {
        Ok(())
    } else {
        anyhow::bail!("unsupported URL scheme {}", url.scheme())
    }
}

fn parse_headers(values: &[String]) -> Result<Vec<HeaderPair>> {
    values
        .iter()
        .map(|value| {
            let (name, header_value) = value
                .split_once(':')
                .context("headers must use 'Name: value' format")?;
            Ok(HeaderPair {
                name: name.trim().to_string(),
                value: header_value.trim().to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_filter_maps_cli_values() {
        let filter = build_task_filter(
            vec![CliTaskState::Downloading, CliTaskState::Paused],
            vec![CliDownloadKind::Http, CliDownloadKind::Sftp],
        );

        assert_eq!(
            filter.states,
            vec![TaskState::Downloading, TaskState::Paused]
        );
        assert_eq!(filter.kinds, vec![DownloadKind::Http, DownloadKind::Sftp]);
    }

    #[test]
    fn settings_update_can_clear_limits() {
        let mut settings = SettingsSnapshot {
            download_limit: Some(1024),
            upload_limit: Some(2048),
            use_system_proxy: true,
            bt_trackers: Vec::new(),
            bt_ip_allow: Vec::new(),
            bt_ip_deny: Vec::new(),
        };

        apply_settings_update(&mut settings, None, true, Some(4096), false, Some(false)).unwrap();

        assert_eq!(settings.download_limit, None);
        assert_eq!(settings.upload_limit, Some(4096));
        assert!(!settings.use_system_proxy);
    }

    #[test]
    fn settings_update_rejects_set_and_clear_same_limit() {
        let mut settings = SettingsSnapshot::default();

        let error = apply_settings_update(&mut settings, Some(1024), true, None, false, None)
            .expect_err("conflicting download limit flags must be rejected");

        assert!(
            error
                .to_string()
                .contains("--download-limit and --clear-download-limit")
        );
    }

    #[test]
    fn http_create_task_splits_sensitive_headers() {
        let input = HttpAddTask {
            url: Url::parse("https://example.com/file.bin").unwrap(),
            common: CommonAddArgs {
                save_dir: PathBuf::from("/tmp"),
                file_name: None,
                download_limit: None,
                upload_limit: None,
            },
            max_connections: None,
            min_split_size: None,
            headers: vec![
                "Cookie: session=secret".to_string(),
                "User-Agent: Fluxion".to_string(),
            ],
        };

        let create = build_http_task(input).unwrap();

        assert_eq!(create.credentials.headers.len(), 1);
        assert_eq!(create.credentials.headers[0].name, "Cookie");
        match create.kind {
            TaskKind::Http(config) => {
                assert_eq!(config.headers.len(), 1);
                assert_eq!(config.headers[0].name, "User-Agent");
            }
            other => panic!("expected HTTP task, got {other:?}"),
        }
    }
}
