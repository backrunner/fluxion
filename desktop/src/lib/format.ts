// Display formatting helpers shared across components.
import type { TaskState } from './types';

export function formatBytes(bytes?: number | null, opts: { decimals?: number } = {}): string {
  if (bytes == null) return '-';
  const decimals = opts.decimals ?? 1;
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let unit = 0;
  while (Math.abs(value) >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  if (unit === 0) return `${value} B`;
  const shown = value >= 100 || unit === 0 ? value.toFixed(0) : value.toFixed(decimals);
  return `${shown} ${units[unit]}`;
}

export function formatSpeed(bytesPerSec?: number | null): string {
  if (!bytesPerSec || bytesPerSec <= 0) return '-';
  return `${formatBytes(bytesPerSec)}/s`;
}

export function formatDuration(seconds: number | null): string {
  if (seconds == null) return '-';
  if (seconds <= 0) return '0s';
  const s = Math.round(seconds);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  const rem = s % 60;
  if (m < 60) return rem ? `${m}m ${rem}s` : `${m}m`;
  const h = Math.floor(m / 60);
  const mRem = m % 60;
  if (h < 24) return mRem ? `${h}h ${mRem}m` : `${h}h`;
  const d = Math.floor(h / 24);
  const hRem = h % 24;
  return hRem ? `${d}d ${hRem}h` : `${d}d`;
}

export function progressPercent(downloaded: number, total?: number | null): number {
  if (!total) return 0;
  return Math.min(100, Math.max(0, (downloaded / total) * 100));
}

export type StateKind = 'good' | 'active' | 'warn' | 'bad' | 'muted' | 'done';

export function stateKind(state: TaskState): StateKind {
  switch (state) {
    case 'Downloading':
    case 'Seeding':
      return 'good';
    case 'Resolving':
    case 'Verifying':
      return 'active';
    case 'Queued':
      return 'warn';
    case 'Failed':
      return 'bad';
    case 'Paused':
    case 'Stopped':
      return 'muted';
    case 'Completed':
      return 'done';
    default:
      return 'muted';
  }
}

export function stateColorVar(kind: StateKind): string {
  return `var(--state-${kind})`;
}

export function stateBgVar(kind: StateKind): string {
  return `var(--state-${kind}-bg)`;
}

// Whether a state represents live in-flight transfer — drives the active
// progress-shimmer treatment on the row and detail hero.
export function isActive(state: TaskState): boolean {
  return state === 'Downloading' || state === 'Resolving' || state === 'Verifying' || state === 'Seeding';
}

// Which actions are valid for a given state.
export function canStart(state: TaskState): boolean {
  return state === 'Paused' || state === 'Stopped' || state === 'Queued' || state === 'Failed';
}
export function canPause(state: TaskState): boolean {
  return state === 'Downloading' || state === 'Seeding' || state === 'Resolving' || state === 'Verifying';
}
export function canStop(state: TaskState): boolean {
  return (
    state === 'Downloading' ||
    state === 'Seeding' ||
    state === 'Resolving' ||
    state === 'Verifying' ||
    state === 'Queued' ||
    state === 'Paused'
  );
}
export function canDelete(): boolean {
  return true;
}
