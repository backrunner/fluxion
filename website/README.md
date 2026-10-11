# Fluxion official website

Fully customized svedocs 0.2.3 static site and a small Cloudflare Worker for future
OnFire support intake. Canonical domain: https://fluxion.alkinum.com.

The site has English and Simplified Chinese pages, local documentation search,
system/light/dark themes, responsive navigation, an interactive transfer
illustration, and policy/license/release pages. The illustration uses sample data
and is explicitly labeled; it is not an App Store screenshot.

## Development and verification

```sh
pnpm install --frozen-lockfile
pnpm dev --host 127.0.0.1 --port 48610
pnpm check
pnpm check:docs
pnpm check:worker
pnpm test:support
pnpm build
pnpm check:built
pnpm check:deploy
```

Generate app dependency notices from the repository root:

```sh
python3 scripts/release/dependency-notices.py --output website/static/licenses/AppThirdPartyNotices.txt
```

Generate website notices with `python3 scripts/collect-licenses.py` from this
directory after dependency changes. It also packages the actual modified
`vendor/librqbit` source and modification notes for the public license page. The committed inventory was generated from
the installed macOS dependency graph, including native build tools. Supplementary
upstream texts and checksums live in `../compliance/third-party/website`.

## Deployment

`pnpm deploy` builds the static site and deploys Worker `fluxion-site` with a
Cloudflare custom domain. Cloudflare credentials remain outside the repository.
`SUPPORT_ENABLED=false` is the committed production state. Do not set it to true
until the complete support path is verified. No OnFire product secret is configured
by this work.

## Reserved OnFire integration

The user explicitly deferred OnFire setup. No Fluxion product, ticket types,
product key, Turnstile widget or email delivery was created or configured.
The public support page shows preparation status and links to guides. It renders
no email/message fields and loads no Turnstile script while disabled. The API
returns 503 without contacting OnFire, even if a key is supplied accidentally.

When setup is requested:

1. Use a securely provisioned OnFire **account** key to discover the live ToB
   contract, create the dedicated Fluxion product, and publish four ticket forms:
   `problem`, `feature`, `question`, `privacy`.
2. Forms must accept `subject`, `content`, and metadata `preferred_language`,
   `app_version`, `macos_version`, `transfer_protocol` (optional except language).
   Recheck the live ToC contract before activation; this adapter follows the
   controlled EzArchive intake flow, and its tests use mocks, not live tickets.
3. Issue a Fluxion **product** key. Store it only as Worker secret `ONFIRE_API_KEY`;
   never reuse EzArchive's key or put an account key in the public intake Worker.
4. Set `ONFIRE_PRODUCT_ID` and the server-owned `ONFIRE_TICKET_TYPES` mapping in
   Wrangler. Pair a hostname-authorized Turnstile public site key with OnFire's
   verifier. Do not alter a shared widget or reuse another product's configuration
   without confirming its intended scope.
5. Confirm reply/contact and retention arrangements, update both privacy policy
   translations and the bundled app policy, then configure the public build vars
   from `.env.example` and runtime vars from `.dev.vars.example`.
6. Verify success, CAPTCHA rejection, rate limits, all topics/locales and uncertain
   writes end to end before enabling both switches and deploying. A successful
   ticket submission alone does not prove outbound email delivery.

The Worker owns topic routing, obtains the current immutable form version, keeps
customer JWTs server-side, rejects cross-origin/non-JSON/oversized requests,
applies rate limits and upstream deadlines, hides upstream errors, and never
automatically retries a ticket POST. It does not consume Turnstile tokens before
OnFire verifies them. Browser responses expose only a ticket reference.

## App Store materials

Privacy: `/privacy/` and `/zh/privacy/`. Support: `/support/` and `/zh/support/`.
The public policy documents current behavior and the disabled support status;
publisher identity and a working private support/contact route remain app-release
gates. No App Store availability, price or Apple approval is advertised.
