import { relaunch } from '@tauri-apps/plugin-process';
import { check, type DownloadEvent } from '@tauri-apps/plugin-updater';

export type AppUpdate = {
  version: string;
  date?: string;
  body?: string;
  install: (onProgress: (downloaded: number, total?: number) => void) => Promise<void>;
};

export type UpdateCheckState = 'idle' | 'checking' | 'current' | 'available' | 'error';

function isTauriRuntime() {
  return typeof window !== 'undefined'
    && typeof window.__TAURI_INTERNALS__?.invoke === 'function';
}

export async function checkForAppUpdate(): Promise<AppUpdate | null> {
  if (!isTauriRuntime()) return null;

  const update = await check({ timeout: 15_000 });
  if (!update) return null;

  return {
    version: update.version,
    date: update.date,
    body: update.body,
    install: async (onProgress) => {
      let downloaded = 0;
      await update.downloadAndInstall((event: DownloadEvent) => {
        if (event.event === 'Started') {
          downloaded = 0;
          onProgress(downloaded, event.data.contentLength);
        } else if (event.event === 'Progress') {
          downloaded += event.data.chunkLength;
          onProgress(downloaded);
        } else if (event.event === 'Finished') {
          onProgress(downloaded);
        }
      });
    }
  };
}

export async function restartAfterUpdate() {
  await relaunch();
}
