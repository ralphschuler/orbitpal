<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '../lib/i18n';
  import { call, native, prettyError } from '../lib/api';
  import { systemPermission } from '../lib/system-permission';
  import { ScreenPermissionController, type PermissionState } from '../lib/screen-permission';
  let { disabled=false, permissionContext=()=>'' }:{disabled?:boolean;permissionContext?:()=>string}=$props();
  let permissionState=$state<PermissionState>({permission:null,loading:false,error:null});
  let settingsError=$state<unknown>(null);
  let disposed=false;
  const controller=new ScreenPermissionController(command=>call(command),next=>permissionState=next);
  async function requestPermission(){const context=permissionContext();await systemPermission.ensure('screen',()=>!disposed&&!disabled&&context===permissionContext());if(!disposed)await controller.refresh();}
  async function settings(){settingsError=null;try{await call('open_screen_capture_settings');}catch(error){if(!disposed)settingsError=error;}}
  onMount(()=>{
    if(native)void controller.refresh();
    const refresh=()=>{if(native)void controller.refresh();};
    window.addEventListener('focus',refresh);
    return()=>{disposed=true;controller.dispose();systemPermission.invalidate();window.removeEventListener('focus',refresh);};
  });
</script>

{#if native && (permissionState.permission?.platform === 'macos' || permissionState.error)}
  <section class="screen-permission" aria-label={$t('Bildschirmfreigabe')}>
    <div class="permission-heading">
      <strong>{$t(permissionState.permission?.platform === 'macos' ? 'macOS-Bildschirmfreigabe' : 'Bildschirmfreigabe')}</strong>
      <span class="permission-state" class:authorized={!permissionState.loading && !permissionState.error && permissionState.permission?.status === 'authorized'} role="status">
        {permissionState.loading ? $t('Prüft …') : permissionState.error ? $t('Unbekannt') : permissionState.permission?.status === 'authorized' ? $t('macOS-Freigabe erteilt') : $t('Nicht freigegeben')}
      </span>
    </div>
    {#if permissionState.permission?.platform === 'macos' && permissionState.permission.status !== 'authorized'}
      <p>{$t('Klicke auf „Freigabe anfragen“, damit macOS OrbitPal die Bildschirmaufnahme erlauben kann. Falls bereits abgelehnt, aktiviere OrbitPal unter Datenschutz & Sicherheit → Bildschirmaufnahme beziehungsweise Bildschirm- & Systemaudioaufnahme. Fehlt die App, kannst du sie dort über + hinzufügen. Beende und starte OrbitPal nach einer Änderung bei Bedarf neu.')}</p>
    {/if}
    {#if permissionState.permission?.platform === 'macos'}
      <p>{$t('Statusprüfung und Freigabe nehmen keinen Screenshot auf. Die Freigabe gilt für die App; Bildschirmkontext und Bildübertragung bleiben zusätzlich pro Bot steuerbar.')}</p>
    {/if}
    <div class="permission-actions">
      {#if permissionState.permission?.platform === 'macos' && permissionState.permission.status !== 'authorized'}
        <button class="button secondary" disabled={disabled || permissionState.loading} onclick={requestPermission}>
          {$t('Freigabe anfragen')}
        </button>
      {/if}
      {#if permissionState.permission?.platform === 'macos'}
        <button class="button secondary" disabled={disabled || permissionState.loading} onclick={settings}>
          {$t('Bildschirmfreigabe öffnen')}
        </button>
      {/if}
      <button class="button secondary" disabled={permissionState.loading} onclick={() => controller.refresh()}>
        {$t('Status erneut prüfen')}
      </button>
    </div>
    {#if permissionState.error || settingsError}
      <p class="error" role="alert">{$t(prettyError(permissionState.error || settingsError))}</p>
    {/if}
  </section>
{/if}

<style>
  .screen-permission { margin: 16px 0; padding: 16px; border: 1px solid var(--line); border-radius: 7px; background: var(--surface-raised); }
  .permission-heading { display: flex; flex-wrap: wrap; gap: 10px; justify-content: space-between; align-items: center; }
  .permission-heading strong { color: var(--text); font-size: 13px; }
  .permission-state { color: var(--warning); font-size: 12px; line-height: 1.6; }
  .permission-state.authorized { color: var(--accent); }
  p { color: var(--muted); font-size: 12px; line-height: 1.7; margin: 12px 0 0; }
  .permission-actions { display: flex; flex-wrap: wrap; gap: 9px; margin-top: 16px; }
  .permission-actions button { min-height: 40px; white-space: normal; text-align: left; }
  .error { border-left: 3px solid var(--danger); padding-left: 12px; color: var(--danger); }
</style>
