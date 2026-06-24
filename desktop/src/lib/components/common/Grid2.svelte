<script lang="ts">
  // Two-column responsive grid wrapper for form fields.
</script>

<div class="grid-2">
  <slot />
</div>

<style lang="scss">
  @use '../../../styles/tokens' as *;

  .grid-2 {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: $space-3;
  }

  @media (max-width: 560px) {
    .grid-2 {
      grid-template-columns: 1fr;
    }
  }
</style>