<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import { t, translate, locale, supportedLocales, setLocale, formatNumber, formatDate } from './lib/i18n';
  import { open, save, operatorDialogActive, registerOperatorDialogBarrier } from './lib/operator-dialog';
  import { openPath, openUrl } from '@tauri-apps/plugin-opener';
  import { Radio, SlidersHorizontal, Sparkles, Plug, BrainCircuit, Blocks, FileText, History, Play, Pause, Square, ArrowUpRight, Plus, ChevronDown, X, Check, Copy, Upload, Download, ExternalLink, Mic, Send, Volume2, Monitor, MoreHorizontal, RotateCcw, Circle, Settings2, Cable, FolderOpen, Trash2, CheckCheck, Search, MoveUpRight, PanelLeftClose, WandSparkles, MessageCircle, TvMinimalPlay as Youtube, CircleHelp, Zap, Hand, EyeOff, PartyPopper, KeyRound } from '@lucide/svelte';
  import Avatar from './components/Avatar.svelte';
  import OutputPreviewControls from './components/OutputPreviewControls.svelte';
  import { OutputPreviewQueue, validScale, type OutputDisplay, type OutputPreview } from './lib/output-presentation';
  import MotionLibrary from './components/MotionLibrary.svelte';
  import type { MotionAsset } from './lib/avatar-motion';
  import ModelFeatures from './components/ModelFeatures.svelte';
  import AutopilotPanel from './components/AutopilotPanel.svelte';
  import LifeRuntimePanel from './components/LifeRuntimePanel.svelte';
  import LifeActionsPanel from './components/LifeActionsPanel.svelte';
  import WorldStatePanel from './components/WorldStatePanel.svelte';
  import ExperiencePanel from './components/ExperiencePanel.svelte';
  import SocialPanel from './components/SocialPanel.svelte';
  import CarePreferencesPanel from './components/CarePreferencesPanel.svelte';
  import ContentPacksPanel from './components/ContentPacksPanel.svelte';
  import UsageBudgetPanel from './components/UsageBudgetPanel.svelte';
  import ScreenContextPanel from './components/ScreenContextPanel.svelte';
  import { prepareScreenSession } from './lib/screen-session';
  import {LifeOutputVersions,type LifeOutput} from './lib/life-output';
  import {applyOverlayVisibility} from './lib/overlay-visibility';
  import { LifeUI } from './lib/life-ui';
  import { actionChanged, requestQuietRestForOutput, requestLifeMove,requestNativeLifeMove,requestNativeLifeSlot } from './lib/life-actions';
  import {lifeStartContext} from './lib/life-start-context';
  import {lifeSceneParticipants,LIFE_SCENE_OVER_LIMIT} from './lib/life-scene-participants';
  import {parseWorldSettings} from './lib/world-state';
  import type { CanonicalPoint, RegisterWorldActor } from './lib/world-output';
  import { LifeTurnAdmissions, LifeSnapshots, parseLifeBot, parseLifeTurn, parseLifeSnapshot, nativeSchedule, currentLifeTurn, sameLifeTurn, type LifeSnapshot, type LifeMode, type LifeBot, type LifeTurn } from './lib/life-runtime';
  import { ChatScheduler, appendChatMessage, chatPriority, chatParticipants, type ChatClaim } from './lib/chat-scheduler';
  import { speechBlocker, speechStatus } from './lib/autopilot-status';
  import { AutopilotRuntimeVersions, mayUpdateAutopilot, type AutopilotRuntime } from './lib/autopilot-runtime';
  import type { AvatarModelInfo } from './lib/avatar';
  import WakeWord from './components/WakeWord.svelte';
  import ExtensionsPanel from './components/ExtensionsPanel.svelte';
  import HelpTip from './components/HelpTip.svelte';
  import CredentialHelp from './components/CredentialHelp.svelte';
  import ApiAccessPanel from './components/ApiAccessPanel.svelte';
  import CredentialSelector from './components/CredentialSelector.svelte';
  import { getApiConnections, type ApiSnapshot } from './lib/api-access';
  import ConfirmDialog from './components/ConfirmDialog.svelte';
  import UpdatesPanel from './components/UpdatesPanel.svelte';
  import { shortcutFor } from './lib/shortcuts';
  import { focusTrap } from './lib/focus';
  import { SpeechPlayer, recordingToWav } from './lib/audio';
  import { MicrophoneSession } from './lib/microphone-session';
  import { ensureMicrophonePermission, openMicrophone } from './lib/microphone-permission';
  import MicrophonePermissions from './components/MicrophonePermissions.svelte';
  import SystemPermissionDialog from './components/SystemPermissionDialog.svelte';
  import { systemPermission, usablePermission } from './lib/system-permission';
  import { HostCapabilityRequests, uiEmoteActions, type HostCapabilities } from './lib/emote-catalogue';
  import { resolveAvailableAction, unavailableEmote, presentAdmittedEmote, avatarBinding, emoteBinding } from './lib/emote-capabilities';
  import { EmoteSequence, type EmotePayload } from './lib/presentation';
  import { EmoteAudioPlayer } from './lib/emote-audio';
  import { ExternalEmotePlayback } from './lib/emote-playback';
  import { foregroundSoundPlaying, type SoundActivity } from './lib/sound-playback';
  import { SubtitleLifetime } from './lib/subtitles';
  import { PlaybackWaiters, matchesPlayback, cancelPlaybackTurn } from './lib/playback-wait';
  import { call, onEvent, native, cloneDefaults, defaultAgents, defaultPersona, assetUrl, prettyError } from './lib/api';
  import characterCatalog from '../public/character-catalog.json';
  import { version as appVersion } from '../package.json';

  type Page = 'studio'|'character'|'connections'|'models'|'extensions'|'instructions'|'memory'|'autopilot'|'api-access';
  const nav = [{id:'studio',label:'Regiepult',icon:Radio,group:'live'},{id:'character',label:'Charakter & Stimme',icon:Sparkles,group:'bot'},{id:'models',label:'Modelle',icon:BrainCircuit,group:'bot'},{id:'instructions',label:'Persönlichkeit & Regeln',icon:FileText,group:'bot'},{id:'memory',label:'Wissen & Gedächtnis',icon:History,group:'bot'},{id:'connections',label:'Twitch & YouTube',icon:Plug,group:'bot'},{id:'autopilot',label:'Autopilot',icon:WandSparkles,group:'bot'},{id:'extensions',label:'Skills & MCP',icon:Blocks,group:'shared'},{id:'api-access',label:'API-Zugänge',icon:KeyRound,group:'shared'}];
  const headings: Record<Page,string> = {studio:'Regiepult',character:'Charakter & Stimme',connections:'Twitch & YouTube',models:'Modelle & Antworttempo',extensions:'Skills & MCP',instructions:'Persönlichkeit & Regeln',memory:'Wissen & Gedächtnis',autopilot:'Autopilot','api-access':'API-Zugänge'};
  const subheadings: Record<Page,string> = {studio:'Nachrichten auswählen, mit deinem Cohost sprechen und Ausgaben steuern.',character:'Aussehen, Stimme und Emotes für den ausgewählten Bot.',connections:'Stream verbinden und festlegen, wann dieser Bot antwortet.',models:'Dialog, Spracherkennung und Antworttempo unabhängig auswählen.',extensions:'Einmal installieren. Jedem Bot gezielt freigeben.',instructions:'Eigene Persönlichkeit und Arbeitsregeln für jeden Charakter.',memory:'Wissensdokumente, Erinnerungen und Gesprächsverlauf dieses Bots.',autopilot:'Figuren visuell anordnen und auf deinem Desktop starten.','api-access':'Zugänge einmal einrichten und für deine Bots auswählen.'};
  let page = $state<Page>('studio');
  let library = $state<any>({mcpServers:[],skills:[]});
  let shortcutHelp = $state(false);
  let updatesOpen = $state(false);
  let updateInstalling = $state(false);
  let availableUpdate = $state('');
  let wakeActive = $state(false);
  let autopilots = $state<Record<string,AutopilotRuntime>>({});
  let autopilotPending = $state(false);
  let usageBudgetDirty = $state(false);
  let apiAccessDirty = $state(false);
  let apiAccessBusy = $state(false);
  let apiSnapshot = $state<ApiSnapshot>();
  let apiPanel = $state<{saveDraft:()=>Promise<void>;discardDraft:()=>void}>();
  let apiMetadataEpoch = 0;
  function acceptApiSnapshot(value:ApiSnapshot|undefined){++apiMetadataEpoch;apiSnapshot=value;}
  async function refreshApiConnections(){const epoch=++apiMetadataEpoch;try{const value=await getApiConnections(call);if(appAlive&&epoch===apiMetadataEpoch)apiSnapshot=value;}catch{if(appAlive&&epoch===apiMetadataEpoch)apiSnapshot=undefined;}}
  let experienceDirty=$state(false),socialDirty=$state(false),careDirty=$state(false);
  let experienceBusy=$state(false),socialBusy=$state(false),careBusy=$state(false);
  type ExperienceDraftPanel={saveDraft:()=>Promise<void>;discardDraft:()=>void};
  let experiencePanel=$state<ExperienceDraftPanel>(),socialPanel=$state<ExperienceDraftPanel>(),carePanel=$state<ExperienceDraftPanel>();
  const lifePreferencesDirty=$derived(experienceDirty||socialDirty||careDirty);
  const lifePreferencesBusy=$derived(experienceBusy||socialBusy||careBusy);
  let worldDirty = $state(false);
  let worldBusy = $state(false);
  let worldPanel = $state<{saveDraft:()=>Promise<void>;cancelPendingStart:()=>void}>();
  let autopilotStartRevision = 0;
  let autopilotEventRevision = 0;
  const autopilotVersions = new AutopilotRuntimeVersions();
  let autopilotPreview = $state<AutopilotRuntime>();
  let answeringAutopilot = $state(false);
  let lifeRuntime = $state<LifeSnapshot>();
  let lifeOutputs=$state<Record<string,LifeOutput>>({});
  const lifeOutputVersions=new LifeOutputVersions();
  function acceptLifeOutput(value:unknown){
    try{const next=lifeOutputVersions.accept(value);if(!next)return;lifeOutputs={...lifeOutputs,[next.botId]:next};}
    catch(error){lifeError=prettyError(error);}
  }
  function resolvedOutput(config:any){const route=lifeOutputs[config.id];return route&&route.state!=='inactive'?'overlay':config.output.mode;}

  let lifeBots = $state<Record<string, LifeBot>>({});
  let lifeError = $state('');
  let lifeConnected = $state(false);
  let lifeMounted = $state(false);
  let lifeStopping = false;
  let nativeLifeTurn = $state<LifeTurn>();
  let nativeLifeOutput = $state('');
  let nativeReplyId = '';
  let nativePlaybackCompleted = '';
  let lifeActivity = false;
  let operatorRevision = 0;
  let nativeOperatorPending = $state(false);
  let operatorBarriers = $state(0);
  let lifeUi: LifeUI;
  const lifeSnapshots = new LifeSnapshots();
  const lifeAdmissions = new LifeTurnAdmissions();
  const chatScheduler = new ChatScheduler();
  let silentAutopilots = $state<Record<string, string>>({});
  function acceptAutopilot(runtime:AutopilotRuntime,allowStart=false){
    ++autopilotEventRevision;
    if(!autopilotVersions.accept(runtime)||!mayUpdateAutopilot(autopilots[runtime.botId],runtime,allowStart))return;
    if(runtime.active)autopilots={...autopilots,[runtime.botId]:runtime};
    else{const next={...autopilots};delete next[runtime.botId];autopilots=next;}
  }
  function loadAutopilots(values:AutopilotRuntime[]){
    const ids=new Set(values.map(value=>value.botId));
    for(const id of Object.keys(autopilots))if(!ids.has(id)){const next={...autopilots};delete next[id];autopilots=next;}
    for(const runtime of values)acceptAutopilot(runtime,true);
  }
  function previewAutopilot(){
    if(autopilotPreview?.botId===profile.id){autopilotPreview=undefined;return;}
    clearEmotes();autopilotPreview={botId:profile.id,active:true,wander:!!profile.autopilot.wander,gestures:!!profile.autopilot.gestures,startedAt:Date.now(),seed:crypto.randomUUID(),revision:0};
    void tick().then(()=>document.querySelector('.autopilot-stage')?.scrollIntoView({block:'center',behavior:'smooth'}));
  }
  function invalidateNativePlayback(botId?: string, nativeFinished=false, keepAdmission=false) {
    if(!keepAdmission)lifeAdmissions.cancel(botId);
    const previous=nativeLifeTurn;
    if(!previous||botId&&previous.botId!==botId)return;
    nativeLifeTurn=undefined;nativeReplyId='';
    const completed=nativeFinished&&nativePlaybackCompleted===previous.turnId;nativePlaybackCompleted='';
    if(answeringAutopilot&&answeringBotId===previous.botId&&answeringSpeechId===previous.turnId){
      // A confirmed playback end keeps its normal subtitle/emote tail. Stop,
      // cancellation and any unfinished output still invalidate immediately.
      if(!completed){++turnEpoch;player?.stop();subtitleLifetime?.cancel(previous.turnId);clearEmotes();}
      finishPlayback(previous.turnId);
      busy=false;speechPreparing=false;speaking=false;level=0;partial='';answeringBotId='';answeringSpeechId='';answeringAutopilot=false;
    }
  }
  function acceptLifeRuntime(value:unknown) {
    if(lifeStopping)return;
    try{const next=lifeSnapshots.accept(value);if(!next)return;lifeRuntime=next;lifeError='';if(nativeLifeTurn&&!currentLifeTurn(nativeLifeTurn,next))invalidateNativePlayback(undefined,true,true);}
    catch(error){lifeRuntime=undefined;lifeError=prettyError(error);invalidateNativePlayback();}
  }
  async function refreshLifeBot(target=profile.id) {
    if(!native)return;
    try{const value=parseLifeBot(await call('get_life_bot',{botId:target}));if(appAlive)lifeBots={...lifeBots,[target]:value};}
    catch(error){if(appAlive&&target===profile.id)lifeError=prettyError(error);}
  }
  async function refreshLifeRuntime() {
    if(!native)return;
    try{acceptLifeRuntime(await call('get_life_runtime'));await refreshLifeBot();}
    catch(error){lifeRuntime=undefined;lifeError=prettyError(error);invalidateNativePlayback();}
  }
  async function reconnectLifeUI() {
    if(!native||!lifeUi)return;
    try{await lifeUi.connect();await refreshLifeRuntime();await lifeUi.report(!!autopilotBlocker||!lifeRuntime,true);lifeError='';}
    catch(error){lifeError=prettyError(error);}
  }
  async function ensureOperatorBarrier() {
    if(!native)return;
    ++operatorBarriers;
    try{
    const revision=++operatorRevision;nativeOperatorPending=true;invalidateNativePlayback();
    try{if(!lifeUi)throw new Error('Studio unavailable');await lifeUi.report(true,true);if(revision===operatorRevision)nativeOperatorPending=false;}
    catch{
      // A failed reporter is never a clear status. Only confirmed native Stop
      // can let this explicit operator action continue without the reporter.
      await call('stop_generation');invalidateNativePlayback();autopilots={};lifeRuntime=undefined;
      if(revision===operatorRevision)nativeOperatorPending=false;
      notify(translate('Die native Autonomie wurde gestoppt, weil der Studio-Status nicht bestätigt werden konnte. Dein Auftrag kann fortfahren.'));
    }
    }finally{--operatorBarriers;}
  }
  async function toggleLife(mode:LifeMode,active:boolean,initialize=false){
    if(!active)worldPanel?.cancelPendingStart();
    if(active&&(autopilotPending||updateInstalling||worldBusy))return;
    if(active&&worldDirty){notify(translate('Speichere oder verwirf zuerst den gemeinsamen Bühnenentwurf.'));return;}
    if(active&&(busy||speaking||speechPreparing||microphone||microphoneStarting||transcribing)){notify(translate("Beende zuerst die laufende Antwort oder Aufnahme."));return;}
    if(active&&mode==='ambient'&&profile.autopilot.speak&&!profile.model.model&&!cliProvider){navigate('models');notify(translate("Wähle zuerst ein Dialogmodell oder schalte selbstständiges Sprechen aus."));return;}
    const before=lifeRuntime?.sessions??[];
    unlockEmoteAudio();const target=profile.id;const revision=++autopilotStartRevision;autopilotPending=active;
    const epoch=turnEpoch;
    const startContext=()=>lifeStartContext(profileSnapshot(),$state.snapshot(documents));
    const context=startContext();const current=()=>appAlive&&context===startContext()&&revision===autopilotStartRevision&&epoch===turnEpoch&&target===profile.id&&!updateInstalling;
    try{
      if(!active)invalidateNativePlayback(target);
      if(active)await ensureOperatorBarrier();
      if(active&&profile.autopilot.speak&&(profile.model.model||cliProvider)&&!await ensureScreenFor([profileSnapshot()],true,current))return;
      if(active&&!await saveCurrentBot())return;
      if(active&&!current())return;
      const runtime=await call('set_life_mode',{botId:target,mode,active,initialize:active&&initialize});
      if(active&&!current()){await cancelStartedSessions(runtime,[target],before);return;}
      acceptLifeRuntime(runtime);autopilotPreview=undefined;await refreshLifeBot(target);
      notify(translate(active?'Gewählter Modus gestartet. Selbstständige Sprache wartet auf eine ruhige und freigegebene Phase.':'Selbstständiger Modus gestoppt.'));
    }catch(error){if(!active||current()){lifeError=prettyError(error);notify(lifeError,true);}}finally{if(revision===autopilotStartRevision)autopilotPending=false;}
  }
  async function cancelStartedSessions(runtime:unknown,members:string[],before:LifeSnapshot['sessions']){
    const next=parseLifeSnapshot(runtime);
    const created=next.sessions.filter(session=>members.includes(session.botId)&&!before.some(old=>old.botId===session.botId&&old.sessionId===session.sessionId&&old.revision===session.revision));
    const results=await Promise.allSettled(created.map(session=>call('stop_life_session_exact',{botId:session.botId,sessionId:session.sessionId,revision:session.revision})));
    for(const result of results)if(result.status==='rejected')log(prettyError(result.reason),'error');
  }
  async function startLifeScene(){
    if(!native||autopilotPending||updateInstalling||busy||speaking||speechPreparing||microphone||microphoneStarting||transcribing)throw actionChanged();
    const before=lifeRuntime?.sessions??[];
    const target=profile.id,epoch=turnEpoch,revision=++autopilotStartRevision;
    const startContext=()=>lifeStartContext(profileSnapshot(),$state.snapshot(documents));
    const context=startContext();
    const current=()=>appAlive&&profile.id===target&&epoch===turnEpoch&&revision===autopilotStartRevision&&!updateInstalling&&context===startContext();
    autopilotPending=true;unlockEmoteAudio();
    try{
      await tick();if(!current())throw actionChanged();
      if(worldDirty)throw new Error(translate('Speichere oder verwirf zuerst den gemeinsamen Bühnenentwurf.'));
      const settings=parseWorldSettings(await call('get_world_settings'));
      if(!current())throw actionChanged();
      const record=settings.record;
      if(settings.availability!=='ready'||!record?.config.members.length)throw new Error(translate('Wähle zuerst mindestens eine Figur für deinen Desktop aus.'));
      const participants=lifeSceneParticipants(record.config.members);
      if(participants.overLimit)throw new Error(translate(LIFE_SCENE_OVER_LIMIT,{count:participants.count,limit:participants.limit}));
      await ensureOperatorBarrier();
      const configs=record.config.members.map(id=>id===target?profileSnapshot():$state.snapshot(bots.find(bot=>bot.id===id)?.profile)).filter(value=>value?.autopilot?.speak);
      if(!await ensureScreenFor(configs,true,current)||!current())throw actionChanged();
      if(!await saveCurrentBot()||!current())throw actionChanged();
      const runtime=await call('start_life_scene',{expectedRevision:record.config.revision});
      if(!current()){await cancelStartedSessions(runtime,record.config.members,before);throw actionChanged();}
      acceptLifeRuntime(runtime);autopilotPreview=undefined;await refreshLifeBot(target);
      notify(translate('Life auf dem Desktop gestartet.'));
    }catch(error){if(current())lifeError=prettyError(error);throw error;}
    finally{if(revision===autopilotStartRevision)autopilotPending=false;}
  }
  function toggleAutopilot(active:boolean){return toggleLife('ambient',active);}
  function acceptOutputUpdate(output:any){
    const baseline=JSON.parse(savedProfile);baseline.output??={};
    for(const [key,value] of Object.entries(output||{})){
      if(JSON.stringify(profile.output[key])===JSON.stringify(baseline.output[key]))profile.output[key]=value;
      baseline.output[key]=value;
    }
    savedProfile=JSON.stringify(baseline);
  }


  let extensionEditing = $state(false);
  let liveAvatar = $state<{canPresent:(event:EmotePayload)=>boolean;present:(event:EmotePayload)=>boolean}>();
  function emoteAvailable(emote:any){return Boolean(resolveAvailableAction(availableEmoteActions,emote.action));}
  function admittedEmotes(events:EmotePayload[]=[]){return events.filter(event=>liveAvatar?.canPresent(event));}
  let modelInfo = $state<any>();
  let modelInfoError = $state('');
  let renderedModel = $state<AvatarModelInfo>({status:'loading',animations:[],proceduralGestures:[]});
  let hostEmoteCapabilities=$state<HostCapabilities>();
  let hostEmoteError=$state('');
  const hostEmoteRequests=new HostCapabilityRequests(value=>hostEmoteCapabilities=value);
  let builtinModelInfo: Record<string,any> | undefined;
  let switchingBot = $state(false);
  let messageInput = $state<HTMLInputElement>();
  let outputDisplays=$state<OutputDisplay[]>([]);
  let displayRequest=0;
  async function refreshOutputDisplays(){if(!native)return;const revision=++displayRequest;try{const value=await call<OutputDisplay[]>('list_output_displays');if(revision===displayRequest)outputDisplays=value;}catch(error){if(revision===displayRequest)notify(prettyError(error),true);}}
  const outputPreviews=new OutputPreviewQueue(async preview=>{
    if(!native)return;
    if(preview.field==='scale')return call('preview_avatar_scale',{botId:preview.botId,value:preview.value});
    return call('preview_output_monitor',{botId:preview.botId,monitorId:preview.value});
  },error=>notify(prettyError(error),true));
  function previewOutput(value:OutputPreview){
    if(value.botId!==profile.id)return;
    if(value.field==='scale'&&validScale(value.value))profile.avatar.scale=value.value;
    else if(value.field==='monitor'&&typeof value.value==='string')profile.output.desktopMonitorId=value.value;
    else return;
    outputPreviews.put(value);
  }
  let profile = $state<any>(cloneDefaults());
  let savedProfile = $state(JSON.stringify(cloneDefaults()));
  const availableEmoteActions=$derived(uiEmoteActions(profile,liveAvatar?renderedModel:undefined,hostEmoteCapabilities));
  const selectedAutopilot = $derived(autopilots[profile.id]);
  const selectedLifeSession = $derived(lifeRuntime?.sessions.find(session=>session.botId===profile.id));
  const motionRuntime = $derived(selectedAutopilot?.active ? selectedAutopilot : autopilotPreview?.botId===profile.id ? autopilotPreview : undefined);

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
  let connectionOwner = $state<Record<string,string>>({});
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
  let subtitleText = $state('');
  let hadSubtitle = $state(false);
  let subtitleLifetime: SubtitleLifetime;
  let message = $state('');
  let toast = $state('');
  let error = $state(false);
  let instructionTab = $state<'agents'|'persona'>('agents');
  let modelList = $state<string[]>([]);
  let loadingModels = $state(false);
  let checkingCli = $state(false);
  let cliStatus = $state('');
  type CliModel = {id:string;name:string;description?:string;supportedReasoningEfforts:string[];defaultReasoningEffort?:string;isDefault?:boolean;inputModalities?:string[]};
  let cliModels = $state<CliModel[]>([]);
  let cliConfiguredModel = $state('');
  let cliCatalogKey = $state('');
  let cliCatalogNote = $state('');
  let loadingCliModels = $state(false);
  let customCliModel = $state(false);
  let applyingFast = $state(false);
  let transcribing = $state(false);
  let wakeStopSignal = $state(0);
  let systemVoices = $state<SpeechSynthesisVoice[]>([]);
  let microphone = $state(false);
  let microphoneStarting = $state(false);
  const microphoneSession = new MicrophoneSession<MediaStream>(pending=>microphoneStarting=pending);
  let player: SpeechPlayer;
  let emoteSequence: EmoteSequence;
  let emoteAudio: EmoteAudioPlayer;
  let modelAudioStop = $state(0);
  let modelSoundPlaying = false;
  let modelSoundActivity: SoundActivity = 'foreground';
  let effectSoundPlaying = false;
  function updateSoundPlaying(){
    emoteSoundPlaying=modelSoundPlaying||effectSoundPlaying;
    foregroundEmotePlaying=foregroundSoundPlaying(effectSoundPlaying,modelSoundPlaying,modelSoundActivity);
  }
  function modelAudioPlaying(value:boolean,activity:SoundActivity){modelSoundPlaying=value;modelSoundActivity=activity;updateSoundPlaying();}
  let emoteSoundPlaying = $state(false);
  let foregroundEmotePlaying = $state(false);
  let emoteRevision = $state(0);
  let automaticEmoteRevision = $state(0);
  let externalEmotePlaying = $state(false);
  let externalEmoteBlocking = $state(false);
  let externalEmoteRevision = $state(0);
  let externalEmotes: ExternalEmotePlayback;
  let emoteOutput = {botId:"",audio:false};
  let recorder: MediaRecorder | null = null;
  let microphoneStream: MediaStream | null = null;
  let recordingTimer: ReturnType<typeof setTimeout> | undefined;
  let turnEpoch = 0;
  const playbackWaiters = new PlaybackWaiters();
  let answeringSpeechId = $state('');
  const presentedReplies = new Set<string>();
  const undisplayedReplies = new Set<string>();
  function clearEmotes(){modelAudioStop++;++emoteRevision;emoteSequence?.stop();emoteAudio?.stop();profile.avatar.emote=undefined;}
  function applyEmote(emote:EmotePayload){
    if(emoteOutput.botId!==profile.id)return;
    presentAdmittedEmote(liveAvatar,emote,()=>{profile.avatar.emote={...emote,action:emote.action,nonce:emote.nonce||crypto.randomUUID(),duration:(emote.duration||2.5)*1000,intensity:emote.intensity??1};
    if(emoteOutput.audio)emoteAudio?.play(emote);});
  }
  function unlockEmoteAudio(){if((profile.emotes||[]).some((emote:any)=>emote.sound))void emoteAudio?.unlock();}
  async function showManualEmote(event:any){
    clearEmotes();const revision=emoteRevision;const targetBot=profile.id;
    emoteOutput={botId:targetBot,audio:resolvedOutput(profile)==='studio'};
    if(!liveAvatar?.canPresent(event))return;
    if(emoteOutput.audio)await emoteAudio?.prepare([event],soundId=>call('get_emote_sound',{soundId,botId:targetBot}));
    if(revision!==emoteRevision||targetBot!==profile.id)return;
    applyEmote(event);
  }
  function presentReply(reply:any){
    if(!reply.id||presentedReplies.has(reply.id))return;
    undisplayedReplies.delete(reply.id);
    presentedReplies.add(reply.id);
    if(presentedReplies.size>128)presentedReplies.delete(presentedReplies.values().next().value!);
    const botId=reply.botId||answeringBotId;
    const name=botId===profile.id?profile.name:bots.find(bot=>bot.id===botId)?.name||'Cohost';
    if(botId===answeringBotId)speechPreparing=false;
    if(botId===profile.id){
      hadSubtitle=true;subtitleLifetime?.start(reply.id,reply.text);
      emotion=reply.emotion||'neutral';
      if(!history.some(item=>item.id===reply.id))history=[...history,{id:reply.id,role:'assistant',text:reply.text,source:reply.source||'operator',user:name,createdAt:new Date().toISOString()}];
      if(automaticEmoteRevision===emoteRevision)emoteSequence?.start(reply.emotes||[]);
    }
    log(`${name}: ${reply.text}`);
  }
  function finishPlayback(id?:string) { playbackWaiters.finish(id); }
  function waitForPlayback(id:string,botId:string):Promise<void> {
    const epoch=turnEpoch;
    return playbackWaiters.wait(id,botId,(jobId,target)=>call('cancel_speech_job',{id:jobId,botId:target}),()=>{
      if(epoch!==turnEpoch||answeringSpeechId!==id||answeringBotId!==botId)return;
      running=false;speaking=false;subtitleLifetime?.cancel(id);
      log(translate("Keine Abschlussmeldung der Sprachausgabe. Automatische Antworten pausiert."),'error');
    });
  }
  function profileSnapshot(){const p=$state.snapshot(profile);delete p.avatar.emote;return p;}
  const dirty = $derived(JSON.stringify(profileSnapshot()) !== savedProfile);
  const documentDirty = $derived(JSON.stringify(documents)!==JSON.stringify(savedDocuments));
  let extensionDirty = $state(false);
  let extensionBusy = $state(false);
  type Confirmation = {title:string;message:string;confirmLabel:string;action:()=>void|Promise<void>;danger?:boolean;dismissBeforeAction?:boolean};
  let confirmation = $state<Confirmation>();
  let confirming = $state(false);
  const hasUnsavedChanges = $derived(dirty||documentDirty||extensionDirty||worldDirty||apiAccessDirty||lifePreferencesDirty);
  function confirmWorldDiscard(action:()=>void){confirmation={title:translate('Bühnenentwurf verwerfen?'),message:translate('Nicht gespeicherte Bühnenangaben und Positionsentwürfe werden verworfen. Der bestätigte native Zustand bleibt bestehen.'),confirmLabel:translate('Entwurf verwerfen'),action,danger:true,dismissBeforeAction:true};}
  function requestDiscard(action:()=>void) {
    confirmation={title:translate('Entwurf verwerfen?'),message:translate('Deine Änderungen in der Bibliothek wurden noch nicht gespeichert. Beim Fortfahren gehen sie verloren.'),confirmLabel:translate('Entwurf verwerfen'),action,danger:true,dismissBeforeAction:true};
  }
  function navigate(next:Page) {
    if(next===page)return;
    if(apiAccessBusy){notify(translate('Die API-Zugänge werden gerade aktualisiert. Bitte kurz warten.'));return;}
    if(extensionBusy){notify(translate('Die Bibliothek wird gerade aktualisiert. Bitte kurz warten.'));return;}
    const proceed=()=>{botMenu=false;page=next;extensionDirty=false;usageBudgetDirty=false;};
    if(page==='api-access'&&apiAccessDirty){confirmation={title:translate('Zugangsentwurf verwerfen?'),message:translate('Der Entwurf und ein noch nicht gespeicherter Schlüssel werden verworfen. Bereits gespeicherte Verbindungen bleiben bestehen.'),confirmLabel:translate('Entwurf verwerfen'),action:()=>{apiPanel?.discardDraft();proceed();},danger:true,dismissBeforeAction:true};}
    else if(page==='autopilot'&&usageBudgetDirty){confirmation={title:translate('Budgetentwurf verwerfen?'),message:translate('Die neuen Tagesgrenzen wurden noch nicht gespeichert. Beim Verlassen gelten weiterhin die bisherigen Grenzen.'),confirmLabel:translate('Entwurf verwerfen'),action:proceed,danger:true,dismissBeforeAction:true};}
    else if(page==='extensions'&&extensionDirty)requestDiscard(proceed);else proceed();
  }
  async function confirmAction(){
    if(!confirmation||confirming)return;
    const current=confirmation;confirming=true;
    try{if(current.dismissBeforeAction){confirmation=undefined;await tick();}await current.action();if(confirmation===current)confirmation=undefined;}
    catch(error){notify(prettyError(error),true);}
    finally{confirming=false;}
  }
  const nativeOwnOutput = $derived(!!nativeLifeTurn&&answeringAutopilot&&answeringBotId===nativeLifeTurn.botId&&answeringSpeechId===nativeLifeTurn.turnId);
  const nativeOwnSound = $derived(nativeOwnOutput&&automaticEmoteRevision===emoteRevision);
  const externalForeignSound = $derived.by(()=>{externalEmoteRevision;return externalEmotes?.blocksExcept(nativeOwnOutput&&nativeLifeTurn?{botId:nativeLifeTurn.botId,output:nativeLifeOutput,speechId:nativeLifeTurn.turnId}:undefined)??externalEmoteBlocking;});
  const autopilotBlocker = $derived(speechBlocker({
    permission:!!$systemPermission, update:updateInstalling, starting:autopilotPending, reply:(busy||speaking||speechPreparing)&&!nativeOwnOutput,
    sound:foregroundEmotePlaying&&!nativeOwnSound||externalForeignSound, recording:microphone||microphoneStarting,
    transcription:transcribing, saving:saving||worldBusy||apiAccessBusy||lifePreferencesBusy, botChange:switchingBot||creatingBot,
    extensions:extensionEditing||extensionBusy, botDialog:botDialog||!!confirmation||confirming||shortcutHelp||$operatorDialogActive, updatesDialog:updatesOpen, unsaved:hasUnsavedChanges||usageBudgetDirty,
    draft:!!message.trim(), secretDraft:Object.values(secretInputs).some(value=>!!value.trim()),
  }));
  const autopilotSpeechStatus = $derived(speechStatus({
    active:!!selectedLifeSession?.active, speak:profile.autopilot.speak===true,
    outputAvailable:profile.output.mode==='studio'||bots.some(bot=>bot.id===profile.id&&botEnabled(bot)),
    modelConfigured:['claude-code','codex'].includes(profile.model.provider)||typeof profile.model.model==='string'&&!!profile.model.model.trim(),
    blocked:autopilotBlocker, schedule:nativeSchedule(selectedLifeSession),
  }));
  const lifeBlocked = $derived(!!autopilotBlocker||!lifeRuntime||nativeOperatorPending);
  // The click's transient nativeOperatorPending flag is acknowledged by the
  // exact reportProof below. Real blockers and other operator barriers remain.
  const quietRestBlocked = $derived(!!autopilotBlocker||!lifeConnected||!lifeRuntime||!selectedLifeSession?.active||selectedLifeSession.mode!=='life'||lifeRuntime.sessions.some(session=>session.speech.inFlight)||!!nativeLifeTurn||busy||speaking||speechPreparing||emoteSoundPlaying||externalEmotePlaying||wakeActive||operatorBarriers>0);
  async function startQuietRest(){
    if(quietRestBlocked||!lifeUi)throw new Error(translate('Beende zuerst Eingaben, Ausgaben und offene Änderungen. Eine stille Pause erfordert einen bereits gestarteten Life-Modus.'));
    invalidateNativePlayback();
    const target=profile.id,revision=operatorRevision,epoch=turnEpoch,reporter=lifeUi;
    const current=()=>appAlive&&profile.id===target&&operatorRevision===revision&&turnEpoch===epoch&&lifeUi===reporter&&!quietRestBlocked;
    if(!current())throw actionChanged();
    return requestQuietRestForOutput(call,reporter,target,current,()=>{nativeOperatorPending=false;lifeActivity=false;});
  }
  async function registerWorld(request:Omit<RegisterWorldActor,'studioProof'>){
    const reporter=lifeUi,target=profile.id,revision=operatorRevision,epoch=turnEpoch;
    const current=()=>appAlive&&page==='autopilot'&&lifeUi===reporter&&profile.id===target&&operatorRevision===revision&&turnEpoch===epoch&&!quietRestBlocked;
    if(!reporter||!current())throw new Error(translate('Die Verbindung zur sichtbaren Bühne ist nicht bestätigt.'));
    return reporter.reportProof(false,false,proof=>{
      if(!current())return Promise.reject(actionChanged());
      nativeOperatorPending=false;lifeActivity=false;
      return call('register_world_output',{request:{...request,studioProof:proof}});
    });
  }
  async function startWorldMove(targetPoint:CanonicalPoint,nativeOwner=false){
    if(quietRestBlocked||!lifeUi)throw new Error(translate('Beende zuerst Eingaben, Ausgaben und offene Änderungen. Eine sichtbare Bewegung erfordert einen bereits gestarteten Life-Modus.'));
    invalidateNativePlayback();
    const target=profile.id,revision=operatorRevision,epoch=turnEpoch,reporter=lifeUi;
    const current=()=>appAlive&&page==='autopilot'&&profile.id===target&&operatorRevision===revision&&turnEpoch===epoch&&lifeUi===reporter&&!quietRestBlocked;
    if(nativeOwner)return requestNativeLifeMove(call,target,targetPoint,current);
    return requestLifeMove(call,reporter,target,targetPoint,current,()=>{nativeOperatorPending=false;lifeActivity=false;});
  }
  async function startWorldSlot(slotId:string){
    if(quietRestBlocked)throw actionChanged();
    invalidateNativePlayback();const target=profile.id,revision=operatorRevision,epoch=turnEpoch;
    const current=()=>appAlive&&page==='autopilot'&&profile.id===target&&operatorRevision===revision&&turnEpoch===epoch&&!quietRestBlocked;
    return requestNativeLifeSlot(call,target,slotId,current);
  }
  async function reportLife(activity=false) {
    if(!native||!lifeMounted||!lifeUi)return;
    const observedActivity=activity||lifeActivity,revision=operatorRevision;lifeActivity=false;
    try{await lifeUi.report(lifeBlocked,observedActivity);if(observedActivity&&revision===operatorRevision)nativeOperatorPending=false;}
    catch(error){lifeActivity||=observedActivity;lifeError=prettyError(error);}
  }
  $effect(()=>{const blocked=lifeBlocked;if(lifeMounted&&native&&lifeConnected)void lifeUi.report(blocked,false).catch(error=>lifeError=prettyError(error));});
  $effect(()=>{const botId=profile.id;if(lifeMounted&&native)void refreshLifeBot(botId);});
  const updateBusyReason = $derived(
    $systemPermission ? $t('Schließe zuerst die Berechtigungsabfrage.') :
    busy || speaking || speechPreparing || emoteSoundPlaying || externalEmotePlaying ? $t("Beende zuerst laufende Antworten, Sprachausgaben und Emote-Sounds.") :
    microphone || microphoneStarting || transcribing || wakeActive ? $t("Beende zuerst die Mikrofonaufnahme und das Zuhören mit Aktivierungsphrase (Stop).") :
    Object.keys(autopilots).length||lifeRuntime?.sessions.some(session=>session.active) ? $t("Pausiere zuerst alle Autopiloten.") :
    running ? $t("Pausiere zuerst die automatischen Chat-Antworten.") :
    hasUnsavedChanges || usageBudgetDirty || message.trim() || Object.values(secretInputs).some(value=>!!value.trim()) ? $t("Speichere deine Änderungen und sende oder leere Entwürfe vor der Installation.") :
    saving || worldBusy || apiAccessBusy || lifePreferencesBusy || switchingBot || creatingBot || autopilotPending || extensionEditing || botDialog ? $t("Schließe zuerst laufende Bearbeitungen und Speichervorgänge.") : ''
  );
  const currentPage = $derived(nav.find(n => n.id === page));

  const activeStatus = $derived(speaking ? 'Spricht gerade' : speechPreparing ? 'Bereitet Sprache vor' : busy ? 'Antwortet gerade' : selectedAutopilot?.active ? 'Autopilot läuft' : running ? 'Hört zu' : 'Bereit, wenn du es bist');
  const connectedCount = $derived(Object.values(connection).filter(s => s === 'connected').length);
  const cliProvider = $derived(['claude-code','codex'].includes(profile.model.provider));
  const cliKey = $derived(`${profile.model.provider}:${profile.model.command?.trim()||profile.model.provider}`);
  const catalogModels = $derived(cliCatalogKey===cliKey?cliModels:[]);
  const pickerCliModels = $derived(catalogModels.length?catalogModels:profile.model.provider==='claude-code'?[{id:'haiku',name:$t('Haiku · schneller Dialog'),supportedReasoningEfforts:[]},{id:'sonnet',name:$t('Sonnet · ausgewogen'),supportedReasoningEfforts:[]},{id:'opus',name:$t('Opus · anspruchsvolle Aufgaben'),supportedReasoningEfforts:[]}]:[]);
  const selectedCliModel = $derived(catalogModels.find(model=>model.id===(profile.model.model==='default'&&cliCatalogKey===cliKey?cliConfiguredModel:profile.model.model)));
  const cliEfforts = $derived(selectedCliModel?.supportedReasoningEfforts||[]);
  const cliPickerValue = $derived(customCliModel||profile.model.model&&profile.model.model!=='default'&&!pickerCliModels.some(model=>model.id===profile.model.model)?'__custom__':profile.model.model||'default');
  const effortLabels:Record<string,string>={none:'Ohne Denkphase (none)',minimal:'Minimal (minimal)',low:'Niedrig · kürzer nachdenken (low)',medium:'Mittel (medium)',high:'Hoch (high)',xhigh:'Sehr hoch (xhigh)',max:'Maximal (max)',ultra:'Ultra (ultra)'};
  function botEnabled(bot:any){return !!bot.live||bot.profile?.output?.mode==='obs'&&!!bot.profile?.output?.streamingEnabled;}
  const liveBots = $derived(bots.filter(botEnabled));
  const microphoneProviderLabels: Record<string,string> = { 'whisper-local':'Lokales Whisper', openai:'OpenAI Whisper · Cloud', elevenlabs:'ElevenLabs · Cloud', compatible:'Konfigurierte STT-API' };
  const microphoneProviderName = $derived($t(microphoneProviderLabels[profile.stt.provider] || 'Keine Spracherkennung'));
  const filteredCharacters = $derived(characterCatalog.filter(character=>`${character.name} ${$t(character.description)} ${character.author}`.toLocaleLowerCase($locale).includes(characterSearch.trim().toLocaleLowerCase($locale))));
  const selectedCharacter = $derived(characterCatalog.find(character=>character.assetPath===profile.avatar.assetPath));

  $effect(()=>{
    const binding=avatarBinding(profile);const botId=profile.id;
    let savedBinding='';try{savedBinding=JSON.stringify(avatarBinding(JSON.parse(savedProfile)));}catch{/* An unsaved draft cannot authorize an external output. */}
    hostEmoteRequests.invalidate();hostEmoteError='';
    if(!native||savedBinding!==JSON.stringify(binding))return;
    let cancelled=false;
    void hostEmoteRequests.refresh(binding,()=>call('get_emote_capabilities',{botId})).catch(error=>{if(!cancelled)hostEmoteError=prettyError(error);});
    return()=>{cancelled=true;hostEmoteRequests.invalidate();};
  });

  let previousEmoteConfiguration='';
  $effect(()=>{
    const key=JSON.stringify([avatarBinding(profile),(profile.emotes||[]).map(emoteBinding),profile.output.mode]);
    if(previousEmoteConfiguration&&previousEmoteConfiguration!==key)untrack(clearEmotes);
    previousEmoteConfiguration=key;
  });

  $effect(()=>{
    if(page!=='character')return;
    const assetPath=profile.avatar.assetPath||null;const botId=profile.id;let cancelled=false;
    modelInfo=undefined;modelInfoError='';
    const request=async()=>{
      if(native)return call('get_avatar_info',{assetPath,botId});
      if(!builtinModelInfo){const response=await fetch('/character-capabilities.json');if(!response.ok)throw new Error(translate("Die mitgelieferten Modellinformationen konnten nicht geladen werden."));builtinModelInfo=(await response.json()).characters;}
      return builtinModelInfo?.[assetPath||'nova'];
    };
    void request().then(info=>{if(!cancelled)modelInfo=info;}).catch(error=>{if(!cancelled)modelInfoError=prettyError(error);});
    return()=>{cancelled=true;};
  });
  function previewModelAnimation(action:string,duration:number){
    if(busy||speaking||speechPreparing||updateInstalling)return;
    if(!resolveAvailableAction(availableEmoteActions,action))return;
    clearEmotes();profile.avatar.emote={action,nonce:crypto.randomUUID(),duration:Math.min(30,Math.max(0.4,Number.isFinite(duration)&&duration>0?duration:3))*1000,intensity:1};
    document.querySelector('.character-library')?.scrollIntoView({block:'start',behavior:'smooth'});
  }
  function addModelAnimation(action:string,duration:number,name:string){
    profile.emotes=[...(profile.emotes||[]),{id:'animation-'+crypto.randomUUID(),name:name.slice(0,80),action,duration:Math.min(action.startsWith('vrma:')?120:15,Math.max(0.001,Number.isFinite(duration)&&duration>0?duration:3)),intensity:1,allowChat:false}];
    notify(translate("Animation als Emote hinzugefügt. Änderungen speichern, um sie für diesen Bot zu übernehmen."));
  }

  function notify(text:string, isError=false) { toast=text;error=isError;setTimeout(()=>{if(toast===text)toast=''},6500); }
  function log(text:string,kind='info') {logs=[{id:crypto.randomUUID(),text,kind,time:new Date().toISOString()},...logs].slice(0,60);}
  function changeProvider() {
    const urls:Record<string,string>={ollama:'http://localhost:11434/v1',lmstudio:'http://localhost:1234/v1',openai:'https://api.openai.com/v1',anthropic:'https://api.anthropic.com/v1',compatible:'http://localhost:8000/v1'};
    const cli=['claude-code','codex'].includes(profile.model.provider);
    profile.model.baseUrl=urls[profile.model.provider]||'';profile.model.model=cli?'default':'';profile.model.command=profile.model.provider==='claude-code'?'claude':profile.model.provider==='codex'?'codex':'';profile.model.apiKeyId='';profile.model.reasoningEffort='default';modelList=[];cliStatus='';cliModels=[];cliConfiguredModel='';cliCatalogKey='';cliCatalogNote='';customCliModel=false;
  }
  function changeSttProvider(){
    const defaults:Record<string,any>={
      'whisper-local':{baseUrl:'http://127.0.0.1:8080',model:'',apiKeyId:'',localBackend:'whispercpp'},
      'openai':{baseUrl:'https://api.openai.com/v1',model:'whisper-1',apiKeyId:'stt-openai'},
      'elevenlabs':{baseUrl:'https://api.elevenlabs.io/v1',model:'scribe_v2',apiKeyId:'stt-elevenlabs'},
      'compatible':{baseUrl:'http://localhost:8000/v1',model:'whisper-1',apiKeyId:'stt-compatible'}
    };
    Object.assign(profile.stt,defaults[profile.stt.provider]);
    profile.stt.apiKeyId='';
  }
  function changeWhisperBackend(){profile.stt.baseUrl=profile.stt.localBackend==='compatible'?'http://127.0.0.1:8000/v1':'http://127.0.0.1:8080';profile.stt.model=profile.stt.localBackend==='compatible'?'whisper-1':'';}
  async function selectCli(){try{const path=await open({multiple:false,title:translate("CLI-Programm auswählen")});if(typeof path==='string'){profile.model.command=path;cliStatus='';}}catch(e){notify(prettyError(e),true);}}
  async function checkCli(){checkingCli=true;cliStatus='';try{const status=await call('check_cli_provider',{config:$state.snapshot(profile.model)});cliStatus=status.version;notify(translate("CLI gefunden. Der Login erfolgt separat im Terminal."));}catch(e){notify(prettyError(e),true);}finally{checkingCli=false;}}
  async function loadCliModels(quiet=false){
    if(loadingCliModels)return false;
    const key=cliKey;const botId=profile.id;const config=$state.snapshot(profile.model);loadingCliModels=true;
    try{
      const result=await call('list_cli_models',{config});
      if(key!==cliKey||botId!==profile.id)return false;
      cliModels=(result.models||[]).filter((model:any)=>model.id&&model.id!=='default');cliConfiguredModel=result.configuredModel||'';cliCatalogKey=key;
      cliCatalogNote=result.supportsModelListing?'Katalog der installierten Codex-CLI. Die Verfügbarkeit hängt vom CLI-Konto und Anbieter ab.':'Bekannte Claude-Modellnamen und dynamische CLI-Aliase. Dies ist keine Abfrage deines Kontozugangs.';
      if(!quiet)notify(translate('{count} Modelle und ihre Denkstufen geladen.',{count:formatNumber(cliModels.length)}));
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
        if(!model){notify(translate("Wähle zuerst ein konkretes Modell aus dem geladenen CLI-Katalog."),true);return;}
        const order=['none','minimal','low','medium','high','xhigh','max','ultra'];
        profile.model.reasoningEffort=order.find(effort=>model.supportedReasoningEfforts.includes(effort))||'default';
      }
      profile.model.responseStyle='short';profile.model.maxToolRounds=1;profile.memory.maxMessages=12;
      notify(translate("Schnellvorgabe eingestellt: kurz antworten, eine Werkzeugrunde, zwölf Verlaufseinträge. Zum Übernehmen speichern."));
    }finally{applyingFast=false;}
  }
  async function refresh() {
    if(!native)return;
    const initialAutopilotRevision=autopilotEventRevision;const initialStartRevision=autopilotStartRevision;
    const state = await call('get_app_state');
    const visibleReplies=new Set(history.filter(item=>item.role==='assistant').map(item=>item.id));
    if(profile.id!==state.profile.id){subtitleLifetime?.cancel();clearEmotes();hadSubtitle=false;}
    if(initialAutopilotRevision===autopilotEventRevision&&initialStartRevision===autopilotStartRevision)loadAutopilots(state.autopilot||[]);
    if(state.lifeRuntime)acceptLifeRuntime(state.lifeRuntime);
    if(Array.isArray(state.lifeOutputs))state.lifeOutputs.forEach(acceptLifeOutput);
    else if(state.lifeOutput)acceptLifeOutput(state.lifeOutput);
    profile=state.profile;profile.autopilot={...cloneDefaults().autopilot,...profile.autopilot};profile.stt.language??='de';profile.stt.localBackend??='whispercpp';profile.model.command??='';profile.model.reasoningEffort??='default';profile.model.responseStyle??='default';profile.model.maxToolRounds??=4;profile.model.visionEnabled??=true;
    profile.screenContext={enabled:profile.screenContext?.enabled===true,displayId:typeof profile.screenContext?.displayId==='string'?profile.screenContext.displayId:''};
    profile.hindsight={enabled:false,baseUrl:'http://127.0.0.1:8888',bankId:'',apiKeyId:'',autoRecall:true,autoRetain:true,...profile.hindsight};
    profile.wakeWord={enabled:false,phrases:[`hey ${profile.name.toLocaleLowerCase('de-DE')}`],...profile.wakeWord};customCliModel=false;
    savedProfile=JSON.stringify(profileSnapshot());
    library=state.library||{mcpServers:[],skills:state.skills||[]};
    documents=state.documents || documents;savedDocuments={...documents};bots=state.bots||[];skills=state.skills||[];knowledge=state.knowledge||[];
    history=(state.history||[]).filter((item:any)=>!undisplayedReplies.has(item.id)&&(!busy||answeringBotId!==state.profile.id||item.role!=='assistant'||visibleReplies.has(item.id)||presentedReplies.has(item.id)));
    void refreshApiConnections();
    secrets=state.secretIds||[];overlayUrl=state.overlayUrl||'';appDataDir=state.appDataDir||'';
  }
  async function refreshLibrary() {
    if(!native)return;
    const initialAutopilotRevision=autopilotEventRevision;const initialStartRevision=autopilotStartRevision;
    const state=await call('get_app_state');
    if(initialAutopilotRevision===autopilotEventRevision&&initialStartRevision===autopilotStartRevision)loadAutopilots(state.autopilot||[]);
    if(state.lifeRuntime)acceptLifeRuntime(state.lifeRuntime);
    if(Array.isArray(state.lifeOutputs))state.lifeOutputs.forEach(acceptLifeOutput);
    else if(state.lifeOutput)acceptLifeOutput(state.lifeOutput);
    library=state.library||{mcpServers:[],skills:state.skills||[]};bots=state.bots||bots;secrets=state.secretIds||secrets;
    if(state.profile.id===profile.id){
      profile.mcpServers=state.profile.mcpServers||[];profile.skillsEnabled=state.profile.skillsEnabled||[];
      const baseline=JSON.parse(savedProfile);baseline.mcpServers=structuredClone(state.profile.mcpServers||[]);baseline.skillsEnabled=structuredClone(state.profile.skillsEnabled||[]);savedProfile=JSON.stringify(baseline);
    }
  }
  async function persist(quiet=false) {
    saving=true;
    await outputPreviews.flush();
    if(cliProvider&&!profile.model.model?.trim())profile.model.model='default';
    const snapshot=profileSnapshot();const botId=profile.id;
    try { await call('save_profile',{profile:snapshot,botId});if(profile.id===botId)savedProfile=JSON.stringify(snapshot);bots=bots.map(bot=>bot.id===botId?{...bot,name:snapshot.name,profile:snapshot}:bot);if(!quiet)notify(translate("Änderungen gespeichert."));return true; }
    catch(e){notify(prettyError(e),true);return false;}finally{saving=false;}
  }
  async function prepareScreenSource(botId:string,current:()=>boolean):Promise<string|null> {
    if(saving||switchingBot||creatingBot)return null;
    saving=true;
    try {
      return await prepareScreenSession({
        current:()=>appAlive&&!updateInstalling&&!switchingBot&&!creatingBot&&profile.id===botId&&current(),
        context:permissionContext,
        flush:()=>outputPreviews.flush(),
        normalize:()=>{if(cliProvider&&!profile.model.model?.trim())profile.model.model='default';},
        save:async()=>{
          const snapshot=profileSnapshot();
          await call('save_profile',{profile:snapshot,botId});
          if(profile.id===botId)savedProfile=JSON.stringify(snapshot);
          bots=bots.map(bot=>bot.id===botId?{...bot,name:snapshot.name,profile:snapshot}:bot);
        },
      });
    } catch(cause){notify(prettyError(cause),true);return null;}
    finally{saving=false;}
  }
  async function saveCurrentBot(){
    const botId=profile.id;const edits=$state.snapshot(documents);const baseline=$state.snapshot(savedDocuments);
    if(!await persist(true))return false;
    try{for(const key of ['agents','persona'] as const){if(edits[key]!==baseline[key]){await call('save_document',{name:key==='agents'?'AGENTS.md':'PERSONA.md',content:edits[key],botId});if(profile.id===botId)savedDocuments[key]=edits[key];}}return true;}
    catch(e){notify(prettyError(e),true);return false;}
  }
  async function saveContext(){
    if(saving||worldBusy||apiAccessBusy||lifePreferencesBusy||switchingBot||extensionBusy)return;
    if(lifePreferencesDirty){if(experienceDirty)await experiencePanel?.saveDraft();if(socialDirty)await socialPanel?.saveDraft();if(careDirty)await carePanel?.saveDraft();return;}
    if(page==='api-access'){await apiPanel?.saveDraft();return;}
    if(page==='autopilot'&&worldDirty){await worldPanel?.saveDraft();return;}
    if(page==='extensions'){window.dispatchEvent(new CustomEvent('cohost:save-extension'));return;}
    if(await saveCurrentBot())notify(translate('Änderungen für {name} gespeichert.',{name:profile.name}));
  }
  async function handleShortcut(event:KeyboardEvent){
    if(event.defaultPrevented||event.isComposing||updateInstalling||updatesOpen)return;
    if(confirmation){if(event.key==='Escape'){event.preventDefault();if(!confirming)confirmation=undefined;}return;}
    const modal=!!document.querySelector('[role="dialog"],[role="alertdialog"]');
    if(event.key==='Escape'&&!modal&&botMenu){event.preventDefault();botMenu=false;document.querySelector<HTMLButtonElement>('.profile-switch')?.focus();return;}
    if(event.key==='Escape'&&!modal&&page==='extensions'&&extensionEditing){event.preventDefault();window.dispatchEvent(new CustomEvent('cohost:close-extension'));return;}
    const target=event.target instanceof HTMLElement?event.target:null;
    const editing=!!target?.closest('input,textarea,select,[contenteditable="true"],[role="textbox"]');
    const action=shortcutFor(event,{editing,modal,studio:page==='studio'});if(!action)return;
    event.preventDefault();
    if(typeof action==='object'){navigate(nav[action.page].id as Page);return;}
    if(action==='save'){await saveContext();return;}
    if(action==='dismiss'){if(shortcutHelp){shortcutHelp=false;return;}if(botDialog){botDialog=false;return;}botMenu=false;window.dispatchEvent(new CustomEvent('cohost:close-extension'));return;}
    if(action==='stop'){botMenu=false;await stopAll();return;}
    if(action==='help'){shortcutHelp=true;return;}
    if(action==='focus-message'){navigate('studio');await tick();messageInput?.focus();return;}
    if(action==='send'&&!busy&&!microphone&&!microphoneStarting&&!transcribing&&message.trim())await sendOwn();
  }
  async function selectBot(id:string){
    if(lifePreferencesDirty||lifePreferencesBusy){notify(translate('Speichere oder verwirf zuerst die Alltagsvorlieben.'));return;}
    if(busy||microphone||microphoneStarting||transcribing||switchingBot||creatingBot||saving||apiAccessBusy||autopilotPending){notify(translate("Beende zuerst die laufende Antwort, Aufnahme oder Speicherung."));return;}
    if(id===profile.id){botMenu=false;return;}
    systemPermission.cancel();chatScheduler.cancel();switchingBot=true;wakeStopSignal++;
    try{if(!await saveCurrentBot())return;player?.stop();await call('select_bot',{id});await refresh();cliStatus='';partial='';emotion='neutral';botMenu=false;log(translate('{name} zum Bearbeiten ausgewählt.',{name:profile.name}));}catch(e){notify(prettyError(e),true);}finally{switchingBot=false;}
  }
  async function createBot(){
    if(lifePreferencesDirty||lifePreferencesBusy){notify(translate('Speichere oder verwirf zuerst die Alltagsvorlieben.'));return;}
    if(!botName.trim()||creatingBot||switchingBot||saving||apiAccessBusy)return;
    systemPermission.cancel();chatScheduler.cancel();wakeStopSignal++;creatingBot=true;
    try{
      if(!await saveCurrentBot())return;
      await call('create_bot',{name:botName.trim(),copyCurrent:botCopy});await refresh();
      if(!botCopy)profile.wakeWord.phrases=['hey '+profile.name.toLocaleLowerCase('de-DE')];
      const character=characterCatalog.find(entry=>entry.id===botCharacter);
      if(character){profile.avatar.assetPath=character.assetPath;profile.emotes=structuredClone(character.emotes);await persist(true);}
      botDialog=false;botMenu=false;botName='';navigate('character');log(translate('{name} angelegt.',{name:profile.name}));notify(translate("Bot angelegt. Wähle sein Modell und seine Stimme."));
    }catch(e){notify(prettyError(e),true);}finally{creatingBot=false;}
  }
  async function toggleBotLive(bot:any){
    if(busy||microphone||microphoneStarting||transcribing){notify(translate("Beende zuerst die laufende Antwort oder Aufnahme."));return;}
    if(!await saveCurrentBot())return;
    try{await call(botEnabled(bot)?'close_overlay':'open_overlay',{botId:bot.id});await refresh();botMenu=false;notify(translate(botEnabled(bot)?'{name} ausgeblendet.':'{name} ist im eigenen Figurenfenster.',{name:bot.name}));}catch(e){notify(prettyError(e),true);}
  }
  async function copyBotObs(bot:any){
    if(busy){notify(translate("Beende zuerst die laufende Antwort."));return;}
    if(!await saveCurrentBot())return;
    try{const config=structuredClone(bot.id===profile.id?profileSnapshot():$state.snapshot(bot.profile));config.output.mode='obs';config.output.streamingEnabled=true;await call('close_overlay',{botId:bot.id});await call('save_profile',{profile:config,botId:bot.id});await refresh();const updated=bots.find(entry=>entry.id===bot.id);await navigator.clipboard.writeText(updated?.overlayUrl||bot.overlayUrl);notify(translate('OBS-Link für {name} kopiert.',{name:bot.name}));}catch(e){notify(prettyError(e),true);}
  }
  function chooseCharacter(character:any){clearEmotes();const defaults=new Set(characterCatalog.flatMap(entry=>entry.emotes.map(emote=>emote.id)));const own=(profile.emotes||[]).filter((emote:any)=>!defaults.has(emote.id));profile.avatar.assetPath=character.assetPath;profile.emotes=[...structuredClone(character.emotes),...own];profile.avatar.emote=undefined;notify(translate('{name} ausgewählt. Passende Emotes übernommen.',{name:character.name}));}
  async function storeSecret(id:string) {
    const value=secretInputs[id]?.trim();if(!value)return;
    try{await call('set_secret',{id,value});secrets=[...new Set([...secrets,id])];secretInputs[id]='';notify(translate("Zugang im Systemschlüsselbund gespeichert."));}catch(e){notify(prettyError(e),true);}
  }
  async function loadModels() {
    loadingModels=true;
    try{modelList=await call('list_models',{config:$state.snapshot(profile.model)});notify(translate('{count} Modelle verfügbar.',{count:formatNumber(modelList.length)}));}catch(e){notify(prettyError(e),true);}finally{loadingModels=false;}
  }
  let appAlive = true;
  function permissionContext() { return JSON.stringify([profile.id, profileSnapshot(), $state.snapshot(documents), turnEpoch]); }
  function permissionLease() {
    const context = permissionContext();
    return () => appAlive && !updateInstalling && context === permissionContext();
  }
  $effect(() => {
    // Config changes cancel an intent even while an OS prompt is unresolved.
    permissionContext(); JSON.stringify($state.snapshot(bots)); systemPermission.invalidate();
  });
  async function pauseForScreenPermission(botId?: string, error: unknown = null) {
    running = false; chatScheduler.cancelAutomatic(); ++wakeStopSignal;
    // The OS screen grant is app-wide. Pause every affected autonomous bot once
    // so closing recovery cannot produce a new denial dialog on the next tick.
    const affected = Object.keys(autopilots).filter(id => id === botId || (id === profile.id ? profile : bots.find(bot => bot.id === id)?.profile)?.screenContext?.enabled);
    const next = { ...autopilots };
    for (const id of affected) { delete next[id]; }
    autopilots = next;
    systemPermission.recover('screen', error);
    await Promise.all(affected.map(async id => {
      try { acceptAutopilot(await call<AutopilotRuntime>('set_autopilot', { botId: id, active: false })); }
      catch (failure) { if (appAlive) log(prettyError(failure), 'error'); }
    }));
  }
  async function ensureScreenFor(configs: any[], interactive: boolean, current: () => boolean): Promise<boolean> {
    if (!configs.some(config => config?.screenContext?.enabled)) return current();
    if (!current()) return false;
    if (interactive) return systemPermission.ensure('screen', current);
    try {
      const permission = await systemPermission.inspect('screen');
      if (!current()) return false;
      if (usablePermission('screen', permission)) return true;
      await pauseForScreenPermission(configs.find(config => config?.screenContext?.enabled)?.id);
    } catch (error) { if (current()) await pauseForScreenPermission(configs.find(config => config?.screenContext?.enabled)?.id, error); }
    return false;
  }
  async function toggleRunning(){
    if(updateInstalling||systemPermission.active)return;
    unlockEmoteAudio();
    if(running){running=false;chatScheduler.cancelAutomatic();notify(translate("Automatische Antworten pausiert."));return;}
    if(!profile.model.model&&!cliProvider&&!liveBots.some(bot=>bot.profile?.model?.model)){navigate('models');notify(translate("Wähle zuerst ein Dialogmodell aus."));return;}
    const current = permissionLease();
    const participants = bots.filter(bot => bot.id === profile.id || botEnabled(bot)).map(bot => bot.id === profile.id ? profileSnapshot() : $state.snapshot(bot.profile));
    const crewContext = JSON.stringify(participants);
    const crewCurrent = () => current() && crewContext === JSON.stringify(bots.filter(bot => bot.id === profile.id || botEnabled(bot)).map(bot => bot.id === profile.id ? profileSnapshot() : $state.snapshot(bot.profile)));
    if (!await ensureScreenFor(participants, true, crewCurrent) || !crewCurrent()) return;
    if(await persist(true)&&crewCurrent()){running=true;log(translate("Cohost gestartet."));}
  }
  async function stopAll(){worldPanel?.cancelPendingStart();autopilotPending=false;lifeAdmissions.cancel();systemPermission.cancel();chatScheduler.cancel();++autopilotStartRevision;autopilots={};autopilotPreview=undefined;nativeLifeTurn=undefined;nativeReplyId='';lifeStopping=true;lifeRuntime=undefined;externalEmotes?.clear();turnEpoch++;wakeStopSignal++;subtitleLifetime?.cancel();running=false;busy=false;speechPreparing=false;partial='';queue=[];clearTimeout(recordingTimer);microphoneSession.cancel();if(recorder?.state==='recording'){recorder.onstop=null;recorder.stop();}microphoneStream?.getTracks().forEach(t=>t.stop());microphone=false;transcribing=false;player?.stop();clearEmotes();finishPlayback();answeringSpeechId='';answeringBotId='';answeringAutopilot=false;speaking=false;level=0;try{await call('stop_generation');lifeStopping=false;await refreshLifeRuntime();}catch(e){if(native)notify(prettyError(e),true);}finally{lifeStopping=false;}log(translate("Sprache und ausstehende Antworten gestoppt."));}
  async function playReply(reply:any,botConfig:any,epoch:number,source:string) {
    const botId=botConfig.id;
    const output=reply.presentationOutput??resolvedOutput(botConfig);
    if(reply.presentationRouteId&&(lifeOutputs[botId]?.state!=='visible'||lifeOutputs[botId]?.routeId!==reply.presentationRouteId))throw new DOMException('Die native Ausgabe wurde abgebrochen.','AbortError');
    const nativeTurn=nativeLifeTurn?.turnId===reply.id?nativeLifeTurn:undefined;
    const beforeStart=nativeTurn?async()=>{
      acceptLifeRuntime(await call('get_life_runtime'));
      if(epoch!==turnEpoch||nativeOperatorPending||!currentLifeTurn(nativeTurn,lifeRuntime))throw new DOMException('Die native Ausgabe wurde abgebrochen.','AbortError');
    }:undefined;
      const voice=reply.voice;
      if(!voice)throw new Error('Stimme des Wiedergabeauftrags fehlt.');
      if(reply.id&&!presentedReplies.has(reply.id))undisplayedReplies.add(reply.id);
      if(epoch!==turnEpoch)return;
      partial='';
      speechPreparing=!!voice.enabled&&!presentedReplies.has(reply.id);
      // External renderers own text-only presentation too: their figure must
      // return before reporting a start, even when the voice is disabled.
      if(output==='studio'){
        // Publish the reply from the player's real start callback. Synthesis and
        // decoding can take seconds; tokens, text and emotes stay private until then.
        let reporting=Promise.resolve();
        const report=(state:boolean,error?:string)=>{
          reporting=reporting.then(()=>call('report_playback',{id:reply.id,botId,speaking:state,error})).catch(e=>{if(epoch===turnEpoch)log(prettyError(e),'error');});
          return reporting;
        };
        const begin=()=>{if(epoch!==turnEpoch||!matchesPlayback(reply,{id:answeringSpeechId,botId:answeringBotId}))return;presentReply({...reply,botId,source});if(botId!==profile.id&&automaticEmoteRevision===emoteRevision)emoteSequence?.start(reply.emotes||[]);void report(true);};
        let audioError:string|undefined;
        try{
          if(output==='studio'&&automaticEmoteRevision===emoteRevision)await emoteAudio.prepare(admittedEmotes(reply.emotes||[]),soundId=>call('get_emote_sound',{soundId,botId}));
          if(epoch!==turnEpoch)return;
          if(voice.enabled)await player.speak(voice.provider==='elevenlabs'?(reply.speechText||reply.text):reply.text,voice,async()=>call('get_speech_audio',{id:reply.id,botId}),begin,beforeStart);
          else{await beforeStart?.();if(epoch===turnEpoch)begin();}
          if(nativeTurn&&epoch===turnEpoch)nativePlaybackCompleted=reply.id;
        }catch(e){audioError=prettyError(e);throw e;}
        finally{if(epoch===turnEpoch){if(audioError){clearEmotes();subtitleLifetime?.cancel(reply.id);}else subtitleLifetime?.finish(reply.id);await report(false,audioError);}}
      }else if(reply.id)await waitForPlayback(reply.id,botId);
  }
  async function lifeTurn(value:unknown) {
    let event:LifeTurn;
    try{event=parseLifeTurn(value);}catch(error){lifeError=prettyError(error);return;}
    if(event.kind==='finished'){
      lifeAdmissions.finish(event);
      if(sameLifeTurn(nativeLifeTurn,event))invalidateNativePlayback(undefined,true,true);
      return;
    }
    if(event.kind==='started'){
      if(sameLifeTurn(nativeLifeTurn,event))return;
      await lifeAdmissions.start(event,async current=>{
        const epoch=turnEpoch,revision=operatorRevision;
        try{acceptLifeRuntime(await call('get_life_runtime'));}catch(error){lifeError=prettyError(error);return false;}
        if(!current()||!appAlive||epoch!==turnEpoch||revision!==operatorRevision||nativeOperatorPending||!currentLifeTurn(event,lifeRuntime)||autopilotBlocker)return false;
        const config=event.botId===profile.id?profileSnapshot():$state.snapshot(bots.find(bot=>bot.id===event.botId)?.profile);
        if(!config)return false;
        invalidateNativePlayback(undefined,false,true);nativeLifeTurn=event;nativeLifeOutput=resolvedOutput(config);nativeReplyId='';nativePlaybackCompleted='';++turnEpoch;
        clearEmotes();automaticEmoteRevision=emoteRevision;emoteOutput={botId:event.botId,audio:resolvedOutput(config)==='studio'};
        busy=true;answeringBotId=event.botId;answeringSpeechId=event.turnId;answeringAutopilot=true;speechPreparing=false;partial='';
        return true;
      });
      return;
    }
    if(!await lifeAdmissions.ready(event))return;
    if(!sameLifeTurn(nativeLifeTurn,event)||!currentLifeTurn(event,lifeRuntime)||!event.reply||nativeReplyId===event.turnId)return;
    nativeReplyId=event.turnId;
    const epoch=turnEpoch,botId=event.botId;
    const config=botId===profile.id?profileSnapshot():$state.snapshot(bots.find(bot=>bot.id===botId)?.profile);
    if(!config)return;
    try{
      if(event.reply.skipped){silentAutopilots={...silentAutopilots,[botId]:autopilots[botId]?.seed||event.sessionId};log(translate('{name}: Letzter Sprechversuch ohne Beitrag.',{name:config.name}));return;}
      const silent={...silentAutopilots};delete silent[botId];silentAutopilots=silent;
      nativeLifeOutput=event.reply.presentationOutput??resolvedOutput(config);
      emoteOutput={botId,audio:nativeLifeOutput==='studio'};
      await playReply(event.reply,config,epoch,'autopilot');
    }catch(error){if(epoch===turnEpoch){notify(`${config.name}: ${prettyError(error)}`,true);log(prettyError(error),'error');}}
    finally{if(epoch===turnEpoch){busy=false;answeringAutopilot=false;speechPreparing=false;partial='';answeringBotId='';answeringSpeechId='';}}
  }
  async function runMessage(text:string,source='operator',user='Streamer',botId=profile.id,replyRouteId?:string,interactive=true,onAccepted?:()=>void,chatClaim?:ChatClaim) {
    // Native Life owns every spontaneous provider dispatch. This function is
    // solely for explicit operator requests and the fair automatic chat queue.
    if(source==='autopilot'||!text.trim()||busy||updateInstalling||microphoneStarting||systemPermission.active)return;
    // A manual selection/operator send takes priority over automatic preflight.
    // Once playback owns busy, its existing serial presentation guards apply.
    const manualClaim=!chatClaim?chatScheduler.beginManual(botId):undefined;
    if(!chatClaim&&!manualClaim)return;
    try {
    const currentManual=()=>!manualClaim||chatScheduler.isCurrentManual(manualClaim);
    const currentChat=()=>{
      if(!chatClaim)return true;
      const candidate=chatParticipants(bots,profile).find(bot=>bot.botId===botId);
      return running&&chatScheduler.isCurrentClaim(chatClaim)&&!!candidate
        &&queue.some(entry=>entry===chatClaim.message)&&chatPriority(candidate,chatClaim.message,Date.now())!==undefined;
    };
    if(!currentChat()||!currentManual())return;
    await ensureOperatorBarrier();
    const selected=botId===profile.id;
    const botConfig=selected?profileSnapshot():$state.snapshot(bots.find(bot=>bot.id===botId)?.profile);
    if(!botConfig){notify(translate("Bot wurde nicht gefunden."),true);return;}
    if(!botConfig.model.model&&!['claude-code','codex'].includes(botConfig.model.provider)){
      if(selected)navigate('models');notify(translate('Wähle zuerst ein Dialogmodell für {name}.',{name:botConfig.name}));return;
    }
    const permissionCurrent = permissionLease();
    // Saving a selected CLI profile only normalizes its blank default model;
    // every other configuration change invalidates preflight before dispatch.
    const contextOf=(config:any)=>JSON.stringify(config&&['claude-code','codex'].includes(config.model?.provider)&&!config.model?.model?.trim()?{...config,model:{...config.model,model:'default'}}:config);
    const botContext = contextOf(botConfig);
    const sameBotConfig=()=>botContext===contextOf(botId===profile.id?profileSnapshot():$state.snapshot(bots.find(bot=>bot.id===botId)?.profile));
    const current = () => currentChat() && currentManual() && permissionCurrent() && sameBotConfig();
    if (!await ensureScreenFor([botConfig], interactive, current) || !current()) return;
    const requestId=crypto.randomUUID();
    const epoch=++turnEpoch;clearEmotes();automaticEmoteRevision=emoteRevision;emoteOutput={botId,audio:resolvedOutput(botConfig)==='studio'};busy=true;answeringBotId=botId;answeringSpeechId=requestId;answeringAutopilot=false;speechPreparing=false;partial='';
    try {
      if(selected&&(!await saveCurrentBot()||epoch!==turnEpoch))return;
      if(botConfig.output.mode==='overlay'&&!bots.find(bot=>bot.id===botId)?.live)await call('open_overlay',{botId});
      if(epoch!==turnEpoch||!currentChat()||!currentManual()||!sameBotConfig())return;
      onAccepted?.();
      if(selected)history=[...history,{id:crypto.randomUUID(),role:'user',text,source,user,createdAt:new Date().toISOString()}];
      const reply=chatClaim?await call('send_automatic_message',{text,source,user,botId,requestId,replyRouteId}):await call('send_message',{text,source,user,skillIds:[],botId,requestId,replyRouteId});
      if(epoch!==turnEpoch)return;
      if(reply.skipped)return;
      if(!matchesPlayback(reply,{id:answeringSpeechId,botId:answeringBotId}))throw new Error('Antwort gehört nicht zur aktuellen Anfrage.');
      await playReply(reply,botConfig,epoch,source);
    }catch(e){if(epoch===turnEpoch){
      let permissionBlocked=false;
      if(botConfig.screenContext?.enabled){
        try { const permission=await systemPermission.inspect('screen'); permissionBlocked=!usablePermission('screen',permission); } catch { /* Preserve the original error. */ }
      }
      if(epoch!==turnEpoch)return;
      if(permissionBlocked)await pauseForScreenPermission(botId,e);
      else notify(`${botConfig.name}: ${prettyError(e)}`,true);
      log(`${botConfig.name}: ${prettyError(e)}`,'error');
    }}
    finally{if(epoch===turnEpoch){busy=false;answeringAutopilot=false;speechPreparing=false;partial='';answeringBotId='';answeringSpeechId='';}}
    } catch(error){notify(prettyError(error),true);} finally { if(manualClaim)chatScheduler.finishManual(manualClaim); }
  }
  async function sendOwn(){if(microphoneStarting||updateInstalling||systemPermission.active)return;unlockEmoteAudio();const text=message;await runMessage(text,'operator','Streamer',profile.id,undefined,true,()=>{if(message===text)message='';});}
  async function answerQueued(item:any,botId=profile.id,interactive=true,chatClaim?:ChatClaim){
    unlockEmoteAudio();
    if(busy||microphone||microphoneStarting||transcribing)return;
    await runMessage(item.text,item.platform,item.user,botId,item.replyRouteId,interactive,()=>{
      queue=queue.map(entry=>entry.queueId===item.queueId?{...entry,answeredBy:[...new Set([...(entry.answeredBy||[]),botId])]}:entry);
    },chatClaim);
  }
  async function platformAction(platform:string){
    try{if(connection[platform]==='connected'||connection[platform]==='connecting'){await call('disconnect_platform',{platform});connection[platform]='disconnected';}
      else{if(!await persist(true))return;connection[platform]='connecting';await call('connect_platform',{platform});}}
    catch(e){connection[platform]='error';notify(prettyError(e),true);}
  }
  async function selectAvatar(){
    try{const path=await open({multiple:false,filters:[{name:translate('3D-Charakter'),extensions:['vrm','glb','gltf']}]});if(typeof path!=='string')return;
      profile.avatar.assetPath=await call('import_avatar',{sourcePath:path});await persist(true);notify(translate("Charakter importiert."));}catch(e){notify(prettyError(e),true);}
  }
  async function importMotion(){
    const botId=profile.id;
    try{
      const sourcePath=await open({multiple:false,filters:[{name:'VRMA 1.0',extensions:['vrma']}]});
      if(typeof sourcePath!=='string'||botId!==profile.id||!await persist(true))return;
      saving=true;
      const motion=await call<MotionAsset>('import_motion',{sourcePath,botId});
      if(botId!==profile.id)return;
      const snapshot=profileSnapshot();snapshot.motions=[...(snapshot.motions||[]),motion];
      await call('save_profile',{profile:snapshot,botId});
      if(botId===profile.id){profile.motions=snapshot.motions;savedProfile=JSON.stringify(snapshot);clearEmotes();}
    }catch(error){notify(prettyError(error),true);}finally{saving=false;}
  }
  function removeMotion(id:string){
    clearEmotes();profile.motions=(profile.motions||[]).filter((motion:MotionAsset)=>motion.id!==id);
    profile.emotes=(profile.emotes||[]).filter((emote:any)=>!emote.action?.startsWith(`vrma:${id}:`));
  }
  async function openCharacterWindow(){
    if(busy){notify(translate("Beende zuerst die laufende Antwort."));return;}
    try{player?.stop();finishPlayback();profile.output.mode='overlay';profile.output.streamingEnabled=true;if(!await persist(true))return;await call('open_overlay',{botId:profile.id});await refresh();notify(translate("Figurenfenster geöffnet."));}catch(e){notify(prettyError(e),true);}
  }
  async function useStudio(){if(busy){notify(translate("Beende zuerst die laufende Antwort."));return;}player?.stop();finishPlayback();try{await call('close_overlay',{botId:profile.id});}catch{}profile.output.mode='studio';profile.output.streamingEnabled=false;await persist(true);}
  async function copyObs(){
    if(busy){notify(translate("Beende zuerst die laufende Antwort."));return;}
    const bot=bots.find(entry=>entry.id===profile.id);
    if(!bot||!overlayUrl){notify(translate("Der OBS-Link ist in der Desktop-App verfügbar."));return;}
    player?.stop();finishPlayback();await copyBotObs(bot);
  }
  async function configureOverlay(){if(await persist(true))try{await call('configure_overlay',{alwaysOnTop:profile.output.alwaysOnTop,clickThrough:profile.output.clickThrough,botId:profile.id});}catch(e){notify(prettyError(e),true);}}
  async function exportProfile(){try{const destination=await save({defaultPath:`${profile.id||'cohost'}.cohost.json`,filters:[{name:translate('Cohost-Profil'),extensions:['json']}]});if(destination){if(!await saveCurrentBot())return;await call('export_profile',{destination});notify(translate("Profil ohne Zugangsdaten exportiert."));}}catch(e){notify(prettyError(e),true);}}
  async function importProfile(){try{const sourcePath=await open({multiple:false,filters:[{name:translate('Cohost-Profil'),extensions:['json']}]});if(typeof sourcePath==='string'){wakeStopSignal++;await call('import_profile',{sourcePath});await refresh();notify(translate("Profil importiert. Zugangsdaten und eigene CLI-Pfade bitte neu einrichten."));}}catch(e){notify(prettyError(e),true);}}
  async function importKnowledge(){try{const sourcePath=await open({multiple:false,filters:[{name:translate('Wissensdokument'),extensions:['pdf','txt','md','json','csv']}]});if(typeof sourcePath==='string'){if(!await persist(true))return;const file=await call('import_knowledge',{sourcePath});knowledge=[...knowledge.filter(item=>item.name!==file.name),file];notify(translate("Wissensdokument hinzugefügt. Passende Stellen werden automatisch herangezogen."));}}catch(e){notify(prettyError(e),true);}}
  function confirmKnowledgeRemoval(name:string){
    const botId=profile.id;
    confirmation={title:translate('Wissensdatei entfernen?'),message:translate('„{name}“ wird aus der Wissensbibliothek dieses Bots entfernt. Die ursprüngliche Datei bleibt erhalten.',{name}),confirmLabel:translate('Datei entfernen'),danger:true,action:async()=>{if(profile.id!==botId)return;await removeKnowledge(name);}};
  }
  function confirmHistoryClear(){
    const botId=profile.id;
    confirmation={title:translate('Gesprächsverlauf leeren?'),message:translate('Der lokale Gesprächsverlauf von {name} wird gelöscht. Hindsight-Erinnerungen und Wissensdateien bleiben erhalten.',{name:profile.name}),confirmLabel:translate('Verlauf leeren'),danger:true,action:async()=>{if(profile.id!==botId)return;await call('clear_memory');if(profile.id===botId)history=[];notify(translate('Gesprächsverlauf geleert.'));}};
  }
  async function removeKnowledge(name:string){const botId=profile.id;await call('delete_knowledge',{name});if(profile.id===botId)knowledge=knowledge.filter(file=>file.name!==name);notify(translate("Wissensdatei entfernt."));}
  async function triggerEmote(id:string){if(switchingBot||creatingBot||saving||autopilotPending)return;const botId=profile.id;unlockEmoteAudio();try{if(!await persist(true)||botId!==profile.id)return;await call('trigger_emote',{emoteId:id,botId});}catch(e){notify(prettyError(e),true);}}
  function addEmote(){const action=availableEmoteActions[0]?.action;if(!action)return;profile.emotes=[...(profile.emotes||[]),{id:`emote-${Date.now()}`,name:translate("Mein Emote"),action,duration:2.8,intensity:1,allowChat:false,sound:'',soundVolume:0.7}];}
  async function importEmoteSound(emoteId:string){
    const targetBot=profile.id;
    try{
      const sourcePath=await open({multiple:false,title:translate("Emote-Sound importieren"),filters:[{name:'Audio',extensions:['wav','mp3','ogg','webm','m4a']}]});
      if(typeof sourcePath!=='string')return;
      const sound=await call<string>('import_emote_sound',{sourcePath,botId:targetBot});
      if(profile.id!==targetBot)return;
      const emote=profile.emotes.find((item:any)=>item.id===emoteId);
      if(!emote)return;
      emote.sound=sound;emote.soundVolume??=0.7;
      if(await persist(true))notify(translate("Emote-Sound importiert. Er startet gemeinsam mit der Geste."));
    }catch(error){notify(prettyError(error),true);}
  }
  function microphoneContext(){return permissionContext();}
  function lockForUpdate(installing:boolean){
    updateInstalling=installing;
    if(!installing)return;
    systemPermission.cancel();chatScheduler.cancel();
    ++autopilotStartRevision;autopilots={};autopilotPreview=undefined;
    ++turnEpoch;++wakeStopSignal;microphoneSession.cancel();clearEmotes();player?.stop();
    clearTimeout(recordingTimer);if(recorder?.state==='recording'){recorder.onstop=null;recorder.stop();}
    microphoneStream?.getTracks().forEach(track=>track.stop());microphone=false;
  }
  async function toggleMic(){
    unlockEmoteAudio();
    if(microphone&&recorder){recorder.stop();microphone=false;return;}
    if(updateInstalling||microphoneStarting||busy||transcribing||systemPermission.active)return;
    const context=microphoneContext();const targetBot=profile.id;
    const current=()=>context===microphoneContext()&&!updateInstalling;
    let lease:Awaited<ReturnType<typeof microphoneSession.request>>=null;
    try{
      lease=await microphoneSession.request(async()=>{
        if(!await persist(true)||!current())return null;
        if(!await ensureMicrophonePermission(current)||!current())return null;
        return openMicrophone({audio:true},current);
      },current);
      if(!lease)return;
      const captured=lease;const stream=captured.stream;
      microphoneStream=stream;
      const chunks:BlobPart[]=[];
      const sessionRecorder=new MediaRecorder(stream);
      recorder=sessionRecorder;
      sessionRecorder.ondataavailable=e=>{if(e.data.size)chunks.push(e.data)};
      sessionRecorder.onstop=async()=>{
        clearTimeout(recordingTimer);microphoneSession.release(captured);microphone=false;
        if(!current()||!microphoneSession.isCurrent(captured))return;
        transcribing=true;
        try{
          const blob=new Blob(chunks,{type:sessionRecorder.mimeType||'audio/webm'});
          const audio=await recordingToWav(blob);
          if(!current()||!microphoneSession.isCurrent(captured))return;
          const text=await call<string>('transcribe_audio',{...audio,botId:targetBot});
          if(!current()||!microphoneSession.isCurrent(captured))return;
          transcribing=false;message=text;
          if(profile.model.model||cliProvider){message='';await runMessage(text,'operator','Streamer',targetBot);}
          else notify(translate("Aufnahme transkribiert. Wähle ein Dialogmodell, um die Nachricht zu senden."));
        }catch(e){if(current())notify(prettyError(e),true);}finally{if(microphoneSession.isCurrent(captured))transcribing=false;}
      };
      sessionRecorder.start(1000);microphone=true;
      recordingTimer=setTimeout(()=>{if(sessionRecorder.state==='recording'){sessionRecorder.stop();microphone=false;}},120000);
      notify(translate("Mikrofon nimmt auf. Noch einmal klicken, um zu senden. Maximal zwei Minuten."));
    }catch(e){if(lease)microphoneSession.release(lease);if(current()){microphone=false;notify(translate('Mikrofon: {error}',{error:prettyError(e)}),true);}}
  }
  function timeOf(value:string){try{return formatDate(value,{hour:'2-digit',minute:'2-digit'},$locale)}catch{return ''}}

  onMount(()=>{
    subtitleLifetime=new SubtitleLifetime(text=>subtitleText=text);
    player=new SpeechPlayer(v=>level=v,v=>speaking=v);
    externalEmotes=new ExternalEmotePlayback(value=>externalEmotePlaying=value,value=>externalEmoteBlocking=value,()=>externalEmoteRevision++);
    emoteAudio=new EmoteAudioPlayer(text=>notify(text,true),value=>{effectSoundPlaying=value;updateSoundPlaying();});
    emoteSequence=new EmoteSequence(applyEmote);
    const cleanup:(()=>void)[]=[];let disposed=false;
    window.addEventListener('focus',refreshOutputDisplays);cleanup.push(()=>window.removeEventListener('focus',refreshOutputDisplays));
    function subscribe(name:string,handler:(event:any)=>void,required=false){return onEvent(name,handler).then(f=>{if(disposed)f();else cleanup.push(f);}).catch(e=>{log(prettyError(e),'error');if(required)throw e;});}
    const voices=()=>{systemVoices=window.speechSynthesis?.getVoices()||[]};voices();window.speechSynthesis?.addEventListener('voiceschanged',voices);
    if(native)void refreshOutputDisplays();
    const lifeListeners=Promise.all([subscribe('cohost://life-output',acceptLifeOutput,true),subscribe('cohost://life-runtime',acceptLifeRuntime,true),subscribe('cohost://life-turn',value=>{void lifeTurn(value).catch(error=>lifeError=prettyError(error));},true)]);
    lifeUi=new LifeUI(call,available=>{if(appAlive)lifeConnected=available;});
    cleanup.push(registerOperatorDialogBarrier(ensureOperatorBarrier));
    if(native)void lifeListeners.then(async()=>{if(disposed)return;await refresh();if(disposed)return;await lifeUi.connect();if(disposed)return;lifeMounted=true;await refreshLifeRuntime();await reportLife();}).catch(error=>{if(!disposed){lifeMounted=true;lifeError=prettyError(error);}});
    let lastActivity=0;
    const activity=(event:Event)=>{if(!event.isTrusted)return;
      // Only these explicit operator buttons use the native-intent handler,
      // which cancels pending work while retaining its exact physical owner.
      // The World target picker only chooses local geometry. The submit
      // command performs native admission; all text edits, dialogs and other
      // input keep the ordinary global cancellation path below.
      if(event.target instanceof Element&&event.target.closest('button[data-native-life-command]'))return;++operatorRevision;nativeOperatorPending=true;invalidateNativePlayback();lifeActivity=true;const now=Date.now();if(now-lastActivity>=500){lastActivity=now;void reportLife(true);}};
    for(const name of ['pointerdown','keydown','input']){window.addEventListener(name,activity,{capture:true});cleanup.push(()=>window.removeEventListener(name,activity,{capture:true}));}
    const heartbeat=setInterval(()=>{void reportLife();},2000);cleanup.push(()=>clearInterval(heartbeat));
    subscribe('cohost://autopilot',acceptAutopilot);
    subscribe('cohost://chat',m=>{queue=appendChatMessage(queue,{...m,queueId:JSON.stringify([m.platform,m.replyRouteId,m.id]),answeredBy:[]});});
    subscribe('cohost://connection',e=>{connection[e.platform]=e.status;connectionOwner[e.platform]=['connected','connecting'].includes(e.status)?e.ownerName||'':'';connectionMessages[e.platform]=e.message||'';log(`${e.platform==='twitch'?'Twitch':'YouTube'}: ${e.message||translate(({connected:'Verbunden',connecting:'Verbindet …',disconnected:'Nicht verbunden',error:'Verbindungsfehler'} as Record<string,string>)[e.status]||e.status)}`,e.status==='error'?'error':'info');});
    subscribe('cohost://agent',e=>{if(e.kind==='tool')log(e.text||e.tool||translate("Werkzeug aufgerufen"),'tool');if(e.kind==='error')log(e.text||translate("Agentenfehler"),'error');});
    subscribe('cohost://presentation',e=>{if(e.kind==='started'&&busy&&matchesPlayback(e,{id:answeringSpeechId,botId:answeringBotId}))presentReply(e);});
    subscribe('cohost://playback',e=>{
      const active=matchesPlayback(e,{id:answeringSpeechId,botId:answeringBotId});
      if(active&&((answeringAutopilot?nativeLifeOutput:resolvedOutput(profile))!=='studio')&&(!e.output||e.output===(answeringAutopilot?nativeLifeOutput:resolvedOutput(profile))))speaking=!!e.speaking;
      if(!e.speaking){
        if(active&&answeringAutopilot&&!e.error)nativePlaybackCompleted=e.id;
        if(e.botId===profile.id){if(e.error)subtitleLifetime?.cancel(e.id);else if(e.id)subtitleLifetime?.finish(e.id);}
        if(e.id)finishPlayback(e.id);
        if(e.error){
          if(active){clearEmotes();speechPreparing=false;}
          notify(translate('Sprachausgabe: {error}',{error:prettyError(e.error)}),true);log(prettyError(e.error),'error');
        }
      }
    });
    subscribe('cohost://overlay-state',event=>applyOverlayVisibility(event,{
      botId:profile.id,
      setLive:(botId,active)=>{bots=bots.map(bot=>bot.id===botId?{...bot,live:active}:bot);},
      stopPlayer:()=>player?.stop(),
      playbackIdle:()=>{if(!busy)finishPlayback();speaking=false;},
    }));
    subscribe('cohost://overlay',e=>{
      if(e.kind==='cancel-speech'){
        if(typeof e.id!=='string'||!e.id)return;
        const nativeTurn=nativeLifeTurn;
        if(nativeTurn&&nativeTurn.turnId===e.id&&nativeTurn.botId===e.botId)lifeAdmissions.finish(nativeTurn);
        cancelPlaybackTurn(e,{id:answeringSpeechId,botId:answeringBotId,autopilot:answeringAutopilot},()=>{nativeLifeTurn=undefined;},()=>{
          ++turnEpoch;running=false;chatScheduler.cancel();player?.stop();clearEmotes();subtitleLifetime?.cancel(e.id);
          busy=false;speechPreparing=false;answeringBotId='';answeringSpeechId='';answeringAutopilot=false;speaking=false;level=0;
        });
        finishPlayback(e.id);return;
      }
      if(e.kind==='profile'&&e.profile?.id){bots=bots.map(bot=>bot.id===e.profile.id?{...bot,name:e.profile.name,profile:{...bot.profile,name:e.profile.name,tagline:e.profile.tagline,avatar:{...bot.profile.avatar,...e.profile.avatar},output:{...bot.profile.output,...e.profile.output},emotes:e.profile.emotes,motions:e.profile.motions}}:bot);}
      if(e.kind==='stop'){
        if(!e.botId)worldPanel?.cancelPendingStart();
        lifeAdmissions.cancel(e.botId);
        if(e.botId&&e.botId!==profile.id&&e.botId!==answeringBotId)return;
        if(!e.botId||!answeringBotId||e.botId===answeringBotId){
          ++turnEpoch;chatScheduler.cancel(e.botId);player?.stop();finishPlayback();busy=false;speechPreparing=false;answeringBotId='';answeringSpeechId='';answeringAutopilot=false;speaking=false;level=0;
        }
        if(!e.botId||e.botId===profile.id){subtitleLifetime?.cancel();clearEmotes();}
        return;
      }
      if(e.botId&&e.botId!==profile.id||e.kind==='profile'&&e.profile?.id!==profile.id)return;
      if(e.kind==='profile'&&e.profile?.output)acceptOutputUpdate(e.profile.output);
    });
    subscribe('cohost://control',e=>{if(e.action==='pause'){running=false;chatScheduler.cancelAutomatic();systemPermission.cancel();}if(e.action==='stop')void stopAll();});
    subscribe('cohost://emote-playback',e=>externalEmotes.accept(e));
    subscribe('cohost://emote',e=>{if(e.botId&&e.botId!==profile.id)return;void showManualEmote(e);});
    const timer=setInterval(()=>{
      if(autopilotPreview&&Date.now()-autopilotPreview.startedAt>30000)autopilotPreview=undefined;
      queue=queue.filter(q=>Date.now()-new Date(q.createdAt).getTime()<Math.max(profile.chat.expirySeconds,...bots.map(bot=>bot.profile?.chat?.expirySeconds||0))*1000);
      if(systemPermission.active||updateInstalling||busy||speaking||speechPreparing||microphone||microphoneStarting||transcribing||chatScheduler.pending)return;
      // Chat keeps the full audio guard. Autonomous preparation may interrupt
      // its own ambient track later through the fresh rendered-frame gate.
      if(running&&!emoteSoundPlaying&&!externalEmotePlaying){
        const next=chatScheduler.nextClaim(chatParticipants(bots,profile),queue);
        if(next){void chatScheduler.runClaim(next,()=>answerQueued(next.message,next.botId,false,next)).catch(error=>notify(prettyError(error),true));return;}
      }

    },1000);
    return()=>{appAlive=false;lifeAdmissions.cancel();lifeMounted=false;lifeUi.dispose();systemPermission.cancel();chatScheduler.cancel();disposed=true;turnEpoch++;subtitleLifetime?.destroy();clearEmotes();clearTimeout(recordingTimer);microphoneSession.cancel();if(recorder?.state==='recording'){recorder.onstop=null;recorder.stop();}clearInterval(timer);finishPlayback();cleanup.forEach(f=>f());player.destroy();emoteAudio?.destroy();externalEmotes?.destroy();window.speechSynthesis?.removeEventListener('voiceschanged',voices);microphoneStream?.getTracks().forEach(t=>t.stop());};
  });
