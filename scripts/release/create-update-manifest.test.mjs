import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { createManifest } from './create-update-manifest.mjs';

test('creates a signed universal macOS manifest for a channel', () => {
  const root = mkdtempSync(join(tmpdir(), 'fluxion-manifest-'));
  const archive = join(root, 'Fluxion_1.2.3-beta.4_universal.app.tar.gz');
  writeFileSync(archive, 'archive');
  writeFileSync(`${archive}.sig`, 'signature\n');

  const manifest = createManifest({
    channel: 'beta',
    version: '1.2.3-beta.4',
    baseUrl: 'https://assets.fluxion.alkinum.io',
    artifactsDir: root,
    pubDate: '2026-07-11T00:00:00.000Z'
  });

  assert.equal(manifest.version, '1.2.3-beta.4');
  assert.equal(manifest.platforms['darwin-aarch64'].signature, 'signature');
  assert.deepEqual(manifest.platforms['darwin-aarch64'], manifest.platforms['darwin-x86_64']);
  assert.match(
    manifest.platforms['darwin-aarch64'].url,
    /releases\/beta\/1\.2\.3-beta\.4\/Fluxion_1\.2\.3-beta\.4_universal\.app\.tar\.gz$/
  );
});

test('rejects a release without a signed updater archive', () => {
  const root = mkdtempSync(join(tmpdir(), 'fluxion-manifest-'));
  mkdirSync(join(root, 'nested'));
  assert.throws(
    () => createManifest({
      channel: 'stable',
      version: '1.2.3',
      baseUrl: 'https://assets.fluxion.alkinum.io',
      artifactsDir: root
    }),
    /No native updater archive/
  );
});
