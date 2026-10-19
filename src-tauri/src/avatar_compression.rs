//! Bounds for the compressed resources handled by the bundled Three decoders.
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom};
const MAX_DECODED: u64 = 256 * 1024 * 1024;
const MAX_IMAGE: u64 = 64 * 1024 * 1024;
pub(super) const MESHOPT: [&str; 2] = ["EXT_meshopt_compression", "KHR_meshopt_compression"];
fn error() -> String { "Die komprimierten Modelldaten sind ungültig oder überschreiten das Ressourcenlimit.".into() }
fn number(value: &Value) -> Result<u64, String> { value.as_u64().ok_or_else(error) }
fn offset(value: &Value) -> Result<u64, String> { if value.is_null() { Ok(0) } else { number(value) } }
fn array<'a>(doc: &'a Value, key: &str) -> &'a [Value] { doc[key].as_array().map(Vec::as_slice).unwrap_or_default() }
fn range(size: u64, start: u64, length: u64) -> Result<(), String> { if length == 0 || start.checked_add(length).is_none_or(|end| end > size) { Err(error()) } else { Ok(()) } }

/// A required Meshopt stream may have a URI-less, deliberately empty fallback
/// buffer. It is never read by the configured decoder. Every referencing view
/// must carry that required compression; ordinary resources cannot use it.
pub(crate) fn virtual_buffer(doc: &Value, index: usize) -> bool {
    let buffers = array(doc, "buffers");
    let Some(buffer) = buffers.get(index) else { return false; };
    if buffer.get("uri").is_some() { return false; }
    MESHOPT.iter().any(|name| buffer["extensions"][name]["fallback"] == true
        && array(doc,"extensionsRequired").iter().any(|value| value.as_str() == Some(name))
        && array(doc,"bufferViews").iter().filter(|view| view["buffer"].as_u64() == Some(index as u64)).all(|view| view["extensions"].get(name).is_some()))
}

pub(super) fn metadata(doc: &Value) -> Result<(), String> {
    let buffers = array(doc,"buffers"); let views = array(doc,"bufferViews");
    let mut budget = 0u64;
    for view in views {
        for name in MESHOPT {
            let Some(ext) = view["extensions"].get(name) else { continue; };
            if MESHOPT.iter().filter(|name| view["extensions"].get(*name).is_some()).count() != 1 { return Err(error()); }
            let source_index = number(&ext["buffer"])? as usize;
            let source = buffers.get(source_index).ok_or_else(error)?;
            if virtual_buffer(doc,source_index) { return Err(error()); }
            range(number(&source["byteLength"])?,offset(&ext["byteOffset"])?,number(&ext["byteLength"])?)?;
            let count = number(&ext["count"])?; let stride = number(&ext["byteStride"])?;
            let decoded = count.checked_mul(stride).filter(|size| *size > 0 && *size <= MAX_DECODED).ok_or_else(error)?;
            if number(&view["byteLength"])? != decoded { return Err(error()); }
            let mode = ext["mode"].as_str().ok_or_else(error)?;
            let filter = ext.get("filter").map(|value| value.as_str().ok_or_else(error)).transpose()?.unwrap_or("NONE");
            if !["ATTRIBUTES","TRIANGLES","INDICES"].contains(&mode) || !["NONE","OCTAHEDRAL","QUATERNION","EXPONENTIAL","COLOR"].contains(&filter) { return Err(error()); }
            if mode == "ATTRIBUTES" {
                if stride > 256 || stride % 4 != 0 { return Err(error()); }
                if (filter == "OCTAHEDRAL" && ![4,8].contains(&stride)) || (filter == "QUATERNION" && stride != 8) || (filter == "COLOR" && stride != 4) { return Err(error()); }
            } else if ![2,4].contains(&stride) || filter != "NONE" || (mode == "TRIANGLES" && count % 3 != 0) { return Err(error()); }
            budget = budget.checked_add(decoded).filter(|size| *size <= MAX_DECODED).ok_or_else(error)?;
        }
    }
    for mesh in array(doc,"meshes") { for primitive in array(mesh,"primitives") {
        let Some(ext) = primitive["extensions"].get("KHR_draco_mesh_compression") else { continue; };
        let view = views.get(number(&ext["bufferView"])? as usize).ok_or_else(error)?;
        if MESHOPT.iter().any(|name| view["extensions"].get(*name).is_some()) { return Err(error()); }
        let attributes = ext["attributes"].as_object().filter(|attributes| !attributes.is_empty() && attributes.len() <= 32).ok_or_else(error)?;
        for (name,id) in attributes {
            number(id)?;
            let accessor = array(doc,"accessors").get(number(&primitive["attributes"][name])? as usize).ok_or_else(error)?;
            let components = match accessor["type"].as_str() { Some("SCALAR")=>1,Some("VEC2")=>2,Some("VEC3")=>3,Some("VEC4")=>4,_=>return Err(error()) };
            let bytes = match accessor["componentType"].as_u64() { Some(5120|5121)=>1,Some(5122|5123)=>2,Some(5125|5126)=>4,_=>return Err(error()) };
            let decoded = number(&accessor["count"])?.checked_mul(components*bytes).filter(|size| *size > 0).ok_or_else(error)?;
            budget = budget.checked_add(decoded).filter(|size| *size <= MAX_DECODED).ok_or_else(error)?;
        }
        if let Some(index) = primitive.get("indices") {
            let accessor = array(doc,"accessors").get(number(index)? as usize).ok_or_else(error)?;
            if accessor["type"] != "SCALAR" || !matches!(accessor["componentType"].as_u64(),Some(5121|5123|5125)) { return Err(error()); }
            // DRACOLoader materializes indices as Uint32, including a source
            // accessor declared as Uint8/Uint16. Count the actual output width.
            let decoded = number(&accessor["count"])?.checked_mul(4).filter(|size|*size>0).ok_or_else(error)?;
            budget = budget.checked_add(decoded).filter(|size|*size<=MAX_DECODED).ok_or_else(error)?;
        }
    }}
    Ok(())
}

