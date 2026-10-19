#!/usr/bin/env python3
"""Generate original, tiny offline geometry/texture fixtures. No downloads.
Requires Blender 5.2 with its bundled Draco bridge and meshoptimizer library.
Encoder libraries are development tools only; neither is shipped with OrbitPal.
Pass --blender and --meshoptimizer when using different installation locations.
"""
import argparse, ctypes as c, json, pathlib, struct, subprocess, tempfile, zlib
p=argparse.ArgumentParser();p.add_argument('--blender',required=True);p.add_argument('--meshoptimizer',required=True);args=p.parse_args()
root=pathlib.Path(__file__).resolve().parent.parent;dest=root/'tests/fixtures/compressed-avatar';dest.mkdir(parents=True,exist_ok=True)
def glb(doc,data):
 doc=dict(doc);doc['buffers']=[{'byteLength':len(data)}];j=json.dumps(doc,separators=(',',':')).encode();j+=b' '*(-len(j)%4);data+=b'\0'*(-len(data)%4)
 return b'glTF'+struct.pack('<II',2,28+len(j)+len(data))+struct.pack('<II',len(j),0x4e4f534a)+j+struct.pack('<II',len(data),0x004e4942)+data
positions=struct.pack('<9f',-1,0,0,1,0,0,0,2,0);uv=struct.pack('<6f',0,0,1,0,.5,1)
base={'asset':{'version':'2.0','generator':'OrbitPal original offline fixture'},'scene':0,'scenes':[{'nodes':[0]}],'nodes':[{'mesh':0}], 'meshes':[{'primitives':[{'attributes':{'POSITION':0},'material':0}]}], 'accessors':[{'componentType':5126,'count':3,'type':'VEC3','min':[-1,0,0],'max':[1,2,0]}], 'materials':[{'extensions':{'KHR_materials_unlit':{}},'pbrMetallicRoughness':{'baseColorFactor':[1,0,0,1]}}], 'extensionsUsed':['KHR_materials_unlit']}
def clone(): return json.loads(json.dumps(base))
# Original triangle exported with Blender's actual Draco encoder.
code="""
import bpy,sys
bpy.ops.object.select_all(action='SELECT');bpy.ops.object.delete(use_global=False)
m=bpy.data.meshes.new('OriginalTriangle');m.from_pydata([(-1,0,0),(1,0,0),(0,0,2)],[],[(0,1,2)]);m.update()
o=bpy.data.objects.new('OriginalTriangle',m);bpy.context.collection.objects.link(o);bpy.context.view_layer.objects.active=o;o.select_set(True)
bpy.ops.export_scene.gltf(filepath=sys.argv[-1],export_format='GLB',export_draco_mesh_compression_enable=True,export_draco_mesh_compression_level=7,export_normals=False,export_texcoords=False,export_materials='NONE')
"""
with tempfile.TemporaryDirectory() as tmp:
 script=pathlib.Path(tmp)/'fixture.py';script.write_text(code);target=pathlib.Path(tmp)/'draco.glb'
 subprocess.run([args.blender,'--background','--factory-startup','--python',str(script),'--',str(target)],check=True)
 b=target.read_bytes();jslen=struct.unpack_from('<I',b,12)[0];doc=json.loads(b[20:20+jslen]);data=b[28+jslen:]
 # Use the same visible red material as the other fixtures.
 doc['materials']=base['materials'];doc['meshes'][0]['primitives'][0]['material']=0;doc['extensionsUsed'].append('KHR_materials_unlit')
 (dest/'draco.glb').write_bytes(glb(doc,data))
