import { readFile, stat } from 'node:fs/promises';
import assert from 'node:assert/strict';
import { resolve } from 'node:path';

const root = resolve('build');
const origin = 'https://fluxion.alkinum.com';
const docs = ['', 'downloading', 'tasks', 'protocols', 'permissions', 'troubleshooting'];
const pages = ['', 'download', 'support', 'privacy', 'licenses'];
const routes = [...pages.map(p => `/${p}`), ...docs.map(p => `/docs/${p}`),
  ...pages.map(p => `/zh/${p}`), ...docs.map(p => `/docs/zh/${p}`)].map(p => p.replace(/\/$/, '') || '/');
const checks = [];
for (const route of routes) {
  const html = await readFile(resolve(root, `.${route}/index.html`), 'utf8');
  assert.equal((html.match(/<main(?:\s|>)/g) ?? []).length, 1, `${route}: single main landmark`);
  assert.match(html, /<h1(?:\s|>)/, `${route}: server-rendered heading`);
  assert.match(html, /name="description" content="[^"\s]/, `${route}: useful metadata`);
  assert.match(html, new RegExp(`rel="canonical" href="${origin}${route === '/' ? '/' : route + '/'}"`), `${route}: public canonical`);
  assert.match(html, /hreflang="en"/, `${route}: English alternate`);
  assert.match(html, /hreflang="zh-CN"/, `${route}: Chinese alternate`);
  assert.match(html, /<html[^>]*lang="(?:en|zh-CN)"/, `${route}: document language`);
  assert.ok(!html.includes('ezarchive'), `${route}: no reference branding`);
  if (route.endsWith('/support')) {
    assert.ok(!html.includes('name="email"'), `${route}: disabled intake collects no email`);
    assert.ok(!html.includes('turnstile/v0/api.js'), `${route}: no loaded verification script`);
  }
  // Validate local image and download assets, which Markdown page checks do not cover.
  for (const match of html.matchAll(/(?:src|href)="(\/[^"?#]+\.(?:png|svg|txt|json|md|zip))"/g)) {
    const path = resolve(root, `.${match[1]}`);
    assert.ok(path.startsWith(root + '/'));
    assert.ok((await stat(path)).size > 0, `${route}: ${match[1]} exists`);
  }
  checks.push(route);
}
const sitemap = await readFile(resolve(root, 'sitemap.xml'), 'utf8');
for (const route of routes) assert.ok(sitemap.includes(origin + (route === '/' ? '/' : route + '/')), `${route}: sitemap`);
assert.match(await readFile(resolve(root, 'robots.txt'), 'utf8'), /Sitemap: https:\/\/fluxion\.alkinum\.com\/sitemap\.xml/);
assert.match(await readFile(resolve(root, 'llms.txt'), 'utf8'), /# Fluxion/);
assert.ok((await stat(resolve(root, '404.html'))).size > 0);
console.log(`Verified ${checks.length} prerendered pages, metadata, alternates, assets, disabled intake and discovery files.`);
