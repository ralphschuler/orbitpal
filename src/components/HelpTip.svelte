<script lang="ts">
  import { t } from '../lib/i18n';
  import { onMount } from 'svelte';
  interface Props { text: string; label?: string; }
  let { text, label }: Props = $props();
  const id = $props.id();
  let hovered = $state(false);
  let focused = $state(false);
  let dismissed = $state(false);
  const visible = $derived((hovered || focused) && !dismissed);

  function keydown(event: KeyboardEvent) {
    if (event.key === 'Escape' && visible) {
      dismissed = true;
      event.preventDefault();
      event.stopImmediatePropagation();
    }
  }
  onMount(() => {
    window.addEventListener('keydown', keydown, { capture: true });
    return () => window.removeEventListener('keydown', keydown, { capture: true });
  });
</script>

<span class="help-tip">
  <button type="button" aria-label={label ?? $t("Hilfe anzeigen")} aria-describedby={visible ? id : undefined} class="help-trigger" onpointerenter={() => { hovered = true; dismissed = false; }} onpointerleave={() => { hovered = false; }} onfocus={() => { focused = true; dismissed = false; }} onblur={() => { focused = false; dismissed = false; }} onkeydown={keydown} onclick={(event) => { event.preventDefault(); event.stopPropagation(); dismissed = false; }}>
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" aria-hidden="true"><circle cx="8" cy="8" r="6.15" stroke="currentColor" stroke-width="1.2"/><path d="M6.4 5.8a1.65 1.65 0 0 1 3.2.55c0 1.2-1.6 1.3-1.6 2.45" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/><circle cx="8" cy="10.9" r=".7" fill="currentColor"/></svg>
  </button>
  {#if visible}<span class="help-content" id={id} role="tooltip">{text}</span>{/if}
</span>

<style>
  .help-tip { display: inline-flex; align-items: center; position: relative; vertical-align: middle; flex: 0 0 auto; }
  .help-trigger { display: inline-grid; place-items: center; width: 24px; height: 24px; padding: 4px; margin: -3px 0; border: 0; border-radius: 6px; background: transparent; color: #a9b5cb; cursor: help; }
  .help-trigger:hover, .help-trigger:focus-visible { color: #e0e6fc; background: #333f58; }
  .help-trigger:focus-visible { outline: 2px solid #b7c5ff; outline-offset: 2px; }
  .help-content { position: absolute; z-index: 60; top: calc(100% + 8px); right: -5px; width: 300px; max-width: min(300px, 72vw); padding: 12px 14px; border: 1px solid #5a6882; border-radius: 8px; background: #28354a; color: #e1e7f2; box-shadow: 0 10px 30px #050b1880; font-family: 'Manrope Variable', system-ui, sans-serif; font-size: 11px; font-weight: 450; line-height: 1.7; white-space: pre-line; text-align: left; pointer-events: none; }
</style>