# Native encoder, its output is decoded by Three's bundled meshoptimizer WASM.
lib=c.CDLL(args.meshoptimizer);lib.meshopt_encodeVertexBufferBound.argtypes=[c.c_size_t,c.c_size_t];lib.meshopt_encodeVertexBufferBound.restype=c.c_size_t
lib.meshopt_encodeVertexBuffer.argtypes=[c.c_void_p,c.c_size_t,c.c_void_p,c.c_size_t,c.c_size_t];lib.meshopt_encodeVertexBuffer.restype=c.c_size_t
bound=lib.meshopt_encodeVertexBufferBound(3,12);out=c.create_string_buffer(bound);src=c.create_string_buffer(positions);size=lib.meshopt_encodeVertexBuffer(out,bound,src,3,12);assert size
compressed=out.raw[:size]
for ext in ['EXT_meshopt_compression','KHR_meshopt_compression']:
 doc=clone();doc['extensionsUsed'].append(ext);doc['extensionsRequired']=[ext];doc['accessors'][0]['bufferView']=0
 # Keep valid base data too, to also exercise optional-fallback declarations.
 doc['bufferViews']=[{'buffer':0,'byteLength':len(positions),'extensions':{ext:{'buffer':0,'byteOffset':len(positions),'byteLength':size,'byteStride':12,'count':3,'mode':'ATTRIBUTES','filter':'NONE'}}}]
 (dest/('meshopt-'+ext[:3].lower()+'.glb')).write_bytes(glb(doc,positions+compressed))
 doc.pop('extensionsRequired');(dest/('meshopt-'+ext[:3].lower()+'-optional.glb')).write_bytes(glb(doc,positions+compressed))
# KTX2 UASTC LDR solid red block: UASTC mode 8 uses 0x17/5-bit code,
# followed by four 8-bit RGBA components. Zeroed unused ETC hints are valid.
# https://github.com/BinomialLLC/basis_universal/wiki/UASTC-Texture-Specification
block=(0x17 | (255<<5) | (255<<29)).to_bytes(16,'little')
# KTX2 basic DFD: Khronos, v2, UASTC/BT709/sRGB, 4x4 blocks, RGBA sample.
dfd=struct.pack('<IHHHH4B4B8B',44,0,0,2,40,166,1,2,0,3,3,0,0,16,0,0,0,0,0,0,0)+struct.pack('<HBB4BII',0,127,3,0,0,0,0,0,0xffffffff)
ktx=b'\xabKTX 20\xbb\r\n\x1a\n'+struct.pack('<13I2Q',0,1,4,4,0,0,1,1,0,104,len(dfd),0,0,0,0)+struct.pack('<3Q',160,16,16)+dfd+b'\0'*12+block
assert len(ktx)==176
(dest/'red-uastc.ktx2').write_bytes(ktx)
# Own one-pixel PNG provides a genuine optional core fallback.
def chunk(t,b):return struct.pack('>I',len(b))+t+b+struct.pack('>I',zlib.crc32(t+b)&0xffffffff)
png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>2I5B',1,1,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(b'\0\xff\0\0\xff'))+chunk(b'IEND',b'')
for optional in [False,True]:
 doc=clone();doc['accessors'][0]['bufferView']=0;doc['accessors'].append({'bufferView':1,'componentType':5126,'count':3,'type':'VEC2'});doc['meshes'][0]['primitives'][0]['attributes']['TEXCOORD_0']=1
 doc['materials'][0]['pbrMetallicRoughness']={'baseColorTexture':{'index':0}};doc['extensionsUsed'].append('KHR_texture_basisu');doc['textures']=[{'sampler':0,'extensions':{'KHR_texture_basisu':{'source':0}}}];doc['samplers']=[{'minFilter':9728,'magFilter':9728}]
 doc['bufferViews']=[{'buffer':0,'byteLength':36},{'buffer':0,'byteOffset':36,'byteLength':24},{'buffer':0,'byteOffset':60,'byteLength':len(ktx)}];doc['images']=[{'bufferView':2,'mimeType':'image/ktx2'}];data=positions+uv+ktx
 if optional:
  doc['textures'][0]['source']=1;doc['images'].append({'bufferView':3,'mimeType':'image/png'});doc['bufferViews'].append({'buffer':0,'byteOffset':len(data),'byteLength':len(png)});data+=png
 else:doc['extensionsRequired']=['KHR_texture_basisu']
 (dest/('basisu-optional.glb' if optional else 'basisu.glb')).write_bytes(glb(doc,data))
