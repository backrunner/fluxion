// Task store — holds the runtime task list plus event-derived speed/progress
// maps so we can update rows granularly (instead of refetching everything on
// every Core event). `list_tasks` / `get_task` are only used for initial load
// and recovery.
import { writable, derived, get } from 'svelte/store';
import type { CoreEvent, RuntimeTask, TaskState, TaskSummary } from '../types';
import { listTasks } from '../api';

const TRASH_STORAGE_KEY = 'fluxion.trashTaskIds.v1';

const statePriority: Record<TaskState, number> = {
  Queued: 1,
  Resolving: 2,
  Downloading: 3,
  Paused: 4,
  Stopped: 5,
  Completed: 6,
  Seeding: 7,
  Failed: 8,
  Verifying: 9
};

function sortTasks(list: TaskSummary[]): RuntimeTask[] {
  return list
    .slice()
    .sort((a, b) => {
      const order = statePriority[a.state] - statePriority[b.state];
      if (order !== 0) return order;
      return b.updated_at.localeCompare(a.updated_at);
    })
    .map((task) => ({ ...task }));
}

// speed map: taskId -> { download, upload } bytes/sec (event-derived)
export type SpeedMap = Record<string, { down: number; up: number }>;
// progress map: taskId -> { downloaded, total } (event-derived, latest)
export type ProgressMap = Record<string, { downloaded: number; total: number | null }>;

function readTrashIds(): string[] {
  if (typeof window === 'undefined') return [];
  try {
    const raw = window.localStorage.getItem(TRASH_STORAGE_KEY);
    const parsed = raw ? JSON.parse(raw) : [];
    return Array.isArray(parsed) ? parsed.filter((id): id is string => typeof id === 'string') : [];
  } catch {
    return [];
  }
}

const trashIds = writable<string[]>(readTrashIds());

if (typeof window !== 'undefined') {
  trashIds.subscribe((ids) => {
    window.localStorage.setItem(TRASH_STORAGE_KEY, JSON.stringify(ids));
  });
}

export const trashedTaskIds = { subscribe: trashIds.subscribe };

export function trashTasks(taskIds: string[]) {
  trashIds.update((ids) => {
    const next = new Set(ids);
    taskIds.forEach((id) => next.add(id));
    return Array.from(next);
  });
}

export function removeFromTrash(taskIds: string[]) {
  const removed = new Set(taskIds);
  trashIds.update((ids) => ids.filter((id) => !removed.has(id)));
}

function pruneTrashIds(validIds: Set<string>) {
  trashIds.update((ids) => ids.filter((id) => validIds.has(id)));
}

