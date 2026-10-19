// A deliberately narrow transformation of the two reviewed wasm32 decoders.
// Change the single defined memory's maximum and add a nonexecuting notice.
// No executable code/data bytes change.
export const DECODER_MAX_PAGES = 4096; // 256 MiB per worker, one worker per codec.
const encode = (value) => { const out=[]; do { let byte=value%128; value=Math.floor(value/128); out.push(byte|(value?128:0)); } while(value); return Buffer.from(out); };
function leb(bytes, cursor) {
  let value=0, shift=0;
  for(let i=0;i<5;i++) { const byte=bytes[cursor.offset++]; if(byte===undefined)throw new Error('Truncated WASM LEB'); value+=(byte&127)*2**shift; if(!(byte&128)){if(value>0xffffffff)throw new Error('WASM LEB overflow');return value;}shift+=7; }
  throw new Error('Invalid WASM LEB');
}
export function capDecoderMemory(bytes) {
  bytes=Buffer.from(bytes);
  if(!bytes.subarray(0,8).equals(Buffer.from([0,97,115,109,1,0,0,0])))throw new Error('Expected wasm32 module');
  const module=new WebAssembly.Module(bytes);
  if(WebAssembly.Module.imports(module).some((item)=>item.kind!=='function'))throw new Error('Review imported decoder memory/table/global');
  let found=false; const output=[bytes.subarray(0,8)];const cursor={offset:8};
  while(cursor.offset<bytes.length) {
    const id=bytes[cursor.offset++];const length=leb(bytes,cursor);const start=cursor.offset,end=start+length;
    if(end>bytes.length)throw new Error('Truncated WASM section');
    if(id===5) {
      if(found)throw new Error('Duplicate WASM memory section');found=true;
      if(leb(bytes,cursor)!==1||leb(bytes,cursor)!==1)throw new Error('Review decoder memory count/limits flags');
      const initial=leb(bytes,cursor), maximum=leb(bytes,cursor);
      if(cursor.offset!==end||initial>DECODER_MAX_PAGES||maximum<DECODER_MAX_PAGES)throw new Error('Review decoder initial/maximum memory');
      const payload=Buffer.concat([encode(1),encode(1),encode(initial),encode(DECODER_MAX_PAGES)]);
      output.push(Buffer.from([5]),encode(payload.length),payload);
    } else output.push(Buffer.from([id]),encode(length),bytes.subarray(start,end));
    cursor.offset=end;
  }
  if(!found)throw new Error('Missing decoder memory');
  const name=Buffer.from('orbitpal.notice');
  const notice=Buffer.from('OrbitPal modification: wasm32 linear-memory maximum capped at 256 MiB. Executable code/data unchanged. See public/decoders/manifest.json and README.md for source/license/hashes.');
  const label=Buffer.concat([encode(name.length),name,notice]);output.push(Buffer.from([0]),encode(label.length),label);
  const patched=Buffer.concat(output);new WebAssembly.Module(patched);return patched;
}
