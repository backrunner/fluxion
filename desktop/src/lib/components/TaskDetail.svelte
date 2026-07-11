<script lang="ts">
  import {
    AlertTriangle,
    RotateCw,
    ArrowDown,
    ArrowUp,
    Hourglass,
    Activity,
    HardDriveDownload,
    HardDriveUpload,
    CalendarPlus,
    CalendarClock,
    CalendarCheck,
    FolderOpen,
    PanelRightClose
  } from '@lucide/svelte';
  import type { BtStateSnapshot, TaskDetail } from '../types';
  import { formatBytes, formatSpeed, formatDuration, progressPercent, stateKind, stateColorVar, stateBgVar, isActive } from '../format';
  import { eta } from '../stores/tasks';
  import { openTaskFile, revealTaskFile } from '../api';
  import { locale, t } from '../i18n';
  import { windowDrag } from '../windowDrag';

  import ActionToolbar from './detail/ActionToolbar.svelte';
  import CredentialsSection from './detail/CredentialsSection.svelte';
  import BtSection from './detail/BtSection.svelte';

  export let detail: TaskDetail;
  export let downSpeed = 0;
  export let upSpeed = 0;
  export let busy = false;
  export let btState: BtStateSnapshot | null = null;

  export let onAction: (action: 'start' | 'pause' | 'stop' | 'delete') => void = () => {};
  export let onRetry: () => void = () => {};
  export let onLimitsSaved: () => void = () => {};
  export let onClose: () => void = () => {};

  $: kind = stateKind(detail.task.state);
  $: pct = progressPercent(detail.task.downloaded_bytes, detail.task.total_bytes);
  $: secs = eta(detail.task.downloaded_bytes, detail.task.total_bytes, downSpeed);
  $: isBt = detail.task.kind === 'Bt';
  $: live = isActive(detail.task.state) && downSpeed > 0;

  function fmtTime(iso?: string | null): string {
    if (!iso) return '-';
    try {
      const d = new Date(iso);
      if (Number.isNaN(d.getTime())) return iso;
      return d.toLocaleString($locale);
    } catch { return iso; }
  }

  async function openFile() {
    try { await openTaskFile(detail.task.id); }
    catch (cause) { console.error(cause); }
  }
  async function revealFile() {
    try { await revealTaskFile(detail.task.id); }
    catch (cause) { console.error(cause); }
  }
</script>

