<script lang="ts">
  import { tick } from 'svelte';
  import { get } from 'svelte/store';
  import {
    Globe,
    Magnet,
    Folder,
    FileText,
    Link2,
    X,
    FolderOpen,
    ChevronDown,
    Loader2,
    FileCheck2,
    Settings2
  } from '@lucide/svelte';
  import type {
    CreateTaskInput,
    BtTaskConfig,
    BtSource,
    HeaderPair,
    DownloadKind,
    MagnetPreview
  } from '../types';

  import HttpFields from './newtask/HttpFields.svelte';
  import BtFields from './newtask/BtFields.svelte';
  import FtpFields from './newtask/FtpFields.svelte';
  import CommonFields from './newtask/CommonFields.svelte';
  import Field from './common/Field.svelte';
  import { detectKind, inferFileNameFromUrl } from '../detect';
  import { formatBytes } from '../format';
  import { splitHttpHeaders } from '../httpHeaders';
  import { settingsStore } from '../stores/settings';
  import { pickDirectory, pickTorrentFile, resolveMagnetPreview } from '../api';
  import { t } from '../i18n';

  export let open: boolean;
  export let busy: boolean;
  export let error = '';
  export let onSubmit: (input: CreateTaskInput) => void = () => {};
  export let onClose: () => void = () => {};

  let localError = '';

  // Unified source input — a single box for URL or magnet.
  let source = '';
  // Native-picked save directory and optional .torrent file path.
  let saveDir = '';
  let torrentPath = '';
  // Auto-filled, user-editable filename hint.
  let fileName = '';

  // Common advanced fields.
  let downloadLimit = '';
  let uploadLimit = '';
  let useSystemProxy = true;
  let maxConnections = '16';

  // HTTP advanced fields.
  let headers = '';
  let cookie = '';
  let referer = '';
  let userAgent = '';

  // BT advanced fields.
  let btTrackers = '';
  let btMaxConnections = '';
  let shareRatio = '';
  let enableSeeding = true;

  // FTP / SFTP advanced fields.
  let username = '';
  let password = '';
  let privateKeyPath = '';
  let privateKeyPassphrase = '';

  // Magnet pre-resolve state.
  let magnetPreview: MagnetPreview | null = null;
  let resolving = false;
  let resolveError = '';
  let createAnyway = false;
  // Cancel guard so a stale resolve doesn't overwrite a newer one.
  let resolveToken = 0;

  let advancedOpen = false;
  let dialogElement: HTMLDivElement;
  let sourceInputElement: HTMLInputElement;

  const kindIcon: Record<DownloadKind, typeof Globe> = {
    Http: Link2,
    Bt: Magnet,
    Ftp: Folder,
    Sftp: FileText
  };
  const kindLabel: Record<DownloadKind, string> = {
    Http: 'HTTP',
    Bt: 'BitTorrent',
    Ftp: 'FTP',
    Sftp: 'SFTP'
  };

  // Mirror the unified source into the kind-specific variables the existing
  // builders read (url / ftpUrl / magnet), and auto-fill the filename hint.
  $: detectedKind = torrentPath ? 'Bt' : detectKind(source);
  $: isBt = detectedKind === 'Bt';
  $: isMagnet = isBt && source.trim().toLowerCase().startsWith('magnet:');
  $: isTorrentFile = torrentPath.length > 0;
  // The submission dispatch kind (Http/Bt/Ftp/Sftp).
  let submitKind: DownloadKind | null = detectedKind;
  $: submitKind = detectedKind;

  // Track whether the user manually edited the filename so we don't clobber
  // their choice when the source changes.
  let fileNameTouched = false;
  function onFileNameInput() {
    fileNameTouched = true;
  }

  function updateSource(value: string) {
    if (value === source) return;
    source = value;
    resolveToken++;
    resolving = false;
    magnetPreview = null;
    resolveError = '';
    createAnyway = false;
    if (resolveTimer) {
      clearTimeout(resolveTimer);
      resolveTimer = null;
    }
  }

  // Auto-fill filename from URL for HTTP/FTP/SFTP. For BT-from-magnet the
  // name comes from the resolved torrent (handled in the resolve effect).
  $: if (!isBt) {
    const hint = inferFileNameFromUrl(source);
    if (hint && !fileNameTouched) fileName = hint;
  }

  // Reset the magnet preview + filename-touch state whenever the source kind
  // changes or the source is cleared, so stale previews never survive.
  $: if (!isMagnet) {
    if (magnetPreview) magnetPreview = null;
    if (resolveError) resolveError = '';
    if (createAnyway) createAnyway = false;
  }

  // Trigger magnet pre-resolve (debounced) when a magnet is entered. Cancel
  // any in-flight resolve on change so results never arrive out of order.
  let resolveTimer: ReturnType<typeof setTimeout> | null = null;
  $: if (open && isMagnet && source.trim()) {
    if (resolveTimer) clearTimeout(resolveTimer);
    resolveTimer = setTimeout(() => runResolve(source.trim()), 500);
  }
  $: if (!isMagnet && resolveTimer) {
    clearTimeout(resolveTimer);
    resolveTimer = null;
  }

  async function runResolve(magnet: string) {
    const token = ++resolveToken;
    resolving = true;
    resolveError = '';
    magnetPreview = null;
    createAnyway = false;
    try {
      const preview = await resolveMagnetPreview(magnet);
      if (token !== resolveToken) return; // a newer resolve superseded this one
      magnetPreview = preview;
      if (preview.name && !fileNameTouched) fileName = preview.name;
    } catch (err) {
      if (token !== resolveToken) return;
      resolveError = err instanceof Error ? err.message : String(err);
    } finally {
      if (token === resolveToken) resolving = false;
    }
  }

  // Selected file indices from the preview, fed into buildBtInput.
  $: selectedFileIndices = magnetPreview
    ? magnetPreview.files.filter((f) => f.selected).map((f) => f.index)
    : [];

  function toggleFile(idx: number) {
    if (!magnetPreview) return;
    magnetPreview = {
      ...magnetPreview,
      files: magnetPreview.files.map((f) =>
        f.index === idx ? { ...f, selected: !f.selected } : f
      )
    };
  }

  $: ready = (() => {
    if (!submitKind || !saveDir.trim()) return false;
    if (submitKind === 'Bt') {
      if (isMagnet) {
        // Need either a successful resolve or an explicit "create anyway".
        return (Boolean(magnetPreview)
          && magnetPreview!.files.some((file) => file.selected)) || createAnyway;
      }
      return torrentPath.trim().length > 0;
    }
    return source.trim().length > 0;
  })();

  function reset() {
    source = ''; saveDir = ''; torrentPath = ''; fileName = '';
    downloadLimit = ''; uploadLimit = '';
    useSystemProxy = true; maxConnections = '16';
    headers = ''; cookie = ''; referer = ''; userAgent = '';
    btTrackers = ''; btMaxConnections = ''; shareRatio = ''; enableSeeding = true;
    username = ''; password = ''; privateKeyPath = ''; privateKeyPassphrase = '';
    magnetPreview = null; resolving = false; resolveError = ''; createAnyway = false;
    fileNameTouched = false; advancedOpen = false;
    localError = '';
    if (resolveTimer) { clearTimeout(resolveTimer); resolveTimer = null; }
    resolveToken++;
  }

  function close() {
    if (busy) return;
    onClose();
  }

  function handleKey(e: KeyboardEvent) {
    if (open && e.key === 'Escape' && !busy) close();
  }

  async function browseDirectory() {
    const picked = await pickDirectory();
    if (picked) saveDir = picked;
  }

  async function browseTorrent() {
    const picked = await pickTorrentFile();
    if (picked) {
      torrentPath = picked;
      if (!fileNameTouched) {
        fileName = picked.split(/[\\/]/).pop()?.replace(/\.torrent$/i, '') || '';
      }
      // A torrent file IS the source; clear any pasted URL/magnet so the
      // submission builder uses the file path.
      updateSource('');
    }
  }

  async function focusDialog() {
    await tick();
    sourceInputElement?.focus();
  }

  function handleDialogKey(e: KeyboardEvent) {
    if (e.key !== 'Tab' || !dialogElement) return;
    const focusable = Array.from(
      dialogElement.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])'
      )
    ).filter((element) => element.offsetParent !== null);
    if (focusable.length === 0) {
      e.preventDefault();
      dialogElement.focus();
      return;
    }

    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    const active = document.activeElement;
    if (e.shiftKey && (active === first || !dialogElement.contains(active))) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && active === last) {
      e.preventDefault();
      first.focus();
    }
  }

  function trackerList(text: string): string[] {
    return text.split('\n').map((l) => l.trim()).filter(Boolean);
  }

  function limits() {
    return {
      download_bytes_per_second: downloadLimit ? Number(downloadLimit) : null,
      upload_bytes_per_second: uploadLimit ? Number(uploadLimit) : null
    };
  }

  function credentials(extra: {
    headers?: HeaderPair[];
    username?: string | null;
    password?: string | null;
    privateKeyPassphrase?: string | null;
  } = {}) {
    return {
      headers: extra.headers ?? [],
      username: extra.username ?? null,
      password: extra.password ?? null,
      private_key_passphrase: extra.privateKeyPassphrase ?? null,
      extra: {} as Record<string, string>
    };
  }

  function buildHttpInput(): CreateTaskInput {
    const parsedHeaders = splitHttpHeaders({
      customHeaders: headers,
      cookie,
      referer,
      userAgent
    });
    return {
      kind: {
        Http: {
          url: source.trim(), method: 'Get', headers: parsedHeaders.headers,
          max_connections: Number(maxConnections) || 16, min_split_size: null, redirect_limit: 10
        }
      },
      save_dir: saveDir.trim(), file_name: fileName.trim() || null,
      limits: limits(), proxy: useSystemProxy ? 'UseGlobal' : 'Direct',
      credentials: credentials({ headers: parsedHeaders.credentialHeaders })
    };
  }

  function buildBtInput(): CreateTaskInput {
    const src: BtSource = isMagnet
      ? { Magnet: source.trim() }
      : { TorrentFile: torrentPath.trim() };
    const globalTrackers = get(settingsStore)?.bt_trackers ?? [];
    const config: BtTaskConfig = {
      source: src,
      selected_files: isMagnet ? selectedFileIndices : [],
      trackers: Array.from(new Set([...globalTrackers, ...trackerList(btTrackers)])),
      max_connections: btMaxConnections ? Number(btMaxConnections) : null,
      share_ratio_limit: shareRatio ? Number(shareRatio) : null,
      enable_seeding: enableSeeding,
      anti_leech: {
        blocked_client_names: [],
        blocked_peer_id_prefixes: [],
        block_suspicious_fast_disconnects: false
      },
      ip_filter: { allow: [], deny: [] }
    };
    return {
      kind: { Bt: config }, save_dir: saveDir.trim(), file_name: fileName.trim() || null,
      limits: limits(), proxy: useSystemProxy ? 'UseGlobal' : 'Direct', credentials: credentials()
    };
  }

  function parseFtpUrl(value: string): URL | null {
    try {
      return new URL(value.trim());
    } catch {
      return null;
    }
  }

  function sanitizeTransferUrl(parsed: URL) {
    const next = new URL(parsed.toString());
    next.username = '';
    next.password = '';
    return next.toString();
  }

  function inferCredentialsFromUrl(parsed: URL) {
    const decodedUsername = parsed.username ? decodeURIComponent(parsed.username) : '';
    const decodedPassword = parsed.password ? decodeURIComponent(parsed.password) : '';
    return {
      username: username.trim() || decodedUsername || null,
      password: password || decodedPassword || null
    };
  }

  function buildFtpInput(): CreateTaskInput {
    const parsed = parseFtpUrl(source);
    if (!parsed) throw new Error($t('new.validation.ftpUrl'));
    if (!parsed.hostname) throw new Error($t('new.validation.ftpHost'));
    if (!parsed.pathname || parsed.pathname === '/') throw new Error($t('new.validation.ftpPath'));
    const scheme = parsed.protocol.replace(':', '').toLowerCase();
    if (scheme === 'ftps') throw new Error($t('new.validation.ftps'));
    if (scheme !== 'ftp' && scheme !== 'sftp') throw new Error($t('new.validation.ftpScheme'));
    const auth = inferCredentialsFromUrl(parsed);
    const cleanUrl = sanitizeTransferUrl(parsed);
    const transferCredentials = credentials({
      username: auth.username,
      password: auth.password,
      privateKeyPassphrase: scheme === 'sftp' ? privateKeyPassphrase || null : null
    });
    if (scheme === 'ftp') {
      return {
        kind: {
          Ftp: {
            url: cleanUrl,
            username: auth.username,
            passive: true,
            ftps: false
          }
        },
        save_dir: saveDir.trim(), file_name: fileName.trim() || null,
        limits: limits(), proxy: useSystemProxy ? 'UseGlobal' : 'Direct',
        credentials: transferCredentials
      };
    }
    return {
      kind: {
        Sftp: {
          url: cleanUrl,
          username: auth.username,
          private_key_path: privateKeyPath.trim() || null
        }
      },
      save_dir: saveDir.trim(), file_name: fileName.trim() || null,
      limits: limits(), proxy: useSystemProxy ? 'UseGlobal' : 'Direct',
      credentials: transferCredentials
    };
  }

  function submit() {
    if (busy) return;
    localError = '';
    try {
      const input =
        submitKind === 'Http' ? buildHttpInput()
        : submitKind === 'Bt' ? buildBtInput()
        : buildFtpInput();
      onSubmit(input);
    } catch (err) {
      localError = err instanceof Error ? err.message : String(err);
    }
  }

  let lastOpen = false;
  $: if (open && !lastOpen) {
    reset();
    void focusDialog();
  }
  lastOpen = open;
