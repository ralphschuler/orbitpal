// SPDX-License-Identifier: MIT
/** Static rigid source preparation only. It grants no render/native authority. */
import * as THREE from 'three';

export type RigidInterval = readonly [number,number];
export type RigidRange = Readonly<{start:number;end:number}>;
export interface RigidWork {
  operation(units:number):void; vertex():void; check():void; shouldYield():boolean;
}
export type RigidStatus = 'unsupported'|'invalid'|'preparing'|'ready';
export type RigidExtrema = Readonly<{status:'ready';world:Readonly<{min:readonly number[];max:readonly number[]}>;
  ndc:Readonly<{min:readonly number[];max:readonly number[]}>}>;
export type RigidQuery = RigidExtrema | Readonly<{status:Exclude<RigidStatus,'ready'>}>;
export type RigidLimits = Readonly<{maxVertices:number;maxOperations:number;maxBytes:number}>;
export const RIGID_MAX_SOURCE_VERTICES=200_000;
export const RIGID_MAX_WORK=2_000_000;
export const RIGID_MAX_BYTES=32*1024*1024;
const LEAF=32,BLOCK=1024,MAX_RANGES=1024;
const bits=new DataView(new ArrayBuffer(8));
export function nextUp(n:number):number {
  if(Number.isNaN(n)||n===Infinity)return n;
  if(n===0)return Number.MIN_VALUE;
  bits.setFloat64(0,n,false);let hi=bits.getUint32(0,false),lo=bits.getUint32(4,false);
  if(n>0){lo=(lo+1)>>>0;if(lo===0)hi=(hi+1)>>>0;}
  else{if(lo===0)hi=(hi-1)>>>0;lo=(lo-1)>>>0;}
  bits.setUint32(0,hi,false);bits.setUint32(4,lo,false);return bits.getFloat64(0,false);
}
export function nextDown(n:number):number {return -nextUp(-n);}
const whole:RigidInterval=Object.freeze([-Infinity,Infinity]);
export function outwardAdd(a:RigidInterval,b:RigidInterval):RigidInterval {
  const lo=a[0]+b[0],hi=a[1]+b[1];
  return Number.isNaN(lo)||Number.isNaN(hi)?whole:[nextDown(lo),nextUp(hi)];
}
export function outwardMultiply(a:RigidInterval,b:RigidInterval):RigidInterval {
  const values=[a[0]*b[0],a[0]*b[1],a[1]*b[0],a[1]*b[1]];
  return values.some(Number.isNaN)?whole:[nextDown(Math.min(...values)),nextUp(Math.max(...values))];
}
/** Total order for numerical extrema, preserving Math.min/-max signed zero. */
function floatOrder(a:number,b:number):number|null {
  if(Number.isNaN(a)||Number.isNaN(b))return null;
  if(a<b)return -1;if(a>b)return 1;
  if(a===0&&b===0&&!Object.is(a,b))return Object.is(a,-0)?-1:1;
  return 0;
}
function orderedInterval(value:RigidInterval):boolean {const order=floatOrder(value[0],value[1]);return order!==null&&order<=0;}
/** Bounds rounded IEEE values of this operation graph, not ideal-real values.
 * Monotonic rounded addition and sign-region multiplication attain extrema at
 * interval endpoints. NaN/inconclusive arithmetic cannot authorize pruning. */
