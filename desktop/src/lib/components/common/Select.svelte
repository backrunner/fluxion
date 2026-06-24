<script lang="ts">
  // Fluxion styled Select built on bits-ui (Radix-style, Svelte 5-ready primitives).
  // Exposes the project's familiar `export let` + `on:change` idiom so callers
  // don't need to learn bits-ui snippets internally.

  import { Select } from 'bits-ui';
  import { Check, ChevronDown } from '@lucide/svelte';

  type SelectItem = {
    value: string;
    label: string;
    disabled?: boolean;
  };

  export let items: SelectItem[] = [];
  export let value = '';
  export let placeholder = '';
  export let disabled = false;
  export let ariaLabel: string | null = null;
  export let size: 'sm' | 'md' = 'md';
  export let extraClass: string = '';

  // Match native <select> ergonomics: dispatch a `change` event with the new value.
  import { createEventDispatcher } from 'svelte';
  const dispatch = createEventDispatcher<{ change: string }>();

  function sync(next: string) {
    if (next === value) return;
    value = next;
    dispatch('change', next);
  }

  // Render the label of the currently selected item (fall back to the value).
  $: selectedLabel = items.find((item) => item.value === value)?.label ?? value;
</script>

<Select.Root
  type="single"
  {items}
  {disabled}
  bind:value
  onValueChange={(next) => sync(next as string)}
>
  <Select.Trigger class={`fx-select-trigger ${size} ${extraClass}`} aria-label={ariaLabel ?? undefined}>
    <Select.Value placeholder={placeholder}>
      {#snippet children()}
        {selectedLabel || placeholder}
      {/snippet}
    </Select.Value>
    <ChevronDown class="fx-select-caret" size={14} />
  </Select.Trigger>
  <Select.Portal>
    <Select.Content class="fx-select-content" sideOffset={4}>
      <Select.Viewport class="fx-select-viewport">
        {#each items as item (item.value)}
          <Select.Item value={item.value} label={item.label} disabled={item.disabled} class="fx-select-item">
            {#snippet children({ selected })}
              <span class="fx-select-item-label">{item.label}</span>
              <span class="fx-select-item-check" class:shown={selected}>
                <Check size={13} />
              </span>
            {/snippet}
          </Select.Item>
        {/each}
      </Select.Viewport>
    </Select.Content>
  </Select.Portal>
</Select.Root>

<style lang="scss">
  @use '../../../styles/tokens' as *;

  // Trigger ------------------------------------------------------------------
  :global(.fx-select-trigger) {
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: $space-2;
    min-width: 0;
    width: auto;
    border-radius: $radius-sm;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    font-size: $fs-xs;
    font-weight: $fw-medium;
    line-height: 1.2;
    cursor: pointer;
    user-select: none;
    transition: border-color $dur-fast $ease-out, background $dur-fast $ease-out,
      box-shadow $dur-fast $ease-out;
  }
  :global(.fx-select-trigger:hover:not([data-disabled])) {
    border-color: var(--border-strong);
    background: var(--surface-2);
  }
  :global(.fx-select-trigger[data-disabled]) {
    opacity: 0.5;
    cursor: not-allowed;
  }
  :global(.fx-select-trigger[data-state='open']) {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--focus-ring);
  }
  :global(.fx-select-trigger:focus-visible) {
    outline: none;
    box-shadow: 0 0 0 3px var(--focus-ring);
    border-color: var(--accent);
  }
  :global(.fx-select-trigger.sm) {
    height: 28px;
    padding: 0 8px;
  }
  :global(.fx-select-trigger.md) {
    height: 32px;
    padding: 0 $space-2;
  }

  :global(.fx-select-caret) {
    flex: none;
    color: var(--text-muted);
    transition: transform $dur-base $ease-out, color $dur-fast $ease-out;
  }
  :global(.fx-select-trigger[data-state='open'] .fx-select-caret) {
    transform: rotate(180deg);
    color: var(--accent);
  }

  // Content / dropdown -------------------------------------------------------
  :global(.fx-select-content) {
    z-index: 80;
    min-width: var(--bits-anchor-width, 160px);
    max-height: 280px;
    padding: 4px;
    border-radius: $radius-md;
    background: var(--elevated);
    border: 1px solid var(--border-strong);
    box-shadow: var(--shadow-lg), var(--inner-highlight);
    overflow: hidden;
    animation: fx-select-in $dur-base $ease-out;
  }

  :global(.fx-select-viewport) {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  :global(.fx-select-item) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: $space-2;
    padding: 6px $space-2;
    border-radius: $radius-sm;
    color: var(--text);
    font-size: $fs-sm;
    cursor: pointer;
    user-select: none;
    outline: none;
    transition: background $dur-fast $ease-out, color $dur-fast $ease-out;
  }
  :global(.fx-select-item[data-highlighted]) {
    background: var(--surface-3);
  }
  :global(.fx-select-item[data-selected]) {
    color: var(--text-strong);
    font-weight: $fw-semibold;
  }
  :global(.fx-select-item[data-disabled]) {
    opacity: 0.4;
    cursor: not-allowed;
  }

  :global(.fx-select-item-label) {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  :global(.fx-select-item-check) {
    display: inline-flex;
    align-items: center;
    color: var(--accent);
    opacity: 0;
    transform: scale(0.8);
    transition: opacity $dur-fast $ease-out, transform $dur-fast $ease-out;
  }
  :global(.fx-select-item-check.shown) {
    opacity: 1;
    transform: scale(1);
  }

  @keyframes fx-select-in {
    from {
      opacity: 0;
      transform: translateY(-4px) scale(0.98);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }
</style>