<section class="detail" style={`--row-state: ${stateColorVar(kind)}; --row-state-bg: ${stateBgVar(kind)};`}>
  <!-- Hero header: kind chip + filename + state pill, drag spacer, actions. -->
  <header class="titlebar" data-tauri-drag-region="true" use:windowDrag>
    <div class="titlebar-left" data-tauri-drag-region="true">
      <span class={`pill ${kind}`} data-tauri-drag-region="true">{$t(`state.${detail.task.state}`)}</span>
      <h2 class="filename" title={detail.task.file_name ?? detail.task.id} data-tauri-drag-region="true">
        {detail.task.file_name ?? detail.task.id}
      </h2>
    </div>
    <div class="titlebar-drag" data-tauri-drag-region="true"></div>
    <div class="titlebar-actions" data-tauri-drag-region="true">
      <button class="detail-close" on:click={onClose} title={$t('list.hideDetail')} aria-label={$t('list.hideDetail')}>
        <PanelRightClose size={15} />
      </button>
      <ActionToolbar {detail} {busy} {onAction} {onRetry} onOpen={openFile} onReveal={revealFile} onLimitsSaved={onLimitsSaved} compact={true} />
    </div>
  </header>

  <div class="scroll">
    <!-- Progress hero -->
    <div class="progress-hero">
      <div class="path-row">
        <FolderOpen size={13} class="path-ic" aria-hidden="true" />
        <span class="path" title={detail.task.save_dir}>{detail.task.save_dir}</span>
      </div>
      <div class="bar" class:live aria-hidden="true"><div class="bar-fill" style={`width:${pct}%`}></div></div>
      <div class="pct-row">
        <span class="pct-big">{Math.round(pct)}<span class="pct-unit">%</span></span>
        <span class="of">{formatBytes(detail.task.downloaded_bytes)} <span class="of-sep">/</span> {formatBytes(detail.task.total_bytes)}</span>
        {#if secs != null && detail.task.state === 'Downloading'}
          <span class="eta" title={$t('task.remainingTime')}><Hourglass size={12} class="eta-ic" /> {formatDuration(secs)}</span>
        {/if}
      </div>
    </div>

    {#if detail.task.error}
      <div class="banner error" role="alert">
        <AlertTriangle size={15} />
        <div class="banner-body">
          <strong>{$t('detail.failed')}</strong>
          <span>{detail.task.error}</span>
          <span class="hint">{$t('detail.failedHint')}</span>
          {#if detail.task.state === 'Failed'}
            <button class="retry-link" on:click={onRetry} disabled={busy}><RotateCw size={12} /> {$t('detail.retryNow')}</button>
          {/if}
        </div>
      </div>
    {/if}

    {#if isBt}
      <BtSection {detail} {btState} />
    {/if}

    <!-- Stat tiles -->
    <div class="stat-grid">
      <div class="stat-tile">
        <span class="stat-label"><Activity size={12} /> {$t('detail.state')}</span>
        <span class="stat-value" style={`color: var(--row-state)`}>{$t(`state.${detail.task.state}`)}</span>
      </div>
      <div class="stat-tile">
        <span class="stat-label"><HardDriveDownload size={12} /> {$t('detail.downloaded')}</span>
        <span class="stat-value mono">{formatBytes(detail.task.downloaded_bytes)}</span>
      </div>
      <div class="stat-tile">
        <span class="stat-label"><HardDriveUpload size={12} /> {$t('detail.uploaded')}</span>
        <span class="stat-value mono">{formatBytes(detail.task.uploaded_bytes)}</span>
      </div>
      <div class="stat-tile">
        <span class="stat-label"><ArrowDown size={12} /> {$t('detail.downSpeed')}</span>
        <span class="stat-value mono" style={`color: ${downSpeed > 0 ? 'var(--state-good)' : 'var(--text-strong)'}`}>{formatSpeed(downSpeed)}</span>
      </div>
      <div class="stat-tile">
        <span class="stat-label"><ArrowUp size={12} /> {$t('detail.upSpeed')}</span>
        <span class="stat-value mono" style={`color: ${upSpeed > 0 ? 'var(--cool)' : 'var(--text-strong)'}`}>{formatSpeed(upSpeed)}</span>
      </div>
      <div class="stat-tile">
        <span class="stat-label"><Hourglass size={12} /> {$t('detail.remaining')}</span>
        <span class="stat-value mono">{secs != null && detail.task.state === 'Downloading' ? formatDuration(secs) : '-'}</span>
      </div>
    </div>

    <!-- Timeline -->
    <div class="timeline">
      <h4>{$t('detail.timeline')}</h4>
      <div class="tl-row"><span class="tl-label"><CalendarPlus size={12} /> {$t('detail.created')}</span><span class="tl-value">{fmtTime(detail.task.created_at)}</span></div>
      <div class="tl-row"><span class="tl-label"><CalendarClock size={12} /> {$t('detail.updated')}</span><span class="tl-value">{fmtTime(detail.task.updated_at)}</span></div>
      {#if detail.task.completed_at}
        <div class="tl-row"><span class="tl-label"><CalendarCheck size={12} /> {$t('detail.completed')}</span><span class="tl-value">{fmtTime(detail.task.completed_at)}</span></div>
      {/if}
    </div>

    <CredentialsSection {detail} />
  </div>
</section>

<style lang="scss">
  @use '../../styles/tokens' as *;

  .detail {
    display: flex; flex-direction: column; min-width: 0; min-height: 0; height: 100%;
    background: var(--surface-2); overflow: hidden;
    container: detail / inline-size;
  }

  // ---- Titlebar ----
  .titlebar {
    display: flex; align-items: center; gap: $space-2; height: 48px; flex: none;
    padding: 0 10px 0 $space-4;
    border-bottom: 1px solid var(--border);
    background: linear-gradient(180deg, var(--surface), var(--surface-2));
    box-shadow: var(--inner-highlight);
  }

  .titlebar-left {
    display: flex; align-items: center; gap: $space-2; flex: none; min-width: 0;
  }

  .pill {
    flex: none;
    color: var(--row-state);
    background: transparent;
    font-size: 11px;
    font-weight: $fw-semibold;
  }

  .filename {
    font-size: $fs-sm; font-weight: $fw-semibold; color: var(--text-strong);
    @include hide-overflow; max-width: min(40vw, 320px);
  }

  .titlebar-drag { flex: 1; height: 100%; }

  .titlebar-actions {
    flex: none;
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .detail-close {
    display: none;
    place-items: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border-radius: $radius-md;
    @include glass-hover-control;
    color: var(--text-muted);
    cursor: pointer;
    @include focus-ring;

    &:hover {
      color: var(--text-strong);
    }
  }

  // ---- Scroll content ----
  .scroll {
    flex: 1; overflow-y: auto; padding: $space-3;
    display: flex; flex-direction: column; gap: $space-3; min-height: 0;
    @include scrollbar;
  }

  // ---- Progress hero ----
  .progress-hero {
    display: flex; flex-direction: column; gap: $space-3;
    padding: $space-5;
    border-radius: $radius-xl;
    background: var(--selected-gradient);
    border: 1px solid var(--accent-soft-strong);
    box-shadow: var(--selected-shadow), var(--inner-highlight);
  }

  .path-row { display: flex; align-items: center; gap: 6px; min-width: 0; }
  :global(.path-ic) { flex: none; color: var(--text-faint); }
  .path {
    flex: 1; min-width: 0; font-size: $fs-xs; color: var(--text-muted); font-family: $font-mono;
    @include hide-overflow;
  }

  .bar { height: 5px; border-radius: 3px; background: var(--surface-3); overflow: hidden; }
  .bar-fill {
    position: relative; height: 100%; border-radius: inherit;
    background: var(--row-state);
    transition: width $dur-slow $ease-out;
  }
  .bar.live .bar-fill {
    @include bar-shimmer;
  }

  .pct-row { display: flex; align-items: baseline; gap: $space-3; flex-wrap: wrap; }
  .pct-big {
    font-family: $font-mono; font-size: $fs-2xl; font-weight: $fw-bold; color: var(--text-strong);
    line-height: 1; font-variant-numeric: tabular-nums;
  }
  .pct-unit { font-size: $fs-base; font-weight: $fw-semibold; color: var(--text-muted); margin-left: 1px; }
  .of { color: var(--text-muted); font-family: $font-mono; font-size: $fs-sm; }
  .of-sep { color: var(--text-faint); margin: 0 2px; }
  .eta {
    margin-left: auto; display: inline-flex; align-items: center; gap: 4px;
    font-family: $font-mono; color: var(--row-state); font-size: $fs-xs;
    padding: 0; border-radius: 0; background: transparent;
    font-weight: $fw-medium;
  }
  :global(.eta-ic) { opacity: 0.85; }

  .banner { display: flex; gap: $space-2; margin: 0; padding: $space-3; border-radius: $radius-lg; border: 1px solid var(--state-bad); background: var(--state-bad-bg); color: var(--state-bad); box-shadow: var(--inner-highlight); }
  .banner-body { display: flex; flex-direction: column; gap: 2px; font-size: $fs-sm; min-width: 0; strong { color: var(--state-bad); } span { color: var(--text); word-break: break-word; } .hint { color: var(--text-muted); font-size: $fs-xs; } }
  .retry-link {
    align-self: flex-start; margin-top: $space-1; display: inline-flex; align-items: center; gap: 4px;
    padding: 4px $space-2; border-radius: $radius-sm; border: 1px solid var(--state-bad);
    background: transparent; color: var(--state-bad); cursor: pointer; font-size: $fs-xs; font-weight: $fw-medium;
    @include focus-ring;
    &:not(:disabled):hover { background: var(--state-bad-bg); }
    &:disabled { opacity: 0.5; cursor: not-allowed; }
  }

  // ---- Stat tiles ----
  .stat-grid {
    display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 0;
    background: var(--surface-gradient);
    border: 1px solid var(--border);
    border-radius: $radius-xl;
    overflow: hidden;
    box-shadow: var(--shadow-sm), var(--inner-highlight);
  }
  .stat-tile {
    display: flex; flex-direction: column; gap: 4px;
    min-width: 0;
    padding: 14px $space-4; border-radius: 0;
    background: transparent; border: none;
    border-right: 1px solid var(--border);
    border-bottom: 1px solid var(--border);

    &:nth-child(3n) { border-right: none; }
    &:nth-last-child(-n + 3) { border-bottom: none; }
  }
  .stat-label {
    display: inline-flex; align-items: center; gap: 5px;
    color: var(--text-muted); font-size: 11px; font-weight: $fw-medium;
  }
  .stat-value {
    color: var(--text-strong); font-size: $fs-base; font-weight: $fw-semibold;
    word-break: break-word; line-height: $lh-snug;
    font-variant-numeric: tabular-nums;
  }
  .stat-value.mono { font-family: $font-mono; font-weight: $fw-semibold; }

  // ---- Timeline ----
  .timeline {
    display: flex; flex-direction: column; gap: 0;
    margin-top: 0;
    padding: $space-4 $space-5; border-radius: $radius-xl;
    background: var(--surface-gradient); border: 1px solid var(--border);
    box-shadow: var(--shadow-sm), var(--inner-highlight);
  }
  .timeline h4 {
    font-size: $fs-sm; font-weight: $fw-semibold; color: var(--text-strong);
    text-transform: none; letter-spacing: 0; margin-bottom: $space-2;
  }
  .tl-row {
    display: grid; grid-template-columns: 120px 1fr; gap: $space-3; padding: $space-2 0;
    border-bottom: 1px solid var(--border); align-items: center;
    &:last-child { border-bottom: none; }
  }
  .tl-label { display: flex; align-items: center; gap: 6px; color: var(--text-muted); font-size: $fs-sm; @include hide-overflow; }
  .tl-value { color: var(--text-strong); font-size: $fs-sm; word-break: break-word; }

  @media (max-width: 460px) {
    .stat-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .stat-tile,
    .stat-tile:nth-child(3n) { border-right: 1px solid var(--border); border-bottom: 1px solid var(--border); }
    .stat-tile:nth-child(2n) { border-right: none; }
    .stat-tile:nth-last-child(-n + 2) { border-bottom: none; }
    .tl-row { grid-template-columns: 1fr; gap: 2px; }
  }

  @container detail (max-width: 520px) {
    .pill { display: none; }
    .filename { max-width: 150px; }
    .detail-close { display: grid; }
    .titlebar-actions :global(.actions.compact .reveal) { display: none; }
  }

  // Below the desktop split-view breakpoint the detail becomes an overlay.
  // Its close control must remain available even when the overlay is wider
  // than the compact container-query threshold.
  @media (max-width: 980px) {
    .detail-close { display: grid; }
  }
</style>
