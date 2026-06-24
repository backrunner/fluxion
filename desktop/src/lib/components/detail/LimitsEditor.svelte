<script lang="ts">
  import { Gauge, Save } from '@lucide/svelte';
  import type { TaskDetail, TaskRateLimit } from '../../types';
  import { updateTaskLimits } from '../../api';

  export let detail: TaskDetail;
  export let isActive = false;
  export let onSaved: () => void = () => {};

  let limitDownload = '';
  let limitUpload = '';
  let limitsBusy = false;
  let limitsHint = '';

  $: {
    limitDownload = detail.task.limits.download_bytes_per_second?.toString() ?? '';
    limitUpload = detail.task.limits.upload_bytes_per_second?.toString() ?? '';
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
      limitsHint = isActive ? 'Saved — restart the task for new limits to take effect.' : 'Saved.';
      onSaved();
      setTimeout(() => (limitsHint = ''), 4000);
    } catch (cause) {
      limitsHint = cause instanceof Error ? cause.message : String(cause);
    } finally {
      limitsBusy = false;
    }
  }
</script>

<section class="limits">
  <div class="limits-head">
    <Gauge size={13} />
    <span>Rate limits</span>
    {#if isActive}<span class="limits-note">restart to apply</span>{/if}
  </div>
  <div class="limits-grid">
    <label class="field">
      <span class="lbl">Download <em>bytes/s</em></span>
      <input bind:value={limitDownload} inputmode="numeric" placeholder="unlimited" disabled={limitsBusy} />
    </label>
    <label class="field">
      <span class="lbl">Upload <em>bytes/s</em></span>
      <input bind:value={limitUpload} inputmode="numeric" placeholder="unlimited" disabled={limitsBusy} />
    </label>
  </div>
  {#if limitsHint}<p class="limits-hint">{limitsHint}</p>{/if}
  <button class="limits-save" on:click={saveLimits} disabled={limitsBusy}>
    <Save size={13} /> {limitsBusy ? 'Saving…' : 'Save limits'}
  </button>
</section>

<style lang="scss">
  @use '../../../styles/tokens' as *;

  .limits {
    display: flex; flex-direction: column; gap: $space-2; padding: $space-3;
    border-radius: $radius-md; background: var(--surface-3); border: 1px solid var(--border);
  }
  .limits-head { display: flex; align-items: center; gap: 6px; font-size: $fs-sm; color: var(--text-muted); font-weight: $fw-medium; }
  .limits-note { margin-left: auto; font-size: $fs-xs; color: var(--state-warn); }
  .limits-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: $space-3; }
  .field { display: flex; flex-direction: column; gap: 5px; }
  .lbl { font-size: $fs-xs; color: var(--text-muted); font-weight: $fw-medium; em { font-style: normal; color: var(--text-faint); font-weight: $fw-regular; } }

  input {
    width: 100%; box-sizing: border-box; border-radius: $radius-sm; border: 1px solid var(--border);
    background: var(--surface); color: var(--text); padding: 7px $space-2; font-size: $fs-sm; font-family: $font-mono;
    transition: border-color $dur-fast $ease-out, box-shadow $dur-fast $ease-out;
    &::placeholder { color: var(--text-faint); }
    &:focus { outline: none; border-color: var(--accent); box-shadow: 0 0 0 2px var(--focus-ring); }
    &:disabled { opacity: 0.6; }
  }

  .limits-hint { font-size: $fs-xs; color: var(--text-muted); margin: 0; }
  .limits-save {
    align-self: flex-start; display: inline-flex; align-items: center; gap: 5px; padding: 6px $space-3;
    border-radius: $radius-sm; border: 1px solid var(--border); background: var(--surface);
    color: var(--text); cursor: pointer; font-size: $fs-xs; font-weight: $fw-medium;
    transition: background $dur-fast $ease-out, border-color $dur-fast $ease-out;
    @include focus-ring;
    &:not(:disabled):hover { background: var(--surface-3); border-color: var(--border-strong); }
    &:disabled { opacity: 0.5; cursor: not-allowed; }
  }

  @media (max-width: 520px) { .limits-grid { grid-template-columns: 1fr; } }
</style>