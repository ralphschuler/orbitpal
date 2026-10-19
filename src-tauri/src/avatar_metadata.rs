//! Bounded, offline inspection of self-contained GLB 2 / VRM 0 and 1 assets.
//! This validates container/resource compatibility; it does not claim that an
//! animation, facial expression or an unsupported audio extension was played.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeSet, fs::{self, File, OpenOptions}, io::{Cursor, Read, Seek, SeekFrom}, path::Path};

pub const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_JSON_BYTES: u64 = 8 * 1024 * 1024;
const JSON: u32 = 0x4e4f534a;
const BIN: u32 = 0x004e4942;
const LEGACY_JSON_PADDING: &str = "Eine ältere Dateiabweichung wird automatisch berücksichtigt.";

// Derived from the installed three GLTFLoader and registered three-vrm loader,
// not from every extension listed in the glTF specification. Decoder-dependent
// Draco, BasisU/KTX2 and Meshopt are deliberately absent: none is configured.
const SUPPORTED_EXTENSIONS: &[&str] = &[
    "KHR_lights_punctual", "KHR_materials_anisotropy", "KHR_materials_clearcoat",
    "KHR_materials_dispersion", "KHR_materials_emissive_strength", "KHR_materials_ior",
    "KHR_materials_iridescence", "KHR_materials_sheen", "KHR_materials_specular",
    "KHR_materials_transmission", "KHR_materials_unlit", "KHR_materials_volume",
    "KHR_mesh_quantization", "KHR_texture_transform", "EXT_materials_bump",
    "EXT_mesh_gpu_instancing", "EXT_texture_avif", "EXT_texture_webp",
    "VRM", "VRMC_vrm", "VRMC_materials_mtoon", "VRMC_materials_hdr_emissiveMultiplier",
    "VRMC_node_constraint", "VRMC_springBone", "VRMC_springBone_extended_collider",
];
const DECODER_EXTENSIONS: &[&str] = &[
    "KHR_draco_mesh_compression", "KHR_texture_basisu", "EXT_meshopt_compression", "KHR_meshopt_compression",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationInfo { pub name: String, pub index: usize, pub duration: Option<f64> }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarInfo {
    pub format: String,
    pub version: String,
    pub container_version: Option<u32>,
    pub animations: Vec<AnimationInfo>,
    pub expressions: Vec<String>,
    pub vowels: Vec<String>,
    pub bones: Vec<String>,
    pub audio_extensions: Vec<String>,
    pub required_extensions: Vec<String>,
    pub used_extensions: Vec<String>,
    pub warnings: Vec<String>,
    pub external_resources: Vec<String>,
}

fn invalid(detail: &str) -> String { format!("Die 3D-Datei ist ungültig: {detail}") }
fn read<const N: usize>(reader: &mut impl Read) -> Result<[u8; N], String> {
    let mut bytes = [0u8; N];
    reader.read_exact(&mut bytes).map_err(|_| invalid("GLB-Header oder Datenblock ist abgeschnitten."))?;
    Ok(bytes)
}

pub fn inspect_file(path: &Path) -> Result<AvatarInfo, String> {
    let meta = fs::symlink_metadata(path).map_err(|_| "Die 3D-Datei konnte nicht geöffnet werden.".to_owned())?;
    if !meta.is_file() || meta.file_type().is_symlink() { return Err(invalid("Bitte eine reguläre 3D-Datei verwenden.")); }
    let mut file = File::open(path).map_err(|_| "Die 3D-Datei konnte nicht gelesen werden.".to_owned())?;
    let length = file.metadata().map_err(|_| "Die Dateigröße konnte nicht gelesen werden.".to_owned())?.len();
    inspect(&mut file, length)
}
pub fn inspect_bytes(bytes: &[u8]) -> Result<AvatarInfo, String> { inspect(&mut Cursor::new(bytes), bytes.len() as u64) }

fn inspect(reader: &mut (impl Read + Seek), length: u64) -> Result<AvatarInfo, String> {
    let (document, binary_length, legacy_padding) = container(reader, length)?;
    let mut info = from_document(&document, binary_length)?;
    if legacy_padding { info.warnings.push(LEGACY_JSON_PADDING.into()); }
    Ok(info)
}

pub(crate) fn container_bytes(bytes: &[u8]) -> Result<(Value, Option<u64>), String> {
    container(&mut Cursor::new(bytes), bytes.len() as u64).map(|(document, binary, _)| (document, binary))
}

fn container(reader: &mut (impl Read + Seek), length: u64) -> Result<(Value, Option<u64>, bool), String> {
    if !(20..=MAX_FILE_BYTES).contains(&length) { return Err(invalid("Avatar muss eine GLB-/VRM-Datei zwischen 20 Bytes und 256 MiB sein.")); }
    let header = read::<12>(reader)?;
    if &header[..4] != b"glTF" { return Err(invalid("Erwartet wird eine binäre GLB- oder VRM-Datei; JSON-glTF und andere Formate werden nicht importiert.")); }
    let version = u32::from_le_bytes(header[4..8].try_into().unwrap());
    if version != 2 { return Err(format!("GLB-Version {version} wird nicht unterstützt. Bitte als GLB 2.0 oder VRM 0/1 exportieren.")); }
    if u32::from_le_bytes(header[8..12].try_into().unwrap()) as u64 != length { return Err(invalid("Die im GLB-Header angegebene Länge stimmt nicht mit der Datei überein.")); }
    let first = read::<8>(reader)?;
    let json_length = u32::from_le_bytes(first[..4].try_into().unwrap()) as u64;
    if u32::from_le_bytes(first[4..].try_into().unwrap()) != JSON || json_length == 0 { return Err(invalid("Der erste GLB-Datenblock muss glTF-JSON enthalten.")); }
    // Some existing VRM-0 exporters omit JSON's four-byte whitespace padding.
    // GLTFLoader reads those chunk headers with DataView at their exact offsets.
    // Preserve that bounded compatibility while checking every declared length.
    let legacy_padding = json_length % 4 != 0;
    if json_length > MAX_JSON_BYTES { return Err(invalid("Der JSON-Metadatenblock überschreitet 8 MiB.")); }
    let mut offset = 20 + json_length;
    if offset > length { return Err(invalid("Der JSON-Datenblock ist abgeschnitten.")); }
    let mut json = vec![0u8; json_length as usize];
    reader.read_exact(&mut json).map_err(|_| invalid("Der JSON-Datenblock ist abgeschnitten."))?;
    let document: Value = serde_json::from_slice(&json).map_err(|_| invalid("Der Metadatenblock enthält kein gültiges glTF-JSON."))?;
    let mut binary_length = None;
    while offset < length {
        if length - offset < 8 { return Err(invalid("Der letzte GLB-Datenblock hat keinen vollständigen Header.")); }
        let chunk = read::<8>(reader)?;
        let size = u32::from_le_bytes(chunk[..4].try_into().unwrap()) as u64;
        let kind = u32::from_le_bytes(chunk[4..].try_into().unwrap());
        offset = offset.checked_add(8 + size).ok_or_else(|| invalid("Datenblockgröße ist ungültig."))?;
        if size % 4 != 0 || offset > length { return Err(invalid("Ein GLB-Datenblock ist abgeschnitten oder nicht ausgerichtet.")); }
        match kind {
            JSON => return Err(invalid("Die GLB-Datei enthält mehrere JSON-Datenblöcke.")),
            BIN if binary_length.replace(size).is_some() => return Err(invalid("Die GLB-Datei enthält mehrere binäre Datenblöcke.")),
            _ => (), // The glTF 2 container permits unknown future chunk types.
        }
        reader.seek(SeekFrom::Start(offset)).map_err(|_| invalid("Ein Datenblock konnte nicht geprüft werden."))?;
    }
    Ok((document, binary_length, legacy_padding))
}

fn extension_names(document: &Value, key: &str) -> Result<Vec<String>, String> {
    let Some(value) = document.get(key) else { return Ok(Vec::new()); };
    let values = value.as_array().ok_or_else(|| invalid("Die Erweiterungsliste muss ein Array sein."))?;
    if values.len() > 128 { return Err(invalid("Die Datei enthält zu viele Erweiterungen.")); }
    values.iter().map(|name| name.as_str().filter(|name| !name.is_empty() && name.len() <= 128 && !name.chars().any(char::is_control)).map(str::to_owned).ok_or_else(|| invalid("Eine Erweiterung hat keinen gültigen Namen."))).collect()
}
fn label(value: Option<&str>, fallback: String) -> String {
    value.filter(|value| !value.trim().is_empty()).map(|value| value.chars().filter(|ch| !ch.is_control()).take(256).collect()).unwrap_or(fallback)
}
fn vowel(name: &str) -> Option<&'static str> {
    let key: String = name.chars().filter(char::is_ascii_alphanumeric).flat_map(char::to_lowercase).collect();
    let key = key.strip_prefix("vrcv").or_else(|| key.strip_prefix("viseme")).or_else(|| key.strip_prefix("mouth")).unwrap_or(&key);
    match key { "a" | "aa" => Some("aa"), "i" | "ih" => Some("ih"), "u" | "ou" => Some("ou"), "e" | "ee" => Some("ee"), "o" | "oh" => Some("oh"), _ => None }
}
fn all_extensions(value: &Value, extensions: &mut BTreeSet<String>) {
    match value {
        Value::Object(fields) => for (name, value) in fields {
            if name == "extras" { continue; } // Application metadata is not loaded as glTF resources/extensions.
            if name == "extensions" { if let Some(fields) = value.as_object() { extensions.extend(fields.keys().cloned()); } }
            all_extensions(value, extensions);
        },
        Value::Array(values) => for value in values { all_extensions(value, extensions); },
        _ => (),
    }
}

