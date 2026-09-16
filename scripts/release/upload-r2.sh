#!/usr/bin/env bash
set -euo pipefail

: "${R2_BUCKET:?R2_BUCKET is required}"
: "${CHANNEL:?CHANNEL is required}"
: "${VERSION:?VERSION is required}"
: "${ASSETS_DIR:?ASSETS_DIR is required}"

command -v npx >/dev/null 2>&1 || { echo 'npx is required' >&2; exit 1; }
node scripts/release/assert-channel-advance.mjs --channel "${CHANNEL}" --version "${VERSION}" --artifacts-dir "${ASSETS_DIR}"

for file in "${ASSETS_DIR}"/*; do
  [ -f "${file}" ] || continue
  name="$(basename "${file}")"
  case "${name}" in
    *.json) content_type='application/json'; cache_control='no-cache, no-store, must-revalidate' ;;
    *.sig) content_type='text/plain; charset=utf-8'; cache_control='public, max-age=31536000, immutable' ;;
    *.dmg|*.tar.gz) content_type='application/octet-stream'; cache_control='public, max-age=31536000, immutable' ;;
    *.txt) content_type='text/plain; charset=utf-8'; cache_control='public, max-age=31536000, immutable' ;;
    *) content_type='application/octet-stream'; cache_control='public, max-age=31536000, immutable' ;;
  esac
  npx --yes wrangler@4 r2 object put "${R2_BUCKET}/releases/${CHANNEL}/${VERSION}/${name}" \
    --remote --file "${file}" --content-type "${content_type}" --cache-control "${cache_control}" --force
done

npx --yes wrangler@4 r2 object put "${R2_BUCKET}/updates/${CHANNEL}/latest.json" \
  --remote --file "${ASSETS_DIR}/latest.json" \
  --content-type 'application/json' \
  --cache-control 'no-cache, no-store, must-revalidate' \
  --force
