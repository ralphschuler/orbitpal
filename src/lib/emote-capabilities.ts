// SPDX-License-Identifier: MIT
import * as THREE from 'three';
import type { EmotePayload } from './presentation';
export const unavailableEmote = 'Diese Emote-Aktion ist für die aktuelle Figur nicht verfügbar.';
export type AvailableAction = { action:string; name:string; kind:'clip'|'gesture'|'motion'; duration?:number };
const aliases:Record<string,string[]>={wave:['wave'],celebrate:['cheer','jump','jump_full_short'],nod:['yes'],dance:['dance']};
export function avatarBinding(profile:any):unknown[]{return [profile?.id??null,profile?.avatar?.assetPath??'nova',(profile?.motions??[]).map((motion:any)=>motion.id??null)];}
const clamp=(value:unknown,fallback:number,min:number,max:number)=>Math.min(max,Math.max(min,typeof value==='number'&&Number.isFinite(value)?value:fallback));
export function emoteBinding(emote:any):unknown[]{return [emote.id??null,emote.action??null,typeof emote.sound==='string'?emote.sound:'',clamp(emote.soundVolume,.7,0,1),clamp(emote.duration,2.5,.2,15),clamp(emote.intensity,1,0,2),emote.allowChat===true];}
export function resolveAvailableAction(actions:readonly AvailableAction[],action:unknown):AvailableAction|undefined{
  if(typeof action!=='string'||!action.trim()||Array.from(action).length>128||/[\u0000-\u001f\u007f-\u009f]/.test(action))return;
  const exact=actions.filter(item=>item.action===action);if(exact.length)return exact.length===1?exact[0]:undefined;
  const insensitive=actions.filter(item=>item.action.toLowerCase()===action.toLowerCase());if(insensitive.length)return insensitive.length===1?insensitive[0]:undefined;
  for(const alias of aliases[action]??[]){const matches=actions.filter(item=>item.kind==='clip'&&item.action.toLowerCase()===alias);if(matches.length)return matches.length===1?matches[0]:undefined;}
}
export function availableActions(clips:readonly THREE.AnimationClip[],gestures:readonly string[]):AvailableAction[]{
  const actions:AvailableAction[]=clips.filter(clip=>clip.name.trim()&&Array.from(clip.name).length<=128&&!/[\u0000-\u001f\u007f-\u009f]/.test(clip.name)&&clips.filter(other=>other.name===clip.name).length===1&&Number.isFinite(clip.duration)&&clip.duration>0&&clip.tracks.length>0).map(clip=>({action:clip.name,name:clip.name,kind:clip.name.startsWith('vrma:')?'motion':'clip',duration:clip.duration}));
  for(const action of gestures){if(!clips.some(clip=>clip.name.toLowerCase()===action||(aliases[action]??[]).includes(clip.name.toLowerCase())))actions.push({action,name:action,kind:'gesture'});}
  return actions;
}
/** A queued host event keeps its original configuration; edits cannot substitute
 * a different action or sound at presentation time. Preview events have no ID. */
export function currentEmotePayload(profile:any,event:EmotePayload):boolean{
  if(typeof event.id!=='string'||!event.id)return false;
  const configured=(profile?.emotes??[]).filter((emote:any)=>emote.id===event.id);
  return configured.length===1&&event.botId===profile.id&&JSON.stringify(event.avatarBinding)===JSON.stringify(avatarBinding(profile))&&JSON.stringify(event.emoteBinding)===JSON.stringify(emoteBinding(configured[0]));
}
/** Check binding targets before a clip may authorize visual output or SFX. */
export function clipHasTarget(root:THREE.Object3D,clip:THREE.AnimationClip,targetAllows:(node:THREE.Object3D,property:string)=>boolean=()=>true):boolean{
  return clip.tracks.some(track=>{
    if(!track.times.length||!track.values.length||Array.from(track.times).some(value=>!Number.isFinite(value))||Array.from(track.values).some(value=>!Number.isFinite(Number(value))))return false;
    try{
      const parsed=THREE.PropertyBinding.parseTrackName(track.name);
      let node=THREE.PropertyBinding.findNode(root,parsed.nodeName) as THREE.Object3D|null;
      if(parsed.objectName==='bones')node=(node as THREE.SkinnedMesh)?.skeleton?.getBoneByName(parsed.objectIndex!)??null;
      else if(parsed.objectName)return false;
      if(!node||!targetAllows(node,parsed.propertyName)||!['position','quaternion','scale','morphTargetInfluences','weight'].includes(parsed.propertyName))return false;
      const value=(node as any)[parsed.propertyName];
      if(value===undefined||value===null)return false;
      if(parsed.propertyName==='weight'&&'binds' in node&&(!(node as any).binds.length))return false;
      if(parsed.propertyIndex===undefined)return typeof value==='number'||typeof value==='object';
      const key=parsed.propertyIndex;
      if(Array.isArray(value)||ArrayBuffer.isView(value)){
        const index=parsed.propertyName==='morphTargetInfluences'?(node as THREE.Mesh).morphTargetDictionary?.[key]??Number(key):Number(key);
        return Number.isSafeInteger(index)&&index>=0&&index<(value as ArrayLike<number>).length&&typeof (value as any)[index]==='number';
      }
      return ['x','y','z','w'].includes(key)&&typeof value[key]==='number';
    }catch{return false;}
  });
}
export type EmoteTarget={canPresent:(event:EmotePayload)=>boolean;present:(event:EmotePayload)=>boolean};
export function presentAdmittedEmote(target:EmoteTarget|undefined,event:EmotePayload,onPresented:()=>void):boolean{
  if(!target?.present(event))return false;
  onPresented();return true;
}
