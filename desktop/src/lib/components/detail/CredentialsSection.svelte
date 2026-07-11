<script lang="ts">
  import { Info, Eye, EyeOff, Hash } from '@lucide/svelte';
  import type { TaskDetail } from '../../types';
  import { t } from '../../i18n';

  export let detail: TaskDetail;

  let revealCreds = false;

  const sensitiveHeaders = new Set(['authorization', 'cookie', 'proxy-authorization', 'set-cookie']);

  $: sensitiveEntries = (detail.credentials.headers ?? []).filter((h) =>
    sensitiveHeaders.has(h.name.toLowerCase())
  );
  $: safeEntries = (detail.credentials.headers ?? []).filter(
    (h) => !sensitiveHeaders.has(h.name.toLowerCase())
  );

  function maskValue(value: string): string {
    if (!value) return '';
    if (value.length <= 6) return '••••••';
    return `${value.slice(0, 3)}${'•'.repeat(Math.min(12, value.length))}`;
  }
</script>

{#if sensitiveEntries.length > 0 || detail.credentials.username || detail.credentials.password || detail.credentials.private_key_passphrase}
  <section class="creds">
    <div class="creds-head">
      <Info size={13} />
      <span>{$t('credentials.title')}</span>
      <button class="reveal" on:click={() => (revealCreds = !revealCreds)}>
        {#if revealCreds}<EyeOff size={13} /> {$t('common.hide')}{:else}<Eye size={13} /> {$t('common.reveal')}{/if}
      </button>
    </div>
    <dl class="grid">
      {#each sensitiveEntries as entry}
        <div class="dl-row"><dt>{entry.name}</dt><dd class="mono">{revealCreds ? entry.value : maskValue(entry.value)}</dd></div>
      {/each}
      {#if detail.credentials.username}
        <div class="dl-row"><dt>{$t('credentials.username')}</dt><dd class="mono">{revealCreds ? detail.credentials.username : maskValue(detail.credentials.username)}</dd></div>
      {/if}
      {#if detail.credentials.password}
        <div class="dl-row"><dt>{$t('credentials.password')}</dt><dd class="mono">{revealCreds ? detail.credentials.password : '••••••'}</dd></div>
      {/if}
      {#if detail.credentials.private_key_passphrase}
        <div class="dl-row"><dt>{$t('credentials.keyPassphrase')}</dt><dd class="mono">••••••</dd></div>
      {/if}
    </dl>
  </section>
{/if}

{#if safeEntries.length > 0}
  <section class="creds">
    <div class="creds-head"><Hash size={13} /><span>{$t('credentials.requestHeaders')}</span></div>
    <dl class="grid">
      {#each safeEntries as entry}
        <div class="dl-row"><dt>{entry.name}</dt><dd class="mono">{entry.value}</dd></div>
      {/each}
    </dl>
  </section>
{/if}

<style lang="scss">
  @use '../../../styles/tokens' as *;

  .creds {
    display: flex; flex-direction: column; gap: $space-2; padding: $space-3;
    border-radius: $radius-md; background: var(--surface-3); border: 1px solid var(--border);
    box-shadow: var(--inner-highlight);
  }
  .creds-head { display: flex; align-items: center; gap: 6px; font-size: $fs-sm; color: var(--text-muted); font-weight: $fw-medium; }

  .reveal {
    margin-left: auto; display: inline-flex; align-items: center; gap: 4px; padding: 3px $space-2;
    border-radius: $radius-sm; border: 1px solid var(--border); background: var(--surface);
    color: var(--text-muted); cursor: pointer; font-size: $fs-xs;
    @include focus-ring;
    &:hover { color: var(--text-strong); }
  }

  .grid { display: grid; grid-template-columns: 1fr; gap: 0; margin: 0; }
  .dl-row {
    display: grid; grid-template-columns: 140px 1fr; gap: $space-3; padding: $space-2 0;
    border-bottom: 1px solid var(--border); align-items: center;
    &:last-child { border-bottom: none; }
  }
  dt { color: var(--text-muted); font-size: $fs-sm; @include hide-overflow; }
  dd { margin: 0; color: var(--text-strong); font-size: $fs-sm; word-break: break-word; }
  .mono { font-family: $font-mono; font-variant-numeric: tabular-nums; }
</style>
