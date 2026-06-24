<script lang="ts">
  import {
    Layers,
    Download,
    CheckCircle2,
    XCircle,
    Trash2,
    OctagonPause,
    Settings,
    HardDriveDownload
  } from '@lucide/svelte';
  import { aggregateSpeed } from '../stores/tasks';
  import { formatSpeed } from '../format';
  import { taskList, trashedTaskIds } from '../stores/tasks';
  import type { SidebarFilter } from '../types';

  export let filter: SidebarFilter;
  export let onFilter: (f: SidebarFilter) => void;

  $: trashIdSet = new Set($trashedTaskIds);
  $: visibleTasks = $taskList.filter((t) => !trashIdSet.has(t.id));
  $: completedCount = visibleTasks.filter((t) => t.state === 'Completed').length;
  $: downloadingCount = visibleTasks.filter((t) => t.state === 'Downloading' || t.state === 'Seeding' || t.state === 'Resolving' || t.state === 'Queued').length;
  $: failedCount = visibleTasks.filter((t) => t.state === 'Failed').length;
  $: stoppedCount = visibleTasks.filter((t) => t.state === 'Stopped').length;
  $: trashCount = $trashedTaskIds.length;

  type Category = { key: SidebarFilter; label: string; Icon: typeof Layers; count: number };
  let categories: Category[] = [];

  $: categories = [
    { key: 'all', label: 'All Tasks', Icon: Layers, count: visibleTasks.length },
    { key: 'downloading', label: 'Downloading', Icon: Download, count: downloadingCount },
    { key: 'completed', label: 'Completed', Icon: CheckCircle2, count: completedCount },
    { key: 'failed', label: 'Failed', Icon: XCircle, count: failedCount },
    { key: 'stopped', label: 'Stopped', Icon: OctagonPause, count: stoppedCount },
    { key: 'trash', label: 'Trash', Icon: Trash2, count: trashCount }
  ];
</script>

<aside class="sidebar">
  <!-- macOS traffic-light drag region. -->
  <div class="titlebar" data-tauri-drag-region="true"></div>

  <div class="brand">
    <div class="mark" aria-hidden="true">
      <HardDriveDownload size={18} strokeWidth={2.2} />
    </div>
    <div class="brand-text">
      <h1>Fluxion</h1>
      <p>Download control</p>
    </div>
  </div>

  <nav class="nav">
    {#each categories as cat}
      <button
        class:active={filter === cat.key}
        on:click={() => onFilter(cat.key)}
        aria-current={filter === cat.key ? 'page' : undefined}
      >
        <cat.Icon size={16} />
        <span>{cat.label}</span>
        {#if cat.count > 0}
          <span class="badge">{cat.count}</span>
        {/if}
      </button>
    {/each}
  </nav>

  <div class="spacer"></div>

  <div class="speed-card">
    <div class="speed-row">
      <span class="speed-label">Down</span>
      <span class="speed-value">{$aggregateSpeed.down ? formatSpeed($aggregateSpeed.down) : '—'}</span>
    </div>
    <div class="speed-row">
      <span class="speed-label">Up</span>
      <span class="speed-value">{$aggregateSpeed.up ? formatSpeed($aggregateSpeed.up) : '—'}</span>
    </div>
  </div>

  <button
    class="settings-btn"
    class:active={filter === 'settings'}
    on:click={() => onFilter('settings')}
    aria-current={filter === 'settings' ? 'page' : undefined}
  >
    <Settings size={16} />
    <span>Settings</span>
  </button>
</aside>

<style lang="scss">
  @use '../../styles/tokens' as *;

  .sidebar {
    display: flex;
    flex-direction: column;
    gap: $space-1;
    padding: 0 $space-2 $space-3;
    // Layered depth: a faint vertical sheen over the surface plus the
    // material edge highlight, so the sidebar reads as a distinct panel
    // rather than a flat strip.
    background:
      linear-gradient(180deg, color-mix(in srgb, var(--surface) 92%, var(--accent) 8%), var(--surface) 38%),
      var(--surface);
    border-right: 1px solid var(--border);
    box-shadow: var(--inner-highlight), 1px 0 0 rgba(0, 0, 0, 0.04);
    overflow: hidden;
  }

  // Drag strip for the macOS traffic lights.
  .titlebar {
    height: 52px;
    flex: none;
    -webkit-app-region: drag;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: $space-2;
    padding: $space-2 $space-2 $space-4;
  }

  .mark {
    width: 36px;
    height: 36px;
    border-radius: $radius-md;
    display: grid;
    place-items: center;
    background: linear-gradient(145deg, var(--accent-hover), var(--accent-press));
    color: var(--accent-contrast);
    box-shadow: var(--shadow-glow), var(--inner-highlight);
    flex: none;
  }

  .brand-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .brand-text h1 {
    font-size: $fs-base;
    font-weight: $fw-semibold;
    letter-spacing: -0.01em;
  }

  .brand-text p {
    font-size: $fs-xs;
    color: var(--text-muted);
  }

  .nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .nav button,
  .settings-btn {
    display: flex;
    align-items: center;
    gap: $space-2;
    padding: 8px $space-2;
    border-radius: $radius-sm;
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    width: 100%;
    text-align: left;
    font-size: $fs-sm;
    position: relative;
    transition: background $dur-fast $ease-out, color $dur-fast $ease-out,
      border-color $dur-fast $ease-out;
    @include focus-ring;
  }

  .nav button:hover,
  .settings-btn:hover {
    background: var(--surface-3);
    color: var(--text-strong);
  }

  // Active nav item: a soft accent-tinted pill with a crisp left edge bar so
  // the current section is unambiguous at a glance.
  .nav button.active,
  .settings-btn.active {
    background: var(--accent-soft);
    color: var(--accent);
    border-color: rgba(249, 115, 22, 0.24);

    &::before {
      content: '';
      position: absolute;
      left: -#{$space-2};
      top: 50%;
      transform: translateY(-50%);
      width: 3px;
      height: 18px;
      border-radius: $radius-pill;
      background: var(--accent);
    }
  }

  .badge {
    margin-left: auto;
    min-width: 18px;
    height: 16px;
    padding: 0 5px;
    border-radius: $radius-pill;
    background: var(--surface-3);
    color: var(--text-muted);
    font-size: 10px;
    font-weight: $fw-semibold;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    transition: background $dur-fast $ease-out, color $dur-fast $ease-out;
  }

  .nav button.active .badge {
    background: var(--accent);
    color: var(--accent-contrast);
  }

  .spacer {
    flex: 1;
    min-height: $space-2;
  }

  .speed-card {
    display: flex;
    flex-direction: column;
    gap: $space-1;
    padding: $space-3;
    border-radius: $radius-md;
    background: var(--surface-3);
    border: 1px solid var(--border);
    box-shadow: var(--inner-highlight);
  }

  .speed-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    font-size: $fs-xs;
  }

  .speed-label {
    color: var(--text-faint);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-weight: $fw-medium;
  }

  .speed-value {
    font-family: $font-mono;
    color: var(--text-strong);
    font-size: $fs-sm;
    font-variant-numeric: tabular-nums;
  }

  .settings-btn {
    color: var(--text-muted);
  }
</style>
