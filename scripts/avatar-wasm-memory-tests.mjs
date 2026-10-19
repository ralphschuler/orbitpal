import test from 'node:test';import assert from 'node:assert/strict';import fs from 'node:fs';import path from 'node:path';import {fileURLToPath} from 'node:url';
import {capDecoderMemory,DECODER_MAX_PAGES} from './avatar-wasm-memory.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');const manifest=JSON.parse(fs.readFileSync(path.join(root,'public/decoders/manifest.json')));
for(const asset of manifest.files.filter((asset)=>asset.path?.endsWith('.wasm')))test(asset.source+' has an enforced linear-memory ceiling without a boundary-sized allocation',async()=>{
 const original=fs.readFileSync(path.join(root,'node_modules/three',asset.source));const bundled=fs.readFileSync(path.join(root,asset.path));assert.deepEqual(capDecoderMemory(original),bundled);
 const module=new WebAssembly.Module(bundled);assert.equal(WebAssembly.Module.customSections(module,'orbitpal.notice').length,1);const imports={};for(const item of WebAssembly.Module.imports(module)){assert.equal(item.kind,'function');(imports[item.module]??={})[item.name]=()=>0;}
 const instance=new WebAssembly.Instance(module,imports);const name=WebAssembly.Module.exports(module).find((item)=>item.kind==='memory').name;const memory=instance.exports[name];
 const initial=memory.buffer.byteLength;assert.ok(initial<=32*1024*1024);const pages=initial/65536;
 assert.throws(()=>memory.grow(DECODER_MAX_PAGES-pages+1),RangeError);assert.equal(memory.buffer.byteLength,initial,'Rejected growth must allocate nothing');
});
