<script lang="ts">
  import {
    Download,
    Trash2,
    Settings,
    ArrowDown,
    ArrowUp
  } from '@lucide/svelte';
  import { aggregateSpeed } from '../stores/tasks';
  import { formatSpeed } from '../format';
  import { taskList, trashedTaskIds } from '../stores/tasks';
  import type { SidebarFilter } from '../types';
  import { t } from '../i18n';
  import { windowDrag } from '../windowDrag';

  export let filter: SidebarFilter;
  export let onFilter: (f: SidebarFilter) => void;

  $: trashIdSet = new Set($trashedTaskIds);
  $: visibleTasks = $taskList.filter((t) => !trashIdSet.has(t.id));
  $: trashCount = $trashedTaskIds.length;

  type Category = { key: SidebarFilter; label: string; Icon: typeof Download; count: number };
  let categories: Category[] = [];

  $: categories = [
    { key: 'all', label: $t('nav.downloads'), Icon: Download, count: visibleTasks.length },
    { key: 'trash', label: $t('nav.trash'), Icon: Trash2, count: trashCount }
  ];

  $: downHas = $aggregateSpeed.down > 0;
  $: upHas = $aggregateSpeed.up > 0;
</script>

<aside class="sidebar">
  <!-- macOS traffic-light drag region. -->
  <div class="titlebar" data-tauri-drag-region="true" use:windowDrag></div>

  <nav class="nav">
    {#each categories as cat}
      <button
        class:active={filter === cat.key}
        on:click={() => onFilter(cat.key)}
        aria-current={filter === cat.key ? 'page' : undefined}
      >
        <cat.Icon size={16} />
        <span>{cat.label}</span>
        {#if cat.key !== 'all' && cat.count > 0}
          <span class="badge">{cat.count}</span>
        {/if}
      </button>
    {/each}
  </nav>

  <div class="spacer"></div>

  <div class="speed-card">
    <div class="speed-row">
      <span class="speed-label"><ArrowDown size={11} class="dir dn" /> {$t('nav.down')}</span>
      <span class="speed-value">{downHas ? formatSpeed($aggregateSpeed.down) : '-'}</span>
    </div>
    <div class="speed-divider" aria-hidden="true"></div>
    <div class="speed-row">
      <span class="speed-label"><ArrowUp size={11} class="dir up" /> {$t('nav.up')}</span>
      <span class="speed-value">{upHas ? formatSpeed($aggregateSpeed.up) : '-'}</span>
    </div>
  </div>

  <button
    class="settings-btn"
    class:active={filter === 'settings'}
    on:click={() => onFilter('settings')}
    aria-current={filter === 'settings' ? 'page' : undefined}
  >
    <Settings size={16} />
    <span>{$t('nav.settings')}</span>
  </button>
</aside>

<style lang="scss">
  @use '../../styles/tokens' as *;

  .sidebar {
    display: flex;
    flex-direction: column;
    gap: $space-1;
    padding: 0 10px $space-3;
    background: var(--sidebar);
    border-right: 1px solid var(--border);
    overflow: hidden;
  }

  .titlebar {
    height: 46px;
    margin: 0 -10px 2px;
    flex: none;
  }

  .nav {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .nav button,
  .settings-btn {
    display: flex;
    align-items: center;
    gap: $space-2;
    min-height: 34px;
    padding: 7px 10px;
    border-radius: $radius-lg;
    @include glass-hover-control;
    color: var(--text-muted);
    cursor: pointer;
    width: 100%;
    text-align: left;
    font-size: $fs-sm;
    position: relative;
    transition: background $dur-fast $ease-out, color $dur-fast $ease-out,
      border-color $dur-fast $ease-out, box-shadow $dur-fast $ease-out,
      transform $dur-fast $ease-out;
    @include focus-ring;
  }

  .nav button:hover,
  .settings-btn:hover {
    color: var(--text-strong);
  }

  .nav button.active,
  .settings-btn.active {
    background: var(--selected-gradient);
    color: var(--text-strong);
    border-color: transparent;
    font-weight: $fw-semibold;
    -webkit-backdrop-filter: blur(14px) saturate(150%);
    backdrop-filter: blur(14px) saturate(150%);
    box-shadow: var(--glass-rim), var(--selected-shadow);
  }

  .badge {
    margin-left: auto;
    min-width: 18px;
    height: 17px;
    padding: 0 5px;
    border-radius: $radius-xs;
    background: color-mix(in srgb, var(--sidebar-active) 72%, var(--surface-3) 28%);
    color: var(--text-muted);
    font-size: 10px;
    font-weight: $fw-semibold;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    transition: background $dur-fast $ease-out, color $dur-fast $ease-out;
    font-variant-numeric: tabular-nums;
  }

  .nav button.active .badge {
    background: var(--accent-soft);
    color: var(--accent);
  }

  .spacer {
    flex: 1;
    min-height: $space-2;
  }

  .speed-card {
    display: flex;
    flex-direction: column;
    gap: 0;
    padding: $space-2 10px;
    border-radius: $radius-lg;
    background: var(--surface-gradient);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-sm), var(--inner-highlight);
  }

  .speed-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: $fs-xs;
    padding: 3px 0;
  }

  .speed-label {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--text-faint);
    letter-spacing: 0;
    font-weight: $fw-medium;
  }

  :global(.dir.dn) { color: var(--state-good); }
  :global(.dir.up) { color: var(--cool); }

  .speed-divider {
    height: 1px;
    background: var(--border);
    margin: 2px 0;
  }

  .speed-value {
    font-family: $font-mono;
    color: var(--text-strong);
    font-size: $fs-xs;
    font-variant-numeric: tabular-nums;
  }

  .settings-btn {
    color: var(--text-muted);
  }
</style>