fn le32(bytes: &[u8], offset: usize) -> Result<u32,String> { bytes.get(offset..offset+4).map(|bytes|u32::from_le_bytes(bytes.try_into().unwrap())).ok_or_else(error) }
fn le64(bytes: &[u8], offset: usize) -> Result<u64,String> { bytes.get(offset..offset+8).map(|bytes|u64::from_le_bytes(bytes.try_into().unwrap())).ok_or_else(error) }
/// Validates 2D BasisLZ ETC1S / UASTC KTX2 containers, before the browser/WASM
/// allocates decoded pixels. Actual transcoding remains in the renderer.
pub(crate) fn validate_ktx2(bytes: &[u8]) -> Result<(),String> { ktx2_budget(bytes).map(|_|()) }
fn ktx2_budget(bytes: &[u8]) -> Result<u64,String> {
    if bytes.len() < 104 || bytes.len() as u64 > MAX_IMAGE || !bytes.starts_with(b"\xabKTX 20\xbb\r\n\x1a\n") || le32(bytes,12)? != 0 || le32(bytes,16)? != 1 { return Err(error()); }
    let width = u64::from(le32(bytes,20)?); let height = u64::from(le32(bytes,24)?);
    if width == 0 || height == 0 || width % 4 != 0 || height % 4 != 0 || width > 16384 || height > 16384 || width*height > 64*1024*1024 { return Err(error()); }
    if le32(bytes,28)? != 0 || le32(bytes,32)? != 0 || le32(bytes,36)? != 1 { return Err(error()); }
    let levels = le32(bytes,40)? as usize; let scheme = le32(bytes,44)?;
    let max_levels = 64 - width.max(height).leading_zeros() as usize;
    if levels == 0 || levels > max_levels || 80 + levels*24 > bytes.len() { return Err(error()); }
    let dfd = le32(bytes,48)? as usize; let dfd_len = le32(bytes,52)? as usize;
    range(bytes.len() as u64,dfd as u64,dfd_len as u64)?;
    if dfd < 80+levels*24 || dfd_len < 44 || le32(bytes,dfd)? as usize != dfd_len || le32(bytes,dfd+4)? != 0 { return Err(error()); }
    let model = bytes[dfd+12];
    if !matches!((model,scheme),(163,1)|(166,0|2)) { return Err(error()); }
    if bytes[dfd+16..dfd+20] != [3,3,0,0] { return Err(error()); }
    for (start,len) in [(le32(bytes,56)? as u64,le32(bytes,60)? as u64),(le64(bytes,64)?,le64(bytes,72)?)] { if len > 0 { range(bytes.len() as u64,start,len)?; } }
    if scheme == 1 && le64(bytes,72)? < 20 { return Err(error()); }
    let mut budget = 0u64;
    for level in 0..levels {
        let start = le64(bytes,80+level*24)?; let length = le64(bytes,88+level*24)?; let uncompressed = le64(bytes,96+level*24)?;
        range(bytes.len() as u64,start,length)?;
        if start < (dfd+dfd_len) as u64 { return Err(error()); }
        let w = (width >> level).max(1); let h = (height >> level).max(1);
        let decoded = w*h*4;
        budget = budget.checked_add(decoded).filter(|size| *size <= MAX_DECODED).ok_or_else(error)?;
        if model == 166 {
            let blocks = w.div_ceil(4)*h.div_ceil(4)*16;
            if uncompressed != blocks || (scheme == 0 && length != blocks) { return Err(error()); }
        } else if uncompressed != 0 && uncompressed > MAX_DECODED { return Err(error()); }
    }
    Ok(budget)
}

