<script lang="ts">
  import { t } from '../lib/i18n';
  import { focusTrap } from '../lib/focus';
  import { AlertTriangle, X } from '@lucide/svelte';

  let { title, message, confirmLabel, danger = false, pending = false, onCancel, onConfirm }: {
    title: string; message: string; confirmLabel: string; danger?: boolean; pending?: boolean;
    onCancel: () => void; onConfirm: () => void;
  } = $props();
  const uid = $props.id();
</script>

<div class="modal-backdrop" role="presentation" onclick={event => {
  if (event.target === event.currentTarget && !pending) onCancel();
}}>
  <div class="modal confirm-dialog" role="alertdialog" aria-modal="true"
    aria-labelledby={`${uid}-title`} aria-describedby={`${uid}-message`} tabindex="-1" use:focusTrap>
    <div class="modal-heading">
      <div class="confirm-heading"><AlertTriangle size={21} aria-hidden="true"/><h2 id={`${uid}-title`}>{title}</h2></div>
      <button class="icon-button" onclick={onCancel} disabled={pending} aria-label={$t('Dialog schließen')}><X size={19}/></button>
    </div>
    <p id={`${uid}-message`}>{message}</p>
    <div class="modal-footer">
      <button class="button secondary" data-autofocus onclick={onCancel} disabled={pending}>{$t('Abbrechen')}</button>
      <button class="button" class:danger onclick={onConfirm} disabled={pending}>{pending ? $t('Wird ausgeführt …') : confirmLabel}</button>
    </div>
  </div>
</div>

<style>
  .confirm-dialog { width: min(480px, calc(100vw - 32px)); }
  .confirm-heading { display: flex; align-items: center; gap: 12px; }
  .confirm-heading > :global(svg) { color: var(--warning); flex: 0 0 auto; }
  .confirm-heading h2 { font-size: 20px; }
  .confirm-dialog > p { color: var(--muted); font-size: 13px; line-height: 1.7; overflow-wrap: anywhere; }
  .danger { color: #23171b; background: var(--danger); }
  .danger:hover { background: #f4bcc0; }
</style>
