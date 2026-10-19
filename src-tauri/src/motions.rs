//! Separate, bot-owned VRMA 1.0 assets. No model, texture or remote resource loading.
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::{BTreeMap, BTreeSet}, fs, io::{Read, Write}, path::{Path, PathBuf}};

pub const MAX_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_MOTIONS: usize = 32;
pub const MAX_SECONDS: f64 = 120.0;
const BONES: &[&str] = &["hips","spine","chest","upperChest","neck","head","jaw","leftShoulder","leftUpperArm","leftLowerArm","leftHand","rightShoulder","rightUpperArm","rightLowerArm","rightHand","leftUpperLeg","leftLowerLeg","leftFoot","leftToes","rightUpperLeg","rightLowerLeg","rightFoot","rightToes","leftThumbMetacarpal","leftThumbProximal","leftThumbDistal","leftIndexProximal","leftIndexIntermediate","leftIndexDistal","leftMiddleProximal","leftMiddleIntermediate","leftMiddleDistal","leftRingProximal","leftRingIntermediate","leftRingDistal","leftLittleProximal","leftLittleIntermediate","leftLittleDistal","rightThumbMetacarpal","rightThumbProximal","rightThumbDistal","rightIndexProximal","rightIndexIntermediate","rightIndexDistal","rightMiddleProximal","rightMiddleIntermediate","rightMiddleDistal","rightRingProximal","rightRingIntermediate","rightRingDistal","rightLittleProximal","rightLittleIntermediate","rightLittleDistal"];
const REQUIRED_BONES: &[&str] = &["hips","spine","head","leftUpperArm","leftLowerArm","leftHand","rightUpperArm","rightLowerArm","rightHand","leftUpperLeg","leftLowerLeg","leftFoot","rightUpperLeg","rightLowerLeg","rightFoot"];
const PRESETS: &[&str] = &["happy","angry","sad","relaxed","surprised","aa","ih","ou","ee","oh","blink","blinkLeft","blinkRight","neutral"];
fn error(value: impl std::fmt::Display) -> String { value.to_string() }
fn invalid() -> String { "VRMA ist ungültig oder überschreitet die unterstützten Grenzen.".into() }
fn array<'a>(value: &'a Value, limit: usize) -> Result<&'a Vec<Value>, String> { value.as_array().filter(|values| values.len() <= limit).ok_or_else(invalid) }
fn label(value: Option<&str>, fallback: &str) -> String { value.filter(|name| !name.trim().is_empty()).unwrap_or(fallback).chars().filter(|ch| !ch.is_control()).take(160).collect() }
pub fn valid_id(id: &str) -> bool { id.strip_suffix(".vrma").is_some_and(|id| uuid::Uuid::parse_str(id).ok().is_some_and(|parsed| parsed.to_string() == id)) }
pub fn action(id: &str, index: usize) -> String { format!("vrma:{id}:{index}") }
pub fn parse_action(value: &str) -> Option<(&str, usize)> {
    let (id, index) = value.strip_prefix("vrma:")?.split_once(':')?;
    let index: usize = index.parse().ok()?;
    (valid_id(id) && index < 32 && value == action(id, index)).then_some((id, index))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip { pub name: String, pub index: usize, pub duration: f64, pub body: Vec<String>, pub expressions: Vec<String>, pub look_at: bool }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Motion { pub id: String, pub name: String, pub clips: Vec<Clip> }

/// Validate the bytes that will actually be stored/served, including accessor
/// allocations and real input times. Declared accessor min/max is not trusted.
pub fn inspect(bytes: &[u8]) -> Result<Vec<Clip>, String> {
    if bytes.len() > MAX_BYTES { return Err("Eine VRMA-Datei darf höchstens 16 MiB groß sein.".into()); }
    let (document, binary_length) = crate::avatar_metadata::container_bytes(bytes)?;
    if document["asset"]["version"] != "2.0" || document["asset"].get("minVersion").is_some_and(|version| version != "2.0") { return Err(invalid()); }
    let extensions = document["extensions"].as_object().ok_or_else(invalid)?;
    if extensions.len() != 1 || extensions.get("VRMC_vrm_animation").is_none_or(|value| value["specVersion"] != "1.0") { return Err("Der Bewegungsimport unterstützt ausschließlich VRMA 1.0.".into()); }
    for key in ["extensionsUsed", "extensionsRequired"] { if let Some(values) = document.get(key) { if array(values, 1)?.iter().any(|value| value != "VRMC_vrm_animation") { return Err(invalid()); } } }
    for key in ["meshes","skins","materials","images","textures","cameras"] { if document.get(key).is_some_and(|value| value.as_array().is_none_or(|values| !values.is_empty())) { return Err("VRMA-Bewegungen müssen ohne Geometrie, Bilder und externe Ressourcen eingebettet sein.".into()); } }
    fn nested_extensions(value: &Value, root: bool) -> bool {
        match value {
            Value::Object(fields) => fields.iter().any(|(key, value)| key != "extras" && ((key == "extensions" && !root) || nested_extensions(value, false))),
            Value::Array(values) => values.iter().any(|value| nested_extensions(value, false)),
            _ => false,
        }
    }
    if nested_extensions(&document, true) { return Err(invalid()); }
    crate::avatar_metadata::validate_resources(&document, binary_length)?;
    let nodes = array(&document["nodes"], 2048)?;
    let mut parents = vec![None; nodes.len()];
    for (index, node) in nodes.iter().enumerate() {
        if !node.is_object() || node.get("matrix").is_some() { return Err(invalid()); }
        for (key, len) in [("translation",3), ("rotation",4), ("scale",3)] { if let Some(value) = node.get(key) { if array(value, len)?.len() != len || value.as_array().unwrap().iter().any(|value| value.as_f64().is_none_or(|value| !value.is_finite())) { return Err(invalid()); } } }
        if let Some(rotation)=node["rotation"].as_array(){let norm=rotation.iter().map(|value|value.as_f64().unwrap().powi(2)).sum::<f64>();if (norm-1.0).abs()>0.001{return Err(invalid());}}
        if node["scale"].as_array().is_some_and(|scale|scale.iter().any(|value|value.as_f64().unwrap().abs()<0.000001)){return Err(invalid());}
        if let Some(children) = node.get("children") { for child in array(children, nodes.len())? { let child = child.as_u64().and_then(|index| usize::try_from(index).ok()).filter(|child| *child < nodes.len()).ok_or_else(invalid)?; if parents[child].replace(index).is_some() { return Err(invalid()); } } }
    }
    for index in 0..nodes.len() { let mut next = Some(index); let mut seen = BTreeSet::new(); while let Some(index) = next { if !seen.insert(index) || seen.len() > 128 { return Err(invalid()); } next = parents[index]; } }
    let scenes=array(&document["scenes"],32)?;
    let selected=document.get("scene").map(|scene|scene.as_u64().ok_or_else(invalid)).transpose()?.unwrap_or(0);
    let selected=scenes.get(selected as usize).ok_or_else(invalid)?;
    let mut roots=BTreeSet::new();
    for scene in scenes { for node in array(&scene["nodes"],nodes.len())? { if node.as_u64().is_none_or(|index| index as usize >= nodes.len() || parents[index as usize].is_some()) { return Err(invalid()); } } }
    for node in array(&selected["nodes"],nodes.len())?{if !roots.insert(node.as_u64().ok_or_else(invalid)? as usize){return Err(invalid());}}
    let extension = &extensions["VRMC_vrm_animation"];
    let mut mapping = BTreeMap::<usize, (String, String)>::new();
    let mut bind = |value: &Value, kind: &str, name: &str| -> Result<(), String> {
        let index = value["node"].as_u64().filter(|index| *index < nodes.len() as u64).ok_or_else(invalid)? as usize;
        if mapping.insert(index, (kind.into(), name.into())).is_some() { return Err(invalid()); } Ok(())
    };
    if let Some(humanoid) = extension.get("humanoid") {
        let bones = humanoid["humanBones"].as_object().ok_or_else(invalid)?;
        if REQUIRED_BONES.iter().any(|name| !bones.contains_key(*name)) { return Err(invalid()); }
        for (name, value) in bones { if !BONES.contains(&name.as_str()) { return Err(invalid()); } bind(value, "body", name)?; }
        // The official retargeter scales hips translation by restHipsPosition.y.
        // Validate the actual world rest height before it can divide by zero.
        let mut point=[0f64;3];let mut current=Some(bones["hips"]["node"].as_u64().ok_or_else(invalid)? as usize);
        while let Some(index)=current{
            let node=&nodes[index];let scale=node["scale"].as_array();for axis in 0..3{point[axis]*=scale.map(|scale|scale[axis].as_f64().unwrap()).unwrap_or(1.0);}
            if let Some(rotation)=node["rotation"].as_array(){let q=[rotation[0].as_f64().unwrap(),rotation[1].as_f64().unwrap(),rotation[2].as_f64().unwrap()];let w=rotation[3].as_f64().unwrap();let cross=|a:[f64;3],b:[f64;3]|[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]];let first=cross(q,point);let second=cross(q,first);for axis in 0..3{point[axis]+=2.0*(w*first[axis]+second[axis]);}}
            if let Some(translation)=node["translation"].as_array(){for axis in 0..3{point[axis]+=translation[axis].as_f64().unwrap();}}
            if point.iter().any(|value|!value.is_finite()){return Err(invalid());}current=parents[index];
        }
        if point[1]<=0.001{return Err(invalid());}
    }
    if let Some(expressions) = extension.get("expressions") { for kind in ["preset", "custom"] { if let Some(values) = expressions.get(kind) { let values = values.as_object().filter(|values| values.len() <= 128).ok_or_else(invalid)?; for (name, value) in values { if name.is_empty() || name.len() > 128 || name.chars().any(char::is_control) || (kind == "preset") != PRESETS.contains(&name.as_str()) || ["lookUp","lookDown","lookLeft","lookRight"].contains(&name.as_str()) { return Err(invalid()); } bind(value, "expression", name)?; } } } }
    if let Some(look_at) = extension.get("lookAt") { if look_at.get("node").is_some() { bind(look_at, "lookAt", "lookAt")?; } if let Some(offset) = look_at.get("offsetFromHeadBone") { if array(offset, 3)?.len() != 3 || offset.as_array().unwrap().iter().any(|value| value.as_f64().is_none_or(|value| !value.is_finite())) { return Err(invalid()); } } }
    if mapping.is_empty() { return Err(invalid()); }
    for node in mapping.keys(){let mut root=*node;while let Some(parent)=parents[root]{root=parent;}if !roots.contains(&root){return Err(invalid());}}
    let mut binary = None; let mut offset = 12;
    while offset + 8 <= bytes.len() { let length = u32::from_le_bytes(bytes[offset..offset+4].try_into().unwrap()) as usize; let kind = u32::from_le_bytes(bytes[offset+4..offset+8].try_into().unwrap()); if kind == 0x004e4942 { binary = Some(&bytes[offset+8..offset+8+length]); } offset += 8 + length; }
    let buffers = array(&document["buffers"], 32)?.iter().map(|buffer| {
        if let Some(uri) = buffer["uri"].as_str() { let (_, data) = uri.split_once(',').ok_or_else(invalid)?; base64::engine::general_purpose::STANDARD.decode(data).map_err(|_| invalid()) }
        else { binary.map(|binary| binary.to_vec()).ok_or_else(invalid) }
    }).collect::<Result<Vec<_>, _>>()?;
    if buffers.iter().map(Vec::len).sum::<usize>() > MAX_BYTES { return Err(invalid()); }
    let accessors = array(&document["accessors"], 4096)?;
    let views = array(&document["bufferViews"], 4096)?;
    let values = |index: &Value, shape: &str| -> Result<Vec<f32>, String> {
        let accessor = index.as_u64().and_then(|index| accessors.get(index as usize)).ok_or_else(invalid)?;
        let dimensions = match shape { "SCALAR" => 1, "VEC3" => 3, "VEC4" => 4, _ => return Err(invalid()) };
        let count = accessor["count"].as_u64().filter(|count| (1..=200_000).contains(count)).ok_or_else(invalid)? as usize;
        if accessor["componentType"] != 5126 || accessor["type"] != shape || accessor.get("sparse").is_some() || accessor["normalized"].as_bool() == Some(true) { return Err(invalid()); }
        let view = accessor["bufferView"].as_u64().and_then(|index| views.get(index as usize)).ok_or_else(invalid)?;
        if view.get("byteStride").is_some() { return Err(invalid()); }
        let start = view["byteOffset"].as_u64().unwrap_or(0).checked_add(accessor["byteOffset"].as_u64().unwrap_or(0)).ok_or_else(invalid)?;
        let length = count * dimensions * 4;
        if start % 4 != 0 || accessor["byteOffset"].as_u64().unwrap_or(0).checked_add(length as u64).is_none_or(|end| end > view["byteLength"].as_u64().unwrap_or(0)) { return Err(invalid()); }
        let buffer = view["buffer"].as_u64().and_then(|index| buffers.get(index as usize)).ok_or_else(invalid)?;
        let bytes = buffer.get(start as usize..start as usize+length).ok_or_else(invalid)?;
        let values = bytes.chunks_exact(4).map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap())).collect::<Vec<_>>();
        if values.iter().any(|value| !value.is_finite()) { return Err(invalid()); } Ok(values)
    };
    let animations = array(&document["animations"], 32)?;
    if animations.is_empty() { return Err(invalid()); }
    let mut clips = Vec::new(); let mut point_budget = 2_000_000usize;
    for (index, animation) in animations.iter().enumerate() {
        let samplers = array(&animation["samplers"], 256)?; let channels = array(&animation["channels"], 256)?;
        if channels.is_empty() { return Err(invalid()); }
        let mut duration = 0f64; let mut body = BTreeSet::new(); let mut expressions = BTreeSet::new(); let mut look_at = false; let mut targets = BTreeSet::new();
        for channel in channels {
            let node = channel["target"]["node"].as_u64().ok_or_else(invalid)? as usize;
            let (kind, name) = mapping.get(&node).ok_or_else(invalid)?;
            let path = channel["target"]["path"].as_str().ok_or_else(invalid)?;
            if !targets.insert((node, path)) { return Err(invalid()); }
            let shape = match (kind.as_str(), path) { ("body", "rotation") => { body.insert(name.clone()); "VEC4" }, ("body", "translation") if name == "hips" => { body.insert(name.clone()); "VEC3" }, ("expression", "translation") => { expressions.insert(name.clone()); "VEC3" }, ("lookAt", "rotation") => { look_at = true; "VEC4" }, _ => return Err(invalid()) };
            let sampler = channel["sampler"].as_u64().and_then(|index| samplers.get(index as usize)).ok_or_else(invalid)?;
            let interpolation = sampler["interpolation"].as_str().unwrap_or("LINEAR");
            if !["LINEAR","STEP","CUBICSPLINE"].contains(&interpolation) { return Err(invalid()); }
            let times = values(&sampler["input"], "SCALAR")?;
            if times[0] < 0.0 || times.windows(2).any(|pair| pair[0] >= pair[1]) { return Err(invalid()); }
            duration = duration.max(*times.last().unwrap() as f64);
            let output = values(&sampler["output"], shape)?;
            let dimensions = if shape == "VEC4" { 4 } else { 3 };
            if output.len() != times.len() * dimensions * if interpolation == "CUBICSPLINE" { 3 } else { 1 } { return Err(invalid()); }
            if shape=="VEC4"{let factor=if interpolation=="CUBICSPLINE"{3}else{1};for group in output.chunks_exact(dimensions*factor){let key=if factor==3{&group[dimensions..dimensions*2]}else{group};let norm=key.iter().map(|value|(*value as f64).powi(2)).sum::<f64>();if (norm-1.0).abs()>0.01{return Err(invalid());}}}
            point_budget = point_budget.checked_sub(times.len() + output.len()).ok_or_else(invalid)?;
        }
        if duration <= 0.0 || duration > MAX_SECONDS { return Err("VRMA-Clips müssen länger als 0 und höchstens 120 Sekunden sein.".into()); }
        clips.push(Clip { name: label(animation["name"].as_str(), &format!("Animation {}",index+1)), index, duration, body: body.into_iter().collect(), expressions: expressions.into_iter().collect(), look_at });
    }
    Ok(clips)
}

