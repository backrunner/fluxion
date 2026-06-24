<script lang="ts">
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
  import { pickDirectory, pickTorrentFile, resolveMagnetPreview } from '../api';

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
  $: detectedKind = detectKind(source);
  $: isBt = detectedKind === 'Bt';
  $: isMagnet = isBt && source.trim().toLowerCase().startsWith('magnet:');
  $: isTorrentFile = isBt && !isMagnet && torrentPath.length > 0;
  // The submission dispatch kind (Http/Bt/Ftp/Sftp).
  let submitKind: DownloadKind | null = detectedKind;
  $: submitKind = detectedKind;

  // Track whether the user manually edited the filename so we don't clobber
  // their choice when the source changes.
  let fileNameTouched = false;
  function onFileNameInput() {
    fileNameTouched = true;
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
        return Boolean(magnetPreview) || createAnyway;
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
    if (e.key === 'Escape' && !busy) close();
  }

  async function browseDirectory() {
    const picked = await pickDirectory();
    if (picked) saveDir = picked;
  }

  async function browseTorrent() {
    const picked = await pickTorrentFile();
    if (picked) {
      torrentPath = picked;
      // A torrent file IS the source; clear any pasted URL/magnet so the
      // submission builder uses the file path.
      source = '';
    }
  }

  function parseHeaders(): HeaderPair[] {
    const result: HeaderPair[] = [];
    const seen = new Set<string>();
    for (const line of headers.split('\n')) {
      const trimmed = line.trim();
      if (!trimmed) continue;
      const idx = trimmed.indexOf(':');
      if (idx <= 0) continue;
      const name = trimmed.slice(0, idx).trim();
      const value = trimmed.slice(idx + 1).trim();
      if (name && !seen.has(name.toLowerCase())) {
        seen.add(name.toLowerCase());
        result.push({ name, value });
      }
    }
    if (cookie) result.push({ name: 'Cookie', value: cookie });
    if (referer) result.push({ name: 'Referer', value: referer });
    if (userAgent) result.push({ name: 'User-Agent', value: userAgent });
    return result;
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

  function credentials(extra: { username?: string | null; password?: string | null; privateKeyPassphrase?: string | null } = {}) {
    return {
      headers: [] as HeaderPair[],
      username: extra.username ?? null,
      password: extra.password ?? null,
      private_key_passphrase: extra.privateKeyPassphrase ?? null,
      extra: {} as Record<string, string>
    };
  }

  function buildHttpInput(): CreateTaskInput {
    return {
      kind: {
        Http: {
          url: source.trim(), method: 'Get', headers: parseHeaders(),
          max_connections: Number(maxConnections) || 16, min_split_size: null, redirect_limit: 10
        }
      },
      save_dir: saveDir.trim(), file_name: fileName.trim() || null,
      limits: limits(), proxy: useSystemProxy ? 'UseGlobal' : 'Direct', credentials: credentials()
    };
  }

  function buildBtInput(): CreateTaskInput {
    const src: BtSource = isMagnet
      ? { Magnet: source.trim() }
      : { TorrentFile: torrentPath.trim() };
    const config: BtTaskConfig = {
      source: src,
      selected_files: isMagnet ? selectedFileIndices : [],
      trackers: trackerList(btTrackers),
      max_connections: btMaxConnections ? Number(btMaxConnections) : null,
      share_ratio_limit: shareRatio ? Number(shareRatio) : null,
      enable_seeding: enableSeeding, anti_leech: null, ip_filter: null
    };
    return {
      kind: { Bt: config }, save_dir: saveDir.trim(), file_name: null,
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
    if (!parsed) throw new Error('Enter a valid ftp:// or sftp:// URL.');
    if (!parsed.hostname) throw new Error('FTP/SFTP URL must include a host.');
    if (!parsed.pathname || parsed.pathname === '/') throw new Error('FTP/SFTP URL must point to a file path.');
    const scheme = parsed.protocol.replace(':', '').toLowerCase();
    if (scheme === 'ftps') throw new Error('FTPS is not supported yet. Use ftp:// or sftp://.');
    if (scheme !== 'ftp' && scheme !== 'sftp') throw new Error('Use ftp:// or sftp:// for this task.');
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
  $: if (open && !lastOpen) reset();
  lastOpen = open;
</script>

{#if open}
  <div class="overlay" on:keydown={handleKey} role="presentation">
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="nt-title" tabindex="-1" on:click|stopPropagation on:keydown|stopPropagation>
      <div class="dialog-head">
        <h3 id="nt-title">New download task</h3>
        <button class="icon-btn" on:click={close} disabled={busy} aria-label="Close"><X size={16} /></button>
      </div>

      <form class="form" on:submit|preventDefault={submit}>
        <!-- 1. Source input — one box for URL or magnet. -->
        <label class="field source-field">
          <span class="lbl">Link or magnet</span>
          <div class="source-row">
            <input
              class="fx-input source-input"
              value={source}
              on:input={(e) => { source = e.currentTarget.value; }}
              placeholder="Paste a https://, ftp://, sftp:// link or magnet:?xt=urn:btih:…"
              autocomplete="off"
              spellcheck="false"
              disabled={isTorrentFile}
            />
            {#if detectedKind}
              <span class="kind-badge {detectedKind.toLowerCase()}" title={kindLabel[detectedKind]}>
                <svelte:component this={kindIcon[detectedKind]} size={13} />
                {kindLabel[detectedKind]}
              </span>
            {/if}
          </div>
          {#if isTorrentFile}
            <span class="source-hint">Source set from .torrent file below.</span>
          {/if}
        </label>

        <!-- 2. .torrent file picker (BT-only, when not a magnet). -->
        {#if isBt && !isMagnet}
          <div class="torrent-row">
            {#if torrentPath}
              <span class="torrent-chip" title={torrentPath}>
                <FileCheck2 size={13} />
                <span class="torrent-chip-name">{torrentPath.split('/').pop() ?? torrentPath}</span>
                <button type="button" class="torrent-chip-x" on:click={() => { torrentPath = ''; }} aria-label="Remove torrent file" disabled={busy}><X size={12} /></button>
              </span>
            {:else}
              <button type="button" class="torrent-pick" on:click={browseTorrent} disabled={busy}>
                <FolderOpen size={14} />
                Choose .torrent file
              </button>
            {/if}
          </div>
        {/if}

        <!-- 3. Save directory (native picker). -->
        <label class="field">
          <span class="lbl">Save to</span>
          <div class="dir-row">
            <input
              class="fx-input dir-input"
              value={saveDir}
              on:input={(e) => { saveDir = e.currentTarget.value; }}
              placeholder="Choose a folder…"
              autocomplete="off"
              spellcheck="false"
            />
            <button type="button" class="browse-btn" on:click={browseDirectory} disabled={busy} aria-label="Browse for folder">
              <FolderOpen size={15} />
              <span>Browse</span>
            </button>
          </div>
        </label>

        <!-- 4. File name (auto-filled hint, editable). -->
        <label class="field">
          <span class="lbl">File name <em>optional</em></span>
          <input
            class="fx-input"
            value={fileName}
            on:input={(e) => { fileName = e.currentTarget.value; onFileNameInput(); }}
            placeholder={isBt ? 'resolved from torrent' : 'auto-detected'}
            autocomplete="off"
            spellcheck="false"
          />
        </label>

        <!-- 5. Magnet file preview (magnet only, after resolve). -->
        {#if isMagnet}
          <div class="magnet-preview">
            <div class="preview-head">
              <span class="preview-title">Files</span>
              {#if resolving}
                <span class="preview-status"><Loader2 size={13} class="spin" /> Resolving…</span>
              {:else if resolveError}
                <span class="preview-status error">{resolveError}</span>
              {:else if magnetPreview}
                <span class="preview-status">{magnetPreview.files.length} files · {formatBytes(magnetPreview.total_bytes)}</span>
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
                Couldn't resolve this magnet. You can still create the task and pick files later in its detail view.
                <button type="button" class="anyway-btn" on:click={() => (createAnyway = true)} disabled={busy}>Create anyway</button>
              </p>
            {:else if !resolving}
              <p class="preview-fallback muted">Paste a magnet link above to preview its files.</p>
            {/if}
          </div>
        {/if}

        <!-- 6. Advanced settings (collapsed by default). -->
        <details class="advanced" class:open={advancedOpen} bind:open={advancedOpen}>
          <summary class="advanced-summary">
            <Settings2 size={14} />
            <span>Advanced settings</span>
            <ChevronDown size={14} class="advanced-caret" />
          </summary>
          <div class="advanced-body">
            {#if submitKind === 'Http'}
              <HttpFields bind:headers bind:cookie bind:referer bind:userAgent />
              <Field label="Max connections">
                <input class="fx-input" bind:value={maxConnections} inputmode="numeric" placeholder="16" autocomplete="off" />
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
            {/if}
          </div>
        </details>

        {#if localError || error}
          <div class="error-banner" role="alert">{localError || error}</div>
        {/if}

        <div class="form-actions">
          <button type="button" on:click={close} disabled={busy}>Cancel</button>
          <button type="submit" class="primary" disabled={busy || !ready}>
            {busy ? 'Creating…' : 'Create task'}
          </button>
        </div>
      </form>
    </div>
  </div>
{/if}

<style lang="scss">
  @use '../../styles/tokens' as *;

  .overlay {
    position: fixed; inset: 0; background: var(--overlay);
    backdrop-filter: blur(6px); -webkit-backdrop-filter: blur(6px);
    display: grid; place-items: center; padding: $space-6; z-index: 50;
    animation: fade $dur-base $ease-out;
  }
  @keyframes fade { from { opacity: 0; } to { opacity: 1; } }

  .dialog {
    width: min(620px, 100%); max-height: 90vh; overflow-y: auto;
    display: flex; flex-direction: column; gap: $space-4; padding: $space-6;
    border-radius: $radius-xl; background: var(--elevated);
    border: 1px solid var(--border-strong); box-shadow: var(--shadow-lg), var(--inner-highlight);
    animation: pop $dur-base $ease-out; @include scrollbar;
  }
  @keyframes pop {
    from { opacity: 0; transform: translateY(12px) scale(0.98); }
    to { opacity: 1; transform: translateY(0) scale(1); }
  }

  .dialog-head { display: flex; align-items: center; justify-content: space-between; }
  .dialog-head h3 { font-size: $fs-lg; }

  .icon-btn {
    display: grid; place-items: center; width: 30px; height: 30px;
    border-radius: $radius-sm; border: 1px solid transparent; background: transparent;
    color: var(--text-muted); cursor: pointer;
    transition: background $dur-fast $ease-out, color $dur-fast $ease-out;
    @include focus-ring;
    &:not(:disabled):hover { background: var(--surface-3); color: var(--text-strong); }
  }

  .form { display: flex; flex-direction: column; gap: $space-3; }

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
    display: inline-flex; align-items: center; gap: 6px; padding: 7px $space-3;
    border-radius: $radius-md; border: 1px dashed var(--border-strong);
    background: var(--surface-3); color: var(--text-muted); cursor: pointer;
    font-size: $fs-sm; transition: border-color $dur-fast $ease-out, color $dur-fast $ease-out;
    @include focus-ring;
    &:not(:disabled):hover { color: var(--text-strong); border-color: var(--accent); }
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
    padding: 0 $space-3; border-radius: $radius-md; border: 1px solid var(--border);
    background: var(--surface); color: var(--text); cursor: pointer; font-size: $fs-sm;
    transition: border-color $dur-fast $ease-out, background $dur-fast $ease-out;
    @include focus-ring;
    &:not(:disabled):hover { border-color: var(--border-strong); background: var(--surface-3); }
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
    border: 1px solid var(--border); border-radius: $radius-md;
    background: var(--surface-2);
  }
  :global(.advanced.open .advanced-caret) { transform: rotate(180deg); }
  .advanced-summary {
    display: flex; align-items: center; gap: $space-2;
    padding: $space-3 $space-4; cursor: pointer; list-style: none;
    font-size: $fs-sm; font-weight: $fw-medium; color: var(--text);
    transition: background $dur-fast $ease-out;
    &:hover { background: var(--surface-3); }
    &::-webkit-details-marker { display: none; }
  }
  .advanced-caret { margin-left: auto; color: var(--text-muted); transition: transform $dur-base $ease-out; }
  .advanced-body {
    display: flex; flex-direction: column; gap: $space-3;
    padding: 0 $space-4 $space-4; border-top: 1px solid var(--border);
  }

  .error-banner {
    padding: $space-3; border-radius: $radius-md; border: 1px solid var(--state-bad);
    background: var(--state-bad-bg); color: var(--state-bad);
    font-size: $fs-sm; word-break: break-word;
  }

  .form-actions { display: flex; justify-content: flex-end; gap: $space-2; margin-top: $space-2; }
  .form-actions button {
    padding: $space-2 $space-4; border-radius: $radius-md; border: 1px solid var(--border);
    background: var(--surface); color: var(--text); cursor: pointer; font-size: $fs-sm;
    transition: background $dur-fast $ease-out, border-color $dur-fast $ease-out, filter $dur-fast $ease-out;
    @include focus-ring;
    &:not(:disabled):hover { background: var(--surface-3); border-color: var(--border-strong); }
    &:disabled { opacity: 0.5; cursor: not-allowed; }
  }
  .primary {
    background: linear-gradient(145deg, var(--accent-hover), var(--accent-press)) !important;
    color: var(--accent-contrast) !important; border-color: transparent !important; font-weight: $fw-semibold;
    &:not(:disabled):hover { filter: brightness(1.06); box-shadow: var(--shadow-glow); }
  }

  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>