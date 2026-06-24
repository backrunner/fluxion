<script lang="ts">
  import { Play, Pause, Square, Trash2, RotateCw, ExternalLink, FolderSearch, Gauge, X } from '@lucide/svelte';
  import type { TaskDetail, TaskRateLimit, TaskState } from '../../types';
  import { canStart, canPause, canStop } from '../../format';
  import { updateTaskLimits } from '../../api';

  export let detail: TaskDetail;
  export let busy = false;
  export let compact = false;

  export let onAction: (action: 'start' | 'pause' | 'stop' | 'delete') => void = () => {};
  export let onRetry: () => void = () => {};
  export let onOpen: () => void = () => {};
  export let onReveal: () => void = () => {};
  export let onLimitsSaved: () => void = () => {};

  $: state = detail.task.state;
  $: startEnabled = canStart(state) && state !== 'Failed' && !busy;
  $: pauseEnabled = canPause(state) && !busy;
  $: stopEnabled = canStop(state) && !busy;
  $: retryEnabled = state === 'Failed' && !busy;
  $: openEnabled = state === 'Completed' && !busy;

  let limitsOpen = false;
  let limitDownload = '';
  let limitUpload = '';
  let limitsBusy = false;
  let limitsHint = '';

  function openLimits() {
    limitDownload = detail.task.limits.download_bytes_per_second?.toString() ?? '';
    limitUpload = detail.task.limits.upload_bytes_per_second?.toString() ?? '';
    limitsHint = '';
    limitsOpen = true;
  }

  function closeLimits() {
    if (!limitsBusy) limitsOpen = false;
  }

  async function saveLimits() {
    limitsBusy = true;
    limitsHint = '';
    const limits: TaskRateLimit = {
      download_bytes_per_second: limitDownload ? Number(limitDownload) : null,
      upload_bytes_per_second: limitUpload ? Number(limitUpload) : null
    };
    try {
      await updateTaskLimits(detail.task.id, limits);
      limitsHint = 'Saved — restart the task for new limits to take effect.';
      onLimitsSaved();
      setTimeout(() => { limitsHint = ''; limitsOpen = false; }, 900);
    } catch (cause) {
      limitsHint = cause instanceof Error ? cause.message : String(cause);
    } finally {
      limitsBusy = false;
    }
  }
</script>

