<script lang="ts">
  import { Copy, Check, FolderOpen } from '@lucide/svelte';
  import type { TaskDetail } from '../../types';
  import { stateKind, stateColorVar, stateBgVar } from '../../format';

  export let detail: TaskDetail;
  let copied = false;

  $: kind = stateKind(detail.task.state);

  async function copyDir() {
    try {
      await navigator.clipboard.writeText(detail.task.save_dir);
      copied = true;
      setTimeout(() => (copied = false), 1400);
    } catch {
      // Clipboard may be unavailable; ignore.
    }
  }
</script>

<header class="head">
  <div class="head-main">
    <h3 class="title" title={detail.task.file_name ?? detail.task.id}>
      {detail.task.file_name ?? detail.task.id}
    </h3>
    <button class="copy-dir" on:click={copyDir} title="Copy save directory">
      {#if copied}<Check size={14} /> <span>Copied</span>
      {:else}<Copy size={14} /> <span>Copy path</span>{/if}
    </button>
  </div>
  <p class="path" title={detail.task.save_dir}>
    <FolderOpen size={13} />
    <span class="mono">{detail.task.save_dir}</span>
  </p>
  <span class={`pill ${kind}`} style={`--row-state: ${stateColorVar(kind)}; --row-state-bg: ${stateBgVar(kind)};`}>
    {detail.task.state}
  </span>
</header>

<style lang="scss">
  @use '../../../styles/tokens' as *;

  .head { display: flex; flex-direction: column; gap: $space-2; }
  .head-main { display: flex; align-items: center; justify-content: space-between; gap: $space-3; }
  .title { font-size: $fs-lg; color: var(--text-strong); @include hide-overflow; }

  .copy-dir {
    display: inline-flex; align-items: center; gap: 5px; padding: 5px $space-2;
    border-radius: $radius-sm; border: 1px solid var(--border); background: var(--surface-3);
    color: var(--text-muted); cursor: pointer; font-size: $fs-xs; flex: none;
    box-shadow: var(--inner-highlight);
    transition: color $dur-fast $ease-out, border-color $dur-fast $ease-out;
    @include focus-ring;
    &:hover { color: var(--text-strong); border-color: var(--border-strong); }
  }

  .path { display: flex; align-items: center; gap: 6px; font-size: $fs-sm; color: var(--text-muted); min-width: 0; }
  .path .mono { font-family: $font-mono; @include hide-overflow; }

  .pill {
    @include pill; align-self: flex-start;
    color: var(--row-state); background: var(--row-state-bg);
  }
</style>