function createTasksStore() {
  const list = writable<RuntimeTask[]>([]);
  const loading = writable(true);
  const speeds = writable<SpeedMap>({});
  const progress = writable<ProgressMap>({});

  async function load() {
    loading.set(true);
    try {
      const loaded = await listTasks();
      pruneTrashIds(new Set(loaded.map((task) => task.id)));
      list.set(sortTasks(loaded));
    } finally {
      loading.set(false);
    }
  }

  function applyEvent(event: CoreEvent) {
    if ('TaskCreated' in event) {
      const summary = event.TaskCreated;
      list.update((items) => {
        if (items.some((t) => t.id === summary.id)) return items;
        return sortTasks([...items, summary]);
      });
      return;
    }

    if ('TaskStateChanged' in event) {
      const { task_id, state } = event.TaskStateChanged;
      list.update((items) =>
        items.map((t) => (t.id === task_id ? { ...t, state, updated_at: new Date().toISOString() } : t))
      );
      if (state === 'Completed' || state === 'Stopped' || state === 'Paused' || state === 'Failed') {
        speeds.update((map) => {
          if (!map[task_id]) return map;
          const next = { ...map };
          delete next[task_id];
          return next;
        });
      }
      return;
    }

    if ('TaskProgress' in event) {
      const { task_id, downloaded_bytes, uploaded_bytes, total_bytes } = event.TaskProgress;
      progress.update((map) => ({
        ...map,
        [task_id]: { downloaded: downloaded_bytes, total: total_bytes ?? null }
      }));
      list.update((items) =>
        items.map((t) =>
          t.id === task_id
            ? {
                ...t,
                downloaded_bytes: downloaded_bytes,
                uploaded_bytes: uploaded_bytes,
                total_bytes: total_bytes ?? t.total_bytes
              }
            : t
        )
      );
      return;
    }

    if ('TaskSpeed' in event) {
      const { task_id, download_bytes_per_second, upload_bytes_per_second } = event.TaskSpeed;
      speeds.update((map) => ({
        ...map,
        [task_id]: { down: download_bytes_per_second, up: upload_bytes_per_second }
      }));
      return;
    }

    if ('TaskError' in event) {
      const { task_id, message } = event.TaskError;
      list.update((items) =>
        items.map((t) => (t.id === task_id ? { ...t, state: 'Failed', error: message } : t))
      );
      return;
    }

    if ('TaskCompleted' in event) {
      const { task_id } = event.TaskCompleted;
      list.update((items) =>
        items.map((t) =>
          t.id === task_id
            ? {
                ...t,
                state: 'Completed',
                downloaded_bytes: t.total_bytes ?? t.downloaded_bytes,
                updated_at: new Date().toISOString()
              }
            : t
        )
      );
      speeds.update((map) => {
        if (!map[task_id]) return map;
        const next = { ...map };
        delete next[task_id];
        return next;
      });
      return;
    }

    // SettingsChanged — handled by settingsStore, not here.
  }

  function remove(taskId: string) {
    removeFromTrash([taskId]);
    list.update((items) => items.filter((t) => t.id !== taskId));
    speeds.update((map) => {
      const next = { ...map };
      delete next[taskId];
      return next;
    });
    progress.update((map) => {
      const next = { ...map };
      delete next[taskId];
      return next;
    });
  }

  function clearSpeedsFor(taskId: string) {
    speeds.update((map) => {
      if (!map[taskId]) return map;
      const next = { ...map };
      delete next[taskId];
      return next;
    });
  }

  return {
    list: { subscribe: list.subscribe },
    loading: { subscribe: loading.subscribe },
    speeds: { subscribe: speeds.subscribe },
    progress: { subscribe: progress.subscribe },
    load,
    applyEvent,
    remove,
    clearSpeedsFor,
    getRaw: get.bind(null, list) as () => RuntimeTask[]
  };
}

export const tasksStore = createTasksStore();

// Named, directly-subscribable stores (preferred for `$`-auto-subscriptions
// in components, since `tasksStore` is a container, not a store itself).
export const taskList = tasksStore.list;
export const taskLoading = tasksStore.loading;
export const taskSpeeds = tasksStore.speeds;
export const taskProgress = tasksStore.progress;

// Aggregate speed across all active tasks (for the status bar / sidebar).
export const aggregateSpeed = derived(tasksStore.speeds, ($speeds) => {
  let down = 0;
  let up = 0;
  for (const id of Object.keys($speeds)) {
    down += $speeds[id].down;
    up += $speeds[id].up;
  }
  return { down, up };
});

// Active task count (Downloading / Seeding / Resolving / Queued).
export const activeCount = derived(tasksStore.list, ($list) =>
  $list.filter((t) => t.state === 'Downloading' || t.state === 'Seeding' || t.state === 'Resolving' || t.state === 'Queued')
    .length
);

// ETA (seconds) for a task given its speed; null when unknown.
export function eta(downloaded: number, total: number | null | undefined, downSpeed: number): number | null {
  if (!total || downSpeed <= 0) return null;
  const remaining = total - downloaded;
  if (remaining <= 0) return 0;
  return Math.ceil(remaining / downSpeed);
}
