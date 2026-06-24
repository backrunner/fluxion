<script lang="ts">
  import { Save, Gauge, Network, Magnet, Palette, Sun, Moon } from '@lucide/svelte';
  import type { SettingsSnapshot } from '../types';
  import { settingsStore, hydrateSettingsForm, settingsFormToSnapshot, type SettingsForm } from '../stores/settings';
  import { themeStore } from '../stores/theme';
  import { updateSettings } from '../api';

  export let busy: boolean;
  export let onError: (message: string) => void = () => {};
  export let onSaved: () => void = () => {};

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

  async function save() {
    if (busy || !$settingsStore) return;
    try {
      await updateSettings(settingsFormToSnapshot(form, $settingsStore));
      onSaved();
    } catch (cause) {
      onError(cause instanceof Error ? cause.message : String(cause));
    }
  }

  function parseRate(value: string): number | null {
    if (!value.trim()) return null;
    const n = Number(value);
    return Number.isFinite(n) && n > 0 ? n : null;
  }
  void parseRate;
</script>

<div class="settings">
  <section class="card">
    <header class="card-head">
      <span class="card-icon"><Gauge size={16} /></span>
      <div>
        <h3>Transfer limits</h3>
        <p>Global rate caps applied to all tasks. Leave blank for unlimited.</p>
      </div>
    </header>
    <div class="grid-2">
      <label class="field">
        <span class="lbl">Download limit <em>bytes/s</em></span>
        <input class="fx-input" bind:value={form.downloadLimit} inputmode="numeric" placeholder="unlimited" />
      </label>
      <label class="field">
        <span class="lbl">Upload limit <em>bytes/s</em></span>
        <input class="fx-input" bind:value={form.uploadLimit} inputmode="numeric" placeholder="unlimited" />
      </label>
    </div>
  </section>

  <section class="card">
    <header class="card-head">
      <span class="card-icon"><Network size={16} /></span>
      <div>
        <h3>Proxy</h3>
        <p>Route transfers through the system proxy when available.</p>
      </div>
    </header>
    <label class="toggle">
      <input type="checkbox" bind:checked={form.useSystemProxy} />
      <span>Use system proxy</span>
    </label>
  </section>

  <section class="card">
    <header class="card-head">
      <span class="card-icon"><Magnet size={16} /></span>
      <div>
        <h3>BitTorrent</h3>
        <p>Trackers and peer IP policy. One entry per line.</p>
      </div>
    </header>
    <div class="grid-3">
      <label class="field">
        <span class="lbl">Tracker list</span>
        <textarea class="fx-textarea" bind:value={form.btTrackers} rows="5" placeholder="udp://tracker.opentrackr.org:1337/announce"></textarea>
      </label>
      <label class="field">
        <span class="lbl">IP allow list</span>
        <textarea class="fx-textarea" bind:value={form.btIpAllow} rows="5" placeholder="198.51.100.0/24"></textarea>
      </label>
      <label class="field">
        <span class="lbl">IP deny list</span>
        <textarea class="fx-textarea" bind:value={form.btIpDeny} rows="5" placeholder="203.0.113.0/24"></textarea>
      </label>
    </div>
  </section>

  <section class="card appearance">
    <header class="card-head">
      <span class="card-icon"><Palette size={16} /></span>
      <div class="appearance-title">
        <h3>Appearance</h3>
        <p>Choose the color theme for the interface.</p>
      </div>
      <button
        class="theme-switch"
        class:dark={isDark}
        on:click={() => themeStore.toggle()}
        role="switch"
        aria-checked={isDark}
        aria-label="Toggle dark mode"
        title={isDark ? 'Switch to light' : 'Switch to dark'}
      >
        <span class="switch-track">
          <Sun size={13} class="switch-icon sun" />
          <Moon size={13} class="switch-icon moon" />
          <span class="switch-thumb"></span>
        </span>
      </button>
    </header>
  </section>

  <div class="actions">
    <button class="primary" on:click={save} disabled={busy || !$settingsStore}>
      <Save size={15} /> {busy ? 'Saving…' : 'Save settings'}
    </button>
  </div>
</div>

<style lang="scss">
  @use '../../styles/tokens' as *;

  .settings {
    display: flex;
    flex-direction: column;
    gap: $space-4;
    max-width: 880px;
  }

  .card {
    @include surface-card;
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
    background: var(--accent-soft);
    color: var(--accent);
    flex: none;
  }

  .card-head h3 {
    font-size: $fs-md;
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

  .grid-3 {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
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

  .appearance-title {
    flex: 1;
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
    border-color: rgba(249, 115, 22, 0.4);
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
    transition: transform $dur-base $ease-out;
  }

  .theme-switch.dark .switch-thumb {
    transform: translateX(24px);
    background: var(--accent);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
  }

  .primary {
    display: inline-flex;
    align-items: center;
    gap: $space-2;
    padding: $space-3 $space-5;
    border-radius: $radius-md;
    border: none;
    background: linear-gradient(145deg, var(--accent-hover), var(--accent-press));
    color: var(--accent-contrast);
    font-weight: $fw-semibold;
    cursor: pointer;
    transition: filter $dur-fast $ease-out, box-shadow $dur-fast $ease-out,
      transform $dur-fast $ease-out;
    @include focus-ring;

    &:not(:disabled):hover {
      filter: brightness(1.06);
      box-shadow: var(--shadow-glow);
    }
    &:not(:disabled):active {
      transform: translateY(1px);
    }
    &:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
  }

  @media (max-width: 720px) {
    .grid-2,
    .grid-3 {
      grid-template-columns: 1fr;
    }
  }
</style>