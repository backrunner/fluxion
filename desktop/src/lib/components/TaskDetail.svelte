<script lang="ts">
  import { AlertTriangle, RotateCw, Activity, Hash, Clock, Check } from '@lucide/svelte';
  import type { BtStateSnapshot, TaskDetail } from '../types';
  import { formatBytes, formatSpeed, formatDuration, progressPercent, stateKind, stateColorVar, stateBgVar } from '../format';
  import { eta } from '../stores/tasks';
  import { openTaskFile, revealTaskFile } from '../api';

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

  $: kind = stateKind(detail.task.state);
  $: pct = progressPercent(detail.task.downloaded_bytes, detail.task.total_bytes);
  $: secs = eta(detail.task.downloaded_bytes, detail.task.total_bytes, downSpeed);
  $: isBt = detail.task.kind === 'Bt';

  function fmtTime(iso?: string | null): string {
    if (!iso) return '—';
    try {
      const d = new Date(iso);
      if (Number.isNaN(d.getTime())) return iso;
      return d.toLocaleString();
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
  <!-- Titlebar: file name + status pill on the left, drag spacer, action toolbar on the right. -->
  <header class="titlebar">
    <div class="titlebar-left">
      <span class={`pill ${kind}`}>{detail.task.state}</span>
      <h2 class="filename" title={detail.task.file_name ?? detail.task.id}>
        {detail.task.file_name ?? detail.task.id}
      </h2>
    </div>
    <div class="titlebar-drag" data-tauri-drag-region="true"></div>
    <div class="titlebar-actions">
      <ActionToolbar {detail} {busy} {onAction} {onRetry} onOpen={openFile} onReveal={revealFile} onLimitsSaved={onLimitsSaved} compact={true} />
    </div>
  </header>

  <div class="scroll">
    <div class="path-row">
      <span class="path" title={detail.task.save_dir}>{detail.task.save_dir}</span>
      <span class="pct-inline">{Math.round(pct)}%</span>
    </div>

    <div class="bar" aria-hidden="true"><div class="bar-fill" style={`width:${pct}%`}></div></div>
    <div class="pct-row">
      <span class="of">{formatBytes(detail.task.downloaded_bytes)} / {formatBytes(detail.task.total_bytes)}</span>
      {#if secs != null && detail.task.state === 'Downloading'}
        <span class="eta" title="Remaining time">ETA {formatDuration(secs)}</span>
      {/if}
    </div>

    {#if detail.task.error}
      <div class="banner error" role="alert">
        <AlertTriangle size={15} />
        <div class="banner-body">
          <strong>Download failed</strong>
          <span>{detail.task.error}</span>
          <span class="hint">Check the URL, refresh cookies/credentials, then retry.</span>
          {#if detail.task.state === 'Failed'}
            <button class="retry-link" on:click={onRetry} disabled={busy}><RotateCw size={12} /> Retry now</button>
          {/if}
        </div>
      </div>
    {/if}

    {#if isBt}
      <BtSection {detail} {btState} />
    {/if}

    <dl class="grid">
      <div class="dl-row"><dt><Activity size={13} /> State</dt><dd>{detail.task.state}</dd></div>
      <div class="dl-row"><dt><Hash size={13} /> Downloaded</dt><dd class="mono">{formatBytes(detail.task.downloaded_bytes)}</dd></div>
      <div class="dl-row"><dt><Hash size={13} /> Uploaded</dt><dd class="mono">{formatBytes(detail.task.uploaded_bytes)}</dd></div>
      <div class="dl-row"><dt><Activity size={13} /> Down speed</dt><dd class="mono">{formatSpeed(downSpeed)}</dd></div>
      <div class="dl-row"><dt><Activity size={13} /> Up speed</dt><dd class="mono">{formatSpeed(upSpeed)}</dd></div>
      <div class="dl-row"><dt><Clock size={13} /> Remaining</dt><dd class="mono">{secs != null && detail.task.state === 'Downloading' ? formatDuration(secs) : '—'}</dd></div>
      <div class="dl-row"><dt><Clock size={13} /> Created</dt><dd>{fmtTime(detail.task.created_at)}</dd></div>
      <div class="dl-row"><dt><Clock size={13} /> Updated</dt><dd>{fmtTime(detail.task.updated_at)}</dd></div>
      {#if detail.task.completed_at}
        <div class="dl-row"><dt><Check size={13} /> Completed</dt><dd>{fmtTime(detail.task.completed_at)}</dd></div>
      {/if}
    </dl>

    <CredentialsSection {detail} />
  </div>
</section>

<style lang="scss">
  @use '../../styles/tokens' as *;

  .detail {
    display: flex; flex-direction: column; min-width: 0; min-height: 0; height: 100%;
    background: var(--surface-2); overflow: hidden;
  }

  // ---- Titlebar ----
  .titlebar {
    display: flex; align-items: center; gap: $space-2; height: 52px; flex: none;
    padding: 0 $space-2 0 $space-4;
    border-bottom: 1px solid var(--border);
    background: linear-gradient(180deg, color-mix(in srgb, var(--surface) 82%, var(--surface-2) 18%), var(--surface));
    box-shadow: var(--inner-highlight);
  }

  .titlebar-left {
    display: flex; align-items: center; gap: $space-2; flex: none; min-width: 0;
  }

  .pill {
    @include pill; flex: none;
    color: var(--row-state); background: var(--row-state-bg);
  }

  .filename {
    font-size: $fs-sm; font-weight: $fw-semibold; color: var(--text-strong);
    @include hide-overflow; max-width: min(40vw, 320px);
  }

  .titlebar-drag { flex: 1; height: 100%; -webkit-app-region: drag; }

  .titlebar-actions {
    flex: none;
    -webkit-app-region: no-drag;
  }

  // ---- Scroll content ----
  .scroll {
    flex: 1; overflow-y: auto; padding: $space-4;
    display: flex; flex-direction: column; gap: $space-3; min-height: 0;
    @include scrollbar;
  }

  .path-row { display: flex; align-items: baseline; gap: $space-2; }
  .path {
    flex: 1; font-size: $fs-xs; color: var(--text-muted); font-family: $font-mono;
    @include hide-overflow;
  }
  .pct-inline { font-size: $fs-sm; font-weight: $fw-semibold; color: var(--text-strong); flex: none; }

  .bar { height: 8px; border-radius: $radius-pill; background: var(--surface-3); overflow: hidden; }
  .bar-fill {
    height: 100%; border-radius: inherit;
    background: linear-gradient(90deg, var(--row-state), color-mix(in srgb, var(--row-state) 60%, white));
    transition: width $dur-slow $ease-out;
  }

  .pct-row { display: flex; align-items: baseline; gap: $space-3; font-size: $fs-sm; }
  .of { color: var(--text-muted); font-family: $font-mono; }
  .eta { margin-left: auto; font-family: $font-mono; color: var(--row-state); font-size: $fs-xs; padding: 1px $space-2; border-radius: $radius-pill; background: var(--row-state-bg); }

  .banner { display: flex; gap: $space-2; padding: $space-3; border-radius: $radius-md; border: 1px solid var(--state-bad); background: var(--state-bad-bg); color: var(--state-bad); box-shadow: var(--inner-highlight); }
  .banner-body { display: flex; flex-direction: column; gap: 2px; font-size: $fs-sm; min-width: 0; strong { color: var(--state-bad); } span { color: var(--text); word-break: break-word; } .hint { color: var(--text-muted); font-size: $fs-xs; } }
  .retry-link {
    align-self: flex-start; margin-top: $space-1; display: inline-flex; align-items: center; gap: 4px;
    padding: 4px $space-2; border-radius: $radius-sm; border: 1px solid var(--state-bad);
    background: transparent; color: var(--state-bad); cursor: pointer; font-size: $fs-xs; font-weight: $fw-medium;
    @include focus-ring;
    &:not(:disabled):hover { background: var(--state-bad-bg); }
    &:disabled { opacity: 0.5; cursor: not-allowed; }
  }

  .grid {
    display: grid; grid-template-columns: 1fr; gap: 0; margin: 0;
    padding: $space-2 $space-4; border-radius: $radius-md;
    background: var(--surface); border: 1px solid var(--border);
    box-shadow: var(--shadow-sm), var(--inner-highlight);
  }
  .dl-row {
    display: grid; grid-template-columns: 140px 1fr; gap: $space-3; padding: $space-2 0;
    border-bottom: 1px solid var(--border); align-items: center;
    &:last-child { border-bottom: none; }
  }
  dt { display: flex; align-items: center; gap: 6px; color: var(--text-muted); font-size: $fs-sm; @include hide-overflow; }
  dd { margin: 0; color: var(--text-strong); font-size: $fs-sm; word-break: break-word; }
  .mono { font-family: $font-mono; font-variant-numeric: tabular-nums; }
</style>