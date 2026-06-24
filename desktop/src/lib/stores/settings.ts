// Settings store — holds the SettingsSnapshot and a derived form model.
import { writable } from 'svelte/store';
import type { SettingsSnapshot } from '../types';

export const settingsStore = writable<SettingsSnapshot | null>(null);

export function hydrateSettingsForm(s: SettingsSnapshot) {
  return {
    downloadLimit: s.download_limit?.toString() ?? '',
    uploadLimit: s.upload_limit?.toString() ?? '',
    useSystemProxy: s.use_system_proxy,
    btTrackers: s.bt_trackers.join('\n'),
    btIpAllow: s.bt_ip_allow.join('\n'),
    btIpDeny: s.bt_ip_deny.join('\n')
  };
}

export type SettingsForm = ReturnType<typeof hydrateSettingsForm>;

export function settingsFormToSnapshot(form: SettingsForm, base: SettingsSnapshot): SettingsSnapshot {
  const toList = (text: string) =>
    text
      .split('\n')
      .map((line) => line.trim())
      .filter(Boolean);
  return {
    ...base,
    download_limit: form.downloadLimit ? Number(form.downloadLimit) : null,
    upload_limit: form.uploadLimit ? Number(form.uploadLimit) : null,
    use_system_proxy: form.useSystemProxy,
    bt_trackers: toList(form.btTrackers),
    bt_ip_allow: toList(form.btIpAllow),
    bt_ip_deny: toList(form.btIpDeny)
  };
}