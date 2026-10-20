<script lang="ts">
  import { t, translate } from '../lib/i18n';
  import { Cable, Copy, BookOpen } from '@lucide/svelte';
  import { call, native, prettyError } from '../lib/api';
  import HelpTip from './HelpTip.svelte';
  let {busy=false,onInstalled,onNotify}:{busy?:boolean;onInstalled:(id:string)=>Promise<void>;onNotify:(message:string,error?:boolean)=>void}=$props();
  let operation=$state('');
  let client=$state('claude');
  async function install(){
    if(!native||busy||operation)return;
    operation='install';
    try{
      const result=await call('install_control_extension');
      await onInstalled(result.server.id);
      onNotify(translate("Softwaresteuerung und Skill installiert. Wähle anschließend die Freigaben für deinen Bot."));
    }catch(error){onNotify(prettyError(error),true);}finally{operation='';}
  }
  async function copyConfig(){
    if(!native||operation)return;
    operation='copy';
    try{
      const {url,token}=await call<{url:string;token:string}>('get_control_mcp_config');
      const config=client==='codex'
        ? `[mcp_servers.orbitpal]\nurl = ${JSON.stringify(url)}\nhttp_headers = { Authorization = ${JSON.stringify('Bearer '+token)} }\n`
        : JSON.stringify({mcpServers:{'orbitpal':{type:'http',url,headers:{Authorization:'Bearer '+token}}}},null,2);
      await navigator.clipboard.writeText(config);
      onNotify(translate("MCP-Konfiguration kopiert. Sie enthält den lokalen Steuerungsschlüssel und gilt bis zum nächsten App-Neustart."));
    }catch(error){onNotify(prettyError(error),true);}finally{operation='';}
  }
</script>
<section class="control-setup" aria-labelledby="control-mcp-heading">
  <div class="control-intro"><span class="control-symbol"><Cable size={21}/></span><div><h2 id="control-mcp-heading">{$t("Den Cohost per Agent steuern")}</h2><p>{$t("Eigener MCP-Server und passender Skill für diese Software.")}</p></div><HelpTip text={$t("Der eingebaute MCP-Server steuert freigegebene Funktionen der laufenden App: Bots und Emotes anzeigen, erlaubte Gesten auslösen und Ausgaben steuern. Installiere ihn hier einmal und erteile anschließend gezielt Bot- und Werkzeugrechte. Externe Agenten verbinden sich über den lokalen HTTP-Endpunkt.")}/></div>
  <div class="control-actions"><button class="control-install" onclick={install} disabled={!native||busy||!!operation}><BookOpen size={15}/>{operation==='install'?$t("Installiert …"):$t("MCP + Skill installieren")}</button><div class="external-config"><select aria-label={$t("Format für externen MCP-Client")} bind:value={client}><option value="claude">Claude / MCP-JSON</option><option value="codex">Codex · TOML</option></select><button onclick={copyConfig} disabled={!native||!!operation}><Copy size={14}/>{operation==='copy'?$t("Kopiert …"):$t("Verbindung kopieren")}</button></div></div>
  <p class="control-note">{$t("Installation erteilt keine Botrechte. Externe Verbindungen benötigen die laufende App; der kopierte Steuerungsschlüssel erneuert sich bei jedem Neustart. Externe Agenten mit diesem Schlüssel erhalten die angebotenen Steuerungswerkzeuge für alle Bots. Behandle ihn wie einen Zugangsschlüssel.")}</p>
</section>
<style>
  .control-setup { border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel); padding: 22px; margin-bottom: 22px; }
  .control-intro { display: flex; gap: 12px; align-items: center; }
  .control-symbol { display: grid; place-items: center; width: 38px; height: 38px; flex-shrink: 0; border-radius: 8px; background: var(--surface-raised); color: var(--accent); }
  .control-intro h2 { font-size: 16px; }
  .control-intro p { font-size: 12px; color: var(--muted); margin-top: 5px; }
  .control-intro > div { flex: 1; min-width: 0; }
  .control-actions { display: flex; gap: 16px; align-items: center; justify-content: space-between; margin-top: 18px; flex-wrap: wrap; }
  .control-actions button { display: flex; gap: 8px; align-items: center; min-height: 40px; padding: 10px 14px; border: 1px solid var(--line); border-radius: 7px; background: var(--surface-raised); color: var(--text); font-size: 12px; }
  .control-actions button:hover { background: #2b3e4a; }
  .control-actions .control-install { background: var(--accent); border-color: var(--accent); color: #132a2c; font-weight: 700; }
  .external-config { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; min-width: 0; }
  .external-config select { width: auto; max-width: 100%; min-height: 40px; padding: 10px 12px; border: 1px solid var(--line); border-radius: 7px; background: var(--input-bg); color: var(--text); font-size: 12px; margin: 0; }
  .control-note { font-size: 12px; color: var(--muted); line-height: 1.7; margin-top: 14px; max-width: 100ch; }
</style>
