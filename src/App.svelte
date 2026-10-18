<script lang="ts">
  import { onMount } from 'svelte';
  import { open, save } from '@tauri-apps/plugin-dialog';
  import { openPath, openUrl } from '@tauri-apps/plugin-opener';
  import { Radio, SlidersHorizontal, Sparkles, Plug, BrainCircuit, Blocks, FileText, History, Play, Pause, Square, ArrowUpRight, Plus, ChevronDown, X, Check, Copy, Upload, Download, ExternalLink, Mic, Send, Volume2, Monitor, MoreHorizontal, RotateCcw, Circle, Settings2, Cable, FolderOpen, Trash2, CheckCheck, Search, MoveUpRight, PanelLeftClose, WandSparkles, MessageCircle, TvMinimalPlay as Youtube, CircleHelp, Zap, Hand, EyeOff, PartyPopper } from '@lucide/svelte';
  import Avatar from './components/Avatar.svelte';
  import WakeWord from './components/WakeWord.svelte';
  import { SpeechPlayer, recordingToWav } from './lib/audio';
  import { EmoteSequence } from './lib/presentation';
  import { call, onEvent, native, cloneDefaults, defaultAgents, defaultPersona, assetUrl, prettyError } from './lib/api';
  import characterCatalog from '../public/character-catalog.json';
  import { version as appVersion } from '../package.json';

  type Page = 'studio'|'character'|'connections'|'models'|'extensions'|'instructions'|'memory';
  const nav = [{id:'studio',label:'Regiepult',icon:Radio},{id:'character',label:'Charakter & Stimme',icon:Sparkles},{id:'connections',label:'Verbindungen',icon:Plug},{id:'models',label:'Modelle',icon:BrainCircuit},{id:'extensions',label:'Erweiterungen',icon:Blocks},{id:'instructions',label:'Anweisungen',icon:FileText},{id:'memory',label:'Gedächtnis',icon:History}];
  const headings: Record<Page,string> = {studio:'Dein Stream. Zu zweit.',character:'Eine eigene Persönlichkeit.',connections:'Hier hört dein Cohost zu.',models:'Die Intelligenz dahinter.',extensions:'Mehr Möglichkeiten.',instructions:'Deine Regeln. Sein Charakter.',memory:'Was im Gespräch bleibt.'};
  const subheadings: Record<Page,string> = {studio:'Alles an einem Ort. Du gibst den Ton an.',character:'Gestalte, wie dein Gegenüber aussieht und klingt.',connections:'Twitch, YouTube und dein Mikrofon laufen zusammen.',models:'Cloud, lokal oder eine Mischung. Du entscheidest.',extensions:'Verbinde Werkzeuge und bring deinem Cohost neue Abläufe bei.',instructions:'Bearbeite die Dateien, die deinen Agenten ausmachen.',memory:'Ein Verlauf, den du einsehen und verwalten kannst.'};
  let page = $state<Page>('studio');
  let profile = $state<any>(cloneDefaults());
  let savedProfile = $state(JSON.stringify(cloneDefaults()));
  let documents = $state({agents:defaultAgents,persona:defaultPersona});
  let savedDocuments = $state({agents:defaultAgents,persona:defaultPersona});
  let bots = $state<any[]>([]);
  let botMenu = $state(false);
  let botDialog = $state(false);
  let botName = $state('');
  let botCopy = $state(false);
  let creatingBot = $state(false);
  let botCharacter = $state('nova');
  let characterSearch = $state('');
  let answeringBotId = $state('');
  let skills = $state<any[]>([]);
  let knowledge = $state<{name:string,size:number}[]>([]);
  let history = $state<any[]>([]);
  let queue = $state<any[]>([]);
  let logs = $state<any[]>([]);
  let secrets = $state<string[]>([]);
  let secretInputs = $state<Record<string,string>>({});
  let overlayUrl = $state('');
  let appDataDir = $state('');
  let connection = $state<Record<string,string>>({twitch:'disconnected',youtube:'disconnected'});
  let connectionMessages = $state<Record<string,string>>({});
  let running = $state(false);
  let busy = $state(false);
  let saving = $state(false);
  let speaking = $state(false);
  let speechPreparing = $state(false);
  let level = $state(0);
  let emotion = $state('neutral');
  let partial = $state('');
  let message = $state('');
  let toast = $state('');
  let error = $state(false);
  let extensionsTab = $state('mcp');
  let instructionTab = $state<'agents'|'persona'>('agents');
  let selectedSkill = $state('');
  let skillContent = $state('');
  let skillId = $state('');
  let modelList = $state<string[]>([]);
  let loadingModels = $state(false);
  let checkingCli = $state(false);
  let cliStatus = $state('');
  type CliModel = {id:string;name:string;description?:string;supportedReasoningEfforts:string[];defaultReasoningEffort?:string;isDefault?:boolean};
  let cliModels = $state<CliModel[]>([]);
  let cliConfiguredModel = $state('');
  let cliCatalogKey = $state('');
  let cliCatalogNote = $state('');
  let loadingCliModels = $state(false);
  let customCliModel = $state(false);
  let applyingFast = $state(false);
  let transcribing = $state(false);
  let wakeStopSignal = $state(0);
  let mcpDialog = $state(false);
  let mcpDraft = $state<any>({name:'',transport:'stdio',command:'npx',argsText:'',url:'',apiKeyId:''});
  let mcpStatuses = $state<Record<string,string>>({});
  let mcpTools = $state<Record<string,any[]>>({});
  let systemVoices = $state<SpeechSynthesisVoice[]>([]);
  let microphone = $state(false);
  let player: SpeechPlayer;
  let emoteSequence: EmoteSequence;
  let recorder: MediaRecorder | null = null;
  let microphoneStream: MediaStream | null = null;
  let recordingTimer: ReturnType<typeof setTimeout> | undefined;
  let turnEpoch = 0;
  const playbackWaiters = new Map<string,()=>void>();
  const finishedPlayback = new Set<string>();
  const presentedReplies = new Set<string>();
  const undisplayedReplies = new Set<string>();
  function clearEmotes(){emoteSequence?.stop();profile.avatar.emote=undefined;}
  function presentReply(reply:any){
    if(!reply.id||presentedReplies.has(reply.id))return;
    undisplayedReplies.delete(reply.id);
    presentedReplies.add(reply.id);
    if(presentedReplies.size>128)presentedReplies.delete(presentedReplies.values().next().value!);
    const botId=reply.botId||answeringBotId;
    const name=botId===profile.id?profile.name:bots.find(bot=>bot.id===botId)?.name||'Cohost';
    if(botId===answeringBotId)speechPreparing=false;
    if(botId===profile.id){
      emotion=reply.emotion||'neutral';
      if(!history.some(item=>item.id===reply.id))history=[...history,{id:reply.id,role:'assistant',text:reply.text,source:reply.source||'operator',user:name,createdAt:new Date().toISOString()}];
      emoteSequence?.start(reply.emotes||[]);
    }
    log(`${name}: ${reply.text}`);
  }
  function finishPlayback(id?:string) {
    if(!id){playbackWaiters.forEach(finish=>finish());playbackWaiters.clear();return;}
    finishedPlayback.add(id);
    if(finishedPlayback.size>128)finishedPlayback.delete(finishedPlayback.values().next().value!);
    playbackWaiters.get(id)?.();
  }
  function waitForPlayback(id:string):Promise<void> {
    if(finishedPlayback.has(id))return Promise.resolve();
    return new Promise(resolve=>{
      const finish=()=>{clearTimeout(timer);playbackWaiters.delete(id);resolve();};
      const timer=setTimeout(()=>{running=false;speaking=false;log('Keine Abschlussmeldung der Sprachausgabe. Automatische Antworten pausiert.','error');finish();},180000);
      playbackWaiters.set(id,finish);
    });
  }
  function profileSnapshot(){const p=$state.snapshot(profile);delete p.avatar.emote;return p;}
  const dirty = $derived(JSON.stringify(profileSnapshot()) !== savedProfile);
  const currentPage = $derived(nav.find(n => n.id === page));
  const lastReply = $derived([...history].reverse().find(h => h.role === 'assistant')?.text || '');
  const activeStatus = $derived(speaking ? 'Spricht gerade' : speechPreparing ? 'Bereitet Sprache vor' : busy ? 'Antwortet gerade' : running ? 'Hört zu' : 'Bereit, wenn du es bist');
  const connectedCount = $derived(Object.values(connection).filter(s => s === 'connected').length);
  const cliProvider = $derived(['claude-code','codex'].includes(profile.model.provider));
  const cliKey = $derived(`${profile.model.provider}:${profile.model.command?.trim()||profile.model.provider}`);
  const catalogModels = $derived(cliCatalogKey===cliKey?cliModels:[]);
  const pickerCliModels = $derived(catalogModels.length?catalogModels:profile.model.provider==='claude-code'?[{id:'haiku',name:'Haiku · schneller Dialog',supportedReasoningEfforts:[]},{id:'sonnet',name:'Sonnet · ausgewogen',supportedReasoningEfforts:[]},{id:'opus',name:'Opus · anspruchsvolle Aufgaben',supportedReasoningEfforts:[]}]:[]);
  const selectedCliModel = $derived(catalogModels.find(model=>model.id===(profile.model.model==='default'&&cliCatalogKey===cliKey?cliConfiguredModel:profile.model.model)));
  const cliEfforts = $derived(selectedCliModel?.supportedReasoningEfforts||[]);
  const cliPickerValue = $derived(customCliModel||profile.model.model&&profile.model.model!=='default'&&!pickerCliModels.some(model=>model.id===profile.model.model)?'__custom__':profile.model.model||'default');
  const effortLabels:Record<string,string>={none:'Ohne Denkphase (none)',minimal:'Minimal (minimal)',low:'Niedrig · kürzer nachdenken (low)',medium:'Mittel (medium)',high:'Hoch (high)',xhigh:'Sehr hoch (xhigh)',max:'Maximal (max)',ultra:'Ultra (ultra)'};
  function botEnabled(bot:any){return !!bot.live||bot.profile?.output?.mode==='obs'&&!!bot.profile?.output?.streamingEnabled;}
  const liveBots = $derived(bots.filter(botEnabled));
  const filteredCharacters = $derived(characterCatalog.filter(character=>`${character.name} ${character.description} ${character.author}`.toLocaleLowerCase('de-DE').includes(characterSearch.trim().toLocaleLowerCase('de-DE'))));
  const selectedCharacter = $derived(characterCatalog.find(character=>character.assetPath===profile.avatar.assetPath));

  function notify(text:string, isError=false) { toast=text;error=isError;setTimeout(()=>{if(toast===text)toast=''},6500); }
  function log(text:string,kind='info') {logs=[{id:crypto.randomUUID(),text,kind,time:new Date().toLocaleTimeString('de-DE',{hour:'2-digit',minute:'2-digit'})},...logs].slice(0,60);}
  function changeProvider() {
    const urls:Record<string,string>={ollama:'http://localhost:11434/v1',lmstudio:'http://localhost:1234/v1',openai:'https://api.openai.com/v1',anthropic:'https://api.anthropic.com/v1',compatible:'http://localhost:8000/v1'};
    const cli=['claude-code','codex'].includes(profile.model.provider);
    profile.model.baseUrl=urls[profile.model.provider]||'';profile.model.model=cli?'default':'';profile.model.command=profile.model.provider==='claude-code'?'claude':profile.model.provider==='codex'?'codex':'';profile.model.apiKeyId=cli?'':`${profile.id}-llm-${profile.model.provider}`;profile.model.reasoningEffort='default';modelList=[];cliStatus='';cliModels=[];cliConfiguredModel='';cliCatalogKey='';cliCatalogNote='';customCliModel=false;
  }
  function changeSttProvider(){
    const defaults:Record<string,any>={
      'whisper-local':{baseUrl:'http://127.0.0.1:8080',model:'',apiKeyId:'',localBackend:'whispercpp'},
      'openai':{baseUrl:'https://api.openai.com/v1',model:'whisper-1',apiKeyId:'stt-openai'},
      'elevenlabs':{baseUrl:'https://api.elevenlabs.io/v1',model:'scribe_v2',apiKeyId:'stt-elevenlabs'},
      'compatible':{baseUrl:'http://localhost:8000/v1',model:'whisper-1',apiKeyId:'stt-compatible'}
    };
    Object.assign(profile.stt,defaults[profile.stt.provider]);
    if(profile.stt.apiKeyId)profile.stt.apiKeyId=`${profile.id}-${profile.stt.apiKeyId}`;
  }
  function changeWhisperBackend(){profile.stt.baseUrl=profile.stt.localBackend==='compatible'?'http://127.0.0.1:8000/v1':'http://127.0.0.1:8080';profile.stt.model=profile.stt.localBackend==='compatible'?'whisper-1':'';}
  async function selectCli(){try{const path=await open({multiple:false,title:'CLI-Programm auswählen'});if(typeof path==='string'){profile.model.command=path;cliStatus='';}}catch(e){notify(prettyError(e),true);}}
  async function checkCli(){checkingCli=true;cliStatus='';try{const status=await call('check_cli_provider',{config:$state.snapshot(profile.model)});cliStatus=status.version;notify('CLI gefunden. Der Login erfolgt separat im Terminal.');}catch(e){notify(prettyError(e),true);}finally{checkingCli=false;}}
  async function loadCliModels(quiet=false){
    if(loadingCliModels)return false;
    const key=cliKey;const botId=profile.id;const config=$state.snapshot(profile.model);loadingCliModels=true;
    try{
      const result=await call('list_cli_models',{config});
      if(key!==cliKey||botId!==profile.id)return false;
      cliModels=(result.models||[]).filter((model:any)=>model.id&&model.id!=='default');cliConfiguredModel=result.configuredModel||'';cliCatalogKey=key;
      cliCatalogNote=result.supportsModelListing?'Katalog der installierten Codex-CLI. Die Verfügbarkeit hängt vom CLI-Konto und Anbieter ab.':'Bekannte Claude-Modellnamen und dynamische CLI-Aliase. Dies ist keine Abfrage deines Kontozugangs.';
      if(!quiet)notify(`${cliModels.length} Modelle und ihre Denkstufen geladen.`);
      return true;
    }catch(e){if(key===cliKey&&botId===profile.id){cliCatalogKey=key;cliCatalogNote=prettyError(e);if(!quiet)notify(prettyError(e),true);}return false;}
    finally{loadingCliModels=false;}
  }
  function selectCliModel(value:string){
    customCliModel=value==='__custom__';
    if(!customCliModel)profile.model.model=value;
    profile.model.reasoningEffort='default';
  }
  async function applyFastPreset(){
    const botId=profile.id;const key=cliKey;applyingFast=true;
    try{
      if(profile.model.provider==='claude-code'){
        profile.model.model='haiku';profile.model.reasoningEffort='default';customCliModel=false;
      }else if(profile.model.provider==='codex'){
        if(cliCatalogKey!==cliKey&&!await loadCliModels())return;
        if(botId!==profile.id||key!==cliKey)return;
        const id=profile.model.model==='default'?cliConfiguredModel:profile.model.model;
        const model=cliModels.find(item=>item.id===id);
        if(!model){notify('Wähle zuerst ein konkretes Modell aus dem geladenen CLI-Katalog.',true);return;}
        const order=['none','minimal','low','medium','high','xhigh','max','ultra'];
        profile.model.reasoningEffort=order.find(effort=>model.supportedReasoningEfforts.includes(effort))||'default';
      }
      profile.model.responseStyle='short';profile.model.maxToolRounds=1;profile.memory.maxMessages=12;
      notify('Schnellvorgabe eingestellt: kurz antworten, eine Werkzeugrunde, zwölf Verlaufseinträge. Zum Übernehmen speichern.');
    }finally{applyingFast=false;}
  }
  async function refresh() {
    if(!native)return;
    const state = await call('get_app_state');
    const visibleReplies=new Set(history.filter(item=>item.role==='assistant').map(item=>item.id));
    profile=state.profile;profile.stt.language??='de';profile.stt.localBackend??='whispercpp';profile.model.command??='';profile.model.reasoningEffort??='default';profile.model.responseStyle??='default';profile.model.maxToolRounds??=4;
    profile.hindsight={enabled:false,baseUrl:'http://127.0.0.1:8888',bankId:'',apiKeyId:`${profile.id}-hindsight`,autoRecall:true,autoRetain:true,...profile.hindsight};
    profile.wakeWord={enabled:false,phrases:[`hey ${profile.name.toLocaleLowerCase('de-DE')}`],...profile.wakeWord};customCliModel=false;savedProfile=JSON.stringify(profileSnapshot());
    documents=state.documents || documents;savedDocuments={...documents};bots=state.bots||[];skills=state.skills||[];knowledge=state.knowledge||[];
    history=(state.history||[]).filter((item:any)=>!undisplayedReplies.has(item.id)&&(!busy||answeringBotId!==state.profile.id||item.role!=='assistant'||visibleReplies.has(item.id)||presentedReplies.has(item.id)));
    secrets=state.secretIds||[];overlayUrl=state.overlayUrl||'';appDataDir=state.appDataDir||'';
  }
  async function persist(quiet=false) {
    saving=true;
    if(cliProvider&&!profile.model.model?.trim())profile.model.model='default';
    try { await call('save_profile',{profile:profileSnapshot(),botId:profile.id});savedProfile=JSON.stringify(profileSnapshot());bots=bots.map(bot=>bot.id===profile.id?{...bot,name:profile.name,profile:profileSnapshot()}:bot);if(!quiet)notify('Profil gespeichert.');return true; }
    catch(e){notify(prettyError(e),true);return false;}finally{saving=false;}
  }
  async function saveCurrentBot(){
    if(!await persist(true))return false;
    try{for(const key of ['agents','persona'] as const){if(documents[key]!==savedDocuments[key]){await call('save_document',{name:key==='agents'?'AGENTS.md':'PERSONA.md',content:documents[key]});savedDocuments[key]=documents[key];}}return true;}
    catch(e){notify(prettyError(e),true);return false;}
  }
  async function selectBot(id:string){
    if(busy||microphone||transcribing){notify('Beende zuerst die laufende Antwort oder Aufnahme.');return;}
    if(id===profile.id){botMenu=false;return;}
    wakeStopSignal++;
    if(!await saveCurrentBot())return;
    try{player?.stop();await call('select_bot',{id});await refresh();selectedSkill='';mcpStatuses={};mcpTools={};cliStatus='';partial='';emotion='neutral';botMenu=false;log(`${profile.name} zum Bearbeiten ausgewählt.`);}catch(e){notify(prettyError(e),true);}
  }
  async function createBot(){
    if(!botName.trim())return;
    wakeStopSignal++;creatingBot=true;
    try{
      if(!await saveCurrentBot())return;
      await call('create_bot',{name:botName.trim(),copyCurrent:botCopy});await refresh();
      if(!botCopy)profile.wakeWord.phrases=[`hey ${profile.name.toLocaleLowerCase('de-DE')}`];
      const character=characterCatalog.find(entry=>entry.id===botCharacter);
      if(character){profile.avatar.assetPath=character.assetPath;profile.emotes=structuredClone(character.emotes);await persist(true);}
      botDialog=false;botMenu=false;botName='';page='character';selectedSkill='';mcpStatuses={};mcpTools={};log(`${profile.name} angelegt.`);notify('Bot angelegt. Wähle sein Modell und seine Stimme.');
    }catch(e){notify(prettyError(e),true);}finally{creatingBot=false;}
  }
  async function toggleBotLive(bot:any){
    if(busy||microphone||transcribing){notify('Beende zuerst die laufende Antwort oder Aufnahme.');return;}
    if(!await saveCurrentBot())return;
    try{await call(botEnabled(bot)?'close_overlay':'open_overlay',{botId:bot.id});await refresh();botMenu=false;notify(botEnabled(bot)?`${bot.name} ausgeblendet.`:`${bot.name} ist im eigenen Figurenfenster.`);}catch(e){notify(prettyError(e),true);}
  }
  async function copyBotObs(bot:any){
    if(busy){notify('Beende zuerst die laufende Antwort.');return;}
    if(!await saveCurrentBot())return;
    try{const config=structuredClone(bot.id===profile.id?profileSnapshot():$state.snapshot(bot.profile));config.output.mode='obs';config.output.streamingEnabled=true;await call('close_overlay',{botId:bot.id});await call('save_profile',{profile:config,botId:bot.id});await refresh();const updated=bots.find(entry=>entry.id===bot.id);await navigator.clipboard.writeText(updated?.overlayUrl||bot.overlayUrl);notify(`OBS-Link für ${bot.name} kopiert.`);}catch(e){notify(prettyError(e),true);}
  }
  function chooseCharacter(character:any){const defaults=new Set(characterCatalog.flatMap(entry=>entry.emotes.map(emote=>emote.id)));const own=(profile.emotes||[]).filter((emote:any)=>!defaults.has(emote.id));profile.avatar.assetPath=character.assetPath;profile.emotes=[...structuredClone(character.emotes),...own];profile.avatar.emote=undefined;notify(`${character.name} ausgewählt. Passende Emotes übernommen.`);}
  async function storeSecret(id:string) {
    const value=secretInputs[id]?.trim();if(!value)return;
    try{await call('set_secret',{id,value});secrets=[...new Set([...secrets,id])];secretInputs[id]='';notify('Zugang im Systemschlüsselbund gespeichert.');}catch(e){notify(prettyError(e),true);}
  }
  async function loadModels() {
    loadingModels=true;
    try{modelList=await call('list_models',{config:$state.snapshot(profile.model)});notify(`${modelList.length} Modelle verfügbar.`);}catch(e){notify(prettyError(e),true);}finally{loadingModels=false;}
  }
  async function toggleRunning(){
    if(running){running=false;notify('Automatische Antworten pausiert.');return;}
    if(!profile.model.model&&!cliProvider&&!liveBots.some(bot=>bot.profile?.model?.model)){page='models';notify('Wähle zuerst ein Dialogmodell aus.');return;}
    if(await persist(true)){running=true;log('Cohost gestartet.');}
  }
  async function stopAll(){turnEpoch++;wakeStopSignal++;running=false;busy=false;speechPreparing=false;partial='';queue=[];clearTimeout(recordingTimer);if(recorder?.state==='recording'){recorder.onstop=null;recorder.stop();}microphoneStream?.getTracks().forEach(t=>t.stop());microphone=false;transcribing=false;player?.stop();clearEmotes();finishPlayback();speaking=false;level=0;try{await call('stop_generation');}catch(e){if(native)notify(prettyError(e),true);}log('Sprache und ausstehende Antworten gestoppt.');}
  async function runMessage(text:string,source='operator',user='Streamer',botId=profile.id) {
    if(!text.trim()||busy)return;
    const selected=botId===profile.id;
    const botConfig=selected?profileSnapshot():$state.snapshot(bots.find(bot=>bot.id===botId)?.profile);
    if(!botConfig){notify('Bot wurde nicht gefunden.',true);return;}
    if(!botConfig.model.model&&!['claude-code','codex'].includes(botConfig.model.provider)){
      if(selected)page='models';notify(`Wähle zuerst ein Dialogmodell für ${botConfig.name}.`);return;
    }
    const epoch=++turnEpoch;busy=true;answeringBotId=botId;speechPreparing=false;partial='';
    try {
      if(selected&&(!await saveCurrentBot()||epoch!==turnEpoch))return;
      if(botConfig.output.mode==='overlay'&&!bots.find(bot=>bot.id===botId)?.live)await call('open_overlay',{botId});
      if(selected)history=[...history,{id:crypto.randomUUID(),role:'user',text,source,user,createdAt:new Date().toISOString()}];
      const reply=await call('send_message',{text,source,user,skillIds:[],botId});
      if(reply.id&&!presentedReplies.has(reply.id))undisplayedReplies.add(reply.id);
      if(epoch!==turnEpoch)return;
      partial='';
      speechPreparing=!!botConfig.voice.enabled&&!presentedReplies.has(reply.id);
      if(botConfig.output.mode==='studio'||!botConfig.voice.enabled){
        // Publish the reply from the player's real start callback. Synthesis and
        // decoding can take seconds; tokens, text and emotes stay private until then.
        let reporting=Promise.resolve();
        const report=(state:boolean,error?:string)=>{
          reporting=reporting.then(()=>call('report_playback',{id:reply.id,botId,speaking:state,error})).catch(e=>{if(epoch===turnEpoch)log(prettyError(e),'error');});
          return reporting;
        };
        const begin=()=>{if(epoch!==turnEpoch)return;presentReply({...reply,botId,source});void report(true);};
        let audioError:string|undefined;
        try{
          if(botConfig.voice.enabled)await player.speak(reply.text,botConfig.voice,async()=>call('get_speech_audio',{id:reply.id,botId}),begin);
          else begin();
        }catch(e){audioError=prettyError(e);throw e;}
        finally{if(epoch===turnEpoch){if(audioError)clearEmotes();await report(false,audioError);}}
      }else if(reply.id)await waitForPlayback(reply.id);
    }catch(e){if(epoch===turnEpoch){notify(`${botConfig.name}: ${prettyError(e)}`,true);log(`${botConfig.name}: ${prettyError(e)}`,'error');}}
    finally{if(epoch===turnEpoch){busy=false;speechPreparing=false;partial='';answeringBotId='';}}
  }
  async function sendOwn(){const text=message;message='';await runMessage(text);}
  async function answerQueued(item:any,botId=profile.id){
    if(busy||microphone||transcribing)return;
    queue=queue.map(entry=>entry.id===item.id?{...entry,answeredBy:[...new Set([...(entry.answeredBy||[]),botId])]}:entry);
    await runMessage(item.text,item.platform,item.user,botId);
  }
  async function platformAction(platform:string){
    try{if(connection[platform]==='connected'||connection[platform]==='connecting'){await call('disconnect_platform',{platform});connection[platform]='disconnected';}
      else{if(!await persist(true))return;connection[platform]='connecting';await call('connect_platform',{platform});}}
    catch(e){connection[platform]='error';notify(prettyError(e),true);}
  }
  async function selectAvatar(){
    try{const path=await open({multiple:false,filters:[{name:'3D-Charakter',extensions:['vrm','glb']}]});if(typeof path!=='string')return;
      profile.avatar.assetPath=await call('import_avatar',{sourcePath:path});await persist(true);notify('Charakter importiert.');}catch(e){notify(prettyError(e),true);}
  }
  async function openCharacterWindow(){
    if(busy){notify('Beende zuerst die laufende Antwort.');return;}
    try{player?.stop();finishPlayback();profile.output.mode='overlay';profile.output.streamingEnabled=true;if(!await persist(true))return;await call('open_overlay',{botId:profile.id});await refresh();notify('Figurenfenster geöffnet.');}catch(e){notify(prettyError(e),true);}
  }
  async function useStudio(){if(busy){notify('Beende zuerst die laufende Antwort.');return;}player?.stop();finishPlayback();try{await call('close_overlay',{botId:profile.id});}catch{}profile.output.mode='studio';profile.output.streamingEnabled=false;await persist(true);}
  async function copyObs(){
    if(busy){notify('Beende zuerst die laufende Antwort.');return;}
    const bot=bots.find(entry=>entry.id===profile.id);
    if(!bot||!overlayUrl){notify('Der OBS-Link ist in der Desktop-App verfügbar.');return;}
    player?.stop();finishPlayback();await copyBotObs(bot);
  }
  async function configureOverlay(){if(await persist(true))try{await call('configure_overlay',{alwaysOnTop:profile.output.alwaysOnTop,clickThrough:profile.output.clickThrough,botId:profile.id});}catch(e){notify(prettyError(e),true);}}
  async function exportProfile(){try{const destination=await save({defaultPath:`${profile.id||'cohost'}.cohost.json`,filters:[{name:'Cohost-Profil',extensions:['json']}]});if(destination){await persist(true);await call('export_profile',{destination});notify('Profil ohne Zugangsdaten exportiert.');}}catch(e){notify(prettyError(e),true);}}
  async function importProfile(){try{const sourcePath=await open({multiple:false,filters:[{name:'Cohost-Profil',extensions:['json']}]});if(typeof sourcePath==='string'){wakeStopSignal++;await call('import_profile',{sourcePath});await refresh();notify('Profil importiert.');}}catch(e){notify(prettyError(e),true);}}
  async function saveDocument(){try{await call('save_document',{name:instructionTab==='agents'?'AGENTS.md':'PERSONA.md',content:documents[instructionTab]});savedDocuments[instructionTab]=documents[instructionTab];notify('Anweisungen gespeichert. Gelten ab der nächsten Antwort.');}catch(e){notify(prettyError(e),true);}}
  function editSkill(skill:any){selectedSkill=skill.id;skillId=skill.id;skillContent=skill.content||'';}
  function newSkill(){selectedSkill='new';skillId='';skillContent='---\nname: mein-skill\ndescription: Beschreibe, wann der Cohost diesen Ablauf verwenden soll.\n---\n\n# Mein Skill\n\nBeschreibe hier den Ablauf und die benötigten Werkzeuge.\n';}
  async function saveSkill(){try{await call('save_skill',{id:skillId,content:skillContent});await refresh();selectedSkill=skillId;notify('Skill gespeichert.');}catch(e){notify(prettyError(e),true);}}
  async function removeSkill(id:string){try{await call('delete_skill',{id});await refresh();selectedSkill='';notify('Skill entfernt.');}catch(e){notify(prettyError(e),true);}}
  async function importSkill(){try{const sourcePath=await open({directory:true,multiple:false});if(typeof sourcePath==='string'){await call('import_skill',{sourcePath});await refresh();notify('Skill-Ordner mit Referenzen und Assets importiert.');}}catch(e){notify(prettyError(e),true);}}
  async function importKnowledge(){try{const sourcePath=await open({multiple:false,filters:[{name:'Wissensdokument',extensions:['pdf','txt','md','json','csv']}]});if(typeof sourcePath==='string'){if(!await persist(true))return;const file=await call('import_knowledge',{sourcePath});knowledge=[...knowledge.filter(item=>item.name!==file.name),file];notify('Wissensdokument hinzugefügt. Passende Stellen werden automatisch herangezogen.');}}catch(e){notify(prettyError(e),true);}}
  async function removeKnowledge(name:string){try{await call('delete_knowledge',{name});knowledge=knowledge.filter(file=>file.name!==name);notify('Wissensdatei entfernt.');}catch(e){notify(prettyError(e),true);}}
  async function triggerEmote(id:string){try{if(!await persist(true))return;await call('trigger_emote',{emoteId:id,botId:profile.id});}catch(e){notify(prettyError(e),true);}}
  function addEmote(){profile.emotes=[...(profile.emotes||[]),{id:`emote-${Date.now()}`,name:'Mein Emote',action:'wave',duration:2.8,intensity:1,allowChat:false}];}
  async function addMcp(){
    if(!mcpDraft.name.trim()){notify('Gib dem Server einen Namen.');return;}
    const server={id:crypto.randomUUID(),name:mcpDraft.name,transport:mcpDraft.transport,command:mcpDraft.command,args:mcpDraft.argsText.split('\n').map((s:string)=>s.trim()).filter(Boolean),url:mcpDraft.url,apiKeyId:mcpDraft.apiKeyId,enabled:true,allowedTools:[],viewerAllowedTools:[]};
    profile.mcpServers=[...profile.mcpServers,server];if(await persist(true)){mcpDialog=false;notify('MCP-Server hinzugefügt. Verbinde ihn, um Werkzeuge auszuwählen.');}
  }
  async function connectMcp(server:any){
    const id=server.id;server.enabled=true;mcpStatuses[id]='connecting';
    try{if(!await persist(true))return;await call('mcp_connect',{serverId:id});mcpTools[id]=await call('mcp_tools',{serverId:id});mcpStatuses[id]='connected';notify('Server verbunden. Wähle die freigegebenen Werkzeuge.');}catch(e){mcpStatuses[id]='error';notify(prettyError(e),true);}
  }
  async function disconnectMcp(id:string){try{const server=profile.mcpServers.find((s:any)=>s.id===id);if(server)server.enabled=false;await call('mcp_disconnect',{serverId:id});await persist(true);mcpStatuses[id]='disconnected';}catch(e){notify(prettyError(e),true);}}
  async function toolAccess(server:any,name:string,viewer=false){const field=viewer?'viewerAllowedTools':'allowedTools';const list=server[field]||[];server[field]=list.includes(name)?list.filter((x:string)=>x!==name):[...list,name];if(!viewer&&!server.allowedTools.includes(name))server.viewerAllowedTools=(server.viewerAllowedTools||[]).filter((x:string)=>x!==name);await persist(true);}
  async function toggleMic(){
    if(microphone&&recorder){recorder.stop();microphone=false;return;}
    if(busy||transcribing)return;
    if(!await persist(true))return;
    const epoch=turnEpoch;
    try{
      const stream=await navigator.mediaDevices.getUserMedia({audio:true});
      if(epoch!==turnEpoch){stream.getTracks().forEach(t=>t.stop());return;}
      microphoneStream=stream;
      const chunks:BlobPart[]=[];
      const sessionRecorder=new MediaRecorder(stream);
      recorder=sessionRecorder;
      sessionRecorder.ondataavailable=e=>{if(e.data.size)chunks.push(e.data)};
      sessionRecorder.onstop=async()=>{
        clearTimeout(recordingTimer);stream.getTracks().forEach(t=>t.stop());microphone=false;
        if(epoch!==turnEpoch)return;
        transcribing=true;
        try{
          const blob=new Blob(chunks,{type:sessionRecorder.mimeType||'audio/webm'});
          const audio=await recordingToWav(blob);
          if(epoch!==turnEpoch)return;
          const text=await call<string>('transcribe_audio',{...audio,botId:profile.id});
          if(epoch!==turnEpoch)return;
          transcribing=false;message=text;
          if(profile.model.model||cliProvider){message='';await runMessage(text,'operator','Streamer');}
          else notify('Aufnahme transkribiert. Wähle ein Dialogmodell, um die Nachricht zu senden.');
        }catch(e){if(epoch===turnEpoch)notify(prettyError(e),true);}finally{transcribing=false;}
      };
      sessionRecorder.start(1000);microphone=true;
      recordingTimer=setTimeout(()=>{if(sessionRecorder.state==='recording'){sessionRecorder.stop();microphone=false;}},120000);
      notify('Mikrofon nimmt auf. Noch einmal klicken, um zu senden. Maximal zwei Minuten.');
    }catch(e){microphoneStream?.getTracks().forEach(t=>t.stop());microphone=false;notify(`Mikrofon: ${prettyError(e)}`,true);}
  }
  function timeOf(value:string){try{return new Date(value).toLocaleTimeString('de-DE',{hour:'2-digit',minute:'2-digit'})}catch{return ''}}

  onMount(()=>{
    player=new SpeechPlayer(v=>level=v,v=>speaking=v);
    emoteSequence=new EmoteSequence(emote=>{profile.avatar.emote={action:emote.action,nonce:emote.nonce||crypto.randomUUID(),duration:(emote.duration||2.5)*1000,intensity:emote.intensity??1};});
    const cleanup:(()=>void)[]=[];let disposed=false;
    function subscribe(name:string,handler:(event:any)=>void){onEvent(name,handler).then(f=>{if(disposed)f();else cleanup.push(f);}).catch(e=>log(prettyError(e),'error'));}
    const voices=()=>{systemVoices=window.speechSynthesis?.getVoices()||[]};voices();window.speechSynthesis?.addEventListener('voiceschanged',voices);
    if(native)refresh().catch(e=>notify(prettyError(e),true));
    subscribe('cohost://chat',m=>{queue=[...queue,{...m,answeredBy:[]}].slice(-200);});
    subscribe('cohost://connection',e=>{connection[e.platform]=e.status;connectionMessages[e.platform]=e.message||'';log(`${e.platform==='twitch'?'Twitch':'YouTube'}: ${e.message||e.status}`,e.status==='error'?'error':'info');});
    subscribe('cohost://agent',e=>{if(e.kind==='tool')log(e.text||e.tool||'Werkzeug aufgerufen','tool');if(e.kind==='error')log(e.text||'Agentenfehler','error');});
    subscribe('cohost://presentation',e=>{if(e.kind==='started'&&busy&&e.botId===answeringBotId)presentReply(e);});
    subscribe('cohost://playback',e=>{if((!e.botId||e.botId===profile.id)&&profile.output.mode!=='studio'&&(!e.output||e.output===profile.output.mode))speaking=!!e.speaking;if(!e.speaking){if(e.id||!e.botId||e.botId===answeringBotId)finishPlayback(e.id);if(e.error){if(e.botId===profile.id)clearEmotes();speechPreparing=false;notify(`Sprachausgabe: ${prettyError(e.error)}`,true);log(prettyError(e.error),'error');}}});
    subscribe('cohost://overlay-state',e=>{bots=bots.map(bot=>bot.id===e.botId?{...bot,live:!!e.active}:bot);if(e.botId&&e.botId!==profile.id)return;if(e.active){player?.stop();profile.output.mode='overlay';profile.output.streamingEnabled=true;}else{if(!busy)finishPlayback();speaking=false;}});
    subscribe('cohost://overlay',e=>{if(e.kind==='profile'&&e.profile?.id){bots=bots.map(bot=>bot.id===e.profile.id?{...bot,name:e.profile.name,profile:{...bot.profile,name:e.profile.name,tagline:e.profile.tagline,avatar:{...bot.profile.avatar,...e.profile.avatar},output:{...bot.profile.output,...e.profile.output},emotes:e.profile.emotes}}:bot);}if(e.botId&&e.botId!==profile.id||e.kind==='profile'&&e.profile?.id!==profile.id)return;if(e.kind==='stop'){player?.stop();clearEmotes();speechPreparing=false;if(!e.botId)finishPlayback();speaking=false;level=0;}if(e.kind==='profile'&&e.profile?.output){profile.output={...profile.output,...e.profile.output};}});
    subscribe('cohost://emote',e=>{if(e.botId&&e.botId!==profile.id)return;clearEmotes();profile.avatar.emote={action:e.action,nonce:e.nonce,duration:(e.duration||2.5)*1000,intensity:e.intensity??1};});
    const timer=setInterval(()=>{
      queue=queue.filter(q=>Date.now()-new Date(q.createdAt).getTime()<Math.max(profile.chat.expirySeconds,...bots.map(bot=>bot.profile?.chat?.expirySeconds||0))*1000);
      if(!running||busy||speaking||microphone||transcribing)return;
      const participants=bots.filter(bot=>botEnabled(bot)||bot.id===profile.id);
      for(const bot of participants){
        const config=bot.id===profile.id?profile:bot.profile;
        if(!config||config.chat.mode==='manual')continue;
        const next=queue.find(item=>!(item.answeredBy||[]).includes(bot.id)&&Date.now()-new Date(item.createdAt).getTime()<config.chat.expirySeconds*1000&&(!config.chat.trigger||item.text.toLowerCase().includes(config.chat.trigger.toLowerCase())||item.eventType&&item.eventType!=='message'));
        if(next){void answerQueued(next,bot.id);break;}
      }
    },1000);
    return()=>{disposed=true;turnEpoch++;clearEmotes();clearTimeout(recordingTimer);if(recorder?.state==='recording'){recorder.onstop=null;recorder.stop();}clearInterval(timer);finishPlayback();cleanup.forEach(f=>f());player.destroy();window.speechSynthesis?.removeEventListener('voiceschanged',voices);microphoneStream?.getTracks().forEach(t=>t.stop());};
  });
