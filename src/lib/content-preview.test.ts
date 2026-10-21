import {describe,it,expect} from 'vitest';
import example from '../../public/content-packs/desk-garden.json';
import {contentActivityExamples,contentActionLabels,contentConditionLabels,contentMotionLabels} from './content-preview';
import {readFileSync,readdirSync} from 'node:fs';

describe('declaration-only package activity examples',()=>{
  it('joins the actual offered behavior to its object, condition, duration and declared motion fallback',()=>{
    const manifest=structuredClone(example),before=structuredClone(manifest);
    expect(contentActivityExamples(manifest)).toEqual([{id:'desk-garden:tend',objectName:'Seed toy',action:'play',condition:'low_engagement',durationMs:4000,weight:400,motion:'play',fallback:'idle'}]);
    expect(manifest).toEqual(before);
  });
  it('rejects missing, foreign and disconnected declarations instead of inventing an example',()=>{
    for(const change of [(m:any)=>m.objects[0].offers=[],(m:any)=>m.activities[0].objectId='desk-garden:missing',
      (m:any)=>m.activities[0].motionId='other:walk',(m:any)=>m.activities[0].action='shell',
      (m:any)=>m.activities[0].condition='always',(m:any)=>m.motions[0].fallback='script',
      (m:any)=>m.activities[0].durationMs=60_001,(m:any)=>m.activities[0].weight=0,
      (m:any)=>m.activities.push({...m.activities[0]})]){
      const manifest=structuredClone(example);change(manifest);expect(()=>contentActivityExamples(manifest)).toThrow();
    }
  });
  it('bounds the read model and returns no fabricated action for an empty package',()=>{
    const manifest=structuredClone(example);manifest.activities=[];expect(contentActivityExamples(manifest)).toEqual([]);
    expect(()=>contentActivityExamples({...example,activities:Array(33).fill(example.activities[0])})).toThrow();
  });
  it('has explicit localized labels for every supported action, condition and motion',()=>{
    const directory=new URL('./locales/',import.meta.url);
    const catalogues=readdirSync(directory).filter(file=>file.endsWith('.json')).map(file=>JSON.parse(readFileSync(new URL(file,directory),'utf8')));
    for(const language of ['en','fr','es']){
      const messages=Object.assign({},...catalogues.map(c=>c[language]));
      for(const label of [...Object.values(contentActionLabels),...Object.values(contentConditionLabels),...Object.values(contentMotionLabels)])expect(messages[label]?.trim(),`${language}: ${label}`).toBeTruthy();
    }
  });
});
