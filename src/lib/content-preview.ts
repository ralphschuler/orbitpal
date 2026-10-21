import {parseContentRef} from './content-reference';

export const contentActionLabels={rest:'Ruhen',snack:'Snackpause',play:'Spielen',socialize:'Gemeinsame Zeit'} as const;
export const contentConditionLabels={low_energy:'Bei wenig Energie',low_nourishment:'Bei wenig Nahrung oder Ladung',low_engagement:'Bei wenig Beschäftigung',low_social:'Bei wenig sozialer Nähe'} as const;
export const contentMotionLabels={idle:'Stillstehen',walk:'Laufbewegung',wave:'Winken',play:'Spielen',rest:'Ruhen'} as const;
export interface ContentActivityExample {
  id:string;objectName:string;action:keyof typeof contentActionLabels;condition:keyof typeof contentConditionLabels;
  durationMs:number;weight:number;motion:keyof typeof contentMotionLabels;fallback:keyof typeof contentMotionLabels;
}
const fail=()=>{throw Error('pack_invalid_schema');};
const record=(value:unknown):Record<string,unknown>=>value&&typeof value==='object'&&!Array.isArray(value)?value as Record<string,unknown>:fail();
const list=(value:unknown):unknown[]=>Array.isArray(value)&&value.length<=32?value:fail();
const key=<T extends Record<string,string>>(value:unknown,labels:T):keyof T=>typeof value==='string'&&Object.hasOwn(labels,value)?value:fail();
const integer=(value:unknown,min:number,max:number)=>Number.isSafeInteger(value)&&Number(value)>=min&&Number(value)<=max?Number(value):fail();

/** Declaration-only read model. Never executes a preview or infers native/asset capability. */
export function contentActivityExamples(value:unknown):ContentActivityExample[]{
  const manifest=record(value);
  const reference={packageId:manifest.id,packageVersion:manifest.version};
  const id=(value:unknown)=>parseContentRef({...reference,templateId:value}).templateId;
  const objects=new Map<string,{name:string;offers:string[]}>();
  for(const raw of list(manifest.objects)){
    const object=record(raw),objectId=id(object.id),name=object.name;
    if(objects.has(objectId)||typeof name!=='string'||!name.trim()||[...name].length>80||/\p{Cc}/u.test(name))fail();
    objects.set(objectId,{name:name as string,offers:list(object.offers).map(id)});
  }
  const motions=new Map<string,{motion:ContentActivityExample['motion'];fallback:ContentActivityExample['fallback']}>();
  for(const raw of list(manifest.motions)){
    const motion=record(raw),motionId=id(motion.id);if(motions.has(motionId))fail();
    motions.set(motionId,{motion:key(motion.capability,contentMotionLabels),fallback:key(motion.fallback,contentMotionLabels)});
  }
  const seen=new Set<string>();
  return list(manifest.activities).map(raw=>{
    const activity=record(raw),activityId=id(activity.id),object=objects.get(id(activity.objectId)),motion=motions.get(id(activity.motionId));
    if(seen.has(activityId)||!object||!motion||!object.offers.includes(activityId))return fail();
    seen.add(activityId);
    return {id:activityId,objectName:object.name,action:key(activity.action,contentActionLabels),condition:key(activity.condition,contentConditionLabels),
      durationMs:integer(activity.durationMs,500,60_000),weight:integer(activity.weight,1,1000),...motion};
  });
}
