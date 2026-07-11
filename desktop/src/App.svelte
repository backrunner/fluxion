<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { AlertCircle, Download, X, MousePointerClick, RefreshCw } from '@lucide/svelte';
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
    TaskDetail as TaskDetailDto
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
  import { canPause, canStart, canStop } from './lib/format';
  import { t } from './lib/i18n';
  import {
    checkForAppUpdate,
    restartAfterUpdate,
    type AppUpdate,
    type UpdateCheckState
  } from './lib/updater';

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
  let availableUpdate: AppUpdate | null = null;
  let updateBusy = false;
  let updateDownloaded = 0;
  let updateTotal: number | undefined;
  let updateError = '';
  let updateDismissed = false;
  let updateCheckState: UpdateCheckState = 'idle';
  let updateCheckError = '';

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
    if (selectedTaskId && (f === 'all' || f === 'trash')) {
      const selectedIsTrashed = trashIds.includes(selectedTaskId);
      const belongsToDestination = f === 'trash' ? selectedIsTrashed : !selectedIsTrashed;
      if (!belongsToDestination) {
        selectedTaskId = '';
        selectedTask = null;
        btState = null;
        stopBtPoll();
      }
    }
    sidebarFilter = f;
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
    const requestedIds = uniqueTaskIds(taskIds);
    const taskById = new Map(tasks.map((task) => [task.id, task]));
    const ids = action === 'delete'
      ? requestedIds
      : requestedIds.filter((taskId) => {
          const task = taskById.get(taskId);
          if (!task) return false;
          if (action === 'start') return canStart(task.state);
          if (action === 'pause') return canPause(task.state);
          return canStop(task.state);
        });
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

  function confirmTitle(kind: typeof confirmKind, count: number) {
    const suffix = count === 1 ? 'one' : 'many';
    if (kind === 'stop') return $t(`confirm.stopTitle.${suffix}`, { count });
    if (kind === 'moveToTrash') return $t(`confirm.trashTitle.${suffix}`, { count });
    if (kind === 'deletePermanent') return $t(`confirm.deleteTitle.${suffix}`, { count });
    if (kind === 'clearTrash') return $t('confirm.clearTitle');
    return $t('confirm.default');
  }

  function confirmMessage(kind: typeof confirmKind, count: number) {
    if (kind === 'stop') return $t('confirm.stopMessage');
    if (kind === 'moveToTrash') return $t('confirm.trashMessage');
    if (kind === 'deletePermanent') return $t('confirm.deleteMessage');
    if (kind === 'clearTrash') return count > 0
      ? $t(`confirm.clearMessage.${count === 1 ? 'one' : 'many'}`, { count })
      : $t('confirm.trashEmpty');
    return '';
  }

  function confirmLabel(kind: typeof confirmKind) {
    if (kind === 'stop') return $t('list.action.stop');
    if (kind === 'moveToTrash') return $t('confirm.moveTrash');
    if (kind === 'clearTrash') return $t('confirm.clearTitle');
    return $t('common.delete');
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

  async function checkForUpdates(manual = false) {
    if (updateCheckState === 'checking') return;
    updateCheckState = 'checking';
    updateCheckError = '';
    try {
      availableUpdate = await checkForAppUpdate();
      updateError = '';
      if (availableUpdate) {
        updateDismissed = false;
        updateCheckState = 'available';
      } else {
        updateCheckState = manual ? 'current' : 'idle';
      }
    } catch (cause) {
      if (manual) {
        updateCheckError = cause instanceof Error ? cause.message : String(cause);
        updateCheckState = 'error';
      } else {
        // Automatic checks are best-effort and must not interrupt download management.
        updateCheckState = 'idle';
      }
    }
  }

  async function installAvailableUpdate() {
    if (!availableUpdate || updateBusy) return;
    updateBusy = true;
    updateError = '';
    try {
      await availableUpdate.install((downloaded, total) => {
        updateDownloaded = downloaded;
        if (total !== undefined) updateTotal = total;
      });
      await restartAfterUpdate();
    } catch (cause) {
      updateError = cause instanceof Error ? cause.message : String(cause);
      updateBusy = false;
    }
  }

  onMount(() => {
    void refreshAll();
    void checkForUpdates();
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

<div class="shell" class:detail-collapsed={!detailOpen}>
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

    <section class="detail-pane" class:closed={!detailOpen} class:no-selection={!selectedTask}>
      {#if selectedTask}
        <TaskDetail
          detail={selectedTask}
          downSpeed={detailDown}
          upSpeed={detailUp}
          {busy}
          {btState}
          onAction={actionForSelected}
          onRetry={() => retryTask(selectedTaskId)}
          onClose={() => (detailOpen = false)}
          onLimitsSaved={async () => {
            if (selectedTaskId) {
              selectedTask = await getTask(selectedTaskId);
              await refreshBtState();
            }
          }}
        />
      {:else}
        <div class="detail-empty" data-tauri-drag-region="true">
          <div class="empty-mark"><MousePointerClick size={24} /></div>
          <h3>{$t('task.empty.title')}</h3>
          <p>{$t('task.empty.copy')}</p>
        </div>
      {/if}
    </section>
  {:else}
    <main class="settings-pane">
      <SettingsView
        {busy}
        {updateCheckState}
        updateVersion={availableUpdate?.version ?? null}
        {updateCheckError}
        onCheckForUpdates={() => checkForUpdates(true)}
        onError={(m) => (error = m)}
        onSaved={() => (error = '')}
      />
    </main>
  {/if}
</div>

{#if availableUpdate && !updateDismissed}
  <section class="update-banner" role="status" aria-live="polite">
    <div class="update-icon"><Download size={16} /></div>
    <div class="update-copy">
      <strong>{$t('update.available', { version: availableUpdate.version })}</strong>
      {#if updateBusy}
        <span>
          {$t('update.downloading')}
          {#if updateTotal}
            {' '}({Math.round((updateDownloaded / updateTotal) * 100)}%)
          {/if}
        </span>
      {:else if updateError}
        <span class="update-error">{updateError}</span>
      {:else if availableUpdate.body}
        <span>{availableUpdate.body}</span>
      {:else}
        <span>{$t('update.ready')}</span>
      {/if}
    </div>
    <div class="update-actions">
      {#if !updateBusy}
        <button class="update-install" on:click={installAvailableUpdate}>
          <RefreshCw size={14} /> {$t('update.install')}
        </button>
      {/if}
      <button
        class="update-dismiss"
        on:click={() => (updateDismissed = true)}
        aria-label={$t('common.dismiss')}
        disabled={updateBusy}
      >
        <X size={14} />
      </button>
    </div>
  </section>
{/if}

{#if error}
  <div class="toast" role="alert">
    <AlertCircle size={15} />
    <span class="toast-text">{error}</span>
    <button class="toast-close" on:click={dismissError} aria-label={$t('common.dismiss')}><X size={14} /></button>
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
  cancelLabel={$t('common.cancel')}
  secondaryLabel={confirmKind === 'deletePermanent' || confirmKind === 'clearTrash' ? $t('confirm.deleteFiles') : null}
  bind:secondaryChecked={confirmDeleteFiles}
  {busy}
  onConfirm={confirmAction}
  onCancel={closeConfirm}
/>

<style lang="scss">
  @use './styles/tokens' as *;

  .shell {
    --list-head-height: 85px;
    position: relative;
    display: grid;
    grid-template-columns:
      var(--nav-w, 200px)
      var(--list-w, 360px)
      calc(100vw - var(--nav-w, 200px) - var(--list-w, 360px));
    height: 100vh;
    min-height: 100vh;
    overflow: hidden;
    transition: grid-template-columns 360ms cubic-bezier(0.4, 0, 0.2, 1);
  }

  // When the detail pane is collapsed, animate the list into the freed space.
  .shell.detail-collapsed {
    grid-template-columns:
      var(--nav-w, 200px)
      calc(100vw - var(--nav-w, 200px))
      0px;
  }

  .detail-pane {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--surface-2);
    overflow: hidden;
    opacity: 1;
    transform: translateX(0);
    will-change: transform, opacity;
    transition:
      opacity 240ms cubic-bezier(0.4, 0, 0.2, 1),
      transform 360ms cubic-bezier(0.4, 0, 0.2, 1);

    &.closed {
      opacity: 0;
      transform: translateX(32px);
      pointer-events: none;
    }
  }

  .detail-pane > :global(*) {
    min-width: min(360px, calc(100vw - var(--nav-w, 200px) - 280px));
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
    gap: $space-3;
    text-align: center;
    padding: var(--list-head-height) $space-6 0;
    color: var(--text-muted);
  }

  .empty-mark {
    width: 48px;
    height: 48px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--surface-3);
    border: 1px solid var(--border);
    color: var(--text-faint);
  }

  .detail-empty h3 {
    font-size: $fs-md;
    color: var(--text-strong);
    margin: 0;
  }

  .detail-empty p {
    margin: 0;
    max-width: 280px;
    font-size: $fs-sm;
    line-height: $lh-normal;
    color: var(--text-muted);
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
    max-width: min(90vw, 540px);
    animation: toast-in $dur-base $ease-out;
  }

  .update-banner {
    position: fixed;
    top: 12px;
    right: 16px;
    z-index: 45;
    display: flex;
    align-items: flex-start;
    gap: $space-3;
    width: min(540px, calc(100vw - 32px));
    padding: $space-3 $space-3 $space-3 $space-4;
    border: 1px solid var(--accent);
    border-radius: $radius-md;
    background: var(--elevated);
    box-shadow: var(--shadow-lg), var(--inner-highlight);
    color: var(--text);
  }

  .update-icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 28px;
    height: 28px;
    border-radius: $radius-sm;
    background: var(--accent-soft);
    color: var(--accent);
  }

  .update-copy {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
    font-size: $fs-sm;
  }

  .update-copy strong,
  .update-copy span {
    overflow-wrap: anywhere;
  }

  .update-copy strong { color: var(--text-strong); }
  .update-copy span { color: var(--text-muted); }
  .update-copy .update-error { color: var(--state-bad); }

  .update-actions {
    display: flex;
    align-items: center;
    gap: $space-2;
    flex: none;
  }

  .update-install,
  .update-dismiss {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    min-height: 28px;
    border: 1px solid var(--border);
    border-radius: $radius-sm;
    cursor: pointer;
  }

  .update-install {
    padding: 0 9px;
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-contrast);
    font-size: $fs-xs;
    font-weight: $fw-semibold;
  }

  .update-dismiss {
    width: 28px;
    background: transparent;
    color: var(--text-muted);
  }

  .update-dismiss:hover { color: var(--text-strong); background: var(--surface-3); }
  .update-install:disabled,
  .update-dismiss:disabled { opacity: 0.55; cursor: not-allowed; }

  .toast-text {
    color: var(--text);
    word-break: break-word;
    flex: 1;
    min-width: 0;
  }

  .toast-close {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    flex: none;
    border-radius: $radius-xs;
    border: none;
    background: transparent;
    color: inherit;
    cursor: pointer;
    transition: background $dur-fast $ease-out;
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

  @media (max-width: 980px) {
    .shell {
      grid-template-columns: var(--nav-w, 200px) 1fr;
    }
    .shell:has(.detail-pane.closed) {
      grid-template-columns: var(--nav-w, 200px) 1fr;
    }
    .detail-pane {
      position: absolute;
      right: 0;
      top: 0;
      bottom: 0;
      width: min(420px, 100%);
      z-index: 20;
      border-left: 1px solid var(--border);
      box-shadow: var(--shadow-lg);
    }
    .detail-pane.closed {
      transform: translateX(100%);
    }
    .detail-pane.no-selection {
      display: none;
    }
  }

  @media (max-width: 760px) {
    .shell {
      --nav-w: 168px;
      grid-template-columns: var(--nav-w) minmax(0, 1fr);
    }
    .detail-pane {
      left: var(--nav-w);
      right: 0;
      width: auto;
    }
  }
</style>
