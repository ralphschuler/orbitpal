<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { LibraryBig, Cable, BookOpen, Search, Plus, Upload, Check, Pencil, Trash2, X, Plug, Unplug, ShieldCheck, Users, FileText, FolderOpen, ArrowRight } from '@lucide/svelte';
  import HelpTip from './HelpTip.svelte';
  import ControlMcpSetup from './ControlMcpSetup.svelte';
  import { call, prettyError } from '../lib/api';
  import { mcpGrant, setMcpEnabled, setMcpTool, setSkillEnabled } from '../lib/extensions';

  interface Library { mcpServers: any[]; skills: any[]; }
  interface Props {
    profile: any; library: Library; native: boolean; busy: boolean;
    onSave: () => Promise<boolean>; onRefresh: () => Promise<void>;
    onNotify: (message: string, error?: boolean) => void;
  }
  let { profile = $bindable(), library, native, busy, onSave, onRefresh, onNotify }: Props = $props();
  const uid = $props.id();
  let tab = $state<'mcp' | 'skills'>('mcp');
  let search = $state('');
  let filter = $state('all');
  let serverId = $state('');
  let skillId = $state('');
  let editor = $state<'mcp' | 'skill' | undefined>();
  let mcpDraft = $state<any>({});
  let skillDraft = $state({ id: '', content: '', existing: false, resources: [] as any[] });
  let operation = $state('');
  let toolCatalogs = $state<Record<string, any[]>>({});
  let deletion = $state<{ kind: 'mcp' | 'skill'; id: string; name: string }>();
  let dialog = $state<HTMLDivElement>();
  let cancelButton = $state<HTMLButtonElement>();
  let previousFocus: HTMLElement | undefined;
  let editorFocus: HTMLElement | undefined;
  let revision = 0;
  let disposed = false;
  const servers = $derived(library?.mcpServers || []);
  const skills = $derived(library?.skills || []);
  const selected = $derived(tab === 'mcp' ? servers.find((item) => item.id === serverId) : skills.find((item) => item.id === skillId));
  const grant = $derived(mcpGrant(profile.mcpServers, serverId));
  const skillActive = $derived((profile.skillsEnabled || []).includes(skillId));
  const loadedTools = $derived(toolCatalogs[serverId]);
  const items = $derived((tab === 'mcp' ? servers : skills).filter((item) => {
    const enabled = tab === 'mcp' ? mcpGrant(profile.mcpServers, item.id).enabled : (profile.skillsEnabled || []).includes(item.id);
    return `${item.name || item.id} ${item.description || ''} ${item.id}`.toLocaleLowerCase('de-DE').includes(search.trim().toLocaleLowerCase('de-DE'))
      && (filter === 'all' || (filter === 'enabled' ? enabled : !enabled));
  }));

  $effect(() => {
    const serverIds = servers.map((item) => item.id);
    const skillIds = skills.map((item) => item.id);
    untrack(() => {
      if (!serverIds.includes(serverId)) serverId = serverIds[0] || '';
      if (!skillIds.includes(skillId)) skillId = skillIds[0] || '';
    });
  });

  function notifyError(error: unknown) { onNotify(prettyError(error), true); }
  async function focusEditor(request: number) {
    await tick();
    if (!disposed && request === revision) document.getElementById(`${uid}-editor`)?.querySelector<HTMLElement>('input:not([readonly]), textarea, select')?.focus();
  }
  function selectItem(id: string) { ++revision; editor = undefined; if (tab === 'mcp') serverId = id; else skillId = id; }
  function selectTab(value: 'mcp' | 'skills') { ++revision; tab = value; editor = undefined; search = ''; }
  async function tabKey(event: KeyboardEvent) {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    selectTab(event.key === 'Home' ? 'mcp' : event.key === 'End' ? 'skills' : tab === 'mcp' ? 'skills' : 'mcp');
    await tick(); document.getElementById(`${uid}-${tab}-tab`)?.focus();
  }

  async function saveBot(field: string, value: unknown) {
    if (!native || operation) return;
    const id = profile.id;
    const previous = profile[field];
    profile[field] = value;
    operation = 'grant';
    try {
      if (!await onSave() && profile.id === id) profile[field] = previous;
    } catch (error) { if (profile.id === id) profile[field] = previous; notifyError(error); }
    finally { operation = ''; }
  }

  async function saveBindings() {
    if (!native || operation) return;
    operation = 'grant';
    const name = profile.name;
    try { if (await onSave()) onNotify(`Freigaben für ${name} gespeichert.`); }
    catch (error) { notifyError(error); }
    finally { operation = ''; }
  }

  async function loadTools(server: any) {
    if (!native || operation || busy) return;
    operation = 'tools';
    try {
      const tools = await call<any[]>('mcp_connect', { serverId: server.id });
      if (!Array.isArray(tools)) throw new Error('Der Server hat keinen gültigen Werkzeugkatalog geliefert.');
      if (!disposed) toolCatalogs = { ...toolCatalogs, [server.id]: tools.filter((tool) => typeof tool.name === 'string') };
      onNotify(`Werkzeuge von ${server.name || server.id} geladen. Botfreigaben wurden nicht verändert.`);
    } catch (error) { notifyError(error); }
    finally { operation = ''; }
  }

  async function disconnectServer(server: any) {
    if (!native || operation || busy) return;
    operation = 'disconnect';
    try {
      await call('mcp_disconnect', { serverId: server.id });
      const next = { ...toolCatalogs }; delete next[server.id]; toolCatalogs = next;
      onNotify('Die gemeinsame Serververbindung wurde getrennt. Botfreigaben bleiben gespeichert.');
    } catch (error) { notifyError(error); }
    finally { operation = ''; }
  }

  function editMcp(server?: any) {
    if (operation) return;
    ++revision;
    editorFocus = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    mcpDraft = {
      original: server || {}, id: server?.id || '', name: server?.name || '',
      transport: !server || server.transport === 'stdio' ? 'stdio' : 'http',
      command: server?.command || '', argsText: (server?.args || []).join('\n'), cwd: server?.cwd || '',
      url: server?.url || '', apiKeyId: server?.apiKeyId || '', tokenEnv: server?.tokenEnv || '',
    };
    editor = 'mcp';
    void focusEditor(revision);
  }

  function newSkill() {
    if (operation) return;
    ++revision;
    editorFocus = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    skillDraft = { id: '', existing: false, resources: [], content: '---\nname: mein-skill\ndescription: Beschreibe, wann dieser Skill verwendet wird.\n---\n\n# Mein Skill\n\nBeschreibe hier den Ablauf und die benötigten Werkzeuge.\n' };
    editor = 'skill';
    void focusEditor(revision);
  }

  async function editSkill(skill: any) {
    if (!native || operation) return;
    const request = ++revision;
    editorFocus = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    operation = 'read-skill';
    try {
      const content = await call<any>('get_library_skill', { id: skill.id });
      if (disposed || request !== revision) return;
      skillDraft = { id: skill.id, existing: true, content: content.content || '', resources: content.resources || [] };
      editor = 'skill';
      void focusEditor(request);
    } catch (error) { notifyError(error); }
    finally { operation = ''; }
  }

  async function saveEditor() {
    if (!native || operation || busy || !editor) return;
    const kind = editor;
    const request = revision;
    operation = 'save';
    try {
      if (kind === 'mcp') {
        if (!mcpDraft.name.trim()) throw new Error('Gib dem MCP-Server einen Namen.');
        if (mcpDraft.transport === 'stdio' && !mcpDraft.command.trim()) throw new Error('Gib den Startbefehl des Servers an.');
        if (mcpDraft.transport === 'http') {
          const url = new URL(mcpDraft.url.trim());
          if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password) throw new Error('Verwende eine HTTP(S)-URL ohne Zugangsdaten.');
        }
        if (mcpDraft.transport === 'stdio' && mcpDraft.apiKeyId.trim() && !mcpDraft.tokenEnv.trim()) throw new Error('Gib an, in welcher Umgebungsvariable der Server den Schlüssel erwartet.');
        const server = {
          ...mcpDraft.original, id: mcpDraft.id, name: mcpDraft.name.trim(), transport: mcpDraft.transport,
          command: mcpDraft.command.trim(), args: mcpDraft.argsText.split('\n').filter((line: string) => line.trim()),
          cwd: mcpDraft.cwd.trim(), url: mcpDraft.url.trim(), apiKeyId: mcpDraft.apiKeyId.trim(), tokenEnv: mcpDraft.tokenEnv.trim(),
        };
        delete server.enabled; delete server.allowedTools; delete server.viewerAllowedTools;
        const saved = await call<any>('save_library_mcp', { server });
        const next = { ...toolCatalogs }; delete next[saved.id]; toolCatalogs = next;
        await onRefresh(); serverId = saved.id;
        onNotify('MCP-Server in der gemeinsamen Bibliothek gespeichert. Die Botfreigaben bleiben separat.');
      } else {
        if (!/^[A-Za-z0-9_-]{1,96}$/.test(skillDraft.id)) throw new Error('Die Skill-ID benötigt 1–96 Buchstaben, Zahlen, - oder _.');
        if (!skillDraft.existing && skills.some((skill) => skill.id.toLowerCase() === skillDraft.id.toLowerCase())) throw new Error('Diese Skill-ID ist bereits installiert. Öffne den vorhandenen Skill zum Bearbeiten oder wähle eine andere ID.');
        await call('save_skill', { id: skillDraft.id, content: skillDraft.content });
        await onRefresh(); skillId = skillDraft.id;
        onNotify('Skill global gespeichert. Wähle separat, welche Bots ihn verwenden dürfen.');
      }
      if (!disposed && request === revision) closeEditor();
    } catch (error) { notifyError(error); }
    finally { operation = ''; }
  }

  async function importSkill() {
    if (!native || operation || busy) return;
    operation = 'import';
    try {
      const sourcePath = await open({ directory: true, multiple: false, title: 'Skillordner mit SKILL.md auswählen' });
      if (typeof sourcePath !== 'string' || disposed) return;
      const imported = await call<any>('import_skill', { sourcePath });
      await onRefresh(); skillId = imported.id || skillId;
      onNotify('Skillordner global importiert. Er ist dadurch bei keinem weiteren Bot aktiviert.');
    } catch (error) { notifyError(error); }
    finally { operation = ''; }
  }

  async function chooseExecutable() {
    const request = revision;
    try { const path = await open({ multiple: false, title: 'MCP-Startprogramm auswählen' }); if (typeof path === 'string' && !disposed && request === revision) mcpDraft.command = path; }
    catch (error) { notifyError(error); }
  }

  async function askDelete() {
    if (!selected || operation) return;
    previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    deletion = { kind: tab === 'mcp' ? 'mcp' : 'skill', id: selected.id, name: selected.name || selected.id };
    await tick(); cancelButton?.focus();
  }
  function closeDelete() { deletion = undefined; void tick().then(() => { if (previousFocus?.isConnected) previousFocus.focus(); else document.getElementById(`${uid}-${tab}-tab`)?.focus(); }); }
  async function deleteGlobal() {
    if (!deletion || operation || busy) return;
    const target = deletion;
    operation = 'delete';
    try {
      await call(target.kind === 'mcp' ? 'delete_library_mcp' : 'delete_skill', target.kind === 'mcp' ? { serverId: target.id } : { id: target.id });
      await onRefresh();
      const next = { ...toolCatalogs }; delete next[target.id]; toolCatalogs = next;
      closeDelete();
      onNotify(`${target.name} wurde global entfernt. Die Freigaben wurden bei allen Bots entzogen.`);
    } catch (error) { notifyError(error); }
    finally { operation = ''; }
  }

  function closeEditor() { ++revision; editor = undefined; void tick().then(() => { if (editorFocus?.isConnected) editorFocus.focus(); else document.getElementById(`${uid}-${tab}-tab`)?.focus(); }); }
  function closeOverlay() { if (deletion) closeDelete(); else if (editor) closeEditor(); }
  function keydown(event: KeyboardEvent) {
    if (event.key === 'Escape' && document.querySelector('[role="tooltip"]')) return;
    if (event.key === 'Escape' && (deletion || editor)) { event.preventDefault(); event.stopImmediatePropagation(); closeOverlay(); }
    if (event.key === 'Tab' && deletion && dialog) {
      const focusable = [...dialog.querySelectorAll<HTMLElement>('button:not(:disabled), [tabindex="0"]')];
      const first = focusable[0]; const last = focusable[focusable.length - 1];
      if (!first) { event.preventDefault(); dialog.focus(); }
      else if (!dialog.contains(document.activeElement)) { event.preventDefault(); (event.shiftKey ? last : first)?.focus(); }
      else if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
    }
  }
  onMount(() => {
    const save = () => { if (!deletion) { if (editor) void saveEditor(); else void saveBindings(); } };
    window.addEventListener('cohost:save-extension', save);
    window.addEventListener('cohost:close-extension', closeOverlay);
    window.addEventListener('keydown', keydown, { capture: true });
    return () => { disposed = true; ++revision; window.removeEventListener('cohost:save-extension', save); window.removeEventListener('cohost:close-extension', closeOverlay); window.removeEventListener('keydown', keydown, { capture: true }); };
  });
