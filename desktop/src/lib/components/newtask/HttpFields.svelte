<script lang="ts">
  // HTTP advanced settings. The URL itself is entered at the top level of the
  // unified new-task form (the single source box), so this component only
  // holds the optional headers/cookie/referer/user-agent fields.
  import Field from '../common/Field.svelte';
  import Grid2 from '../common/Grid2.svelte';

  export let headers = '';
  export let cookie = '';
  export let referer = '';
  export let userAgent = '';
</script>

<Field label="Custom headers">
  <textarea class="fx-textarea" bind:value={headers} rows="3" placeholder="Accept-Language: en-US&#10;X-Custom: value" spellcheck="false"></textarea>
</Field>

<Grid2>
  <Field label="Cookie" hint="sensitive">
    <input class="fx-input" bind:value={cookie} type="password" placeholder="session=…" autocomplete="off" spellcheck="false" />
  </Field>
  <Field label="Referer">
    <input class="fx-input" bind:value={referer} placeholder="https://example.com" autocomplete="off" spellcheck="false" />
  </Field>
</Grid2>

<Field label="User-Agent">
  <input class="fx-input" bind:value={userAgent} placeholder="Mozilla/5.0 …" autocomplete="off" spellcheck="false" />
</Field>

<style lang="scss">
  @use '../../../styles/tokens' as *;
</style>