<script lang="ts">
  // FTP / SFTP advanced settings. The URL itself is entered at the top level of
  // the unified new-task form; this component holds only the credentials. The
  // parent infers FTP vs SFTP from the URL's scheme and passes showSftpOptions.
  import Field from '../common/Field.svelte';
  import Grid2 from '../common/Grid2.svelte';

  export let username = '';
  export let password = '';
  export let privateKeyPath = '';
  export let privateKeyPassphrase = '';
  export let showSftpOptions = false;
</script>

<Grid2>
  <Field label="Username" hint="sensitive">
    <input class="fx-input" bind:value={username} autocomplete="off" />
  </Field>
  <Field label="Password" hint="sensitive">
    <input class="fx-input" bind:value={password} type="password" autocomplete="off" />
  </Field>
</Grid2>

{#if showSftpOptions}
  <Grid2>
    <Field label="Private key path" hint="optional">
      <input class="fx-input" bind:value={privateKeyPath} placeholder="~/.ssh/id_ed25519" autocomplete="off" spellcheck="false" />
    </Field>
    <Field label="Key passphrase" hint="sensitive">
      <input class="fx-input" bind:value={privateKeyPassphrase} type="password" autocomplete="off" />
    </Field>
  </Grid2>
{/if}

<style lang="scss">
  @use '../../../styles/tokens' as *;
</style>