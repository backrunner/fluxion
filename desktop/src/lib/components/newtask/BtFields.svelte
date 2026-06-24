<script lang="ts">
  // BT advanced settings. The magnet link and .torrent file path are entered
  // at the top level of the unified new-task form, so this component only
  // holds extra trackers, connection limit, share ratio, and seeding toggle.
  import Field from '../common/Field.svelte';
  import Grid2 from '../common/Grid2.svelte';

  export let trackers = '';
  export let maxConnections = '';
  export let shareRatio = '';
  export let enableSeeding = true;
</script>

<Grid2>
  <Field label="Max connections" hint="optional">
    <input class="fx-input" bind:value={maxConnections} inputmode="numeric" placeholder="unlimited" autocomplete="off" />
  </Field>
  <Field label="Share ratio stop" hint="e.g. 1.0">
    <input class="fx-input" bind:value={shareRatio} inputmode="decimal" placeholder="no limit" autocomplete="off" />
  </Field>
</Grid2>

<Field label="Extra trackers" hint="one per line">
  <textarea class="fx-textarea" bind:value={trackers} rows="3" placeholder="udp://tracker.opentrackr.org:1337/announce"></textarea>
</Field>

<label class="toggle">
  <input type="checkbox" bind:checked={enableSeeding} />
  <span>Seed after completion</span>
</label>

<p class="note">File selection is available in the task detail view once the torrent metadata is parsed.</p>

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