fn from_document(document: &Value, binary_length: Option<u64>) -> Result<AvatarInfo, String> {
    if document["asset"]["version"].as_str() != Some("2.0") || document["asset"]["minVersion"].as_str().is_some_and(|version| version != "2.0") { return Err(invalid("Unterstützt wird glTF 2.0; die angegebene Mindestversion wird nicht unterstützt.")); }
    if document["extensions"].get("VRMC_vrm_animation").is_some() { return Err("Diese Datei ist eine VRMA-Animationsdatei. Bitte den separaten Bewegungsimport unter der Figur verwenden.".into()); }
    validate_vrm_versions(document)?;
    let used = extension_names(document, "extensionsUsed")?;
    let required = extension_names(document, "extensionsRequired")?;
    if required.iter().any(|name| !used.contains(name)) { return Err(invalid("Eine erforderliche Erweiterung fehlt in extensionsUsed.")); }
    let mut extensions: BTreeSet<String> = used.iter().chain(&required).cloned().collect();
    all_extensions(document, &mut extensions);
    // All configured texture, mesh and VRM loaders resolve resource URIs through
    // the glTF buffers/images arrays. A URI in extras or license metadata is data.
    let mut external = Vec::new();
    for kind in ["buffers", "images"] {
        for (index, entry) in document[kind].as_array().into_iter().flatten().enumerate() {
            if entry["uri"].as_str().is_some_and(|uri| !uri.starts_with("data:")) { external.push(format!("{kind}/{index}/uri")); }
        }
    }
    if !external.is_empty() { return Err(format!("Die 3D-Datei enthält {} externe Ressourcen. Bitte Texturen, Puffer und weitere Ressourcen in einer einzelnen GLB-/VRM-Datei einbetten; es werden keine externen Dateien oder URLs nachgeladen.", external.len())); }
    let audio_extensions: Vec<String> = extensions.iter().filter(|name| { let name = name.to_ascii_lowercase(); name.contains("audio") || name.contains("sound") || name.contains("emitter") }).cloned().collect();
    for name in &required {
        if !SUPPORTED_EXTENSIONS.contains(&name.as_str()) {
            if audio_extensions.contains(name) { return Err(format!("Die erforderliche Audio-Erweiterung {name} wird vom Avatar-Renderer nicht unterstützt. Modell-Sounds können nicht automatisch abgespielt werden; bitte ohne diese erforderliche Erweiterung exportieren oder Emote-Sounds separat zuweisen.")); }
            if DECODER_EXTENSIONS.contains(&name.as_str()) { return Err(format!("Die erforderliche Komprimierung {name} benötigt einen nicht eingerichteten Decoder. Bitte als unkomprimierte, eingebettete GLB-/VRM-Datei exportieren.")); }
            return Err(format!("Die erforderliche glTF-Erweiterung {name} wird von den installierten Avatar-Loadern nicht unterstützt."));
        }
    }
    // GLTFLoader constructs its Draco handler whenever this is in extensionsUsed,
    // even for optional declarations, and fails without a configured decoder.
    if extensions.contains("KHR_draco_mesh_compression") { return Err("Draco-komprimierte Modelle benötigen einen nicht eingerichteten Decoder. Bitte unkomprimiert als GLB-/VRM-Datei exportieren.".into()); }
    let mut warnings = Vec::new();
    for name in extensions.iter().filter(|name| !SUPPORTED_EXTENSIONS.contains(&name.as_str())) {
        warnings.push(if audio_extensions.contains(name) { format!("Die optionale Audio-Erweiterung {name} wurde erkannt. Eingebettete Modell-Sounds werden damit nicht abgespielt; Emote-Sounds müssen separat zugewiesen werden.") } else { format!("Die optionale Erweiterung {name} wird nicht vollständig unterstützt. Vorhandene glTF-Basisdaten werden verwendet, soweit die Datei einen Fallback enthält.") });
    }
    validate_resources(document, binary_length)?;
    let vrm0 = document["extensions"].get("VRM");
    let vrm1 = document["extensions"].get("VRMC_vrm");
    if vrm0.is_some() && vrm1.is_some() { return Err(invalid("VRM 0 und VRM 1 dürfen nicht gleichzeitig als Modellformat angegeben sein.")); }
    let (format, version, vrm) = match (vrm0, vrm1) {
        (Some(vrm), _) => {
            if vrm["specVersion"].as_str() != Some("0.0") { return Err("Diese VRM-0-Spezifikationsversion wird nicht unterstützt. Bitte als standardkonformes VRM 0 (specVersion 0.0) exportieren.".into()); }
            ("vrm0", "0.0", Some(vrm))
        },
        (_, Some(vrm)) => {
            let version = vrm["specVersion"].as_str().unwrap_or_default();
            if !["1.0", "1.0-beta"].contains(&version) { return Err("Diese VRM-1-Spezifikationsversion wird von three-vrm nicht unterstützt. Bitte als VRM 1.0 exportieren.".into()); }
            ("vrm1", version, Some(vrm))
        },
        _ => ("glb2", "2.0", None),
    };
    let animations = document["animations"].as_array().into_iter().flatten().enumerate().map(|(index, animation)| {
        let duration = animation["samplers"].as_array().into_iter().flatten().filter_map(|sampler| sampler["input"].as_u64().and_then(|input| document["accessors"][input as usize]["max"][0].as_f64())).filter(|time| time.is_finite() && *time >= 0.0).reduce(f64::max);
        AnimationInfo { name: label(animation["name"].as_str(), format!("animation_{index}")), index, duration }
    }).take(1024).collect();
    let mut bones = BTreeSet::new();
    let mut expressions = BTreeSet::new();
    let mut vowels = BTreeSet::new();
    if let Some(vrm) = vrm {
        if format == "vrm0" {
            for bone in vrm["humanoid"]["humanBones"].as_array().into_iter().flatten() { if let Some(name) = bone["bone"].as_str() { bones.insert(label(Some(name), String::new())); } }
            for group in vrm["blendShapeMaster"]["blendShapeGroups"].as_array().into_iter().flatten() {
                let name = group["presetName"].as_str().filter(|name| *name != "unknown").or_else(|| group["name"].as_str()).unwrap_or_default();
                if !name.is_empty() { expressions.insert(label(Some(name), String::new())); }
                if group["binds"].as_array().is_some_and(|binds| !binds.is_empty()) { if let Some(name) = vowel(name) { vowels.insert(name.into()); } }
            }
        } else {
            if let Some(values) = vrm["humanoid"]["humanBones"].as_object() { bones.extend(values.keys().map(|name| label(Some(name), String::new()))); }
            for kind in ["preset", "custom"] { if let Some(values) = vrm["expressions"][kind].as_object() { for (name, expression) in values {
                expressions.insert(label(Some(name), String::new()));
                if expression["morphTargetBinds"].as_array().is_some_and(|binds| !binds.is_empty()) { if let Some(name) = vowel(name) { vowels.insert(name.into()); } }
            } } }
        }
    } else {
        for skin in document["skins"].as_array().into_iter().flatten() { for joint in skin["joints"].as_array().into_iter().flatten().filter_map(Value::as_u64) { bones.insert(label(document["nodes"][joint as usize]["name"].as_str(), format!("node_{joint}"))); } }
        for mesh in document["meshes"].as_array().into_iter().flatten() { for name in mesh["extras"]["targetNames"].as_array().into_iter().flatten().filter_map(Value::as_str) { expressions.insert(label(Some(name), String::new())); if let Some(name) = vowel(name) { vowels.insert(name.into()); } } }
    }
    if !audio_extensions.is_empty() && warnings.is_empty() { warnings.push("Modell-Audio wurde erkannt; die Avatar-Loader spielen eingebettete Sounds nicht ab.".into()); }
    Ok(AvatarInfo { format: format.into(), version: version.into(), container_version: Some(2), animations,
        expressions: expressions.into_iter().take(1024).collect(), vowels: vowels.into_iter().collect(), bones: bones.into_iter().take(4096).collect(),
        audio_extensions, required_extensions: required, used_extensions: used, warnings, external_resources: Vec::new() })
}

