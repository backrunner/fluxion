import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import type {
  BtStateSnapshot,
  CoreEvent,
  CreateTaskInput,
  DownloadTask,
  MagnetPreview,
  SettingsSnapshot,
  TaskDetail,
  TaskRateLimit,
  TaskSummary
} from './types';

type TauriInternals = {
  invoke?: unknown;
};

declare global {
  interface Window {
    __TAURI_INTERNALS__?: TauriInternals;
  }
}

let previewSettings: SettingsSnapshot = {
  download_limit: 12 * 1024 * 1024,
  upload_limit: 2 * 1024 * 1024,
  use_system_proxy: true,
  bt_trackers: ['udp://tracker.opentrackr.org:1337/announce'],
  bt_ip_allow: [],
  bt_ip_deny: []
};

let previewTasks: TaskSummary[] = [
  {
    id: 'preview-http-iso',
    kind: 'Http',
    file_name: 'macos-sdk-symbols.dmg',
    state: 'Downloading',
    total_bytes: 3_742_580_736,
    downloaded_bytes: 2_104_492_032,
    uploaded_bytes: 0,
    error: null,
    created_at: '2026-06-19T12:45:00+08:00',
    updated_at: '2026-06-19T13:12:30+08:00'
  },
  {
    id: 'preview-bt-archive',
    kind: 'Bt',
    file_name: 'linux-images-collection',
    state: 'Paused',
    total_bytes: 18_937_331_712,
    downloaded_bytes: 6_218_145_792,
    uploaded_bytes: 214_958_080,
    error: null,
    created_at: '2026-06-19T11:18:00+08:00',
    updated_at: '2026-06-19T12:58:10+08:00'
  },
  {
    id: 'preview-http-docs',
    kind: 'Http',
    file_name: 'fluxion-core-reference.zip',
    state: 'Completed',
    total_bytes: 489_684_992,
    downloaded_bytes: 489_684_992,
    uploaded_bytes: 0,
    error: null,
    created_at: '2026-06-19T10:02:00+08:00',
    updated_at: '2026-06-19T10:06:42+08:00'
  }
];

function shouldUsePreviewApi() {
  return import.meta.env.DEV && (typeof window === 'undefined' || typeof window.__TAURI_INTERNALS__?.invoke !== 'function');
}

function cloneTask(task: TaskSummary): TaskSummary {
  return { ...task };
}

function toDownloadTask(task: TaskSummary): DownloadTask {
  return {
    ...task,
    kind: task.kind,
    save_dir: task.kind === 'Bt' ? '/Users/you/Downloads/Torrents' : '/Users/you/Downloads',
    limits: {
      download_bytes_per_second: null,
      upload_bytes_per_second: null
    },
    proxy: 'UseGlobal',
    completed_at: task.state === 'Completed' ? task.updated_at : null
  };
}

function inferFileName(url: string) {
  try {
    const name = new URL(url).pathname.split('/').filter(Boolean).pop();
    return name || 'download.bin';
  } catch {
    return 'download.bin';
  }
}

export function listTasks() {
  if (shouldUsePreviewApi()) return Promise.resolve(previewTasks.map(cloneTask));
  return invoke<TaskSummary[]>('list_tasks', { filter: null });
}

export function getTask(taskId: string) {
  if (shouldUsePreviewApi()) {
    const task = previewTasks.find((item) => item.id === taskId);
    return Promise.resolve<TaskDetail | null>(
      task
        ? {
            task: toDownloadTask(task),
            credentials: {
              headers: [],
              username: null,
              password: null,
              private_key_passphrase: null,
              extra: {}
            }
          }
        : null
    );
  }
  return invoke<TaskDetail | null>('get_task', { taskId });
}

