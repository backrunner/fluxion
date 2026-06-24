<script lang="ts">
  import { Globe, FileText, Magnet, Folder, GripVertical } from '@lucide/svelte';
  import type { DownloadKind, RuntimeTask } from '../types';
  import {
    formatBytes,
    formatSpeed,
    formatDuration,
    progressPercent,
    stateKind,
    stateColorVar,
    stateBgVar
  } from '../format';
  import { eta } from '../stores/tasks';

  export let task: RuntimeTask;
  export let selected: boolean;
  export let downSpeed = 0;
  export let upSpeed = 0;
  export let busy: boolean;
  export let draggableRow = false;
  export let dragging = false;
  export let dropTarget = false;

  export let select: (event: MouseEvent | KeyboardEvent) => void = () => {};
  export let openContextMenu: (event: MouseEvent) => void = () => {};
  export let dragStart: (event: DragEvent) => void = () => {};
  export let dragOver: (event: DragEvent) => void = () => {};
  export let dragLeave: (event: DragEvent) => void = () => {};
  export let drop: (event: DragEvent) => void = () => {};
  export let dragEnd: (event: DragEvent) => void = () => {};

  const kindIcon: Record<DownloadKind, typeof Globe> = {
    Http: Globe,
    Bt: Magnet,
    Ftp: Folder,
    Sftp: FileText
  };

  $: kind = stateKind(task.state);
  $: pct = progressPercent(task.downloaded_bytes, task.total_bytes);
  $: secs = eta(task.downloaded_bytes, task.total_bytes, downSpeed);
  $: Icon = kindIcon[task.kind];

  function onRowClick(e: MouseEvent) {
    select(e);
  }

  function onRowKey(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      select(e);
    }
  }
</script>

<div
  class="task-row"
  class:selected
  class:dragging
  class:drop-target={dropTarget}
  role="button"
  tabindex="0"
  aria-pressed={selected}
  draggable={draggableRow && !busy}
  style={`--row-state: ${stateColorVar(kind)}; --row-state-bg: ${stateBgVar(kind)};`}
  on:click={onRowClick}
  on:keydown={onRowKey}
  on:contextmenu={openContextMenu}
  on:dragstart={dragStart}
  on:dragover={dragOver}
  on:dragleave={dragLeave}
  on:drop={drop}
  on:dragend={dragEnd}
>
  <div class="top">
    <span class="kind" title={task.kind} aria-hidden="true">
      <Icon size={14} />
    </span>
    <span class="name" title={task.file_name ?? task.id}>{task.file_name ?? task.id.slice(0, 8)}</span>
    <span class={`pill ${kind}`}>{task.state}</span>
  </div>

  <div class="bar" aria-hidden="true">
    <div class="bar-fill" style={`width:${pct}%`}></div>
  </div>

  <div class="meta">
    <span class="size">{formatBytes(task.downloaded_bytes)} / {formatBytes(task.total_bytes)}</span>
    {#if downSpeed > 0}
      <span class="speed down" title="Download speed">{formatSpeed(downSpeed)}</span>
    {/if}
    {#if upSpeed > 0}
      <span class="speed up" title="Upload speed">↑ {formatSpeed(upSpeed)}</span>
    {/if}
    {#if secs != null && task.state === 'Downloading'}
      <span class="eta" title="Remaining time">{formatDuration(secs)}</span>
    {/if}
    <span class="row-actions">
      <span class="row-progress" title="Progress">{pct.toFixed(1)}%</span>
      <GripVertical size={13} aria-hidden="true" />
    </span>
  </div>
</div>

<style lang="scss">
  @use '../../styles/tokens' as *;

  .task-row {
    display: grid;
    grid-template-rows: auto auto auto;
    gap: $space-2;
    width: 100%;
    padding: $space-3 $space-4;
    border-radius: $radius-lg;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    text-align: left;
    cursor: pointer;
    transition: border-color $dur-fast $ease-out, background $dur-fast $ease-out,
      transform $dur-fast $ease-out, box-shadow $dur-fast $ease-out;
    @include focus-ring;
  }

  .task-row:hover {
    border-color: var(--border-strong);
    background: var(--surface-2);
    transform: translateY(-1px);
    box-shadow: var(--shadow-sm);
  }

  .task-row.selected {
    border-color: var(--accent);
    background: linear-gradient(180deg, var(--accent-soft), transparent 72%), var(--surface);
    box-shadow: 0 0 0 1px var(--accent);
  }

  .task-row.dragging {
    opacity: 0.58;
  }

  .task-row.drop-target {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  .top {
    display: flex;
    align-items: center;
    gap: $space-2;
    min-width: 0;
  }

  .kind {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border-radius: $radius-sm;
    background: var(--surface-3);
    border: 1px solid var(--border);
    box-shadow: var(--inner-highlight);
    color: var(--text-muted);
    flex: none;
    flex-shrink: 0;

    :global(svg) {
      display: block;
      margin: 0;
    }
  }

  .name {
    flex: 1 1 auto;
    min-width: 0;
    font-weight: $fw-semibold;
    font-size: $fs-base;
    color: var(--text-strong);
    @include hide-overflow;
  }

  .pill {
    @include pill;
    color: var(--row-state);
    background: var(--row-state-bg);
    flex: none;
  }

  .bar {
    height: 6px;
    border-radius: $radius-pill;
    background: var(--surface-3);
    overflow: hidden;
    position: relative;
  }

  .bar-fill {
    height: 100%;
    border-radius: inherit;
    background: linear-gradient(90deg, var(--row-state), color-mix(in srgb, var(--row-state) 62%, white));
    transition: width $dur-slow $ease-out;
  }

  .meta {
    display: flex;
    align-items: center;
    gap: $space-3;
    font-size: $fs-xs;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }

  .size {
    font-family: $font-mono;
  }

  .speed {
    font-family: $font-mono;
    &.down {
      color: var(--state-good);
    }
    &.up {
      color: var(--cool);
    }
  }

  .eta {
    margin-left: auto;
    font-family: $font-mono;
    color: var(--text);
    padding: 1px $space-2;
    border-radius: $radius-pill;
    background: var(--surface-3);
  }

  .row-actions {
    display: inline-flex;
    align-items: center;
    gap: $space-1;
    margin-left: auto;
    color: var(--text-faint);

    :global(svg) {
      display: block;
      margin: 0;
      opacity: 0.72;
    }
  }

  .row-progress {
    min-width: 46px;
    padding: 2px $space-2;
    border-radius: $radius-pill;
    background: var(--surface-3);
    color: var(--text-strong);
    font-family: $font-mono;
    font-weight: $fw-semibold;
    text-align: right;
  }
</style>
