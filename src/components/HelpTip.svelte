<script lang="ts">
  import { t } from '../lib/i18n';
  import { onMount, tick } from 'svelte';
  interface Props { text: string; label?: string; }
  let { text, label }: Props = $props();
  const id = $props.id();
  let hovered = $state(false);
  let focused = $state(false);
  let dismissed = $state(false);
  let trigger = $state<HTMLButtonElement>();
  let tooltip = $state<HTMLSpanElement>();
  let positioned = $state(false);
  let position = $state({ left: 12, top: 12 });
  let hoverTimer: ReturnType<typeof setTimeout> | undefined;
  const visible = $derived((hovered || focused) && !dismissed);

  function hover(value: boolean) {
    clearTimeout(hoverTimer);
    if (value) { hovered = true; dismissed = false; }
    else hoverTimer = setTimeout(() => { hovered = false; }, 120);
  }
  function place() {
    if (!visible || !trigger || !tooltip) return;
    const anchor = trigger.getBoundingClientRect();
    const width = tooltip.offsetWidth;
    const height = tooltip.offsetHeight;
    const below = anchor.bottom + 8;
    position = {
      left: Math.max(12, Math.min(anchor.left, window.innerWidth - width - 12)),
      top: Math.max(12, Math.min(below + height <= window.innerHeight - 12 ? below : anchor.top - height - 8, window.innerHeight - height - 12)),
    };
    positioned = true;
  }
  $effect(() => {
    if (!visible) { positioned = false; return; }
    const content = text;
    void tick().then(() => { if (content === text) place(); });
  });

  function keydown(event: KeyboardEvent) {
    if (event.key === 'Escape' && visible) {
      dismissed = true;
      event.preventDefault();
      event.stopImmediatePropagation();
    }
  }
  onMount(() => {
    window.addEventListener('keydown', keydown, { capture: true });
    window.addEventListener('resize', place);
    window.addEventListener('scroll', place, { capture: true, passive: true });
    return () => {
      clearTimeout(hoverTimer);
      window.removeEventListener('keydown', keydown, { capture: true });
      window.removeEventListener('resize', place);
      window.removeEventListener('scroll', place, { capture: true });
    };
  });
</script>

<span class="help-tip">
  <button bind:this={trigger} type="button" aria-label={label ?? $t("Hilfe anzeigen")} aria-describedby={id} class="help-trigger" onpointerenter={() => hover(true)} onpointerleave={() => hover(false)} onfocus={() => { focused = true; dismissed = false; }} onblur={() => { focused = false; dismissed = false; }} onkeydown={keydown} onclick={(event) => { event.preventDefault(); event.stopPropagation(); dismissed = false; }}>
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" aria-hidden="true"><circle cx="8" cy="8" r="6.15" stroke="currentColor" stroke-width="1.2"/><path d="M6.4 5.8a1.65 1.65 0 0 1 3.2.55c0 1.2-1.6 1.3-1.6 2.45" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/><circle cx="8" cy="10.9" r=".7" fill="currentColor"/></svg>
  </button>
  {#if visible}<span bind:this={tooltip} class="help-content" id={id} role="tooltip" style:left={`${position.left}px`} style:top={`${position.top}px`} style:visibility={positioned ? 'visible' : 'hidden'} onpointerenter={() => hover(true)} onpointerleave={() => hover(false)}>{text}</span>{:else}<span class="help-description" id={id}>{text}</span>{/if}
</span>

<style>
  .help-tip { display: inline-flex; align-items: center; position: relative; vertical-align: middle; flex: 0 0 auto; }
  .help-trigger { display: inline-grid; place-items: center; width: 28px; height: 28px; padding: 5px; margin: -3px 0; border: 0; border-radius: 6px; background: transparent; color: var(--muted, #a5b4c3); cursor: help; }
  .help-trigger:hover, .help-trigger:focus-visible { color: var(--text, #edf2f6); background: var(--surface-raised, #22313d); }
  .help-trigger:focus-visible { outline: 2px solid var(--accent, #91cfc4); outline-offset: 2px; }
  .help-content { position: fixed; z-index: 80; width: 320px; max-width: calc(100vw - 24px); max-height: calc(100vh - 24px); overflow-y: auto; overscroll-behavior: contain; padding: 14px 16px; border: 1px solid var(--line, #344653); border-radius: 8px; background: var(--surface-raised, #22313d); color: var(--text, #edf2f6); box-shadow: 0 8px 28px #060e1855; font-family: 'Manrope Variable', system-ui, sans-serif; font-size: 12px; font-weight: 450; line-height: 1.7; white-space: pre-line; overflow-wrap: anywhere; text-align: left; }
  .help-description { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0; }
</style>
