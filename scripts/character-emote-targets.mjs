// SPDX-License-Identifier: MIT
// Checked against the native target resolver for every bundled model.
import { glbJson, bundledAvatarFiles } from './character-capabilities.mjs';
import { writeFileSync } from 'node:fs';
import { resolve, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
const project=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const presets={a:'aa',i:'ih',u:'ou',e:'ee',o:'oh',joy:'happy',sorrow:'sad',fun:'relaxed',lookup:'lookUp',lookdown:'lookDown',lookleft:'lookLeft',lookright:'lookRight',blink_l:'blinkLeft',blink_r:'blinkRight'};
export function target(document){
  const legacy=document.extensions?.VRM,vrm=legacy||document.extensions?.VRMC_vrm;
  const names=(document.animations||[]).map((animation,index)=>animation.name||`animation_${index}`);
  const clips=(document.animations||[]).flatMap((animation,index)=>(animation.channels||[]).some(channel=>document.nodes?.[channel.target?.node]&&['translation','rotation','scale','weights'].includes(channel.target?.path)&&['input','output'].every(key=>document.accessors?.[animation.samplers?.[channel.sampler]?.[key]]))?[index]:[]);
  if(!vrm)return {bones:[],expressions:[],lookAt:false,clips,names};
  const nodes=document.nodes||[];
  const pairs=legacy?(vrm.humanoid?.humanBones||[]).map(bone=>[bone.bone,bone.node]):Object.entries(vrm.humanoid?.humanBones||{}).map(([name,bone])=>[name,bone.node]);
  const bones=[...new Set(pairs.filter(([name,index])=>typeof name==='string'&&Number.isSafeInteger(index)&&index>=0&&index<nodes.length&&pairs.filter(([n,i])=>n===name||i===index).length===1).map(([name])=>name))].sort();
  const morph=(mesh,index)=>Number.isSafeInteger(mesh)&&Number.isSafeInteger(index)&&mesh>=0&&index>=0&&document.meshes?.[mesh]?.primitives?.some(primitive=>primitive.targets?.[index]);
  const expressions=[];
  if(legacy){for(const group of vrm.blendShapeMaster?.blendShapeGroups||[]){
    const works=group.binds?.some(bind=>Number.isFinite(bind.weight)&&bind.weight!==0&&morph(bind.mesh,bind.index))||group.materialValues?.some(bind=>document.materials?.some(material=>material.name===bind.materialName)&&typeof bind.propertyName==='string'&&bind.targetValue?.length);
    if(works){const name=group.presetName&&group.presetName!=='unknown'?(presets[group.presetName]||group.presetName):group.name;if(name)expressions.push(name);}
  }}else{for(const kind of ['preset','custom'])for(const [name,expression]of Object.entries(vrm.expressions?.[kind]||{})){
    const works=expression.morphTargetBinds?.some(bind=>Number.isFinite(bind.weight)&&bind.weight!==0&&morph(nodes[bind.node]?.mesh,bind.index))||['materialColorBinds','textureTransformBinds'].some(kind=>expression[kind]?.some(bind=>Number.isSafeInteger(bind.material)&&bind.material>=0&&document.materials?.[bind.material]));
    if(works)expressions.push(name);
  }}
  const unique=[...new Set(expressions)].sort();
  const expressionLook=legacy?vrm.firstPerson?.lookAtTypeName==='BlendShape':vrm.lookAt?.type==='expression';
  const hasLook=legacy?!!vrm.firstPerson:!!vrm.lookAt&&document.extensionsUsed?.includes('VRMC_vrm');
  const lookAt=!!hasLook&&bones.includes('head')&&(expressionLook?['lookUp','lookDown','lookLeft','lookRight'].some(name=>unique.includes(name)):['leftEye','rightEye'].some(name=>bones.includes(name)));
  return {bones,expressions:unique,lookAt,clips,names};
}
const catalog={nova:{bones:[],expressions:[],lookAt:false,clips:[],names:[]}};
for(const[id,file]of Object.entries(bundledAvatarFiles))catalog[id]=target(glbJson(join(project,'public/characters',file)).document);
writeFileSync(join(project,'public/character-emote-targets.json'),JSON.stringify(catalog,null,2)+'\n');
