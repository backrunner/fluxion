<script lang="ts">
  // FTP / SFTP advanced settings. The URL itself is entered at the top level of
  // the unified new-task form; this component holds only the credentials. The
  // parent infers FTP vs SFTP from the URL's scheme and passes showSftpOptions.
  import Field from '../common/Field.svelte';
  import Grid2 from '../common/Grid2.svelte';
  import { t } from '../../i18n';

  export let username = '';
  export let password = '';
  export let privateKeyPath = '';
  export let privateKeyPassphrase = '';
  export let showSftpOptions = false;
</script>

<Grid2>
  <Field label={$t('credentials.username')} hint={$t('common.sensitive')}>
    <input class="fx-input" bind:value={username} autocomplete="off" />
  </Field>
  <Field label={$t('credentials.password')} hint={$t('common.sensitive')}>
    <input class="fx-input" bind:value={password} type="password" autocomplete="off" />
  </Field>
</Grid2>

{#if showSftpOptions}
  <Grid2>
    <Field label={$t('fields.privateKeyPath')} hint={$t('common.optional')}>
      <input class="fx-input" bind:value={privateKeyPath} placeholder="~/.ssh/id_ed25519" autocomplete="off" spellcheck="false" />
    </Field>
    <Field label={$t('credentials.keyPassphrase')} hint={$t('common.sensitive')}>
      <input class="fx-input" bind:value={privateKeyPassphrase} type="password" autocomplete="off" />
    </Field>
  </Grid2>
{/if}

<style lang="scss">
  @use '../../../styles/tokens' as *;
</style>
