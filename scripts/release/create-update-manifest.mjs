#!/usr/bin/env node

import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';

export function parseArgs(argv) {
  const args = {};
  for (let i = 0; i < argv.length; i += 1) {
    const value = argv[i];
    if (!value.startsWith('--')) continue;
    const key = value.slice(2);
    const next = argv[i + 1];
    if (!next || next.startsWith('--')) {
      args[key] = true;
    } else {
      args[key] = next;
      i += 1;
    }
  }
  return args;
}

function required(args, name) {
  if (!args[name] || typeof args[name] !== 'string') {
    throw new Error(`Missing --${name}`);
  }
  return args[name];
}

function walkFiles(root) {
  const result = [];
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) result.push(...walkFiles(path));
    else result.push(path);
  }
  return result;
}

function urlPathSegment(value) {
  return encodeURIComponent(value).replaceAll('%2F', '/');
}

export function createManifest({ channel, version, baseUrl, artifactsDir, notes, pubDate }) {
  validateRelease(channel, version);
  if (baseUrl.replace(/\/$/, '') !== 'https://assets.fluxion.alkinum.io') throw new Error('Unexpected update origin');
  const root = resolve(artifactsDir);
  if (!existsSync(root)) throw new Error(`Artifacts directory does not exist: ${root}`);

  const files = walkFiles(root);
  const archives = files.filter((file) => file.endsWith('.app.tar.gz'));
  if (archives.length > 1) throw new Error('Expected exactly one universal update archive');
  const updateArchive = archives[0];
  if (!updateArchive) throw new Error('No native updater archive (*.app.tar.gz) found');
  const signature = `${updateArchive}.sig`;
  if (!existsSync(signature)) throw new Error(`Missing updater signature: ${relative(root, signature)}`);

  const normalizedBase = baseUrl.replace(/\/$/, '');
  const releaseBase = `${normalizedBase}/releases/${channel}/${version}`;
  const archiveName = relative(root, updateArchive).split('\\').join('/');
  const signatureText = readFileSync(signature, 'utf8').trim();
  if (!signatureText) throw new Error(`Empty updater signature: ${relative(root, signature)}`);

  const platform = {
    signature: signatureText,
    url: `${releaseBase}/${archiveName.split('/').map(urlPathSegment).join('/')}`
  };

  return {
    channel,
    version,
    notes: notes ?? `Fluxion ${version} (${channel})`,
    pub_date: pubDate ?? new Date().toISOString(),
    platforms: {
      'darwin-aarch64': platform,
      'darwin-x86_64': platform
    }
  };
}

export function validateRelease(channel, version) {
  if (!/^(stable|beta)$/.test(channel)) throw new Error(`Unsupported channel: ${channel}`);
  const n = '(0|[1-9][0-9]*)';
  if (!new RegExp(`^${n}\\.${n}\\.${n}(?:-beta\\.${n})?$`).test(version)) throw new Error(`Invalid version: ${version}`);
  if ((channel === 'beta') !== version.includes('-beta.')) throw new Error('Release channel does not match version');
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const manifest = createManifest({
    channel: required(args, 'channel'),
    version: required(args, 'version'),
    baseUrl: required(args, 'base-url'),
    artifactsDir: required(args, 'artifacts-dir'),
    notes: args.notes,
    pubDate: args['pub-date']
  });
  const output = resolve(required(args, 'output'));
  mkdirSync(resolve(output, '..'), { recursive: true });
  writeFileSync(output, `${JSON.stringify(manifest, null, 2)}\n`);
  process.stdout.write(`${output}\n`);
}

if (import.meta.url === `file://${process.argv[1]}`) main();