export function createTask(input: CreateTaskInput) {
  if (shouldUsePreviewApi()) {
    const now = new Date().toISOString();
    const id = `preview-${Date.now()}`;
    const sourceUrl = 'Http' in input.kind
      ? input.kind.Http.url
      : 'Bt' in input.kind
        ? 'Magnet' in input.kind.Bt.source
          ? input.kind.Bt.source.Magnet
          : input.kind.Bt.source.TorrentFile
        : 'Ftp' in input.kind
          ? input.kind.Ftp.url
          : input.kind.Sftp.url;
    const kindLabel = 'Http' in input.kind
      ? 'Http'
      : 'Bt' in input.kind
        ? 'Bt'
        : 'Ftp' in input.kind
          ? 'Ftp'
          : 'Sftp';
    previewTasks = [
      {
        id,
        kind: kindLabel,
        file_name: input.file_name || inferFileName(sourceUrl),
        state: 'Queued',
        total_bytes: null,
        downloaded_bytes: 0,
        uploaded_bytes: 0,
        error: null,
        created_at: now,
        updated_at: now
      },
      ...previewTasks
    ];
    return Promise.resolve(id);
  }
  return invoke<string>('create_task', { input });
}

export function startTask(taskId: string) {
  if (shouldUsePreviewApi()) {
    previewTasks = previewTasks.map((task) =>
      task.id === taskId ? { ...task, state: 'Downloading', updated_at: new Date().toISOString() } : task
    );
    return Promise.resolve();
  }
  return invoke<void>('start_task', { taskId });
}

export function pauseTask(taskId: string) {
  if (shouldUsePreviewApi()) {
    previewTasks = previewTasks.map((task) =>
      task.id === taskId ? { ...task, state: 'Paused', updated_at: new Date().toISOString() } : task
    );
    return Promise.resolve();
  }
  return invoke<void>('pause_task', { taskId });
}

export function stopTask(taskId: string) {
  if (shouldUsePreviewApi()) {
    previewTasks = previewTasks.map((task) =>
      task.id === taskId ? { ...task, state: 'Stopped', updated_at: new Date().toISOString() } : task
    );
    return Promise.resolve();
  }
  return invoke<void>('stop_task', { taskId });
}

export function deleteTask(taskId: string, deleteFiles = false) {
  if (shouldUsePreviewApi()) {
    previewTasks = previewTasks.filter((task) => task.id !== taskId);
    return Promise.resolve();
  }
  return invoke<void>('delete_task', { taskId, deleteFiles });
}

export function getSettings() {
  if (shouldUsePreviewApi()) return Promise.resolve({ ...previewSettings });
  return invoke<SettingsSnapshot>('get_settings');
}

export function updateSettings(settings: SettingsSnapshot) {
  if (shouldUsePreviewApi()) {
    previewSettings = { ...settings };
    return Promise.resolve();
  }
  return invoke<void>('update_settings', { settings });
}

export function pauseAll() {
  if (shouldUsePreviewApi()) {
    previewTasks = previewTasks.map((task) =>
      task.state === 'Downloading' || task.state === 'Resolving'
        ? { ...task, state: 'Paused', updated_at: new Date().toISOString() }
        : task
    );
    return Promise.resolve();
  }
  return invoke<void>('pause_all');
}

export function startAll() {
  if (shouldUsePreviewApi()) {
    previewTasks = previewTasks.map((task) =>
      task.state === 'Paused' || task.state === 'Stopped' || task.state === 'Queued' || task.state === 'Failed'
        ? { ...task, state: 'Downloading', updated_at: new Date().toISOString() }
        : task
    );
    return Promise.resolve();
  }
  return invoke<void>('start_all');
}

export function clearFinished(deleteFiles = false) {
  if (shouldUsePreviewApi()) {
    previewTasks = previewTasks.filter(
      (task) => task.state !== 'Completed' && task.state !== 'Failed' && task.state !== 'Stopped'
    );
    return Promise.resolve();
  }
  return invoke<void>('clear_finished', { deleteFiles });
}

export function updateTaskLimits(taskId: string, limits: TaskRateLimit) {
  if (shouldUsePreviewApi()) {
    return Promise.resolve();
  }
  return invoke<void>('update_task_limits', { taskId, limits });
}

export function openTaskFile(taskId: string) {
  if (shouldUsePreviewApi()) {
    return Promise.resolve();
  }
  return invoke<void>('open_task_file', { taskId });
}

export function revealTaskFile(taskId: string) {
  if (shouldUsePreviewApi()) {
    return Promise.resolve();
  }
  return invoke<void>('reveal_task_file', { taskId });
}

