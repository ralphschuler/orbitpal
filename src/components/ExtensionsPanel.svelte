<script lang="ts">
  import { t, translate, locale, formatNumber } from '../lib/i18n';
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
    beforeSave?: () => Promise<boolean>;
    onNotify: (message: string, error?: boolean) => void;
    onEditingChange?: (editing: boolean) => void;
    onDirtyChange?: (dirty: boolean) => void;
    onBusyChange?: (busy: boolean) => void;
    confirmBeforeDiscard?: (continueAction: () => void) => void;
  }
  let { profile = $bindable(), library, native, busy, onSave, beforeSave, onRefresh, onNotify, onEditingChange, onDirtyChange, onBusyChange, confirmBeforeDiscard }: Props = $props();
  const uid = $props.id();
  let tab = $state<'mcp' | 'skills'>('mcp');
  let search = $state('');
  let filter = $state('all');
  let serverId = $state('');
  let skillId = $state('');
  let editor = $state<'mcp' | 'skill' | undefined>();
  let mcpDraft = $state<any>({});
  let skillDraft = $state({ id: '', content: '', existing: false, resources: [] as any[] });
  let editorBaseline = $state('');
  function draftSnapshot() {
    return editor === 'mcp'
      ? JSON.stringify([mcpDraft.id, mcpDraft.name, mcpDraft.transport, mcpDraft.command, mcpDraft.argsText, mcpDraft.cwd, mcpDraft.url, mcpDraft.apiKeyId, mcpDraft.tokenEnv])
      : editor === 'skill' ? JSON.stringify([skillDraft.id, skillDraft.content]) : '';
  }
  const editorDirty = $derived(!!editor && draftSnapshot() !== editorBaseline);
  let operation = $state('');
  let toolCatalogs = $state<Record<string, any[]>>({});
  let deletion = $state<{ kind: 'mcp' | 'skill'; id: string; name: string }>();
  let dialog = $state<HTMLDivElement>();
  let cancelButton = $state<HTMLButtonElement>();
  let previousFocus: HTMLElement | undefined;
  let editorFocus: HTMLElement | undefined;
  let revision = 0;
  let disposed = false;
  $effect(() => { onEditingChange?.(!!editor || !!operation || !!deletion); });
  $effect(() => { onDirtyChange?.(editorDirty); });
  $effect(() => { onBusyChange?.(!!operation); });
  const servers = $derived(library?.mcpServers || []);
  const skills = $derived(library?.skills || []);
  const selected = $derived(tab === 'mcp' ? servers.find((item) => item.id === serverId) : skills.find((item) => item.id === skillId));
  const grant = $derived(mcpGrant(profile.mcpServers, serverId));
  const skillActive = $derived((profile.skillsEnabled || []).includes(skillId));
  const loadedTools = $derived(toolCatalogs[serverId]);
  const items = $derived((tab === 'mcp' ? servers : skills).filter((item) => {
    const enabled = tab === 'mcp' ? mcpGrant(profile.mcpServers, item.id).enabled : (profile.skillsEnabled || []).includes(item.id);
    return `${item.name || item.id} ${item.description || ''} ${item.id}`.toLocaleLowerCase($locale).includes(search.trim().toLocaleLowerCase($locale))
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
  function beforeDiscard(action: () => void) {
    if (operation) return;
    if (!editorDirty) { action(); return; }
    const request = revision;
    const snapshot = draftSnapshot();
    confirmBeforeDiscard?.(() => {
      if (!disposed && !operation && request === revision && snapshot === draftSnapshot()) action();
    });
  }
  function clearEditor() { editor = undefined; editorBaseline = ''; onDirtyChange?.(false); }
  function selectItem(id: string) { beforeDiscard(() => { ++revision; clearEditor(); if (tab === 'mcp') serverId = id; else skillId = id; }); }
  function selectTab(value: 'mcp' | 'skills', focus = false) { beforeDiscard(() => { ++revision; tab = value; clearEditor(); search = ''; if (focus) void tick().then(() => document.getElementById(`${uid}-${tab}-tab`)?.focus()); }); }
  function tabKey(event: KeyboardEvent) {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    selectTab(event.key === 'Home' ? 'mcp' : event.key === 'End' ? 'skills' : tab === 'mcp' ? 'skills' : 'mcp', true);
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
    try { if (await onSave()) onNotify(translate('Freigaben für {name} gespeichert.', { name })); }
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
      onNotify(translate('Werkzeuge von {name} geladen. Botfreigaben wurden nicht verändert.', { name: server.name || server.id }));
    } catch (error) { notifyError(error); }
    finally { operation = ''; }
  }

  async function disconnectServer(server: any) {
    if (!native || operation || busy) return;
    operation = 'disconnect';
    try {
      await call('mcp_disconnect', { serverId: server.id });
      const next = { ...toolCatalogs }; delete next[server.id]; toolCatalogs = next;
      onNotify(translate("Die gemeinsame Serververbindung wurde getrennt. Botfreigaben bleiben gespeichert."));
    } catch (error) { notifyError(error); }
    finally { operation = ''; }
  }

  function editMcp(server?: any) {
    if (operation) return;
    beforeDiscard(() => {
      ++revision;
      editorFocus = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
      mcpDraft = {
        original: server || {}, id: server?.id || '', name: server?.name || '',
        transport: !server || server.transport === 'stdio' ? 'stdio' : 'http',
        command: server?.command || '', argsText: (server?.args || []).join('\n'), cwd: server?.cwd || '',
        url: server?.url || '', apiKeyId: server?.apiKeyId || '', tokenEnv: server?.tokenEnv || '',
      };
      editor = 'mcp'; editorBaseline = draftSnapshot();
      void focusEditor(revision);
    });
  }

  function newSkill() {
    if (operation) return;
    beforeDiscard(() => {
      ++revision;
      editorFocus = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
      skillDraft = { id: '', existing: false, resources: [], content: translate("---\nname: mein-skill\ndescription: Beschreibe, wann dieser Skill verwendet wird.\n---\n\n# Mein Skill\n\nBeschreibe hier den Ablauf und die benötigten Werkzeuge.\n") };
      editor = 'skill'; editorBaseline = draftSnapshot();
      void focusEditor(revision);
    });
  }

  function editSkill(skill: any) {
    if (!native || operation) return;
    beforeDiscard(() => { void openSkillEditor(skill); });
  }
  async function openSkillEditor(skill: any) {
    if (!native || operation) return;
    const request = ++revision;
    editorFocus = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    operation = 'read-skill';
    try {
      const content = await call<any>('get_library_skill', { id: skill.id });
      if (disposed || request !== revision) return;
      skillDraft = { id: skill.id, existing: true, content: content.content || '', resources: content.resources || [] };
      editor = 'skill'; editorBaseline = draftSnapshot();
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
      if (beforeSave && !await beforeSave()) return;
      if (disposed || request !== revision || editor !== kind) return;
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
        onNotify(translate("MCP-Server in der gemeinsamen Bibliothek gespeichert. Die Botfreigaben bleiben separat."));
      } else {
        if (!/^[A-Za-z0-9_-]{1,96}$/.test(skillDraft.id)) throw new Error('Die Skill-ID benötigt 1–96 Buchstaben, Zahlen, - oder _.');
        if (!skillDraft.existing && skills.some((skill) => skill.id.toLowerCase() === skillDraft.id.toLowerCase())) throw new Error('Diese Skill-ID ist bereits installiert. Öffne den vorhandenen Skill zum Bearbeiten oder wähle eine andere ID.');
        await call('save_skill', { id: skillDraft.id, content: skillDraft.content });
        await onRefresh(); skillId = skillDraft.id;
        onNotify(translate("Skill global gespeichert. Wähle separat, welche Bots ihn verwenden dürfen."));
      }
      if (!disposed && request === revision) finishEditor();
    } catch (error) { notifyError(error); }
    finally { operation = ''; }
  }

  async function importSkill() {
    if (!native || operation || busy) return;
    operation = 'import';
    try {
      const sourcePath = await open({ directory: true, multiple: false, title: translate("Skillordner mit SKILL.md auswählen") });
      if (typeof sourcePath !== 'string' || disposed) return;
      const imported = await call<any>('import_skill', { sourcePath });
      await onRefresh(); skillId = imported.id || skillId;
      onNotify(translate("Skillordner global importiert. Er ist dadurch bei keinem weiteren Bot aktiviert."));
    } catch (error) { notifyError(error); }
    finally { operation = ''; }
  }

  async function chooseExecutable() {
    const request = revision;
    try { const path = await open({ multiple: false, title: translate("MCP-Startprogramm auswählen") }); if (typeof path === 'string' && !disposed && request === revision) mcpDraft.command = path; }
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
      onNotify(translate('{name} wurde global entfernt. Die Freigaben wurden bei allen Bots entzogen.', { name: target.name }));
    } catch (error) { notifyError(error); }
    finally { operation = ''; }
  }

  function finishEditor() { ++revision; clearEditor(); void tick().then(() => { if (editorFocus?.isConnected) editorFocus.focus(); else document.getElementById(`${uid}-${tab}-tab`)?.focus(); }); }
  function closeEditor() { beforeDiscard(finishEditor); }
  function closeOverlay() { if (deletion) closeDelete(); else if (editor) closeEditor(); }
  function keydown(event: KeyboardEvent) {
    if (!deletion && document.querySelector('[role="dialog"],[role="alertdialog"]')) return;
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
    return () => { disposed = true; ++revision; onEditingChange?.(false); onDirtyChange?.(false); onBusyChange?.(false); window.removeEventListener('cohost:save-extension', save); window.removeEventListener('cohost:close-extension', closeOverlay); window.removeEventListener('keydown', keydown, { capture: true }); };
  });
