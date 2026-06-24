export type DownloadKind = 'Http' | 'Bt' | 'Ftp' | 'Sftp';

export type TaskState =
  | 'Queued'
  | 'Resolving'
  | 'Downloading'
  | 'Paused'
  | 'Stopped'
  | 'Completed'
  | 'Seeding'
  | 'Failed'
  | 'Verifying';

export interface TaskRateLimit {
  download_bytes_per_second?: number | null;
  upload_bytes_per_second?: number | null;
}

export interface TaskSummary {
  id: string;
  kind: DownloadKind;
  file_name?: string | null;
  state: TaskState;
  total_bytes?: number | null;
  downloaded_bytes: number;
  uploaded_bytes: number;
  error?: string | null;
  created_at: string;
  updated_at: string;
}

export interface HeaderPair {
  name: string;
  value: string;
}

export interface HttpTaskConfig {
  url: string;
  method: 'Get';
  headers: HeaderPair[];
  max_connections?: number | null;
  min_split_size?: number | null;
  redirect_limit: number;
}

export type BtSource =
  | { TorrentFile: string }
  | { Magnet: string };

export interface BtTaskConfig {
  source: BtSource;
  selected_files: number[];
  trackers: string[];
  max_connections?: number | null;
  share_ratio_limit?: number | null;
  enable_seeding: boolean;
  anti_leech: unknown;
  ip_filter: unknown;
}

export interface FtpTaskConfig {
  url: string;
  username?: string | null;
  passive: boolean;
  ftps: boolean;
}

export interface SftpTaskConfig {
  url: string;
  username?: string | null;
  private_key_path?: string | null;
}

export type TaskKind =
  | { Http: HttpTaskConfig }
  | { Bt: BtTaskConfig }
  | { Ftp: FtpTaskConfig }
  | { Sftp: SftpTaskConfig };

export interface CreateTaskInput {
  kind: TaskKind;
  save_dir: string;
  file_name?: string | null;
  limits: TaskRateLimit;
  proxy: 'UseGlobal' | 'System' | 'Direct';
  credentials: {
    headers: HeaderPair[];
    username?: string | null;
    password?: string | null;
    private_key_passphrase?: string | null;
    extra: Record<string, string>;
  };
}

export interface DownloadTask {
  id: string;
  kind: DownloadKind;
  save_dir: string;
  file_name?: string | null;
  state: TaskState;
  limits: TaskRateLimit;
  proxy: unknown;
  total_bytes?: number | null;
  downloaded_bytes: number;
  uploaded_bytes: number;
  error?: string | null;
  created_at: string;
  updated_at: string;
  completed_at?: string | null;
}

export interface TaskDetail {
  task: DownloadTask;
  credentials: {
    headers: HeaderPair[];
    username?: string | null;
    password?: string | null;
    private_key_passphrase?: string | null;
    extra: Record<string, string>;
  };
}

export interface SettingsSnapshot {
  download_limit?: number | null;
  upload_limit?: number | null;
  use_system_proxy: boolean;
  bt_trackers: string[];
  bt_ip_allow: string[];
  bt_ip_deny: string[];
}

export type CoreEvent =
  | { TaskCreated: TaskSummary }
  | { TaskStateChanged: { task_id: string; state: TaskState } }
  | {
      TaskProgress: {
        task_id: string;
        downloaded_bytes: number;
        uploaded_bytes: number;
        total_bytes?: number | null;
      };
    }
  | {
      TaskSpeed: {
        task_id: string;
        download_bytes_per_second: number;
        upload_bytes_per_second: number;
      };
    }
  | { TaskError: { task_id: string; message: string } }
  | { TaskCompleted: { task_id: string; file_path: string } }
  | { SettingsChanged: SettingsSnapshot };

export interface RuntimeTask extends TaskSummary {
  download_speed?: number;
  upload_speed?: number;
}

export interface BtFileState {
  index: number;
  name: string;
  size: number;
  downloaded: number;
  selected: boolean;
}

export interface BtStateSnapshot {
  files: BtFileState[];
  piece_haves: number[];
  total_pieces: number;
  seed_ratio: number | null;
}

export interface MagnetPreviewFile {
  index: number;
  name: string;
  size: number;
  selected: boolean;
}

export interface MagnetPreview {
  name: string | null;
  info_hash: string;
  total_bytes: number;
  files: MagnetPreviewFile[];
  trackers: string[];
}

// Sidebar navigation: task categories + settings.
export type SidebarFilter =
  | 'all'
  | 'downloading'
  | 'completed'
  | 'failed'
  | 'stopped'
  | 'trash'
  | 'settings';
