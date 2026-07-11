import type { HeaderPair } from './types';

export interface HttpHeaderFields {
  customHeaders: string;
  cookie: string;
  referer: string;
  userAgent: string;
}

export interface SplitHttpHeaders {
  headers: HeaderPair[];
  credentialHeaders: HeaderPair[];
}

export function isSensitiveHeader(name: string): boolean {
  const lower = name.toLowerCase();
  return lower === 'cookie'
    || lower === 'authorization'
    || lower === 'proxy-authorization'
    || lower.includes('token')
    || lower.includes('secret')
    || lower.includes('password')
    || lower.includes('key');
}

export function splitHttpHeaders(fields: HttpHeaderFields): SplitHttpHeaders {
  const byName = new Map<string, HeaderPair>();

  for (const line of fields.customHeaders.split('\n')) {
    const trimmed = line.trim();
    if (!trimmed) continue;

    const separator = trimmed.indexOf(':');
    if (separator <= 0) continue;

    const name = trimmed.slice(0, separator).trim();
    const value = trimmed.slice(separator + 1).trim();
    const normalizedName = name.toLowerCase();
    if (name && !byName.has(normalizedName)) {
      byName.set(normalizedName, { name, value });
    }
  }

  // Dedicated fields are the most explicit input, so they replace a custom
  // header with the same case-insensitive name instead of creating a duplicate.
  const setDedicatedHeader = (name: string, value: string) => {
    if (value) byName.set(name.toLowerCase(), { name, value });
  };
  setDedicatedHeader('Cookie', fields.cookie);
  setDedicatedHeader('Referer', fields.referer);
  setDedicatedHeader('User-Agent', fields.userAgent);

  const headers: HeaderPair[] = [];
  const credentialHeaders: HeaderPair[] = [];
  for (const header of byName.values()) {
    (isSensitiveHeader(header.name) ? credentialHeaders : headers).push(header);
  }

  return { headers, credentialHeaders };
}
