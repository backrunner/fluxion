// Theme store — light/dark. Persists ONLY the theme name (non-secret).
import { writable } from 'svelte/store';

export type Theme = 'light' | 'dark';

const STORAGE_KEY = 'fluxion.theme';

function isTheme(value: unknown): value is Theme {
  return value === 'light' || value === 'dark';
}

function readInitial(): Theme {
  if (typeof window === 'undefined') return 'dark';
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    if (isTheme(stored)) return stored;
  } catch {
    // localStorage unavailable — fall through.
  }
  // Respect the OS preference on first run.
  if (typeof window.matchMedia === 'function' && window.matchMedia('(prefers-color-scheme: light)').matches) {
    return 'light';
  }
  return 'dark';
}

function createThemeStore() {
  const initial = readInitial();
  const { subscribe, set, update } = writable<Theme>(initial);

  function persist(theme: Theme) {
    try {
      window.localStorage.setItem(STORAGE_KEY, theme);
    } catch {
      // Ignore persistence failures.
    }
    document.documentElement.setAttribute('data-theme', theme);
  }

  return {
    subscribe,
    set(theme: Theme) {
      persist(theme);
      set(theme);
    },
    toggle() {
      update((current) => {
        const next: Theme = current === 'dark' ? 'light' : 'dark';
        persist(next);
        return next;
      });
    },
    current(): Theme {
      return initial;
    }
  };
}

export const themeStore = createThemeStore();