</script>

<svelte:head><title>Streamer AI Cohost</title></svelte:head>

<div class="app-shell">
  <aside class="sidebar">
    <div class="window-drag" data-tauri-drag-region></div>
    <a class="brand" href="/" onclick={(e)=>{e.preventDefault();page='studio'}} aria-label="Streamer AI Cohost Regiepult"><span class="brand-symbol"><Sparkles size={22} strokeWidth={2.3}/></span><span>streamer<span class="brand-ai">ai</span><small>Cohost studio</small></span></a>
    <div class="bot-switcher">
      <button class="profile-switch" aria-expanded={botMenu} onclick={()=>botMenu=!botMenu}><span class="mini-avatar"><span></span><span></span></span><span><strong>{profile.name}</strong><small>{bots.length||1} {bots.length===1?'Bot':'Bots'} · Auswahl</small></span><ChevronDown size={15}/></button>
      {#if botMenu}<div class="bot-menu"><div class="bot-menu-heading"><strong>Deine Bots</strong><button class="icon-button" aria-label="Bot-Auswahl schließen" onclick={()=>botMenu=false}><X size={14}/></button></div>{#each bots as bot}<button class="bot-menu-entry" class:chosen={bot.id===profile.id} onclick={()=>selectBot(bot.id)} disabled={busy||microphone||transcribing}><span class="bot-initial">{bot.name.charAt(0)}</span><span><strong>{bot.name}</strong><small>{bot.live?'Eigenes Figurenfenster':bot.profile?.output?.streamingEnabled?'OBS-Ausgabe':'Profil'}</small></span>{#if bot.id===profile.id}<Check size={14}/>{/if}</button>{/each}<button class="button secondary full" disabled={busy||microphone||transcribing} onclick={()=>{botMenu=false;botDialog=true;botCopy=false;botCharacter='nova'}}><Plus size={14}/>Neuen Bot anlegen</button></div>{/if}
    </div>
    <button class="new-bot-button" disabled={busy||microphone||transcribing} onclick={()=>{botDialog=true;botCopy=false;botCharacter='nova'}}><Plus size={14}/><span>Neuen Bot anlegen</span></button>
    <nav aria-label="Hauptnavigation">{#each nav as item}<button class:active={page===item.id} onclick={()=>page=item.id as Page}><item.icon size={18} strokeWidth={1.7}/><span>{item.label}</span>{#if item.id==='extensions'&&profile.mcpServers.length}<span class="nav-count">{profile.mcpServers.length}</span>{/if}</button>{/each}</nav>
    <div class="sidebar-bottom"><div class="runtime-line"><span class="status-dot" class:online={native}></span>{native?'Desktop-App':'Browser-Vorschau'}<span>v{appVersion}</span></div><button class="subtle-button" onclick={()=>{page='models'}}><SlidersHorizontal size={16}/> Integrationen verwalten</button><p>Dein Charakter. Deine Regeln.</p></div>
  </aside>

  <div class="workspace">
    <header class="app-header" data-tauri-drag-region><div class="breadcrumb">Studio <span>/</span> <strong>{currentPage?.label}</strong></div><div class="header-actions"><span class="mode-pill"><span class="small-dot"></span>{cliProvider?'CLI-Anbieter':['ollama','lmstudio'].includes(profile.model.provider)?'Lokales Modell':'Cloud / API'}</span>{#if dirty}<button class="button small secondary" onclick={()=>persist()} disabled={saving}><Check size={14}/>{saving?'Speichert …':'Profil speichern'}</button>{/if}<button class="stop-button" onclick={stopAll} title="Sprache und Agent sofort stoppen"><Square size={13} fill="currentColor"/>Stop</button></div></header>
    <main>
      <WakeWord bind:config={profile.wakeWord} botId={profile.id} sttProvider={profile.stt.provider} blocked={busy||speaking||speechPreparing||microphone||transcribing} stopSignal={wakeStopSignal} onTrigger={text=>runMessage(text,'microphone','Streamer',profile.id)} onError={message=>notify(message,true)} onConfigChange={()=>persist(true)}/>
      <div class="page-heading"><div><h1>{page==='studio'&&bots.length>1?'Dein Stream. Deine Crew.':headings[page]}</h1><p>{subheadings[page]}</p></div>{#if page==='studio'}<button class="button primary" class:running onclick={toggleRunning}>{#if running}<Pause size={16}/>Cohost pausieren{:else}<Play size={16} fill="currentColor"/>Cohost starten{/if}</button>{:else if page==='instructions'}<button class="button primary" onclick={saveDocument}><Check size={16}/>Datei speichern</button>{:else if page==='extensions'}<button class="button primary" onclick={()=>extensionsTab==='mcp'?mcpDialog=true:newSkill()}><Plus size={16}/>{extensionsTab==='mcp'?'Server hinzufügen':'Skill erstellen'}</button>{:else}<button class="button primary" onclick={()=>persist()} disabled={saving}><Check size={16}/>Änderungen speichern</button>{/if}</div>

      {#if page==='studio'}
        <section class="crew-panel"><div class="crew-heading"><div><h2>Deine Stream-Crew <span>{liveBots.length} Ausgaben aktiv</span></h2><p>Mehrere Figuren gleichzeitig. Jede mit eigenem Modell und eigener Stimme.</p></div><button class="button secondary small" disabled={busy||microphone||transcribing} onclick={()=>{botDialog=true;botCopy=false;botCharacter='nova'}}><Plus size={14}/>Bot hinzufügen</button></div><div class="crew-list">{#each bots as bot}<article class="crew-card" class:editing={bot.id===profile.id}><button class="crew-identity" onclick={()=>selectBot(bot.id)} disabled={busy||microphone||transcribing}><span class="bot-initial">{bot.name.charAt(0)}</span><span><strong>{bot.name}</strong><small>{bot.id===profile.id?'Zum Bearbeiten ausgewählt':bot.profile?.model?.provider||'Eigenes Profil'}</small></span></button><div class="crew-actions"><button class="button small" class:secondary={!(botEnabled(bot))} class:live-output={botEnabled(bot)} disabled={busy||microphone||transcribing} onclick={()=>toggleBotLive(bot)}><Monitor size={12}/>{botEnabled(bot)?'Ausgabe beenden':'Figur zeigen'}</button><button class="icon-button" title={`Eigener OBS-Link für ${bot.name}`} onclick={()=>copyBotObs(bot)} disabled={busy||microphone||transcribing}><Copy size={14}/></button></div></article>{/each}</div><p class="crew-note">Der Live-Chat ist die gemeinsame Eingabe. Eigene Ansprache und Antwortmodus legst du pro Bot unter Verbindungen fest. Antworten werden nacheinander gesprochen.</p></section>
        <div class="studio-layout"><section class="stage-column"><div class="studio-stage"><div class="stage-top"><span class="stage-identity"><span class="stage-dot" class:alive={running||speaking}></span>{profile.name}<span class="soft-label">Dein KI-Cohost</span></span><button class="stage-tool" onclick={()=>page='character'} title="Charakter bearbeiten"><Settings2 size={17}/></button></div>
          {#if profile.output.mode==='studio'}<Avatar {profile} {speaking} {level} {emotion} assetUrl={assetUrl(profile.avatar.assetPath,overlayUrl)}/>{:else}<div class="stage-away"><Monitor size={48} strokeWidth={1}/><h2>{profile.output.mode==='overlay'?'Auf deinem Desktop.':'Bereit für deinen Stream.'}</h2><p>{profile.output.mode==='overlay'?'Dein Charakter läuft im eigenen Figurenfenster.':'Füge den kopierten Link als Browserquelle in OBS ein.'}</p><button class="button dark" onclick={useStudio}>Zur Studioansicht</button></div>{/if}
          <div class="stage-caption">{#if partial||speaking||lastReply}<p>{partial||lastReply}</p>{:else}<h2>Hi, ich bin {profile.name}.</h2><p>{profile.tagline}</p>{/if}</div><div class="stage-bottom"><span><span class="stage-dot" class:alive={busy||speaking||running}></span>{activeStatus}</span><span><Volume2 size={13}/>{profile.voice.provider==='system'?'Systemstimme':profile.voice.provider==='elevenlabs'?'ElevenLabs':'Eigene Sprach-API'}</span></div></div>
          <div class="output-strip"><div class="output-label"><Monitor size={17}/><span>Darstellung</span></div><div class="segmented"><button class:selected={profile.output.mode==='studio'} onclick={useStudio}>Studio</button><button class:selected={profile.output.mode==='overlay'} onclick={openCharacterWindow}>Desktop-Overlay <MoveUpRight size={12}/></button><button class:selected={profile.output.mode==='obs'} onclick={copyObs}>OBS <Copy size={12}/></button></div></div>
          <div class="emote-toolbar"><span>Emotes</span>{#each profile.emotes || [] as emote}<button onclick={()=>triggerEmote(emote.id)} title={`${emote.name} ausführen`}>{#if emote.action==='wave'}<Hand size={14}/>{:else if emote.action==='hide'}<EyeOff size={14}/>{:else}<PartyPopper size={14}/>{/if}{emote.name}</button>{/each}<button class="emote-edit" onclick={()=>page='character'} title="Emotes bearbeiten"><Plus size={14}/></button></div>
          <div class="composer"><div class="composer-title"><MessageCircle size={16}/><span>Sprich mit {profile.name}</span><small>Nur für dich und deinen Cohost</small></div><form onsubmit={(e)=>{e.preventDefault();sendOwn()}}><input aria-label="Nachricht an deinen Cohost" placeholder="Was möchtest du deinem Cohost sagen?" bind:value={message} disabled={busy||transcribing||microphone}/><button type="button" class:recording={microphone} class="mic-button" onclick={toggleMic} disabled={busy||transcribing} title={microphone?'Aufnahme senden':'Mikrofon aufnehmen'} aria-label={microphone?'Aufnahme senden':'Mikrofon aufnehmen'}><Mic size={18}/></button><button class="send-button" type="submit" disabled={!message.trim()||busy||transcribing||microphone} aria-label="Nachricht senden"><Send size={17}/></button></form><div class="composer-foot"><span>{transcribing?'Aufnahme wird transkribiert …':microphone?'Aufnahme läuft …':busy?'Antwort wird erstellt …':profile.model.model||'Wähle unter Modelle deine KI aus.'}</span><span>Enter zum Senden</span></div></div>
        </section><aside class="chat-column"><div class="chat-panel"><div class="panel-title"><h2>Live-Chat <span class="count-badge">{queue.length}</span></h2><button class="icon-button" onclick={()=>page='connections'} title="Chat-Verbindungen"><Plug size={17}/></button></div><div class="platform-tabs"><button onclick={()=>page='connections'}><span class="platform-icon twitch">▰</span>Twitch<span class="status-dot" class:online={connection.twitch==='connected'}></span></button><button onclick={()=>page='connections'}><Youtube size={15} class="youtube"/>YouTube<span class="status-dot" class:online={connection.youtube==='connected'}></span></button></div><div class="chat-messages">{#if !queue.length}<div class="empty-chat"><span class="empty-chat-icon"><MessageCircle size={28} strokeWidth={1.3}/></span><h3>{connectedCount?'Ein offenes Ohr.':'Noch ganz unter uns.'}</h3><p>{connectedCount?'Neue Nachrichten erscheinen hier. Du entscheidest, worauf dein Cohost antwortet.':'Verbinde deinen Chat und hol deine Community dazu.'}</p>{#if !connectedCount}<button class="text-link" onclick={()=>page='connections'}>Chat verbinden <ArrowUpRight size={14}/></button>{/if}</div>{:else}{#each queue as item (item.id)}<article class="chat-message"><div><span class="chat-platform" class:youtube={item.platform==='youtube'}>{item.platform==='twitch'?'T':'Y'}</span><strong>{item.user}</strong><time>{timeOf(item.createdAt)}</time></div><p>{item.text}</p><button class="text-link" disabled={busy||transcribing||microphone} onclick={()=>answerQueued(item)}>{(item.answeredBy||[]).includes(profile.id)?'Erneut mit':'Antwort mit'} {profile.name} <ArrowUpRight size={12}/></button></article>{/each}{/if}</div><div class="chat-control"><label for="reply-mode">Antwortmodus</label><select id="reply-mode" bind:value={profile.chat.mode}><option value="manual">Manuell auswählen</option><option value="cohost">Co-Host</option><option value="afk">Pausenvertretung</option></select>{#if profile.chat.mode!=='manual'}<small>Automatische Antworten starten mit „Cohost starten“.</small>{/if}</div></div><div class="activity-panel"><div class="panel-title"><h2>Aktivität</h2><span class="quiet-icon"><Zap size={15}/></span></div>{#if !logs.length}<p class="activity-empty">Bereit für deinen nächsten Stream.</p>{:else}<div class="activity-list">{#each logs.slice(0,4) as entry}<div><span class="activity-tick" class:failed={entry.kind==='error'}>{#if entry.kind==='error'}<X size={11}/>{:else}<Check size={11}/>{/if}</span><p>{entry.text}</p><time>{entry.time}</time></div>{/each}</div>{/if}<div class="local-note"><span class="status-dot online"></span>Profil lokal gespeichert<FolderOpen size={13}/></div></div></aside></div>

      {:else if page==='character'}
        <section class="settings-panel character-library"><div class="section-heading"><Sparkles size={20}/><div><h2>Figurenbibliothek</h2><p>{characterCatalog.length} Charaktere sind bereits dabei. Weitere VRM- und GLB-Figuren kannst du importieren.</p></div></div><div class="character-library-layout"><div class="character-live-preview"><Avatar {profile} {speaking} {level} {emotion} assetUrl={assetUrl(profile.avatar.assetPath,overlayUrl)}/><span>{selectedCharacter?.name||'Deine importierte Figur'}</span></div><div class="character-choices"><input class="character-search" aria-label="Charaktere durchsuchen" bind:value={characterSearch} placeholder="Figuren durchsuchen …"/><div class="character-choice-list">{#each filteredCharacters as character}<button class="character-choice" class:chosen={profile.avatar.assetPath===character.assetPath} onclick={()=>chooseCharacter(character)}><span class="character-symbol">{#if character.id==='nova'}<Sparkles size={24}/>{:else if character.id==='builtin:fox'}<PartyPopper size={24}/>{:else}<Radio size={24}/>{/if}</span><span><strong>{character.name}</strong><small>{character.description}</small><em>{character.license}</em></span>{#if profile.avatar.assetPath===character.assetPath}<Check size={17}/>{/if}</button>{/each}{#if !filteredCharacters.length}<p class="field-note">Keine Figur passt zu deiner Suche.</p>{/if}</div><button class="button secondary" onclick={selectAvatar}><Upload size={14}/>Eigene Figur importieren</button></div></div>{#if selectedCharacter?.sourceUrl}<p class="character-credit">{selectedCharacter.author} · {selectedCharacter.license} <button class="text-link" onclick={()=>openUrl(selectedCharacter.sourceUrl)}>Quelle & Lizenz <ExternalLink size={12}/></button></p>{/if}</section>
        <div class="settings-grid"><section class="settings-panel"><div class="section-heading"><Sparkles size={19}/><div><h2>Dein Charakter</h2><p>Eine Identität, die bleibt.</p></div></div><label>Name<input bind:value={profile.name}/></label><label>Kurze Vorstellung<input bind:value={profile.tagline}/></label><div class="field-row"><label>Akzentfarbe<input type="color" bind:value={profile.avatar.accent}/></label><label>Kamera<select bind:value={profile.avatar.camera}><option value="portrait">Porträt</option><option value="full">Ganze Figur</option></select></label></div><label>Größe <span>{Number(profile.avatar.scale).toFixed(1)}×</span><input type="range" min="0.6" max="1.5" step="0.05" bind:value={profile.avatar.scale}/></label><div class="toggle-row"><div><strong>Natürliche Bewegung</strong><small>Atmen, ruhige Bewegungen und Blinzeln</small></div><input type="checkbox" role="switch" bind:checked={profile.avatar.idle}/></div><div class="asset-drop"><Upload size={24}/><strong>{profile.avatar.assetPath?'Eigener 3D-Charakter':'Nova ist schon dabei.'}</strong><p>{profile.avatar.assetPath?.split(/[\\/]/).pop()||'Oder importiere deine eigene VRM- oder GLB-Datei.'}</p><button class="button secondary" onclick={selectAvatar}>3D-Datei importieren</button>{#if profile.avatar.assetPath}<button class="text-link" onclick={()=>chooseCharacter(characterCatalog[0])}>Nova wiederherstellen</button>{/if}</div></section><section class="settings-panel"><div class="section-heading"><Volume2 size={19}/><div><h2>Stimme & Sprache</h2><p>Ein eigener Klang für deinen Cohost.</p></div></div><label>Sprachausgabe<select bind:value={profile.voice.provider} onchange={()=>{profile.voice.apiKeyId=`${profile.id}-tts-${profile.voice.provider}`;profile.voice.voiceId='';profile.voice.model=profile.voice.provider==='elevenlabs'?'eleven_flash_v2_5':'tts-1';}}><option value="system">Systemstimme · sofort verfügbar</option><option value="elevenlabs">ElevenLabs · Kunststimme</option><option value="compatible">Lokale / kompatible TTS-API</option></select></label>{#if profile.voice.provider==='system'}<label>Stimme<select bind:value={profile.voice.voiceId}><option value="">Passende deutsche Systemstimme</option>{#each systemVoices as v}<option value={v.voiceURI}>{v.name} ({v.lang})</option>{/each}</select></label><p class="field-note">Die Systemstimme kommt vom Betriebssystem. Eine neue Kunststimme richtest du über einen Sprach-Anbieter oder eine lokale TTS-API ein.</p>{:else}<label>{profile.voice.provider==='elevenlabs'?'Voice-ID':'Stimmenname / Voice-ID'}<input bind:value={profile.voice.voiceId} placeholder="Deine gespeicherte Stimme"/></label><label>Sprachmodell<input bind:value={profile.voice.model}/></label>{#if profile.voice.provider==='compatible'}<label>API-Basis-URL<input bind:value={profile.voice.baseUrl} placeholder="http://localhost:8000/v1"/></label>{/if}<label>API-Schlüssel <span class="saved-tag">{secrets.includes(profile.voice.apiKeyId)?'Gespeichert':'Nicht gesetzt'}</span><div class="input-action"><input type="password" autocomplete="off" bind:value={secretInputs[profile.voice.apiKeyId]} placeholder="Neuen Schlüssel eingeben"/><button class="button secondary" onclick={()=>storeSecret(profile.voice.apiKeyId)}>Speichern</button></div></label>{#if profile.voice.provider==='elevenlabs'}<button class="text-link" onclick={()=>native?openUrl('https://elevenlabs.io/app/voice-lab'):window.open('https://elevenlabs.io/app/voice-lab','_blank','noopener')}>Kunststimme bei ElevenLabs gestalten <ExternalLink size={13}/></button>{/if}{/if}<label>Sprechtempo <span>{Number(profile.voice.rate).toFixed(1)}×</span><input type="range" min="0.6" max="1.5" step="0.05" bind:value={profile.voice.rate}/></label><label>Lautstärke <span>{Math.round(profile.voice.volume*100)} %</span><input type="range" min="0" max="1" step="0.05" bind:value={profile.voice.volume}/></label><div class="toggle-row"><div><strong>Sprachausgabe aktiv</strong><small>Antworten vorlesen und die Figur animieren</small></div><input type="checkbox" role="switch" bind:checked={profile.voice.enabled}/></div><p class="inline-note"><CircleHelp size={16}/>Eine Cloud-Stimme benötigt im lokalen Betrieb ein eigenes lokales Stimmenmodell.</p></section></div>

      {:else if page==='models'}
        <div class="settings-grid">
          <section class="settings-panel">
            <div class="section-heading"><BrainCircuit size={20}/><div><h2>Dialoganbieter</h2><p>Persönlichkeit und Werkzeuge bleiben beim Wechsel erhalten.</p></div></div>
            <label>Integration<select bind:value={profile.model.provider} onchange={changeProvider}>
              <option value="ollama">Ollama · lokal</option><option value="lmstudio">LM Studio · lokal</option>
              <option value="openai">OpenAI · Cloud-API</option><option value="anthropic">Anthropic · Cloud-API</option>
              <option value="claude-code">Claude Code · CLI-Login</option><option value="codex">Codex · CLI-Login</option>
              <option value="compatible">Kompatible API · eigener Endpunkt</option>
            </select></label>
            {#if cliProvider}
              <label>CLI-Programm<div class="input-action"><input bind:value={profile.model.command} placeholder={profile.model.provider==='codex'?'codex oder absoluter Programmpfad':'claude oder absoluter Programmpfad'}/><button class="button secondary" onclick={selectCli}><FolderOpen size={14}/>Auswählen</button></div></label>
              <div class="button-row"><button class="button secondary small" onclick={checkCli} disabled={checkingCli||loadingCliModels}><Cable size={14}/>{checkingCli?'Prüft …':'CLI prüfen'}</button><button class="button secondary small" onclick={()=>loadCliModels()} disabled={loadingCliModels||checkingCli}><RotateCcw size={14}/>{loadingCliModels?'Lädt …':'Modelle & Denkstufen laden'}</button>{#if cliStatus}<span class="field-note">{cliStatus}</span>{/if}</div>
              <label>Modell auswählen<select value={cliPickerValue} onchange={event=>selectCliModel(event.currentTarget.value)}><option value="default">CLI-Standard{cliCatalogKey===cliKey&&cliConfiguredModel?` · ${cliConfiguredModel}`:''}</option>{#each pickerCliModels as model}<option value={model.id}>{model.name} · {model.id}</option>{/each}<option value="__custom__">Eigene Modell-ID eingeben …</option></select></label>
              {#if cliPickerValue==='__custom__'}<label>Genaue Modell-ID<input bind:value={profile.model.model} oninput={()=>profile.model.reasoningEffort='default'} placeholder="Exakter Modellname aus deiner CLI"/></label>{/if}
              {#if cliCatalogNote&&cliCatalogKey===cliKey}<p class="field-note">{cliCatalogNote}</p>{/if}
              <label>Denkaufwand<select bind:value={profile.model.reasoningEffort}><option value="default">CLI-Standard{selectedCliModel?.defaultReasoningEffort?` · ${selectedCliModel.defaultReasoningEffort}`:''}</option>{#each cliEfforts as effort}<option value={effort}>{effortLabels[effort]||effort}</option>{/each}{#if profile.model.reasoningEffort!=='default'&&!cliEfforts.includes(profile.model.reasoningEffort)}<option value={profile.model.reasoningEffort}>{profile.model.reasoningEffort} · Katalog prüfen</option>{/if}</select></label>
              <p class="field-note">{cliEfforts.length?'Weniger Denkaufwand kann schneller antworten. Die Einstellung gilt für diesen Bot.':profile.model.provider==='claude-code'?'Für eine gezielte Denkstufe lade die Modellnamen und wähle eine genaue Modell-ID. Haiku 4.5 hat keine separate Denkstufe.':'Lade den Modellkatalog und wähle ein Modell, um dessen unterstützte Denkstufen einzustellen.'}</p>
              <p class="field-note">Mit <code>default</code> verwendet die CLI ihren Standard. Installiere {profile.model.provider==='codex'?'Codex':'Claude Code'} separat und melde dich dort im Terminal an. Die App nutzt diesen CLI-Zugang.</p>
              <p class="inline-note"><CircleHelp size={16}/>Das Programm läuft auf deinem Rechner. Die Modellverarbeitung erfolgt über den in der CLI eingerichteten Anbieter.</p>
            {:else}
              <label>API-Basis-URL<input bind:value={profile.model.baseUrl}/></label>
              <label>API-Schlüssel <span class="saved-tag">{secrets.includes(profile.model.apiKeyId)?'Gespeichert':'Optional für lokale Modelle'}</span><div class="input-action"><input type="password" autocomplete="off" bind:value={secretInputs[profile.model.apiKeyId]} placeholder="Schlüssel bleibt im Systemschlüsselbund"/><button class="button secondary" onclick={()=>storeSecret(profile.model.apiKeyId)}>Speichern</button></div></label>
              <label>Modell<div class="input-action"><input bind:value={profile.model.model} list="models" placeholder="Modellnamen eintragen"/><button class="button secondary" onclick={loadModels} disabled={loadingModels}>{loadingModels?'Lädt …':'Modelle laden'}</button></div><datalist id="models">{#each modelList as model}<option value={model}></option>{/each}</datalist></label>
              <label>Kreativität <span>{Number(profile.model.temperature).toFixed(1)}</span><input type="range" min="0" max="1.5" step="0.1" bind:value={profile.model.temperature}/></label>
              <label>Maximale Antwortlänge<select bind:value={profile.model.maxTokens}><option value={256}>Kurz · 256 Tokens</option><option value={512}>Normal · 512 Tokens</option><option value={1024}>Ausführlich · 1.024 Tokens</option></select></label>
            {/if}
            <div class="toggle-row"><div><strong>Werkzeugaufrufe aktivieren</strong><small>{cliProvider?'MCP, Skills und Emotes werden von der App ausgeführt.':'Das ausgewählte Modell muss Tool Calling unterstützen.'}</small></div><input type="checkbox" role="switch" bind:checked={profile.model.toolsEnabled}/></div>
            <div class="model-speed-settings"><div class="section-heading"><Zap size={18}/><div><h3>Antworttempo</h3><p>Weniger Kontext und kurze Antworten können die Wartezeit verkürzen.</p></div></div>
              {#if cliProvider}<button class="button secondary full" onclick={applyFastPreset} disabled={applyingFast||loadingCliModels||busy}><Zap size={14}/>{applyingFast?'Stellt ein …':'Schnellvorgabe für den Live-Chat'}</button><p class="field-note">{profile.model.provider==='claude-code'?'Haiku, kurzer Antwortstil, eine Werkzeugrunde und zwölf Verlaufseinträge.':'Geringster unterstützter Denkaufwand des gewählten Modells, kurzer Stil, eine Werkzeugrunde und zwölf Verlaufseinträge.'} Einstellungen anschließend speichern.</p>{/if}
              <label>Antwortstil<select bind:value={profile.model.responseStyle}><option value="default">Wie in AGENTS.md / Persona</option><option value="short">Kurz · ein bis zwei Sätze</option><option value="normal">Normal · zwei bis vier Sätze</option><option value="detailed">Ausführlicher nach Bedarf</option></select></label><p class="field-note">Eine Stilvorgabe für das Modell; kein festes Tokenlimit.</p>
              <div class="field-row"><label>Werkzeugrunden<select bind:value={profile.model.maxToolRounds} disabled={!profile.model.toolsEnabled}><option value={1}>1 · wenige Rückfragen</option><option value={2}>2 · ausgewogen</option><option value={4}>4 · komplexe Abläufe</option></select></label><label>Verlauf im Kontext<input type="number" min="2" max="100" bind:value={profile.memory.maxMessages} disabled={!profile.memory.enabled}/></label></div><p class="field-note">Eine Werkzeugrunde kann mehrere freigegebene Aktionen enthalten. Danach folgt die gesprochene Antwort. Die Vorbereitung der Audiodatei braucht zusätzlich Zeit.</p>
            </div>
            {#if !cliProvider}<p class="inline-note"><Cable size={16}/>Lokale Laufzeiten wie Ollama oder LM Studio startest du auf deinem Rechner. Die App verbindet sich mit ihrer API.</p>{/if}
          </section>
          <div class="settings-stack">
            <section class="settings-panel">
              <div class="section-heading"><Mic size={19}/><div><h2>Spracherkennung</h2><p>Für Mikrofonaufnahmen im Regiepult.</p></div></div>
              <label>Anbieter<select bind:value={profile.stt.provider} onchange={changeSttProvider}>
                <option value="whisper-local">Whisper · lokal</option><option value="elevenlabs">ElevenLabs · Cloud</option><option value="openai">OpenAI Whisper · Cloud</option><option value="compatible">Kompatible Transkriptions-API</option>
              </select></label>
              {#if profile.stt.provider==='whisper-local'}
                <label>Lokale Laufzeit<select bind:value={profile.stt.localBackend} onchange={changeWhisperBackend}><option value="whispercpp">whisper.cpp · whisper-server</option><option value="compatible">OpenAI-kompatibler Whisper-Server</option></select></label>
                <label>Lokale Server-URL<input bind:value={profile.stt.baseUrl} placeholder="http://127.0.0.1:8080"/></label>
                {#if profile.stt.localBackend==='compatible'}<label>Modell<input bind:value={profile.stt.model} placeholder="whisper-1"/></label>{/if}
                <p class="field-note">Der Whisper-Server mit deinem Modell muss separat auf diesem Rechner laufen. Bei whisper.cpp wird das Modell beim Serverstart geladen. Audio bleibt bei diesem Anbieter auf deinem Rechner.</p>
              {:else}
                {#if profile.stt.provider==='compatible'}<label>Transkriptions-API<input bind:value={profile.stt.baseUrl} placeholder="http://localhost:8000/v1"/></label>{/if}
                <label>Modell<input bind:value={profile.stt.model} placeholder={profile.stt.provider==='elevenlabs'?'scribe_v2':'whisper-1'}/></label>
                <label>API-Schlüssel <span class="saved-tag">{secrets.includes(profile.stt.apiKeyId)?'Gespeichert':profile.stt.provider==='compatible'?'Optional':'Benötigt'}</span><div class="input-action"><input type="password" autocomplete="off" bind:value={secretInputs[profile.stt.apiKeyId]} placeholder="Eigener Schlüssel für diese Integration"/><button class="button secondary" onclick={()=>storeSecret(profile.stt.apiKeyId)}>Speichern</button></div></label>
                {#if ['openai','elevenlabs'].includes(profile.stt.provider)}<p class="field-note">Deine Aufnahme wird zur Transkription an {profile.stt.provider==='elevenlabs'?'ElevenLabs':'OpenAI'} gesendet.</p>{/if}
              {/if}
              <label>Sprache<input bind:value={profile.stt.language} placeholder="de · leer = automatisch erkennen"/></label>
              <p class="field-note">Mikrofonaufnahmen werden auf deinem Rechner in Mono-WAV umgewandelt. Dialog, Erkennung und Stimme lassen sich unabhängig kombinieren.</p>
            </section>
            <section class="settings-panel tinted"><h2>Deine Kombination.</h2><p>Codex für Dialoge, lokales Whisper zum Hören und ElevenLabs zum Sprechen? Jede Integration hat ihre eigene Auswahl und ihren eigenen Zugang.</p><div class="integration-flow"><span><BrainCircuit size={17}/>Dialog</span><span><Mic size={17}/>Hören</span><span><Volume2 size={17}/>Sprechen</span></div></section>
          </div>
        </div>

      {:else if page==='connections'}
        <div class="settings-grid"><section class="settings-panel"><div class="connection-heading"><span class="platform-logo twitch-bg">▰</span><div><h2>Twitch</h2><p>EventSub WebSocket</p></div><span class="connection-state" class:connected={connection.twitch==='connected'}>{connection.twitch==='connected'?'Verbunden':connection.twitch==='connecting'?'Verbindet …':'Nicht verbunden'}</span></div><label>Client-ID<input bind:value={profile.chat.twitch.clientId} placeholder="Deine Twitch-Anwendung"/></label><div class="field-row"><label>Broadcaster-ID<input bind:value={profile.chat.twitch.broadcasterId}/></label><label>Bot-Nutzer-ID<input bind:value={profile.chat.twitch.userId}/></label></div><label>OAuth-Token <span class="saved-tag">{secrets.includes(profile.chat.twitch.secretId)?'Gespeichert':'Benötigt user:read:chat'}</span><div class="input-action"><input type="password" autocomplete="off" bind:value={secretInputs[profile.chat.twitch.secretId]} placeholder="Token deiner Twitch-Anwendung"/><button class="button secondary" onclick={()=>storeSecret(profile.chat.twitch.secretId)}>Speichern</button></div></label><button class="button secondary full" onclick={()=>platformAction('twitch')}><Plug size={15}/>{['connected','connecting'].includes(connection.twitch)?'Verbindung trennen':'Twitch verbinden'}</button>{#if connectionMessages.twitch}<p class="field-note">{connectionMessages.twitch}</p>{/if}<div class="toggle-row"><div><strong>Antworten auch in den Chat senden</strong><small>Benötigt zusätzlich user:write:chat.</small></div><input type="checkbox" role="switch" bind:checked={profile.chat.twitch.sendReplies}/></div><p class="field-note">Verwende ein OAuth-Token deiner eigenen Twitch-App mit Chat-Leserechten. Anmeldung und Token-Erstellung erfolgen bei Twitch.</p></section><section class="settings-panel"><div class="connection-heading"><span class="platform-logo youtube-bg"><Youtube size={24}/></span><div><h2>YouTube</h2><p>Live-Chat</p></div><span class="connection-state" class:connected={connection.youtube==='connected'}>{connection.youtube==='connected'?'Verbunden':connection.youtube==='connecting'?'Verbindet …':'Nicht verbunden'}</span></div><label>Live-Video-URL oder Video-ID<input bind:value={profile.chat.youtube.videoId} placeholder="https://youtube.com/watch?v=…"/></label><label>Live-Chat-ID <span>Optional</span><input bind:value={profile.chat.youtube.liveChatId} placeholder="Wird aus dem Live-Video ermittelt"/></label><label>YouTube Data API-Schlüssel <span class="saved-tag">{secrets.includes(profile.chat.youtube.apiKeyId)?'Gespeichert':'Nicht gesetzt'}</span><div class="input-action"><input type="password" autocomplete="off" bind:value={secretInputs[profile.chat.youtube.apiKeyId]} placeholder="Schlüssel deines Google-Projekts"/><button class="button secondary" onclick={()=>storeSecret(profile.chat.youtube.apiKeyId)}>Speichern</button></div></label><button class="button secondary full" onclick={()=>platformAction('youtube')}><Plug size={15}/>{['connected','connecting'].includes(connection.youtube)?'Verbindung trennen':'YouTube verbinden'}</button>{#if connectionMessages.youtube}<p class="field-note">{connectionMessages.youtube}</p>{/if}<div class="toggle-row"><div><strong>Antworten auch in den Chat senden</strong><small>Benötigt ein separates OAuth-Token mit YouTube-Schreibrecht.</small></div><input type="checkbox" role="switch" bind:checked={profile.chat.youtube.sendReplies}/></div>{#if profile.chat.youtube.sendReplies}<label>OAuth-Token für Chatantworten <span class="saved-tag">{secrets.includes(profile.chat.youtube.sendSecretId||'youtube-write')?'Gespeichert':'Nicht gesetzt'}</span><div class="input-action"><input type="password" autocomplete="off" bind:value={secretInputs[profile.chat.youtube.sendSecretId||'youtube-write']} placeholder="OAuth-Token deiner Google-Anwendung"/><button class="button secondary" onclick={()=>storeSecret(profile.chat.youtube.sendSecretId||'youtube-write')}>Speichern</button></div></label>{/if}<p class="field-note">Der Chat wird mit dem von YouTube vorgegebenen Abrufintervall gelesen. Beide Plattformen können gleichzeitig verbunden sein.</p></section><section class="settings-panel span-two"><div class="section-heading"><SlidersHorizontal size={19}/><div><h2>Gesprächssteuerung</h2><p>Damit aus vielen Nachrichten ein gutes Gespräch wird.</p></div></div><div class="three-fields"><label>Ansprache / Auslöser<input bind:value={profile.chat.trigger} placeholder="Leer = alle Nachrichten"/></label><label>Warteschlange maximal<input type="number" min="1" max="200" bind:value={profile.chat.maxQueue}/></label><label>Nachrichten verfallen nach (Sek.)<input type="number" min="10" max="600" bind:value={profile.chat.expirySeconds}/></label></div></section></div>

      {:else if page==='extensions'}
        <div class="page-tabs"><button class:active={extensionsTab==='mcp'} onclick={()=>extensionsTab='mcp'}><Cable size={16}/>MCP-Server <span>{profile.mcpServers.length}</span></button><button class:active={extensionsTab==='skills'} onclick={()=>extensionsTab='skills'}><WandSparkles size={16}/>Skills <span>{skills.length}</span></button></div>
        {#if extensionsTab==='mcp'}<div class="extension-intro"><div><h2>Verbinde deinen Agenten mit der Welt.</h2><p>Lokale Prozesse oder entfernte Server. Du legst pro Werkzeug fest, ob es für deinen Cohost und für Chat-Anfragen verfügbar ist.</p></div><Cable size={42} strokeWidth={1}/></div>{#if !profile.mcpServers.length}<div class="large-empty"><Blocks size={36} strokeWidth={1.2}/><h2>Platz für neue Fähigkeiten.</h2><p>Füge deinen ersten MCP-Server hinzu, zum Beispiel für OBS, Wissensquellen oder andere Anwendungen.</p><button class="button secondary" onclick={()=>mcpDialog=true}><Plus size={15}/>MCP-Server hinzufügen</button></div>{:else}<div class="server-list">{#each profile.mcpServers as server}<section class="settings-panel"><div class="server-heading"><span class="server-symbol"><Cable size={21}/></span><div><h2>{server.name}</h2><p>{server.transport==='stdio'?`${server.command} ${(server.args||[]).join(' ')}`:server.url}</p></div><span class="connection-state" class:connected={mcpStatuses[server.id]==='connected'}>{mcpStatuses[server.id]==='connected'?'Verbunden':mcpStatuses[server.id]==='connecting'?'Verbindet …':'Nicht verbunden'}</span>{#if mcpStatuses[server.id]==='connected'}<button class="button secondary small" onclick={()=>disconnectMcp(server.id)}>Trennen</button>{:else}<button class="button secondary small" onclick={()=>connectMcp(server)}>Verbinden</button>{/if}<button class="icon-button danger-text" title="Server entfernen" onclick={()=>{disconnectMcp(server.id);profile.mcpServers=profile.mcpServers.filter((s:any)=>s.id!==server.id);persist(true)}}><Trash2 size={16}/></button></div>{#if mcpTools[server.id]?.length}<div class="tool-table"><div class="tool-header"><span>Werkzeug</span><span>Cohost</span><span>Chat</span></div>{#each mcpTools[server.id] as tool}<div class="tool-row"><div><strong>{tool.name}</strong><p>{tool.description||'Keine Beschreibung hinterlegt.'}</p></div><input aria-label={`${tool.name} freigeben`} type="checkbox" checked={(server.allowedTools||[]).includes(tool.name)} onchange={()=>toolAccess(server,tool.name)}/><input aria-label={`${tool.name} für Zuschauer freigeben`} type="checkbox" disabled={!(server.allowedTools||[]).includes(tool.name)} checked={(server.viewerAllowedTools||[]).includes(tool.name)} onchange={()=>toolAccess(server,tool.name,true)}/></div>{/each}</div>{:else if mcpStatuses[server.id]==='connected'}<p class="field-note">Der Server stellt aktuell keine Werkzeuge bereit.</p>{/if}</section>{/each}</div>{/if}
        {:else}<div class="skill-layout"><section><div class="button-row skill-import"><button class="button secondary small" onclick={importSkill}><FolderOpen size={14}/>Skill-Ordner importieren</button></div><div class="skill-list">{#each skills as skill}<button class="skill-card" class:chosen={selectedSkill===skill.id} onclick={()=>editSkill(skill)}><div><span class="skill-icon"><WandSparkles size={18}/></span><span class="skill-state">{profile.skillsEnabled.includes(skill.id)?'Aktiv':'Inaktiv'}</span></div><h3>{skill.name}</h3><p>{skill.description}</p><small>{skill.id}/SKILL.md</small></button>{/each}<button class="skill-card add-skill" onclick={newSkill}><Plus size={25}/><span>Eigenen Skill erstellen</span></button></div><div class="settings-panel script-permission"><div class="toggle-row"><div><strong>Skill-Skripte für Betreiber-Anfragen</strong><small>Erlaubt Python- und Node-Skripte aus dem jeweiligen Skill-Ordner.</small></div><input type="checkbox" role="switch" bind:checked={profile.allowSkillScripts}/></div></div></section>{#if selectedSkill}<section class="settings-panel skill-editor"><div class="section-heading"><FileText size={18}/><div><h2>SKILL.md</h2><p>Anweisungen und Referenzen für deinen Agenten.</p></div></div><label>Ordnername<input bind:value={skillId} placeholder="mein-skill" disabled={selectedSkill!=='new'}/></label><textarea class="code-editor" spellcheck="false" bind:value={skillContent} aria-label="Skill-Inhalt"></textarea><div class="button-row"><button class="button primary" onclick={saveSkill}>Skill speichern</button>{#if selectedSkill!=='new'}<button class="button secondary" onclick={()=>{profile.skillsEnabled=profile.skillsEnabled.includes(skillId)?profile.skillsEnabled.filter((s:string)=>s!==skillId):[...profile.skillsEnabled,skillId];persist(true)}}>{profile.skillsEnabled.includes(skillId)?'Deaktivieren':'Aktivieren'}</button><button class="icon-button danger-text" onclick={()=>removeSkill(skillId)} title="Skill entfernen"><Trash2 size={17}/></button>{/if}</div></section>{:else}<div class="editor-empty"><FileText size={32} strokeWidth={1.2}/><p>Wähle einen Skill zum Bearbeiten.</p></div>{/if}</div>{/if}

      {:else if page==='instructions'}
        <div class="document-layout"><section class="document-panel"><div class="document-tabs"><button class:active={instructionTab==='agents'} onclick={()=>instructionTab='agents'}><FileText size={15}/>AGENTS.md</button><button class:active={instructionTab==='persona'} onclick={()=>instructionTab='persona'}><Sparkles size={15}/>PERSONA.md</button><span>Markdown</span></div><textarea class="document-editor" spellcheck="false" bind:value={documents[instructionTab]} aria-label={instructionTab==='agents'?'AGENTS.md bearbeiten':'PERSONA.md bearbeiten'}></textarea><div class="document-footer"><span>Änderungen gelten ab der nächsten Antwort.</span><span>{documents[instructionTab].length} Zeichen</span></div></section><aside class="settings-stack"><section class="settings-panel"><h2>{instructionTab==='agents'?'Wie dein Agent arbeitet.':'Wer dein Charakter ist.'}</h2><p>{instructionTab==='agents'?'Lege fest, wie dein Cohost Antworten formuliert, Werkzeuge verwendet und mit deinen Zuschauern umgeht.':'Gib deinem Charakter eine Geschichte, eine Haltung und einen eigenen Humor. Diese Beschreibung bleibt beim Modellwechsel erhalten.'}</p><div class="document-hint"><Check size={15}/><span>Persönliche Anweisungen</span></div><div class="document-hint"><Check size={15}/><span>Unabhängig vom Anbieter</span></div><div class="document-hint"><Check size={15}/><span>Als Datei bearbeitbar</span></div></section><section class="settings-panel"><h2>Dein Profil mitnehmen.</h2><p>Exportiere Charakter und Anweisungen. Zugangsdaten bleiben im Schlüsselbund.</p><div class="button-row"><button class="button secondary small" onclick={exportProfile}><Download size={14}/>Exportieren</button><button class="button secondary small" onclick={importProfile}><Upload size={14}/>Importieren</button></div>{#if appDataDir}<button class="text-link folder-link" onclick={()=>openPath(appDataDir)}><FolderOpen size={14}/>Profilordner öffnen</button>{/if}</section></aside></div>

      {:else if page==='memory'}
        <section class="settings-panel knowledge-panel"><div class="section-heading"><FileText size={19}/><div><h2>Deine Wissensbibliothek</h2><p>Kanalinfos, PDFs und Fakten, die du selbst hinterlegst.</p></div><button class="button secondary small" onclick={importKnowledge}><Plus size={14}/>Datei hinzufügen</button></div>
          {#if knowledge.length}<div class="knowledge-list">{#each knowledge as file}<div><FileText size={17}/><strong>{file.name}</strong><span>{(file.size/1024).toLocaleString('de-DE',{maximumFractionDigits:1})} KB</span><button class="icon-button danger-text" title={`${file.name} entfernen`} onclick={()=>removeKnowledge(file.name)}><Trash2 size={15}/></button></div>{/each}</div>{:else}<p class="field-note">Importiere PDF, TXT, Markdown, JSON oder CSV. Passende Textstellen werden vor jeder Antwort automatisch gesucht und mit Dateiname und gegebenenfalls PDF-Seite an das Dialogmodell übergeben.</p>{/if}
          <p class="field-note">Die Stichwortsuche läuft lokal und benötigt keine Modell-Werkzeuge. PDFs bis 20 MiB und 500 Seiten, Text bis 4 MiB. Gescannte PDFs benötigen vorher eine Texterkennung. Bei Cloud-Dialogmodellen gehen relevante Auszüge an den gewählten Anbieter.</p>
        </section>
        <section class="settings-panel knowledge-panel"><div class="section-heading"><BrainCircuit size={19}/><div><h2>Hindsight · Langzeitgedächtnis</h2><p>Erinnerungen automatisch abrufen und neue Gespräche speichern.</p></div></div>
          <div class="toggle-row"><div><strong>Hindsight verwenden</strong><small>Benötigt deinen eigenen lokalen oder entfernten Hindsight-Server.</small></div><input type="checkbox" role="switch" bind:checked={profile.hindsight.enabled}/></div>
          {#if profile.hindsight.enabled}
            <div class="field-row"><label>Server-Basis-URL<input bind:value={profile.hindsight.baseUrl} placeholder="http://127.0.0.1:8888"/></label><label>Memory-Bank<input bind:value={profile.hindsight.bankId} placeholder={profile.id}/></label></div>
            <p class="field-note">Ohne eigene Bank-ID wird die Bot-ID verwendet. Jeder neue Bot bekommt damit seine eigene Bank. Trage nur absichtlich dieselbe Bank bei mehreren Bots ein.</p>
            <label>API-Schlüssel <span class="saved-tag">{secrets.includes(profile.hindsight.apiKeyId)?'Gespeichert':'Optional, abhängig vom Server'}</span><div class="input-action"><input type="password" autocomplete="off" bind:value={secretInputs[profile.hindsight.apiKeyId]} placeholder="Bleibt im Systemschlüsselbund"/><button class="button secondary" onclick={()=>storeSecret(profile.hindsight.apiKeyId)}>Speichern</button></div></label>
            <div class="toggle-row"><div><strong>Erinnerungen automatisch abrufen</strong><small>Relevante Erinnerungen als Hintergrund vor jeder Antwort einfügen.</small></div><input type="checkbox" role="switch" bind:checked={profile.hindsight.autoRecall}/></div>
            <div class="toggle-row"><div><strong>Neue Gespräche automatisch speichern</strong><small>Überträgt die neue Nachricht und Antwort, sobald die Ausgabe beginnt.</small></div><input type="checkbox" role="switch" bind:checked={profile.hindsight.autoRetain}/></div>
          {/if}
          <p class="field-note">Alte Verläufe und Wissensdateien werden nicht gesammelt hochgeladen. Hindsight kann selbst Cloud-Modelle oder Embeddings nutzen; das hängt von deinem Server ab. Bei einem Ausfall läuft das Gespräch ohne Erinnerungen weiter. Erinnerungen verwaltest und löschst du auf deinem Hindsight-Server.</p>
        </section>
        <div class="memory-header settings-panel"><div><h2>Lokaler Gesprächsverlauf</h2><p>Kurzzeitkontext für nächste Antworten. Leeren löscht keine Hindsight-Erinnerungen.</p></div><label class="memory-toggle"><span>Kurzzeitkontext verwenden</span><input type="checkbox" role="switch" bind:checked={profile.memory.enabled}/></label><button class="button secondary small" onclick={async()=>{try{await call('clear_memory');history=[];notify('Gesprächsverlauf geleert.')}catch(e){notify(prettyError(e),true)}}}><Trash2 size={14}/>Verlauf leeren</button></div><div class="history-list">{#if !history.length}<div class="large-empty"><History size={36} strokeWidth={1.2}/><h2>Ein frischer Anfang.</h2><p>Gespräche mit deinem Cohost erscheinen hier.</p><button class="text-link" onclick={()=>page='studio'}>Zum Regiepult <ArrowUpRight size={14}/></button></div>{:else}{#each history as entry}<article class="history-entry" class:assistant={entry.role==='assistant'}><span class="history-avatar">{entry.role==='assistant'?profile.name.charAt(0):'S'}</span><div><div><strong>{entry.role==='assistant'?profile.name:entry.user||'Streamer'}</strong><span>{entry.source||'operator'}</span><time>{timeOf(entry.createdAt)}</time></div><p>{entry.text}</p></div></article>{/each}{/if}</div>
      {/if}

      {#if page==='character'}<section class="settings-panel emote-settings"><div class="emote-section-heading"><div class="section-heading"><PartyPopper size={19}/><div><h2>Eigene Emotes</h2><p>Winken, verstecken, freuen. Oder eine Animation deiner importierten Figur.</p></div></div><button class="button secondary small" onclick={addEmote}><Plus size={14}/>Emote hinzufügen</button></div><div class="emote-grid">{#each profile.emotes || [] as emote}<div class="emote-card"><div class="emote-card-heading"><input aria-label="Emote-Name" bind:value={emote.name}/><button class="icon-button danger-text" title="Emote entfernen" onclick={()=>profile.emotes=profile.emotes.filter((e:any)=>e.id!==emote.id)}><Trash2 size={14}/></button></div><label>Aktion oder Clipname<input list="emote-actions" bind:value={emote.action}/></label><div class="field-row"><label>Dauer (Sekunden)<input type="number" min="0.2" max="15" step="0.1" bind:value={emote.duration}/></label><label>Intensität<input type="number" min="0" max="2" step="0.1" bind:value={emote.intensity}/></label></div><div class="toggle-row"><div><strong>Für Chat-Anfragen</strong><small>Der Agent darf dieses Emote auf Zuschauerwunsch ausführen.</small></div><input type="checkbox" role="switch" bind:checked={emote.allowChat}/></div><button class="button secondary full" onclick={()=>triggerEmote(emote.id)}><Play size={13}/>Ausführen</button></div>{/each}</div><datalist id="emote-actions"><option value="wave">Winken</option><option value="hide">Verstecken</option><option value="celebrate">Freuen</option><option value="nod">Nicken</option><option value="dance">Tanzen</option></datalist><p class="field-note">Eigene Emotes können vorhandene Animationsclips einer importierten GLB-Datei über ihren Namen verwenden. Bewegungen bei VRM werden an die vorhandenen Humanoidknochen angepasst.</p></section><section class="settings-panel output-settings"><div class="section-heading"><Monitor size={19}/><div><h2>Darstellung & Ausgabe</h2><p>Dein Cohost bekommt seinen eigenen Platz.</p></div></div><div class="three-fields"><label>Ausgabe<select bind:value={profile.output.mode} onchange={()=>profile.output.mode==='overlay'?openCharacterWindow():profile.output.mode==='obs'?copyObs():useStudio()}><option value="studio">Studioansicht</option><option value="overlay">Desktop-Overlay</option><option value="obs">OBS-Browserquelle</option></select></label><label>Bildrate<select bind:value={profile.output.fps}><option value={30}>30 FPS</option><option value={60}>60 FPS</option></select></label><div class="button-row align-end"><button class="button secondary" onclick={openCharacterWindow}><MoveUpRight size={14}/>Figur öffnen</button><button class="button secondary" onclick={copyObs}><Copy size={14}/>OBS-Link</button></div></div><div class="toggle-row"><div><strong>Immer im Vordergrund</strong><small>Abhängig von Betriebssystem und Fenstermanager.</small></div><input type="checkbox" role="switch" bind:checked={profile.output.alwaysOnTop} onchange={configureOverlay}/></div><div class="toggle-row"><div><strong>Mausklicks durchreichen</strong><small>Zum Bearbeiten im Steuerungsfenster wieder ausschalten.</small></div><input type="checkbox" role="switch" bind:checked={profile.output.clickThrough} onchange={configureOverlay}/></div><div class="toggle-row"><div><strong>Untertitel anzeigen</strong><small>Gesprochener Text unter der Figur.</small></div><input type="checkbox" role="switch" bind:checked={profile.output.showSubtitles}/></div>{#if overlayUrl}<label>OBS-Browserquelle<input readonly value={overlayUrl}/></label>{/if}</section>{/if}
      <footer class="workspace-footer"><span>Streamer AI Cohost</span><span>Gemacht für deinen nächsten Stream.</span><span><span class="status-dot" class:online={native}></span>{native?'Lokal auf deinem Rechner':'Vorschau · Integrationen in der Desktop-App'}</span></footer>
    </main>
  </div>
</div>

{#if toast}<div class="toast" class:error role="status">{#if error}<CircleHelp size={18}/>{:else}<CheckCheck size={18}/>{/if}<span>{toast}</span><button onclick={()=>toast=''} aria-label="Meldung schließen"><X size={15}/></button></div>{/if}
{#if mcpDialog}<div class="modal-backdrop" role="presentation" onclick={(e)=>{if(e.target===e.currentTarget)mcpDialog=false}}><div class="modal" role="dialog" aria-modal="true" aria-labelledby="mcp-heading" tabindex="-1"><div class="modal-heading"><div><h2 id="mcp-heading">MCP-Server hinzufügen</h2><p>Ein neuer Anschluss für deinen Cohost.</p></div><button class="icon-button" onclick={()=>mcpDialog=false} aria-label="Dialog schließen"><X size={20}/></button></div><label>Name<input bind:value={mcpDraft.name} placeholder="Zum Beispiel: OBS Studio"/></label><label>Verbindung<select bind:value={mcpDraft.transport}><option value="stdio">Lokaler Prozess · stdio</option><option value="http">Entfernter Server · Streamable HTTP</option></select></label>{#if mcpDraft.transport==='stdio'}<label>Programm / Befehl<input bind:value={mcpDraft.command} placeholder="npx, uvx oder absoluter Programmpfad"/></label><label>Argumente · eines pro Zeile<textarea rows="4" bind:value={mcpDraft.argsText} placeholder="-y&#10;dein-mcp-paket"></textarea></label><p class="field-note">Der Server benötigt seine eigene Laufzeit. Node.js, Python oder andere Programme müssen auf deinem Rechner verfügbar sein.</p>{:else}<label>MCP-Endpunkt<input bind:value={mcpDraft.url} placeholder="https://dein-server.de/mcp"/></label><label>Schlüsselbund-ID für Bearer-Token <span>Optional</span><input bind:value={mcpDraft.apiKeyId} placeholder="mcp-mein-server"/></label>{#if mcpDraft.apiKeyId}<label>Bearer-Token<div class="input-action"><input type="password" bind:value={secretInputs[mcpDraft.apiKeyId]} autocomplete="off"/><button class="button secondary" onclick={()=>storeSecret(mcpDraft.apiKeyId)}>Speichern</button></div></label>{/if}{/if}<div class="modal-footer"><button class="button secondary" onclick={()=>mcpDialog=false}>Abbrechen</button><button class="button primary" onclick={addMcp}><Plus size={16}/>Server hinzufügen</button></div></div></div>{/if}

{#if botDialog}<div class="modal-backdrop" role="presentation" onclick={(event)=>{if(event.target===event.currentTarget&&!creatingBot)botDialog=false}}><div class="modal bot-create-dialog" role="dialog" aria-modal="true" aria-labelledby="bot-create-heading" tabindex="-1"><form onsubmit={(event)=>{event.preventDefault();createBot()}}><div class="modal-heading"><div><h2 id="bot-create-heading">Ein neues Crew-Mitglied.</h2><p>Jeder Bot bekommt sein eigenes Profil.</p></div><button type="button" class="icon-button" onclick={()=>botDialog=false} disabled={creatingBot} aria-label="Dialog schließen"><X size={20}/></button></div><label>Bot-Name<input bind:value={botName} placeholder="Wie heißt dein neuer Cohost?" maxlength="80" required/></label><label>Charakter<select bind:value={botCharacter}>{#each characterCatalog as character}<option value={character.id}>{character.name}</option>{/each}</select></label><label class="bot-copy-option"><input type="checkbox" bind:checked={botCopy}/><span><strong>Einstellungen von {profile.name} übernehmen</strong><small>Modell, Stimme, Skills und Anweisungen kopieren. Zugangsdaten bleiben als Verweise im Schlüsselbund.</small></span></label><p class="field-note">Du kannst alle Bots parallel einblenden und ihnen eigene OBS-Quellen geben. Welches Profil du hier bearbeitest, ändert die anderen Figurenfenster nicht.</p><div class="modal-footer"><button type="button" class="button secondary" onclick={()=>botDialog=false} disabled={creatingBot}>Abbrechen</button><button type="submit" class="button primary" disabled={!botName.trim()||creatingBot}><Plus size={15}/>{creatingBot?'Legt Bot an …':'Bot anlegen'}</button></div></form></div></div>{/if}