</script>

<!-- Window-level Escape: works even when nothing inside the dialog holds
     focus (an overlay keydown only fires for events bubbling within it). -->
<svelte:window on:keydown|capture={handleKey} />

{#if open}
  <div class="overlay" role="presentation">
    <div bind:this={dialogElement} class="dialog" role="dialog" aria-modal="true" aria-labelledby="nt-title" tabindex="-1" on:click|stopPropagation on:keydown|stopPropagation={handleDialogKey}>
      <div class="dialog-head">
        <h3 id="nt-title">{$t('new.title')}</h3>
        <button class="icon-btn" on:click={close} disabled={busy} aria-label={$t('common.close')}><X size={16} /></button>
      </div>

      <form class="form" on:submit|preventDefault={submit}>
        <!-- 1. Source input — one box for URL or magnet. -->
        <label class="field source-field">
          <span class="lbl">{$t('new.link')}</span>
          <div class="source-row">
            <input
              bind:this={sourceInputElement}
              class="fx-input source-input"
              value={source}
              on:input={(e) => updateSource(e.currentTarget.value)}
              placeholder={$t('new.linkPlaceholder')}
              autocomplete="off"
              spellcheck="false"
              disabled={isTorrentFile}
            />
            <button
              type="button"
              class="torrent-pick"
              on:click={browseTorrent}
              disabled={busy}
              title={$t('new.chooseTorrent')}
              aria-label={$t('new.chooseTorrent')}
            >
              <FolderOpen size={14} />
              <span>.torrent</span>
            </button>
            {#if detectedKind}
              <span class="kind-badge {detectedKind.toLowerCase()}" title={kindLabel[detectedKind]}>
                <svelte:component this={kindIcon[detectedKind]} size={13} />
                {kindLabel[detectedKind]}
              </span>
            {/if}
          </div>
          {#if isTorrentFile}
            <span class="source-hint">{$t('new.sourceFromTorrent')}</span>
          {/if}
        </label>

        <!-- 2. Selected .torrent file. The picker itself stays beside the
             unified source field so the file workflow is always reachable. -->
        {#if isTorrentFile}
          <div class="torrent-row">
            <span class="torrent-chip" title={torrentPath}>
              <FileCheck2 size={13} />
              <span class="torrent-chip-name">{torrentPath.split('/').pop() ?? torrentPath}</span>
              <button type="button" class="torrent-chip-x" on:click={() => { torrentPath = ''; }} aria-label={$t('new.removeTorrent')} disabled={busy}><X size={12} /></button>
            </span>
          </div>
        {/if}

        <!-- 3. Save directory (native picker). -->
        <label class="field">
          <span class="lbl">{$t('new.saveTo')}</span>
          <div class="dir-row">
            <input
              class="fx-input dir-input"
              value={saveDir}
              on:input={(e) => { saveDir = e.currentTarget.value; }}
              placeholder={$t('new.chooseFolder')}
              autocomplete="off"
              spellcheck="false"
            />
            <button type="button" class="browse-btn" on:click={browseDirectory} disabled={busy} aria-label={$t('new.browseFolder')}>
              <FolderOpen size={15} />
              <span>{$t('new.browse')}</span>
            </button>
          </div>
        </label>

        <!-- 4. File name (auto-filled hint, editable). -->
        <label class="field">
          <span class="lbl">{$t('new.fileName')} <em>{$t('common.optional')}</em></span>
          <input
            class="fx-input"
            value={fileName}
            on:input={(e) => { fileName = e.currentTarget.value; onFileNameInput(); }}
            placeholder={isBt ? $t('new.resolvedTorrent') : $t('new.autoDetected')}
            autocomplete="off"
            spellcheck="false"
          />
        </label>

        <!-- 5. Magnet file preview (magnet only, after resolve). -->
        {#if isMagnet}
          <div class="magnet-preview">
            <div class="preview-head">
              <span class="preview-title">{$t('new.files')}</span>
              {#if resolving}
                <span class="preview-status"><Loader2 size={13} class="spin" /> {$t('new.resolving')}</span>
              {:else if resolveError}
                <span class="preview-status error">{resolveError}</span>
              {:else if magnetPreview}
                <span class="preview-status">{$t('new.fileCount', { count: magnetPreview.files.length })} · {formatBytes(magnetPreview.total_bytes)}</span>
              {/if}
            </div>
            {#if magnetPreview}
              <div class="file-list">
                {#each magnetPreview.files as file (file.index)}
                  <label class="file-row" class:off={!file.selected}>
                    <input type="checkbox" checked={file.selected} on:change={() => toggleFile(file.index)} />
                    <span class="file-name" title={file.name}>{file.name}</span>
                    <span class="file-size">{formatBytes(file.size)}</span>
                  </label>
                {/each}
              </div>
            {:else if resolveError}
              <p class="preview-fallback">
                {$t('new.resolveFailed')}
                <button type="button" class="anyway-btn" on:click={() => (createAnyway = true)} disabled={busy}>{$t('new.createAnyway')}</button>
              </p>
            {:else if !resolving}
              <p class="preview-fallback muted">{$t('new.previewMagnet')}</p>
            {/if}
          </div>
        {/if}

        <!-- 6. Advanced settings (collapsed by default). Plain button toggle:
             <details bind:open> is unreliable inside WKWebView. -->
        <div class="advanced" class:open={advancedOpen}>
          <button
            type="button"
            class="advanced-summary"
            aria-expanded={advancedOpen}
            on:click={() => (advancedOpen = !advancedOpen)}
          >
            <Settings2 size={14} />
            <span>{$t('new.advanced')}</span>
            <ChevronDown size={14} class="advanced-caret" />
          </button>
          {#if advancedOpen}
            <div class="advanced-body">
              {#if submitKind === 'Http'}
                <HttpFields bind:headers bind:cookie bind:referer bind:userAgent />
                <Field label={$t('new.maxConnections')}>
                  <input class="fx-input" bind:value={maxConnections} type="number" min="1" max="65535" step="1" placeholder="16" autocomplete="off" />
                </Field>
                <CommonFields bind:downloadLimit bind:uploadLimit bind:useSystemProxy />
              {:else if submitKind === 'Bt'}
                <BtFields bind:trackers={btTrackers} bind:maxConnections={btMaxConnections} bind:shareRatio bind:enableSeeding />
                <CommonFields bind:downloadLimit bind:uploadLimit bind:useSystemProxy />
              {:else if submitKind === 'Ftp' || submitKind === 'Sftp'}
                <FtpFields
                  bind:username
                  bind:password
                  bind:privateKeyPath
                  bind:privateKeyPassphrase
                  showSftpOptions={submitKind === 'Sftp'}
                />
                <CommonFields bind:downloadLimit bind:uploadLimit bind:useSystemProxy />
              {:else}
                <p class="advanced-neutral">{$t('new.protocolOptions')}</p>
              {/if}
            </div>
          {/if}
        </div>

        {#if localError || error}
          <div class="error-banner" role="alert">{localError || error}</div>
        {/if}

        <div class="form-actions">
          <button type="button" on:click={close} disabled={busy}>{$t('common.cancel')}</button>
          <button type="submit" class="primary" disabled={busy || !ready}>
            {busy ? $t('new.creating') : $t('new.create')}
          </button>
        </div>
      </form>
    </div>
  </div>
{/if}

<style lang="scss">
  @use '../../styles/tokens' as *;

  .overlay {
    position: fixed; inset: 0;
    background: color-mix(in srgb, var(--scrim) 82%, transparent);
    backdrop-filter: blur(16px) saturate(125%);
    -webkit-backdrop-filter: blur(16px) saturate(125%);
    display: grid; place-items: center; padding: $space-6; z-index: 50;
    animation: fade $dur-base $ease-glass;
  }
  @keyframes fade { from { opacity: 0; } to { opacity: 1; } }

  .dialog {
    width: min(620px, 100%); max-height: 90vh; overflow-y: auto;
    display: flex; flex-direction: column; gap: $space-4; padding: $space-5;
    border-radius: 16px;
    background:
      linear-gradient(118deg, rgba(255, 255, 255, 0.09), transparent 32%, rgba(255, 255, 255, 0.025) 72%, transparent),
      color-mix(in srgb, var(--elevated) 92%, transparent);
    border: 1px solid transparent;
    -webkit-backdrop-filter: blur(28px) saturate(150%);
    backdrop-filter: blur(28px) saturate(150%);
    box-shadow:
      var(--glass-rim),
      inset 0 0 0 1px color-mix(in srgb, var(--border-strong) 60%, transparent),
      0 30px 90px color-mix(in srgb, var(--scrim) 82%, transparent);
    animation: pop 240ms $ease-glass; @include scrollbar;
  }
  @keyframes pop {
    from { opacity: 0; transform: translateY(8px) scale(0.99); }
    to { opacity: 1; transform: translateY(0) scale(1); }
  }

  .dialog-head { display: flex; align-items: center; justify-content: space-between; }
  .dialog-head h3 { font-size: $fs-lg; letter-spacing: 0; }

  .icon-btn {
    display: grid; place-items: center; width: 30px; height: 30px;
    border-radius: $radius-md; @include glass-hover-control;
    color: var(--text-muted); cursor: pointer;
    transition: background $dur-fast $ease-out, color $dur-fast $ease-out;
    @include focus-ring;
    &:not(:disabled):hover { color: var(--text-strong); }
  }

  .form { display: flex; flex-direction: column; gap: 14px; }

  .dialog :global(.fx-input),
  .dialog :global(.fx-textarea) {
    min-height: 40px;
    border-radius: $radius-lg;
    border-color: transparent;
    background:
      linear-gradient(180deg, rgba(255, 255, 255, 0.035), transparent 42%),
      color-mix(in srgb, var(--surface-3) 84%, transparent);
    -webkit-backdrop-filter: blur(12px) saturate(130%);
    backdrop-filter: blur(12px) saturate(130%);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.07),
      inset 0 -1px 0 rgba(0, 0, 0, 0.14),
      0 0 0 1px color-mix(in srgb, var(--border) 68%, transparent);
    transition: background $dur-base $ease-glass, box-shadow $dur-base $ease-glass,
      color $dur-fast $ease-out;
  }

  .dialog :global(.fx-input:hover:not(:disabled):not(:focus)),
  .dialog :global(.fx-textarea:hover:not(:disabled):not(:focus)) {
    border-color: transparent;
    background:
      linear-gradient(180deg, rgba(255, 255, 255, 0.055), transparent 44%),
      color-mix(in srgb, var(--surface-3) 92%, transparent);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.09),
      inset 0 -1px 0 rgba(0, 0, 0, 0.14),
      0 0 0 1px var(--border-strong);
  }

  .dialog :global(.fx-input:focus),
  .dialog :global(.fx-textarea:focus) {
    border-color: transparent;
    background: color-mix(in srgb, var(--surface-3) 96%, transparent);
    box-shadow:
      0 0 0 2px var(--focus-ring),
      inset 0 1px 0 rgba(255, 255, 255, 0.09),
      inset 0 -1px 0 rgba(0, 0, 0, 0.14);
  }

  // Inline field labels (kept local so we don't depend on the Field component
  // for the source/save/filename rows that have composite controls).
  .field { display: flex; flex-direction: column; gap: 5px; }
  .lbl {
    display: inline-flex; align-items: center; gap: 5px;
    font-size: $fs-xs; color: var(--text-muted); font-weight: $fw-medium;
    em { font-style: normal; color: var(--text-faint); font-weight: $fw-regular; }
  }

  .source-row { display: flex; align-items: center; gap: $space-2; }
  .source-input { flex: 1; min-width: 0; }
  .source-field .source-hint { font-size: $fs-xs; color: var(--text-faint); }

  .kind-badge {
    display: inline-flex; align-items: center; gap: 4px; flex: none;
    padding: 4px $space-2; border-radius: $radius-pill;
    background: var(--accent-soft); color: var(--accent);
    font-size: $fs-xs; font-weight: $fw-semibold;
    border: 1px solid rgba(249, 115, 22, 0.24);
  }

  .torrent-row { display: flex; align-items: center; }
  .torrent-pick {
    display: inline-flex; align-items: center; justify-content: center; gap: 6px;
    height: 40px; padding: 0 $space-3; flex: none;
    border-radius: $radius-lg; @include glass-control;
    color: var(--text-muted); cursor: pointer;
    font-size: $fs-xs; font-weight: $fw-medium;
    @include focus-ring;
    &:not(:disabled):hover { color: var(--text-strong); }
    &:disabled { opacity: 0.5; cursor: not-allowed; }
  }
  .torrent-chip {
    display: inline-flex; align-items: center; gap: 6px; max-width: 100%;
    padding: 5px $space-2; border-radius: $radius-pill;
    background: var(--accent-soft); border: 1px solid rgba(249, 115, 22, 0.24);
    color: var(--text); font-size: $fs-xs;
  }
  .torrent-chip-name { @include hide-overflow; max-width: 320px; }
  .torrent-chip-x {
    display: inline-grid; place-items: center; width: 18px; height: 18px;
    border: none; background: transparent; color: var(--text-muted); cursor: pointer;
    border-radius: 50%; &:hover { background: var(--surface-3); color: var(--text-strong); }
  }

  .dir-row { display: flex; align-items: stretch; gap: $space-2; }
  .dir-input { flex: 1; min-width: 0; }
  .browse-btn {
    display: inline-flex; align-items: center; gap: 6px; flex: none;
    padding: 0 $space-3; border-radius: $radius-lg; @include glass-control;
    color: var(--text); cursor: pointer; font-size: $fs-sm; font-weight: $fw-medium;
    @include focus-ring;
  }

  // Magnet preview -----------------------------------------------------------
  .magnet-preview {
    display: flex; flex-direction: column; gap: $space-2;
    padding: $space-3; border-radius: $radius-md;
    background: var(--surface-3); border: 1px solid var(--border);
    box-shadow: var(--inner-highlight);
  }
  .preview-head { display: flex; align-items: center; gap: $space-2; }
  .preview-title { font-size: $fs-sm; font-weight: $fw-semibold; color: var(--text-strong); }
  .preview-status {
    margin-left: auto; display: inline-flex; align-items: center; gap: 5px;
    font-size: $fs-xs; color: var(--text-muted);
    &.error { color: var(--state-bad); }
  }
  .file-list {
    display: flex; flex-direction: column; gap: 1px;
    max-height: 220px; overflow-y: auto; @include scrollbar;
  }
  .file-row {
    display: flex; align-items: center; gap: $space-2; padding: 6px $space-2;
    border-radius: $radius-sm; cursor: pointer;
    &:hover { background: var(--surface); }
    &.off .file-name { color: var(--text-faint); text-decoration: line-through; }
    input { flex: none; accent-color: var(--accent); }
  }
  .file-name { flex: 1; min-width: 0; font-size: $fs-sm; @include hide-overflow; }
  .file-size { flex: none; font-size: $fs-xs; color: var(--text-muted); font-family: $font-mono; }
  .preview-fallback {
    font-size: $fs-sm; color: var(--text-muted); margin: 0;
    &.muted { color: var(--text-faint); }
  }
  .anyway-btn {
    margin-left: $space-2; padding: 3px $space-2; border-radius: $radius-sm;
    border: 1px solid var(--border-strong); background: var(--surface);
    color: var(--text); cursor: pointer; font-size: $fs-xs; font-weight: $fw-medium;
    @include focus-ring;
    &:not(:disabled):hover { border-color: var(--accent); color: var(--accent); }
  }

  // Advanced disclosure ------------------------------------------------------
  .advanced {
    border: none; border-radius: $radius-lg;
    background: transparent;
  }
  :global(.advanced.open .advanced-caret) { transform: rotate(180deg); }
  .advanced-summary {
    display: flex; align-items: center; gap: $space-2; width: 100%;
    min-height: 40px; padding: 0 $space-3; cursor: pointer;
    @include glass-control; text-align: left;
    font-size: $fs-sm; font-weight: $fw-medium; color: var(--text);
    border-radius: $radius-lg;
    @include focus-ring;
  }
  .advanced.open .advanced-summary {
    background: var(--glass-fill-hover);
  }
  :global(.advanced-caret) { margin-left: auto; color: var(--text-muted); transition: transform $dur-base $ease-out; }
  .advanced-body {
    display: flex; flex-direction: column; gap: $space-3;
    margin-top: $space-2;
    padding: $space-4;
    border-radius: $radius-lg;
    background: color-mix(in srgb, var(--surface-2) 72%, transparent);
    box-shadow: inset 0 1px 0 var(--border), var(--inner-highlight);
  }
  .advanced-neutral {
    font-size: $fs-sm; color: var(--text-faint); margin: 0;
  }

  .error-banner {
    padding: $space-3; border-radius: $radius-md; border: 1px solid var(--state-bad);
    background: var(--state-bad-bg); color: var(--state-bad);
    font-size: $fs-sm; word-break: break-word;
  }

  .form-actions { display: flex; justify-content: flex-end; gap: $space-2; margin-top: $space-1; }
  .form-actions button {
    @include ghost-button;
    min-height: 36px;
    padding: 0 $space-4;
    font-size: $fs-sm;
  }
  .primary {
    @include primary-button;
    min-width: 92px;
    padding: 0 $space-4 !important;
    font-size: $fs-sm;
  }

  @media (prefers-reduced-transparency: reduce) {
    .overlay { backdrop-filter: none; -webkit-backdrop-filter: none; }
    .dialog { background: var(--elevated); backdrop-filter: none; -webkit-backdrop-filter: none; }
  }

  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