</script>

<div class="extensions-workspace">
  <ControlMcpSetup {busy} onNotify={onNotify} onInstalled={async(id)=>{await onRefresh();selectTab('mcp');serverId=id;}}/>
  {#if !native}<p class="desktop-note">{$t("Installationen und Freigaben bearbeitest du in der Desktop-App.")}</p>{/if}
  <div class="extension-columns">
    <section class="library-column" aria-label={$t("Gemeinsame Erweiterungsbibliothek")}>
      <header class="scope-heading"><span class="scope-icon"><LibraryBig size={20}/></span><div><h2>{$t("Bibliothek für alle Bots")}</h2><p>{$t("Einmal installieren, gezielt freigeben.")}</p></div><HelpTip label={$t("Hilfe zur gemeinsamen Bibliothek")} text={$t("MCP-Server und Skills liegen gemeinsam in dieser Bibliothek. Das Hinzufügen installiert eine Erweiterung, gibt sie aber keinem Bot automatisch frei. Die Rechte legst du rechts für den ausgewählten Bot fest.")}/></header>
      <div class="library-tabs" role="tablist" aria-label={$t("Erweiterungsart")}><button id={`${uid}-mcp-tab`} role="tab" aria-selected={tab === 'mcp'} aria-controls={`${uid}-list`} tabindex={tab === 'mcp' ? 0 : -1} disabled={!!operation} class:chosen={tab === 'mcp'} onclick={() => selectTab('mcp')} onkeydown={tabKey}><Cable size={15}/>MCP <span>{formatNumber(servers.length, $locale)}</span></button><button id={`${uid}-skills-tab`} role="tab" aria-selected={tab === 'skills'} aria-controls={`${uid}-list`} tabindex={tab === 'skills' ? 0 : -1} disabled={!!operation} class:chosen={tab === 'skills'} onclick={() => selectTab('skills')} onkeydown={tabKey}><BookOpen size={15}/>Skills <span>{formatNumber(skills.length, $locale)}</span></button></div>
      <label class="library-search"><Search size={15}/><input aria-label={$t("Erweiterungen suchen")} bind:value={search} placeholder={tab === 'mcp' ? $t("Server suchen …") : $t("Skills suchen …")}/></label>
      <label class="library-filter"><span>{$t("Zeigen")}</span><select aria-label={$t("Bibliothek nach Botfreigabe filtern")} bind:value={filter}><option value="all">{$t("Alle Installationen")}</option><option value="enabled">{$t('Für {name} freigegeben', { name: profile.name })}</option><option value="disabled">{$t("Nicht freigegeben")}</option></select></label>
      <div class="library-list" id={`${uid}-list`} role="tabpanel" aria-labelledby={`${uid}-${tab}-tab`}>
        {#each items as item (item.id)}
          {@const enabled = tab === 'mcp' ? mcpGrant(profile.mcpServers, item.id).enabled : (profile.skillsEnabled || []).includes(item.id)}
          <button class="library-entry" class:selected={selected?.id === item.id} onclick={() => selectItem(item.id)} disabled={!!operation} aria-pressed={selected?.id === item.id}>
            <span class="entry-symbol">{#if tab === 'mcp'}<Cable size={19}/>{:else}<BookOpen size={19}/>{/if}</span>
            <span class="entry-copy"><strong>{item.name || item.id}</strong><small>{tab === 'mcp' ? item.transport === 'builtin' ? $t("Steuerung dieser Software") : item.transport === 'stdio' ? $t("Lokaler MCP-Prozess") : $t("MCP über HTTP") : item.description || $t("Wiederverwendbarer Ablauf")}</small><span class="entry-access" class:enabled>{#if enabled}<Check size={10}/>{$t('Für {name} freigegeben', { name: profile.name })}{:else}{$t("Keine Botfreigabe")}{/if}</span></span>
            {#if selected?.id === item.id}<ArrowRight size={14}/>{/if}
          </button>
        {:else}<div class="empty-library"><Search size={23}/><strong>{search || filter !== 'all' ? $t("Keine passenden Einträge") : $t("Die Bibliothek ist noch leer.")}</strong><p>{search || filter !== 'all' ? $t("Passe Suche oder Filter an.") : tab === 'mcp' ? $t("Füge einen Server hinzu. Die Botrechte wählst du danach.") : $t("Importiere einen Skillordner oder erstelle einen eigenen Ablauf.")}</p></div>{/each}
      </div>
      <footer class="library-actions">{#if tab === 'mcp'}<button class="ext-button library-primary" onclick={() => editMcp()} disabled={!native || !!operation || busy}><Plus size={15}/>{$t("MCP-Server hinzufügen")}</button>{:else}<button class="ext-button library-primary" onclick={importSkill} disabled={!native || !!operation || busy}><Upload size={15}/>{$t("Skillordner importieren")}</button><button class="ext-button quiet" onclick={newSkill} disabled={!native || !!operation || busy}><Plus size={14}/>{$t("Skill erstellen")}</button>{/if}<small>{$t("Installiert global. Aktiviert keine zusätzlichen Bots.")}</small></footer>
    </section>

    <section class="details-column" aria-label={$t('Erweiterungsdetails für {name}', { name: profile.name })}>
      {#if editor}
        <header class="scope-heading editor-heading"><span class="scope-icon"><Pencil size={19}/></span><div><h2>{editor === 'mcp' ? mcpDraft.id ? $t("MCP global bearbeiten") : $t("MCP global hinzufügen") : skillDraft.existing ? $t("Skill global bearbeiten") : $t("Skill global erstellen")}</h2><p>{$t("Änderungen gelten in der gemeinsamen Bibliothek.")}</p>{#if editorDirty}<small class="draft-status" role="status">{$t("Ungespeicherter Entwurf")}</small>{/if}</div><button class="icon-control" onclick={closeEditor} disabled={!!operation} aria-label={$t("Editor schließen")}><X size={18}/></button></header>
        <form class="extension-editor" id={`${uid}-editor`} aria-busy={operation === 'save'} onsubmit={(event) => { event.preventDefault(); void saveEditor(); }}><fieldset disabled={operation === 'save'}>
          <p class="scope-notice">{editor === 'mcp' ? $t("Servereinstellungen werden von allen zugeordneten Bots gemeinsam verwendet. Speichern trennt eine bestehende Serververbindung; die Botrechte bleiben erhalten.") : $t("Der Skillinhalt ist für alle Bots gleich, die diesen Skill verwenden. Persona und AGENTS.md bleiben eigene Dateien jedes Bots.")}</p>
          {#if editor === 'mcp'}
            <label>{$t("Name")}<input bind:value={mcpDraft.name} maxlength="160" placeholder={$t("Zum Beispiel OBS oder ComfyUI")} required/></label>
            <label>{$t("Verbindung")}<select bind:value={mcpDraft.transport}><option value="stdio">{$t("Lokaler Prozess · stdio")}</option><option value="http">{$t("Server-Adresse · HTTP")}</option></select></label>
            {#if mcpDraft.transport === 'stdio'}
              <label>{$t("Programm / Startbefehl")}<span class="editor-input-action"><input bind:value={mcpDraft.command} placeholder={$t("npx, uvx oder absoluter Programmpfad")} required/><button type="button" class="ext-button" onclick={chooseExecutable} disabled={!native}><FolderOpen size={14}/><span class="sr-only">{$t("Programm auswählen")}</span></button></span></label>
              <label>{$t("Argumente")} <span>{$t("Ein Argument pro Zeile")}</span><textarea rows="3" bind:value={mcpDraft.argsText} placeholder="-y&#10;dein-mcp-paket"></textarea></label>
              <label>{$t("Arbeitsordner")} <span>{$t("Optional")}</span><input bind:value={mcpDraft.cwd} placeholder={$t("Absoluter Ordnerpfad")}/></label>
            {:else}<label>{$t("MCP-Endpunkt")}<input type="url" bind:value={mcpDraft.url} placeholder="http://127.0.0.1:8188/mcp" required/></label>{/if}
            <label class="label-with-help" for={`${uid}-key-reference`}><span>{$t("Schlüsselbund-ID")} <HelpTip label={$t("Hilfe zu Schlüsselbund-Verweisen")} text={$t("Hier steht nur der Name eines bereits hinterlegten Schlüssels, niemals dessen Wert. Bei HTTP verwendet die App ihn als Bearer-Token. Bei einem lokalen Prozess gibst du zusätzlich dessen Umgebungsvariablenname an. Ohne Zugangsdaten bleibt dieses Feld leer.")}/></span><input id={`${uid}-key-reference`} bind:value={mcpDraft.apiKeyId} placeholder={$t("Optional: mcp-mein-server")} autocomplete="off"/></label>
            {#if mcpDraft.transport === 'stdio' && mcpDraft.apiKeyId}<label>{$t("Schlüssel an Umgebungsvariable übergeben")}<input bind:value={mcpDraft.tokenEnv} placeholder={$t("Zum Beispiel API_KEY")}/></label>{/if}
            <p class="editor-hint">{$t("Lokale Server benötigen ihre installierte Laufzeit. Zugangsdaten bleiben im Betriebssystem-Schlüsselbund; hier werden nur Verweise bearbeitet.")}</p>
          {:else}
            <label>{$t("Skill-ID")}<input bind:value={skillDraft.id} readonly={skillDraft.existing} placeholder="stream-begruessung" required pattern={'[A-Za-z0-9_-]{1,96}'}/></label>
            <label>SKILL.md<textarea class="skill-source" rows="16" bind:value={skillDraft.content} spellcheck="false" aria-label={$t("Globalen Skillinhalt bearbeiten")}></textarea></label>
            {#if skillDraft.resources.length}<div class="skill-resources"><strong>{$t("Mitgelieferte Referenzen und Dateien")}</strong>{#each skillDraft.resources as resource}<span><FileText size={12}/>{typeof resource === 'string' ? resource : resource.path || resource.name || $t("Ressource")}</span>{/each}</div>{/if}
            <p class="editor-hint">{$t("Mit einem Skillordner importierst du auch Referenzen und Skripte. Hier bearbeitest du dessen SKILL.md; vorhandene Begleitdateien bleiben erhalten.")}</p>
          {/if}
          <footer class="editor-actions"><button type="button" class="ext-button quiet" onclick={closeEditor}>{$t("Abbrechen")}</button><button type="submit" class="ext-button library-primary" disabled={!native || !!operation || busy}><Check size={14}/>{operation === 'save' ? $t("Speichert …") : $t("Global speichern")}</button></footer>
        </fieldset></form>
      {:else if selected}
        <header class="scope-heading bot-scope"><span class="bot-marker">{profile.name?.charAt(0) || 'B'}</span><div><h2>{$t('Freigaben für {name}', { name: profile.name })}</h2><p>{$t("Diese Einstellungen gelten nur für diesen Bot.")}</p></div><HelpTip label={$t("Hilfe zu Botfreigaben")} text={$t("Die Bibliothek wird geteilt, die Berechtigungen nicht. Ein Bot erhält nur ausdrücklich freigegebene Skills und Werkzeuge. Zuschauerchat kann nur die zusätzlich für ihn freigegebenen Werkzeuge anfordern. Die App prüft diese Rechte bei jeder Ausführung.")}/></header>
        <div class="selected-extension"><div class="selected-title"><span class="detail-symbol">{#if tab === 'mcp'}<Cable size={24}/>{:else}<BookOpen size={24}/>{/if}</span><div><h3>{selected.name || selected.id}</h3><p>{tab === 'mcp' ? selected.transport === 'stdio' ? $t("Gemeinsam installierter MCP-Prozess") : $t("Gemeinsam installierter MCP-Server") : selected.description || $t("Gemeinsam installierter Skill")}</p></div><div class="global-actions"><button class="icon-control" onclick={() => tab === 'mcp' ? editMcp(selected) : editSkill(selected)} disabled={!native || !!operation || busy || tab==='mcp' && selected.transport==='builtin'} aria-label={$t("Global bearbeiten")} title={$t("Global bearbeiten")}><Pencil size={15}/></button><button class="icon-control danger" onclick={askDelete} disabled={!native || !!operation || busy} aria-label={$t("Für alle Bots entfernen")} title={$t("Für alle Bots entfernen")}><Trash2 size={15}/></button></div></div>
          <div class="access-switch"><div><strong>{$t('Für {name} verwenden', { name: profile.name })}</strong><p>{tab === 'mcp' ? $t("Danach einzelne Werkzeuge und Zuschauerrechte auswählen.") : $t("Der Bot darf die Anweisungen dieses Skills bei Bedarf laden.")}</p></div><input type="checkbox" role="switch" aria-label={$t('{extension} für {name} verwenden', { extension: selected.name || selected.id, name: profile.name })} checked={tab === 'mcp' ? grant.enabled : skillActive} disabled={!native || !!operation} onchange={(event) => { if (tab === 'mcp') void saveBot('mcpServers', setMcpEnabled(profile.mcpServers, selected, event.currentTarget.checked)); else void saveBot('skillsEnabled', setSkillEnabled(profile.skillsEnabled, selected.id, event.currentTarget.checked)); }}/></div>
          {#if profile.model.toolsEnabled === false}<p class="scope-notice">{$t("Werkzeugaufrufe sind im Dialogmodell dieses Bots deaktiviert. Die hier gespeicherten Freigaben werden erst bei aktivierten Modellwerkzeugen verwendet.")}</p>{/if}
          {#if tab === 'mcp'}
            <div class="tools-heading"><div><h3>{$t("Werkzeuge")}</h3><p>{loadedTools ? loadedTools.length === 1 ? $t('1 Werkzeug geladen') : $t('{count} Werkzeuge geladen', { count: formatNumber(loadedTools.length, $locale) }) : $t("Lade die angebotenen Werkzeuge, bevor du Rechte vergibst.")}</p></div><button class="ext-button" onclick={() => loadTools(selected)} disabled={!native || !!operation || busy}><Plug size={14}/>{operation === 'tools' ? $t("Lädt …") : $t("Werkzeuge laden")}</button><HelpTip label={$t("Werkzeugkatalog erklären")} text={$t("Diese Aktion verbindet den gemeinsam installierten MCP-Server und liest seinen Werkzeugkatalog. Sie führt keine Werkzeuge aus und aktiviert keinen Bot. Die Verbindung kann von mehreren Bots verwendet werden; eine Trennung betrifft daher alle Bots.")}/></div>
            {#if loadedTools?.length}
              <table class="permissions-table" aria-label={$t('Freigaben für {name}', { name: profile.name })}>
                <thead><tr><th scope="col">{$t("Werkzeug")}</th><th scope="col">{$t("Bot")}</th><th scope="col"><span>{$t("Chat")} <HelpTip label={$t("Hilfe zu Zuschauerrechten")} text={$t("Nur aktivieren, wenn Zuschauer dieses Werkzeug über deinen Cohost anfordern dürfen. Chatfreigaben benötigen immer auch die Botfreigabe. Das Entfernen der Botfreigabe entzieht beides.")}/></span></th></tr></thead>
                <tbody>{#each loadedTools as tool (tool.name)}<tr>
                  <th scope="row"><strong>{tool.name}</strong><p>{tool.description || $t("Keine Beschreibung vom Server vorhanden.")}</p></th>
                  <td><label class="tool-permission"><input type="checkbox" aria-label={$t('{tool} für {name} erlauben', { tool: tool.name, name: profile.name })} checked={grant.allowedTools.includes(tool.name)} disabled={!native || !!operation || !grant.enabled} onchange={(event) => saveBot('mcpServers', setMcpTool(profile.mcpServers, selected.id, tool.name, event.currentTarget.checked))}/></label></td>
                  <td><label class="tool-permission"><input type="checkbox" aria-label={$t('{tool} für Zuschaueranfragen an {name} erlauben', { tool: tool.name, name: profile.name })} checked={grant.viewerAllowedTools.includes(tool.name)} disabled={!native || !!operation || !grant.enabled || !grant.allowedTools.includes(tool.name)} onchange={(event) => saveBot('mcpServers', setMcpTool(profile.mcpServers, selected.id, tool.name, event.currentTarget.checked, true))}/></label></td>
                </tr>{/each}</tbody>
              </table>
            {:else if loadedTools}<div class="tools-empty"><Cable size={24}/><p>{$t("Der Server stellt derzeit keine Werkzeuge bereit.")}</p></div>{:else}<div class="tools-empty"><ShieldCheck size={28}/><p>{grant.allowedTools.length ? $t('{count} Werkzeugfreigaben gespeichert', { count: formatNumber(grant.allowedTools.length, $locale) }) : $t("Installiert bedeutet noch nicht freigegeben.")}</p><small>{grant.allowedTools.length ? $t("Die gespeicherten Rechte bleiben erhalten. Lade den Werkzeugkatalog, um einzelne Freigaben zu prüfen und anzupassen.") : $t("Auch bei aktiviertem Server bleiben alle Werkzeuge gesperrt, bis du sie hier auswählst.")}</small>{#if grant.allowedTools.length}<button class="ext-button quiet" disabled={!native || !!operation} onclick={() => saveBot('mcpServers', grant.allowedTools.reduce((bindings, tool) => setMcpTool(bindings, selected.id, tool, false), profile.mcpServers))}>{$t("Alle Werkzeugrechte entziehen")}</button>{/if}</div>{/if}
            <div class="connection-footer"><p><Users size={14}/>{$t("Hostrechte gelten pro Bot. Ob Daten und Sitzungen getrennt bleiben, hängt zusätzlich vom verbundenen Server ab.")}</p>{#if loadedTools}<button class="ext-button quiet" onclick={() => disconnectServer(selected)} disabled={!native || !!operation || busy}><Unplug size={13}/>{$t("Gemeinsame Verbindung trennen")}</button>{/if}</div>
          {:else}
            <div class="skill-reading"><BookOpen size={30}/><h3>{$t("Ein Ablauf. Für deine Bots verfügbar.")}</h3><p>{$t("Der vollständige Skill und seine Referenzen werden bei Bedarf geladen. Eine Installation verändert weder die Persönlichkeit noch die AGENTS.md eines Bots.")}</p><button class="ext-button" onclick={() => editSkill(selected)} disabled={!native || !!operation}><FileText size={14}/>{operation === 'read-skill' ? $t("Lädt …") : $t("Inhalt öffnen")}</button></div>
            <div class="script-access"><div><strong>{$t('Skill-Skripte für {name} erlauben', { name: profile.name })}</strong><p>{$t("Gilt für alle freigegebenen Skills dieses Bots. Nur ausdrückliche Streamer-Aufträge können Skripte starten; Zuschauer erhalten diese Fähigkeit nicht.")}</p></div><input type="checkbox" role="switch" aria-label={$t('Skill-Skripte für {name} erlauben', { name: profile.name })} checked={profile.allowSkillScripts === true} disabled={!native || !!operation} onchange={(event) => saveBot('allowSkillScripts', event.currentTarget.checked)}/><HelpTip label={$t("Hilfe zu Skill-Skripten")} text={$t("Ein Skill kann lokale Python- oder JavaScript-Skripte mitbringen. Mit dieser gesonderten Freigabe dürfen sie bei expliziten Streamer-Aufträgen ausgeführt werden. Aktiviere sie nur für Skills, deren Skripte du geprüft hast. Diese Freigabe ist unabhängig von der globalen Installation.")}/></div>
          {/if}
          <p class="saved-note" role="status">{operation === 'grant' ? $t("Botfreigabe wird gespeichert …") : $t("Botfreigaben werden unmittelbar gespeichert.")}</p>
        </div>
      {:else}<div class="no-selection"><span class="scope-icon"><LibraryBig size={30}/></span><h2>{$t("Deine Bibliothek, deine Freigaben.")}</h2><p>{$t('Wähle links einen MCP-Server oder Skill. Rechts entscheidest du, was {name} und sein Zuschauerchat verwenden dürfen.', { name: profile.name })}</p><p class="no-selection-note">{$t('Persona und AGENTS.md bleiben unter „Persönlichkeit & Regeln“ pro Bot anpassbar.')}</p></div>{/if}
    </section>
  </div>
</div>

{#if deletion}<div class="extension-modal-backdrop" role="presentation"><div class="extension-delete-dialog" role="alertdialog" aria-modal="true" aria-labelledby={`${uid}-delete-title`} aria-describedby={`${uid}-delete-description`} tabindex="-1" bind:this={dialog}><span class="delete-symbol"><Trash2 size={24}/></span><h2 id={`${uid}-delete-title`}>{$t("Für alle Bots entfernen?")}</h2><p id={`${uid}-delete-description`}>{$t('„{name}“ wird aus der gemeinsamen Bibliothek gelöscht. Alle Bots verlieren ihre Freigaben für diese Erweiterung.', { name: deletion.name })}{deletion.kind === 'skill' ? ' ' + $t('Der Skillordner und seine Begleitdateien werden entfernt.') : ' ' + $t('Eine bestehende Serververbindung wird getrennt.')}</p><p class="delete-alternative">{$t('Wenn nur {name} die Erweiterung nicht mehr verwenden soll, brich ab und schalte seine Botfreigabe aus.', { name: profile.name })}</p><div class="delete-actions"><button class="ext-button" bind:this={cancelButton} onclick={closeDelete} disabled={operation === 'delete'}>{$t("Abbrechen")}</button><button class="ext-button delete-confirm" onclick={deleteGlobal} disabled={!!operation || busy}>{operation === 'delete' ? $t("Entfernt …") : $t("Für alle Bots entfernen")}</button></div></div></div>{/if}

<style>
  .extensions-workspace { --ext-line: var(--line, #344653); --ext-panel: var(--panel, #19242e); --ext-soft: var(--surface-raised, #22313d); --ext-muted: var(--muted, #a5b4c3); --ext-text: var(--text, #edf2f6); --ext-accent: var(--accent, #91cfc4); --ext-input: var(--input-bg, #121c25); --ext-danger: var(--danger, #efa8ae); color: var(--ext-text); font-size: 13px; }
  .extension-columns { display: grid; grid-template-columns: minmax(245px, .8fr) minmax(0, 1.2fr); align-items: start; gap: 22px; }
  .library-column, .details-column { border: 1px solid var(--ext-line); border-radius: 10px; background: var(--ext-panel); min-width: 0; }
  .scope-heading { display: flex; gap: 12px; align-items: center; padding: 20px; border-bottom: 1px solid var(--ext-line); }
  .scope-heading > div { flex: 1; min-width: 0; }
  .scope-heading h2 { font-size: 16px; line-height: 1.5; overflow-wrap: anywhere; }
  .scope-heading p { font-size: 12px; line-height: 1.7; color: var(--ext-muted); margin-top: 4px; }
  .scope-icon, .bot-marker { display: grid; place-items: center; width: 36px; height: 36px; flex-shrink: 0; border-radius: 8px; background: var(--ext-soft); color: var(--ext-muted); }
  .bot-marker { color: var(--ext-accent); font-family: 'Space Grotesk', sans-serif; font-size: 18px; border: 1px solid #466960; }
  .bot-scope { border-top: 2px solid var(--ext-accent); border-radius: 10px 10px 0 0; }
  .library-tabs { display: flex; gap: 6px; padding: 16px 16px 0; }
  .library-tabs button { flex: 1; display: flex; gap: 8px; align-items: center; justify-content: center; min-height: 40px; padding: 9px 12px; border: 1px solid transparent; border-radius: 6px; background: transparent; color: var(--ext-muted); font-size: 13px; }
  .library-tabs button:hover { color: var(--ext-text); background: var(--ext-soft); }
  .library-tabs button.chosen { background: var(--ext-soft); border-color: var(--ext-line); color: var(--ext-text); }
  .library-tabs button > span { font-size: 12px; color: var(--ext-muted); margin-left: 4px; font-variant-numeric: tabular-nums; }
  .library-search { display: flex; align-items: center; gap: 10px; margin: 16px 16px 12px; padding: 0 12px; border: 1px solid var(--ext-line); border-radius: 6px; color: var(--ext-muted); background: var(--ext-input); }
  .library-search input { min-width: 0; margin: 0; padding: 12px 0; border: 0; background: transparent; color: var(--ext-text); font-size: 13px; }
  .library-search:focus-within { outline: 2px solid var(--ext-accent); outline-offset: 3px; }
  .library-search input:focus-visible { outline: none; }
  .library-filter { display: flex; align-items: center; gap: 10px; margin: 0 16px 14px; color: var(--ext-muted); font-size: 12px; }
  .library-filter select { margin: 0; min-height: 36px; padding: 7px 9px; font-size: 12px; min-width: 0; flex: 1; border-color: var(--ext-line); color: var(--ext-text); background: var(--ext-input); }
  .library-list { padding: 0 8px 8px; min-height: 230px; max-height: 490px; overflow: auto; overscroll-behavior: contain; }
  .library-entry { width: 100%; display: flex; align-items: center; gap: 12px; padding: 14px 12px; border: 1px solid transparent; border-radius: 6px; background: transparent; color: var(--ext-muted); text-align: left; margin: 3px 0; }
  .library-entry:hover { background: var(--ext-soft); color: var(--ext-text); }
  .library-entry.selected { border-color: #527b73; background: var(--ext-soft); color: var(--ext-text); }
  .entry-symbol { flex-shrink: 0; color: var(--ext-muted); }
  .entry-copy { flex: 1; min-width: 0; }
  .entry-copy strong { display: block; color: var(--ext-text); font-size: 14px; overflow-wrap: anywhere; }
  .entry-copy small { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; color: var(--ext-muted); font-size: 12px; margin-top: 5px; line-height: 1.65; overflow-wrap: anywhere; }
  .entry-access { display: flex; gap: 5px; align-items: center; font-size: 12px; line-height: 1.6; color: var(--ext-muted); margin-top: 7px; overflow-wrap: anywhere; }
  .entry-access.enabled { color: var(--ext-accent); }
  .library-actions { display: flex; flex-direction: column; gap: 10px; padding: 16px; border-top: 1px solid var(--ext-line); }
  .library-actions small { font-size: 12px; line-height: 1.7; color: var(--ext-muted); }
  .ext-button { display: inline-flex; align-items: center; justify-content: center; gap: 8px; min-height: 38px; padding: 9px 12px; border: 1px solid var(--ext-line); border-radius: 6px; background: var(--ext-soft); color: var(--ext-text); font-size: 12px; font-weight: 650; line-height: 1.5; }
  .ext-button:hover { background: #2b404d; border-color: #59747d; }
  .ext-button.library-primary { background: var(--ext-accent); border-color: var(--ext-accent); color: #142925; }
  .ext-button.library-primary:hover { background: #b0e0d8; border-color: #b0e0d8; }
  .ext-button.quiet { background: transparent; color: var(--ext-muted); }
  .ext-button.quiet:hover { color: var(--ext-text); background: var(--ext-soft); }
  button:disabled { opacity: .45; cursor: not-allowed; }
  button:focus-visible, input:focus-visible, select:focus-visible, textarea:focus-visible { outline: 2px solid var(--ext-accent); outline-offset: 3px; }
  .empty-library, .no-selection { display: flex; flex-direction: column; align-items: center; justify-content: center; text-align: center; gap: 14px; padding: 40px 24px; color: var(--ext-muted); }
  .empty-library strong { color: var(--ext-text); font-size: 14px; }
  .empty-library p { font-size: 13px; line-height: 1.8; max-width: 280px; }
  .no-selection { min-height: 480px; }
  .no-selection .scope-icon { width: 52px; height: 52px; border-radius: 12px; }
  .no-selection h2 { font-size: 21px; color: var(--ext-text); }
  .no-selection p { font-size: 13px; line-height: 1.8; max-width: 380px; }
  .no-selection .no-selection-note { margin-top: 16px; font-size: 12px; color: var(--ext-muted); }
  .selected-extension { padding: 22px; }
  .selected-title { display: flex; align-items: flex-start; gap: 12px; }
  .selected-title > div:nth-child(2) { flex: 1; min-width: 0; }
  .selected-title h3 { font-size: 22px; font-weight: 500; line-height: 1.4; color: var(--ext-text); overflow-wrap: anywhere; }
  .selected-title p { font-size: 12px; line-height: 1.7; color: var(--ext-muted); margin-top: 6px; overflow-wrap: anywhere; }
  .detail-symbol { color: var(--ext-muted); padding-top: 3px; }
  .global-actions { display: flex; gap: 4px; }
  .icon-control { display: inline-grid; place-items: center; width: 36px; height: 36px; padding: 7px; flex-shrink: 0; background: transparent; border: 1px solid transparent; border-radius: 6px; color: var(--ext-muted); }
  .icon-control:hover { background: var(--ext-soft); border-color: var(--ext-line); color: var(--ext-text); }
  .icon-control.danger { color: var(--ext-danger); }
  .access-switch { display: flex; align-items: center; gap: 18px; margin: 24px 0; padding: 16px; border: 1px solid #496b64; border-radius: 8px; background: #1c302e; }
  .access-switch > div { flex: 1; min-width: 0; }
  .access-switch strong { display: block; font-size: 14px; color: var(--ext-text); overflow-wrap: anywhere; }
  .access-switch p { font-size: 12px; line-height: 1.7; color: var(--ext-muted); margin-top: 5px; }
  input[role="switch"] { width: 36px; height: 20px; }
  input[role="switch"]:before { width: 14px; height: 14px; top: 2px; left: 2px; }
  input[role="switch"]:checked { background: var(--ext-accent); border-color: var(--ext-accent); }
  input[role="switch"]:checked:before { transform: translateX(16px); }
  .tools-heading { display: flex; align-items: center; gap: 10px; margin: 26px 0 16px; }
  .tools-heading > div { flex: 1; min-width: 0; }
  .tools-heading h3 { font-size: 16px; }
  .tools-heading p { font-size: 12px; line-height: 1.7; color: var(--ext-muted); margin-top: 5px; }
  .permissions-table { width: 100%; table-layout: fixed; border-collapse: collapse; margin-top: 14px; border-top: 1px solid var(--ext-line); }
  .permissions-table th, .permissions-table td { padding: 12px 0; border-bottom: 1px solid var(--ext-line); vertical-align: middle; }
  .permissions-table thead th { font-size: 12px; font-weight: 500; color: var(--ext-muted); text-align: left; }
  .permissions-table th:nth-child(2), .permissions-table td:nth-child(2) { width: 48px; text-align: center; }
  .permissions-table th:nth-child(3), .permissions-table td:nth-child(3) { width: 64px; text-align: center; }
  .permissions-table th:last-child > span { display: inline-flex; align-items: center; gap: 1px; }
  .permissions-table tbody th { text-align: left; font-weight: 400; padding-right: 12px; }
  .permissions-table strong { display: block; font-size: 13px; color: var(--ext-text); overflow-wrap: anywhere; }
  .permissions-table p { margin-top: 6px; font-size: 12px; line-height: 1.7; color: var(--ext-muted); overflow-wrap: anywhere; }
  .tool-permission { display: grid; place-items: center; width: 44px; height: 44px; margin: 0 auto; cursor: pointer; }
  .tool-permission input { width: 18px; height: 18px; margin: 0; accent-color: var(--ext-accent); }
  .tools-empty { border: 1px dashed var(--ext-line); border-radius: 8px; padding: 28px 20px; display: flex; flex-direction: column; align-items: center; gap: 12px; color: var(--ext-muted); text-align: center; }
  .tools-empty p { font-size: 13px; line-height: 1.7; }
  .tools-empty small { max-width: 350px; font-size: 12px; line-height: 1.8; color: var(--ext-muted); }
  .connection-footer { margin-top: 22px; display: flex; flex-direction: column; align-items: flex-start; gap: 14px; }
  .connection-footer p { display: flex; align-items: flex-start; gap: 9px; color: var(--ext-muted); font-size: 12px; line-height: 1.8; }
  .connection-footer p :global(svg) { flex-shrink: 0; margin-top: 4px; }
  .saved-note { font-size: 12px; line-height: 1.7; color: var(--ext-accent); padding-top: 22px; }
  .scope-notice { padding: 13px 15px; border-left: 2px solid var(--ext-accent); background: var(--ext-soft); color: var(--ext-muted); font-size: 12px; line-height: 1.8; margin-bottom: 22px; overflow-wrap: anywhere; }
  .skill-reading { display: flex; flex-direction: column; align-items: flex-start; gap: 13px; margin: 26px 0; color: var(--ext-muted); }
  .skill-reading h3 { color: var(--ext-text); font-size: 17px; }
  .skill-reading p { max-width: 450px; font-size: 13px; line-height: 1.8; color: var(--ext-muted); }
  .script-access { display: flex; gap: 12px; align-items: center; padding-top: 24px; border-top: 1px solid var(--ext-line); }
  .script-access > div { flex: 1; min-width: 0; }
  .script-access strong { font-size: 13px; color: var(--ext-text); overflow-wrap: anywhere; }
  .script-access p { margin-top: 6px; color: var(--ext-muted); font-size: 12px; line-height: 1.8; }
  .extension-editor { padding: 22px; }
  .extension-editor fieldset { border: 0; margin: 0; padding: 0; min-width: 0; }
  .editor-heading { background: var(--ext-soft); }
  .draft-status { display: block; font-size: 12px; font-weight: 600; color: var(--ext-accent); line-height: 1.6; margin-top: 7px; }
  .extension-editor label { display: block; font-size: 13px; color: var(--ext-text); margin: 18px 0; font-weight: 600; }
  .extension-editor label > span:not(.editor-input-action) { color: var(--ext-muted); font-size: 12px; font-weight: 450; }
  .extension-editor .label-with-help > span { display: inline-flex; gap: 5px; align-items: center; color: var(--ext-text); font-size: 13px; font-weight: 600; }
  .extension-editor input, .extension-editor select, .extension-editor textarea { width: 100%; background: var(--ext-input); border: 1px solid var(--ext-line); color: var(--ext-text); padding: 11px 12px; border-radius: 6px; margin-top: 8px; font-size: 13px; }
  .extension-editor textarea { resize: vertical; line-height: 1.8; }
  .extension-editor .skill-source { font-family: ui-monospace, SFMono-Regular, Consolas, monospace; font-size: 12px; min-height: 340px; }
  .extension-editor input:read-only { color: var(--ext-muted); background: var(--ext-soft); }
  .editor-input-action { display: flex; align-items: stretch; gap: 8px; margin-top: 8px; }
  .editor-input-action input { min-width: 0; margin: 0; }
  .editor-hint { font-size: 12px; line-height: 1.8; color: var(--ext-muted); }
  .editor-actions { display: flex; justify-content: flex-end; flex-wrap: wrap; gap: 10px; margin-top: 24px; padding-top: 20px; border-top: 1px solid var(--ext-line); }
  .skill-resources { display: flex; flex-direction: column; gap: 9px; font-size: 12px; padding: 14px 0; color: var(--ext-muted); }
  .skill-resources > span { display: flex; align-items: center; gap: 8px; overflow-wrap: anywhere; }
  .desktop-note { font-size: 13px; line-height: 1.8; color: var(--ext-muted); margin-bottom: 18px; }
  .extension-modal-backdrop { position: fixed; inset: 0; z-index: 40; display: grid; place-items: center; background: #070e18bf; backdrop-filter: blur(5px); padding: 24px; }
  .extension-delete-dialog { max-width: 500px; width: 100%; max-height: calc(100vh - 48px); overflow: auto; overscroll-behavior: contain; padding: 28px; background: var(--ext-panel); border: 1px solid var(--ext-line); border-radius: 12px; box-shadow: 0 20px 80px #050b1880; color: var(--ext-text); }
  .delete-symbol { display: inline-grid; place-items: center; width: 44px; height: 44px; background: #3d2b33; color: var(--ext-danger); border-radius: 10px; margin-bottom: 20px; }
  .extension-delete-dialog h2 { font-size: 24px; }
  .extension-delete-dialog p { margin-top: 15px; font-size: 13px; color: var(--ext-muted); line-height: 1.8; overflow-wrap: anywhere; }
  .extension-delete-dialog .delete-alternative { font-size: 12px; color: var(--ext-text); padding: 12px 14px; border: 1px solid var(--ext-line); border-radius: 6px; }
  .delete-actions { display: flex; gap: 10px; justify-content: flex-end; flex-wrap: wrap; padding-top: 24px; }
  .delete-confirm { background: var(--ext-danger); border-color: var(--ext-danger); color: #352029; }
  .delete-confirm:hover { background: #f6c2c7; border-color: #f6c2c7; }
  .sr-only { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0; }
  @media (max-width: 1200px) { .extension-columns { gap: 16px; grid-template-columns: minmax(225px, .8fr) minmax(0, 1.2fr); } .scope-heading { padding: 18px; gap: 10px; } .selected-extension, .extension-editor { padding: 18px; } .selected-title { flex-wrap: wrap; } .tools-heading { flex-wrap: wrap; } .tools-heading > div { flex-basis: calc(100% - 35px); } }
  @media (max-width: 1050px) { .extension-columns { grid-template-columns: 1fr; } .library-list { min-height: 0; max-height: 280px; } .library-actions { flex-direction: row; flex-wrap: wrap; } .library-actions small { flex-basis: 100%; } .no-selection { min-height: 300px; } }
  @media (max-width: 650px) { .scope-heading { padding: 16px; } .selected-extension, .extension-editor { padding: 16px; } .access-switch { padding: 14px; gap: 12px; } .selected-title h3 { font-size: 20px; } }
  @media (prefers-reduced-motion: reduce) { button { transition: none; } }
</style>
