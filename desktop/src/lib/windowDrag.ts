import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

const INTERACTIVE_SELECTOR = [
  'button',
  'input',
  'textarea',
  'select',
  'a',
  '[role="button"]',
  '[role="menuitem"]',
  '[contenteditable="true"]',
  '[data-window-no-drag]'
].join(',');

/** Start native window dragging from non-interactive titlebar content. */
function startWindowDrag(event: MouseEvent) {
  if (event.button !== 0 || !isTauri()) return;

  const target = event.target;
  if (target instanceof Element && target.closest(INTERACTIVE_SELECTOR)) return;

  void getCurrentWindow().startDragging().catch(() => {
    // The data-tauri-drag-region attributes remain as a native fallback.
  });
}

export function windowDrag(node: HTMLElement) {
  node.addEventListener('mousedown', startWindowDrag);
  return {
    destroy() {
      node.removeEventListener('mousedown', startWindowDrag);
    }
  };
}
