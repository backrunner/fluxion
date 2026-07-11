<script lang="ts">
  import Field from '../common/Field.svelte';
  import Grid2 from '../common/Grid2.svelte';
  import { t } from '../../i18n';

  // Advanced settings shared across all task kinds: per-task rate limits and
  // the proxy policy. Save directory, filename, and max connections are handled
  // at the top level of the unified new-task form, so they are NOT here.
  export let downloadLimit = '';
  export let uploadLimit = '';
  export let useSystemProxy = true;
</script>

<Grid2>
  <Field label={$t('settings.downloadLimit')} hint="bytes/s">
    <input class="fx-input" bind:value={downloadLimit} type="number" min="1" step="1" placeholder={$t('common.unlimited')} autocomplete="off" />
  </Field>
  <Field label={$t('settings.uploadLimit')} hint="bytes/s">
    <input class="fx-input" bind:value={uploadLimit} type="number" min="1" step="1" placeholder={$t('common.unlimited')} autocomplete="off" />
  </Field>
</Grid2>

<label class="toggle">
  <input type="checkbox" bind:checked={useSystemProxy} />
  <span>{$t('settings.proxy.useSystem')}</span>
</label>

<style lang="scss">
  @use '../../../styles/tokens' as *;

  .toggle {
    display: flex;
    align-items: center;
    gap: $space-2;
    font-size: $fs-sm;
    color: var(--text);
    cursor: pointer;
    padding: $space-1 0;

    input {
      width: 16px;
      height: 16px;
      accent-color: var(--accent);
      cursor: pointer;
    }
  }
</style>