pub fn directory(root: &Path, bot_id: &str, create: bool) -> Result<PathBuf, String> {
    if bot_id.is_empty() || bot_id.len() > 96 || !bot_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-'|b'_')) { return Err(invalid()); }
    let mut directory = root.canonicalize().map_err(error)?;
    for (index, part) in ["profiles",bot_id,"motions"].into_iter().enumerate() { directory.push(part); if create && index == 2 && !directory.exists() { fs::create_dir(&directory).map_err(error)?; } let metadata = fs::symlink_metadata(&directory).map_err(error)?; if metadata.file_type().is_symlink() || !metadata.is_dir() { return Err(invalid()); } }
    Ok(directory)
}
fn read_file(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(error)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_BYTES as u64 { return Err(invalid()); }
    let mut bytes = Vec::new(); fs::File::open(path).map_err(error)?.take(MAX_BYTES as u64+1).read_to_end(&mut bytes).map_err(error)?;
    inspect(&bytes)?; Ok(bytes)
}
pub fn import(root: &Path, bot_id: &str, source: &Path) -> Result<Motion, String> {
    if !source.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("vrma")) { return Err("Bitte eine separate .vrma-Datei auswählen.".into()); }
    let bytes = read_file(source)?; let clips = inspect(&bytes)?;
    let name = label(source.file_stem().and_then(|name|name.to_str()), "VRMA");
    let id = format!("{}.vrma", uuid::Uuid::new_v4()); let directory = directory(root, bot_id, true)?;
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(directory.join(&id)).map_err(error)?;
    if let Err(problem) = file.write_all(&bytes).and_then(|_|file.sync_all()) { drop(file); let _ = fs::remove_file(directory.join(&id)); return Err(error(problem)); }
    Ok(Motion { id, name, clips })
}
pub fn validate_profile(profile: &Value) -> Result<(), String> {
    if let Some(motions) = profile.get("motions") { let mut ids = BTreeSet::new(); for motion in array(motions, MAX_MOTIONS)? { let id = motion["id"].as_str().filter(|id| valid_id(id)).ok_or_else(invalid)?; if !ids.insert(id) || motion["name"].as_str().is_none_or(|name| name.len() > 640 || name.chars().any(char::is_control)) { return Err(invalid()); } } }
    for emote in profile["emotes"].as_array().into_iter().flatten() { if let Some(action) = emote["action"].as_str().filter(|action|action.starts_with("vrma:")) { let (id,index) = parse_action(action).ok_or_else(invalid)?; if !profile["motions"].as_array().is_some_and(|motions| motions.iter().any(|motion|motion["id"] == id && motion["clips"].as_array().is_some_and(|clips|index < clips.len()))) { return Err("VRMA-Emote verweist auf keine zugewiesene Bewegung dieses Bots.".into()); } } }
    Ok(())
}
/// Replace untrusted/client-supplied metadata with inspection of the actual files.
pub fn refresh(profile: &mut Value, directory: &Path) -> Result<(), String> {
    validate_profile(profile)?;
    let mut total = 0usize;
    if profile["motions"].as_array().is_some_and(|motions| !motions.is_empty()) { for path in [directory.to_path_buf(),directory.join("motions")] { let meta = fs::symlink_metadata(path).map_err(error)?; if !meta.is_dir() || meta.file_type().is_symlink() { return Err(invalid()); } } }
    if let Some(motions) = profile.get_mut("motions").and_then(Value::as_array_mut) { for motion in motions { let id = motion["id"].as_str().ok_or_else(invalid)?; let path = directory.join("motions").join(id); let bytes = read_file(&path)?; total += bytes.len(); if total > 64*1024*1024 { return Err(invalid()); } motion["clips"] = serde_json::to_value(inspect(&bytes)?).map_err(error)?; } }
    validate_profile(profile)
}
pub fn read(root: &Path, profile: &Value, id: &str) -> Result<Vec<u8>, String> {
    if !valid_id(id) || !profile["motions"].as_array().is_some_and(|motions| motions.iter().any(|motion|motion["id"] == id)) { return Err("Diese Bewegung ist diesem Bot nicht zugewiesen.".into()); }
    read_file(&directory(root, profile["id"].as_str().ok_or_else(invalid)?, false)?.join(id))
}
pub fn duration(profile: &Value, action: &str) -> Result<f64, String> {
    let (id, index) = parse_action(action).ok_or_else(invalid)?;
    profile["motions"].as_array().and_then(|motions| motions.iter().find(|motion|motion["id"] == id)).and_then(|motion|motion["clips"][index]["duration"].as_f64()).filter(|duration| duration.is_finite() && *duration > 0.0 && *duration <= MAX_SECONDS).ok_or_else(invalid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const FIXTURE: &[u8] = include_bytes!("../../src/lib/fixtures/greeting.vrma");
    fn changed(change: impl FnOnce(&mut Value)) -> Vec<u8> {
        let (mut document, _) = crate::avatar_metadata::container_bytes(FIXTURE).unwrap(); change(&mut document);
        let json_length = u32::from_le_bytes(FIXTURE[12..16].try_into().unwrap()) as usize;
        let binary = &FIXTURE[20+json_length+8..];
        let mut json = serde_json::to_vec(&document).unwrap(); while json.len()%4 != 0 { json.push(b' '); }
        let mut bytes = b"glTF".to_vec(); bytes.extend(2u32.to_le_bytes()); bytes.extend(((28+json.len()+binary.len()) as u32).to_le_bytes()); bytes.extend((json.len() as u32).to_le_bytes()); bytes.extend(0x4e4f534au32.to_le_bytes()); bytes.extend(json); bytes.extend((binary.len() as u32).to_le_bytes()); bytes.extend(0x004e4942u32.to_le_bytes()); bytes.extend(binary); bytes
    }
    struct Fixture(PathBuf);
    impl Fixture { fn new() -> Self { let root=std::env::temp_dir().join(format!("orbitpal-motion-{}",uuid::Uuid::new_v4())); for id in ["nova","fox"]{fs::create_dir_all(root.join("profiles").join(id)).unwrap();} Self(root) } }
    impl Drop for Fixture {fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
    #[test]
    fn real_vrma_metadata_uses_actual_key_times_and_all_track_kinds() {
        let clips=inspect(FIXTURE).unwrap(); assert_eq!(clips.len(),2); assert_eq!(clips[0].name,"Hello"); assert_eq!(clips[0].duration,1.25); assert_eq!(clips[1].duration,0.5); assert_eq!(clips[0].body,["hips","leftUpperArm"]); assert_eq!(clips[0].expressions,["happy"]); assert!(clips[0].look_at);
        let misleading=changed(|document|document["accessors"][0]["max"]=json!([119]));assert_eq!(inspect(&misleading).unwrap()[0].duration,1.25);
        assert!(crate::avatar_metadata::inspect_bytes(FIXTURE).unwrap_err().contains("separaten Bewegungsimport"));
    }
    #[test]
    fn resources_versions_geometry_and_required_extensions_are_rejected() {
        for bytes in [changed(|d|d["extensions"]["VRMC_vrm_animation"]["specVersion"]=json!("1.0-draft")),changed(|d|d["extensions"]=json!({})),changed(|d|d["buffers"][0]["uri"]=json!("https://example.invalid/private")),changed(|d|d["meshes"]=json!([{}])),changed(|d|d["extensionsRequired"]=json!(["VENDOR_bad"])),changed(|d|d["nodes"][0]["extensions"]=json!({"VENDOR_bad":{}}))] { assert!(inspect(&bytes).is_err()); }
        assert!(inspect(&FIXTURE[..FIXTURE.len()-1]).is_err());
        let mut oversized=Vec::from(FIXTURE);oversized.resize(MAX_BYTES+1,0);assert!(inspect(&oversized).is_err());
    }
    #[test]
    fn malformed_graph_accessors_and_targets_fail_without_allocation_or_panics() {
        for bytes in [changed(|d|d["nodes"][0]["children"]=json!([0])),changed(|d|d["nodes"][1]["children"]=json!([2,2])),changed(|d|d["nodes"][0]["translation"]=json!([0,0,0])),changed(|d|d["accessors"][0]["count"]=json!(u64::MAX)),changed(|d|d["accessors"][0]["byteOffset"]=json!(u64::MAX)),changed(|d|d["accessors"][0]["sparse"]=json!({})),changed(|d|d["animations"][0]["channels"][0]["target"]["node"]=json!(2049)),changed(|d|d["animations"][0]["channels"][0]["target"]["path"]=json!("scale")),changed(|d|d["animations"][0]["samplers"][0]["output"]=json!(99999))] { assert!(inspect(&bytes).is_err()); }
    }
    #[test]
    fn only_assigned_regular_bot_files_can_be_served_and_metadata_is_refreshed() {
        let fixture=Fixture::new(); let source=fixture.0.join("hello.vrma");fs::write(&source,FIXTURE).unwrap();let motion=import(&fixture.0,"nova",&source).unwrap();
        let mut profile=json!({"id":"nova","motions":[motion],"emotes":[]}); assert_eq!(read(&fixture.0,&profile,profile["motions"][0]["id"].as_str().unwrap()).unwrap(),FIXTURE);
        let id=profile["motions"][0]["id"].as_str().unwrap().to_owned(); assert!(read(&fixture.0,&json!({"id":"fox","motions":profile["motions"]}),&id).is_err());assert!(read(&fixture.0,&json!({"id":"nova","motions":[]}),&id).is_err());
        profile["motions"][0]["clips"][0]["duration"]=json!(119);refresh(&mut profile,&fixture.0.join("profiles/nova")).unwrap();assert_eq!(duration(&profile,&action(&id,0)).unwrap(),1.25);
        profile["emotes"]=json!([{"action":action(&id,31)}]);assert!(validate_profile(&profile).is_err());
        for id in ["../other.vrma","00000000000000000000000000000000.vrma","https://example.invalid/motion.vrma"] {assert!(!valid_id(id));}
    }
    #[test]
    fn profile_roundtrip_copies_and_revalidates_motions_without_exporting_unassigned_files() {
        let fixture=Fixture::new();let store=crate::storage::Store::new(fixture.0.clone()).unwrap();let source=fixture.0.join("hello.vrma");fs::write(&source,FIXTURE).unwrap();
        let motion=import(&fixture.0,"nova",&source).unwrap();let action=action(&motion.id,0);
        let mut profile=store.profile().unwrap();profile["avatar"]["assetPath"]=json!("builtin:osa-mushy");profile["motions"]=json!([motion]);profile["emotes"]=json!([{"id":"hello","name":"Hello","action":action,"duration":119,"allowChat":false}]);store.save_profile(profile).unwrap();
        assert_eq!(crate::overlay::prepare_emote_profile(&fixture.0,&store.profile().unwrap(),"hello",false).unwrap()["duration"],1.25);
        let orphan=format!("{}.vrma",uuid::Uuid::new_v4());fs::write(fixture.0.join("profiles/nova/motions").join(orphan),FIXTURE).unwrap();
        let exported=fixture.0.join("export.json");store.export(&exported).unwrap();let bundle:Value=serde_json::from_slice(&fs::read(&exported).unwrap()).unwrap();
        assert_eq!(bundle["files"].as_array().unwrap().iter().filter(|file|file["path"].as_str().is_some_and(|path|path.starts_with("motions/"))).count(),1);
        let target=Fixture::new();let destination=crate::storage::Store::new(target.0.clone()).unwrap();let imported=destination.import(&exported).unwrap();assert_ne!(imported["id"],"nova");
        assert_eq!(read(&target.0,&imported,&motion.id).unwrap(),FIXTURE);
        let copied=store.create_bot("Motion copy",true).unwrap();assert_eq!(read(&fixture.0,&copied,&motion.id).unwrap(),FIXTURE);
        let mut bad=bundle;bad["files"].as_array_mut().unwrap().iter_mut().find(|file|file["path"].as_str().is_some_and(|path|path.starts_with("motions/"))).unwrap()["base64"]=json!(base64::engine::general_purpose::STANDARD.encode(b"invalid"));
        fs::write(&exported,bad.to_string()).unwrap();let before=destination.list_bots().unwrap().len();assert!(destination.import(&exported).is_err());assert_eq!(destination.list_bots().unwrap().len(),before);
    }
    #[cfg(unix)]
    #[test]
    fn symlinks_and_parent_redirection_are_rejected() {
        let fixture=Fixture::new();let source=fixture.0.join("hello.vrma");fs::write(&source,FIXTURE).unwrap();let link=fixture.0.join("link.vrma");std::os::unix::fs::symlink(&source,&link).unwrap();assert!(import(&fixture.0,"nova",&link).is_err());
        std::os::unix::fs::symlink(fixture.0.join("profiles/nova"),fixture.0.join("profiles/alias")).unwrap();assert!(import(&fixture.0,"alias",&source).is_err());
    }
}