<div class="actions" class:compact role="toolbar" aria-label="Task actions">
  {#if state === 'Failed'}
    <button class="primary" disabled={!retryEnabled} on:click={onRetry} title="Retry">
      <RotateCw size={14} /> {#if !compact}<span>Retry</span>{/if}
    </button>
  {:else}
    <button disabled={!startEnabled} on:click={() => onAction('start')} title="Start">
      <Play size={14} /> {#if !compact}<span>Start</span>{/if}
    </button>
  {/if}
  <button disabled={!pauseEnabled} on:click={() => onAction('pause')} title="Pause">
    <Pause size={14} /> {#if !compact}<span>Pause</span>{/if}
  </button>
  <button disabled={!stopEnabled} on:click={() => onAction('stop')} title="Stop">
    <Square size={14} /> {#if !compact}<span>Stop</span>{/if}
  </button>
  <button class="limits-btn" on:click={openLimits} disabled={busy} title="Rate limits">
    <Gauge size={14} /> {#if !compact}<span>Limits</span>{/if}
  </button>
  <button class="danger" disabled={busy} on:click={() => onAction('delete')} title="Delete">
    <Trash2 size={14} /> {#if !compact}<span>Delete</span>{/if}
  </button>
  <button class="reveal" disabled={!openEnabled} on:click={onReveal} title="Reveal in Finder">
    <FolderSearch size={14} /> {#if !compact}<span>Reveal</span>{/if}
  </button>
  <button class="reveal" disabled={!openEnabled} on:click={onOpen} title="Open file">
    <ExternalLink size={14} /> {#if !compact}<span>Open</span>{/if}
  </button>
</div>

{#if limitsOpen}
  <div class="overlay" on:click|self={closeLimits} on:keydown={(e) => e.key === 'Escape' && closeLimits()} role="presentation">
    <div class="dialog" role="dialog" aria-modal="true" aria-label="Rate limits" tabindex="-1" on:click|stopPropagation on:keydown|stopPropagation>
      <div class="dialog-head">
        <div class="dialog-title"><Gauge size={15} /> Rate limits</div>
        <button class="close" on:click={closeLimits} disabled={limitsBusy} aria-label="Close"><X size={14} /></button>
      </div>
      <label class="field">
        <span class="lbl">Download limit <em>bytes/s</em></span>
        <input bind:value={limitDownload} inputmode="numeric" placeholder="unlimited" disabled={limitsBusy} />
      </label>
      <label class="field">
        <span class="lbl">Upload limit <em>bytes/s</em></span>
        <input bind:value={limitUpload} inputmode="numeric" placeholder="unlimited" disabled={limitsBusy} />
      </label>
      {#if limitsHint}<p class="hint">{limitsHint}</p>{/if}
      <div class="dialog-actions">
        <button on:click={closeLimits} disabled={limitsBusy}>Cancel</button>
        <button class="primary" on:click={saveLimits} disabled={limitsBusy}>{limitsBusy ? 'Saving…' : 'Save'}</button>
      </div>
    </div>
  </div>
{/if}

<style lang="scss">
  @use '../../../styles/tokens' as *;

  .actions { display: flex; flex-wrap: wrap; gap: $space-2; padding: $space-2 0; }

  .actions.compact { flex-wrap: nowrap; padding: 0; gap: 2px; }

  .actions button {
    display: inline-flex; align-items: center; gap: 6px; padding: 7px $space-3;
    border-radius: $radius-md; border: 1px solid var(--border); background: var(--surface);
    color: var(--text); cursor: pointer; font-size: $fs-sm;
    transition: background $dur-fast $ease-out, border-color $dur-fast $ease-out,
      color $dur-fast $ease-out, opacity $dur-fast $ease-out;
    @include focus-ring;
    &:not(:disabled):hover { background: var(--surface-3); border-color: var(--border-strong); }
    &:disabled { opacity: 0.4; cursor: not-allowed; }
  }

  // Compact mode: icon-only buttons for the titlebar.
  .actions.compact button {
    display: inline-flex; align-items: center; justify-content: center;
    width: 28px; height: 28px; padding: 0;
    border-radius: $radius-sm; border: 1px solid transparent; background: transparent;
    color: var(--text-muted);
    &:not(:disabled):hover { background: var(--surface-3); color: var(--text-strong); border-color: var(--border); }

    :global(svg) {
      display: block;
      margin: 0;
    }
  }
  .actions.compact .primary:not(:disabled) { color: var(--accent); }
  .actions.compact .limits-btn:not(:disabled) { color: var(--text-muted); }
  .actions.compact .danger {
    color: var(--state-bad);
    &:not(:disabled):hover {
      background: var(--state-bad-bg);
      border-color: var(--state-bad);
      color: var(--state-bad);
    }
  }
  .actions.compact .reveal:not(:disabled) { color: var(--text-muted); &:not(:disabled):hover { color: var(--accent); } }

  .primary {
    background: linear-gradient(145deg, var(--accent-hover), var(--accent-press));
    border-color: transparent; color: var(--accent-contrast); font-weight: $fw-semibold;
    &:not(:disabled):hover { filter: brightness(1.06); box-shadow: var(--shadow-glow); }
  }

  .limits-btn {
    color: var(--text-muted);
    &:not(:disabled):hover { color: var(--accent); border-color: var(--accent); background: var(--accent-soft); }
  }

  .danger {
    color: var(--state-bad); border-color: rgba(248, 113, 113, 0.28);
    &:not(:disabled):hover {
      background: var(--state-bad-bg);
      border-color: var(--state-bad);
      color: var(--state-bad);
    }
  }

  .reveal {
    color: var(--text-muted); border-color: var(--border); margin-left: auto;
    &:not(:disabled):hover { color: var(--accent); border-color: var(--accent); background: var(--accent-soft); }
  }

  // ---- Limits dialog ----
  .overlay {
    position: fixed; inset: 0; background: var(--scrim);
    backdrop-filter: blur(4px); -webkit-backdrop-filter: blur(4px);
    display: grid; place-items: center; padding: $space-6; z-index: 60;
    animation: fade $dur-fast $ease-out;
  }
  @keyframes fade { from { opacity: 0; } to { opacity: 1; } }

  .dialog {
    width: min(380px, 100%); display: flex; flex-direction: column; gap: $space-3;
    padding: $space-5; border-radius: $radius-lg; background: var(--elevated);
    border: 1px solid var(--border-strong); box-shadow: var(--shadow-lg);
    animation: pop $dur-base $ease-out;
  }
  @keyframes pop {
    from { opacity: 0; transform: translateY(8px) scale(0.98); }
    to { opacity: 1; transform: translateY(0) scale(1); }
  }

  .dialog-head { display: flex; align-items: center; justify-content: space-between; }
  .dialog-title { display: flex; align-items: center; gap: 6px; font-size: $fs-md; font-weight: $fw-semibold; color: var(--text-strong); }
  .close {
    display: grid; place-items: center; width: 26px; height: 26px;
    border-radius: $radius-xs; border: 1px solid transparent; background: transparent;
    color: var(--text-muted); cursor: pointer;
    @include focus-ring;
    &:not(:disabled):hover { background: var(--surface-3); color: var(--text-strong); }
    &:disabled { opacity: 0.5; cursor: not-allowed; }
  }

  .field { display: flex; flex-direction: column; gap: 5px; }
  .lbl { font-size: $fs-xs; color: var(--text-muted); font-weight: $fw-medium; em { font-style: normal; color: var(--text-faint); font-weight: $fw-regular; } }

  input {
    width: 100%; box-sizing: border-box; border-radius: $radius-sm; border: 1px solid var(--border);
    background: var(--surface-3); color: var(--text); padding: 8px $space-3; font-size: $fs-sm; font-family: $font-mono;
    transition: border-color $dur-fast $ease-out, box-shadow $dur-fast $ease-out;
    &::placeholder { color: var(--text-faint); }
    &:focus { outline: none; border-color: var(--accent); box-shadow: 0 0 0 2px var(--focus-ring); }
    &:disabled { opacity: 0.6; }
  }

  .hint { font-size: $fs-xs; color: var(--text-muted); margin: 0; }

  .dialog-actions { display: flex; justify-content: flex-end; gap: $space-2; }
  .dialog-actions button {
    padding: 7px $space-4; border-radius: $radius-md; border: 1px solid var(--border);
    background: var(--surface); color: var(--text); cursor: pointer; font-size: $fs-sm;
    @include focus-ring;
    &:not(:disabled):hover { background: var(--surface-3); border-color: var(--border-strong); }
    &:disabled { opacity: 0.5; cursor: not-allowed; }
  }
  .dialog-actions .primary {
    background: linear-gradient(145deg, var(--accent-hover), var(--accent-press));
    border-color: transparent; color: var(--accent-contrast); font-weight: $fw-semibold;
    &:not(:disabled):hover { filter: brightness(1.06); box-shadow: var(--shadow-glow); }
  }
</style>
