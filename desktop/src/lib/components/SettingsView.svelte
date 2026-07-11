<script lang="ts">
  import { Save, Gauge, Network, Magnet, Palette, Sun, Moon, Languages, RefreshCw } from '@lucide/svelte';
  import type { SettingsSnapshot } from '../types';
  import { settingsStore, hydrateSettingsForm, settingsFormToSnapshot, type SettingsForm } from '../stores/settings';
  import { themeStore } from '../stores/theme';
  import { updateSettings } from '../api';
  import Select from './common/Select.svelte';
  import { languageOptions, locale, t, type Locale } from '../i18n';
  import { windowDrag } from '../windowDrag';
  import type { UpdateCheckState } from '../updater';

  export let busy: boolean;
  export let onError: (message: string) => void = () => {};
  export let onSaved: () => void = () => {};
  export let updateCheckState: UpdateCheckState = 'idle';
  export let updateVersion: string | null = null;
  export let updateCheckError = '';
  export let onCheckForUpdates: () => void | Promise<void> = () => {};

  let form: SettingsForm = hydrateSettingsForm({
    download_limit: null,
    upload_limit: null,
    use_system_proxy: true,
    bt_trackers: [],
    bt_ip_allow: [],
    bt_ip_deny: []
  });

  // Re-hydrate when the store snapshot changes.
  $: if ($settingsStore) {
    form = hydrateSettingsForm($settingsStore);
  }

  $: isDark = $themeStore === 'dark';
  $: ratesValid = [form.downloadLimit, form.uploadLimit].every((value) => {
    if (!value.trim()) return true;
    const number = Number(value);
    return Number.isSafeInteger(number) && number > 0;
  });

  async function save() {
    if (busy || !$settingsStore) return;
    try {
      await updateSettings(settingsFormToSnapshot(form, $settingsStore));
      onSaved();
    } catch (cause) {
      onError(cause instanceof Error ? cause.message : String(cause));
    }
  }

</script>

