<script lang="ts">
  import {
    ArrowDown,
    ArrowUp,
    FileArchive,
    FileAudio,
    FileCode,
    FileImage,
    FileSpreadsheet,
    FileText,
    FileVideo,
    Folder,
    Globe,
    Magnet,
    Package
  } from '@lucide/svelte';
  import type { DownloadKind, RuntimeTask } from '../types';
  import {
    formatBytes,
    formatSpeed,
    formatDuration,
    progressPercent,
    stateKind,
    stateColorVar,
    stateBgVar,
    isActive
  } from '../format';
  import { eta } from '../stores/tasks';
  import { t } from '../i18n';

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

  const iconByExtension: Record<string, typeof Globe> = {
    '7z': FileArchive,
    aac: FileAudio,
    apk: Package,
    app: Package,
    avi: FileVideo,
    bz2: FileArchive,
    csv: FileSpreadsheet,
    deb: Package,
    dmg: Package,
    doc: FileText,
    docx: FileText,
    exe: Package,
    flac: FileAudio,
    gz: FileArchive,
    htm: FileCode,
    html: FileCode,
    iso: Package,
    jpeg: FileImage,
    jpg: FileImage,
    js: FileCode,
    json: FileCode,
    m4a: FileAudio,
    m4v: FileVideo,
    md: FileText,
    mkv: FileVideo,
    mov: FileVideo,
    mp3: FileAudio,
    mp4: FileVideo,
    msi: Package,
    ogg: FileAudio,
    pdf: FileText,
    pkg: Package,
    png: FileImage,
    ppt: FileText,
    pptx: FileText,
    rar: FileArchive,
    rpm: Package,
    sh: FileCode,
    svg: FileImage,
    tar: FileArchive,
    tgz: FileArchive,
    ts: FileCode,
    txt: FileText,
    wav: FileAudio,
    webm: FileVideo,
    webp: FileImage,
    xls: FileSpreadsheet,
    xlsx: FileSpreadsheet,
    xml: FileCode,
    yaml: FileCode,
    yml: FileCode,
    zip: FileArchive
  };

  $: kind = stateKind(task.state);
  $: pct = progressPercent(task.downloaded_bytes, task.total_bytes);
  $: secs = eta(task.downloaded_bytes, task.total_bytes, downSpeed);
  $: Icon = iconForTask(task);
  $: live = isActive(task.state) && downSpeed > 0;

  function iconForTask(item: RuntimeTask) {
    if (item.kind !== 'Http') return kindIcon[item.kind];
    const ext = fileExtension(item.file_name ?? '');
    return ext ? iconByExtension[ext] ?? FileText : kindIcon.Http;
  }

  function fileExtension(name: string) {
    const clean = name.split(/[?#]/)[0]?.trim() ?? '';
    const basename = clean.split('/').filter(Boolean).pop() ?? clean;
    const match = /\.([a-z0-9]+)$/i.exec(basename);
    return match?.[1].toLowerCase() ?? '';
  }

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
  class:live
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
    <span class={`pill ${kind}`}>{$t(`state.${task.state}`)}</span>
  </div>

  <div class="bar" aria-hidden="true">
    <div class="bar-fill" style={`width:${pct}%`}></div>
  </div>

  <div class="meta">
    <span class="size">{formatBytes(task.downloaded_bytes)} / {formatBytes(task.total_bytes)}</span>
    {#if downSpeed > 0}
      <span class="sep" aria-hidden="true"></span>
      <span class="speed down" title={$t('task.downloadSpeed')}><ArrowDown size={11} class="ic" /> {formatSpeed(downSpeed)}</span>
    {/if}
    {#if upSpeed > 0}
      <span class="sep" aria-hidden="true"></span>
      <span class="speed up" title={$t('task.uploadSpeed')}><ArrowUp size={11} class="ic" /> {formatSpeed(upSpeed)}</span>
    {/if}
    {#if secs != null && task.state === 'Downloading'}
      <span class="sep" aria-hidden="true"></span>
      <span class="eta" title={$t('task.remainingTime')}>{formatDuration(secs)}</span>
    {/if}
    <span class="row-progress" title={$t('task.progress')}>{pct.toFixed(1)}%</span>
  </div>
</div>

<style lang="scss">
  @use '../../styles/tokens' as *;

  .task-row {
    position: relative;
    display: grid;
    grid-template-rows: auto auto auto;
    gap: 5px;
    width: calc(100% - 16px);
    height: 70px;
    margin: 2px 8px;
    padding: 7px $space-3;
    border-radius: $radius-lg;
    border: 1px solid transparent;
    background: color-mix(in srgb, var(--surface-2) 36%, transparent);
    color: var(--text);
    text-align: left;
    cursor: pointer;
    box-shadow: inset 0 1px 0 color-mix(in srgb, var(--border) 35%, transparent);
    transition: background $dur-base $ease-out, border-color $dur-fast $ease-out,
      box-shadow $dur-base $ease-out, transform $dur-fast $ease-out;
    @include focus-ring;
  }

  .task-row:hover {
    background: var(--surface-gradient);
    border-color: var(--border);
    box-shadow: var(--shadow-sm), var(--inner-highlight);
  }

  .task-row.selected {
    background: var(--selected-gradient);
    border-color: var(--accent-soft-strong);
    box-shadow: var(--selected-shadow), var(--inner-highlight);
  }

  .task-row.dragging {
    opacity: 0.58;
  }

  .task-row.drop-target {
    border-color: var(--accent);
    box-shadow: var(--selected-shadow), var(--inner-highlight);
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
    background: var(--control-gradient);
    border: 1px solid var(--border);
    box-shadow: var(--inner-highlight);
    color: var(--text-muted);
    flex: none;

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
    display: inline-flex;
    align-items: center;
    padding: 0;
    font-size: 11px;
    font-weight: $fw-semibold;
    line-height: 1.4;
    letter-spacing: 0;
    color: var(--row-state);
    background: transparent;
    flex: none;
  }

  .bar {
    height: 3px;
    border-radius: 2px;
    background: var(--surface-3);
    overflow: hidden;
    position: relative;
  }

  .bar-fill {
    position: relative;
    height: 100%;
    border-radius: inherit;
    background: var(--row-state);
    transition: width $dur-slow $ease-out;
  }

  // Live progress: a drifting sheen marks in-flight transfers.
  .task-row.live .bar-fill {
    @include bar-shimmer;
  }

  .meta {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: $fs-xs;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
    min-width: 0;
  }

  .size {
    font-family: $font-mono;
    @include hide-overflow;
  }

  .sep {
    width: 1px;
    height: 10px;
    background: var(--border-strong);
    flex: none;
  }

  .speed {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-family: $font-mono;
    white-space: nowrap;
    &.down {
      color: var(--state-good);
    }
    &.up {
      color: var(--cool);
    }
    :global(.ic) {
      opacity: 0.85;
    }
  }

  .eta {
    font-family: $font-mono;
    color: var(--text);
    white-space: nowrap;
  }

  .row-progress {
    margin-left: auto;
    min-width: 44px;
    padding: 0;
    border-radius: 0;
    background: transparent;
    color: var(--text-strong);
    font-family: $font-mono;
    font-weight: $fw-semibold;
    text-align: right;
    flex: none;
  }

</style>
