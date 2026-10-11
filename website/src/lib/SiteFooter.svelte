<script lang="ts">
  import type { SvedocsThemeContext } from 'svedocs/theme/types';
  import { resolveLocalizedHref } from 'svedocs/theme/headless';
  export let context: SvedocsThemeContext;
  $: footer = context.config.theme.footer;
</script>
{#if footer !== false}
  <footer class="sd-footer" data-theme-component="footer">
    <span>{context.t('footer.text', { site: 'Fluxion' })}</span>
    <nav aria-label={context.t('nav.footer')}>
      {#each footer.links as link}<a href={resolveLocalizedHref(link.href, context)}>{link.labelKey ? context.t(link.labelKey) : link.label}</a>{/each}
    </nav>
  </footer>
{/if}
