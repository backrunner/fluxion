import './styles/base.scss';
import App from './App.svelte';
import { mount } from 'svelte';
import { themeStore, type Theme } from './lib/stores/theme';

function applyTheme(theme: Theme) {
  document.documentElement.setAttribute('data-theme', theme);
}

applyTheme(themeStore.current());

const app = mount(App, {
  target: document.getElementById('app') as HTMLElement
});

export default app;