import {parseContentRef,type ContentRef} from './content-reference';
import {anchorKinds,type AnchorKind} from './world-state';
export interface PackObjectTemplate {reference:ContentRef;name:string;packageName:string;kind:AnchorKind}
const object=(v:unknown):Record<string,unknown>=>{if(!v||typeof v!=='object'||Array.isArray(v))throw Error('pack_invalid_schema');return v as Record<string,unknown>;};
const text=(v:unknown,max:number)=>{if(typeof v!=='string'||!v.trim()||[...v].length>max||/\p{Cc}/u.test(v))throw Error('pack_invalid_schema');return v;};
/** A read-only tray projection. The host revalidates an exact active version before any action. */
export function activeObjectTemplates(value:unknown):PackObjectTemplate[]{
  if(!Array.isArray(value)||value.length>16)throw Error('pack_invalid_schema');
  const result:PackObjectTemplate[]=[],packages=new Set<string>();
  for(const item of value){const pack=object(item),manifest=object(pack.manifest);
    if(typeof pack.active!=='boolean'||!Number.isSafeInteger(pack.revision)||Number(pack.revision)<1||manifest.schemaVersion!==1||manifest.runtimeVersion!==1||!Array.isArray(manifest.objects)||manifest.objects.length>32)throw Error('pack_invalid_schema');
    const packageName=text(manifest.name,80),base=parseContentRef({packageId:manifest.id,packageVersion:manifest.version,templateId:String(manifest.id)+':placeholder'});
    if(packages.has(base.packageId))throw Error('pack_invalid_schema');packages.add(base.packageId);
    const objects=new Set<string>();
    for(const raw of manifest.objects){const item=object(raw),reference=parseContentRef({...base,templateId:item.id}),name=text(item.name,80);
      if(typeof item.kind!=='string'||!Object.hasOwn(anchorKinds,item.kind)||objects.has(reference.templateId))throw Error('pack_invalid_schema');objects.add(reference.templateId);
      if(pack.active)result.push({reference,name,packageName,kind:item.kind as AnchorKind});
    }
  }
  return result;
}
