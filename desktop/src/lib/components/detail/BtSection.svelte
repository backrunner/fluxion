<script lang="ts">
  import { ChevronDown, ChevronRight, Sprout, Layers } from '@lucide/svelte';
  import type { BtStateSnapshot, TaskDetail } from '../../types';
  import { formatBytes, formatSpeed, progressPercent } from '../../format';
  import { t } from '../../i18n';

  export let detail: TaskDetail;
  export let btState: BtStateSnapshot | null;

  $: isSeeding = detail.task.state === 'Seeding';
  $: ratio = btState?.seed_ratio ?? (detail.task.total_bytes && detail.task.total_bytes > 0
    ? detail.task.uploaded_bytes / detail.task.total_bytes
    : null);

  // Decode the packed piece bitfield (MSB-first within each byte) into a
  // boolean array for the grid visualization.
  $: pieces = (() => {
    if (!btState || btState.total_pieces === 0) return [] as boolean[];
    const out: boolean[] = [];
    for (let i = 0; i < btState.total_pieces; i++) {
      const byteIdx = Math.floor(i / 8);
      const bitIdx = 7 - (i % 8);
      const byte = btState.piece_haves[byteIdx] ?? 0;
      out.push(((byte >> bitIdx) & 1) === 1);
    }
    return out;
  })();

  $: haveCount = pieces.filter(Boolean).length;

  let filesOpen = true;
  let piecesOpen = true;

  function toggleFiles() { filesOpen = !filesOpen; }
  function togglePieces() { piecesOpen = !piecesOpen; }
</script>

<section class="bt-section">
  <div class="bt-summary">
    <div class="summary-card" class:seeding={isSeeding}>
      <Sprout size={15} />
      <div class="summary-body">
        <span class="summary-label">{isSeeding ? $t('state.Seeding') : $t('bt.downloadSeed')}</span>
        <span class="summary-value">{ratio != null ? $t('bt.ratio', { ratio: ratio.toFixed(3) }) : '-'}</span>
      </div>
    </div>
    <div class="summary-card">
      <Layers size={15} />
      <div class="summary-body">
        <span class="summary-label">{$t('bt.pieces')}</span>
        <span class="summary-value">{btState ? `${haveCount} / ${btState.total_pieces}` : '-'}</span>
      </div>
    </div>
  </div>

  {#if !btState}
    <p class="bt-unavailable">{$t('bt.unavailable')}</p>
  {:else}
    <!-- File list -->
    <div class="collapsible">
      <button class="collapsible-head" on:click={toggleFiles}>
        {#if filesOpen}<ChevronDown size={14} />{:else}<ChevronRight size={14} />{/if}
        <span>{$t('bt.files', { count: btState.files.length })}</span>
      </button>
      {#if filesOpen}
        <div class="file-list">
          {#each btState.files as file (file.index)}
            {@const pct = progressPercent(file.downloaded, file.size)}
            <div class="file-row">
              <span class="file-name" title={file.name}>{file.name}</span>
              <div class="file-bar" aria-hidden="true"><div class="file-bar-fill" style={`width:${pct}%`}></div></div>
              <span class="file-meta">{formatBytes(file.downloaded)} / {formatBytes(file.size)}</span>
              <span class="file-pct">{Math.round(pct)}%</span>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <!-- Piece grid -->
    <div class="collapsible">
      <button class="collapsible-head" on:click={togglePieces}>
        {#if piecesOpen}<ChevronDown size={14} />{:else}<ChevronRight size={14} />{/if}
        <span>{$t('bt.pieceMap', { have: haveCount, total: btState.total_pieces })}</span>
      </button>
      {#if piecesOpen}
        <div class="piece-grid" role="img" aria-label={$t('bt.pieceMapAria')}>
          {#each pieces as have, i (i)}
            <span class="piece" class:have title={$t('bt.pieceTitle', { index: i, status: have ? $t('bt.have') : $t('bt.missing') })}></span>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</section>

<style lang="scss">
  @use '../../../styles/tokens' as *;

  .bt-section { display: flex; flex-direction: column; gap: $space-3; }

  .bt-summary { display: grid; grid-template-columns: repeat(2, 1fr); gap: $space-2; }
  .summary-card {
    display: flex; align-items: center; gap: $space-2; padding: $space-3;
    border-radius: $radius-md; background: var(--surface); border: 1px solid var(--border);
    box-shadow: var(--inner-highlight);
    color: var(--text-muted);
    &.seeding { color: var(--state-good); border-color: color-mix(in srgb, var(--state-good) 32%, transparent); background: linear-gradient(180deg, var(--state-good-bg), transparent 74%), var(--surface); }
  }
  .summary-body { display: flex; flex-direction: column; gap: 1px; }
  .summary-label { font-size: $fs-xs; color: var(--text-faint); text-transform: uppercase; letter-spacing: 0.06em; }
  .summary-value { font-size: $fs-sm; font-family: $font-mono; color: var(--text-strong); }

  .bt-unavailable { font-size: $fs-sm; color: var(--text-faint); margin: 0; padding: $space-2 0; }

  .collapsible { display: flex; flex-direction: column; gap: $space-2; }
  .collapsible-head {
    display: flex; align-items: center; gap: 6px; padding: $space-2 0;
    background: transparent; border: none; color: var(--text-muted); cursor: pointer;
    font-size: $fs-sm; font-weight: $fw-medium; text-align: left; width: 100%;
    &:hover { color: var(--text-strong); }
  }

  .file-list { display: flex; flex-direction: column; gap: $space-1; }
  .file-row {
    display: grid; grid-template-columns: 1fr 80px auto auto; gap: $space-2; align-items: center;
    padding: $space-2 0; border-bottom: 1px solid var(--border);
    &:last-child { border-bottom: none; }
  }
  .file-name { font-size: $fs-sm; color: var(--text-strong); @include hide-overflow; }
  .file-bar { height: 5px; border-radius: $radius-pill; background: var(--surface-3); overflow: hidden; }
  .file-bar-fill { height: 100%; border-radius: inherit; background: linear-gradient(90deg, var(--state-good), color-mix(in srgb, var(--state-good) 60%, white)); transition: width $dur-slow $ease-out; }
  .file-meta { font-size: $fs-xs; color: var(--text-muted); font-family: $font-mono; white-space: nowrap; }
  .file-pct { font-size: $fs-xs; color: var(--text-strong); font-weight: $fw-medium; }

  .piece-grid {
    display: grid; grid-template-columns: repeat(auto-fill, 10px); gap: 2px;
    padding: $space-3; border-radius: $radius-md; background: var(--surface-3); border: 1px solid var(--border);
    box-shadow: var(--inner-highlight);
  }
  .piece {
    width: 10px; height: 10px; border-radius: 3px; background: var(--skeleton);
    transition: background $dur-fast $ease-out, transform $dur-fast $ease-out;
    &.have { background: var(--state-good); box-shadow: 0 0 4px color-mix(in srgb, var(--state-good) 50%, transparent); }
  }
</style>
