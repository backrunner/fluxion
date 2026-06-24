<script lang="ts">
  // Shared form field wrapper: label + input/textarea slot.
  export let label = '';
  export let hint: string | null = null; // optional inline hint (e.g. "bytes/s")
</script>

<label class="field">
  <span class="lbl">
    {label}
    {#if hint}<em>{hint}</em>{/if}
  </span>
  <slot />
</label>

<style lang="scss">
  @use '../../../styles/tokens' as *;

  .field {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .lbl {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: $fs-xs;
    color: var(--text-muted);
    font-weight: $fw-medium;

    em {
      font-style: normal;
      color: var(--text-faint);
      font-weight: $fw-regular;
    }
  }
</style>