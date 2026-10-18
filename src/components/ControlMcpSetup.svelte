<script lang="ts">
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
      onNotify('Softwaresteuerung und Skill installiert. Wähle anschließend die Freigaben für deinen Bot.');
    }catch(error){onNotify(prettyError(error),true);}finally{operation='';}
  }
  async function copyConfig(){
    if(!native||operation)return;
    operation='copy';
    try{
      const {url,token}=await call<{url:string;token:string}>('get_control_mcp_config');
      const config=client==='codex'
        ? `[mcp_servers.streamer-ai-cohost]\nurl = ${JSON.stringify(url)}\nhttp_headers = { Authorization = ${JSON.stringify('Bearer '+token)} }\n`
        : JSON.stringify({mcpServers:{'streamer-ai-cohost':{type:'http',url,headers:{Authorization:'Bearer '+token}}}},null,2);
      await navigator.clipboard.writeText(config);
      onNotify('MCP-Konfiguration kopiert. Sie enthält den lokalen Steuerungsschlüssel und gilt bis zum nächsten App-Neustart.');
    }catch(error){onNotify(prettyError(error),true);}finally{operation='';}
  }
</script>
<section class="control-setup" aria-labelledby="control-mcp-heading">
  <div class="control-intro"><span class="control-symbol"><Cable size={21}/></span><div><h2 id="control-mcp-heading">Den Cohost per Agent steuern</h2><p>Eigener MCP-Server und passender Skill für diese Software.</p></div><HelpTip text="Der eingebaute MCP-Server steuert freigegebene Funktionen der laufenden App: Bots und Emotes anzeigen, erlaubte Gesten auslösen und Ausgaben steuern. Installiere ihn hier einmal und erteile anschließend gezielt Bot- und Werkzeugrechte. Externe Agenten verbinden sich über den lokalen HTTP-Endpunkt."/></div>
  <div class="control-actions"><button class="control-install" onclick={install} disabled={!native||busy||!!operation}><BookOpen size={15}/>{operation==='install'?'Installiert …':'MCP + Skill installieren'}</button><div class="external-config"><select aria-label="Format für externen MCP-Client" bind:value={client}><option value="claude">Claude / MCP-JSON</option><option value="codex">Codex · TOML</option></select><button onclick={copyConfig} disabled={!native||!!operation}><Copy size={14}/>{operation==='copy'?'Kopiert …':'Verbindung kopieren'}</button></div></div>
  <p class="control-note">Installation erteilt keine Botrechte. Externe Verbindungen benötigen die laufende App; der kopierte Steuerungsschlüssel erneuert sich bei jedem Neustart. Externe Agenten mit diesem Schlüssel erhalten die angebotenen Steuerungswerkzeuge für alle Bots. Behandle ihn wie einen Zugangsschlüssel.</p>
</section>
<style>
.control-setup{border:1px solid #46516a;border-radius:10px;background:#202839;padding:19px 21px;margin-bottom:22px}.control-intro{display:flex;gap:12px;align-items:center}.control-symbol{display:grid;place-items:center;width:36px;height:36px;flex-shrink:0;border-radius:8px;background:#354158;color:#c3d1ed}.control-intro h2{font-size:16px}.control-intro p{font-size:10px;color:#99a9c0;margin-top:5px}.control-intro>div{flex:1}.control-actions{display:flex;gap:15px;align-items:center;justify-content:space-between;margin-top:17px;flex-wrap:wrap}.control-actions button{display:flex;gap:7px;align-items:center;padding:10px 12px;border:1px solid #55647e;border-radius:6px;background:#303c52;color:#d3def3;font-size:10px}.control-actions button:hover{background:#3c4b67}.control-actions .control-install{background:#b5c1ed;border-color:#b5c1ed;color:#202b42;font-weight:700}.external-config{display:flex;align-items:center;gap:7px}.external-config select{width:auto;padding:10px 12px;border:1px solid #46566f;border-radius:6px;background:#182235;color:#c2cee5;font-size:10px;margin:0}.control-note{font-size:9px;color:#93a3bc;line-height:1.8;margin-top:13px;max-width:830px}
</style>
