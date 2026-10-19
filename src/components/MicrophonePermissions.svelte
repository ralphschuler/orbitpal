<script lang="ts">
  import { t } from '../lib/i18n';
  import { onMount } from 'svelte';
  import { call, native, prettyError } from '../lib/api';
  import { microphoneFailure, microphonePermissionMessage, type MicrophonePermission } from '../lib/microphone-permission';
  let permission = $state<MicrophonePermission | null>(null);
  let loading = $state(false);
  let error = $state<unknown>(null);
  let expanded = $state(false);
  let disposed = false;
  const detailsVisible = $derived(expanded || !!$microphoneFailure || !!permission && ['denied', 'restricted'].includes(permission.status));
  const labels = { notDetermined: 'Noch nicht angefragt', restricted: 'Vom System beschränkt', denied: 'Nicht freigegeben', authorized: 'macOS-Freigabe erteilt', webview: 'Wird beim Start von der WebView geprüft', unknown: 'Unbekannt' };
  async function refresh() {
    if (!native || loading) return;
    loading = true; error = null;
    try { const next = await call<MicrophonePermission>('get_microphone_permission'); if (!disposed) permission = next; }
    catch (failure) { if (!disposed) error = failure; }
    finally { if (!disposed) loading = false; }
  }
  async function settings() {
    try { await call('open_microphone_settings'); }
    catch (failure) { error = failure; }
  }
  onMount(() => { void refresh(); return () => { disposed = true; }; });
</script>

{#if native}
  <section class="microphone-permissions" aria-label={$t("Mikrofonfreigabe")}>
    <button class="permission-heading" onclick={() => { expanded = !expanded; if (expanded) void refresh(); }} aria-expanded={detailsVisible}>
      {$t("Mikrofonfreigabe")} <span>{permission ? $t(labels[permission.status]) : $t("Status prüfen")} · {expanded ? '−' : '+'}</span>
    </button>
    {#if detailsVisible}
      <div class="permission-details">
        {#if $microphoneFailure}<p role="alert">{$t($microphoneFailure)}</p>{:else if permission && ['denied', 'restricted', 'unknown'].includes(permission.status)}<p>{$t(microphonePermissionMessage(permission.status))}</p>{/if}
        <p>{$t("Die Freigabe wird nur mit deinem Klick auf Aufnahme oder „Zuhören starten“ angefragt. Eine Statusprüfung öffnet kein Mikrofon. Nach Änderungen und App-Neustart musst du die Aufnahme erneut starten.")}</p>
        {#if permission?.platform === 'macos'}<p>{$t("macOS: Systemeinstellungen → Datenschutz & Sicherheit → Mikrofon → OrbitPal. Fehlt die App in der Liste, verwende nach der Installation der korrigierten Version zuerst den Aufnahme- oder Startknopf.")}</p>{/if}
        <div class="permission-actions">
          {#if permission && ['macos', 'windows'].includes(permission.platform)}<button onclick={settings}>{$t("Mikrofoneinstellungen öffnen")}</button>{/if}
          <button onclick={refresh} disabled={loading}>{loading ? $t("Prüft …") : $t("Status erneut prüfen")}</button>
          {#if $microphoneFailure}<button onclick={() => microphoneFailure.set('')}>{$t("Hinweis schließen")}</button>{/if}
        </div>
        {#if error}<p role="alert">{$t(prettyError(error))}</p>{/if}
      </div>
    {/if}
  </section>
{/if}

<style>
  .microphone-permissions { margin: -8px 0 18px; border: 1px solid #3c4860; border-radius: 10px; background: #1e2635; }
  .permission-heading { width: 100%; display: flex; flex-wrap: wrap; justify-content: space-between; gap: 8px; padding: 10px 13px; background: none; border: 0; color: #cad6ee; font: inherit; font-size: 11px; text-align: left; cursor: pointer; }
  .permission-heading span { color: #a8b6ce; }
  .permission-details { padding: 0 13px 13px; }
  p { margin: 6px 0; font-size: 11px; line-height: 1.6; color: #b5c3dc; }
  [role=alert] { color: #efbbc0; }
  .permission-actions { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 10px; }
  .permission-actions button { background: #303c52; border: 1px solid #55647e; border-radius: 7px; padding: 7px 10px; color: #d3def3; font: inherit; font-size: 11px; cursor: pointer; }
</style>
