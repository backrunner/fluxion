import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';
import { svedocs } from 'svedocs/vite';
import svedocsConfig from './svedocs.config.ts';
export default defineConfig({
  plugins: [
    svedocs({ config: svedocsConfig, components: { SupportForm: '$lib/SupportForm.svelte' }, theme: { components: { Footer: '$lib/SiteFooter.svelte' } } }),
    tailwindcss(),
    sveltekit()
  ]
});
