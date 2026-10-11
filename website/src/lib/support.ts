// The public site key must match Fluxion's hostname and OnFire verifier.
export const supportSiteKey = import.meta.env?.VITE_TURNSTILE_SITE_KEY ?? '';
export const supportEnabled = import.meta.env?.VITE_SUPPORT_ENABLED === 'true' && Boolean(supportSiteKey);
export const supportTopics = ['problem', 'feature', 'question', 'privacy'] as const;
export const supportLocales = ['en', 'zh'] as const;
export type SupportLocale = typeof supportLocales[number];
export type SupportTopic = typeof supportTopics[number];

export interface SupportInput {
  email: string;
  topic: SupportTopic;
  locale: SupportLocale;
  subject: string;
  content: string;
  appVersion: string;
  macosVersion: string;
  transferProtocol: string;
  turnstileToken: string;
}

export function validateSupportInput(value: unknown): SupportInput | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const source = value as Record<string, unknown>;
  const limits = { email: 254, topic: 20, locale: 12, subject: 200, content: 10000, appVersion: 80, macosVersion: 80, transferProtocol: 80, turnstileToken: 2048 };
  const clean: Record<string, string> = {};
  for (const [key, limit] of Object.entries(limits)) {
    const v = source[key] ?? '';
    if (typeof v !== 'string' || v.length > limit) return null;
    clean[key] = v.trim();
  }
  if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(clean.email) || !clean.subject || !clean.content || !clean.turnstileToken) return null;
  if (!supportTopics.some(topic => topic === clean.topic) || !supportLocales.some(locale => locale === clean.locale)) return null;
  clean.email = clean.email.toLowerCase();
  return clean as unknown as SupportInput;
}
