// URL/magnet auto-detection for the unified new-task form.
// Pure functions — no Tauri/Core dependencies, so they are trivially testable.

import type { DownloadKind } from './types';

/**
 * Infer the download kind from a pasted source string. Detection order matters:
 * magnet links are checked first (they don't have a parseable URL scheme), then
 * URL schemes. Returns null when the input is empty or unrecognized, so the
 * caller can keep the form in a neutral state instead of guessing.
 */
export function detectKind(source: string): DownloadKind | null {
  const value = source.trim().toLowerCase();
  if (!value) return null;
  if (value.startsWith('magnet:')) return 'Bt';
  // `new URL` throws on bare strings, so guard with a scheme check first.
  if (!/^[a-z][a-z0-9+.-]*:\/\//i.test(value)) return null;
  try {
    const scheme = new URL(value).protocol.replace(':', '').toLowerCase();
    if (scheme === 'http' || scheme === 'https') return 'Http';
    if (scheme === 'ftp' || scheme === 'ftps') return 'Ftp';
    if (scheme === 'sftp') return 'Sftp';
    return null;
  } catch {
    return null;
  }
}

/**
 * Infer a display filename from the last path segment of an HTTP/FTP/SFTP URL.
 * Decodes percent-encoding and strips query strings. Returns null when no
 * filename can be guessed (e.g. URL ends at a directory, or it's a magnet —
 * magnets get their name from the resolved torrent metadata instead).
 */
export function inferFileNameFromUrl(source: string): string | null {
  const value = source.trim();
  if (!value || value.toLowerCase().startsWith('magnet:')) return null;
  if (!/^[a-z][a-z0-9+.-]*:\/\//i.test(value)) return null;
  try {
    const parsed = new URL(value);
    const segments = parsed.pathname.split('/').filter(Boolean);
    const last = segments[segments.length - 1];
    if (!last) return null;
    // Strip any trailing query/hash that slipped through pathname parsing.
    const cleaned = last.split(/[?#]/)[0];
    if (!cleaned) return null;
    try {
      return decodeURIComponent(cleaned);
    } catch {
      return cleaned;
    }
  } catch {
    return null;
  }
}