// Materials, node constraints and extended spring-bone colliders are nested in
// glTF objects. Check their versions there, rather than only at document root.
fn validate_vrm_versions(value: &Value) -> Result<(), String> {
    match value {
        Value::Object(fields) => {
            if let Some(extensions) = fields.get("extensions").and_then(Value::as_object) {
                for (name, extension) in extensions {
                    let supported: Option<&[&str]> = match name.as_str() {
                        "VRMC_vrm" | "VRMC_materials_mtoon" | "VRMC_node_constraint" | "VRMC_springBone" => Some(&["1.0", "1.0-beta"]),
                        "VRMC_springBone_extended_collider" => Some(&["1.0"]),
                        _ => None,
                    };
                    if let Some(supported) = supported { if extension["specVersion"].as_str().is_none_or(|version| !supported.contains(&version)) { return Err(format!("Die Spezifikationsversion der VRM-Erweiterung {name} wird vom installierten Loader nicht unterstützt.")); } }
                }
            }
            for (name, nested) in fields { if name != "extras" { validate_vrm_versions(nested)?; } }
        },
        Value::Array(values) => for nested in values { validate_vrm_versions(nested)?; },
        _ => (),
    }
    Ok(())
}

pub(crate) fn validate_resources(document: &Value, binary_length: Option<u64>) -> Result<(), String> {
    use base64::Engine;
    let buffers = document["buffers"].as_array().map(Vec::as_slice).unwrap_or_default();
    for (index, buffer) in buffers.iter().enumerate() {
        let size = buffer["byteLength"].as_u64().ok_or_else(|| invalid("Ein Puffer hat keine gültige byteLength."))?;
        if let Some(uri) = buffer["uri"].as_str() {
            let (kind, payload) = uri.split_once(',').filter(|(kind, _)| kind.starts_with("data:") && kind.ends_with(";base64")).ok_or_else(|| invalid("Eingebettete Puffer müssen eine glTF-Base64-Daten-URI enthalten."))?;
            if !["data:application/octet-stream;base64", "data:application/gltf-buffer;base64"].contains(&kind) { return Err(invalid("Die Daten-URI eines Puffers hat keinen unterstützten glTF-Medientyp.")); }
            let bytes = base64::engine::general_purpose::STANDARD.decode(payload).map_err(|_| invalid("Die eingebetteten Pufferdaten sind kein gültiges Base64."))?;
            if bytes.len() as u64 != size { return Err(invalid("Die eingebetteten Pufferdaten stimmen nicht mit byteLength überein.")); }
        } else if index != 0 || binary_length.is_none_or(|length| size > length || length - size > 3) { return Err(invalid("Ein eingebetteter Puffer fehlt, ist abgeschnitten oder hat eine falsche Länge.")); }
    }
    for view in document["bufferViews"].as_array().into_iter().flatten() {
        let buffer = view["buffer"].as_u64().and_then(|index| buffers.get(index as usize)).ok_or_else(|| invalid("Ein bufferView verweist auf einen unbekannten Puffer."))?;
        let size = view["byteLength"].as_u64().ok_or_else(|| invalid("Ein bufferView hat keine gültige Länge."))?;
        let offset = view["byteOffset"].as_u64().unwrap_or(0);
        if offset.checked_add(size).is_none_or(|end| end > buffer["byteLength"].as_u64().unwrap_or(0)) { return Err(invalid("Ein bufferView liegt außerhalb des eingebetteten Puffers.")); }
    }
    Ok(())
}

