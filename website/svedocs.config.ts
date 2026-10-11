import { defineConfig } from 'svedocs/config';
import { en, zh } from './src/lib/messages.ts';

export default defineConfig({
  site: {
    name: 'Fluxion', title: 'Fluxion',
    description: 'A native Mac download manager. Bring your transfers together, keep the details in view, and stay in control.',
    url: 'https://fluxion.alkinum.com'
  },
  build: { mode: 'static' },
  theme: {
    defaultMode: 'system', readingStyle: 'plain',
    palette: { accent: '#af4e2d', neutral: 'stone' },
    fonts: { sans: '-apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC", sans-serif', mono: '"SFMono-Regular", Consolas, monospace' },
    radius: '12px',
    brand: { label: 'Fluxion', href: '/', logo: '/app-icon.png', mark: false },
    nav: [
      { label: 'Overview', labelKey: 'site.overview', href: '/' },
      { label: 'Documentation', labelKey: 'nav.documentation', href: '/docs' },
      { label: 'Support', labelKey: 'site.support', href: '/support' }
    ],
    footer: {
      text: 'Fluxion · Make room for what matters.',
      links: [
        { label: 'Privacy', labelKey: 'site.privacy', href: '/privacy' },
        { label: 'Licenses', labelKey: 'site.licenses', href: '/licenses' },
        { label: 'Release status', labelKey: 'site.download', href: '/download' },
        { label: 'Support', labelKey: 'site.support', href: '/support' }
      ]
    }
  },
  search: { provider: 'local', scope: 'current' }, ai: false,
  agent: { enabled: true, markdown: true, llms: true, negotiation: false },
  checks: { assets: true, translations: true },
  i18n: {
    defaultLocale: 'en', prefixDefaultLocale: false,
    locales: [{ code: 'en', label: 'English', hreflang: 'en', ogLocale: 'en_US' }, { code: 'zh', label: '简体中文', hreflang: 'zh-CN', ogLocale: 'zh_CN' }],
    messages: { en, zh }
  }
});
