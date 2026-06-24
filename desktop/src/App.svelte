<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { AlertCircle, X } from '@lucide/svelte';
  import {
    createTask,
    deleteTask,
    getBtState,
    getSettings,
    getTask,
    listenToCoreEvents,
    pauseTask,
    startTask,
    stopTask
  } from './lib/api';
  import type {
    BtStateSnapshot,
    CoreEvent,
    CreateTaskInput,
    SidebarFilter,
    TaskDetail as TaskDetailDto,
    TaskState
  } from './lib/types';
  import {
    removeFromTrash,
    tasksStore,
    taskList,
    taskLoading,
    taskSpeeds,
    trashTasks,
    trashedTaskIds
  } from './lib/stores/tasks';
  import { settingsStore } from './lib/stores/settings';

  import Sidebar from './lib/components/Sidebar.svelte';
  import TaskListPane from './lib/components/TaskListPane.svelte';
  import TaskDetail from './lib/components/TaskDetail.svelte';
  import NewTaskDialog from './lib/components/NewTaskDialog.svelte';
  import SettingsView from './lib/components/SettingsView.svelte';
  import ConfirmDialog from './lib/components/ConfirmDialog.svelte';

  let sidebarFilter: SidebarFilter = 'all';
  let view: 'downloads' | 'settings' = 'downloads';
  $: view = sidebarFilter === 'settings' ? 'settings' : 'downloads';
  let selectedTaskId = '';
  let selectedTask: TaskDetailDto | null = null;
  let detailDown = 0;
  let detailUp = 0;
  let btState: BtStateSnapshot | null = null;
  let btPollHandle: ReturnType<typeof setInterval> | null = null;

  let busy = false;
  let error = '';
  let refreshing = false;

  let showNewTask = false;
  let newTaskError = '';

  let confirmOpen = false;
  let confirmDeleteFiles = false;
  let confirmKind: 'stop' | 'moveToTrash' | 'deletePermanent' | 'clearTrash' | null = null;
  let confirmTaskIds: string[] = [];

  let detailOpen = true;

  // Subscriptions to named stores for the template.
  $: tasks = $taskList;
  $: listLoading = $taskLoading;
  $: speeds = $taskSpeeds;
  $: trashIds = $trashedTaskIds;

  async function refreshAll() {
    refreshing = true;
    error = '';
    try {
      await Promise.all([tasksStore.load(), getSettings().then((s) => settingsStore.set(s))]);
      if (selectedTaskId) {
        selectedTask = await getTask(selectedTaskId);
      } else {
        selectedTask = null;
      }
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      refreshing = false;
    }
  }

  async function selectTask(taskId: string) {
    selectedTaskId = taskId;
    detailOpen = true;
    try {
      selectedTask = await getTask(taskId);
      await refreshBtState();
      restartBtPoll();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  async function refreshBtState() {
    if (!selectedTaskId || !selectedTask) {
      btState = null;
      return;
    }
    if (selectedTask.task.kind !== 'Bt') {
      btState = null;
      return;
    }
    try {
      btState = await getBtState(selectedTaskId);
    } catch {
      btState = null;
    }
  }

  function restartBtPoll() {
    if (btPollHandle) {
      clearInterval(btPollHandle);
      btPollHandle = null;
    }
    const active = selectedTask && (selectedTask.task.state === 'Downloading'
      || selectedTask.task.state === 'Seeding'
      || selectedTask.task.state === 'Resolving'
      || selectedTask.task.state === 'Verifying');
    if (selectedTask && selectedTask.task.kind === 'Bt' && active) {
      btPollHandle = setInterval(() => void refreshBtState(), 1000);
    }
  }

  function stopBtPoll() {
    if (btPollHandle) {
      clearInterval(btPollHandle);
      btPollHandle = null;
    }
  }

  // Detail panel live speed from the event-derived speed map.
  $: if (selectedTaskId && speeds[selectedTaskId]) {
    detailDown = speeds[selectedTaskId].down;
    detailUp = speeds[selectedTaskId].up;
  } else if (selectedTaskId) {
    detailDown = 0;
    detailUp = 0;
  }

  function setFilter(f: SidebarFilter) {
    sidebarFilter = f;
  }

  // Map a sidebar category to the status filter consumed by TaskListPane.
  function sidebarFilterToStatus(f: SidebarFilter): TaskState | 'all' {
    switch (f) {
      case 'downloading': return 'Downloading';
      case 'completed': return 'Completed';
      case 'failed': return 'Failed';
      case 'stopped': return 'Stopped';
      default: return 'all';
    }
  }

  async function actionForSelected(action: 'start' | 'pause' | 'stop' | 'delete') {
    if (!selectedTaskId) return;
    if (action === 'delete') {
      openConfirm(trashIds.includes(selectedTaskId) ? 'deletePermanent' : 'moveToTrash', [selectedTaskId]);
      return;
    }
    if (action === 'stop') {
      openConfirm('stop', [selectedTaskId]);
      return;
    }
    busy = true;
    error = '';
    try {
      if (action === 'start') await startTask(selectedTaskId);
      if (action === 'pause') await pauseTask(selectedTaskId);
      selectedTask = await getTask(selectedTaskId);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  // Retry a failed task = start it again.
  async function retryTask(taskId: string) {
    busy = true;
    error = '';
    try {
      await startTask(taskId);
      if (taskId === selectedTaskId) selectedTask = await getTask(taskId);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  async function actionForTasks(taskIds: string[], action: 'start' | 'pause' | 'stop' | 'delete') {
    const ids = uniqueTaskIds(taskIds);
    if (ids.length === 0) return;
    if (action === 'delete') {
      openConfirm(sidebarFilter === 'trash' ? 'deletePermanent' : 'moveToTrash', ids);
      return;
    }
    if (action === 'stop') {
      openConfirm('stop', ids);
      return;
    }
    busy = true;
    error = '';
    try {
      for (const taskId of ids) {
        if (action === 'start') {
          removeFromTrash([taskId]);
          await startTask(taskId);
        }
        if (action === 'pause') await pauseTask(taskId);
      }
      if (selectedTaskId && ids.includes(selectedTaskId)) selectedTask = await getTask(selectedTaskId);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  function uniqueTaskIds(taskIds: string[]) {
    return Array.from(new Set(taskIds.filter(Boolean)));
  }

  function openConfirm(kind: typeof confirmKind, taskIds: string[]) {
    confirmKind = kind;
    confirmTaskIds = uniqueTaskIds(taskIds);
    confirmDeleteFiles = false;
    confirmOpen = confirmTaskIds.length > 0 || kind === 'clearTrash';
  }

  function pluralTask(count: number) {
    return count === 1 ? 'task' : 'tasks';
  }

  function confirmTitle(kind: typeof confirmKind, count: number) {
    if (kind === 'stop') return `Stop ${count} ${pluralTask(count)}`;
    if (kind === 'moveToTrash') return `Move ${count} ${pluralTask(count)} to Trash`;
    if (kind === 'deletePermanent') return `Delete ${count} ${pluralTask(count)}`;
    if (kind === 'clearTrash') return 'Clear Trash';
    return 'Confirm';
  }

  function confirmMessage(kind: typeof confirmKind, count: number) {
    if (kind === 'stop') return 'Stop the selected download task? You can start it again later from the stopped list.';
    if (kind === 'moveToTrash') return 'Move the selected task to Trash? It will be hidden from normal lists until Trash is cleared.';
    if (kind === 'deletePermanent') return 'Permanently delete the selected task from Trash? This cannot be undone.';
    if (kind === 'clearTrash') return count > 0
      ? `Permanently delete ${count} ${pluralTask(count)} from Trash? This cannot be undone.`
      : 'Trash is already empty.';
    return '';
  }

  function confirmLabel(kind: typeof confirmKind) {
    if (kind === 'stop') return 'Stop';
    if (kind === 'moveToTrash') return 'Move to Trash';
    if (kind === 'clearTrash') return 'Clear Trash';
    return 'Delete';
  }

  function closeConfirm() {
    if (busy) return;
    confirmOpen = false;
    confirmKind = null;
    confirmTaskIds = [];
    confirmDeleteFiles = false;
  }

  async function confirmAction() {
    if (!confirmKind) return;
    busy = true;
    error = '';
    try {
      if (confirmKind === 'stop') {
        for (const taskId of confirmTaskIds) await stopTask(taskId);
        if (selectedTaskId && confirmTaskIds.includes(selectedTaskId)) selectedTask = await getTask(selectedTaskId);
      }
      if (confirmKind === 'moveToTrash') {
        await moveTasksToTrash(confirmTaskIds);
      }
      if (confirmKind === 'deletePermanent') {
        await permanentlyDeleteTasks(confirmTaskIds, confirmDeleteFiles);
      }
      if (confirmKind === 'clearTrash') {
        await permanentlyDeleteTasks(trashIds, confirmDeleteFiles);
      }
      confirmOpen = false;
      confirmKind = null;
      confirmTaskIds = [];
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  async function moveTasksToTrash(taskIds: string[]) {
    const taskById = new Map(tasks.map((task) => [task.id, task]));
    for (const taskId of taskIds) {
      const task = taskById.get(taskId);
      if (task && task.state !== 'Completed' && task.state !== 'Failed') {
        await stopTask(taskId);
      }
    }
    trashTasks(taskIds);
    if (selectedTaskId && taskIds.includes(selectedTaskId) && sidebarFilter !== 'trash') {
      selectedTaskId = '';
      selectedTask = null;
    }
  }

  async function permanentlyDeleteTasks(taskIds: string[], deleteFiles: boolean) {
    const ids = uniqueTaskIds(taskIds);
    for (const taskId of ids) {
      await deleteTask(taskId, deleteFiles);
      tasksStore.remove(taskId);
    }
    if (selectedTaskId && ids.includes(selectedTaskId)) {
      selectedTaskId = '';
      selectedTask = null;
    }
  }

  async function submitNewTask(input: CreateTaskInput) {
    busy = true;
    newTaskError = '';
    try {
      await createTask(input);
      showNewTask = false;
      await refreshAll();
    } catch (cause) {
      // Preserve user input: do not close the dialog.
      newTaskError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  onMount(() => {
    void refreshAll();
    const unlisten = listenToCoreEvents(async (event: CoreEvent) => {
      if ('SettingsChanged' in event) {
        settingsStore.set(event.SettingsChanged);
        return;
      }
      tasksStore.applyEvent(event);
      if (selectedTaskId && affectsTask(event, selectedTaskId) && !busy) {
        try {
          selectedTask = await getTask(selectedTaskId);
          await refreshBtState();
          restartBtPoll();
        } catch {
          // Ignore transient fetch errors; store already reflects the event.
        }
      }
    });
    return () => {
      void unlisten.then((dispose) => dispose());
    };
  });

  onDestroy(() => {
    stopBtPoll();
  });

  function affectsTask(event: CoreEvent, taskId: string): boolean {
    if ('TaskCreated' in event) return event.TaskCreated.id === taskId;
    if ('TaskStateChanged' in event) return event.TaskStateChanged.task_id === taskId;
    if ('TaskProgress' in event) return event.TaskProgress.task_id === taskId;
    if ('TaskSpeed' in event) return event.TaskSpeed.task_id === taskId;
    if ('TaskError' in event) return event.TaskError.task_id === taskId;
    if ('TaskCompleted' in event) return event.TaskCompleted.task_id === taskId;
    return false;
  }

  function dismissError() {
    error = '';
  }

  $: confirmCount = confirmKind === 'clearTrash' ? trashIds.length : confirmTaskIds.length;
</script>

<svelte:head>
  <title>Fluxion</title>
</svelte:head>

<div class="shell" class:detail-collapsed={!detailOpen} data-tauri-drag-region="false">
  <Sidebar
    filter={sidebarFilter}
    onFilter={setFilter}
  />

  {#if view === 'downloads'}
    <TaskListPane
      tasks={tasks}
      loading={listLoading}
      {selectedTaskId}
      {busy}
      statusFilter={sidebarFilterToStatus(sidebarFilter)}
      onSelect={selectTask}
      onTaskAction={actionForTasks}
      onNewTask={() => (showNewTask = true)}
      onRefresh={refreshAll}
      {refreshing}
      detailOpen={detailOpen}
      onToggleDetail={() => (detailOpen = !detailOpen)}
      sidebarFilter={sidebarFilter}
      onClearTrash={() => openConfirm('clearTrash', trashIds)}
    />

    <section class="detail-pane" class:closed={!detailOpen}>
      {#if selectedTask}
        <TaskDetail
          detail={selectedTask}
          downSpeed={detailDown}
          upSpeed={detailUp}
          {busy}
          {btState}
          onAction={actionForSelected}
          onRetry={() => retryTask(selectedTaskId)}
          onLimitsSaved={async () => {
            if (selectedTaskId) {
              selectedTask = await getTask(selectedTaskId);
              await refreshBtState();
            }
          }}
        />
      {:else}
        <div class="detail-empty" data-tauri-drag-region="true">
          <div class="empty-mark">⌘</div>
          <h3>Select a task</h3>
          <p>Choose a download to inspect its details, progress and credentials.</p>
        </div>
      {/if}
    </section>
  {:else}
    <main class="settings-pane">
      <div class="pane-header" data-tauri-drag-region="true">
        <h2>Settings</h2>
      </div>
      <div class="scroll-area">
        <SettingsView
          {busy}
          onError={(m) => (error = m)}
          onSaved={() => (error = '')}
        />
      </div>
    </main>
  {/if}
</div>

{#if error}
  <div class="toast" role="alert">
    <AlertCircle size={15} />
    <span class="toast-text">{error}</span>
    <button class="toast-close" on:click={dismissError} aria-label="Dismiss"><X size={14} /></button>
  </div>
{/if}

<NewTaskDialog
  open={showNewTask}
  {busy}
  error={newTaskError}
  onSubmit={submitNewTask}
  onClose={() => (showNewTask = false)}
/>

<ConfirmDialog
  open={confirmOpen}
  title={confirmTitle(confirmKind, confirmCount)}
  message={confirmMessage(confirmKind, confirmCount)}
  confirmLabel={confirmLabel(confirmKind)}
  secondaryLabel={confirmKind === 'deletePermanent' || confirmKind === 'clearTrash' ? 'Also delete downloaded files' : null}
  bind:secondaryChecked={confirmDeleteFiles}
  {busy}
  onConfirm={confirmAction}
  onCancel={closeConfirm}
/>

<style lang="scss">
  @use './styles/tokens' as *;

  .shell {
    display: grid;
    grid-template-columns: var(--nav-w, 220px) minmax(var(--list-w, 360px), 1fr) minmax(420px, 58vw);
    height: 100vh;
    min-height: 100vh;
    overflow: hidden;
    transition: grid-template-columns 260ms $ease-out;
  }

  // When the detail pane is collapsed, animate the list into the freed space.
  .shell.detail-collapsed {
    grid-template-columns: var(--nav-w, 220px) minmax(0, 1fr) 0;
  }

  .detail-pane {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-left: 1px solid var(--border);
    background: var(--surface-2);
    overflow: hidden;
    opacity: 1;
    transform: translateX(0);
    transition:
      opacity 190ms $ease-out,
      transform 260ms $ease-out,
      border-color 260ms $ease-out;

    &.closed {
      opacity: 0;
      transform: translateX(18px);
      pointer-events: none;
      border-left-color: transparent;
    }
  }

  .detail-pane > :global(*) {
    min-width: min(420px, calc(100vw - var(--nav-w, 220px) - 280px));
  }

  @media (prefers-reduced-motion: reduce) {
    .shell,
    .detail-pane {
      transition: none;
    }
  }

  .detail-empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: $space-1;
    text-align: center;
    padding: $space-8;
    color: var(--text-muted);
    -webkit-app-region: drag;
  }

  .empty-mark {
    width: 60px;
    height: 60px;
    border-radius: $radius-xl;
    display: grid;
    place-items: center;
    background: var(--surface-3);
    border: 1px solid var(--border);
    box-shadow: var(--inner-highlight);
    color: var(--text-faint);
    font-size: $fs-lg;
    font-weight: $fw-bold;
    margin-bottom: $space-2;
  }

  .detail-empty h3 {
    font-size: $fs-md;
    color: var(--text-strong);
    margin: 0;
  }

  .detail-empty p {
    font-size: $fs-sm;
    max-width: 280px;
    margin: 0;
    line-height: $lh-normal;
  }

  .settings-pane {
    grid-column: 2 / -1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: var(--surface-2);
  }

  .pane-header {
    display: flex;
    align-items: center;
    height: 52px;
    padding: 0 $space-6;
    border-bottom: 1px solid var(--border);
    -webkit-app-region: drag;

    h2 {
      font-size: $fs-lg;
      letter-spacing: -0.01em;
    }
  }

  .scroll-area {
    flex: 1;
    overflow-y: auto;
    padding: $space-6;
    @include scrollbar;
  }

  // Floating error toast (non-blocking) instead of a banner row.
  .toast {
    position: fixed;
    bottom: $space-6;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: $space-2;
    padding: $space-3 $space-4;
    border-radius: $radius-md;
    background: var(--elevated);
    border: 1px solid var(--state-bad);
    box-shadow: var(--shadow-lg), var(--inner-highlight);
    color: var(--state-bad);
    font-size: $fs-sm;
    z-index: 40;
    animation: toast-in $dur-base $ease-out;
  }

  .toast-text {
    color: var(--text);
    word-break: break-word;
    max-width: 60vw;
  }

  .toast-close {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: $radius-xs;
    border: none;
    background: transparent;
    color: inherit;
    cursor: pointer;
    &:hover {
      background: rgba(248, 113, 113, 0.18);
    }
  }

  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translate(-50%, 12px);
    }
    to {
      opacity: 1;
      transform: translate(-50%, 0);
    }
  }

  @media (max-width: 1100px) {
    .shell {
      grid-template-columns: var(--nav-w, 220px) 1fr;
    }
    .shell:has(.detail-pane.closed) {
      grid-template-columns: var(--nav-w, 220px) 1fr;
    }
    .detail-pane {
      position: absolute;
      right: 0;
      top: 0;
      bottom: 0;
      width: min(420px, 100%);
      z-index: 20;
      box-shadow: var(--shadow-lg);
    }
  }

  @media (max-width: 760px) {
    .shell {
      grid-template-columns: 1fr;
    }
    .detail-pane {
      width: 100%;
    }
  }
</style>
