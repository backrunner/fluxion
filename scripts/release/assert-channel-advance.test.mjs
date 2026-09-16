import test from 'node:test';
import assert from 'node:assert/strict';
import { assertAdvance, assertUnpublished } from './assert-channel-advance.mjs';
test('channel pointers advance numerically and never cross channels', () => {
  assertAdvance('stable', '1.10.0', { version: '1.9.0' });
  assertAdvance('beta', '1.0.0-beta.10', { version: '1.0.0-beta.9' });
  assertAdvance('beta', '1.0.0-beta.1', null);
  for (const version of ['1.0.0', '0.9.0']) assert.throws(() => assertAdvance('stable', version, {version:'1.0.0'}), /equal or older/);
  assert.throws(() => assertAdvance('stable', '1.1.0', {version: '1.0.0', channel: 'beta'}), /mismatch/);
});
test('immutable artifacts cannot be overwritten and errors fail closed', async () => {
  await assertUnpublished(['https://example.com/new'], async () => ({ status: 404 }));
  for (const status of [200, 403, 500]) {
    await assert.rejects(assertUnpublished(['https://example.com/existing'], async () => ({ status })), /overwrite/);
  }
});
