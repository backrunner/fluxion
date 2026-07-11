<script lang="ts">
  import {
    Check,
    DownloadCloud,
    ListFilter,
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
  import { DropdownMenu } from 'bits-ui';
  import type { RuntimeTask, SidebarFilter, TaskState } from '../types';
  import { canPause, canStart, canStop } from '../format';
  import { taskSpeeds, trashedTaskIds } from '../stores/tasks';
  import TaskRow from './TaskRow.svelte';
  import Select from './common/Select.svelte';
  import { t, type Translate } from '../i18n';
  import { windowDrag } from '../windowDrag';

  type SortMode = 'created' | 'speed' | 'name' | 'manual';
  type TaskAction = 'start' | 'pause' | 'stop' | 'delete';
  type StatusFilter = 'all' | 'active' | 'completed' | 'failed' | 'stopped';

  const ROW_HEIGHT = 74;
  const VIRTUALIZE_AFTER = 60;
  const OVERSCAN = 5;

  let sortOptions: { value: SortMode; label: string }[] = [];
  $: sortOptions = [
    { value: 'created', label: $t('list.sort.added') },
    { value: 'speed', label: $t('list.sort.speed') },
    { value: 'name', label: $t('list.sort.name') },
    { value: 'manual', label: $t('list.sort.manual') }
  ];

  export let tasks: RuntimeTask[];
  export let loading: boolean;
  export let selectedTaskId: string;
  export let busy: boolean;
  export let detailOpen: boolean;
  export let refreshing = false;
  export let sidebarFilter: SidebarFilter = 'all';

  export let onSelect: (id: string) => void | Promise<void> = () => {};
  export let onTaskAction: (ids: string[], action: TaskAction) => void | Promise<void> = () => {};
  export let onNewTask: () => void = () => {};
  export let onRefresh: () => void = () => {};
  export let onToggleDetail: () => void = () => {};
  export let onClearTrash: () => void = () => {};

  let filter = '';
  let statusFilter: StatusFilter = 'all';
  let sortMode: SortMode = 'created';
  let selectedIds: string[] = [];
  let anchorId = '';
  let manualOrder: string[] = [];
  let draggingId = '';
  let dropTargetId = '';
  let listScrollTop = 0;
  let listViewportHeight = 0;
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
  $: title = isTrashView ? $t('nav.trash') : $t('nav.downloads');
  $: normalTasks = tasks.filter((task) => !trashIdSet.has(task.id));
  $: hasTasksInView = isTrashView ? tasks.some((task) => trashIdSet.has(task.id)) : normalTasks.length > 0;
  $: filterOptions = buildFilterOptions(normalTasks, $t);
  $: activeFilterLabel = filterOptions.find((item) => item.value === statusFilter)?.label ?? $t('list.filter.all');
  $: visibleBase = filterTasks(tasks, trashIdSet, isTrashView, statusFilter, filter);
  $: visibleTasks = sortTasks(visibleBase, sortMode, manualOrder, speeds);
  $: virtualized = visibleTasks.length > VIRTUALIZE_AFTER;
  $: virtualStart = virtualized
    ? Math.min(visibleTasks.length, Math.max(0, Math.floor(listScrollTop / ROW_HEIGHT) - OVERSCAN))
    : 0;
  $: virtualEnd = virtualized
    ? Math.min(visibleTasks.length, Math.ceil((listScrollTop + listViewportHeight) / ROW_HEIGHT) + OVERSCAN)
    : visibleTasks.length;
  $: renderedTasks = visibleTasks.slice(virtualStart, virtualEnd);
  $: virtualBefore = virtualStart * ROW_HEIGHT;
  $: virtualAfter = Math.max(0, (visibleTasks.length - virtualEnd) * ROW_HEIGHT);
  $: visibleIds = visibleTasks.map((task) => task.id);
  $: selectedCount = selectedIds.length;
  $: contextIds = idsForContextMenu(contextMenu, selectedIds);
  $: contextTasks = contextIds.map((id) => tasks.find((task) => task.id === id)).filter(Boolean) as RuntimeTask[];
  $: contextStartText = contextStartLabel(contextTasks, $t);
  $: contextStartDisabled = busy || isTrashView || !contextTasks.some((task) => canStart(task.state));
  $: contextPauseDisabled = busy || isTrashView || !contextTasks.some((task) => canPause(task.state));
  $: contextStopDisabled = busy || isTrashView || !contextTasks.some((task) => canStop(task.state));
  $: contextDeleteDisabled = busy || contextTasks.length === 0;
  $: canPauseSelection = selectedIds.some((id) => canPause(tasks.find((task) => task.id === id)?.state ?? 'Completed'));
  $: emptyTitle = isTrashView ? $t('list.empty.trash.title') : normalTasks.length === 0 ? $t('list.empty.downloads.title') : $t('list.empty.matches.title');
  $: emptyCopy = isTrashView
    ? $t('list.empty.trash.copy')
    : normalTasks.length === 0
      ? $t('list.empty.downloads.copy')
      : $t('list.empty.matches.copy');

  function buildFilterOptions(source: RuntimeTask[], translate: Translate) {
    return [
      { value: 'all' as const, label: translate('list.filter.all'), count: source.length },
      { value: 'active' as const, label: translate('list.filter.active'), count: source.filter((task) => isActiveState(task.state)).length },
      { value: 'completed' as const, label: translate('list.filter.completed'), count: source.filter((task) => task.state === 'Completed').length },
      { value: 'failed' as const, label: translate('list.filter.failed'), count: source.filter((task) => task.state === 'Failed').length },
      { value: 'stopped' as const, label: translate('list.filter.stopped'), count: source.filter((task) => task.state === 'Stopped').length }
    ];
  }

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
    stateFilter: StatusFilter,
    search: string
  ) {
    const needle = search.trim().toLowerCase();
    return source.filter((task) => {
      const isTrashed = trashed.has(task.id);
      if (trashView) {
        if (!isTrashed) return false;
      } else {
        if (isTrashed) return false;
        if (!matchesStatusFilter(task.state, stateFilter)) return false;
      }

      if (!needle) return true;
      const haystack = `${task.file_name ?? ''} ${task.id} ${task.state} ${task.kind}`.toLowerCase();
      return haystack.includes(needle);
    });
  }

  function matchesStatusFilter(state: TaskState, stateFilter: StatusFilter) {
    if (stateFilter === 'all') return true;
    if (stateFilter === 'active') return isActiveState(state);
    if (stateFilter === 'completed') return state === 'Completed';
    if (stateFilter === 'failed') return state === 'Failed';
    if (stateFilter === 'stopped') return state === 'Stopped';
    return true;
  }

  function isActiveState(state: TaskState) {
    return state === 'Downloading' || state === 'Seeding' || state === 'Resolving' || state === 'Queued' || state === 'Verifying';
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

  function handleListScroll(event: Event) {
    const viewport = event.currentTarget as HTMLDivElement;
    listScrollTop = viewport.scrollTop;
    listViewportHeight = viewport.clientHeight;
    closeContextMenu();
  }

  function idsForContextMenu(menu: typeof contextMenu, selection: string[]) {
    if (!menu.taskId) return [];
    return selection.includes(menu.taskId) ? selection : [menu.taskId];
  }

  function runContextAction(action: TaskAction) {
    const ids = eligibleIds(contextIds, action);
    closeContextMenu();
    void onTaskAction(ids, action);
  }

  function runPauseSelected() {
    const ids = eligibleIds(selectedIds, 'pause');
    if (ids.length === 0) return;
    void onTaskAction(ids, 'pause');
  }

  function eligibleIds(ids: string[], action: TaskAction) {
    if (action === 'delete') return ids;
    return ids.filter((id) => {
      const task = tasks.find((item) => item.id === id);
      if (!task) return false;
      if (action === 'start') return canStart(task.state);
      if (action === 'pause') return canPause(task.state);
      return canStop(task.state);
    });
  }

  function contextStartLabel(items: RuntimeTask[], translate: Translate) {
    if (items.some((task) => task.state === 'Stopped')) return translate('list.action.downloadAgain');
    if (items.some((task) => task.state === 'Failed')) return translate('list.action.retry');
    return translate('list.action.start');
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
  <header class="pane-head" data-tauri-drag-region="true" use:windowDrag>
    <div class="title-row" data-tauri-drag-region="true">
      <h2 data-tauri-drag-region="true">{title}</h2>
      {#if selectedCount > 1}
        <span class="selected-capsule">{$t('list.selected', { count: selectedCount })}</span>
      {/if}
      <div class="head-tools" data-tauri-drag-region="true">
        {#if isTrashView}
          <button class="clear-btn danger" on:click={onClearTrash} disabled={trashIds.length === 0 || busy} title={$t('list.clearTrash')}>
            <Trash2 size={14} />
            <span>{$t('list.clear')}</span>
          </button>
        {:else}
          <button class="new-btn" on:click={onNewTask} title={$t('list.newDownload')} aria-label={$t('list.newDownload')}>
            <Plus size={17} />
          </button>
        {/if}
        {#if hasTasksInView}
          <button class="icon-btn" on:click={onToggleDetail} title={detailOpen ? $t('list.hideDetail') : $t('list.showDetail')} aria-label={$t('list.toggleDetail')}>
            {#if detailOpen}
              <PanelRightClose size={16} />
            {:else}
              <PanelRightOpen size={16} />
            {/if}
          </button>
        {/if}
      </div>
    </div>
    <div class="search-row" data-tauri-drag-region="true">
      <div class="head-search">
        <Search size={13} />
        <input
          type="text"
          placeholder={$t('list.search')}
          value={filter}
          on:input={(e) => (filter = e.currentTarget.value)}
        />
      </div>
      <span class="sort-wrap">
        <Select
          items={sortOptions}
          bind:value={sortMode}
          placeholder={$t('list.sort')}
          size="sm"
          ariaLabel={$t('list.sortTasks')}
        />
      </span>
      {#if !isTrashView}
        <button
          class="icon-btn"
          on:click={runPauseSelected}
          disabled={!canPauseSelection || busy}
          title={$t('list.pauseSelected')}
          aria-label={$t('list.pauseSelected')}
        >
          <Pause size={15} />
        </button>
        <DropdownMenu.Root>
          <DropdownMenu.Trigger
            class={`filter-trigger ${statusFilter !== 'all' ? 'active' : ''}`}
            title={$t('list.filterTitle', { filter: activeFilterLabel })}
            aria-label={$t('list.filterAria', { filter: activeFilterLabel })}
          >
            <ListFilter size={15} />
          </DropdownMenu.Trigger>
          <DropdownMenu.Portal>
            <DropdownMenu.Content class="filter-menu" align="end" sideOffset={6}>
              {#each filterOptions as option (option.value)}
                <DropdownMenu.Item
                  class="filter-menu-item"
                  textValue={option.label}
                  onSelect={() => (statusFilter = option.value)}
                >
                  <span class="filter-menu-label">{option.label}</span>
                  <span class="filter-menu-count">{option.count}</span>
                  <span class="filter-menu-check" class:shown={statusFilter === option.value}>
                    <Check size={13} />
                  </span>
                </DropdownMenu.Item>
              {/each}
            </DropdownMenu.Content>
          </DropdownMenu.Portal>
        </DropdownMenu.Root>
      {/if}
      <button class="icon-btn" on:click={onRefresh} disabled={refreshing} title={$t('common.refresh')} aria-label={$t('common.refresh')}>
        <RefreshCw size={15} class={refreshing ? 'spin' : ''} />
      </button>
    </div>
  </header>

  <div
    class="list"
    bind:clientHeight={listViewportHeight}
    on:scroll={handleListScroll}
  >
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
        <div class="empty-icon">{#if isTrashView}<Trash2 size={26} />{:else}<DownloadCloud size={26} />{/if}</div>
        <h3>{emptyTitle}</h3>
        <p>{emptyCopy}</p>
      </div>
    {:else}
      {#if virtualized && virtualBefore > 0}
        <div class="virtual-spacer" style={`height:${virtualBefore}px;`} aria-hidden="true"></div>
      {/if}
      {#each renderedTasks as task (task.id)}
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
      {#if virtualized && virtualAfter > 0}
        <div class="virtual-spacer" style={`height:${virtualAfter}px;`} aria-hidden="true"></div>
      {/if}
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
        <Pause size={14} /> {$t('list.action.pause')}
      </button>
      <button class="context-item" disabled={contextStopDisabled} on:click={() => runContextAction('stop')} role="menuitem">
        <Square size={14} /> {$t('list.action.stop')}
      </button>
      <div class="context-sep" aria-hidden="true"></div>
    {/if}
    <button class="context-item danger" disabled={contextDeleteDisabled} on:click={() => runContextAction('delete')} role="menuitem">
      <Trash2 size={14} /> {isTrashView ? $t('list.action.deletePermanent') : $t('common.delete')}
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
    background: var(--surface);
    border-right: 1px solid var(--border);
    overflow: hidden;
  }

  .pane-head {
    display: flex;
    flex-direction: column;
    gap: 7px;
    flex: none;
    padding: 9px 12px 8px 14px;
    border-bottom: 1px solid var(--border);
    background: linear-gradient(180deg, var(--surface), var(--surface-2));
    box-shadow: var(--inner-highlight);
  }

  .title-row {
    display: flex;
    align-items: center;
    gap: $space-2;
    min-width: 0;
    min-height: 30px;
  }

  .pane-head h2 {
    font-size: 15px;
    letter-spacing: 0;
    flex: none;
  }

  .selected-capsule {
    display: inline-flex;
    align-items: center;
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
    border-color: var(--accent-soft-strong);
    box-shadow: var(--inner-highlight);
  }

  .search-row {
    display: flex;
    align-items: center;
    gap: $space-1;
    min-width: 0;
  }

  .head-search {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 $space-2;
    height: 30px;
    flex: 1;
    min-width: 0;
    border-radius: $radius-lg;
    @include glass-control;
    color: var(--text-muted);
    transition: border-color $dur-fast $ease-out, box-shadow $dur-fast $ease-out, background $dur-fast $ease-out;

    &:hover:not(:focus-within) {
      border-color: transparent;
      transform: none;
    }
    &:focus-within {
      border-color: transparent;
      background: var(--glass-fill-hover);
      box-shadow: 0 0 0 2px var(--focus-ring), var(--glass-rim),
        var(--glass-control-shadow);
      color: var(--text);
      transform: none;
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
  }

  .sort-wrap {
    display: inline-flex;
    align-items: center;
    flex: none;
    min-width: 108px;

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
    border-radius: $radius-md;
    @include glass-hover-control;
    color: var(--text-muted);
    cursor: pointer;
    transition: background $dur-fast $ease-out, color $dur-fast $ease-out,
      border-color $dur-fast $ease-out, box-shadow $dur-fast $ease-out;
    @include focus-ring;

    &:not(:disabled):hover {
      color: var(--text-strong);
    }
    &:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
  }

  .icon-btn {
    width: 30px;
  }

  .new-btn {
    @include primary-button;
    width: 30px;
    height: 30px;
    padding: 0;
    border-radius: $radius-lg;
    margin-right: 2px;
  }

  // Base styles for the bits-ui dropdown trigger must be :global — Svelte
  // scoping hashes don't reach elements rendered by child components, so the
  // scoped `.icon-btn` rules above never match it (it rendered as a bare
  // user-agent button otherwise).
  :global(.filter-trigger) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    border-radius: $radius-md;
    @include glass-hover-control;
    color: var(--text-muted);
    cursor: pointer;
    transition: background $dur-fast $ease-out, color $dur-fast $ease-out,
      border-color $dur-fast $ease-out, box-shadow $dur-fast $ease-out;
  }

  :global(.filter-trigger:hover) {
    color: var(--text-strong);
  }

  :global(.filter-trigger:focus-visible) {
    outline: none;
    box-shadow: 0 0 0 2px var(--focus-ring);
    border-color: var(--accent);
  }

  :global(.filter-trigger.active) {
    background: var(--selected-gradient);
    color: var(--accent);
    border-color: transparent;
    box-shadow: var(--glass-rim), var(--selected-shadow);
  }

  :global(.filter-menu) {
    z-index: 80;
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 172px;
    padding: $space-1;
    border-radius: $radius-md;
    background: var(--elevated);
    border: 1px solid var(--border-strong);
    box-shadow: var(--shadow-lg), var(--inner-highlight);
    animation: menu-in $dur-base $ease-out;
  }

  :global(.filter-menu-item) {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto 16px;
    align-items: center;
    gap: $space-2;
    padding: 8px $space-2;
    border-radius: $radius-sm;
    color: var(--text);
    font-size: $fs-sm;
    cursor: pointer;
    outline: none;
    user-select: none;
    transition: background $dur-fast $ease-out, color $dur-fast $ease-out;
  }

  :global(.filter-menu-item[data-highlighted]) {
    background: var(--surface-3);
  }

  :global(.filter-menu-label) {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  :global(.filter-menu-count) {
    min-width: 20px;
    height: 18px;
    padding: 0 6px;
    border-radius: $radius-pill;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: var(--surface-3);
    color: var(--text-muted);
    font-size: 10px;
    font-weight: $fw-semibold;
    font-variant-numeric: tabular-nums;
  }

  :global(.filter-menu-check) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--accent);
    opacity: 0;
  }

  :global(.filter-menu-check.shown) {
    opacity: 1;
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
    gap: 0;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 4px 0 8px;
    min-height: 0;
    @include scrollbar;
  }

  .virtual-spacer {
    width: 1px;
    flex: 0 0 auto;
    pointer-events: none;
  }

  .empty {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: $space-3;
    padding: $space-12 $space-6;
    text-align: center;
    color: var(--text-muted);
  }

  .empty-icon {
    width: 48px;
    height: 48px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--surface-3);
    border: 1px solid var(--border);
    color: var(--text-faint);
  }

  .empty h3 {
    font-size: $fs-md;
    color: var(--text-strong);
  }

  .empty p {
    font-size: $fs-sm;
    max-width: 280px;
    line-height: $lh-normal;
  }

  .skeleton {
    display: grid;
    gap: $space-2;
    margin: 3px 8px;
    padding: 15px 16px;
    border-radius: $radius-lg;
    border: 1px solid var(--border);
    background: var(--surface-gradient);
    box-shadow: var(--inner-highlight);
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
    gap: 1px;
    width: 184px;
    padding: $space-1;
    border-radius: $radius-md;
    background: var(--elevated);
    border: 1px solid var(--border-strong);
    box-shadow: var(--shadow-lg), var(--inner-highlight);
    animation: menu-in $dur-base $ease-out;
  }

  @keyframes menu-in {
    from {
      opacity: 0;
      transform: translateY(-6px) scale(0.97);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  .context-item {
    display: flex;
    align-items: center;
    gap: $space-2;
    padding: 8px $space-2;
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
    .selected-capsule {
      display: none;
    }

    .sort-wrap {
      min-width: 98px;
    }
  }
</style>