</script>

<svelte:head><title>OrbitPal</title></svelte:head>
<svelte:window onkeydown={handleShortcut}/>

<a class="skip-link" href="#main-content">{$t("Zum Inhalt springen")}</a>
<div class="app-shell" inert={updateInstalling||!!confirmation}>
  <aside class="sidebar">
    <div class="window-drag" data-tauri-drag-region></div>
    <a class="brand" href="/" onclick={(e)=>{e.preventDefault();navigate('studio')}} aria-label={$t("OrbitPal Regiepult")}><span class="brand-symbol"><Sparkles size={22} strokeWidth={2.3}/></span><span>Orbit<span class="brand-ai">Pal</span><small>Cohost studio</small></span></a>
    <div class="bot-switcher">
      <button class="profile-switch" aria-label={$t("Bot auswählen: {name}",{name:profile.name})} aria-expanded={botMenu} onclick={()=>botMenu=!botMenu}><span class="mini-avatar"><span></span><span></span></span><span><strong>{profile.name}</strong><small>{$t(bots.length<=1?'{count} Bot · Auswahl':'{count} Bots · Auswahl',{count:formatNumber(bots.length||1,$locale)})}</small></span><ChevronDown size={15}/></button>
      {#if botMenu}<div class="bot-menu"><div class="bot-menu-heading"><strong>{$t("Deine Bots")}</strong><button class="icon-button" aria-label={$t("Bot-Auswahl schließen")} onclick={()=>botMenu=false}><X size={14}/></button></div>{#each bots as bot}<button class="bot-menu-entry" class:chosen={bot.id===profile.id} onclick={()=>selectBot(bot.id)} disabled={busy||microphone||microphoneStarting||transcribing||switchingBot||creatingBot||saving||autopilotPending}><span class="bot-initial">{bot.name.charAt(0)}</span><span><strong>{bot.name}</strong><small>{bot.live?$t("Eigenes Figurenfenster"):bot.profile?.output?.streamingEnabled?$t('OBS-Ausgabe'):$t("Profil")}</small></span>{#if bot.id===profile.id}<Check size={14}/>{/if}</button>{/each}<button class="button secondary full" disabled={busy||microphone||microphoneStarting||transcribing||switchingBot||creatingBot||saving||autopilotPending} onclick={()=>{botMenu=false;botDialog=true;botCopy=false;botCharacter='nova'}}><Plus size={14}/>{$t("Neuen Bot anlegen")}</button></div>{/if}
    </div>

    <nav aria-label={$t("Hauptnavigation")}>{#each ['live','bot','shared'] as group}{#if group!=='live'}<p class="nav-group-label">{group==='bot'?$t('Für {name}',{name:profile.name}):$t('Gemeinsame Bibliothek')}</p>{/if}{#each nav.filter(item=>item.group===group) as item}<button class:active={page===item.id} onclick={()=>navigate(item.id as Page)} aria-current={page===item.id?'page':undefined} aria-label={$t(item.label)} title={$t(item.label)+' (Alt/⌥+'+(nav.indexOf(item)+1)+')'}><item.icon size={18} strokeWidth={1.7}/><span>{$t(item.label)}</span>{#if item.id==='extensions'&&(library.mcpServers.length+library.skills.length)}<span class="nav-count">{formatNumber(library.mcpServers.length+library.skills.length,$locale)}</span>{/if}</button>{/each}{/each}</nav>
    <div class="sidebar-bottom"><div class="runtime-line"><span class="status-dot" class:online={native}></span>{native?$t('Desktop-App'):$t('Browser-Vorschau')}<span>v{appVersion}</span></div><button class="subtle-button" onclick={()=>updatesOpen=true}><Download size={16}/>{availableUpdate ? $t('Update auf v{version}',{version:availableUpdate}) : $t('Updates & Version')}</button><button class="subtle-button" onclick={()=>{navigate('models')}}><SlidersHorizontal size={16}/> {$t("Integrationen verwalten")}</button><p>{$t("Dein Charakter. Deine Regeln.")}</p></div>
  </aside>

  <div class="workspace">
    <header class="app-header" data-tauri-drag-region><div class="breadcrumb">{page==='api-access'?$t('Gemeinsame Zugänge'):page==='extensions'?$t("Bibliothek"):profile.name} <span>/</span> <strong>{$t(currentPage?.label||'')}</strong></div><div class="header-actions"><label class="interface-language" title={$t('Sprache der Oberfläche. Gespräch, Stimme und Bot-Anweisungen bleiben unverändert.')}><span class="sr-only">{$t('Sprache der Oberfläche')}</span><select aria-label={$t('Sprache der Oberfläche')} value={$locale} onchange={event=>setLocale(event.currentTarget.value as typeof $locale)}>{#each supportedLocales as language}<option value={language.code}>{language.label}</option>{/each}</select></label><span class="mode-pill"><span class="small-dot"></span>{cliProvider?$t('CLI-Anbieter'):['ollama','lmstudio'].includes(profile.model.provider)?$t("Lokales Modell"):$t("Cloud / API")}</span><span class="save-state" class:unsaved={hasUnsavedChanges} role="status">{extensionBusy||apiAccessBusy?$t("Wird ausgeführt …"):saving?$t("Speichert …"):hasUnsavedChanges?$t("Nicht gespeicherte Änderungen"):$t("Gespeichert")}</span><button class="button small primary header-save" onclick={saveContext} disabled={saving||extensionBusy||apiAccessBusy||!hasUnsavedChanges}><Check size={15}/>{extensionBusy||apiAccessBusy?$t("Wird ausgeführt …"):saving?$t("Speichert …"):$t("Speichern")}</button><button class="icon-button" onclick={()=>updatesOpen=true} title={availableUpdate?$t('Update auf v{version}',{version:availableUpdate}):$t('Updates & Version')} aria-label={$t("Updates & Version anzeigen")}><Download size={18}/></button><button class="icon-button keyboard-help-button" onclick={()=>shortcutHelp=true} title={$t("Tastatursteuerung (?)")} aria-label={$t("Tastatursteuerung anzeigen")}><CircleHelp size={18}/></button><button class="stop-button" onclick={stopAll} title={$t("Sprache und Agent sofort stoppen")}><Square size={13} fill="currentColor"/>{$t("Stop")}</button></div></header>
    <main id="main-content" tabindex="-1">
      {#if page==='studio'}<MicrophonePermissions/>{/if}
      {#if page==='extensions'}<div class="scope-strip shared"><span>{$t('Für alle Bots installiert')}</span><p>{$t("Freigaben rechts gelten ausschließlich für den gewählten Bot.")}</p></div>{/if}

      <div class="page-heading"><div><h1>{$t(headings[page])}</h1><p>{$t(subheadings[page])}</p></div>{#if page==='studio'}<button class="button primary" class:running onclick={toggleRunning}>{#if running}<Pause size={16}/>{$t("Cohost pausieren")}{:else}<Play size={16} fill="currentColor"/>{$t("Cohost starten")}{/if}</button>{/if}</div>
      {#if lifePreferencesDirty}<div class="scope-strip shared"><span>{$t('Alltagsvorlieben warten auf Speichern oder Verwerfen.')}</span><button class="text-link" onclick={()=>navigate('autopilot')}>{$t('Zu den Alltagsvorlieben')}</button><button class="text-link" disabled={lifePreferencesBusy} onclick={()=>{experiencePanel?.discardDraft();socialPanel?.discardDraft();carePanel?.discardDraft();}}>{$t('Entwurf verwerfen')}</button></div>{/if}
      {#if worldDirty&&page!=='autopilot'}<div class="scope-strip shared"><span>{$t('Gemeinsamer Bühnenentwurf wartet auf Speichern oder Verwerfen.')}</span><button class="text-link" onclick={()=>navigate('autopilot')}>{$t('Zum Bühnenentwurf')}</button></div>{/if}
      <div class="wake-control" hidden={page!=='studio'&&!profile.wakeWord?.enabled}><WakeWord {permissionContext} sttConfig={profile.stt} bind:config={profile.wakeWord} botId={profile.id} sttProvider={profile.stt.provider} blocked={updateInstalling||busy||speaking||speechPreparing||emoteSoundPlaying||externalEmotePlaying||microphone||microphoneStarting||transcribing} stopSignal={wakeStopSignal} onTrigger={text=>runMessage(text,'microphone','Streamer',profile.id,undefined,false)} onError={message=>notify(message,true)} onConfigChange={()=>persist(true)} onActiveChange={active=>wakeActive=active}/></div>

      <div hidden={page!=='autopilot'}><WorldStatePanel bind:this={worldPanel} {bots} selectedBotId={profile.id} outputUrl={overlayUrl} displays={outputDisplays} visible={page==='autopilot'} disabled={updateInstalling||saving||switchingBot||creatingBot} executionBlocked={quietRestBlocked} registerWorld={registerWorld} onStart={startLifeScene} onStop={stopAll} active={!!lifeRuntime?.sessions.some(session=>session.active)} starting={autopilotPending} onMove={startWorldMove} onSlotMove={startWorldSlot} onDirtyChange={value=>worldDirty=value} onBusyChange={value=>worldBusy=value} confirmDiscard={confirmWorldDiscard}/></div>

      <div hidden={page!=='autopilot'}>{#key profile.id}
        <ExperiencePanel bind:this={experiencePanel} botId={profile.id} {bots} active={!!selectedLifeSession?.active} disabled={saving||switchingBot||creatingBot||updateInstalling} onDirtyChange={value=>experienceDirty=value} onBusyChange={value=>experienceBusy=value}/>
        <SocialPanel bind:this={socialPanel} botId={profile.id} {bots} active={!!selectedLifeSession?.active} disabled={saving||switchingBot||creatingBot||updateInstalling} onDirtyChange={value=>socialDirty=value} onBusyChange={value=>socialBusy=value}/>
        <CarePreferencesPanel bind:this={carePanel} botId={profile.id} disabled={saving||switchingBot||creatingBot||updateInstalling} onDirtyChange={value=>careDirty=value} onBusyChange={value=>careBusy=value}/>
      {/key}<ContentPacksPanel disabled={saving||switchingBot||creatingBot||updateInstalling} onCreated={async()=>{bots=await call<any[]>('list_bots');}}/></div>

      {#if page==='studio'}
        <section class="crew-panel"><div class="crew-heading"><div><h2>{$t("Deine Stream-Crew")} <HelpTip text={$t("Der Live-Chat ist die gemeinsame Eingabe. Eigene Ansprache und Antwortmodus legst du pro Bot unter Verbindungen fest. Antworten werden nacheinander gesprochen.")}/><span>{$t('{count} Ausgaben aktiv',{count:formatNumber(liveBots.length,$locale)})}</span></h2></div><button class="button secondary small" disabled={busy||microphone||microphoneStarting||transcribing||switchingBot||creatingBot||saving||autopilotPending} onclick={()=>{botDialog=true;botCopy=false;botCharacter='nova'}}><Plus size={14}/>{$t("Bot hinzufügen")}</button></div><div class="crew-list">{#each bots as bot}<article class="crew-card" class:editing={bot.id===profile.id}><button class="crew-identity" onclick={()=>selectBot(bot.id)} disabled={busy||microphone||microphoneStarting||transcribing||switchingBot||creatingBot||saving||autopilotPending}><span class="bot-initial">{bot.name.charAt(0)}</span><span><strong>{bot.name}</strong><small>{bot.id===profile.id?$t("Zum Bearbeiten ausgewählt"):bot.profile?.model?.provider||$t("Eigenes Profil")}</small></span></button><div class="crew-actions"><button class="button small" class:secondary={!(botEnabled(bot))} class:live-output={botEnabled(bot)} disabled={busy||microphone||microphoneStarting||transcribing||switchingBot||creatingBot||saving||autopilotPending} onclick={()=>toggleBotLive(bot)}><Monitor size={12}/>{botEnabled(bot)?$t("Ausgabe beenden"):$t("Figur zeigen")}</button><button class="icon-button" title={$t('Eigener OBS-Link für {name}',{name:bot.name})} aria-label={$t('Eigener OBS-Link für {name}',{name:bot.name})} onclick={()=>copyBotObs(bot)} disabled={busy||microphone||microphoneStarting||transcribing||switchingBot||creatingBot||saving||autopilotPending}><Copy size={14}/></button></div></article>{/each}</div></section>
        <div class="studio-layout"><section class="stage-column"><div class="studio-stage"><div class="stage-top"><span class="stage-identity"><span class="stage-dot" class:alive={running||speaking}></span>{profile.name}<span class="soft-label">{$t("Dein KI-Cohost")}</span></span><button class="stage-tool" onclick={()=>navigate('character')} title={$t("Charakter bearbeiten")} aria-label={$t("Charakter bearbeiten")}><Settings2 size={17}/></button></div>
          {#if profile.output.mode==='studio'}<Avatar bind:this={liveAvatar} audioEnabled={resolvedOutput(profile)==='studio'} audioStop={modelAudioStop} onAudioPlaying={modelAudioPlaying} {profile} {speaking} {level} {emotion} autopilot={motionRuntime} outputUrl={overlayUrl} assetUrl={assetUrl(profile.avatar.assetPath,overlayUrl)} onModelLoaded={info=>renderedModel=info}/>{:else}<div class="stage-away"><Monitor size={48} strokeWidth={1}/><h2>{profile.output.mode==='overlay'?$t("Auf deinem Desktop."):$t("Bereit für deinen Stream.")}</h2><p>{profile.output.mode==='overlay'?$t("Dein Charakter läuft im eigenen Figurenfenster."):$t("Füge den kopierten Link als Browserquelle in OBS ein.")}</p><button class="button dark" onclick={useStudio}>{$t("Zur Studioansicht")}</button></div>{/if}
          <div class="stage-caption">{#if subtitleText}<p>{subtitleText}</p>{:else if !hadSubtitle}<p class="intro-caption">{profile.tagline}</p>{/if}</div><div class="stage-bottom"><span><span class="stage-dot" class:alive={busy||speaking||running}></span>{$t(activeStatus)}</span><span><Volume2 size={13}/>{profile.voice.provider==='system'?$t("Systemstimme"):profile.voice.provider==='elevenlabs'?'ElevenLabs':$t("Eigene Sprach-API")}</span></div></div>
          <div class="composer"><div class="composer-title"><MessageCircle size={16}/><span>{$t('Sprich mit {name}',{name:profile.name})}</span><small>{$t("Nur für dich und deinen Cohost")}</small></div><form onsubmit={(e)=>{e.preventDefault();sendOwn()}}><input bind:this={messageInput} aria-label={$t("Nachricht an deinen Cohost")} placeholder={$t("Was möchtest du deinem Cohost sagen?")} bind:value={message} disabled={busy||transcribing||microphone||microphoneStarting}/><button type="button" class:recording={microphone} class="mic-button" aria-describedby="manual-microphone-disclosure" onclick={toggleMic} disabled={busy||transcribing||microphoneStarting} title={microphone?$t("Aufnahme senden"):$t("Mikrofon aufnehmen")} aria-label={microphone?$t("Aufnahme senden"):$t("Mikrofon aufnehmen")}><Mic size={18}/></button><button class="send-button" type="submit" disabled={!message.trim()||busy||transcribing||microphone||microphoneStarting} aria-label={$t("Nachricht senden")}><Send size={17}/></button></form><p class="field-note" id="manual-microphone-disclosure">{#if profile.stt.provider==='whisper-local'}{$t('Aufnahmen werden mit {provider} auf diesem Rechner verarbeitet.',{provider:microphoneProviderName})}{:else if microphoneProviderLabels[profile.stt.provider]}{$t('Aufnahmen werden von {provider} verarbeitet. Bei Cloud- oder Remote-Anbietern verlässt das Audio deinen Rechner.',{provider:microphoneProviderName})}{:else}{$t('Wähle unter Modelle zuerst einen Spracherkennungsanbieter.')}{/if}</p><div class="composer-foot"><span>{microphoneStarting?$t("Mikrofon wird aktiviert …"):transcribing?$t("Aufnahme wird transkribiert …"):microphone?$t("Aufnahme läuft …"):busy?$t("Antwort wird erstellt …"):profile.model.model||$t("Wähle unter Modelle deine KI aus.")}</span><span>{$t("Enter zum Senden")}</span></div></div>
          <div class="output-strip"><div class="output-label"><Monitor size={17}/><span>{$t("Darstellung")}</span></div><div class="segmented"><button class:selected={profile.output.mode==='studio'} onclick={useStudio}>{$t("Studio")}</button><button class:selected={profile.output.mode==='overlay'} onclick={openCharacterWindow}>{$t("Desktop-Overlay")} <MoveUpRight size={12}/></button><button class:selected={profile.output.mode==='obs'} onclick={copyObs}>{$t("OBS")} <Copy size={12}/></button></div></div>
          <div class="emote-toolbar"><span>{$t("Emotes")}</span>{#if hostEmoteError&&profile.output.mode!=='studio'&&(!liveAvatar||renderedModel.status!=='ready')}<span role="status" title={hostEmoteError}>{$t("Emote-Liste nicht verfügbar")}</span>{/if}{#each profile.emotes || [] as emote}<button disabled={!emoteAvailable(emote)} onclick={()=>triggerEmote(emote.id)} title={$t('{name} ausführen',{name:emote.name})}>{#if emote.action==='wave'}<Hand size={14}/>{:else if emote.action==='hide'}<EyeOff size={14}/>{:else}<PartyPopper size={14}/>{/if}{emote.name}</button>{/each}<button class="emote-edit" onclick={()=>navigate('character')} title={$t("Emotes bearbeiten")} aria-label={$t("Emotes bearbeiten")}><Plus size={14}/></button></div>

        </section><aside class="chat-column"><div class="chat-panel"><div class="panel-title"><h2>{$t("Live-Chat")} <span class="count-badge">{formatNumber(queue.length,$locale)}</span></h2><button class="icon-button" onclick={()=>navigate('connections')} title={$t("Chat-Verbindungen")} aria-label={$t("Chat-Verbindungen")}><Plug size={17}/></button></div><div class="platform-tabs"><button onclick={()=>navigate('connections')}><span class="platform-icon twitch">▰</span>Twitch<span class="status-dot" class:online={connection.twitch==='connected'}></span></button><button onclick={()=>navigate('connections')}><Youtube size={15} class="youtube"/>YouTube<span class="status-dot" class:online={connection.youtube==='connected'}></span></button></div><div class="chat-messages">{#if !queue.length}<div class="empty-chat"><span class="empty-chat-icon"><MessageCircle size={28} strokeWidth={1.3}/></span><h3>{connectedCount?$t("Ein offenes Ohr."):$t("Noch ganz unter uns.")}</h3><p>{connectedCount?$t("Neue Nachrichten erscheinen hier. Du entscheidest, worauf dein Cohost antwortet."):$t("Verbinde deinen Chat und hol deine Community dazu.")}</p>{#if !connectedCount}<button class="text-link" onclick={()=>navigate('connections')}>{$t("Chat verbinden")} <ArrowUpRight size={14}/></button>{/if}</div>{:else}{#each queue as item (item.queueId)}<article class="chat-message"><div><span class="chat-platform" class:youtube={item.platform==='youtube'}>{item.platform==='twitch'?'T':'Y'}</span><strong>{item.user}</strong><time>{timeOf(item.createdAt)}</time></div><p>{item.text}</p><button class="text-link" disabled={busy||transcribing||microphone||microphoneStarting} onclick={()=>answerQueued(item)}>{$t((item.answeredBy||[]).includes(profile.id)?'Erneut mit {name}':'Antwort mit {name}',{name:profile.name})} <ArrowUpRight size={12}/></button></article>{/each}{/if}</div><div class="chat-control"><label for="reply-mode">{$t("Antwortmodus")}</label><select id="reply-mode" bind:value={profile.chat.mode}><option value="manual">{$t("Manuell auswählen")}</option><option value="cohost">{$t("Co-Host")}</option><option value="afk">{$t("Pausenvertretung")}</option></select>{#if profile.chat.mode!=='manual'}<small>{$t("Automatische Antworten starten mit „Cohost starten“.")}</small>{/if}</div></div><div class="activity-panel"><div class="panel-title"><h2>{$t("Aktivität")}</h2><span class="quiet-icon"><Zap size={15}/></span></div>{#if !logs.length}<p class="activity-empty">{$t("Bereit für deinen nächsten Stream.")}</p>{:else}<div class="activity-list">{#each logs.slice(0,4) as entry}<div><span class="activity-tick" class:failed={entry.kind==='error'}>{#if entry.kind==='error'}<X size={11}/>{:else}<Check size={11}/>{/if}</span><p>{entry.text}</p><time>{timeOf(entry.time)}</time></div>{/each}</div>{/if}<div class="local-note"><span class="status-dot online"></span>{hasUnsavedChanges?$t("Nicht gespeicherte Änderungen"):$t("Profil lokal gespeichert")}<FolderOpen size={13}/></div></div></aside></div>

      {:else if page==='autopilot'}
        <details class="life-options"><summary>{$t('Einzelne Figur starten')}</summary>
        <section class="settings-panel"><OutputPreviewControls {profile} displays={outputDisplays} refreshDisplays={refreshOutputDisplays} monitor={true} disabled={saving||switchingBot||creatingBot||updateInstalling} preview={previewOutput}/></section>
        <LifeRuntimePanel botId={profile.id} botName={profile.name} session={selectedLifeSession} system={lifeRuntime?.system} bot={lifeBots[profile.id]} connected={lifeConnected} error={lifeError} pending={autopilotPending} desktopState={lifeOutputs[profile.id]?.state} disabled={busy||speaking||speechPreparing||microphone||microphoneStarting||transcribing||updateInstalling} speechStatus={autopilotSpeechStatus} lastAttemptSilent={!!selectedAutopilot?.active&&silentAutopilots[profile.id]===selectedAutopilot.seed} previewing={autopilotPreview?.botId===profile.id} onPreview={previewAutopilot} onStart={(mode,initialize)=>void toggleLife(mode,true,initialize)} onStop={()=>void toggleLife(selectedLifeSession?.mode??'ambient',false)} onRefresh={()=>void refreshLifeRuntime()} onReconnect={()=>void reconnectLifeUI()}/>
        </details>
        <details class="life-options"><summary>{$t('Verhalten, Sprache und weitere Optionen')}</summary>
        <LifeActionsPanel botId={profile.id} botName={profile.name} disabled={quietRestBlocked} onQuietRest={startQuietRest}/>
        <AutopilotPanel showControls={false} bind:config={profile.autopilot} botName={profile.name} active={!!selectedAutopilot?.active} speechStatus={autopilotSpeechStatus} lastAttemptSilent={!!selectedAutopilot?.active&&silentAutopilots[profile.id]===selectedAutopilot.seed} disabled={busy||speaking||speechPreparing||microphone||microphoneStarting||transcribing||updateInstalling} pending={autopilotPending} previewing={autopilotPreview?.botId===profile.id} onStart={()=>toggleAutopilot(true)} onStop={()=>toggleAutopilot(false)} onPreview={previewAutopilot}/>
        <UsageBudgetPanel {bots} disabled={updateInstalling||saving||switchingBot||creatingBot} onDirtyChange={value=>usageBudgetDirty=value}/>
        {#key profile.id}<ScreenContextPanel botId={profile.id} {permissionContext} prepareSelection={prepareScreenSource} onError={message=>notify(message,true)} bind:config={profile.screenContext} visionEnabled={profile.model.visionEnabled} disabled={updateInstalling}/>{/key}
        {#if motionRuntime}<section class="autopilot-stage"><Avatar bind:this={liveAvatar} {profile} {speaking} {level} {emotion} autopilot={motionRuntime} outputUrl={overlayUrl} assetUrl={assetUrl(profile.avatar.assetPath,overlayUrl)} onModelLoaded={info=>renderedModel=info}/><span>{autopilotPreview ? $t('Stumme Bewegungsvorschau · 30 Sekunden') : $t('Autopilot · {name}',{name:profile.name})}</span></section>{/if}
        </details>
      {:else if page==='character'}
        <section class="settings-panel character-library"><div class="section-heading"><Sparkles size={20}/><div><h2>{$t("Figurenbibliothek")}</h2><p>{$t('{count} Charaktere sind bereits dabei. Weitere VRM-, GLB- und glTF-Figuren kannst du importieren.',{count:formatNumber(characterCatalog.length,$locale)})}</p></div></div><div class="character-library-layout"><div class="character-live-preview"><Avatar bind:this={liveAvatar} {profile} {speaking} {level} {emotion} autopilot={motionRuntime} outputUrl={overlayUrl} assetUrl={assetUrl(profile.avatar.assetPath,overlayUrl)} onModelLoaded={info=>renderedModel=info}/><span>{selectedCharacter?.name||$t("Deine importierte Figur")}</span></div><div class="character-choices"><input class="character-search" aria-label={$t("Charaktere durchsuchen")} bind:value={characterSearch} placeholder={$t("Figuren durchsuchen …")}/><div class="character-choice-list">{#each filteredCharacters as character}<button class="character-choice" class:chosen={profile.avatar.assetPath===character.assetPath} onclick={()=>chooseCharacter(character)}><span class="character-symbol">{#if character.id==='nova'}<Sparkles size={24}/>{:else if character.id==='builtin:fox'}<PartyPopper size={24}/>{:else}<Radio size={24}/>{/if}</span><span><strong>{character.name}</strong><small>{$t(character.description)}</small><em>{character.license}</em></span>{#if profile.avatar.assetPath===character.assetPath}<Check size={17}/>{/if}</button>{/each}{#if !filteredCharacters.length}<p class="field-note">{$t("Keine Figur passt zu deiner Suche.")}</p>{/if}</div><button class="button secondary" onclick={selectAvatar}><Upload size={14}/>{$t("Eigene Figur importieren")}</button></div></div>{#if selectedCharacter?.sourceUrl}<p class="character-credit">{selectedCharacter.author} · {selectedCharacter.license} <button class="text-link" onclick={()=>openUrl(selectedCharacter.sourceUrl)}>{$t("Quelle & Lizenz")} <ExternalLink size={12}/></button></p>{/if}</section>
        <ModelFeatures info={modelInfo} animations={renderedModel.animations} status={renderedModel.status} error={renderedModel.error} metadataError={modelInfoError} disabled={busy||speaking||speechPreparing||updateInstalling} procedural={renderedModel.proceduralGestures||[]} onPreview={previewModelAnimation} onAddEmote={addModelAnimation}/>
        <MotionLibrary motions={profile.motions||[]} availableActions={availableEmoteActions.map(action=>action.action)} compatible={renderedModel.status==='ready'&&['vrm0','vrm1'].includes(renderedModel.format||'')} disabled={busy||speaking||speechPreparing||saving||switchingBot||creatingBot||updateInstalling} onImport={importMotion} onRemove={removeMotion} onPreview={previewModelAnimation} onAddEmote={addModelAnimation}/>


        <div class="settings-grid"><section class="settings-panel"><div class="section-heading"><Sparkles size={19}/><div><h2>{$t("Dein Charakter")}</h2><p>{$t("Eine Identität, die bleibt.")}</p></div></div><label>{$t("Name")}<input bind:value={profile.name}/></label><label>{$t("Kurze Vorstellung")}<input bind:value={profile.tagline}/></label><div class="field-row"><label>{$t("Akzentfarbe")}<input type="color" bind:value={profile.avatar.accent}/></label><label>{$t("Kamera")}<select bind:value={profile.avatar.camera}><option value="portrait">{$t("Porträt")}</option><option value="full">{$t("Ganze Figur")}</option></select></label></div><OutputPreviewControls {profile} disabled={saving||switchingBot||creatingBot||updateInstalling} preview={previewOutput}/><div class="toggle-row"><div><strong>{$t("Natürliche Bewegung")}</strong><small>{$t("Atmen, mitgelieferte Ruhepose und Blinzeln")}</small></div><input type="checkbox" role="switch" aria-label={$t("Natürliche Bewegung")} bind:checked={profile.avatar.idle}/></div><div class="asset-drop"><Upload size={24}/><strong>{profile.avatar.assetPath?$t("Eigener 3D-Charakter"):$t("Nova ist schon dabei.")}</strong><p>{profile.avatar.assetPath?.split(/[\\/]/).pop()||$t("Oder importiere deine eigene VRM-, GLB- oder glTF-Datei.")}</p><p class="field-note">{$t("Bei glTF das vollständige Paket zuerst entpacken. BIN-Dateien und PNG/JPEG-Texturen bleiben im Modellordner; die App bündelt sie beim Import.")}</p><button class="button secondary" onclick={selectAvatar}>{$t("3D-Datei importieren")}</button>{#if profile.avatar.assetPath}<button class="text-link" onclick={()=>chooseCharacter(characterCatalog[0])}>{$t("Nova wiederherstellen")}</button>{/if}</div></section><section class="settings-panel"><div class="section-heading"><Volume2 size={19}/><div><h2>{$t("Stimme & Sprache")}</h2><p>{$t("Ein eigener Klang für deinen Cohost.")}</p></div></div><label>{$t("Sprachausgabe")}<select bind:value={profile.voice.provider} onchange={()=>{profile.voice.apiKeyId='';profile.voice.voiceId='';profile.voice.model=profile.voice.provider==='elevenlabs'?'eleven_flash_v2_5':'tts-1';}}><option value="system">{$t("Systemstimme · sofort verfügbar")}</option><option value="elevenlabs">{$t("ElevenLabs · Kunststimme")}</option><option value="compatible">{$t("Lokale / kompatible TTS-API")}</option></select></label>{#if profile.voice.provider==='system'}<label>{$t("Stimme")}<select bind:value={profile.voice.voiceId}><option value="">{$t("Passende deutsche Systemstimme")}</option>{#each systemVoices as v}<option value={v.voiceURI}>{v.name} ({v.lang})</option>{/each}</select></label><p class="field-note">{$t("Die Systemstimme kommt vom Betriebssystem. Eine neue Kunststimme richtest du über einen Sprach-Anbieter oder eine lokale TTS-API ein.")}</p>{:else}<label>{profile.voice.provider==='elevenlabs'?'Voice-ID':$t("Stimmenname / Voice-ID")}<input bind:value={profile.voice.voiceId} placeholder={$t("Deine gespeicherte Stimme")}/></label><label>{$t("Sprachmodell")}<input bind:value={profile.voice.model}/></label>{#if profile.voice.provider==='compatible'}<label>{$t("API-Basis-URL")}<input bind:value={profile.voice.baseUrl} placeholder="http://localhost:8000/v1"/></label>{/if}<CredentialSelector botId={profile.id} purpose="speech" config={profile.voice} snapshot={apiSnapshot} disabled={saving||apiAccessBusy||updateInstalling} onChange={value=>profile.voice.apiKeyId=value} onManage={()=>navigate('api-access')}/>{#if profile.voice.provider==='elevenlabs'}<button class="text-link" onclick={()=>native?openUrl('https://elevenlabs.io/app/voice-lab'):window.open('https://elevenlabs.io/app/voice-lab','_blank','noopener')}>{$t("Kunststimme bei ElevenLabs gestalten")} <ExternalLink size={13}/></button>{/if}{/if}<p class="field-note">{$t("Audio-Tags wie [crying] oder [shouts] werden nur an ElevenLabs übergeben. Die Anzeige enthält den bereinigten Text. Die Interpretation der Tags hängt vom gewählten ElevenLabs-Modell ab, zum Beispiel eleven_v3.")}</p><label>{$t("Sprechtempo")} <span>{formatNumber(Number(profile.voice.rate),$locale,{minimumFractionDigits:1,maximumFractionDigits:1})}×</span><input type="range" min="0.6" max="1.5" step="0.05" bind:value={profile.voice.rate}/></label><label>{$t("Lautstärke")} <span>{formatNumber(Math.round(profile.voice.volume*100),$locale)} %</span><input type="range" min="0" max="1" step="0.05" bind:value={profile.voice.volume}/></label><div class="toggle-row"><div><strong>{$t("Sprachausgabe aktiv")}</strong><small>{$t("Antworten vorlesen und die Figur animieren")}</small></div><input type="checkbox" role="switch" aria-label={$t("Sprachausgabe aktiv")} bind:checked={profile.voice.enabled}/></div><p class="inline-note"><CircleHelp size={16}/>{$t("Eine Cloud-Stimme benötigt im lokalen Betrieb ein eigenes lokales Stimmenmodell.")}</p></section></div>

      {:else if page==='models'}
        <div class="settings-grid">
          <section class="settings-panel">
            <div class="section-heading"><BrainCircuit size={20}/><div><h2>{$t("Dialoganbieter")}</h2><p>{$t("Persönlichkeit und Werkzeuge bleiben beim Wechsel erhalten.")}</p></div></div>
            <div class="field-with-help"><div class="field-label"><label for="cohost-field-1">{$t("Integration")} </label><HelpTip text={$t("Wählt den Dialoganbieter dieses Bots. Lokale Modelle brauchen eine laufende lokale API; Codex und Claude Code verwenden die separat installierte und angemeldete CLI.")}/></div><select id="cohost-field-1" bind:value={profile.model.provider} onchange={changeProvider}>
              <option value="ollama">{$t("Ollama · lokal")}</option><option value="lmstudio">{$t("LM Studio · lokal")}</option>
              <option value="openai">{$t("OpenAI · Cloud-API")}</option><option value="anthropic">{$t("Anthropic · Cloud-API")}</option>
              <option value="claude-code">{$t("Claude Code · CLI-Login")}</option><option value="codex">{$t("Codex · CLI-Login")}</option>
              <option value="compatible">{$t("Kompatible API · eigener Endpunkt")}</option>
            </select></div>
            {#if cliProvider}
              <label>{$t("CLI-Programm")}<div class="input-action"><input bind:value={profile.model.command} placeholder={profile.model.provider==='codex'?$t("codex oder absoluter Programmpfad"):$t("claude oder absoluter Programmpfad")}/><button class="button secondary" onclick={selectCli}><FolderOpen size={14}/>{$t("Auswählen")}</button></div></label>
              <div class="button-row"><button class="button secondary small" onclick={checkCli} disabled={checkingCli||loadingCliModels}><Cable size={14}/>{checkingCli?$t("Prüft …"):$t("CLI prüfen")}</button><button class="button secondary small" onclick={()=>loadCliModels()} disabled={loadingCliModels||checkingCli}><RotateCcw size={14}/>{loadingCliModels?$t("Lädt …"):$t("Modelle & Denkstufen laden")}</button>{#if cliStatus}<span class="field-note">{cliStatus}</span>{/if}</div>
              <label>{$t("Modell auswählen")}<select value={cliPickerValue} onchange={event=>selectCliModel(event.currentTarget.value)}><option value="default">{$t("CLI-Standard")}{cliCatalogKey===cliKey&&cliConfiguredModel?` · ${cliConfiguredModel}`:''}</option>{#each pickerCliModels as model}<option value={model.id}>{model.name} · {model.id}</option>{/each}<option value="__custom__">{$t("Eigene Modell-ID eingeben …")}</option></select></label>
              {#if cliPickerValue==='__custom__'}<label>{$t("Genaue Modell-ID")}<input bind:value={profile.model.model} oninput={()=>profile.model.reasoningEffort='default'} placeholder={$t("Exakter Modellname aus deiner CLI")}/></label>{/if}
              {#if cliCatalogNote&&cliCatalogKey===cliKey}<p class="field-note">{$t(cliCatalogNote)}</p>{/if}
              <div class="field-with-help"><div class="field-label"><label for="cohost-field-2">{$t("Denkaufwand")} </label><HelpTip text={$t("Steuert, wie lange das Modell vor seiner Antwort nachdenkt. Für spontane Stream-Gespräche kann eine niedrige Stufe helfen. Die angebotenen Stufen hängen vom gewählten Modell ab.")}/></div><select id="cohost-field-2" bind:value={profile.model.reasoningEffort}><option value="default">{$t("CLI-Standard")}{selectedCliModel?.defaultReasoningEffort?` · ${selectedCliModel.defaultReasoningEffort}`:''}</option>{#each cliEfforts as effort}<option value={effort}>{$t(effortLabels[effort]||effort)}</option>{/each}{#if profile.model.reasoningEffort!=='default'&&!cliEfforts.includes(profile.model.reasoningEffort)}<option value={profile.model.reasoningEffort}>{profile.model.reasoningEffort} {$t("· Katalog prüfen")}</option>{/if}</select></div>
              <p class="field-note">{cliEfforts.length?$t("Weniger Denkaufwand kann schneller antworten. Die Einstellung gilt für diesen Bot."):profile.model.provider==='claude-code'?$t("Für eine gezielte Denkstufe lade die Modellnamen und wähle eine genaue Modell-ID. Haiku 4.5 hat keine separate Denkstufe."):$t("Lade den Modellkatalog und wähle ein Modell, um dessen unterstützte Denkstufen einzustellen.")}</p>
              <p class="field-note">{$t('Mit default verwendet die CLI ihren Standard. Installiere {provider} separat und melde dich dort im Terminal an. Die App nutzt diesen CLI-Zugang.',{provider:profile.model.provider==='codex'?'Codex':'Claude Code'})}</p>
              <p class="inline-note"><CircleHelp size={16}/>{$t("Das Programm läuft auf deinem Rechner. Die Modellverarbeitung erfolgt über den in der CLI eingerichteten Anbieter.")}</p>
            {:else}
              <label>{$t("API-Basis-URL")}<input bind:value={profile.model.baseUrl}/></label>
              <CredentialSelector botId={profile.id} purpose="dialog" config={profile.model} snapshot={apiSnapshot} disabled={saving||apiAccessBusy||updateInstalling} onChange={value=>profile.model.apiKeyId=value} onManage={()=>navigate('api-access')}/>
              <label>{$t("Modell")}<div class="input-action"><input bind:value={profile.model.model} list="models" placeholder={$t("Modellnamen eintragen")}/><button class="button secondary" onclick={loadModels} disabled={loadingModels}>{loadingModels?$t("Lädt …"):$t("Modelle laden")}</button></div><datalist id="models">{#each modelList as model}<option value={model}></option>{/each}</datalist></label>
              <label>{$t("Kreativität")} <span>{formatNumber(Number(profile.model.temperature),$locale,{minimumFractionDigits:1,maximumFractionDigits:1})}</span><input type="range" min="0" max="1.5" step="0.1" bind:value={profile.model.temperature}/></label>
              <div class="field-with-help"><div class="field-label"><label for="cohost-field-3">{$t("Maximale Antwortlänge")} </label><HelpTip text={$t("Begrenzt die vom API-Modell erzeugten Tokens. Kürzere Antworten werden meist schneller gesprochen. Ein sehr kleines Limit kann Antworten abschneiden.")}/></div><select id="cohost-field-3" bind:value={profile.model.maxTokens}><option value={256}>{$t("Kurz · 256 Tokens")}</option><option value={512}>{$t("Normal · 512 Tokens")}</option><option value={1024}>{$t("Ausführlich · 1.024 Tokens")}</option></select></div>
            {/if}
            <div class="toggle-row"><div><strong>{$t("Werkzeugaufrufe aktivieren")} <HelpTip text={$t("Erlaubt dem Modell, die für diesen Bot freigegebenen MCP-Werkzeuge, Skills und Emotes anzufordern. Installierte Erweiterungen benötigen zusätzlich eine Bot-Freigabe unter Skills & MCP.")}/></strong><small>{cliProvider?$t("MCP, Skills und Emotes werden von der App ausgeführt."):$t("Das ausgewählte Modell muss Tool Calling unterstützen.")}</small></div><input type="checkbox" role="switch" aria-label={$t("Werkzeugaufrufe aktivieren")} bind:checked={profile.model.toolsEnabled}/></div>
            <div class="toggle-row"><div><strong>{$t("Bilder an das Modell senden")} <HelpTip text={$t("Erlaubt echte Bildeingaben für diesen Bot: automatische Bildschirmaufnahmen sowie Bilder aus freigegebenen MCP-Werkzeugen. Bei Cloud- oder Remote-Anbietern werden die Bilder dort verarbeitet. Das gewählte Modell muss Bildverarbeitung unterstützen; diese Freigabe fügt einem Textmodell keine Sehfähigkeit hinzu.")}/></strong><small>{$t("Bildschirmkontext und Werkzeugbilder. Pro Bot steuerbar.")}</small></div><input type="checkbox" role="switch" aria-label={$t("Bilder an das Modell senden")} bind:checked={profile.model.visionEnabled}/></div>
            {#if selectedCliModel?.inputModalities?.length}<p class="field-note">{$t('Eingaben laut Modellkatalog: {modalities}.',{modalities:selectedCliModel.inputModalities.join(', ')})}{!selectedCliModel.inputModalities.includes('image')?$t(" Dieses Modell kann keine Bilder verarbeiten."):''}</p>{:else}<p class="field-note">{$t("Die Bildfähigkeit hängt vom konkreten Modell ab. Wähle bei lokalen Anbietern ein Vision-Modell; eine kompatible API allein garantiert keine Bilderkennung.")}</p>{/if}
            <div class="model-speed-settings"><div class="section-heading"><Zap size={18}/><div><h3>{$t("Antworttempo")}</h3><p>{$t("Weniger Kontext und kurze Antworten können die Wartezeit verkürzen.")}</p></div></div>
              {#if cliProvider}<button class="button secondary full" onclick={applyFastPreset} disabled={applyingFast||loadingCliModels||busy}><Zap size={14}/>{applyingFast?$t("Stellt ein …"):$t("Schnellvorgabe für den Live-Chat")}</button><p class="field-note">{profile.model.provider==='claude-code'?$t("Haiku, kurzer Antwortstil, eine Werkzeugrunde und zwölf Verlaufseinträge."):$t("Geringster unterstützter Denkaufwand des gewählten Modells, kurzer Stil, eine Werkzeugrunde und zwölf Verlaufseinträge.")} {$t("Einstellungen anschließend speichern.")}</p>{/if}
              <div class="field-with-help"><div class="field-label"><label for="cohost-field-4">{$t("Antwortstil")} </label><HelpTip text={$t("Zusätzliche Stilvorgabe für diesen Bot. Kurze Antworten passen zu einem schnellen Live-Chat. Persönlichkeit und Arbeitsregeln bleiben in PERSONA.md und AGENTS.md erhalten.")}/></div><select id="cohost-field-4" bind:value={profile.model.responseStyle}><option value="default">{$t("Wie in AGENTS.md / Persona")}</option><option value="short">{$t("Kurz · ein bis zwei Sätze")}</option><option value="normal">{$t("Normal · zwei bis vier Sätze")}</option><option value="detailed">{$t("Ausführlicher nach Bedarf")}</option></select></div><p class="field-note">{$t("Eine Stilvorgabe für das Modell; kein festes Tokenlimit.")}</p>
              <div class="field-row"><div class="field-with-help"><div class="field-label"><label for="cohost-field-5">{$t("Werkzeugrunden")} </label><HelpTip text={$t("Maximale Wechsel zwischen Modell und Werkzeugen vor der gesprochenen Antwort. Eine Runde kann mehrere Werkzeuge aufrufen. CUA-Abläufe mit Beobachten und Handeln benötigen oft mehrere Runden.")}/></div><select id="cohost-field-5" bind:value={profile.model.maxToolRounds} disabled={!profile.model.toolsEnabled}><option value={1}>{$t("1 · wenige Rückfragen")}</option><option value={2}>{$t("2 · ausgewogen")}</option><option value={4}>{$t("4 · komplexe Abläufe")}</option></select></div><div class="field-with-help"><div class="field-label"><label for="cohost-field-6">{$t("Verlauf im Kontext")} </label><HelpTip text={$t("Anzahl lokaler Gesprächseinträge, die dem Modell erneut übergeben werden. Weniger Einträge reduzieren Kontext und mögliche Wartezeit; ältere Details fehlen dann im Kurzzeitkontext.")}/></div><input id="cohost-field-6" type="number" min="2" max="100" bind:value={profile.memory.maxMessages} disabled={!profile.memory.enabled}/></div></div><p class="field-note">{$t("Eine Werkzeugrunde kann mehrere freigegebene Aktionen enthalten. Danach folgt die gesprochene Antwort. Die Vorbereitung der Audiodatei braucht zusätzlich Zeit.")}</p>
            </div>
            {#if !cliProvider}<p class="inline-note"><Cable size={16}/>{$t("Lokale Laufzeiten wie Ollama oder LM Studio startest du auf deinem Rechner. Die App verbindet sich mit ihrer API.")}</p>{/if}
          </section>
          <div class="settings-stack">
            <section class="settings-panel">
              <div class="section-heading"><Mic size={19}/><div><h2>{$t("Spracherkennung")}</h2><p>{$t("Für Mikrofonaufnahmen im Regiepult.")}</p></div></div>
              <label>{$t("Anbieter")}<select bind:value={profile.stt.provider} onchange={changeSttProvider}>
                <option value="whisper-local">{$t("Whisper · lokal")}</option><option value="elevenlabs">{$t("ElevenLabs · Cloud")}</option><option value="openai">{$t("OpenAI Whisper · Cloud")}</option><option value="compatible">{$t("Kompatible Transkriptions-API")}</option>
              </select></label>
              {#if profile.stt.provider==='whisper-local'}
                <label>{$t("Lokale Laufzeit")}<select bind:value={profile.stt.localBackend} onchange={changeWhisperBackend}><option value="whispercpp">whisper.cpp · whisper-server</option><option value="compatible">{$t("OpenAI-kompatibler Whisper-Server")}</option></select></label>
                <label>{$t("Lokale Server-URL")}<input bind:value={profile.stt.baseUrl} placeholder="http://127.0.0.1:8080"/></label>
                {#if profile.stt.localBackend==='compatible'}<label>{$t("Modell")}<input bind:value={profile.stt.model} placeholder="whisper-1"/></label>{/if}
                <p class="field-note">{$t("Der Whisper-Server mit deinem Modell muss separat auf diesem Rechner laufen. Bei whisper.cpp wird das Modell beim Serverstart geladen. Audio bleibt bei diesem Anbieter auf deinem Rechner.")}</p>
              {:else}
                {#if profile.stt.provider==='compatible'}<label>{$t("Transkriptions-API")}<input bind:value={profile.stt.baseUrl} placeholder="http://localhost:8000/v1"/></label>{/if}
                <label>{$t("Modell")}<input bind:value={profile.stt.model} placeholder={profile.stt.provider==='elevenlabs'?'scribe_v2':'whisper-1'}/></label>
                <CredentialSelector botId={profile.id} purpose="transcription" config={profile.stt} snapshot={apiSnapshot} disabled={saving||apiAccessBusy||updateInstalling} onChange={value=>profile.stt.apiKeyId=value} onManage={()=>navigate('api-access')}/>
                {#if ['openai','elevenlabs'].includes(profile.stt.provider)}<p class="field-note">{$t('Deine Aufnahme wird zur Transkription an {provider} gesendet.',{provider:profile.stt.provider==='elevenlabs'?'ElevenLabs':'OpenAI'})}</p>{/if}
              {/if}
              {#if profile.stt.provider==='whisper-local'}<CredentialSelector botId={profile.id} purpose="transcription" config={profile.stt} snapshot={apiSnapshot} disabled={saving||apiAccessBusy||updateInstalling} onChange={value=>profile.stt.apiKeyId=value} onManage={()=>navigate('api-access')}/>{/if}
              <label>{$t("Sprache")}<input bind:value={profile.stt.language} placeholder={$t("de · leer = automatisch erkennen")}/></label>
              <p class="field-note">{$t("Mikrofonaufnahmen werden auf deinem Rechner in Mono-WAV umgewandelt. Dialog, Erkennung und Stimme lassen sich unabhängig kombinieren.")}</p>
            </section>
            <section class="settings-panel tinted"><h2>{$t("Deine Kombination.")}</h2><p>{$t("Codex für Dialoge, lokales Whisper zum Hören und ElevenLabs zum Sprechen? Jede Integration hat ihre eigene Auswahl und ihren eigenen Zugang.")}</p><div class="integration-flow"><span><BrainCircuit size={17}/>{$t("Dialog")}</span><span><Mic size={17}/>{$t("Hören")}</span><span><Volume2 size={17}/>{$t("Sprechen")}</span></div></section>
          </div>
        </div>

        {#key profile.id}<ScreenContextPanel botId={profile.id} {permissionContext} prepareSelection={prepareScreenSource} onError={message=>notify(message,true)} bind:config={profile.screenContext} visionEnabled={profile.model.visionEnabled} disabled={updateInstalling}/>{/key}
      {:else if page==='connections'}
        <p class="field-note">{$t("Eine Verbindung pro Plattform gilt für die ganze Crew. Der Bot beim Verbinden ist ihr Inhaber. Andere Bots dürfen sprechen. Für Chatantworten müssen auch ihre Schlüsselbund-Referenzen exakt übereinstimmen; neue Bots mit eigenen Referenzen werden nicht über das Konto des Inhabers schreiben. Nach Änderungen neu verbinden.")}</p><div class="settings-grid"><section class="settings-panel"><div class="connection-heading"><span class="platform-logo twitch-bg">▰</span><div><h2>Twitch</h2><p>EventSub WebSocket</p></div><span class="connection-state" class:connected={connection.twitch==='connected'}>{connection.twitch==='connected'?$t("Verbunden"):connection.twitch==='connecting'?$t("Verbindet …"):$t("Nicht verbunden")}</span></div>{#if connectionOwner.twitch}<p class="field-note">{$t("Verbindungsinhaber: {name}",{name:connectionOwner.twitch})}</p>{/if}<label>{$t("Client-ID")}<input bind:value={profile.chat.twitch.clientId} placeholder={$t("Deine Twitch-Anwendung")}/></label><div class="field-row"><label>{$t("Broadcaster-ID")}<input bind:value={profile.chat.twitch.broadcasterId}/></label><label>{$t("Bot-Nutzer-ID")}<input bind:value={profile.chat.twitch.userId}/></label></div><label>{$t("OAuth-Token")} <span class="saved-tag">{secrets.includes(profile.chat.twitch.secretId)?$t("Gespeichert"):$t("Benötigt user:read:chat")}</span><div class="input-action"><input type="password" autocomplete="off" bind:value={secretInputs[profile.chat.twitch.secretId]} placeholder={$t("Token deiner Twitch-Anwendung")}/><button class="button secondary" onclick={()=>storeSecret(profile.chat.twitch.secretId)}>{$t("Speichern")}</button></div></label><CredentialHelp/><button class="button secondary full" onclick={()=>platformAction('twitch')}><Plug size={15}/>{['connected','connecting'].includes(connection.twitch)?$t("Verbindung trennen"):$t("Twitch verbinden")}</button>{#if connectionMessages.twitch}<p class="field-note">{connectionMessages.twitch}</p>{/if}<div class="toggle-row"><div><strong>{$t("Antworten auch in den Chat senden")}</strong><small>{$t("Benötigt zusätzlich user:write:chat.")}</small></div><input type="checkbox" role="switch" aria-label={$t("Antworten auch in den Chat senden")} bind:checked={profile.chat.twitch.sendReplies}/></div><p class="field-note">{$t("Verwende ein OAuth-Token deiner eigenen Twitch-App mit Chat-Leserechten. Anmeldung und Token-Erstellung erfolgen bei Twitch.")}</p></section><section class="settings-panel"><div class="connection-heading"><span class="platform-logo youtube-bg"><Youtube size={24}/></span><div><h2>YouTube</h2><p>{$t("Live-Chat")}</p></div><span class="connection-state" class:connected={connection.youtube==='connected'}>{connection.youtube==='connected'?$t("Verbunden"):connection.youtube==='connecting'?$t("Verbindet …"):$t("Nicht verbunden")}</span></div>{#if connectionOwner.youtube}<p class="field-note">{$t("Verbindungsinhaber: {name}",{name:connectionOwner.youtube})}</p>{/if}<label>{$t("Live-Video-URL oder Video-ID")}<input bind:value={profile.chat.youtube.videoId} placeholder="https://youtube.com/watch?v=…"/></label><label>{$t("Live-Chat-ID")} <span>{$t("Optional")}</span><input bind:value={profile.chat.youtube.liveChatId} placeholder={$t("Wird aus dem Live-Video ermittelt")}/></label><CredentialSelector botId={profile.id} purpose="youtube_data" config={profile.chat.youtube} snapshot={apiSnapshot} disabled={saving||apiAccessBusy||updateInstalling} onChange={value=>profile.chat.youtube.apiKeyId=value} onManage={()=>navigate('api-access')}/><button class="button secondary full" onclick={()=>platformAction('youtube')}><Plug size={15}/>{['connected','connecting'].includes(connection.youtube)?$t("Verbindung trennen"):$t("YouTube verbinden")}</button>{#if connectionMessages.youtube}<p class="field-note">{connectionMessages.youtube}</p>{/if}<div class="toggle-row"><div><strong>{$t("Antworten auch in den Chat senden")}</strong><small>{$t("Benötigt ein separates OAuth-Token mit YouTube-Schreibrecht.")}</small></div><input type="checkbox" role="switch" aria-label={$t("Antworten auch in den Chat senden")} bind:checked={profile.chat.youtube.sendReplies}/></div>{#if profile.chat.youtube.sendReplies}<label>{$t("OAuth-Token für Chatantworten")} <span class="saved-tag">{secrets.includes(profile.chat.youtube.sendSecretId||'youtube-write')?$t("Gespeichert"):$t("Nicht gesetzt")}</span><div class="input-action"><input type="password" autocomplete="off" bind:value={secretInputs[profile.chat.youtube.sendSecretId||'youtube-write']} placeholder={$t("OAuth-Token deiner Google-Anwendung")}/><button class="button secondary" onclick={()=>storeSecret(profile.chat.youtube.sendSecretId||'youtube-write')}>{$t("Speichern")}</button></div></label><CredentialHelp/>{/if}<p class="field-note">{$t("Der Chat wird mit dem von YouTube vorgegebenen Abrufintervall gelesen. Beide Plattformen können gleichzeitig verbunden sein.")}</p></section><section class="settings-panel span-two"><div class="section-heading"><SlidersHorizontal size={19}/><div><h2>{$t("Gesprächssteuerung")}</h2><p>{$t("Damit aus vielen Nachrichten ein gutes Gespräch wird.")}</p></div></div><div class="three-fields"><div class="field-with-help"><div class="field-label"><label for="cohost-field-7">{$t("Ansprache / Auslöser")} </label><HelpTip text={$t("Filtert Nachrichten aus dem Stream-Chat für diesen Bot. Nur Nachrichten mit dieser Zeichenfolge werden automatisch berücksichtigt. Der Mikrofon-Weckruf wird getrennt im Regiepult eingestellt.")}/></div><input id="cohost-field-7" bind:value={profile.chat.trigger} placeholder={$t("Leer = alle Nachrichten")}/></div><label>{$t("Warteschlange maximal")}<input type="number" min="1" max="200" bind:value={profile.chat.maxQueue}/></label><div class="field-with-help"><div class="field-label"><label for="cohost-field-8">{$t("Nachrichten verfallen nach (Sek.)")} </label><HelpTip text={$t("Verwirft wartende Chatnachrichten nach dieser Zeit, damit der Cohost nicht auf längst überholte Themen antwortet.")}/></div><input id="cohost-field-8" type="number" min="10" max="600" bind:value={profile.chat.expirySeconds}/></div></div></section></div>

      {:else if page==='extensions'}
        <ExtensionsPanel bind:profile {library} {native} {busy} onSave={saveCurrentBot} beforeSave={()=>dirty||documentDirty?saveCurrentBot():Promise.resolve(true)} onRefresh={refreshLibrary} onNotify={notify} onEditingChange={editing=>extensionEditing=editing} onDirtyChange={value=>extensionDirty=value} onBusyChange={value=>extensionBusy=value} confirmBeforeDiscard={requestDiscard}/>

      {:else if page==='instructions'}
        <div class="document-layout"><section class="document-panel"><div class="document-tabs"><button class:active={instructionTab==='agents'} onclick={()=>instructionTab='agents'}><FileText size={15}/>AGENTS.md</button><button class:active={instructionTab==='persona'} onclick={()=>instructionTab='persona'}><Sparkles size={15}/>PERSONA.md</button><span>Markdown</span></div><textarea class="document-editor" spellcheck="false" bind:value={documents[instructionTab]} aria-label={instructionTab==='agents'?$t("AGENTS.md bearbeiten"):$t("PERSONA.md bearbeiten")}></textarea><div class="document-footer"><span>{$t("Änderungen gelten ab der nächsten Antwort.")}</span><span>{$t('{count} Zeichen',{count:formatNumber(documents[instructionTab].length,$locale)})}</span></div></section><aside class="settings-stack"><section class="settings-panel"><h2>{instructionTab==='agents'?$t("Wie dein Agent arbeitet."):$t("Wer dein Charakter ist.")}</h2><p>{instructionTab==='agents'?$t("Lege fest, wie dein Cohost Antworten formuliert, Werkzeuge verwendet und mit deinen Zuschauern umgeht."):$t("Gib deinem Charakter eine Geschichte, eine Haltung und einen eigenen Humor. Diese Beschreibung bleibt beim Modellwechsel erhalten.")}</p><div class="document-hint"><Check size={15}/><span>{$t("Persönliche Anweisungen")}</span></div><div class="document-hint"><Check size={15}/><span>{$t("Unabhängig vom Anbieter")}</span></div><div class="document-hint"><Check size={15}/><span>{$t("Als Datei bearbeitbar")}</span></div></section><section class="settings-panel"><h2>{$t("Dein Profil mitnehmen.")}</h2><p>{$t("Exportiere Charakter und Anweisungen. Zugangsdaten bleiben im Schlüsselbund.")}</p><div class="button-row"><button class="button secondary small" onclick={exportProfile}><Download size={14}/>{$t("Exportieren")}</button><button class="button secondary small" onclick={importProfile}><Upload size={14}/>{$t("Importieren")}</button></div>{#if appDataDir}<button class="text-link folder-link" onclick={()=>openPath(appDataDir)}><FolderOpen size={14}/>{$t("Profilordner öffnen")}</button>{/if}</section></aside></div>

      {:else if page==='memory'}
        <section class="settings-panel knowledge-panel"><div class="section-heading"><FileText size={19}/><div><h2>{$t("Deine Wissensbibliothek")}</h2><p>{$t("Kanalinfos, PDFs und Fakten, die du selbst hinterlegst.")}</p></div><button class="button secondary small" onclick={importKnowledge}><Plus size={14}/>{$t("Datei hinzufügen")}</button></div>
          {#if knowledge.length}<div class="knowledge-list">{#each knowledge as file}<div><FileText size={17}/><strong>{file.name}</strong><span>{formatNumber(file.size/1024,$locale,{maximumFractionDigits:1})} KB</span><button class="icon-button danger-text" title={$t('{name} entfernen',{name:file.name})} aria-label={$t('{name} entfernen',{name:file.name})} onclick={()=>confirmKnowledgeRemoval(file.name)}><Trash2 size={15}/></button></div>{/each}</div>{:else}<p class="field-note">{$t("Importiere PDF, TXT, Markdown, JSON oder CSV. Passende Textstellen werden vor jeder Antwort automatisch gesucht und mit Dateiname und gegebenenfalls PDF-Seite an das Dialogmodell übergeben.")}</p>{/if}
          <p class="field-note">{$t("Die Stichwortsuche läuft lokal und benötigt keine Modell-Werkzeuge. PDFs bis 20 MiB und 500 Seiten, Text bis 4 MiB. Gescannte PDFs benötigen vorher eine Texterkennung. Bei Cloud-Dialogmodellen gehen relevante Auszüge an den gewählten Anbieter.")}</p>
        </section>
        <section class="settings-panel knowledge-panel"><div class="section-heading"><BrainCircuit size={19}/><div><h2>{$t("Hindsight · Langzeitgedächtnis")}</h2><p>{$t("Erinnerungen automatisch abrufen und neue Gespräche speichern.")}</p></div></div>
          <div class="toggle-row"><div><strong>{$t("Hindsight verwenden")} <HelpTip text={$t("Bindet deinen Hindsight-Server als Langzeitgedächtnis ein. Relevante Erinnerungen können automatisch abgerufen und neue Gespräche gespeichert werden. Server und Bank gehören zu diesem Bot.")}/></strong><small>{$t("Benötigt deinen eigenen lokalen oder entfernten Hindsight-Server.")}</small></div><input type="checkbox" role="switch" aria-label={$t("Hindsight verwenden")} bind:checked={profile.hindsight.enabled}/></div>
          {#if profile.hindsight.enabled}
            <div class="field-row"><label>{$t("Server-Basis-URL")}<input bind:value={profile.hindsight.baseUrl} placeholder="http://127.0.0.1:8888"/></label><div class="field-with-help"><div class="field-label"><label for="cohost-field-9">{$t("Memory-Bank")} </label><HelpTip text={$t("Bestimmt den Erinnerungsspeicher auf deinem Hindsight-Server. Leer verwendet die Bot-ID. Dieselbe Bank-ID bei mehreren Bots teilt deren Langzeitgedächtnis.")}/></div><input id="cohost-field-9" bind:value={profile.hindsight.bankId} placeholder={profile.id}/></div></div>
            <p class="field-note">{$t("Ohne eigene Bank-ID wird die Bot-ID verwendet. Jeder neue Bot bekommt damit seine eigene Bank. Trage nur absichtlich dieselbe Bank bei mehreren Bots ein.")}</p>
            <CredentialSelector botId={profile.id} purpose="hindsight" config={profile.hindsight} snapshot={apiSnapshot} disabled={saving||apiAccessBusy||updateInstalling} onChange={value=>profile.hindsight.apiKeyId=value} onManage={()=>navigate('api-access')}/>
            <div class="toggle-row"><div><strong>{$t("Erinnerungen automatisch abrufen")} <HelpTip text={$t("Sucht vor jeder Antwort passende Hindsight-Erinnerungen und gibt sie dem Dialogmodell als Hintergrund. Bei Cloud-Modellen werden diese Auszüge an den Anbieter übermittelt.")}/></strong><small>{$t("Relevante Erinnerungen als Hintergrund vor jeder Antwort einfügen.")}</small></div><input type="checkbox" role="switch" aria-label={$t("Erinnerungen automatisch abrufen")} bind:checked={profile.hindsight.autoRecall}/></div>
            <div class="toggle-row"><div><strong>{$t("Neue Gespräche automatisch speichern")} <HelpTip text={$t("Speichert die neue Nachricht und die präsentierte Antwort in Hindsight, sobald die Ausgabe beginnt. So können spätere Gespräche darauf Bezug nehmen.")}/></strong><small>{$t("Überträgt die neue Nachricht und Antwort, sobald die Ausgabe beginnt.")}</small></div><input type="checkbox" role="switch" aria-label={$t("Neue Gespräche automatisch speichern")} bind:checked={profile.hindsight.autoRetain}/></div>
          {/if}
          <p class="field-note">{$t("Alte Verläufe und Wissensdateien werden nicht gesammelt hochgeladen. Hindsight kann selbst Cloud-Modelle oder Embeddings nutzen; das hängt von deinem Server ab. Bei einem Ausfall läuft das Gespräch ohne Erinnerungen weiter. Erinnerungen verwaltest und löschst du auf deinem Hindsight-Server.")}</p>
        </section>
        <div class="memory-header settings-panel"><div><h2>{$t("Lokaler Gesprächsverlauf")}</h2><p>{$t("Kurzzeitkontext für nächste Antworten. Leeren löscht keine Hindsight-Erinnerungen.")}</p></div><label class="memory-toggle"><span>{$t("Kurzzeitkontext verwenden")}</span><input type="checkbox" role="switch" bind:checked={profile.memory.enabled}/></label><button class="button secondary small" onclick={confirmHistoryClear}><Trash2 size={14}/>{$t("Verlauf leeren")}</button></div><div class="history-list">{#if !history.length}<div class="large-empty"><History size={36} strokeWidth={1.2}/><h2>{$t("Ein frischer Anfang.")}</h2><p>{$t("Gespräche mit deinem Cohost erscheinen hier.")}</p><button class="text-link" onclick={()=>navigate('studio')}>{$t("Zum Regiepult")} <ArrowUpRight size={14}/></button></div>{:else}{#each history as entry}<article class="history-entry" class:assistant={entry.role==='assistant'}><span class="history-avatar">{entry.role==='assistant'?profile.name.charAt(0):'S'}</span><div><div><strong>{entry.role==='assistant'?profile.name:entry.user||'Streamer'}</strong><span>{entry.source||'operator'}</span><time>{timeOf(entry.createdAt)}</time></div><p>{entry.text}</p></div></article>{/each}{/if}</div>
      {/if}

      {#if page==='character'}<section class="settings-panel emote-settings"><div class="emote-section-heading"><div class="section-heading"><PartyPopper size={19}/><div><h2>{$t("Eigene Emotes")}</h2><p>{$t("Winken, verstecken, freuen. Oder eine Animation deiner importierten Figur.")}</p></div></div><button class="button secondary small" disabled={!availableEmoteActions.length} onclick={addEmote}><Plus size={14}/>{$t("Emote hinzufügen")}</button></div><div class="emote-grid">{#each profile.emotes || [] as emote}<div class="emote-card"><div class="emote-card-heading"><input aria-label={$t("Emote-Name")} bind:value={emote.name}/><button class="icon-button danger-text" title={$t("Emote entfernen")} aria-label={$t("Emote entfernen")} onclick={()=>profile.emotes=profile.emotes.filter((e:any)=>e.id!==emote.id)}><Trash2 size={14}/></button></div><div class="field-with-help"><div class="field-label"><label for={'cohost-field-10-'+emote.id}>{$t("Aktion oder Clipname")} </label><HelpTip text={$t("Wähle eine tatsächlich vorhandene Aktion dieser Figur. Veraltete Einträge bleiben sichtbar, bis du sie ersetzt oder entfernst.")}/></div><select id={'cohost-field-10-'+emote.id} value={resolveAvailableAction(availableEmoteActions,emote.action)?.action||emote.action} onchange={event=>emote.action=event.currentTarget.value}>{#if !emoteAvailable(emote)}<option value={emote.action} disabled>{emote.action} · {$t("Für diese Figur nicht verfügbar")}</option>{/if}{#each availableEmoteActions as action}<option value={action.action}>{action.kind==='gesture'?$t({wave:'Winken',hide:'Verstecken',celebrate:'Freuen',nod:'Nicken',dance:'Tanzen'}[action.action]||action.name):action.name}</option>{/each}</select>{#if !emoteAvailable(emote)}<p class="field-note" role="status">{$t(unavailableEmote)}</p>{/if}</div><div class="field-row"><label>{$t("Dauer (Sekunden)")}<input type="number" min="0.2" max={emote.action?.startsWith('vrma:')?120:15} disabled={emote.action?.startsWith('vrma:')} step="0.1" bind:value={emote.duration}/></label><label>{$t("Intensität")}<input type="number" min="0" max="2" step="0.1" bind:value={emote.intensity}/></label></div><div class="field-with-help"><div class="field-label"><label for={'cohost-field-11-'+emote.id}>{$t("Sound")} </label><HelpTip text={$t("Optionaler Klang, der mit dieser Geste startet. Wähle einen integrierten Effekt oder importiere einen eigenen Clip. WAV ist besonders portabel; die verfügbaren weiteren Formate hängen vom Systemdecoder ab. Maximal 5 MiB, Wiedergabe höchstens 15 Sekunden.")}/></div><select id={'cohost-field-11-'+emote.id} value={emote.sound||''} onchange={event=>emote.sound=event.currentTarget.value}><option value="">{$t("Kein Sound")}</option><option value="builtin:wave">{$t("Heller Gruß")}</option><option value="builtin:cheer">{$t("Freudenklang")}</option><option value="builtin:whoosh">{$t("Whoosh")}</option>{#if emote.sound?.startsWith('custom:')}<option value={emote.sound}>{$t("Eigener Audioclip")}</option>{/if}</select></div><div class="emote-sound-row"><button class="button secondary small" onclick={()=>importEmoteSound(emote.id)} disabled={busy||saving}><Upload size={13}/>{$t("Sound importieren")}</button></div>{#if emote.sound}<label>{$t("Soundlautstärke")} <span>{formatNumber(Math.round((emote.soundVolume??0.7)*100),$locale)} %</span><input aria-label={$t('Soundlautstärke für {name}',{name:emote.name})} type="range" min="0" max="1" step="0.05" value={emote.soundVolume??0.7} oninput={event=>emote.soundVolume=Number(event.currentTarget.value)}/></label>{/if}<div class="toggle-row"><div><strong>{$t("Für Chat-Anfragen")} <HelpTip text={$t("Gibt dieses Emote zusätzlich für Zuschaueranfragen frei. Ohne diese Freigabe kann der Streamer es weiterhin selbst auslösen.")}/></strong><small>{$t("Der Agent darf dieses Emote auf Zuschauerwunsch ausführen.")}</small></div><input type="checkbox" role="switch" aria-label={$t("Für Chat-Anfragen")} bind:checked={emote.allowChat}/></div><button class="button secondary full" disabled={!emoteAvailable(emote)} onclick={()=>triggerEmote(emote.id)}><Play size={13}/>{$t("Ausführen")}</button></div>{/each}</div><p class="field-note">{$t("Eigene Emotes können vorhandene Animationsclips einer importierten GLB-Datei über ihren Namen verwenden. Bewegungen bei VRM werden an die vorhandenen Humanoidknochen angepasst.")}</p></section><section class="settings-panel output-settings"><div class="section-heading"><Monitor size={19}/><div><h2>{$t("Darstellung & Ausgabe")}</h2><p>{$t("Dein Cohost bekommt seinen eigenen Platz.")}</p></div></div><div class="three-fields"><label>{$t("Ausgabe")}<select bind:value={profile.output.mode} onchange={()=>profile.output.mode==='overlay'?openCharacterWindow():profile.output.mode==='obs'?copyObs():useStudio()}><option value="studio">{$t("Studioansicht")}</option><option value="overlay">{$t("Desktop-Overlay")}</option><option value="obs">{$t("OBS-Browserquelle")}</option></select></label><label>{$t("Bildrate")}<select bind:value={profile.output.fps}><option value={30}>30 FPS</option><option value={60}>60 FPS</option></select></label><div class="button-row align-end"><button class="button secondary" onclick={openCharacterWindow}><MoveUpRight size={14}/>{$t("Figur öffnen")}</button><button class="button secondary" onclick={copyObs}><Copy size={14}/>{$t("OBS-Link")}</button></div></div><div class="toggle-row"><div><strong>{$t("Immer im Vordergrund")} <HelpTip text={$t("Hält das separate Figurenfenster über anderen Fenstern. Nützlich als Desktop-Begleiter; die Unterstützung hängt vom Betriebssystem ab.")}/></strong><small>{$t("Abhängig von Betriebssystem und Fenstermanager.")}</small></div><input type="checkbox" role="switch" aria-label={$t("Immer im Vordergrund")} bind:checked={profile.output.alwaysOnTop} onchange={configureOverlay}/></div><div class="toggle-row"><div><strong>{$t("Mausklicks durchreichen")} <HelpTip text={$t("Klicks gehen durch das Figurenfenster an das darunterliegende Programm. Zum Verschieben oder Bedienen zuerst hier wieder ausschalten.")}/></strong><small>{$t("Zum Bearbeiten im Steuerungsfenster wieder ausschalten.")}</small></div><input type="checkbox" role="switch" aria-label={$t("Mausklicks durchreichen")} bind:checked={profile.output.clickThrough} disabled={!!selectedAutopilot?.active&&!!selectedAutopilot?.wander} onchange={configureOverlay}/></div><div class="toggle-row"><div><strong>{$t("Untertitel anzeigen")} <HelpTip text={$t("Zeigt den bereinigten Antworttext ab dem Beginn der Sprachausgabe. Er verschwindet 15 Sekunden nach dem Ende der gesprochenen Antwort.")}/></strong><small>{$t("Gesprochener Text unter der Figur.")}</small></div><input type="checkbox" role="switch" aria-label={$t("Untertitel anzeigen")} bind:checked={profile.output.showSubtitles}/></div>{#if overlayUrl}<label>{$t("OBS-Browserquelle")}<input readonly value={overlayUrl}/></label>{/if}</section>{/if}
      <div hidden={page!=='api-access'}><ApiAccessPanel bind:this={apiPanel} visible={page==='api-access'} disabled={updateInstalling||saving||switchingBot||creatingBot} onSnapshot={acceptApiSnapshot} onDirtyChange={value=>apiAccessDirty=value} onBusyChange={value=>apiAccessBusy=value} onLocked={stopAll}/></div>
      <footer class="workspace-footer"><span>OrbitPal</span><span>{$t("Gemacht für deinen nächsten Stream.")}</span><span><span class="status-dot" class:online={native}></span>{native?$t("Lokal auf deinem Rechner"):$t("Vorschau · Integrationen in der Desktop-App")}</span></footer>
    </main>
  </div>
</div>

<SystemPermissionDialog/>

<UpdatesPanel bind:open={updatesOpen} busyReason={updateBusyReason} onInstalling={lockForUpdate} onAvailable={version=>availableUpdate=version}/>

{#if toast}<div class="toast" class:error role="status">{#if error}<CircleHelp size={18}/>{:else}<CheckCheck size={18}/>{/if}<span>{toast}</span><button onclick={()=>toast=''} aria-label={$t("Meldung schließen")}><X size={15}/></button></div>{/if}


{#if botDialog}<div class="modal-backdrop" role="presentation" onclick={(event)=>{if(event.target===event.currentTarget&&!creatingBot)botDialog=false}}><div class="modal bot-create-dialog" role="dialog" aria-modal="true" aria-labelledby="bot-create-heading" tabindex="-1" use:focusTrap><form onsubmit={(event)=>{event.preventDefault();createBot()}}><div class="modal-heading"><div><h2 id="bot-create-heading">{$t("Ein neues Crew-Mitglied.")}</h2><p>{$t("Jeder Bot bekommt sein eigenes Profil.")}</p></div><button type="button" class="icon-button" onclick={()=>botDialog=false} disabled={creatingBot} aria-label={$t("Dialog schließen")}><X size={20}/></button></div><label>{$t("Bot-Name")}<input data-autofocus bind:value={botName} placeholder={$t("Wie heißt dein neuer Cohost?")} maxlength="80" required/></label><label>{$t("Charakter")}<select bind:value={botCharacter}>{#each characterCatalog as character}<option value={character.id}>{character.name}</option>{/each}</select></label><label class="bot-copy-option"><input type="checkbox" bind:checked={botCopy}/><span><strong>{$t('Einstellungen von {name} übernehmen',{name:profile.name})}</strong><small>{$t("Modell, Stimme, Skills und Anweisungen kopieren. Zugangsdaten bleiben als Verweise im Schlüsselbund.")}</small></span></label><p class="field-note">{$t("Du kannst alle Bots parallel einblenden und ihnen eigene OBS-Quellen geben. Welches Profil du hier bearbeitest, ändert die anderen Figurenfenster nicht.")}</p><div class="modal-footer"><button type="button" class="button secondary" onclick={()=>botDialog=false} disabled={creatingBot}>{$t("Abbrechen")}</button><button type="submit" class="button primary" disabled={!botName.trim()||creatingBot}><Plus size={15}/>{creatingBot?$t("Legt Bot an …"):$t("Bot anlegen")}</button></div></form></div></div>{/if}

{#if shortcutHelp}<div class="modal-backdrop" role="presentation" onclick={event=>{if(event.target===event.currentTarget)shortcutHelp=false}}><div class="modal shortcut-dialog" role="dialog" aria-modal="true" aria-labelledby="shortcut-heading" tabindex="-1" use:focusTrap><div class="modal-heading"><div><h2 id="shortcut-heading">{$t("Tastatursteuerung")}</h2><p>{$t("Auch Hilfesymbole lassen sich mit Tab erreichen.")}</p></div><button class="icon-button" onclick={()=>shortcutHelp=false} aria-label={$t("Tastaturhilfe schließen")}><X size={20}/></button></div><dl class="shortcut-list"><div><dt>{$t("Änderungen speichern")}</dt><dd><kbd>{$t("Strg / ⌘")}</kbd> + <kbd>S</kbd></dd></div><div><dt>{$t("Nachricht senden")}</dt><dd><kbd>{$t("Strg / ⌘")}</kbd> + <kbd>Enter</kbd></dd></div><div><dt>{$t("Nachrichtenfeld fokussieren")}</dt><dd><kbd>{$t("Strg / ⌘")}</kbd> + <kbd>K</kbd></dd></div><div><dt>{$t("Bereich wechseln")}</dt><dd><kbd>Alt / ⌥</kbd> + <kbd>1–9</kbd></dd></div><div><dt>{$t("Dialog schließen, sonst Ausgabe stoppen")}</dt><dd><kbd>Esc</kbd></dd></div><div><dt>{$t("Diese Hilfe öffnen")}</dt><dd><kbd>?</kbd> {$t("oder")} <kbd>F1</kbd></dd></div></dl><p class="field-note">{$t("Bereichswechsel und Fragezeichen-Kürzel greifen nicht beim Schreiben. Tab und Umschalt+Tab führen durch die Bedienelemente; Enter oder Leertaste aktiviert fokussierte Schaltflächen.")}</p></div></div>{/if}

{#if confirmation}<ConfirmDialog title={confirmation.title} message={confirmation.message} confirmLabel={confirmation.confirmLabel} danger={!!confirmation.danger} pending={confirming} onCancel={()=>{if(!confirming)confirmation=undefined}} onConfirm={confirmAction}/>{/if}
