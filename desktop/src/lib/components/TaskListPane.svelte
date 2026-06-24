<script lang="ts">
  import {
    Activity,
    Inbox,
    PanelRightClose,
    PanelRightOpen,
    Pause,
    Play,
    Plus,
    RefreshCw,
    Search,
    Square,
    Trash2
  } from '@lucide/svelte';
  import type { RuntimeTask, SidebarFilter, TaskState } from '../types';
  import { canPause, canStart, canStop } from '../format';
  import { taskSpeeds, trashedTaskIds } from '../stores/tasks';
  import TaskRow from './TaskRow.svelte';
  import Select from './common/Select.svelte';

  type SortMode = 'created' | 'speed' | 'name' | 'manual';
  type TaskAction = 'start' | 'pause' | 'stop' | 'delete';

  const sortOptions: { value: SortMode; label: string }[] = [
    { value: 'created', label: 'Added time' },
    { value: 'speed', label: 'Speed' },
    { value: 'name', label: 'Name' },
    { value: 'manual', label: 'Manual' }
  ];

  export let tasks: RuntimeTask[];
  export let loading: boolean;
  export let selectedTaskId: string;
  export let busy: boolean;
  export let detailOpen: boolean;
  export let refreshing = false;
  export let statusFilter: TaskState | 'all' = 'all';
  export let sidebarFilter: SidebarFilter = 'all';

  export let onSelect: (id: string) => void | Promise<void> = () => {};
  export let onTaskAction: (ids: string[], action: TaskAction) => void | Promise<void> = () => {};
  export let onNewTask: () => void = () => {};
  export let onRefresh: () => void = () => {};
  export let onToggleDetail: () => void = () => {};
  export let onClearTrash: () => void = () => {};

  let filter = '';
  let sortMode: SortMode = 'created';
  let selectedIds: string[] = [];
  let anchorId = '';
  let manualOrder: string[] = [];
  let draggingId = '';
  let dropTargetId = '';
  let contextMenu: { open: boolean; x: number; y: number; taskId: string } = {
    open: false,
    x: 0,
    y: 0,
    taskId: ''
  };

  $: speeds = $taskSpeeds;
  $: trashIds = $trashedTaskIds;
  $: trashIdSet = new Set(trashIds);
  $: isTrashView = sidebarFilter === 'trash';
  $: title = isTrashView ? 'Trash' : 'Downloads';
  $: visibleBase = filterTasks(tasks, trashIdSet, isTrashView, statusFilter, filter);
  $: visibleTasks = sortTasks(visibleBase, sortMode, manualOrder, speeds);
  $: visibleIds = visibleTasks.map((task) => task.id);
  $: visibleActiveCount = visibleBase.filter((task) =>
    task.state === 'Downloading' || task.state === 'Seeding' || task.state === 'Resolving' || task.state === 'Queued'
  ).length;
  $: selectedCount = selectedIds.length;
  $: contextIds = idsForContextMenu(contextMenu, selectedIds);
  $: contextTasks = contextIds.map((id) => tasks.find((task) => task.id === id)).filter(Boolean) as RuntimeTask[];
  $: contextStartText = contextStartLabel(contextTasks);
  $: contextStartDisabled = busy || isTrashView || !contextTasks.some((task) => canStart(task.state));
  $: contextPauseDisabled = busy || isTrashView || !contextTasks.some((task) => canPause(task.state));
  $: contextStopDisabled = busy || isTrashView || !contextTasks.some((task) => canStop(task.state));
  $: contextDeleteDisabled = busy || contextTasks.length === 0;
  $: canPauseSelection = selectedIds.some((id) => canPause(tasks.find((task) => task.id === id)?.state ?? 'Completed'));
  $: emptyTitle = isTrashView ? 'Trash is empty' : tasks.length === 0 ? 'No downloads' : 'No matches';
  $: emptyCopy = isTrashView
    ? 'Deleted tasks will wait here until you clear them.'
    : tasks.length === 0
      ? 'Create a new task to get started.'
      : 'Adjust your filters to see more.';

  $: {
    const allIds = tasks.map((task) => task.id);
    const allIdSet = new Set(allIds);
    const nextManual = [
      ...manualOrder.filter((id) => allIdSet.has(id)),
      ...allIds.filter((id) => !manualOrder.includes(id))
    ];
    if (nextManual.length !== manualOrder.length || nextManual.some((id, index) => id !== manualOrder[index])) {
      manualOrder = nextManual;
    }
  }

  $: {
    const visibleIdSet = new Set(visibleIds);
    const nextSelected = selectedIds.filter((id) => visibleIdSet.has(id));
    if (nextSelected.length !== selectedIds.length) {
      selectedIds = nextSelected;
    }
    if (anchorId && !visibleIdSet.has(anchorId)) {
      anchorId = nextSelected[0] ?? '';
    }
  }

  $: if (selectedTaskId && visibleIds.includes(selectedTaskId) && selectedIds.length === 0) {
    selectedIds = [selectedTaskId];
    anchorId = selectedTaskId;
  }

  function filterTasks(
    source: RuntimeTask[],
    trashed: Set<string>,
    trashView: boolean,
    stateFilter: TaskState | 'all',
    search: string
  ) {
    const needle = search.trim().toLowerCase();
    return source.filter((task) => {
      const isTrashed = trashed.has(task.id);
      if (trashView) {
        if (!isTrashed) return false;
      } else {
        if (isTrashed) return false;
        if (stateFilter !== 'all' && task.state !== stateFilter) return false;
      }

      if (!needle) return true;
      const haystack = `${task.file_name ?? ''} ${task.id} ${task.state} ${task.kind}`.toLowerCase();
      return haystack.includes(needle);
    });
  }

  function sortTasks(list: RuntimeTask[], mode: SortMode, order: string[], speedMap: typeof speeds) {
    const sorted = list.slice();
    if (mode === 'manual') {
      const index = new Map(order.map((id, position) => [id, position]));
      return sorted.sort((a, b) => (index.get(a.id) ?? Number.MAX_SAFE_INTEGER) - (index.get(b.id) ?? Number.MAX_SAFE_INTEGER));
    }
    if (mode === 'speed') {
      return sorted.sort((a, b) => speedFor(b.id, speedMap) - speedFor(a.id, speedMap) || b.created_at.localeCompare(a.created_at));
    }
    if (mode === 'name') {
      return sorted.sort((a, b) => (a.file_name ?? a.id).localeCompare(b.file_name ?? b.id));
    }
    return sorted.sort((a, b) => b.created_at.localeCompare(a.created_at));
  }

  function speedFor(taskId: string, speedMap = speeds) {
    return (speedMap[taskId]?.down ?? 0) + (speedMap[taskId]?.up ?? 0);
  }

  function toggleId(id: string) {
    return selectedIds.includes(id)
      ? selectedIds.filter((selectedId) => selectedId !== id)
      : [...selectedIds, id];
  }

  function selectRange(id: string) {
    const from = visibleIds.indexOf(anchorId || id);
    const to = visibleIds.indexOf(id);
    if (from === -1 || to === -1) return [id];
    const [start, end] = from < to ? [from, to] : [to, from];
    return visibleIds.slice(start, end + 1);
  }

  function handleSelect(task: RuntimeTask, event: MouseEvent | KeyboardEvent) {
    closeContextMenu();
    const additive = 'metaKey' in event && (event.metaKey || event.ctrlKey);
    const range = 'shiftKey' in event && event.shiftKey;
    if (range) {
      selectedIds = selectRange(task.id);
    } else if (additive) {
      selectedIds = toggleId(task.id);
      anchorId = task.id;
    } else {
      selectedIds = [task.id];
      anchorId = task.id;
    }
    void onSelect(task.id);
  }

  function openContextMenu(task: RuntimeTask, event: MouseEvent) {
    event.preventDefault();
    if (!selectedIds.includes(task.id)) {
      selectedIds = [task.id];
      anchorId = task.id;
    }
    void onSelect(task.id);
    contextMenu = {
      open: true,
      x: Math.min(event.clientX, window.innerWidth - 190),
      y: Math.min(event.clientY, window.innerHeight - 170),
      taskId: task.id
    };
  }

  function closeContextMenu() {
    if (contextMenu.open) {
      contextMenu = { ...contextMenu, open: false };
    }
  }

  function idsForContextMenu(menu: typeof contextMenu, selection: string[]) {
    if (!menu.taskId) return [];
    return selection.includes(menu.taskId) ? selection : [menu.taskId];
  }

  function runContextAction(action: TaskAction) {
    const ids = contextIds;
    closeContextMenu();
    void onTaskAction(ids, action);
  }

  function runPauseSelected() {
    if (selectedIds.length === 0) return;
    void onTaskAction(selectedIds, 'pause');
  }

  function contextStartLabel(items: RuntimeTask[]) {
    if (items.some((task) => task.state === 'Stopped')) return 'Download again';
    if (items.some((task) => task.state === 'Failed')) return 'Retry';
    return 'Start';
  }

  function onDragStart(task: RuntimeTask, event: DragEvent) {
    draggingId = task.id;
    if (!selectedIds.includes(task.id)) {
      selectedIds = [task.id];
      anchorId = task.id;
    }
    event.dataTransfer?.setData('text/plain', task.id);
    if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move';
  }

  function onDragOver(task: RuntimeTask, event: DragEvent) {
    if (!draggingId || draggingId === task.id) return;
    event.preventDefault();
    dropTargetId = task.id;
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';
  }

  function onDrop(task: RuntimeTask, event: DragEvent) {
    event.preventDefault();
    if (!draggingId || draggingId === task.id) {
      clearDrag();
      return;
    }
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    reorderManual(draggingId, task.id, event.clientY > rect.top + rect.height / 2);
    sortMode = 'manual';
    clearDrag();
  }

  function reorderManual(sourceId: string, targetId: string, after: boolean) {
    const next = manualOrder.filter((id) => id !== sourceId);
    const targetIndex = next.indexOf(targetId);
    const insertAt = targetIndex === -1 ? next.length : targetIndex + (after ? 1 : 0);
    next.splice(insertAt, 0, sourceId);
    manualOrder = next;
  }

  function clearDrag() {
    draggingId = '';
    dropTargetId = '';
  }
