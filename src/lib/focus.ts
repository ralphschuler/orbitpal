import type { Action } from 'svelte/action';
/** Keep keyboard navigation in an open dialog, then restore its trigger. */
export const focusTrap: Action<HTMLElement> = (node) => {
  const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  let mounted = true;
  const candidates = () => Array.from(node.querySelectorAll<HTMLElement>('button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),a[href],[tabindex]:not([tabindex="-1"])')).filter(element => !element.hidden && element.getClientRects().length > 0);
  queueMicrotask(() => { if (mounted) (node.querySelector<HTMLElement>('[data-autofocus]') || candidates()[0] || node).focus(); });
  const keydown = (event: KeyboardEvent) => {
    if (event.key !== 'Tab') return;
    const items = candidates();
    const current = document.activeElement;
    if (!items.length) { event.preventDefault(); node.focus(); return; }
    if (event.shiftKey && (current === items[0] || !node.contains(current))) { event.preventDefault(); items.at(-1)?.focus(); }
    else if (!event.shiftKey && (current === items.at(-1) || !node.contains(current))) { event.preventDefault(); items[0].focus(); }
  };
  node.addEventListener('keydown', keydown);
  return { destroy() { mounted = false; node.removeEventListener('keydown', keydown); if (previous?.isConnected) previous.focus(); } };
};