export function roundedAdd(a:RigidInterval,b:RigidInterval):RigidInterval {
  if(!orderedInterval(a)||!orderedInterval(b))return whole;
  const lo=a[0]+b[0],hi=a[1]+b[1];return Number.isNaN(lo)||Number.isNaN(hi)?whole:[lo,hi];
}
export function roundedMultiply(a:RigidInterval,b:RigidInterval):RigidInterval {
  if(!orderedInterval(a)||!orderedInterval(b))return whole;
  const values=[a[0]*b[0],a[0]*b[1],a[1]*b[0],a[1]*b[1]];
  return values.some(Number.isNaN)?whole:[Math.min(...values),Math.max(...values)];
}
export function roundedContained(interval:RigidInterval,minimum:number,maximum:number):boolean {
  if(![interval[0],interval[1],minimum,maximum].every(Number.isFinite))return false;
  const lo=floatOrder(interval[0],minimum),hi=floatOrder(interval[1],maximum);
  return orderedInterval(interval)&&lo!==null&&hi!==null&&lo>=0&&hi<=0;
}
export function rigidAffine(matrix:THREE.Matrix4):boolean {
  const e=matrix.elements;
  return e.every(Number.isFinite)&&e[3]===0&&e[7]===0&&e[11]===0&&e[15]===1&&Math.abs(matrix.determinant())>1e-12;
}
type Attribute=THREE.BufferAttribute|THREE.InterleavedBufferAttribute;
type AttributeState={ref:Attribute;array:unknown;version:number;count:number;size:number;normalized:boolean;stride:number;offset:number};
function attributeState(ref:Attribute):AttributeState {
  const inter=ref instanceof THREE.InterleavedBufferAttribute;
  return {ref,array:inter?ref.data.array:ref.array,version:inter?ref.data.version:ref.version,count:ref.count,
    size:ref.itemSize,normalized:ref.normalized,stride:inter?ref.data.stride:0,offset:inter?ref.offset:0};
}
function attributeCurrent(state:AttributeState,ref:Attribute|null|undefined):boolean {
  if(!ref||state.ref!==ref)return false;
  const inter=ref instanceof THREE.InterleavedBufferAttribute;
  return state.array===(inter?ref.data.array:ref.array)&&state.version===(inter?ref.data.version:ref.version)
    &&state.count===ref.count&&state.size===ref.itemSize&&state.normalized===ref.normalized
    &&state.stride===(inter?ref.data.stride:0)&&state.offset===(inter?ref.offset:0);
}
function eligible(mesh:THREE.Mesh):boolean {
  const extra=mesh as THREE.Mesh&{isSkinnedMesh?:boolean;isInstancedMesh?:boolean;isBatchedMesh?:boolean};
  return !(mesh instanceof THREE.SkinnedMesh)&&!extra.isSkinnedMesh&&!extra.isInstancedMesh&&!extra.isBatchedMesh
    &&!(mesh.geometry instanceof THREE.InstancedBufferGeometry)&&!(mesh.geometry.morphAttributes.position?.length)
    &&mesh.getVertexPosition===THREE.Mesh.prototype.getVertexPosition;
}
type Source={geometry:THREE.BufferGeometry;position:AttributeState;index:AttributeState|null;
  rangeStart:number;rangeCount:number;groups:readonly number[];materials:readonly THREE.Material[]};
function capture(mesh:THREE.Mesh):Source|null {
  const g=mesh.geometry,p=g.attributes.position;
  if(!eligible(mesh)||!p||p.itemSize!==3||!Number.isSafeInteger(p.count)||p.count<=0||g.groups.length>MAX_RANGES
    ||Array.isArray(mesh.material)&&mesh.material.length>MAX_RANGES)return null;
  const groups=g.groups.flatMap(group=>[group.start,group.count,group.materialIndex??0]);
  return {geometry:g,position:attributeState(p),index:g.index?attributeState(g.index):null,
    rangeStart:g.drawRange.start,rangeCount:g.drawRange.count,groups,
    materials:Array.isArray(mesh.material)?mesh.material.slice():[mesh.material]};
}
function sourceCurrent(mesh:THREE.Mesh,s:Source):boolean {
  const g=mesh.geometry,materials=Array.isArray(mesh.material)?mesh.material:[mesh.material];
  return eligible(mesh)&&g===s.geometry&&attributeCurrent(s.position,g.attributes.position)
    &&(s.index?attributeCurrent(s.index,g.index):g.index===null)
    &&g.drawRange.start===s.rangeStart&&g.drawRange.count===s.rangeCount&&g.groups.length*3===s.groups.length
    &&g.groups.every((group,i)=>group.start===s.groups[i*3]&&group.count===s.groups[i*3+1]&&(group.materialIndex??0)===s.groups[i*3+2])
    &&materials.length===s.materials.length&&materials.every((material,i)=>material===s.materials[i]);
}
type Node={box:Float64Array;left:Node|null;right:Node|null;start:number;count:number};
type MeshData={source:Source;points:Map<number,number>;ranges:Map<string,Plan>};
type Plan={source:Source;start:number;end:number;cursor:number;ids:Uint32Array[];idCount:number;seen:Map<number,Uint32Array>;
  open:Node|null;level:Node[];next:Node[];mergeCursor:number;root:Node|null;
  seedAxis:number;seedNode:Node|null;seedCursor:number;seeds:number[]};
