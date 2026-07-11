<script lang="ts">
  // BT advanced settings. The magnet link and .torrent file path are entered
  // at the top level of the unified new-task form, so this component only
  // holds extra trackers, connection limit, share ratio, and seeding toggle.
  import Field from '../common/Field.svelte';
  import Grid2 from '../common/Grid2.svelte';
  import { t } from '../../i18n';

  export let trackers = '';
  export let maxConnections = '';
  export let shareRatio = '';
  export let enableSeeding = true;
</script>

<Grid2>
  <Field label={$t('new.maxConnections')} hint={$t('common.optional')}>
    <input class="fx-input" bind:value={maxConnections} type="number" min="1" step="1" placeholder={$t('common.unlimited')} autocomplete="off" />
  </Field>
  <Field label={$t('fields.shareRatio')} hint={$t('fields.exampleRatio')}>
    <input class="fx-input" bind:value={shareRatio} type="number" min="0.01" step="any" placeholder={$t('fields.noLimit')} autocomplete="off" />
  </Field>
</Grid2>

<Field label={$t('fields.extraTrackers')} hint={$t('common.onePerLine')}>
  <textarea class="fx-textarea" bind:value={trackers} rows="3" placeholder="udp://tracker.opentrackr.org:1337/announce"></textarea>
</Field>

<label class="toggle">
  <input type="checkbox" bind:checked={enableSeeding} />
  <span>{$t('fields.seedAfter')}</span>
</label>

<p class="note">{$t('fields.btFileHint')}</p>

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

  .note {
    font-size: $fs-xs;
    color: var(--text-faint);
    margin: 0;
  }
</style>