</script>

<div class="extensions-workspace">
  <ControlMcpSetup {busy} onNotify={onNotify} onInstalled={async(id)=>{await onRefresh();selectTab('mcp');serverId=id;}}/>
  {#if !native}<p class="desktop-note">Installationen und Freigaben bearbeitest du in der Desktop-App.</p>{/if}
  <div class="extension-columns">
    <section class="library-column" aria-label="Gemeinsame Erweiterungsbibliothek">
      <header class="scope-heading"><span class="scope-icon"><LibraryBig size={20}/></span><div><h2>Bibliothek für alle Bots</h2><p>Einmal installieren, gezielt freigeben.</p></div><HelpTip text="MCP-Server und Skills liegen gemeinsam in dieser Bibliothek. Das Hinzufügen installiert eine Erweiterung, gibt sie aber keinem Bot automatisch frei. Die Rechte legst du rechts für den ausgewählten Bot fest."/></header>
      <div class="library-tabs" role="tablist" aria-label="Erweiterungsart"><button id={`${uid}-mcp-tab`} role="tab" aria-selected={tab === 'mcp'} aria-controls={`${uid}-list`} tabindex={tab === 'mcp' ? 0 : -1} class:chosen={tab === 'mcp'} onclick={() => selectTab('mcp')} onkeydown={tabKey}><Cable size={15}/>MCP <span>{servers.length}</span></button><button id={`${uid}-skills-tab`} role="tab" aria-selected={tab === 'skills'} aria-controls={`${uid}-list`} tabindex={tab === 'skills' ? 0 : -1} class:chosen={tab === 'skills'} onclick={() => selectTab('skills')} onkeydown={tabKey}><BookOpen size={15}/>Skills <span>{skills.length}</span></button></div>
      <label class="library-search"><Search size={15}/><input aria-label="Erweiterungen suchen" bind:value={search} placeholder={tab === 'mcp' ? 'Server suchen …' : 'Skills suchen …'}/></label>
      <label class="library-filter"><span>Zeigen</span><select aria-label="Bibliothek nach Botfreigabe filtern" bind:value={filter}><option value="all">Alle Installationen</option><option value="enabled">Für {profile.name} freigegeben</option><option value="disabled">Nicht freigegeben</option></select></label>
      <div class="library-list" id={`${uid}-list`} role="tabpanel" aria-labelledby={`${uid}-${tab}-tab`}>
        {#each items as item (item.id)}
          {@const enabled = tab === 'mcp' ? mcpGrant(profile.mcpServers, item.id).enabled : (profile.skillsEnabled || []).includes(item.id)}
          <button class="library-entry" class:selected={selected?.id === item.id} onclick={() => selectItem(item.id)} aria-pressed={selected?.id === item.id}>
            <span class="entry-symbol">{#if tab === 'mcp'}<Cable size={19}/>{:else}<BookOpen size={19}/>{/if}</span>
            <span class="entry-copy"><strong>{item.name || item.id}</strong><small>{tab === 'mcp' ? item.transport === 'builtin' ? 'Steuerung dieser Software' : item.transport === 'stdio' ? 'Lokaler MCP-Prozess' : 'MCP über HTTP' : item.description || 'Wiederverwendbarer Ablauf'}</small><span class="entry-access" class:enabled>{#if enabled}<Check size={10}/>Für {profile.name} freigegeben{:else}Keine Botfreigabe{/if}</span></span>
            {#if selected?.id === item.id}<ArrowRight size={14}/>{/if}
          </button>
        {:else}<div class="empty-library"><Search size={23}/><strong>{search || filter !== 'all' ? 'Keine passenden Einträge' : 'Die Bibliothek ist noch leer.'}</strong><p>{search || filter !== 'all' ? 'Passe Suche oder Filter an.' : tab === 'mcp' ? 'Füge einen Server hinzu. Die Botrechte wählst du danach.' : 'Importiere einen Skillordner oder erstelle einen eigenen Ablauf.'}</p></div>{/each}
      </div>
      <footer class="library-actions">{#if tab === 'mcp'}<button class="ext-button library-primary" onclick={() => editMcp()} disabled={!native || !!operation || busy}><Plus size={15}/>MCP-Server hinzufügen</button>{:else}<button class="ext-button library-primary" onclick={importSkill} disabled={!native || !!operation || busy}><Upload size={15}/>Skillordner importieren</button><button class="ext-button quiet" onclick={newSkill} disabled={!native || !!operation || busy}><Plus size={14}/>Skill erstellen</button>{/if}<small>Installiert global. Aktiviert keine zusätzlichen Bots.</small></footer>
    </section>

    <section class="details-column" aria-label={`Erweiterungsdetails für ${profile.name}`}>
      {#if editor}
        <header class="scope-heading editor-heading"><span class="scope-icon"><Pencil size={19}/></span><div><h2>{editor === 'mcp' ? mcpDraft.id ? 'MCP global bearbeiten' : 'MCP global hinzufügen' : skillDraft.existing ? 'Skill global bearbeiten' : 'Skill global erstellen'}</h2><p>Änderungen gelten in der gemeinsamen Bibliothek.</p></div><button class="icon-control" onclick={closeEditor} aria-label="Editor schließen"><X size={18}/></button></header>
        <form class="extension-editor" id={`${uid}-editor`} aria-busy={operation === 'save'} onsubmit={(event) => { event.preventDefault(); void saveEditor(); }}><fieldset disabled={operation === 'save'}>
          <p class="scope-notice">{editor === 'mcp' ? 'Servereinstellungen werden von allen zugeordneten Bots gemeinsam verwendet. Speichern trennt eine bestehende Serververbindung; die Botrechte bleiben erhalten.' : 'Der Skillinhalt ist für alle Bots gleich, die diesen Skill verwenden. Persona und AGENTS.md bleiben eigene Dateien jedes Bots.'}</p>
          {#if editor === 'mcp'}
            <label>Name<input bind:value={mcpDraft.name} maxlength="160" placeholder="Zum Beispiel OBS oder ComfyUI" required/></label>
            <label>Verbindung<select bind:value={mcpDraft.transport}><option value="stdio">Lokaler Prozess · stdio</option><option value="http">Server-Adresse · HTTP</option></select></label>
            {#if mcpDraft.transport === 'stdio'}
              <label>Programm / Startbefehl<span class="editor-input-action"><input bind:value={mcpDraft.command} placeholder="npx, uvx oder absoluter Programmpfad" required/><button type="button" class="ext-button" onclick={chooseExecutable} disabled={!native}><FolderOpen size={14}/><span class="sr-only">Programm auswählen</span></button></span></label>
              <label>Argumente <span>Ein Argument pro Zeile</span><textarea rows="3" bind:value={mcpDraft.argsText} placeholder="-y&#10;dein-mcp-paket"></textarea></label>
              <label>Arbeitsordner <span>Optional</span><input bind:value={mcpDraft.cwd} placeholder="Absoluter Ordnerpfad"/></label>
            {:else}<label>MCP-Endpunkt<input type="url" bind:value={mcpDraft.url} placeholder="http://127.0.0.1:8188/mcp" required/></label>{/if}
            <label class="label-with-help"><span>Schlüsselbund-ID <HelpTip text="Hier steht nur der Name eines bereits hinterlegten Schlüssels, niemals dessen Wert. Bei HTTP verwendet die App ihn als Bearer-Token. Bei einem lokalen Prozess gibst du zusätzlich dessen Umgebungsvariablenname an. Ohne Zugangsdaten bleibt dieses Feld leer."/></span><input bind:value={mcpDraft.apiKeyId} placeholder="Optional: mcp-mein-server" autocomplete="off"/></label>
            {#if mcpDraft.transport === 'stdio' && mcpDraft.apiKeyId}<label>Schlüssel an Umgebungsvariable übergeben<input bind:value={mcpDraft.tokenEnv} placeholder="Zum Beispiel API_KEY"/></label>{/if}
            <p class="editor-hint">Lokale Server benötigen ihre installierte Laufzeit. Zugangsdaten bleiben im Betriebssystem-Schlüsselbund; hier werden nur Verweise bearbeitet.</p>
          {:else}
            <label>Skill-ID<input bind:value={skillDraft.id} readonly={skillDraft.existing} placeholder="stream-begruessung" required pattern={'[A-Za-z0-9_-]{1,96}'}/></label>
            <label>SKILL.md<textarea class="skill-source" rows="16" bind:value={skillDraft.content} spellcheck="false" aria-label="Globalen Skillinhalt bearbeiten"></textarea></label>
            {#if skillDraft.resources.length}<div class="skill-resources"><strong>Mitgelieferte Referenzen und Dateien</strong>{#each skillDraft.resources as resource}<span><FileText size={12}/>{typeof resource === 'string' ? resource : resource.path || resource.name || 'Ressource'}</span>{/each}</div>{/if}
            <p class="editor-hint">Mit einem Skillordner importierst du auch Referenzen und Skripte. Hier bearbeitest du dessen SKILL.md; vorhandene Begleitdateien bleiben erhalten.</p>
          {/if}
          <footer class="editor-actions"><button type="button" class="ext-button quiet" onclick={closeEditor}>Abbrechen</button><button type="submit" class="ext-button library-primary" disabled={!native || !!operation || busy}><Check size={14}/>{operation === 'save' ? 'Speichert …' : 'Global speichern'}</button></footer>
        </fieldset></form>
      {:else if selected}
        <header class="scope-heading"><span class="bot-marker">{profile.name?.charAt(0) || 'B'}</span><div><h2>Freigaben für {profile.name}</h2><p>Diese Einstellungen gelten nur für diesen Bot.</p></div><HelpTip text="Die Bibliothek wird geteilt, die Berechtigungen nicht. Ein Bot erhält nur ausdrücklich freigegebene Skills und Werkzeuge. Zuschauerchat kann nur die zusätzlich für ihn freigegebenen Werkzeuge anfordern. Die App prüft diese Rechte bei jeder Ausführung."/></header>
        <div class="selected-extension"><div class="selected-title"><span class="detail-symbol">{#if tab === 'mcp'}<Cable size={24}/>{:else}<BookOpen size={24}/>{/if}</span><div><h3>{selected.name || selected.id}</h3><p>{tab === 'mcp' ? selected.transport === 'stdio' ? 'Gemeinsam installierter MCP-Prozess' : 'Gemeinsam installierter MCP-Server' : selected.description || 'Gemeinsam installierter Skill'}</p></div><div class="global-actions"><button class="icon-control" onclick={() => tab === 'mcp' ? editMcp(selected) : editSkill(selected)} disabled={!native || !!operation || busy || tab==='mcp' && selected.transport==='builtin'} aria-label="Global bearbeiten" title="Global bearbeiten"><Pencil size={15}/></button><button class="icon-control danger" onclick={askDelete} disabled={!native || !!operation || busy} aria-label="Für alle Bots entfernen" title="Für alle Bots entfernen"><Trash2 size={15}/></button></div></div>
          <div class="access-switch"><div><strong>Für {profile.name} verwenden</strong><p>{tab === 'mcp' ? 'Danach einzelne Werkzeuge und Zuschauerrechte auswählen.' : 'Der Bot darf die Anweisungen dieses Skills bei Bedarf laden.'}</p></div><input type="checkbox" role="switch" aria-label={`${selected.name || selected.id} für ${profile.name} verwenden`} checked={tab === 'mcp' ? grant.enabled : skillActive} disabled={!native || !!operation} onchange={(event) => { if (tab === 'mcp') void saveBot('mcpServers', setMcpEnabled(profile.mcpServers, selected, event.currentTarget.checked)); else void saveBot('skillsEnabled', setSkillEnabled(profile.skillsEnabled, selected.id, event.currentTarget.checked)); }}/></div>
          {#if profile.model.toolsEnabled === false}<p class="scope-notice">Werkzeugaufrufe sind im Dialogmodell dieses Bots deaktiviert. Die hier gespeicherten Freigaben werden erst bei aktivierten Modellwerkzeugen verwendet.</p>{/if}
          {#if tab === 'mcp'}
            <div class="tools-heading"><div><h3>Werkzeuge</h3><p>{loadedTools ? `${loadedTools.length} Werkzeug${loadedTools.length === 1 ? '' : 'e'} geladen` : 'Lade die angebotenen Werkzeuge, bevor du Rechte vergibst.'}</p></div><button class="ext-button" onclick={() => loadTools(selected)} disabled={!native || !!operation || busy}><Plug size={14}/>{operation === 'tools' ? 'Lädt …' : 'Werkzeuge laden'}</button><HelpTip label="Werkzeugkatalog erklären" text="Diese Aktion verbindet den gemeinsam installierten MCP-Server und liest seinen Werkzeugkatalog. Sie führt keine Werkzeuge aus und aktiviert keinen Bot. Die Verbindung kann von mehreren Bots verwendet werden; eine Trennung betrifft daher alle Bots."/></div>
            {#if loadedTools?.length}<div class="permissions-table"><div class="permissions-header"><span>Werkzeug</span><span>Bot</span><span>Chat <HelpTip text="Nur aktivieren, wenn Zuschauer dieses Werkzeug über deinen Cohost anfordern dürfen. Chatfreigaben benötigen immer auch die Botfreigabe. Das Entfernen der Botfreigabe entzieht beides."/></span></div>{#each loadedTools as tool (tool.name)}<div class="permission-row"><div><strong>{tool.name}</strong><p>{tool.description || 'Keine Beschreibung vom Server vorhanden.'}</p></div><input type="checkbox" aria-label={`${tool.name} für ${profile.name} erlauben`} checked={grant.allowedTools.includes(tool.name)} disabled={!native || !!operation || !grant.enabled} onchange={(event) => saveBot('mcpServers', setMcpTool(profile.mcpServers, selected.id, tool.name, event.currentTarget.checked))}/><input type="checkbox" aria-label={`${tool.name} für Zuschaueranfragen an ${profile.name} erlauben`} checked={grant.viewerAllowedTools.includes(tool.name)} disabled={!native || !!operation || !grant.enabled || !grant.allowedTools.includes(tool.name)} onchange={(event) => saveBot('mcpServers', setMcpTool(profile.mcpServers, selected.id, tool.name, event.currentTarget.checked, true))}/></div>{/each}</div>{:else if loadedTools}<div class="tools-empty"><Cable size={24}/><p>Der Server stellt derzeit keine Werkzeuge bereit.</p></div>{:else}<div class="tools-empty"><ShieldCheck size={28}/><p>{grant.allowedTools.length ? `${grant.allowedTools.length} Werkzeugfreigaben gespeichert` : 'Installiert bedeutet noch nicht freigegeben.'}</p><small>{grant.allowedTools.length ? 'Die gespeicherten Rechte bleiben erhalten. Lade den Werkzeugkatalog, um einzelne Freigaben zu prüfen und anzupassen.' : 'Auch bei aktiviertem Server bleiben alle Werkzeuge gesperrt, bis du sie hier auswählst.'}</small>{#if grant.allowedTools.length}<button class="ext-button quiet" disabled={!native || !!operation} onclick={() => saveBot('mcpServers', grant.allowedTools.reduce((bindings, tool) => setMcpTool(bindings, selected.id, tool, false), profile.mcpServers))}>Alle Werkzeugrechte entziehen</button>{/if}</div>{/if}
            <div class="connection-footer"><p><Users size={14}/>Hostrechte gelten pro Bot. Ob Daten und Sitzungen getrennt bleiben, hängt zusätzlich vom verbundenen Server ab.</p>{#if loadedTools}<button class="ext-button quiet" onclick={() => disconnectServer(selected)} disabled={!native || !!operation || busy}><Unplug size={13}/>Gemeinsame Verbindung trennen</button>{/if}</div>
          {:else}
            <div class="skill-reading"><BookOpen size={30}/><h3>Ein Ablauf. Für deine Bots verfügbar.</h3><p>Der vollständige Skill und seine Referenzen werden bei Bedarf geladen. Eine Installation verändert weder die Persönlichkeit noch die AGENTS.md eines Bots.</p><button class="ext-button" onclick={() => editSkill(selected)} disabled={!native || !!operation}><FileText size={14}/>{operation === 'read-skill' ? 'Lädt …' : 'Inhalt öffnen'}</button></div>
            <div class="script-access"><div><strong>Skill-Skripte für {profile.name} erlauben</strong><p>Gilt für alle freigegebenen Skills dieses Bots. Nur ausdrückliche Streamer-Aufträge können Skripte starten; Zuschauer erhalten diese Fähigkeit nicht.</p></div><input type="checkbox" role="switch" aria-label={`Skill-Skripte für ${profile.name} erlauben`} checked={profile.allowSkillScripts === true} disabled={!native || !!operation} onchange={(event) => saveBot('allowSkillScripts', event.currentTarget.checked)}/><HelpTip text="Ein Skill kann lokale Python- oder JavaScript-Skripte mitbringen. Mit dieser gesonderten Freigabe dürfen sie bei expliziten Streamer-Aufträgen ausgeführt werden. Aktiviere sie nur für Skills, deren Skripte du geprüft hast. Diese Freigabe ist unabhängig von der globalen Installation."/></div>
          {/if}
          <p class="saved-note" role="status">{operation === 'grant' ? 'Botfreigabe wird gespeichert …' : 'Botfreigaben werden unmittelbar gespeichert.'}</p>
        </div>
      {:else}<div class="no-selection"><span class="scope-icon"><LibraryBig size={30}/></span><h2>Deine Bibliothek, deine Freigaben.</h2><p>Wähle links einen MCP-Server oder Skill. Rechts entscheidest du, was {profile.name} und sein Zuschauerchat verwenden dürfen.</p><p class="no-selection-note">Persona und AGENTS.md bleiben unter „Persönlichkeit &amp; Regeln“ pro Bot anpassbar.</p></div>{/if}
    </section>
  </div>
</div>

{#if deletion}<div class="extension-modal-backdrop" role="presentation"><div class="extension-delete-dialog" role="alertdialog" aria-modal="true" aria-labelledby={`${uid}-delete-title`} aria-describedby={`${uid}-delete-description`} tabindex="-1" bind:this={dialog}><span class="delete-symbol"><Trash2 size={24}/></span><h2 id={`${uid}-delete-title`}>Für alle Bots entfernen?</h2><p id={`${uid}-delete-description`}>„{deletion.name}“ wird aus der gemeinsamen Bibliothek gelöscht. Alle Bots verlieren ihre Freigaben für diese Erweiterung.{deletion.kind === 'skill' ? ' Der Skillordner und seine Begleitdateien werden entfernt.' : ' Eine bestehende Serververbindung wird getrennt.'}</p><p class="delete-alternative">Wenn nur {profile.name} die Erweiterung nicht mehr verwenden soll, brich ab und schalte seine Botfreigabe aus.</p><div class="delete-actions"><button class="ext-button" bind:this={cancelButton} onclick={closeDelete} disabled={operation === 'delete'}>Abbrechen</button><button class="ext-button delete-confirm" onclick={deleteGlobal} disabled={!!operation || busy}>{operation === 'delete' ? 'Entfernt …' : 'Für alle Bots entfernen'}</button></div></div></div>{/if}

<style>
  .extensions-workspace { --ext-line: #344054; --ext-panel: #1b2330; --ext-soft: #222e40; --ext-muted: #a4b1c6; color: #e0e6f0; }
  .extension-columns { display: grid; grid-template-columns: minmax(260px, 0.72fr) minmax(370px, 1.28fr); align-items: start; gap: 22px; }
  .library-column, .details-column { border: 1px solid var(--ext-line); border-radius: 11px; background: var(--ext-panel); min-width: 0; }
  .scope-heading { display: flex; gap: 11px; align-items: center; padding: 22px 20px; border-bottom: 1px solid var(--ext-line); }
  .scope-heading > div { flex: 1; min-width: 0; }
  .scope-heading h2 { font-size: 16px; line-height: 1.4; }
  .scope-heading p { font-size: 10px; line-height: 1.6; color: var(--ext-muted); margin-top: 4px; }
  .scope-icon, .bot-marker { display: grid; place-items: center; width: 36px; height: 36px; flex-shrink: 0; border-radius: 10px; background: #2c3650; color: #c1ccf5; }
  .bot-marker { background: #3c3638; color: #edcdb6; font-family: 'Space Grotesk', sans-serif; font-size: 18px; }
  .library-tabs { display: flex; gap: 7px; padding: 17px 17px 0; }
  .library-tabs button { flex: 1; display: flex; gap: 7px; align-items: center; justify-content: center; padding: 10px; border: 1px solid transparent; border-radius: 7px; background: transparent; color: #a4b1c7; font-size: 11px; }
  .library-tabs button.chosen { background: #303a56; border-color: #4b5878; color: #d4dcfa; }
  .library-tabs button > span { font-size: 10px; opacity: 0.75; margin-left: 4px; }
  .library-search { display: flex; align-items: center; gap: 9px; margin: 17px 17px 11px; padding: 0 11px; border: 1px solid #3a4861; border-radius: 7px; color: #8d9fba; background: #131d2b; }
  .library-search input { min-width: 0; margin: 0; padding: 11px 0; border: 0; background: transparent; color: #d9e2ef; font-size: 11px; }
  .library-search:focus-within { outline: 2px solid #b7c5ff; outline-offset: 3px; }
  .library-search input:focus-visible { outline: none; }
  .library-filter { display: flex; align-items: center; gap: 10px; margin: 0 17px 13px; color: #a4b1c4; font-size: 10px; }
  .library-filter select { margin: 0; padding: 6px 7px; font-size: 10px; min-width: 0; flex: 1; border-color: #35435a; background: #1b2739; }
  .library-list { padding: 0 9px 9px; min-height: 230px; max-height: 490px; overflow: auto; }
  .library-entry { width: 100%; display: flex; align-items: center; gap: 11px; padding: 14px 11px; border: 1px solid transparent; border-radius: 7px; background: transparent; color: #aebed8; text-align: left; margin: 3px 0; }
  .library-entry:hover { background: #243047; }
  .library-entry.selected { border-color: #627399; background: #29364f; color: #d3def7; }
  .entry-symbol { flex-shrink: 0; color: #a6b6da; }
  .entry-copy { flex: 1; min-width: 0; }
  .entry-copy strong { display: block; color: #d6dfed; font-size: 12px; overflow-wrap: anywhere; }
  .entry-copy small { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; color: #97a9c6; font-size: 10px; margin-top: 5px; line-height: 1.65; }
  .entry-access { display: flex; gap: 4px; align-items: center; font-size: 9px; color: #8496b5; margin-top: 7px; }
  .entry-access.enabled { color: #a9d3c2; }
  .library-actions { display: flex; flex-direction: column; gap: 9px; padding: 17px; border-top: 1px solid var(--ext-line); }
  .library-actions small { font-size: 9px; line-height: 1.7; color: #96a7c0; text-align: center; }
  .ext-button { display: inline-flex; align-items: center; justify-content: center; gap: 7px; min-height: 35px; padding: 8px 11px; border: 1px solid #43536f; border-radius: 7px; background: #29374e; color: #d0dcef; font-size: 10px; font-weight: 650; }
  .ext-button:hover { background: #374767; }
  .ext-button.library-primary { background: #b2bfee; border-color: #b2bfee; color: #202b42; }
  .ext-button.library-primary:hover { background: #c7d1f7; }
  .ext-button.quiet { background: transparent; border-color: #394861; color: #adbed8; }
  .ext-button:disabled, .icon-control:disabled { opacity: 0.45; cursor: not-allowed; }
  .empty-library, .no-selection { display: flex; flex-direction: column; align-items: center; justify-content: center; text-align: center; gap: 15px; padding: 45px 23px; color: #95a8c7; }
  .empty-library strong { color: #c6d3e8; font-size: 12px; }
  .empty-library p { font-size: 10px; max-width: 240px; }
  .no-selection { min-height: 510px; }
  .no-selection .scope-icon { width: 57px; height: 57px; border-radius: 16px; }
  .no-selection h2 { font-size: 21px; color: #d6e0f0; }
  .no-selection p { font-size: 12px; max-width: 350px; }
  .no-selection .no-selection-note { margin-top: 20px; font-size: 10px; color: #8b9fbe; }
  .selected-extension { padding: 23px; }
  .selected-title { display: flex; align-items: flex-start; gap: 13px; }
  .selected-title > div:nth-child(2) { flex: 1; min-width: 0; }
  .selected-title h3 { font-size: 21px; font-weight: 500; line-height: 1.35; color: #e0e7f4; overflow-wrap: anywhere; }
  .selected-title p { font-size: 10px; color: #a1b3ce; margin-top: 6px; }
  .detail-symbol { color: #b2c2e7; padding-top: 2px; }
  .global-actions { display: flex; gap: 3px; }
  .icon-control { display: inline-grid; place-items: center; width: 31px; height: 31px; padding: 5px; background: transparent; border: 1px solid transparent; border-radius: 6px; color: #acbbd3; }
  .icon-control:hover { background: #32415b; border-color: #4d607e; }
  .icon-control.danger { color: #d1a7ae; }
  .access-switch { display: flex; align-items: center; gap: 20px; margin: 24px 0; padding: 18px; border: 1px solid #596783; border-radius: 8px; background: #29364b; }
  .access-switch > div { flex: 1; }
  .access-switch strong { display: block; font-size: 12px; color: #dce5f6; }
  .access-switch p { font-size: 10px; color: #a8bad6; margin-top: 4px; }
  .tools-heading { display: flex; align-items: center; gap: 10px; margin: 25px 0 15px; }
  .tools-heading > div { flex: 1; min-width: 0; }
  .tools-heading h3 { font-size: 14px; }
  .tools-heading p { font-size: 10px; color: #98accb; margin-top: 5px; }
  .permissions-table { margin-top: 13px; border-top: 1px solid #3b4e6c; }
  .permissions-header, .permission-row { display: grid; grid-template-columns: minmax(130px, 1fr) 42px 51px; align-items: center; column-gap: 12px; padding: 13px 3px; border-bottom: 1px solid #33445e; }
  .permissions-header { font-size: 10px; color: #a9bbd5; }
  .permissions-header > span:last-child { display: flex; align-items: center; gap: 1px; }
  .permission-row strong { display: block; font-size: 11px; color: #d2ddf2; overflow-wrap: anywhere; }
  .permission-row p { margin-top: 5px; font-size: 10px; line-height: 1.7; color: #95aaca; overflow-wrap: anywhere; }
  .permission-row input { justify-self: center; }
  .tools-empty { border: 1px dashed #425472; border-radius: 8px; padding: 30px 20px; display: flex; flex-direction: column; align-items: center; gap: 11px; color: #a0b5da; text-align: center; }
  .tools-empty p { font-size: 12px; }
  .tools-empty small { max-width: 310px; font-size: 10px; line-height: 1.8; color: #8ea5c8; }
  .connection-footer { margin-top: 21px; display: flex; flex-direction: column; align-items: flex-start; gap: 14px; }
  .connection-footer p { display: flex; align-items: flex-start; gap: 8px; color: #96acca; font-size: 10px; line-height: 1.8; }
  .connection-footer p :global(svg) { flex-shrink: 0; margin-top: 3px; }
  .saved-note { font-size: 9px; color: #95b4ad; padding-top: 22px; }
  .scope-notice { padding: 12px 14px; border-left: 2px solid #8497c1; background: #243249; color: #aebfdc; font-size: 10px; line-height: 1.8; margin-bottom: 23px; }
  .skill-reading { display: flex; flex-direction: column; align-items: flex-start; gap: 13px; margin: 27px 0; color: #adc0e4; }
  .skill-reading h3 { color: #d3dded; font-size: 17px; }
  .skill-reading p { max-width: 420px; font-size: 11px; color: #a0b3d0; }
  .script-access { display: flex; gap: 13px; align-items: center; padding-top: 24px; border-top: 1px solid #384a66; }
  .script-access > div { flex: 1; }
  .script-access strong { font-size: 11px; color: #d3dff1; }
  .script-access p { margin-top: 6px; color: #96acca; font-size: 10px; line-height: 1.8; }
  .extension-editor { padding: 23px; }
  .extension-editor fieldset { border: 0; margin: 0; padding: 0; min-width: 0; }
  .editor-heading { background: #252c41; }
  .extension-editor label { display: block; font-size: 11px; color: #c9d7ed; margin: 18px 0; font-weight: 600; }
  .extension-editor label > span:not(.editor-input-action) { color: #9fb0cc; font-size: 10px; font-weight: 450; }
  .extension-editor .label-with-help > span { display: inline-flex; gap: 4px; align-items: center; color: #c9d7ed; font-size: 11px; font-weight: 600; }
  .extension-editor input, .extension-editor select, .extension-editor textarea { width: 100%; background: #142033; border: 1px solid #425573; color: #d8e5fa; padding: 11px 12px; border-radius: 7px; margin-top: 8px; font-size: 11px; }
  .extension-editor textarea { resize: vertical; line-height: 1.8; }
  .extension-editor .skill-source { font-family: ui-monospace, SFMono-Regular, Consolas, monospace; min-height: 330px; }
  .extension-editor input:read-only { color: #9bb0d0; background: #1b2940; }
  .editor-input-action { display: flex; align-items: stretch; gap: 7px; margin-top: 8px; }
  .editor-input-action input { min-width: 0; margin: 0; }
  .editor-hint { font-size: 10px; line-height: 1.8; color: #9db3d3; }
  .editor-actions { display: flex; justify-content: flex-end; gap: 10px; margin-top: 25px; padding-top: 20px; border-top: 1px solid #3a4b68; }
  .skill-resources { display: flex; flex-direction: column; gap: 9px; font-size: 10px; padding: 14px 0; color: #9eb2d1; }
  .skill-resources > span { display: flex; align-items: center; gap: 7px; overflow-wrap: anywhere; }
  .desktop-note { font-size: 11px; color: #e0c5ae; margin-bottom: 17px; }
  .extension-modal-backdrop { position: fixed; inset: 0; z-index: 40; display: grid; place-items: center; background: #050c18bf; backdrop-filter: blur(5px); padding: 25px; }
  .extension-delete-dialog { max-width: 480px; width: 100%; padding: 28px; background: #233147; border: 1px solid #677995; border-radius: 13px; box-shadow: 0 20px 80px #050b1880; color: #dbe6f6; }
  .delete-symbol { display: inline-grid; place-items: center; width: 48px; height: 48px; background: #4f3742; color: #e4b5bd; border-radius: 13px; margin-bottom: 20px; }
  .extension-delete-dialog h2 { font-size: 24px; }
  .extension-delete-dialog p { margin-top: 15px; font-size: 12px; color: #afc1dc; line-height: 1.9; }
  .extension-delete-dialog .delete-alternative { font-size: 11px; color: #cdbbc9; padding: 12px 14px; border: 1px solid #5a526c; border-radius: 7px; }
  .delete-actions { display: flex; gap: 10px; justify-content: flex-end; padding-top: 25px; }
  .delete-confirm { background: #d3a3ad; border-color: #d3a3ad; color: #352331; }
  .delete-confirm:hover { background: #e5bdc5; }
  .sr-only { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0; }
  @media (max-width: 1150px) { .extension-columns { grid-template-columns: minmax(225px, 0.75fr) minmax(320px, 1.25fr); gap: 15px; } .scope-heading { padding: 18px 15px; gap: 9px; } .scope-heading h2 { font-size: 14px; } .scope-icon, .bot-marker { width: 30px; height: 30px; } .selected-extension, .extension-editor { padding: 18px; } .selected-title { gap: 9px; flex-wrap: wrap; } .selected-title h3 { font-size: 19px; } .tools-heading { flex-wrap: wrap; } .tools-heading > div { flex-basis: calc(100% - 35px); } }
  @media (max-width: 760px) { .extension-columns { grid-template-columns: 1fr; } .library-list { min-height: 0; max-height: 270px; } .no-selection { min-height: 300px; } .delete-actions { flex-wrap: wrap; } }
  @media (prefers-reduced-motion: reduce) { button { transition: none; } }
</style>