/** Same rounded singleton-product/addition graph as the public interval
 * helpers, without per-node tuple allocation. Inputs are captured before writes,
 * including when input and out alias. Scratch belongs to one query, not owner. */
export function roundedAffineBounds(input:ArrayLike<number>,matrix:THREE.Matrix4,out:Float64Array):void {
  if(out.length!==6)throw new Error('invalid_geometry');
  const x0=input[0],y0=input[1],z0=input[2],x1=input[3],y1=input[4],z1=input[5],e=matrix.elements;
  const xOrder=floatOrder(x0,x1),yOrder=floatOrder(y0,y1),zOrder=floatOrder(z0,z1);
  for(let row=0;row<3;row++){
    let lo=-Infinity,hi=Infinity;
    for(let axis=0;axis<3;axis++){
      const a=axis===0?x0:axis===1?y0:z0,b=axis===0?x1:axis===1?y1:z1;
      const order=axis===0?xOrder:axis===1?yOrder:zOrder,coefficient=e[row+axis*4];
      let termLo=-Infinity,termHi=Infinity;
      if(order!==null&&order<=0){
        const first=a*coefficient,last=b*coefficient;
        if(!Number.isNaN(first)&&!Number.isNaN(last)){termLo=Math.min(first,last);termHi=Math.max(first,last);}
      }
      if(axis===0){lo=termLo;hi=termHi;}
      else{
        const nextLo=lo+termLo,nextHi=hi+termHi;
        if(Number.isNaN(nextLo)||Number.isNaN(nextHi)){lo=-Infinity;hi=Infinity;}
        else{lo=nextLo;hi=nextHi;}
      }
    }
    const nextLo=lo+e[row+12],nextHi=hi+e[row+12];
    if(Number.isNaN(nextLo)||Number.isNaN(nextHi)){lo=-Infinity;hi=Infinity;}
    else{lo=nextLo;hi=nextHi;}
    // Leaf applyMatrix4 retains this final multiply; canonical affine w=1.
    out[row]=lo*1;out[row+3]=hi*1;
  }
}
function boundsContained(bounds:Float64Array,minimum:readonly number[],maximum:readonly number[]):boolean {
  for(let axis=0;axis<3;axis++){
    const lo=bounds[axis],hi=bounds[axis+3],min=minimum[axis],max=maximum[axis];
    if(!Number.isFinite(lo)||!Number.isFinite(hi)||!Number.isFinite(min)||!Number.isFinite(max))return false;
    const order=floatOrder(lo,hi),low=floatOrder(lo,min),high=floatOrder(hi,max);
    if(order===null||order>0||low===null||low<0||high===null||high>0)return false;
  }
  return true;
}
/** One owner per binding/asset generation. Invalid owners never restart. */
export class RigidSupportPreparation {
  private readonly limits:RigidLimits;
  private dead=false;private meshes=new Map<THREE.Mesh,MeshData>();private coordinates:Float64Array[]=[];
  private vertices=0;private operations=0;private bytes=0;private ranges=0;
  constructor(limits:RigidLimits={maxVertices:RIGID_MAX_SOURCE_VERTICES,maxOperations:RIGID_MAX_WORK,maxBytes:RIGID_MAX_BYTES}) {
    if(![limits.maxVertices,limits.maxOperations,limits.maxBytes].every(n=>Number.isSafeInteger(n)&&n>0)
      ||limits.maxVertices>RIGID_MAX_SOURCE_VERTICES||limits.maxOperations>RIGID_MAX_WORK||limits.maxBytes>RIGID_MAX_BYTES)throw new Error('measurement_budget');
    this.limits={...limits};
  }
  stats(){return {vertices:this.vertices,operations:this.operations,bytes:this.bytes,ranges:this.ranges};}
  hasPending():boolean {if(this.dead)return false;for(const data of this.meshes.values())for(const plan of data.ranges.values())if(plan.seedAxis<6)return true;return false;}
  invalidate():void {this.dead=true;this.meshes.clear();this.coordinates=[];}
  private allocate(bytes:number):void {if(this.bytes+bytes>this.limits.maxBytes)throw new Error('measurement_budget');this.bytes+=bytes;}
  private charge(units:number,work:RigidWork):void {
    if(this.operations+units>this.limits.maxOperations)throw new Error('measurement_budget');this.operations+=units;work.operation(units);
  }
  private data(mesh:THREE.Mesh):MeshData|RigidStatus {
    if(this.dead)return 'invalid';
    const old=this.meshes.get(mesh);
    if(old){if(!sourceCurrent(mesh,old.source)){this.invalidate();return 'invalid';}return old;}
    if(this.meshes.size>=256)throw new Error('measurement_budget');
    const source=capture(mesh);if(!source)return 'unsupported';
    const data={source,points:new Map<number,number>(),ranges:new Map<string,Plan>()};this.meshes.set(mesh,data);return data;
  }
  request(mesh:THREE.Mesh,range:RigidRange):RigidStatus {
    try{return this.requestRange(mesh,range);}catch(error){this.invalidate();throw error;}
  }
  private requestRange(mesh:THREE.Mesh,range:RigidRange):RigidStatus {
    const data=this.data(mesh);if(typeof data==='string')return data;
    const count=data.source.index?.count??data.source.position.count;
    if(![range.start,range.end].every(n=>Number.isSafeInteger(n)&&n>=0)||range.start>=range.end||range.end>count){this.invalidate();return 'invalid';}
    const key=range.start+':'+range.end,old=data.ranges.get(key);if(old)return old.root&&old.seedAxis===6?'ready':'preparing';
    if(this.ranges>=MAX_RANGES)throw new Error('measurement_budget');
    // Nonindexed ranges prove their input size without a vertex scan.
    if(!data.source.index&&range.end-range.start>this.limits.maxVertices)throw new Error('measurement_budget');
    if(range.end-range.start>this.limits.maxOperations)throw new Error('measurement_budget');
    this.ranges++;data.ranges.set(key,{source:data.source,start:range.start,end:range.end,cursor:range.start,
      ids:[],idCount:0,seen:new Map<number,Uint32Array>(),open:null,level:[],next:[],mergeCursor:0,root:null,
      seedAxis:0,seedNode:null,seedCursor:0,seeds:[]});return 'preparing';
  }
  private point(id:number,target:THREE.Vector3):THREE.Vector3 {
    const block=this.coordinates[Math.floor(id/BLOCK)],offset=(id%BLOCK)*3;return target.set(block[offset],block[offset+1],block[offset+2]);
  }
  private copyPoint(data:MeshData,index:number,work:RigidWork):number {
    const old=data.points.get(index);if(old!==undefined)return old;
    if(this.vertices>=this.limits.maxVertices)throw new Error('measurement_budget');
    work.vertex();this.charge(1,work);
    const p=data.source.position.ref,x=p.getX(index),y=p.getY(index),z=p.getZ(index);
    if(![x,y,z].every(Number.isFinite))throw new Error('invalid_geometry');
    const id=this.vertices++,blockIndex=Math.floor(id/BLOCK),offset=(id%BLOCK)*3;
    if(!this.coordinates[blockIndex]){this.allocate(BLOCK*3*8);this.coordinates[blockIndex]=new Float64Array(BLOCK*3);}
    const block=this.coordinates[blockIndex];block[offset]=x;block[offset+1]=y;block[offset+2]=z;data.points.set(index,id);return id;
  }
  private node(left:Node|null,right:Node|null,start:number):Node {
    this.allocate(6*8);const box=new Float64Array([Infinity,Infinity,Infinity,-Infinity,-Infinity,-Infinity]);
    if(left)for(let axis=0;axis<3;axis++){box[axis]=Math.min(left.box[axis],right?.box[axis]??Infinity);box[axis+3]=Math.max(left.box[axis+3],right?.box[axis+3]??-Infinity);}
    return {box,left,right,start,count:0};
  }
  prepare(mesh:THREE.Mesh,range:RigidRange,work:RigidWork):RigidStatus {
    try{return this.advance(mesh,range,work);}catch(error){this.invalidate();throw error;}
  }
  private advance(mesh:THREE.Mesh,range:RigidRange,work:RigidWork):RigidStatus {
    const status=this.requestRange(mesh,range);if(status!=='preparing')return status;
    const data=this.meshes.get(mesh)!,plan=data.ranges.get(range.start+':'+range.end)!,point=new THREE.Vector3();
    while(plan.cursor<plan.end){
      if(work.shouldYield())return 'preparing';work.check();this.charge(1,work);
      const raw=plan.cursor++,index=data.source.index?data.source.index.ref.getX(raw):raw;
      if(!Number.isSafeInteger(index)||index<0||index>=data.source.position.count){this.invalidate();return 'invalid';}
      const id=this.copyPoint(data,index,work),membershipBlock=Math.floor(id/BLOCK),word=Math.floor((id%BLOCK)/32),mask=1<<(id%32);
      let membership=plan.seen.get(membershipBlock);
      if(membership&&(membership[word]&mask)!==0)continue;
      if(!membership){this.allocate(BLOCK/8);membership=new Uint32Array(BLOCK/32);plan.seen.set(membershipBlock,membership);}
      membership[word]|=mask;this.charge(1,work);
      const blockIndex=Math.floor(plan.idCount/BLOCK);if(!plan.ids[blockIndex]){this.allocate(BLOCK*4);plan.ids[blockIndex]=new Uint32Array(BLOCK);}
      plan.ids[blockIndex][plan.idCount%BLOCK]=id;
      plan.open??=this.node(null,null,plan.idCount);plan.idCount++;plan.open.count++;this.point(id,point);
      for(let axis=0;axis<3;axis++){const n=point.getComponent(axis);plan.open.box[axis]=Math.min(plan.open.box[axis],n);plan.open.box[axis+3]=Math.max(plan.open.box[axis+3],n);}
      if(plan.open.count===LEAF){plan.level.push(plan.open);plan.open=null;}
    }
    if(plan.open){plan.level.push(plan.open);plan.open=null;}
    while(plan.level.length>1){
      while(plan.mergeCursor<plan.level.length){
        if(work.shouldYield())return 'preparing';work.check();this.charge(6,work);
        const left=plan.level[plan.mergeCursor++],right=plan.level[plan.mergeCursor++];
        plan.next.push(right?this.node(left,right,0):left);
      }
      plan.level=plan.next;plan.next=[];plan.mergeCursor=0;
    }
    if(!sourceCurrent(mesh,data.source)){this.invalidate();return 'invalid';}
    plan.root=plan.level[0]??null;if(!plan.root){this.invalidate();return 'invalid';}
    // Real source IDs only: root-box extrema guide a bounded tree descent but
    // never become a query answer. This shares the preparation ledger/yield and
    // resumes the exact unfinished search rather than restarting on a later tick.
    while(plan.seedAxis<6){
      plan.seedNode??=plan.root;
      const bound=plan.root.box[plan.seedAxis],axis=plan.seedAxis%3;
      while(plan.seedNode.left){
        if(work.shouldYield())return 'preparing';work.check();this.charge(2,work);
        const left:Node=plan.seedNode.left,right:Node|null=plan.seedNode.right;
        if(Object.is(left.box[plan.seedAxis],bound))plan.seedNode=left;
        else if(right&&Object.is(right.box[plan.seedAxis],bound))plan.seedNode=right;
        else{this.invalidate();return 'invalid';}
      }
      let found=false;
      while(plan.seedCursor<plan.seedNode.count){
        if(work.shouldYield())return 'preparing';work.check();this.charge(2,work);
        const at=plan.seedNode.start+plan.seedCursor++,id=plan.ids[Math.floor(at/BLOCK)][at%BLOCK];
        if(!Object.is(this.point(id,point).getComponent(axis),bound))continue;
        this.charge(plan.seeds.length+1,work);
        if(!plan.seeds.includes(id))plan.seeds.push(id);
        found=true;break;
      }
      if(!found){this.invalidate();return 'invalid';}
      plan.seedAxis++;plan.seedNode=null;plan.seedCursor=0;
    }
    if(!sourceCurrent(mesh,data.source)){this.invalidate();return 'invalid';}
    return 'ready';
  }
  /** Conservative current CPU-geometry enclosure of this exact submitted range.
   * Unlike query(), these endpoints need not be attained by a source vertex.
   * The actual current world stage is validated before projection can hide it. */
  enclose(mesh:THREE.Mesh,range:RigidRange,pv:THREE.Matrix4,work:RigidWork):RigidQuery {
    try{
      const status=this.requestRange(mesh,range);if(status!=='ready')return {status};
      if(!rigidAffine(mesh.matrixWorld)||!rigidAffine(pv))return {status:'unsupported'};
      const data=this.meshes.get(mesh)!,plan=data.ranges.get(range.start+':'+range.end)!;
      const worldMatrix=mesh.matrixWorld.clone(),projection=pv.clone(),world=new Float64Array(6),ndc=new Float64Array(6);
      work.check();work.operation(42);roundedAffineBounds(plan.root!.box,worldMatrix,world);
      if(!world.every(value=>Number.isFinite(value)&&Math.abs(value)<=1_000_000)){this.invalidate();return {status:'invalid'};}
      roundedAffineBounds(world,projection,ndc);work.check();
      if(!ndc.every(Number.isFinite)||!sourceCurrent(mesh,data.source)
        ||!mesh.matrixWorld.elements.every((n,i)=>Object.is(n,worldMatrix.elements[i]))
        ||!pv.elements.every((n,i)=>Object.is(n,projection.elements[i]))){this.invalidate();return {status:'invalid'};}
      return {status:'ready',world:{min:Array.from(world.subarray(0,3)),max:Array.from(world.subarray(3,6))},
        ndc:{min:Array.from(ndc.subarray(0,3)),max:Array.from(ndc.subarray(3,6))}};
    }catch(error){this.invalidate();throw error;}
  }
  query(mesh:THREE.Mesh,range:RigidRange,pv:THREE.Matrix4,work:RigidWork):RigidQuery {
    try{return this.queryRange(mesh,range,pv,work);}catch(error){this.invalidate();throw error;}
  }
  private queryRange(mesh:THREE.Mesh,range:RigidRange,pv:THREE.Matrix4,work:RigidWork):RigidQuery {
    const status=this.requestRange(mesh,range);if(status!=='ready')return {status};
    if(!rigidAffine(mesh.matrixWorld)||!rigidAffine(pv))return {status:'unsupported'};
    const data=this.meshes.get(mesh)!,plan=data.ranges.get(range.start+':'+range.end)!,worldMatrix=mesh.matrixWorld.clone(),projection=pv.clone();
    const worldMin=[Infinity,Infinity,Infinity],worldMax=[-Infinity,-Infinity,-Infinity],ndcMin=[Infinity,Infinity,Infinity],ndcMax=[-Infinity,-Infinity,-Infinity];
    const stack=[plan.root!],point=new THREE.Vector3(),projected=new THREE.Vector3(),seeded=new Set<number>();
    // Fixed96-byte query-local temporary scratch; no persistent cache growth or
    // shared state across a reentrant query. Fully overwritten before each use.
    const worldBounds=new Float64Array(6),ndcBounds=new Float64Array(6);
    const evaluate=(id:number):boolean=>{
      work.vertex();work.operation(2);
      this.point(id,point).applyMatrix4(worldMatrix);projected.copy(point).applyMatrix4(projection);
      if(![point.x,point.y,point.z,projected.x,projected.y,projected.z].every(Number.isFinite)){this.invalidate();return false;}
      for(let axis=0;axis<3;axis++){const w=point.getComponent(axis),p=projected.getComponent(axis);
        worldMin[axis]=Math.min(worldMin[axis],w);worldMax[axis]=Math.max(worldMax[axis],w);ndcMin[axis]=Math.min(ndcMin[axis],p);ndcMax[axis]=Math.max(ndcMax[axis],p);}
      return true;
    };
    // Seed with fresh current transforms; never reuse old transformed values.
    // At most six IDs are stored, deduplicated, and skipped in later leaf visits
    // so a valid200k-point exhaustive query is not charged200k+6 evaluations.
    for(const id of plan.seeds){work.check();work.operation(1);if(seeded.has(id))continue;seeded.add(id);if(!evaluate(id))return {status:'invalid'};}
    while(stack.length){
      work.check();work.operation(42);const node=stack.pop()!;
      roundedAffineBounds(node.box,worldMatrix,worldBounds);roundedAffineBounds(worldBounds,projection,ndcBounds);
      if(boundsContained(worldBounds,worldMin,worldMax)&&boundsContained(ndcBounds,ndcMin,ndcMax))continue;
      if(node.left){if(node.right)stack.push(node.right);stack.push(node.left);continue;}
      for(let i=0;i<node.count;i++){
        work.check();work.operation(1);
        const at=node.start+i,id=plan.ids[Math.floor(at/BLOCK)][at%BLOCK];
        if(!seeded.has(id)&&!evaluate(id))return {status:'invalid'};
      }
    }
    if(!sourceCurrent(mesh,data.source)||!mesh.matrixWorld.elements.every((n,i)=>n===worldMatrix.elements[i])||!pv.elements.every((n,i)=>n===projection.elements[i])){this.invalidate();return {status:'invalid'};}
    return {status:'ready',world:{min:worldMin,max:worldMax},ndc:{min:ndcMin,max:ndcMax}};
  }
}