/// Copy at most the supported budget, validate the actual destination, and
/// remove failed imports. A changed source cannot leave an unchecked avatar.
pub fn copy_validated(source: &Path, destination: &Path) -> Result<(), String> {
    inspect_file(source)?;
    let input = File::open(source).map_err(|_| "Die 3D-Datei konnte nicht gelesen werden.".to_owned())?;
    let mut output = OpenOptions::new().write(true).create_new(true).open(destination).map_err(|_| "Die 3D-Datei konnte nicht importiert werden.".to_owned())?;
    let result = std::io::copy(&mut input.take(MAX_FILE_BYTES + 1), &mut output).map_err(|_| "Die 3D-Datei konnte nicht vollständig kopiert werden.".to_owned())
        .and_then(|size| if size > MAX_FILE_BYTES { Err(invalid("Die Datei überschreitet 256 MiB.")) } else { Ok(()) });
    drop(output);
    let result = result.and_then(|_| inspect_file(destination).map(|_| ()));
    if result.is_err() { let _ = fs::remove_file(destination); }
    result
}

pub fn imported_asset_name(name: &str) -> bool {
    let path = Path::new(name);
    let extension = path.extension().and_then(|value| value.to_str()).unwrap_or_default();
    let stem = path.file_stem().and_then(|value| value.to_str()).unwrap_or_default();
    path.components().count() == 1 && ["vrm", "glb"].contains(&extension) && uuid::Uuid::parse_str(stem).ok().is_some_and(|id| id.to_string() == stem)
}
/// Read only the bounded JSON metadata; seek over BIN instead of loading it.
pub(crate) fn imported_document(root: &Path, name: &str) -> Result<Value, String> {
    if !imported_asset_name(name) { return Err("Unbekannte importierte Figur".into()); }
    let directory = root.join("avatars");
    if fs::symlink_metadata(&directory).map_err(|_| "Der Figurenordner ist nicht verfügbar.")?.file_type().is_symlink() { return Err("Der Figurenordner darf keine symbolische Verknüpfung sein.".into()); }
    let path = directory.join(name);
    let meta = fs::symlink_metadata(&path).map_err(|_| "Die 3D-Datei konnte nicht geöffnet werden.")?;
    if !meta.is_file() || meta.file_type().is_symlink() { return Err("Bitte eine reguläre 3D-Datei verwenden.".into()); }
    let mut file = File::open(path).map_err(|_| "Die 3D-Datei konnte nicht gelesen werden.")?;
    let size = file.metadata().map_err(|_| "Die Dateigröße konnte nicht gelesen werden.")?.len();
    container(&mut file,size).map(|(document,_,_)|document)
}
pub fn for_asset(root: &Path, asset: Option<&str>) -> Result<AvatarInfo, String> {
    if asset.is_none() || asset == Some("nova") || asset.is_some_and(|name| name.starts_with("builtin:")) {
        let catalog: Value = serde_json::from_str(include_str!("../../public/character-capabilities.json")).map_err(|_| "Die mitgelieferten Modelldaten sind ungültig.".to_owned())?;
        return serde_json::from_value(catalog["characters"][asset.unwrap_or("nova")].clone()).map_err(|_| "Die mitgelieferte Figur ist unbekannt.".to_owned());
    }
    let name = asset.unwrap_or_default();
    if !imported_asset_name(name) {
        return Err("Modelldaten sind nur für bekannte Figuren oder importierte GLB-/VRM-Dateien verfügbar.".into());
    }
    let directory = root.join("avatars");
    if fs::symlink_metadata(&directory).map_err(|_| "Der Figurenordner ist nicht verfügbar.")?.file_type().is_symlink() { return Err("Der Figurenordner darf keine symbolische Verknüpfung sein.".into()); }
    inspect_file(&directory.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let directory = std::env::temp_dir().join(format!("cohost-avatar-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(directory.join("avatars")).unwrap(); Self(directory)
        }
    }
    impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
    fn glb(document: Value, binary: Option<&[u8]>) -> Vec<u8> {
        let mut json = serde_json::to_vec(&document).unwrap();
        while json.len() % 4 != 0 { json.push(b' '); }
        let mut bytes = b"glTF".to_vec(); bytes.extend(2u32.to_le_bytes()); bytes.extend([0; 4]);
        bytes.extend((json.len() as u32).to_le_bytes()); bytes.extend(JSON.to_le_bytes()); bytes.extend(json);
        if let Some(binary) = binary {
            let mut binary = binary.to_vec(); while binary.len() % 4 != 0 { binary.push(0); }
            bytes.extend((binary.len() as u32).to_le_bytes()); bytes.extend(BIN.to_le_bytes()); bytes.extend(binary);
        }
        let length = bytes.len() as u32; bytes[8..12].copy_from_slice(&length.to_le_bytes()); bytes
    }
    fn base() -> Value { json!({"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[]}],"nodes":[]}) }

    #[test]
    fn extracts_clip_names_duration_and_generic_skin_morph_features_without_reading_binary() {
        let mut document = base();
        document["nodes"] = json!([{"name":"Head"},{"name":"Jaw"}]);
        document["skins"] = json!([{"joints":[0,1]}]);
        document["meshes"] = json!([{"extras":{"targetNames":["viseme_aa","mouth_i","Smile"]}}]);
        document["accessors"] = json!([{"max":[2.25]}]);
        document["animations"] = json!([{"name":"Wave","samplers":[{"input":0}]},{"samplers":[]}]);
        let info = inspect_bytes(&glb(document, None)).unwrap();
        assert_eq!(info.format, "glb2"); assert_eq!(info.version, "2.0");
        assert_eq!(info.animations[0].name, "Wave"); assert_eq!(info.animations[0].duration, Some(2.25));
        assert_eq!(info.animations[1].name, "animation_1"); assert_eq!(info.bones, ["Head", "Jaw"]);
        assert_eq!(info.vowels, ["aa", "ih"]); assert!(info.audio_extensions.is_empty());
    }
    #[test]
    fn identifies_vrm_zero_and_both_supported_vrm_one_versions_without_extension_guessing() {
        let mut legacy = base(); legacy["extensionsUsed"] = json!(["VRM"]);
        legacy["extensions"] = json!({"VRM":{"specVersion":"0.0","humanoid":{"humanBones":[{"bone":"head","node":0}]},"blendShapeMaster":{"blendShapeGroups":[{"presetName":"a","binds":[{"mesh":0,"index":0,"weight":100}]},{"presetName":"blink","binds":[]}]}}});
        let info = inspect_bytes(&glb(legacy.clone(), None)).unwrap();
        assert_eq!(info.format, "vrm0"); assert_eq!(info.bones, ["head"]); assert_eq!(info.vowels, ["aa"]);
        legacy["extensions"]["VRM"]["specVersion"] = json!("0.9"); assert!(inspect_bytes(&glb(legacy, None)).is_err());
        for version in ["1.0", "1.0-beta"] {
            let mut modern = base(); modern["extensionsUsed"] = json!(["VRMC_vrm"]);
            modern["extensions"] = json!({"VRMC_vrm":{"specVersion":version,"humanoid":{"humanBones":{"head":{"node":0}}},"expressions":{"preset":{"aa":{"morphTargetBinds":[{"node":0,"index":0,"weight":1.0}]}},"custom":{"own":{}}}}});
            let info = inspect_bytes(&glb(modern, None)).unwrap();
            assert_eq!(info.format, "vrm1"); assert_eq!(info.version, version); assert_eq!(info.vowels, ["aa"]); assert!(info.expressions.contains(&"own".to_owned()));
        }
    }
    #[test]
    fn rejects_old_future_truncated_and_mismatched_containers_before_import() {
        let original = glb(base(), None);
        let mut old = original.clone(); old[4..8].copy_from_slice(&1u32.to_le_bytes()); assert!(inspect_bytes(&old).unwrap_err().contains("GLB-Version 1"));
        let mut future = base(); future["asset"]["minVersion"] = json!("3.0"); assert!(inspect_bytes(&glb(future, None)).is_err());
        assert!(inspect_bytes(&original[..original.len()-1]).is_err());
        let mut short_chunk = original.clone(); short_chunk[12..16].copy_from_slice(&((original.len() as u32)+4).to_le_bytes()); assert!(inspect_bytes(&short_chunk).is_err());
        let mut invalid = original; invalid[0] = b'?'; assert!(inspect_bytes(&invalid).is_err());
        assert!(inspect_bytes(b"{}").is_err());
    }
    #[test]
    fn legacy_unaligned_json_is_read_automatically_with_all_length_checks_preserved() {
        let mut document = base(); document["extras"] = json!({"exporter":"legacy"});
        let mut json = serde_json::to_vec(&document).unwrap();
        if json.len() % 4 == 0 { json.push(b' '); }
        let mut bytes = b"glTF".to_vec(); bytes.extend(2u32.to_le_bytes()); bytes.extend(((20+json.len()) as u32).to_le_bytes()); bytes.extend((json.len() as u32).to_le_bytes()); bytes.extend(JSON.to_le_bytes()); bytes.extend(json);
        let info = inspect_bytes(&bytes).unwrap(); assert_eq!(info.format, "glb2"); assert!(info.warnings.contains(&LEGACY_JSON_PADDING.to_owned()));
        assert!(inspect_bytes(&bytes[..bytes.len()-1]).is_err());
    }
    #[test]
    fn json_budget_is_enforced_before_reading_or_allocating_the_declared_block() {
        struct HeaderOnly { inner: Cursor<Vec<u8>>, read_bytes: usize }
        impl Read for HeaderOnly { fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> { let count = self.inner.read(output)?; self.read_bytes += count; Ok(count) } }
        impl Seek for HeaderOnly { fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> { self.inner.seek(position) } }
        let length = 20 + MAX_JSON_BYTES + 4;
        let mut bytes = b"glTF".to_vec(); bytes.extend(2u32.to_le_bytes()); bytes.extend((length as u32).to_le_bytes()); bytes.extend(((MAX_JSON_BYTES+4) as u32).to_le_bytes()); bytes.extend(JSON.to_le_bytes());
        let mut reader = HeaderOnly { inner: Cursor::new(bytes), read_bytes: 0 };
        assert!(inspect(&mut reader, length).unwrap_err().contains("8 MiB")); assert_eq!(reader.read_bytes, 20);
    }
    #[test]
    fn embedded_buffers_and_views_are_checked_without_loading_their_contents() {
        let mut document = base(); document["buffers"] = json!([{"byteLength":5}]); document["bufferViews"] = json!([{"buffer":0,"byteOffset":1,"byteLength":4}]);
        assert!(inspect_bytes(&glb(document.clone(), Some(&[0;5]))).is_ok());
        assert!(inspect_bytes(&glb(document.clone(), None)).is_err());
        document["bufferViews"][0]["byteLength"] = json!(99); assert!(inspect_bytes(&glb(document, Some(&[0;5]))).is_err());
        let mut images = base(); images["images"] = json!([{"uri":"data:image/png;base64,AAAA"}]); assert!(inspect_bytes(&glb(images, None)).is_ok());
        let mut data = base(); data["buffers"] = json!([{"uri":"data:application/octet-stream;base64,AQID","byteLength":3}]); assert!(inspect_bytes(&glb(data.clone(), None)).is_ok());
        data["buffers"][0]["byteLength"] = json!(99); assert!(inspect_bytes(&glb(data, None)).is_err());
    }
    #[test]
    fn external_urls_files_and_relative_resources_are_rejected_without_fetching_anything() {
        for uri in ["https://example.invalid/texture.png", "file:///private/image.png", "../texture.png", "textures/a.png"] {
            let mut document = base(); document["images"] = json!([{"uri":uri}]);
            let error = inspect_bytes(&glb(document, None)).unwrap_err(); assert!(error.contains("externe Ressourcen")); assert!(!error.contains(uri));
        }
    }
    #[test]
    fn arbitrary_extra_metadata_is_not_mistaken_for_loaded_resources_or_extensions() {
        let mut document = base(); document["extras"] = json!({"uri":"urn:asset:example","extensions":{"VRMC_vrm":{"specVersion":"9.9"},"KHR_audio":{"uri":"https://example.invalid/not-a-resource"}}});
        let info = inspect_bytes(&glb(document, None)).unwrap(); assert!(info.warnings.is_empty()); assert!(info.audio_extensions.is_empty()); assert_eq!(info.format, "glb2");
    }
    #[test]
    fn unknown_required_and_unconfigured_decoders_fail_and_optional_features_are_visible() {
        for extension in ["KHR_animation_pointer", "KHR_materials_variants", "VENDOR_future", "KHR_draco_mesh_compression", "KHR_texture_basisu", "EXT_meshopt_compression", "KHR_meshopt_compression"] {
            let mut document = base(); document["extensionsUsed"] = json!([extension]); document["extensionsRequired"] = json!([extension]);
            assert!(inspect_bytes(&glb(document, None)).unwrap_err().contains(extension));
        }
        let mut optional = base(); optional["extensionsUsed"] = json!(["VENDOR_future"]);
        assert!(inspect_bytes(&glb(optional, None)).unwrap().warnings[0].contains("VENDOR_future"));
        let mut standard = base(); standard["extensionsUsed"] = json!(["KHR_materials_unlit"]); standard["extensionsRequired"] = json!(["KHR_materials_unlit"]);
        assert!(inspect_bytes(&glb(standard, None)).is_ok());
    }
    #[test]
    fn audio_extensions_are_never_silently_claimed_as_playable() {
        for name in ["KHR_audio", "MSFT_audio_emitter"] {
            let mut document = base(); document["extensionsUsed"] = json!([name]);
            let info = inspect_bytes(&glb(document.clone(), None)).unwrap(); assert_eq!(info.audio_extensions, [name]); assert!(info.warnings[0].contains("nicht abgespielt"));
            document["extensionsRequired"] = json!([name]); assert!(inspect_bytes(&glb(document, None)).unwrap_err().contains("Audio-Erweiterung"));
        }
    }
    #[test]
    fn animation_only_vrma_and_future_nested_vrm_versions_are_rejected_explicitly() {
        let mut animation = base(); animation["extensions"] = json!({"VRMC_vrm_animation":{"specVersion":"1.0"}});
        assert!(inspect_bytes(&glb(animation, None)).unwrap_err().contains("VRMA-Animationsdatei"));
        for (array, extension) in [("materials", "VRMC_materials_mtoon"), ("nodes", "VRMC_node_constraint")] {
            let mut document = base(); document[array] = json!([{"extensions":{extension:{"specVersion":"2.0"}}}]);
            assert!(inspect_bytes(&glb(document, None)).unwrap_err().contains(extension));
        }
        let mut collider = base(); collider["extensions"] = json!({"VRMC_springBone":{"specVersion":"1.0","colliders":[{"extensions":{"VRMC_springBone_extended_collider":{"specVersion":"1.0-beta"}}}]}});
        assert!(inspect_bytes(&glb(collider, None)).unwrap_err().contains("VRMC_springBone_extended_collider"));
    }
    #[test]
    fn importing_invalid_files_leaves_no_partial_avatar_and_safe_names_cannot_escape() {
        let fixture = Fixture::new(); let source = fixture.0.join("source.glb"); fs::write(&source, b"invalid").unwrap();
        let name = format!("{}.glb", uuid::Uuid::new_v4()); let destination = fixture.0.join("avatars").join(&name);
        assert!(copy_validated(&source, &destination).is_err()); assert!(!destination.exists());
        fs::write(&source, glb(base(), None)).unwrap(); copy_validated(&source, &destination).unwrap();
        assert_eq!(for_asset(&fixture.0, Some(&name)).unwrap().format, "glb2");
        for path in ["../../source.glb", "/tmp/source.glb", "source.glb", "builtin:unknown", "https://example.invalid/avatar.glb"] { assert!(for_asset(&fixture.0, Some(path)).is_err()); }
        let mut old = base(); old["images"] = json!([{"uri":"https://example.invalid/legacy-texture.png"}]); fs::write(&destination, glb(old, None)).unwrap();
        // The same guarded resolver is used by metadata IPC and /assets serving.
        assert!(for_asset(&fixture.0, Some(&name)).unwrap_err().contains("externe Ressourcen"));
    }
    #[test]
    fn direct_and_profile_embedded_imports_both_validate_model_contents() {
        use base64::Engine;
        let fixture = Fixture::new(); let store = crate::storage::Store::new(fixture.0.clone()).unwrap();
        let source = fixture.0.join("source.glb"); fs::write(&source, b"not a glb file").unwrap();
        assert!(store.import_avatar(&source).is_err()); assert_eq!(fs::read_dir(fixture.0.join("avatars")).unwrap().count(), 0);
        fs::write(&source, glb(base(), None)).unwrap();
        let name = store.import_avatar(&source).unwrap(); assert_eq!(for_asset(&fixture.0, Some(&name)).unwrap().format, "glb2");
        let bundle_path = fixture.0.join("profile.json"); store.export(&bundle_path).unwrap();
        let mut bundle: Value = serde_json::from_slice(&fs::read(&bundle_path).unwrap()).unwrap();
        bundle["avatar"] = json!({"name":"model.glb","base64":base64::engine::general_purpose::STANDARD.encode(b"bad embedded avatar")});
        fs::write(&bundle_path, serde_json::to_vec(&bundle).unwrap()).unwrap();
        assert!(store.import(&bundle_path).unwrap_err().contains("3D-Datei"));
        assert_eq!(fs::read_dir(fixture.0.join("avatars")).unwrap().count(), 1);
    }
    #[cfg(unix)]
    #[test]
    fn stored_avatar_symlinks_are_not_followed() {
        let fixture = Fixture::new(); let source = fixture.0.join("source.glb"); fs::write(&source, glb(base(), None)).unwrap();
        let name = format!("{}.glb", uuid::Uuid::new_v4()); std::os::unix::fs::symlink(source, fixture.0.join("avatars").join(&name)).unwrap();
        assert!(for_asset(&fixture.0, Some(&name)).is_err());
    }
    #[test]
    fn generated_catalog_matches_all_sixteen_binary_assets_and_procedural_nova() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../public");
        let catalog: Value = serde_json::from_str(include_str!("../../public/character-capabilities.json")).unwrap();
        assert_eq!(catalog["characters"].as_object().unwrap().len(), 17);
        assert_eq!(for_asset(Path::new("unused"), None).unwrap().format, "builtin");
        let files = [
            ("builtin:robot-expressive", "RobotExpressive.glb"), ("builtin:fox", "Fox.glb"),
            ("builtin:kaykit-knight", "KayKitKnight.glb"), ("builtin:kaykit-mage", "KayKitMage.glb"),
            ("builtin:kaykit-rogue", "KayKitRogue.glb"), ("builtin:kaykit-skeleton", "KayKitSkeleton.glb"),
            ("builtin:osa-mushy", "osa/Mushy.vrm"), ("builtin:osa-dracula", "osa/Dracula.vrm"),
            ("builtin:osa-always-watching", "osa/AlwaysWatching.vrm"), ("builtin:osa-conehead-being", "osa/ConeheadBeing.vrm"),
            ("builtin:osa-cool-alien", "osa/CoolAlien.vrm"), ("builtin:osa-cool-tiger", "osa/CoolTiger.vrm"),
            ("builtin:osa-ghost-pumpking", "osa/GhostPumpking.vrm"), ("builtin:osa-max-hax", "osa/MaxHax.vrm"),
            ("builtin:osa-megan-the-fox", "osa/MeganTheFox.vrm"), ("builtin:osa-paco", "osa/Paco.vrm"),
        ];
        let mut clips = 0;
        for (id, file) in files {
            let info = inspect_file(&root.join("characters").join(file)).unwrap_or_else(|error| panic!("{file}: {error}"));
            clips += info.animations.len(); assert!(info.audio_extensions.is_empty()); assert!(info.external_resources.is_empty());
            let expected: AvatarInfo = serde_json::from_value(catalog["characters"][id].clone()).unwrap();
            assert_eq!(info, expected, "Regenerate character-capabilities.json: {file}");
        }
        assert_eq!(clips, 340);
    }
}
