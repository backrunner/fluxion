import { validateRelease, parseArgs } from './create-update-manifest.mjs';
import { readdirSync } from 'node:fs';

export async function assertUnpublished(urls, request = fetch) {
  await Promise.all(urls.map(async url => {
    const response = await request(url, { method: 'HEAD', redirect: 'error', signal: AbortSignal.timeout(15000) });
    if (response.status !== 404) throw new Error(`Refusing to overwrite an existing or unverifiable artifact: HTTP ${response.status}`);
  }));
}

export function assertAdvance(channel, version, current) {
  validateRelease(channel, version);
  if (!current) return;
  validateRelease(channel, current.version);
  if (current.channel && current.channel !== channel) throw new Error('Published channel mismatch');
  const parse = value => value.split(/\.|-beta\./).map(BigInt);
  const next = parse(version), old = parse(current.version);
  for (let i = 0; i < Math.max(next.length, old.length); i++) {
    if (next[i] > old[i]) return;
    if (next[i] < old[i]) break;
  }
  throw new Error('Refusing to replace the channel with an equal or older release');
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  validateRelease(args.channel, args.version);
  const base = args['base-url'] || 'https://assets.fluxion.alkinum.io';
  if (base !== 'https://assets.fluxion.alkinum.io') throw new Error('Unexpected update origin');
  const response = await fetch(`${base}/updates/${args.channel}/latest.json`, {
    redirect: 'error', signal: AbortSignal.timeout(15000), headers: { 'Cache-Control': 'no-cache' }
  });
  if (response.status !== 404 && !response.ok) throw new Error(`Channel check failed: HTTP ${response.status}`);
  assertAdvance(args.channel, args.version, response.status === 404 ? null : await response.json());
  if (args['artifacts-dir']) {
    const files = readdirSync(args['artifacts-dir'], { withFileTypes: true }).filter(file => file.isFile());
    await assertUnpublished(files.map(file => `${base}/releases/${args.channel}/${args.version}/${encodeURIComponent(file.name)}`));
  }
}
if (import.meta.url === `file://${process.argv[1]}`) await main();