# A source glTF with separate, safely local BIN/KTX2 exercises native repacking.
doc=json.loads((dest/'basisu.glb').read_bytes()[20:20+struct.unpack_from('<I',(dest/'basisu.glb').read_bytes(),12)[0]])
doc['buffers']=[{'uri':'triangle.bin','byteLength':60}];doc['bufferViews']=doc['bufferViews'][:2];doc['images']=[{'uri':'red-uastc.ktx2','mimeType':'image/ktx2'}]
(dest/'triangle.bin').write_bytes(positions+uv);(dest/'basisu.gltf').write_text(json.dumps(doc,indent=2)+'\n')
print('Generated original compressed fixtures:',sorted(x.name for x in dest.iterdir()))

# Required Meshopt with an intentionally URI-less virtual fallback buffer.
for kind in ['ext','khr']:
 raw=(dest/f'meshopt-{kind}.glb').read_bytes();n=struct.unpack_from('<I',raw,12)[0];doc=json.loads(raw[20:20+n]);data=raw[28+n:];ext=doc['extensionsRequired'][0]
 doc['buffers']=[{'byteLength':len(data)},{'byteLength':36,'extensions':{ext:{'fallback':True}}}];doc['bufferViews'][0]['buffer']=1
 j=json.dumps(doc,separators=(',',':')).encode();j+=b' '*(-len(j)%4)
 (dest/f'meshopt-{kind}-virtual.glb').write_bytes(b'glTF'+struct.pack('<II',2,28+len(j)+len(data))+struct.pack('<II',len(j),0x4e4f534a)+j+struct.pack('<II',len(data),0x004e4942)+data)
# Local JSON-glTF sources for the exact same encoded streams.
for file in list(dest.glob('*.glb')):
 if file.stem == 'basisu-data-uri' or file.stem.startswith('invalid-'): continue
 raw=file.read_bytes();n=struct.unpack_from('<I',raw,12)[0];doc=json.loads(raw[20:20+n]);data=raw[28+n:];doc['buffers'][0]['uri']=file.stem+'.bin'
 (dest/(file.stem+'.bin')).write_bytes(data[:doc['buffers'][0]['byteLength']]);(dest/(file.stem+'.gltf')).write_text(json.dumps(doc,indent=2)+'\n')
# Keep a truly external KTX2 case, alongside the embedded-image source above.
raw=(dest/'basisu.glb').read_bytes();n=struct.unpack_from('<I',raw,12)[0];doc=json.loads(raw[20:20+n]);doc['buffers']=[{'uri':'triangle.bin','byteLength':60}];doc['bufferViews']=doc['bufferViews'][:2];doc['images']=[{'uri':'red-uastc.ktx2','mimeType':'image/ktx2'}]
(dest/'basisu-external.gltf').write_text(json.dumps(doc,indent=2)+'\n')

# Embedded base64 buffers are valid self-contained GLB resources too.
import base64
raw=(dest/'basisu.glb').read_bytes();n=struct.unpack_from('<I',raw,12)[0];doc=json.loads(raw[20:20+n]);data=raw[28+n:][:doc['buffers'][0]['byteLength']]
doc['buffers'][0]['uri']='data:application/octet-stream;base64,'+base64.b64encode(data).decode()
j=json.dumps(doc,separators=(',',':')).encode();j+=b' '*(-len(j)%4)
(dest/'basisu-data-uri.glb').write_bytes(b'glTF'+struct.pack('<II',2,20+len(j))+struct.pack('<II',len(j),0x4e4f534a)+j)

# Small inconsistent stream: encoded vertices remain three, metadata claims two.
raw=(dest/'draco.glb').read_bytes();n=struct.unpack_from('<I',raw,12)[0];doc=json.loads(raw[20:20+n]);data=raw[28+n:]
for accessor in doc['meshes'][0]['primitives'][0]['attributes'].values():doc['accessors'][accessor]['count']=2
(dest/'invalid-draco-count.glb').write_bytes(glb(doc,data))