</script>

<svelte:window on:click={closeContextMenu} on:keydown={(e) => e.key === 'Escape' && closeContextMenu()} />

<section class="list-pane">
  <header class="pane-head" data-tauri-drag-region="true">
    <div class="title-row">
      <h2>{title}</h2>
      {#if visibleActiveCount > 0 && !isTrashView}
        <span class="active-capsule" title="{visibleActiveCount} active task{visibleActiveCount === 1 ? '' : 's'}">
          <span class="capsule-dot"></span>
          {visibleActiveCount} active
        </span>
      {/if}
      {#if selectedCount > 1}
        <span class="selected-capsule">{selectedCount} selected</span>
      {/if}
      <span class="sort-wrap">
        <Select
          items={sortOptions}
          bind:value={sortMode}
          placeholder="Sort"
          size="sm"
          ariaLabel="Sort tasks"
        />
      </span>
      <div class="head-tools">
        {#if isTrashView}
          <button class="clear-btn danger" on:click={onClearTrash} disabled={trashIds.length === 0 || busy} title="Clear Trash">
            <Trash2 size={14} />
            <span>Clear</span>
          </button>
        {:else}
          <button class="icon-btn" on:click={onNewTask} title="New task" aria-label="New task">
            <Plus size={16} />
          </button>
          <button
            class="icon-btn"
            on:click={runPauseSelected}
            disabled={!canPauseSelection || busy}
            title="Pause selected"
            aria-label="Pause selected"
          >
            <Pause size={15} />
          </button>
        {/if}
        <button class="icon-btn" on:click={onRefresh} disabled={refreshing} title="Refresh" aria-label="Refresh">
          <RefreshCw size={15} class={refreshing ? 'spin' : ''} />
        </button>
        <button class="icon-btn" on:click={onToggleDetail} title={detailOpen ? 'Hide detail' : 'Show detail'} aria-label="Toggle detail">
          {#if detailOpen}
            <PanelRightClose size={16} />
          {:else}
            <PanelRightOpen size={16} />
          {/if}
        </button>
      </div>
    </div>
    <div class="search-row">
      <div class="head-search">
        <Search size={13} />
        <input
          type="text"
          placeholder="Search..."
          value={filter}
          on:input={(e) => (filter = e.currentTarget.value)}
        />
      </div>
    </div>
  </header>

  <div class="list" on:scroll={closeContextMenu}>
    {#if loading}
      {#each Array(5) as _, i}
        <div class="skeleton" aria-hidden="true">
          <div class="sk-line w-60"></div>
          <div class="sk-bar"></div>
          <div class="sk-line w-40"></div>
        </div>
      {/each}
    {:else if visibleTasks.length === 0}
      <div class="empty">
        <div class="empty-icon"><Inbox size={26} /></div>
        <h3>{emptyTitle}</h3>
        <p>{emptyCopy}</p>
      </div>
    {:else}
      {#each visibleTasks as task (task.id)}
        <TaskRow
          {task}
          selected={selectedIds.includes(task.id)}
          downSpeed={speeds[task.id]?.down ?? 0}
          upSpeed={speeds[task.id]?.up ?? 0}
          {busy}
          draggableRow={true}
          dragging={draggingId === task.id}
          dropTarget={dropTargetId === task.id}
          select={(event) => handleSelect(task, event)}
          openContextMenu={(event) => openContextMenu(task, event)}
          dragStart={(event) => onDragStart(task, event)}
          dragOver={(event) => onDragOver(task, event)}
          dragLeave={() => {
            if (dropTargetId === task.id) dropTargetId = '';
          }}
          drop={(event) => onDrop(task, event)}
          dragEnd={clearDrag}
        />
      {/each}
    {/if}
  </div>
</section>

{#if contextMenu.open}
  <div
    class="context-menu"
    style={`left:${contextMenu.x}px; top:${contextMenu.y}px;`}
    role="menu"
    tabindex="-1"
    on:click|stopPropagation
    on:keydown={(e) => e.key === 'Escape' && closeContextMenu()}
    on:contextmenu|preventDefault
  >
    {#if !isTrashView}
      <button class="context-item" disabled={contextStartDisabled} on:click={() => runContextAction('start')} role="menuitem">
        <Play size={14} /> {contextStartText}
      </button>
      <button class="context-item" disabled={contextPauseDisabled} on:click={() => runContextAction('pause')} role="menuitem">
        <Pause size={14} /> Pause
      </button>
      <button class="context-item" disabled={contextStopDisabled} on:click={() => runContextAction('stop')} role="menuitem">
        <Square size={14} /> Stop
      </button>
      <div class="context-sep" aria-hidden="true"></div>
    {/if}
    <button class="context-item danger" disabled={contextDeleteDisabled} on:click={() => runContextAction('delete')} role="menuitem">
      <Trash2 size={14} /> {isTrashView ? 'Delete permanently' : 'Delete'}
    </button>
  </div>
{/if}

<style lang="scss">
  @use '../../styles/tokens' as *;

  .list-pane {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    background: var(--surface-2);
    border-right: 1px solid var(--border);
    box-shadow: var(--inner-highlight);
    overflow: hidden;
  }

  .pane-head {
    display: flex;
    flex-direction: column;
    gap: $space-2;
    flex: none;
    padding: $space-3 $space-3 $space-2 $space-4;
    border-bottom: 1px solid var(--border);
    background: linear-gradient(180deg, color-mix(in srgb, var(--surface-2) 80%, var(--surface) 20%), var(--surface-2));
    -webkit-app-region: drag;
  }

  .title-row {
    display: flex;
    align-items: center;
    gap: $space-2;
    min-width: 0;
  }

  .pane-head h2 {
    font-size: $fs-md;
    letter-spacing: 0;
    flex: none;
  }

  .active-capsule,
  .selected-capsule {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex: none;
    padding: 2px $space-2;
    border-radius: $radius-pill;
    background: var(--surface-3);
    border: 1px solid var(--border);
    font-size: $fs-xs;
    font-weight: $fw-medium;
    color: var(--text-muted);
    line-height: 1.5;
  }

  .selected-capsule {
    color: var(--accent);
    background: var(--accent-soft);
    border-color: rgba(249, 115, 22, 0.24);
  }

  .capsule-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--state-good);
    flex: none;
  }

  .search-row {
    display: flex;
    min-width: 0;
    -webkit-app-region: no-drag;
  }

  .head-search {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 $space-2;
    height: 32px;
    flex: 1;
    min-width: 0;
    border-radius: $radius-sm;
    background: var(--surface-3);
    border: 1px solid var(--border);
    color: var(--text-muted);
    transition: border-color $dur-fast $ease-out, box-shadow $dur-fast $ease-out;
    -webkit-app-region: no-drag;

    &:focus-within {
      border-color: var(--accent);
      box-shadow: 0 0 0 2px var(--focus-ring);
      color: var(--text);
    }

    input {
      flex: 1;
      min-width: 0;
      border: none;
      background: transparent;
      color: var(--text);
      outline: none;
      font-size: $fs-sm;

      &::placeholder {
        color: var(--text-faint);
      }
    }
  }

  .head-tools {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    margin-left: auto;
    -webkit-app-region: no-drag;
  }

  .sort-wrap {
    display: inline-flex;
    align-items: center;
    margin-left: auto;
    min-width: 116px;
    -webkit-app-region: no-drag;

    :global(.fx-select-trigger) {
      flex: 1;
      width: 100%;
    }
  }

  .icon-btn,
  .clear-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 30px;
    border-radius: $radius-sm;
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    transition: background $dur-fast $ease-out, color $dur-fast $ease-out,
      border-color $dur-fast $ease-out, box-shadow $dur-fast $ease-out;
    @include focus-ring;

    &:not(:disabled):hover {
      background: var(--surface-3);
      color: var(--text-strong);
      border-color: var(--border);
      box-shadow: var(--shadow-sm);
    }
    &:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
  }

  .icon-btn {
    width: 30px;
  }

  .clear-btn {
    gap: 6px;
    padding: 0 $space-2;
    font-size: $fs-sm;
    font-weight: $fw-medium;
  }

  .clear-btn.danger {
    color: var(--state-bad);
    border-color: rgba(248, 113, 113, 0.24);

    &:not(:disabled):hover {
      background: var(--state-bad-bg);
      color: var(--state-bad);
      border-color: var(--state-bad);
    }
  }

  .list {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: $space-1;
    overflow-y: auto;
    overflow-x: hidden;
    padding: $space-2;
    min-height: 0;
    @include scrollbar;
  }

  .empty {
    display: grid;
    place-items: center;
    gap: $space-2;
    padding: $space-12 $space-6;
    text-align: center;
    color: var(--text-muted);
  }

  .empty-icon {
    width: 52px;
    height: 52px;
    border-radius: $radius-xl;
    display: grid;
    place-items: center;
    background: var(--surface-3);
    color: var(--text-faint);
    margin-bottom: $space-1;
  }

  .empty h3 {
    font-size: $fs-base;
    color: var(--text-strong);
  }

  .empty p {
    font-size: $fs-sm;
  }

  .skeleton {
    display: grid;
    gap: $space-2;
    padding: $space-3 $space-4;
    border-radius: $radius-md;
    border: 1px solid var(--border);
    background: var(--surface);
  }

  .sk-line,
  .sk-bar {
    border-radius: $radius-xs;
    background: linear-gradient(
      90deg,
      var(--skeleton) 0%,
      var(--border-strong) 50%,
      var(--skeleton) 100%
    );
    background-size: 200% 100%;
    animation: shimmer 1.4s ease-in-out infinite;
  }

  .sk-line {
    height: 11px;
  }
  .sk-line.w-60 {
    width: 60%;
  }
  .sk-line.w-40 {
    width: 40%;
  }
  .sk-bar {
    height: 5px;
    width: 100%;
  }

  @keyframes shimmer {
    0% {
      background-position: 200% 0;
    }
    100% {
      background-position: -200% 0;
    }
  }

  .context-menu {
    position: fixed;
    z-index: 70;
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 184px;
    padding: $space-1;
    border-radius: $radius-md;
    background: var(--elevated);
    border: 1px solid var(--border-strong);
    box-shadow: var(--shadow-lg);
    animation: menu-in $dur-fast $ease-out;
  }

  @keyframes menu-in {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .context-item {
    display: flex;
    align-items: center;
    gap: $space-2;
    padding: 7px $space-2;
    border: none;
    border-radius: $radius-sm;
    background: transparent;
    color: var(--text);
    cursor: pointer;
    font-size: $fs-sm;
    text-align: left;
    transition: background $dur-fast $ease-out, color $dur-fast $ease-out;

    &:not(:disabled):hover {
      background: var(--surface-3);
    }
    &:disabled {
      opacity: 0.4;
      cursor: not-allowed;
    }
    &.danger {
      color: var(--state-bad);
      &:not(:disabled):hover {
        background: var(--state-bad-bg);
        color: var(--state-bad);
      }
    }
  }

  .context-sep {
    height: 1px;
    background: var(--border);
    margin: 2px 0;
  }

  :global(.spin) {
    animation: spin 900ms linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (max-width: 900px) {
    .active-capsule,
    .selected-capsule {
      display: none;
    }

    .sort-wrap {
      min-width: 98px;
    }
  }
</style>