// Preview mock BT state: deterministic-ish fake files + piece bitfield so the
// detail UI can render the piece grid and file list in browser preview.
function previewBtState(taskId: string): BtStateSnapshot {
  const seed = Array.from(taskId).reduce((acc, ch) => acc + ch.charCodeAt(0), 0);
  const fileCount = 3;
  const files = Array.from({ length: fileCount }, (_, i) => {
    const size = 1024 * 1024 * (64 + ((seed + i * 37) % 256));
    const downloaded = Math.floor(size * (0.2 + ((seed + i * 53) % 70) / 100));
    return {
      index: i,
      name: `archive/part${i + 1}.bin`,
      size,
      downloaded,
      selected: true
    };
  });
  const totalPieces = 48;
  const pieceHaves = Array.from({ length: Math.ceil(totalPieces / 8) }, (_, byteIdx) => {
    // Roughly half the pieces are "have", deterministically.
    let byte = 0;
    for (let bit = 0; bit < 8; bit++) {
      const pieceIdx = byteIdx * 8 + bit;
      if (pieceIdx >= totalPieces) break;
      if ((seed + pieceIdx * 7) % 10 < 6) {
        byte |= 0x80 >> bit;
      }
    }
    return byte;
  });
  const totalBytes = files.reduce((sum, f) => sum + f.size, 0);
  const uploaded = Math.floor(totalBytes * 0.4);
  return {
    files,
    piece_haves: pieceHaves,
    total_pieces: totalPieces,
    seed_ratio: totalBytes > 0 ? uploaded / totalBytes : null
  };
}

export function getBtState(taskId: string) {
  if (shouldUsePreviewApi()) {
    const task = previewTasks.find((t) => t.id === taskId);
    if (!task || task.kind !== 'Bt') return Promise.resolve<BtStateSnapshot | null>(null);
    return Promise.resolve<BtStateSnapshot | null>(previewBtState(taskId));
  }
  return invoke<BtStateSnapshot | null>('get_bt_state', { taskId });
}

export function listenToCoreEvents(handler: (event: CoreEvent) => void) {
  if (shouldUsePreviewApi()) {
    return Promise.resolve(() => {
      void handler;
    });
  }
  return listen<CoreEvent>('fluxion://event', (event) => handler(event.payload));
}

// Open the native macOS folder picker. Returns the chosen directory path, or
// null if the user cancelled. In browser preview mode there is no native
// dialog, so this resolves to null (the form falls back to manual text entry).
export async function pickDirectory(): Promise<string | null> {
  if (shouldUsePreviewApi()) return null;
  const selected = await open({ directory: true, multiple: false });
  if (typeof selected === 'string') return selected;
  // `open` can return a string[] when multiple:true; we passed multiple:false
  // so a non-string result means the user cancelled.
  return null;
}

// Open the native file picker for a single .torrent file. Returns the chosen
// file path, or null if the user cancelled.
export async function pickTorrentFile(): Promise<string | null> {
  if (shouldUsePreviewApi()) return null;
  const selected = await open({
    multiple: false,
    filters: [{ name: 'Torrent', extensions: ['torrent'] }]
  });
  if (typeof selected === 'string') return selected;
  return null;
}

// Preview mock for the magnet pre-resolve: a deterministic fake file list so
// the new-task form can exercise the file-selection UI in browser preview.
function previewMagnetResolve(magnet: string): MagnetPreview {
  const name = decodeURIComponent(
    magnet.match(/[?&]dn=([^&]+)/)?.[1] ?? ''
  ) || 'preview-torrent';
  const files = Array.from({ length: 4 }, (_, i) => ({
    index: i,
    name: `${name}/part${i + 1}.bin`,
    size: 1024 * 1024 * (128 + i * 64),
    selected: true
  }));
  return {
    name,
    info_hash: '0000000000000000000000000000000000000000',
    total_bytes: files.reduce((sum, f) => sum + f.size, 0),
    files,
    trackers: ['udp://tracker.opentrackr.org:1337/announce']
  };
}

// Resolve a magnet link into torrent metadata (name + file list) before task
// creation, so the user can preview and pick files. Network-bound; may take
// seconds or fail on dead swarms — callers must handle errors.
export function resolveMagnetPreview(magnet: string): Promise<MagnetPreview> {
  if (shouldUsePreviewApi()) {
    return Promise.resolve(previewMagnetResolve(magnet));
  }
  return invoke<MagnetPreview>('resolve_magnet_preview', { magnet });
}
