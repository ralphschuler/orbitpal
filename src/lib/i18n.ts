import { derived, get, writable } from 'svelte/store';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen } from '@tauri-apps/api/event';
import app from './locales/app.json';
import panels from './locales/panels.json';
import extensions from './locales/extensions.json';
import system from './locales/system.json';
import outputPresentation from './locales/output-presentation.json';
import autopilotStatus from './locales/autopilot-status.json';
import presentation from './locales/presentation.json';
import microphone from './locales/microphone.json';
import gltf from './locales/gltf.json';
import transports from './locales/transports.json';
import screenPermission from './locales/screen-permission.json';
import systemPermission from './locales/system-permission.json';
import screen from './locales/screen.json';
import motions from './locales/motions.json';
import emotes from './locales/emotes.json';
import avatarAudio from './locales/avatar-audio.json';
import usageBudget from './locales/usage-budget.json';
import lifeRuntime from './locales/life-runtime.json';
import lifeEditor from './locales/life-editor.json';
import lifeActions from './locales/life-actions.json';
import worldState from './locales/world-state.json';
import worldOutput from './locales/world-output.json';
import credentials from './locales/credentials.json';
import credentialSession from './locales/credential-session.json';
import apiAccess from './locales/api-access.json';
import experience from './locales/experience.json';
import type { Locale, Parameters, TranslationCatalogue } from './i18n-types';
export type { Locale, Parameters } from './i18n-types';

export const supportedLocales: readonly {code:Locale;label:string}[] = [
  {code:'de',label:'Deutsch'}, {code:'en',label:'English'},
  {code:'fr',label:'Français'}, {code:'es',label:'Español'},
];
export const localeStorageKey = 'streamer-ai-cohost.ui-language';
export const isLocale = (value:unknown):value is Locale => typeof value==='string' && supportedLocales.some(item=>item.code===value);
export function detectLocale(languages:readonly string[]):Locale {
  for(const language of languages){const code=language.toLowerCase().split(/[-_]/)[0];if(isLocale(code))return code;}
  return 'en';
}
export const resolveLocale=(saved:unknown,languages:readonly string[]):Locale=>isLocale(saved)?saved:detectLocale(languages);
function initialLocale():Locale {
  const languages=typeof navigator==='undefined'?[]:navigator.languages?.length?navigator.languages:[navigator.language];
  try {return resolveLocale(globalThis.localStorage?.getItem(localeStorageKey),languages);}catch{/* Storage can be disabled. */}
  return detectLocale(languages);
}
const current=writable<Locale>(initialLocale());
export const locale={subscribe:current.subscribe};
export const catalogues = [experience,lifeEditor,worldOutput, apiAccess, lifeActions, worldState, lifeRuntime, usageBudget, credentialSession, credentials, presentation, autopilotStatus, outputPresentation, system, microphone, gltf, transports, avatarAudio, motions, emotes, screenPermission, systemPermission, panels, extensions, app, screen] as TranslationCatalogue[];
const messages:TranslationCatalogue={en:{},fr:{},es:{}};
for(const catalogue of catalogues)for(const language of ['en','fr','es'] as const)Object.assign(messages[language],catalogue[language]);
const own=(language:Exclude<Locale,'de'>,key:string)=>Object.hasOwn(messages[language],key)?messages[language][key]:undefined;
const escapeRegex=(value:string)=>value.replace(/[.*+?^${}()|[\]\\]/g,'\\$&');
const patterns=[...new Set(Object.keys(messages.en))].flatMap(source=>{
  const placeholders=[...source.matchAll(/\{([A-Za-z][A-Za-z0-9_]*)\}/g)];
  const staticLength=source.replace(/\{[A-Za-z][A-Za-z0-9_]*\}/g,'').length;
  if(!placeholders.length||staticLength<12)return [];
  let expression='^',offset=0;
  for(const placeholder of placeholders){expression+=escapeRegex(source.slice(offset,placeholder.index))+'([\\s\\S]*?)';offset=placeholder.index!+placeholder[0].length;}
  expression+=escapeRegex(source.slice(offset))+'$';
  return [{source,staticLength,keys:placeholders.map(placeholder=>placeholder[1]),expression:new RegExp(expression)}];
}).sort((a,b)=>b.staticLength-a.staticLength);

export function interpolate(template:string,parameters:Parameters={}):string {
  return template.replace(/\{([A-Za-z][A-Za-z0-9_]*)\}/g,(token,key)=>Object.hasOwn(parameters,key)?String(parameters[key]):token);
}
export function translate(source:string,parameters:Parameters={},language:Locale=get(current)):string {
  if(!isLocale(language))language='en';
  if(language==='de')return interpolate(source,parameters);
  const exact=own(language,source);
  if(exact!==undefined)return interpolate(exact,parameters);
  if(!Object.keys(parameters).length&&source.length<=8192){
    for(const pattern of patterns){const matched=pattern.expression.exec(source);if(!matched)continue;
      const values:Parameters={};pattern.keys.forEach((key,index)=>{values[key]=matched[index+1];});
      return interpolate(own(language,pattern.source)??pattern.source,values);
    }
  }
  return interpolate(source,parameters);
}
export const t=derived(current,language=>(source:string,parameters:Parameters={})=>translate(source,parameters,language));
export function setLocale(language:Locale):void {
  if(!isLocale(language))return;
  current.set(language);
  try{globalThis.localStorage?.setItem(localeStorageKey,language);}catch{/* The in-memory selection remains usable. */}
}
export function formatNumber(value:number,language:Locale=get(current),options:Intl.NumberFormatOptions={}):string {
  return new Intl.NumberFormat(language,options).format(value);
}
export function formatDate(value:string|number|Date,options:Intl.DateTimeFormatOptions={},language:Locale=get(current)):string {
  const date=value instanceof Date?value:new Date(value);
  return Number.isFinite(date.getTime())?new Intl.DateTimeFormat(language,options).format(date):'';
}
// Only app-owned messages should use this helper. Chat, document content,
// model output, identifiers and provider diagnostics are never rewritten.
export function localizeMessage(source:string,language:Locale=get(current)):string {
  return translate(source,{},language);
}
let nativeRevision=0;
let nativeQueue:Promise<unknown>=Promise.resolve();
const nativeWindow=Promise.resolve(isTauri()?getCurrentWindow():undefined);
current.subscribe(language=>{
  if(typeof document!=='undefined')document.documentElement.lang=language;
  const revision=++nativeRevision;
  nativeQueue=nativeQueue.catch(()=>{}).then(async()=>{
    const window=await nativeWindow;
    if(window?.label!=='main'||revision!==nativeRevision)return;
    await invoke('set_ui_locale',{language});
  }).catch(()=>{/* UI selection remains usable when the native menu is unavailable. */});
});
// The studio owns the preference. Native output windows only read/follow it;
// language events never grant bot, tool, microphone or updater access.
void nativeWindow.then(async window=>{
  if(!window||window.label==='main')return;
  let receivedRevision=0;
  await listen<string>('cohost://ui-locale',event=>{if(isLocale(event.payload)){receivedRevision++;current.set(event.payload);}});
  const revision=receivedRevision;
  const language=await invoke<unknown>('get_ui_locale');
  if(isLocale(language)&&revision===receivedRevision)current.set(language);
}).catch(()=>{});
if(typeof window!=='undefined')window.addEventListener('storage',event=>{
  if(event.key===localeStorageKey){if(isLocale(event.newValue))current.set(event.newValue);else if(event.newValue===null)current.set(detectLocale(navigator.languages));}
});