<div class="settings-view">
  <header class="settings-toolbar" data-tauri-drag-region="true" use:windowDrag>
    <h2 data-tauri-drag-region="true">{$t('settings.title')}</h2>
    <button class="primary save-button" on:click={save} disabled={busy || !$settingsStore || !ratesValid}>
      <Save size={14} /> {busy ? $t('common.saving') : $t('settings.save')}
    </button>
  </header>

  <div class="settings-scroll">
  <div class="settings">
  <section class="card">
    <header class="card-head">
      <span class="card-icon"><Gauge size={16} /></span>
      <div>
        <h3>{$t('settings.transfer.title')}</h3>
        <p>{$t('settings.transfer.copy')}</p>
      </div>
    </header>
    <div class="grid-2">
      <label class="field">
        <span class="lbl">{$t('settings.downloadLimit')} <em>bytes/s</em></span>
        <input class="fx-input" bind:value={form.downloadLimit} type="number" min="1" step="1" placeholder={$t('common.unlimited')} />
      </label>
      <label class="field">
        <span class="lbl">{$t('settings.uploadLimit')} <em>bytes/s</em></span>
        <input class="fx-input" bind:value={form.uploadLimit} type="number" min="1" step="1" placeholder={$t('common.unlimited')} />
      </label>
    </div>
  </section>

  <section class="card">
    <header class="card-head">
      <span class="card-icon"><Network size={16} /></span>
      <div>
        <h3>{$t('settings.proxy.title')}</h3>
        <p>{$t('settings.proxy.copy')}</p>
      </div>
    </header>
    <label class="toggle">
      <input type="checkbox" bind:checked={form.useSystemProxy} />
      <span>{$t('settings.proxy.useSystem')}</span>
    </label>
  </section>

  <section class="card">
    <header class="card-head">
      <span class="card-icon"><Magnet size={16} /></span>
      <div>
        <h3>{$t('settings.bt.title')}</h3>
        <p>{$t('settings.bt.copy')}</p>
      </div>
    </header>
    <div class="grid-1">
      <label class="field">
        <span class="lbl">{$t('settings.bt.trackers')}</span>
        <textarea class="fx-textarea" bind:value={form.btTrackers} rows="5" placeholder="udp://tracker.opentrackr.org:1337/announce"></textarea>
      </label>
    </div>
  </section>

  <section class="card appearance">
    <header class="card-head">
      <span class="card-icon"><Palette size={16} /></span>
      <div class="appearance-title">
        <h3>{$t('settings.appearance.title')}</h3>
        <p>{$t('settings.appearance.copy')}</p>
      </div>
      <div class="appearance-controls">
        <label class="language-control">
          <Languages size={14} />
          <Select
            items={languageOptions}
            value={$locale}
            on:change={(event) => locale.set(event.detail as Locale)}
            ariaLabel={$t('settings.language')}
            size="sm"
            extraClass="language-select"
          />
        </label>
        <button
          class="theme-switch"
          class:dark={isDark}
          on:click={() => themeStore.toggle()}
          role="switch"
          aria-checked={isDark}
          aria-label={$t('settings.toggleDark')}
          title={isDark ? $t('settings.switchLight') : $t('settings.switchDark')}
        >
          <span class="switch-track">
            <Sun size={13} class="switch-icon sun" />
            <Moon size={13} class="switch-icon moon" />
            <span class="switch-thumb"></span>
          </span>
        </button>
      </div>
    </header>
  </section>

  <section class="card update-card">
    <header class="card-head">
      <span class="card-icon"><RefreshCw size={16} /></span>
      <div class="update-title">
        <h3>{$t('settings.update.title')}</h3>
        <p aria-live="polite">
          {#if updateCheckState === 'checking'}
            {$t('settings.update.checking')}
          {:else if updateCheckState === 'current'}
            <span class="status-current">{$t('settings.update.current')}</span>
          {:else if updateCheckState === 'available' && updateVersion}
            <span class="status-available">{$t('settings.update.available', { version: updateVersion })}</span>
          {:else if updateCheckState === 'error'}
            <span class="status-error">{$t('settings.update.failed', { error: updateCheckError })}</span>
          {:else}
            {$t('settings.update.copy')}
          {/if}
        </p>
      </div>
      <button
        class="check-update-button"
        on:click={onCheckForUpdates}
        disabled={busy || updateCheckState === 'checking'}
      >
        <span class:spinning={updateCheckState === 'checking'}><RefreshCw size={14} /></span>
        {updateCheckState === 'checking' ? $t('settings.update.checkingButton') : $t('settings.update.check')}
      </button>
    </header>
  </section>
  </div>
  </div>
</div>

<style lang="scss">
  @use '../../styles/tokens' as *;

  .settings-view {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    height: 100%;
    background: var(--surface-2);
  }

  .settings-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: $space-4;
    height: 48px;
    padding: 0 $space-4;
    flex: none;
    border-bottom: 1px solid var(--border);
    background: linear-gradient(180deg, var(--surface), var(--surface-2));
    box-shadow: var(--inner-highlight);

    h2 {
      min-width: 0;
      font-size: 15px;
      letter-spacing: 0;
    }
  }

  .settings-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    @include scrollbar;
  }

  .settings {
    display: flex;
    flex-direction: column;
    gap: $space-3;
    max-width: 920px;
    min-height: 100%;
    padding: $space-4;
    background: transparent;
  }

  .card {
    background: var(--surface-gradient);
    border: 1px solid var(--border);
    border-radius: $radius-xl;
    box-shadow: var(--shadow-sm), var(--inner-highlight);
    padding: $space-5;
    display: flex;
    flex-direction: column;
    gap: $space-4;
  }

  .card-head {
    display: flex;
    align-items: flex-start;
    gap: $space-3;
  }

  .card-icon {
    width: 32px;
    height: 32px;
    border-radius: $radius-sm;
    display: grid;
    place-items: center;
    background: var(--control-gradient);
    border: 1px solid var(--border);
    box-shadow: var(--inner-highlight);
    color: var(--text-muted);
    flex: none;
  }

  .card-head h3 {
    font-size: $fs-md;
    letter-spacing: 0;
  }

  .card-head p {
    margin-top: 2px;
    font-size: $fs-sm;
    color: var(--text-muted);
  }

  .grid-2 {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: $space-4;
  }

  .grid-1 {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: $space-4;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .lbl {
    font-size: $fs-xs;
    color: var(--text-muted);
    font-weight: $fw-medium;

    em {
      font-style: normal;
      color: var(--text-faint);
      font-weight: $fw-regular;
    }
  }

  input,
  textarea {
    // Layout fallback: the .fx-input/.fx-textarea classes carry the full
    // visual treatment (defined globally in base.scss). This scoped rule only
    // guarantees full-width sizing for any control that forgets the class.
    width: 100%;
    box-sizing: border-box;
  }

  .toggle {
    display: flex;
    align-items: center;
    gap: $space-2;
    font-size: $fs-sm;
    color: var(--text);
    cursor: pointer;

    input {
      width: 16px;
      height: 16px;
      accent-color: var(--accent);
      cursor: pointer;
    }
  }

  // Appearance card: title on the left, theme switch vertically centered on the right.
  .appearance .card-head {
    align-items: center;
  }

  .update-card .card-head {
    align-items: center;
  }

  .update-title {
    flex: 1;
    min-width: 0;
  }

  .update-title p {
    overflow-wrap: anywhere;
  }

  .status-current { color: var(--state-good); }
  .status-available { color: var(--accent); }
  .status-error { color: var(--state-bad); }

  .check-update-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-height: 32px;
    padding: 0 11px;
    flex: none;
    border: 1px solid var(--border-strong);
    border-radius: $radius-sm;
    background: var(--control-gradient);
    box-shadow: var(--inner-highlight);
    color: var(--text-strong);
    font-size: $fs-xs;
    font-weight: $fw-semibold;
    cursor: pointer;
    transition: border-color $dur-fast $ease-out, background $dur-fast $ease-out,
      opacity $dur-fast $ease-out;
    @include focus-ring;
  }

  .check-update-button:hover:not(:disabled) {
    border-color: var(--accent);
    background: var(--surface-3);
  }

  .check-update-button:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .check-update-button span {
    display: inline-flex;
  }

  .check-update-button span.spinning {
    animation: update-spin 900ms linear infinite;
  }

  @keyframes update-spin {
    to { transform: rotate(360deg); }
  }

  .appearance-title {
    flex: 1;
    min-width: 0;
  }

  .appearance-controls,
  .language-control {
    display: flex;
    align-items: center;
  }

  .appearance-controls {
    gap: $space-3;
    flex: none;
  }

  .language-control {
    gap: 6px;
    color: var(--text-faint);
  }

  :global(.language-select) {
    min-width: 112px;
  }

  .theme-switch {
    flex: none;
    display: grid;
    place-items: center;
    background: transparent;
    border: none;
    cursor: pointer;
    padding: 0;
    @include focus-ring;
    border-radius: $radius-pill;
  }

  .switch-track {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    width: 52px;
    height: 28px;
    padding: 0 5px;
    border-radius: $radius-pill;
    background: var(--surface-3);
    border: 1px solid var(--border-strong);
    box-shadow: var(--inner-highlight);
    transition: background $dur-base $ease-out, border-color $dur-base $ease-out;
  }

  .theme-switch.dark .switch-track {
    background: var(--accent-soft);
    border-color: var(--accent-soft-strong);
  }

  :global(.switch-icon) {
    flex: none;
    color: var(--text-faint);
    z-index: 1;
  }
  :global(.switch-icon.sun) { color: var(--state-warn); }
  :global(.switch-icon.moon) { color: var(--accent); }

  .switch-thumb {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--surface);
    box-shadow: var(--shadow-sm);
    transition: transform $dur-base $ease-out, background $dur-base $ease-out;
  }

  .theme-switch.dark .switch-thumb {
    transform: translateX(24px);
    background: var(--accent);
  }

  .primary {
    @include primary-button;
    padding: 7px 12px;
    font-size: $fs-sm;
    white-space: nowrap;
  }

  .save-button {
    flex: none;
    min-height: 30px;
  }

  @media (max-width: 720px) {
    .grid-2,
    .grid-1 {
      grid-template-columns: 1fr;
    }

    .appearance .card-head {
      align-items: flex-start;
      flex-wrap: wrap;
    }

    .appearance-controls {
      width: 100%;
      justify-content: space-between;
      padding-left: 44px;
    }
  }
</style>
