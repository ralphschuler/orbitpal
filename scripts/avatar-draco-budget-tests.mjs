import test from 'node:test';import assert from 'node:assert/strict';import {installDracoOutputBudget} from './avatar-draco-budget.mjs';
test('aligned attributes and Uint32 indices share a bound before output copies, independent of accessor metadata',()=>{
 let reads=0;class Decoder {GetAttributeDataArrayForAllPoints(){reads++;return true;}GetTrianglesUInt32Array(){reads++;return true;}}
 installDracoOutputBudget({Decoder},32);const decoder=new Decoder(),geometry={num_points:()=>3};
 assert.equal(decoder.GetAttributeDataArrayForAllPoints(geometry,null,0,9,0),true); // 12 aligned bytes.
 assert.equal(decoder.GetAttributeDataArrayForAllPoints(geometry,null,0,12,0),true); // Total24.
 assert.throws(()=>decoder.GetTrianglesUInt32Array(geometry,12,0),/Ressourcenlimit/);assert.equal(reads,2);
 assert.throws(()=>decoder.GetAttributeDataArrayForAllPoints({num_points:()=>1},null,0,256*1024*1024+4,0),/Ressourcenlimit/);assert.equal(reads,2);
});

test('several geometries consume one real per-model budget; a new model is independent',()=>{
 let reads=0;class Decoder {DecodeArrayToMesh(){return true;}DecodeArrayToPointCloud(){return true;}GetAttributeDataArrayForAllPoints(){reads++;return true;}GetTrianglesUInt32Array(){reads++;return true;}}
 const inputs=new WeakMap(),model={used:0};const a=new Uint8Array(1),b=new Uint8Array(1),other=new Uint8Array(1);inputs.set(a.buffer,model);inputs.set(b.buffer,model);inputs.set(other.buffer,{used:0});
 installDracoOutputBudget({Decoder},32,inputs);const decoder=new Decoder();const first={num_points:()=>3},second={num_points:()=>3},third={num_points:()=>3};
 decoder.DecodeArrayToMesh(a,1,first);decoder.DecodeArrayToMesh(b,1,second);decoder.DecodeArrayToMesh(other,1,third);
 decoder.GetAttributeDataArrayForAllPoints(first,null,0,12,0);decoder.GetTrianglesUInt32Array(first,12,0); // Model24.
 assert.throws(()=>decoder.GetAttributeDataArrayForAllPoints(second,null,0,9,0),/Ressourcenlimit/); // Model36, including padding.
 assert.equal(decoder.GetAttributeDataArrayForAllPoints(third,null,0,12,0),true);assert.equal(reads,3);
});