pub(super) fn images(doc: &Value, reader: &mut (impl Read+Seek), binary_start: Option<u64>) -> Result<(),String> {
    use base64::Engine;
    let views = array(doc,"bufferViews"); let buffers = array(doc,"buffers"); let images = array(doc,"images");
    let mut total = 0u64; let mut seen = std::collections::BTreeSet::new();
    for texture in array(doc,"textures") {
        let Some(ext) = texture["extensions"].get("KHR_texture_basisu") else { continue; };
        let index = number(&ext["source"])? as usize;
        let image = images.get(index).ok_or_else(error)?;
        if !seen.insert(index) { continue; }
        let data = if let Some(uri) = image["uri"].as_str() {
            let payload = uri.strip_prefix("data:image/ktx2;base64,").ok_or_else(error)?;
            if payload.len() as u64 > MAX_IMAGE*4/3+4 { return Err(error()); }
            base64::engine::general_purpose::STANDARD.decode(payload).map_err(|_|error())?
        } else {
            if image["mimeType"] != "image/ktx2" { return Err(error()); }
            let view = views.get(number(&image["bufferView"])? as usize).ok_or_else(error)?;
            if MESHOPT.iter().any(|name|view["extensions"].get(*name).is_some()) { return Err(error()); }
            let index = number(&view["buffer"])? as usize; let buffer = buffers.get(index).ok_or_else(error)?;
            let start = offset(&view["byteOffset"])?; let len = number(&view["byteLength"])?;
            if len == 0 || len > MAX_IMAGE { return Err(error()); }
            if let Some(uri) = buffer["uri"].as_str() {
                let (_,payload) = uri.split_once(',').ok_or_else(error)?;
                let decoded = base64::engine::general_purpose::STANDARD.decode(payload).map_err(|_|error())?;
                decoded.get(start as usize..(start+len) as usize).ok_or_else(error)?.to_vec()
            } else {
                if index != 0 { return Err(error()); }
                reader.seek(SeekFrom::Start(binary_start.ok_or_else(error)? + start)).map_err(|_|error())?;
                let mut bytes = vec![0;len as usize]; reader.read_exact(&mut bytes).map_err(|_|error())?; bytes
            }
        };
        total = total.checked_add(ktx2_budget(&data)?).filter(|bytes|*bytes<=MAX_DECODED).ok_or_else(error)?;
    }
    Ok(())
}
