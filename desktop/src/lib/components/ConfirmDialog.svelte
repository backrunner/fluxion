<script lang="ts">
  import { AlertTriangle } from '@lucide/svelte';

  export let open: boolean;
  export let title = 'Confirm';
  export let message = '';
  export let confirmLabel = 'Delete';
  export let cancelLabel = 'Cancel';
  export let busy = false;
  // Optional secondary destructive toggle (e.g. delete files).
  export let secondaryLabel: string | null = null;
  export let secondaryChecked = false;

  export let onConfirm: () => void = () => {};
  export let onCancel: () => void = () => {};

  function handleKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && !busy) onCancel();
  }
</script>

{#if open}
  <div class="overlay" on:click|self={onCancel} on:keydown={handleKey} role="presentation">
    <div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="cd-title">
      <div class="icon"><AlertTriangle size={20} /></div>
      <h3 id="cd-title">{title}</h3>
      <p>{message}</p>

      {#if secondaryLabel}
        <label class="secondary">
          <input type="checkbox" bind:checked={secondaryChecked} disabled={busy} />
          <span>{secondaryLabel}</span>
        </label>
      {/if}

      <div class="actions">
        <button on:click={onCancel} disabled={busy}>{cancelLabel}</button>
        <button class="danger" on:click={onConfirm} disabled={busy}>{confirmLabel}</button>
      </div>
    </div>
  </div>
{/if}

<style lang="scss">
  @use '../../styles/tokens' as *;

  .overlay {
    position: fixed;
    inset: 0;
    background: var(--scrim);
    backdrop-filter: blur(4px);
    -webkit-backdrop-filter: blur(4px);
    display: grid;
    place-items: center;
    padding: $space-6;
    z-index: 60;
  }

  .dialog {
    width: min(420px, 100%);
    display: flex;
    flex-direction: column;
    gap: $space-3;
    padding: $space-6;
    border-radius: $radius-xl;
    background: var(--elevated);
    border: 1px solid var(--border-strong);
    box-shadow: var(--shadow-lg), var(--inner-highlight);
    animation: pop $dur-base $ease-out;
  }

  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.98);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  .icon {
    width: 44px;
    height: 44px;
    border-radius: $radius-md;
    display: grid;
    place-items: center;
    background: var(--state-bad-bg);
    color: var(--state-bad);
    border: 1px solid rgba(248, 113, 113, 0.24);
    box-shadow: var(--inner-highlight);
  }

  h3 {
    font-size: $fs-lg;
  }

  p {
    color: var(--text-muted);
    font-size: $fs-sm;
    line-height: $lh-normal;
  }

  .secondary {
    display: flex;
    align-items: center;
    gap: $space-2;
    padding: $space-3;
    border-radius: $radius-md;
    background: var(--surface-3);
    border: 1px solid var(--border);
    box-shadow: var(--inner-highlight);
    font-size: $fs-sm;
    color: var(--text);
    cursor: pointer;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: $space-2;
    margin-top: $space-2;
  }

  button {
    padding: $space-2 $space-4;
    border-radius: $radius-md;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    cursor: pointer;
    font-size: $fs-sm;
    transition: background $dur-fast $ease-out, border-color $dur-fast $ease-out;
    @include focus-ring;

    &:not(:disabled):hover {
      background: var(--surface-3);
      border-color: var(--border-strong);
    }
    &:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
  }

  .danger {
    background: var(--state-bad);
    border-color: var(--state-bad);
    color: #fff;

    &:not(:disabled):hover {
      filter: brightness(1.08);
      background: var(--state-bad);
    }
  }
</style>