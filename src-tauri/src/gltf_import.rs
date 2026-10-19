//! Offline glTF 2 JSON import. Resolve only resources below the explicitly
//! selected file's directory and publish one validated, self-contained GLB.
//! Geometry, skins, morph targets, materials and animation objects are retained.
//! This is container/resource validation, not a claim of full image decoding.
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::{fs::{self, File, Metadata, OpenOptions}, io::{Read, Write}, path::{Component, Path, PathBuf}};

const MAX_SOURCE_JSON: usize = 32 * 1024 * 1024;
const MAX_JSON: usize = 8 * 1024 * 1024;
const MAX_TOTAL: usize = crate::avatar_metadata::MAX_FILE_BYTES as usize;
const MAX_IMAGE: usize = 64 * 1024 * 1024;
const MAX_RESOURCES: usize = 1024;
const JSON_CHUNK: u32 = 0x4e4f534a;
const BIN_CHUNK: u32 = 0x004e4942;

fn invalid(detail: &str) -> String { format!("glTF-Import: {detail}") }
fn aligned(length: usize) -> Result<usize, String> { length.checked_add(3).map(|value| value & !3).ok_or_else(|| invalid("Ressourcengröße ist ungültig.")) }
fn within_total(length: usize) -> Result<(), String> { if length <= MAX_TOTAL { Ok(()) } else { Err(invalid("Das vollständige Modell überschreitet 256 MiB.")) } }

fn array<'a>(document: &'a Value, key: &str) -> Result<&'a [Value], String> {
    match document.get(key) { None => Ok(&[]), Some(value) => value.as_array().map(Vec::as_slice).ok_or_else(|| invalid("Puffer, bufferViews und Bilder müssen Arrays sein.")) }
}

fn relative_uri(uri: &str) -> Result<PathBuf, String> {
    if uri.is_empty() || uri.len() > 8192 { return Err(invalid("Ein Ressourcenpfad ist leer oder zu lang.")); }
    let mut decoded = Vec::with_capacity(uri.len());
    let mut position = 0;
    while position < uri.len() {
        let byte = uri.as_bytes()[position];
        if byte == b'%' {
            let hex = |byte: u8| (byte as char).to_digit(16).map(|value| value as u8);
            let high = uri.as_bytes().get(position + 1).and_then(|byte| hex(*byte));
            let low = uri.as_bytes().get(position + 2).and_then(|byte| hex(*byte));
            decoded.push(high.zip(low).map(|(high, low)| high * 16 + low).ok_or_else(|| invalid("Ein Ressourcenpfad enthält ungültige Prozentkodierung."))?);
            position += 3;
        } else { decoded.push(byte); position += 1; }
    }
    let decoded = String::from_utf8(decoded).map_err(|_| invalid("Ressourcenpfade müssen UTF-8 verwenden."))?;
    // Decode before checking separators, queries, drive prefixes and traversal.
    // No URL parser, browser or second percent decoder ever sees this path.
    if decoded.chars().any(|character| character.is_control() || matches!(character, '\\' | ':' | '?' | '#')) || decoded.starts_with('/') {
        return Err(invalid("Nur relative Ressourcenpfade ohne URL, Laufwerk, Query oder Fragment sind erlaubt."));
    }
    let path = PathBuf::from(decoded);
    if path.as_os_str().is_empty() || path.components().any(|part| !matches!(part, Component::Normal(_) | Component::CurDir)) {
        return Err(invalid("Ressourcen dürfen den ausgewählten Modellordner nicht verlassen (kein '..')."));
    }
    Ok(path)
}

fn redirected(metadata: &Metadata) -> bool {
    #[cfg(target_os = "windows")]
    { use std::os::windows::fs::MetadataExt; metadata.file_attributes() & 0x400 != 0 }
    #[cfg(not(target_os = "windows"))]
    { metadata.file_type().is_symlink() }
}

fn checked_path(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    if relative.is_absolute() || relative.components().any(|part| !matches!(part, Component::Normal(_) | Component::CurDir)) { return Err(invalid("Ungültiger relativer Ressourcenpfad.")); }
    let mut path = root.to_path_buf();
    for component in relative.components() {
        if component == Component::CurDir { continue; }
        path.push(component);
        let metadata = fs::symlink_metadata(&path).map_err(|_| invalid("Eine benötigte Ressource fehlt oder ist nicht lesbar."))?;
        if redirected(&metadata) { return Err(invalid("Symlinks und umgeleitete Ressourcen sind nicht erlaubt.")); }
    }
    let actual = fs::canonicalize(&path).map_err(|_| invalid("Ein Ressourcenpfad konnte nicht geprüft werden."))?;
    if !actual.starts_with(root) { return Err(invalid("Eine Ressource liegt außerhalb des ausgewählten Modellordners.")); }
    let leaf = fs::symlink_metadata(&actual).map_err(|_| invalid("Eine Ressource konnte nicht geprüft werden."))?;
    if !leaf.is_file() || redirected(&leaf) { return Err(invalid("Ressourcen müssen reguläre Dateien sein; FIFOs, Geräte und Verzeichnisse sind nicht erlaubt.")); }
    Ok(actual)
}

fn resource_open_options() -> OpenOptions {
    let mut options = OpenOptions::new(); options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // A source can be replaced after the metadata check. Never wait on a
        // substituted FIFO, and never follow a substituted leaf symlink.
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
    }
    options
}

// Query the *opened handle* before reading. Canonicalizing just the original
// filename would permit a directory/symlink swap between validation and open.
#[cfg(target_os = "macos")]
fn handle_path(file: &File) -> Result<PathBuf, String> {
    use std::{ffi::CStr, os::{fd::AsRawFd, unix::ffi::OsStrExt}};
    let mut bytes = vec![0u8; libc::PATH_MAX as usize];
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETPATH, bytes.as_mut_ptr()) } != 0 { return Err(invalid("Der geöffnete Ressourcenpfad konnte nicht geprüft werden.")); }
    let path = unsafe { CStr::from_ptr(bytes.as_ptr().cast()) };
    Ok(PathBuf::from(std::ffi::OsStr::from_bytes(path.to_bytes())))
}
#[cfg(target_os = "linux")]
fn handle_path(file: &File) -> Result<PathBuf, String> {
    use std::os::fd::AsRawFd;
    fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd())).map_err(|_| invalid("Der geöffnete Ressourcenpfad konnte nicht geprüft werden (/proc erforderlich)."))
}
#[cfg(target_os = "windows")]
fn handle_path(file: &File) -> Result<PathBuf, String> {
    use std::{ffi::{c_void, OsString}, os::windows::{ffi::OsStringExt, io::AsRawHandle}};
    #[link(name = "kernel32")]
    unsafe extern "system" { fn GetFinalPathNameByHandleW(file: *mut c_void, path: *mut u16, length: u32, flags: u32) -> u32; }
    let mut path = vec![0u16; 32768];
    let length = unsafe { GetFinalPathNameByHandleW(file.as_raw_handle(), path.as_mut_ptr(), path.len() as u32, 0) } as usize;
    if length == 0 || length >= path.len() { return Err(invalid("Der geöffnete Ressourcenpfad konnte nicht geprüft werden.")); }
    Ok(PathBuf::from(OsString::from_wide(&path[..length])))
}
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn handle_path(_: &File) -> Result<PathBuf, String> { Err(invalid("Ressourcenimport wird auf diesem Betriebssystem nicht unterstützt.")) }

fn verify_handle(file: &File, root: &Path, expected: &Path) -> Result<(), String> {
    let opened = handle_path(file)?;
    if !opened.starts_with(root) { return Err(invalid("Eine geöffnete Ressource liegt außerhalb des ausgewählten Modellordners.")); }
    let path = fs::canonicalize(opened).map_err(|_| invalid("Eine Ressource wurde während des Imports geändert."))?;
    if !path.starts_with(root) || path != expected { return Err(invalid("Eine geöffnete Ressource liegt außerhalb des ausgewählten Modellordners oder wurde umgeleitet.")); }
    Ok(())
}

fn read_local(root: &Path, relative: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let expected = checked_path(root, relative)?;
    let mut file = resource_open_options().open(&expected).map_err(|_| invalid("Eine Ressource konnte nicht geöffnet werden."))?;
    let metadata = file.metadata().map_err(|_| invalid("Eine Ressourcengröße konnte nicht geprüft werden."))?;
    if !metadata.is_file() { return Err(invalid("Ressourcen müssen reguläre Dateien sein.")); }
    verify_handle(&file, root, &expected)?;
    if metadata.len() > limit as u64 { return Err(invalid("Eine Ressource überschreitet das verbleibende Größenlimit.")); }
    let length = metadata.len() as usize;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(length).map_err(|_| invalid("Für dieses Modell steht nicht genügend Arbeitsspeicher zur Verfügung."))?;
    bytes.resize(length, 0);
    file.read_exact(&mut bytes).map_err(|_| invalid("Eine Ressource wurde während des Imports verändert oder ist abgeschnitten."))?;
    if file.read(&mut [0u8; 1]).map_err(|_| invalid("Eine Ressource konnte nicht vollständig gelesen werden."))? != 0 { return Err(invalid("Eine Ressource wurde während des Imports geändert oder ist zu groß.")); }
    verify_handle(&file, root, &expected)?;
    if checked_path(root, relative)? != expected { return Err(invalid("Eine Ressource wurde während des Imports umgeleitet.")); }
    Ok(bytes)
}

fn data_uri(uri: &str, allowed: &[&str], limit: usize) -> Result<(String, Vec<u8>), String> {
    let (header, payload) = uri.split_once(',').ok_or_else(|| invalid("Eine Daten-URI ist unvollständig."))?;
    let mime = header.strip_prefix("data:").and_then(|value| value.strip_suffix(";base64")).filter(|mime| allowed.contains(mime)).ok_or_else(|| invalid("Daten-URIs benötigen einen unterstützten MIME-Typ und Base64-Kodierung."))?;
    let padding = payload.bytes().rev().take_while(|byte| *byte == b'=').count();
    let size = (payload.len() / 4).checked_mul(3).and_then(|size| size.checked_sub(padding)).ok_or_else(|| invalid("Ungültige Base64-Größe."))?;
    if payload.len() % 4 != 0 || padding > 2 || size > limit { return Err(invalid("Eingebettete Ressourcen sind ungültig oder überschreiten das Größenlimit.")); }
    let bytes = STANDARD.decode(payload).map_err(|_| invalid("Eine eingebettete Ressource enthält ungültiges Base64."))?;
    if bytes.len() != size || bytes.len() > limit { return Err(invalid("Eine eingebettete Ressource überschreitet das Größenlimit.")); }
    Ok((mime.into(), bytes))
}

fn image_type(bytes: &[u8]) -> Result<&'static str, String> {
    if bytes.starts_with(b"\xabKTX 20\xbb\r\n\x1a\n") { crate::avatar_metadata::validate_ktx2(bytes)?; return Ok("image/ktx2"); }
    let (mime, width, height) = if bytes.len() >= 45 && bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes[8..12] == [0, 0, 0, 13] && &bytes[12..16] == b"IHDR" && &bytes[bytes.len()-8..bytes.len()-4] == b"IEND" {
        ("image/png", u32::from_be_bytes(bytes[16..20].try_into().unwrap()), u32::from_be_bytes(bytes[20..24].try_into().unwrap()))
    } else if bytes.len() >= 14 && bytes.starts_with(&[0xff, 0xd8, 0xff]) && bytes.ends_with(&[0xff, 0xd9]) {
        let mut position = 2;
        let mut dimensions = None;
        while position + 1 < bytes.len() {
            if bytes[position] != 0xff { break; }
            while bytes.get(position) == Some(&0xff) { position += 1; }
            let Some(&marker) = bytes.get(position) else { break; }; position += 1;
            if matches!(marker, 0xda | 0xd9 | 0x00) { break; }
            if marker == 0x01 || (0xd0..=0xd8).contains(&marker) { continue; }
            let Some(header) = bytes.get(position..position+2) else { break; };
            let length = u16::from_be_bytes(header.try_into().unwrap()) as usize;
            if length < 2 || position.checked_add(length).is_none_or(|end| end > bytes.len()) { break; }
            if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) && length >= 8 {
                dimensions = Some((u16::from_be_bytes(bytes[position+5..position+7].try_into().unwrap()) as u32, u16::from_be_bytes(bytes[position+3..position+5].try_into().unwrap()) as u32)); break;
            }
            position += length;
        }
        let (width, height) = dimensions.ok_or_else(|| invalid("JPEG-Textur hat keinen gültigen Größenheader."))?;
        ("image/jpeg", width, height)
    } else { return Err(invalid("Texturinhalt ist kein unterstütztes PNG/JPEG/KTX2 oder sein Dateikopf ist unvollständig.")); };
    if width == 0 || height == 0 || width > 16384 || height > 16384 || u64::from(width) * u64::from(height) > 64 * 1024 * 1024 { return Err(invalid("Texturen benötigen gültige Abmessungen bis 16384 Pixel Kantenlänge und 64 Megapixel.")); }
    Ok(mime)
}

fn append(binary: &mut Vec<u8>, bytes: &[u8]) -> Result<usize, String> {
    let offset = aligned(binary.len())?;
    let length = offset.checked_add(bytes.len()).ok_or_else(|| invalid("Ungültige Puffergröße."))?;
    within_total(length)?;
    if binary.capacity() < length {
        let capacity = length.next_power_of_two().min(MAX_TOTAL);
        binary.try_reserve_exact(capacity - binary.len()).map_err(|_| invalid("Für dieses Modell steht nicht genügend Arbeitsspeicher zur Verfügung."))?;
    }
    binary.resize(offset, 0);
    binary.extend_from_slice(bytes);
    Ok(offset)
}

fn pack(source: &Path) -> Result<(Vec<u8>, Vec<u8>), String> {
    let root = fs::canonicalize(source.parent().ok_or_else(|| invalid("Der Modellordner fehlt."))?).map_err(|_| invalid("Der ausgewählte Modellordner konnte nicht geöffnet werden."))?;
    let filename = Path::new(source.file_name().ok_or_else(|| invalid("Die ausgewählte Modelldatei fehlt."))?);
    let bytes = read_local(&root, filename, MAX_SOURCE_JSON)?;
    let mut document: Value = serde_json::from_slice(&bytes).map_err(|_| invalid("Die ausgewählte Datei enthält kein gültiges glTF-JSON (höchstens 32 MiB)."))?;
    drop(bytes);
    if !document.is_object() || document["asset"]["version"] != "2.0" || document["asset"]["minVersion"].as_str().is_some_and(|version| version != "2.0") { return Err(invalid("Unterstützt wird glTF 2.0 JSON.")); }
    if document["extensions"].get("VRMC_vrm_animation").is_some() { return Err(invalid("VRMA enthält Animationen und ist kein importierbares Charaktermodell.")); }
    let buffers = array(&document, "buffers")?;
    let images = array(&document, "images")?;
    if buffers.len().checked_add(images.len()).is_none_or(|count| count > MAX_RESOURCES) { return Err(invalid("Ein Modell darf höchstens 1024 Puffer und Texturen enthalten.")); }
    let mut binary = Vec::new();
    let mut ranges = Vec::with_capacity(buffers.len());
    let mut virtual_indices = std::collections::BTreeMap::new();
    let mut virtual_buffers = Vec::new();
    for (index,buffer) in buffers.iter().enumerate() {
        if crate::avatar_metadata::is_virtual_buffer(&document,index) {
            let length = buffer["byteLength"].as_u64().filter(|size| *size > 0 && *size <= MAX_TOTAL as u64).ok_or_else(||invalid("Ungültiger Meshopt-Fallback-Puffer."))? as usize;
            virtual_indices.insert(index,virtual_buffers.len()+1); virtual_buffers.push(buffer.clone()); ranges.push((0,length)); continue;
        }
        if !buffer.is_object() || buffer.get("extensions").is_some_and(|value| value.as_object().is_none_or(|extensions| !extensions.is_empty())) { return Err(invalid("Puffererweiterungen können nicht sicher zusammengeführt werden. Bitte ohne Puffererweiterungen exportieren.")); }
        let length = buffer["byteLength"].as_u64().filter(|length| *length > 0 && *length <= MAX_TOTAL as u64).ok_or_else(|| invalid("Ein Puffer hat eine ungültige byteLength."))? as usize;
        let remaining = MAX_TOTAL.checked_sub(aligned(binary.len())?).ok_or_else(|| invalid("Das Modell überschreitet 256 MiB."))?;
        if length > remaining { return Err(invalid("Das Modell überschreitet 256 MiB.")); }
        let uri = buffer["uri"].as_str().ok_or_else(|| invalid("JSON-glTF-Puffer benötigen eine relative URI oder Base64-Daten-URI."))?;
        let data = if uri.starts_with("data:") { data_uri(uri, &["application/octet-stream", "application/gltf-buffer"], length)?.1 } else {
            let relative = relative_uri(uri)?;
            if !relative.extension().and_then(|value| value.to_str()).is_some_and(|extension| extension.eq_ignore_ascii_case("bin")) { return Err(invalid("Externe glTF-Puffer müssen .bin-Dateien sein.")); }
            read_local(&root, &relative, length)?
        };
        if data.len() != length { return Err(invalid("Die tatsächliche Puffergröße stimmt nicht mit byteLength überein.")); }
        let offset = append(&mut binary, &data)?;
        ranges.push((offset, length));
    }
    let mut views = array(&document, "bufferViews")?.to_vec();
    for view in &mut views {
        if !view.is_object() { return Err(invalid("Ein bufferView ist ungültig.")); }
        let source_index = view["buffer"].as_u64().and_then(|index|usize::try_from(index).ok()).ok_or_else(||invalid("Ungültiger Pufferindex."))?;
        let (base, buffer_length) = ranges.get(source_index).ok_or_else(|| invalid("Ein bufferView verweist auf einen unbekannten Puffer."))?;
        let offset = match view.get("byteOffset") { Some(value) => value.as_u64().ok_or_else(|| invalid("Ein bufferView hat einen ungültigen Offset."))?, None => 0 };
        let length = view["byteLength"].as_u64().filter(|length| *length > 0).ok_or_else(|| invalid("Ein bufferView hat eine ungültige Länge."))?;
        if offset.checked_add(length).is_none_or(|end| end > *buffer_length as u64) { return Err(invalid("Ein bufferView liegt außerhalb seines Quellpuffers.")); }
        view["buffer"] = json!(virtual_indices.get(&source_index).copied().unwrap_or(0));
        view["byteOffset"] = json!(*base as u64 + offset);
        for name in ["EXT_meshopt_compression","KHR_meshopt_compression"] {
            if let Some(ext) = view["extensions"].get_mut(name) {
                let index = ext["buffer"].as_u64().and_then(|index|usize::try_from(index).ok()).ok_or_else(||invalid("Ungültiger Meshopt-Quellpuffer."))?;
                if virtual_indices.contains_key(&index) { return Err(invalid("Meshopt-Daten dürfen keinen leeren Fallback-Puffer verwenden.")); }
                let (base,size) = ranges.get(index).ok_or_else(||invalid("Unbekannter Meshopt-Quellpuffer."))?;
                let offset = match ext.get("byteOffset") { None=>0,Some(value)=>value.as_u64().ok_or_else(||invalid("Ungültiger Meshopt-Offset."))? };
                let length = ext["byteLength"].as_u64().filter(|length|*length>0).ok_or_else(||invalid("Ungültige Meshopt-Länge."))?;
                if offset.checked_add(length).is_none_or(|end|end>*size as u64) { return Err(invalid("Meshopt-Daten liegen außerhalb des Quellpuffers.")); }
                ext["buffer"] = json!(0); ext["byteOffset"] = json!(*base as u64+offset);
            }
        }
    }
    let source_view_count = views.len();
    let mut images = images.to_vec();
    for image in &mut images {
        if !image.is_object() || image.get("uri").is_some() == image.get("bufferView").is_some() { return Err(invalid("Ein Bild benötigt entweder uri oder bufferView.")); }
        let declared = image.get("mimeType").map(|value| value.as_str().ok_or_else(|| invalid("Ein Bild-MIME-Typ ist ungültig."))).transpose()?;
        if declared.is_some_and(|mime| !["image/png", "image/jpeg", "image/ktx2"].contains(&mime)) { return Err(invalid("Der glTF-Import unterstützt PNG-, JPEG- und BasisU-KTX2-Texturen.")); }
        let (view_index, mime) = if let Some(uri) = image.get("uri") {
            let uri = uri.as_str().ok_or_else(|| invalid("Eine Bild-URI ist ungültig."))?;
            let remaining = MAX_TOTAL.checked_sub(aligned(binary.len())?).unwrap_or(0).min(MAX_IMAGE);
            let (data_mime, data) = if uri.starts_with("data:") { let (mime, data) = data_uri(uri, &["image/png", "image/jpeg", "image/ktx2"], remaining)?; (Some(mime), data) } else {
                let relative = relative_uri(uri)?;
                let extension = relative.extension().and_then(|value| value.to_str()).unwrap_or_default().to_ascii_lowercase();
                if !["png", "jpg", "jpeg", "ktx2"].contains(&extension.as_str()) { return Err(invalid("Externe Texturen müssen PNG-, JPEG- oder KTX2-Dateien sein.")); }
                (None, read_local(&root, &relative, remaining)?)
            };
            let mime = image_type(&data)?;
            if declared.is_some_and(|declared| declared != mime) || data_mime.as_deref().is_some_and(|declared| declared != mime) { return Err(invalid("Der Texturinhalt stimmt nicht mit dem angegebenen MIME-Typ überein.")); }
            let offset = append(&mut binary, &data)?;
            let index = views.len();
            views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":data.len()}));
            (index, mime)
        } else {
            let index = image["bufferView"].as_u64().and_then(|index| usize::try_from(index).ok()).filter(|index| *index < source_view_count).ok_or_else(|| invalid("Ein Bild verweist auf einen unbekannten bufferView."))?;
            let view = &views[index];
            if view["buffer"] != 0 || view["extensions"].get("EXT_meshopt_compression").is_some() || view["extensions"].get("KHR_meshopt_compression").is_some() { return Err(invalid("Bilddaten müssen in einem echten, unkomprimierten bufferView liegen.")); }
            let offset = view["byteOffset"].as_u64().unwrap_or(0) as usize;
            let length = view["byteLength"].as_u64().unwrap_or(0) as usize;
            if length > MAX_IMAGE { return Err(invalid("Eine Textur überschreitet 64 MiB.")); }
            let data = binary.get(offset..offset+length).ok_or_else(|| invalid("Ein Bild-bufferView liegt außerhalb des Puffers."))?;
            let mime = image_type(data)?;
            if declared != Some(mime) { return Err(invalid("Ein Bild-bufferView benötigt den zum Inhalt passenden MIME-Typ.")); }
            (index, mime)
        };
        image.as_object_mut().unwrap().remove("uri");
        image["bufferView"] = json!(view_index);
        image["mimeType"] = json!(mime);
    }
    let fields = document.as_object_mut().unwrap();
    if !binary.is_empty() { let mut packed_buffers = vec![json!({"byteLength":binary.len()})]; packed_buffers.extend(virtual_buffers); fields.insert("buffers".into(), json!(packed_buffers)); }
    else { fields.remove("buffers"); }
    if !views.is_empty() { fields.insert("bufferViews".into(), json!(views)); }
    if !images.is_empty() { fields.insert("images".into(), json!(images)); }
    struct JsonWriter(Vec<u8>);
    impl Write for JsonWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let size = self.0.len().checked_add(bytes.len()).filter(|size| *size <= MAX_JSON).ok_or_else(|| std::io::Error::other("GLB JSON limit"))?;
            if self.0.capacity() < size { self.0.try_reserve_exact(size.next_power_of_two().min(MAX_JSON) - self.0.len()).map_err(std::io::Error::other)?; }
            self.0.extend_from_slice(bytes); Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
    }
    let mut writer = JsonWriter(Vec::new());
    serde_json::to_writer(&mut writer, &document).map_err(|_| invalid("Die fertigen GLB-Metadaten überschreiten 8 MiB oder konnten nicht geschrieben werden."))?;
    let mut json = writer.0;
    let json_length = aligned(json.len())?;
    if json_length > MAX_JSON { return Err(invalid("Die fertigen GLB-Metadaten überschreiten 8 MiB.")); }
    let binary_length = aligned(binary.len())?;
    let total = 20usize.checked_add(json_length).and_then(|size| size.checked_add(if binary.is_empty() { 0 } else { 8 + binary_length })).ok_or_else(|| invalid("Die fertige GLB-Größe ist ungültig."))?;
    within_total(total)?;
    json.resize(json_length, b' ');
    binary.resize(binary_length, 0);
    Ok((json, binary))
}

struct Temporary(PathBuf);
impl Drop for Temporary { fn drop(&mut self) { let _ = fs::remove_file(&self.0); } }

pub fn import(source: &Path, destination: &Path) -> Result<(), String> {
    let (json, binary) = pack(source)?;
    let parent = destination.parent().ok_or_else(|| invalid("Der Avatar-Zielordner fehlt."))?;
    let metadata = fs::symlink_metadata(parent).map_err(|_| invalid("Der Avatar-Zielordner fehlt."))?;
    if !metadata.is_dir() || redirected(&metadata) { return Err(invalid("Der Avatar-Zielordner darf keine umgeleitete Datei sein.")); }
    let temporary_path = parent.join(format!(".gltf-import-{}.tmp", uuid::Uuid::new_v4()));
    let mut output = OpenOptions::new().write(true).create_new(true).open(&temporary_path).map_err(|_| invalid("Die temporäre GLB-Datei konnte nicht angelegt werden."))?;
    let temporary = Temporary(temporary_path);
    let total = 20 + json.len() + if binary.is_empty() { 0 } else { 8 + binary.len() };
    let write = (|| -> std::io::Result<()> {
        output.write_all(b"glTF")?; output.write_all(&2u32.to_le_bytes())?; output.write_all(&(total as u32).to_le_bytes())?;
        output.write_all(&(json.len() as u32).to_le_bytes())?; output.write_all(&JSON_CHUNK.to_le_bytes())?; output.write_all(&json)?;
        if !binary.is_empty() { output.write_all(&(binary.len() as u32).to_le_bytes())?; output.write_all(&BIN_CHUNK.to_le_bytes())?; output.write_all(&binary)?; }
        output.sync_all()
    })();
    drop(output);
    write.map_err(|_| invalid("Die GLB-Datei konnte nicht vollständig geschrieben werden."))?;
    crate::avatar_metadata::inspect_file(&temporary.0)?;
    // Same-directory hard link publishes the complete, validated inode in one
    // no-clobber operation. Existing avatars are never overwritten. Filesystems
    // without atomic hard links fail explicitly rather than expose partial data.
    fs::hard_link(&temporary.0, destination).map_err(|_| invalid("Die validierte GLB-Datei konnte nicht atomar importiert werden. Die Zieldatei darf nicht existieren; das Dateisystem muss Hardlinks unterstützen."))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture { root: PathBuf }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("cohost-gltf-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(root.join("source/textures")).unwrap();
            fs::create_dir_all(root.join("store/avatars")).unwrap();
            Self { root: fs::canonicalize(root).unwrap() }
        }
        fn source(&self, document: &Value) -> PathBuf {
            let path = self.root.join("source/model.gltf");
            fs::write(&path, serde_json::to_vec(document).unwrap()).unwrap(); path
        }
        fn destination(&self) -> PathBuf { self.root.join("store/avatars/model.glb") }
        fn run(&self, document: &Value) -> Result<PathBuf, String> {
            let destination = self.destination();
            import(&self.source(document), &destination)?;
            Ok(destination)
        }
        fn empty_target(&self) -> bool { fs::read_dir(self.root.join("store/avatars")).unwrap().next().is_none() }
    }
    impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.root); } }
    fn base() -> Value { json!({"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[]}],"nodes":[]}) }
    fn png() -> Vec<u8> { STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=").unwrap() }
    fn embedded(mime: &str, bytes: &[u8]) -> String { format!("data:{mime};base64,{}", STANDARD.encode(bytes)) }
    fn unpack(path: &Path) -> (Value, Vec<u8>) {
        let bytes = fs::read(path).unwrap();
        assert_eq!(&bytes[..4], b"glTF");
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize, bytes.len());
        let length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let document = serde_json::from_slice(&bytes[20..20+length]).unwrap();
        let binary = if bytes.len() > 20+length { bytes[28+length..].to_vec() } else { Vec::new() };
        (document, binary)
    }
    fn character(fixture: &Fixture) -> Value {
        let mut binary = Vec::new();
        let floats = |values: &[f32]| values.iter().flat_map(|value| value.to_le_bytes()).collect::<Vec<_>>();
        binary.extend(floats(&[0.,0.,0.,1.,0.,0.,0.,1.,0.]));
        binary.extend([0u8; 12]);
        binary.extend(floats(&[1.,0.,0.,0.,1.,0.,0.,0.,1.,0.,0.,0.]));
        binary.extend(floats(&[1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.]));
        binary.extend(floats(&[0.,1.]));
        binary.extend(floats(&[0.,0.,0.,1.,0.,0.,0.70710677,0.70710677]));
        binary.extend(floats(&[0.,0.1,0.,0.,0.1,0.,0.,0.1,0.]));
        assert_eq!(binary.len(), 236);
        fs::write(fixture.root.join("source/character.bin"), &binary).unwrap();
        fs::write(fixture.root.join("source/textures/face.png"), png()).unwrap();
        json!({
            "asset":{"version":"2.0","copyright":"Local synthetic fixture"},"scene":0,"scenes":[{"nodes":[0,1]}],
            "nodes":[{"name":"Head"},{"name":"Character","mesh":0,"skin":0}],
            "skins":[{"joints":[0],"inverseBindMatrices":3}],
            "buffers":[{"uri":"character.bin","byteLength":binary.len()}],
            "bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":12},{"buffer":0,"byteOffset":48,"byteLength":48},{"buffer":0,"byteOffset":96,"byteLength":64},{"buffer":0,"byteOffset":160,"byteLength":8},{"buffer":0,"byteOffset":168,"byteLength":32},{"buffer":0,"byteOffset":200,"byteLength":36}],
            "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]},{"bufferView":1,"componentType":5121,"count":3,"type":"VEC4"},{"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"},{"bufferView":3,"componentType":5126,"count":1,"type":"MAT4"},{"bufferView":4,"componentType":5126,"count":2,"type":"SCALAR","min":[0],"max":[1]},{"bufferView":5,"componentType":5126,"count":2,"type":"VEC4"},{"bufferView":6,"componentType":5126,"count":3,"type":"VEC3"}],
            "images":[{"uri":"textures/face.png"}],"textures":[{"source":0}],"materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0,"JOINTS_0":1,"WEIGHTS_0":2},"material":0,"targets":[{"POSITION":6}]}],"extras":{"targetNames":["viseme_aa"]}}],
            "animations":[{"name":"Wave","samplers":[{"input":4,"output":5,"interpolation":"LINEAR"}],"channels":[{"sampler":0,"target":{"node":0,"path":"rotation"}}]}]
        })
    }

    #[test]
    fn imports_textured_character_without_changing_skin_morphs_or_animation() {
        let fixture = Fixture::new(); let original = character(&fixture);
        let path = fixture.run(&original).unwrap();
        let info = crate::avatar_metadata::inspect_file(&path).unwrap();
        assert_eq!(info.format, "glb2"); assert_eq!(info.bones, ["Head"]); assert_eq!(info.vowels, ["aa"]);
        assert_eq!(info.animations[0].name, "Wave"); assert_eq!(info.animations[0].duration, Some(1.));
        assert!(info.external_resources.is_empty());
        let (packed, binary) = unpack(&path);
        for key in ["asset", "scenes", "nodes", "skins", "meshes", "accessors", "animations", "textures", "materials"] { assert_eq!(packed[key], original[key], "preserve {key}"); }
        assert!(packed["images"][0].get("uri").is_none()); assert_eq!(packed["images"][0]["mimeType"], "image/png");
        let view = &packed["bufferViews"][packed["images"][0]["bufferView"].as_u64().unwrap() as usize];
        let offset = view["byteOffset"].as_u64().unwrap() as usize;
        assert_eq!(&binary[offset..offset+png().len()], png());
        assert_eq!(&binary[..236], fs::read(fixture.root.join("source/character.bin")).unwrap());
    }

    #[test]
    fn combines_multiple_buffers_with_alignment_and_correct_view_offsets() {
        let fixture = Fixture::new(); let mut document = base();
        fs::write(fixture.root.join("source/second buffer.bin"), [4,5,6,7,8]).unwrap();
        document["buffers"] = json!([{"uri":embedded("application/gltf-buffer", &[1,2,3]),"byteLength":3},{"uri":"second%20buffer.bin","byteLength":5}]);
        document["bufferViews"] = json!([{"buffer":0,"byteOffset":1,"byteLength":2},{"buffer":1,"byteOffset":1,"byteLength":3}]);
        document["images"] = json!([{"uri":embedded("image/png", &png())}]);
        let path = fixture.run(&document).unwrap(); let (packed, binary) = unpack(&path);
        assert_eq!(packed["buffers"].as_array().unwrap().len(), 1);
        assert_eq!(packed["bufferViews"][0]["byteOffset"], 1); assert_eq!(packed["bufferViews"][1]["byteOffset"], 5);
        assert_eq!(packed["bufferViews"][2]["byteOffset"], 12);
        assert_eq!(&binary[..3], [1,2,3]); assert_eq!(&binary[4..9], [4,5,6,7,8]);
        assert!(packed["bufferViews"].as_array().unwrap().iter().all(|view| view["buffer"] == 0));
    }

    #[test]
    fn preserves_existing_embedded_image_views_and_supports_jpeg_data_uris() {
        let fixture = Fixture::new(); let mut document = base(); let image = png();
        document["buffers"] = json!([{"uri":embedded("application/octet-stream", &image),"byteLength":image.len()}]);
        document["bufferViews"] = json!([{"buffer":0,"byteLength":image.len()}]);
        document["images"] = json!([{"bufferView":0,"mimeType":"image/png"}]);
        let (packed, binary) = unpack(&fixture.run(&document).unwrap());
        assert_eq!(packed["images"][0]["bufferView"], 0); assert_eq!(&binary[..image.len()], image);
        let fixture = Fixture::new(); let mut document = base(); let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut jpeg).encode(&[20,40,60], 1, 1, image::ExtendedColorType::Rgb8).unwrap();
        document["images"] = json!([{"uri":embedded("image/jpeg", &jpeg)}]);
        let (packed, binary) = unpack(&fixture.run(&document).unwrap());
        assert_eq!(packed["images"][0]["mimeType"], "image/jpeg"); assert_eq!(&binary[..jpeg.len()], jpeg);
    }

    #[test]
    fn rejects_urls_drive_paths_queries_and_decoded_traversal_without_output() {
        for uri in ["https://example.invalid/secret.bin", "http://user:password@example.invalid/a.bin", "file:///secret.bin", "/secret.bin", "//server/share/a.bin", "C:/secret.bin", "C%3a/secret.bin", "..%2fsecret.bin", "%2e%2e/secret.bin", "a/../secret.bin", "a\\secret.bin", "a%5csecret.bin", "a.bin?token=secret", "a.bin#fragment", "a.bin%3fquery", "a.bin%23fragment", "a%00.bin", "%xx.bin"] {
            let fixture = Fixture::new(); let mut document = base();
            document["buffers"] = json!([{"uri":uri,"byteLength":1}]);
            assert!(fixture.run(&document).is_err(), "reject {uri}"); assert!(fixture.empty_target());
        }
    }

    #[test]
    fn missing_resources_bad_mime_and_bad_views_never_publish_partial_models() {
        let fixture = Fixture::new();
        for mutate in 0..7 {
            let mut document = character(&fixture);
            match mutate {
                0 => document["images"][0]["uri"] = json!("textures/missing.png"),
                1 => document["images"][0]["mimeType"] = json!("image/jpeg"),
                2 => document["images"][0]["mimeType"] = json!("image/webp"),
                3 => document["bufferViews"][0]["byteOffset"] = json!(u64::MAX),
                4 => document["buffers"][0]["byteLength"] = json!(235),
                5 => document["images"][0]["bufferView"] = json!(0),
                _ => document["images"] = json!([{"uri":embedded("image/jpeg", &png())}]),
            }
            assert!(fixture.run(&document).is_err(), "mutation {mutate}"); assert!(fixture.empty_target());
        }
        fs::write(fixture.root.join("source/textures/face.png"), b"not an image").unwrap();
        let mut document = base(); document["images"] = json!([{"uri":"textures/face.png"}]);
        assert!(fixture.run(&document).is_err()); assert!(fixture.empty_target());
    }

    #[test]
    fn checks_source_resource_and_final_metadata_limits_before_publication() {
        let fixture = Fixture::new();
        let source = fixture.root.join("source/model.gltf");
        File::create(&source).unwrap().set_len(MAX_SOURCE_JSON as u64 + 1).unwrap();
        assert!(import(&source, &fixture.destination()).unwrap_err().contains("Größenlimit")); assert!(fixture.empty_target());
        let mut document = base(); document["buffers"] = json!([{"uri":"huge.bin","byteLength":MAX_TOTAL as u64 + 1}]);
        assert!(fixture.run(&document).unwrap_err().contains("byteLength")); assert!(fixture.empty_target());
        File::create(fixture.root.join("source/huge.bin")).unwrap().set_len(MAX_TOTAL as u64 + 1).unwrap();
        document["buffers"][0]["byteLength"] = json!(1);
        assert!(fixture.run(&document).unwrap_err().contains("Größenlimit")); assert!(fixture.empty_target());
        let mut document = base(); document["extras"] = json!({"tooLarge":"x".repeat(MAX_JSON + 1)});
        assert!(fixture.run(&document).unwrap_err().contains("8 MiB")); assert!(fixture.empty_target());
        let mut document = base(); document["images"] = json!(vec![json!({"uri":embedded("image/png", &png())}); MAX_RESOURCES+1]);
        assert!(fixture.run(&document).unwrap_err().contains("1024")); assert!(fixture.empty_target());
    }

    #[test]
    fn bounds_texture_bytes_dimensions_and_embedded_view_mime() {
        let fixture = Fixture::new(); let mut document = base();
        File::create(fixture.root.join("source/textures/huge.png")).unwrap().set_len(MAX_IMAGE as u64 + 1).unwrap();
        document["images"] = json!([{"uri":"textures/huge.png"}]);
        assert!(fixture.run(&document).unwrap_err().contains("Größenlimit")); assert!(fixture.empty_target());
        let mut oversized = png(); oversized[16..20].copy_from_slice(&16385u32.to_be_bytes());
        document["images"] = json!([{"uri":embedded("image/png", &oversized)}]);
        assert!(fixture.run(&document).unwrap_err().contains("Abmessungen")); assert!(fixture.empty_target());
        document["buffers"] = json!([{"uri":embedded("application/octet-stream", &png()),"byteLength":png().len()}]);
        document["bufferViews"] = json!([{"buffer":0,"byteLength":png().len()}]);
        document["images"] = json!([{"bufferView":0}]);
        assert!(fixture.run(&document).unwrap_err().contains("MIME-Typ")); assert!(fixture.empty_target());
    }

    #[test]
    fn rejects_unsupported_versions_and_malformed_embedded_resources() {
        let fixture = Fixture::new();
        for uri in ["data:application/octet-stream;base64,AAAA", "data:text/plain;base64,AQ==", "data:application/octet-stream,AQ==", "data:application/octet-stream;base64,@@=="] {
            let mut document = base(); document["buffers"] = json!([{"uri":uri,"byteLength":1}]);
            assert!(fixture.run(&document).is_err()); assert!(fixture.empty_target());
        }
        for version in ["1.0", "3.0"] {
            let mut document = base(); document["asset"]["version"] = json!(version);
            assert!(fixture.run(&document).is_err()); assert!(fixture.empty_target());
        }
        let mut document = base(); document["extensions"] = json!({"VRMC_vrm_animation":{"specVersion":"1.0"}});
        assert!(fixture.run(&document).unwrap_err().contains("VRMA"));
        let mut document = base(); document["bufferViews"] = json!([{"extensions":{"EXT_meshopt_compression":{"buffer":0}}}]);
        assert!(fixture.run(&document).is_err()); assert!(fixture.empty_target());
        let mut document = base(); document["images"] = json!([{"uri":embedded("image/png", &png())},{"bufferView":0,"mimeType":"image/png"}]);
        assert!(fixture.run(&document).unwrap_err().contains("unbekannten bufferView")); assert!(fixture.empty_target());
    }

    #[test]
    fn existing_files_survive_and_failed_final_inspection_removes_temporaries() {
        let fixture = Fixture::new(); let mut document = base();
        document["extensionsUsed"] = json!(["COHOST_unsupported_fixture"]); document["extensionsRequired"] = json!(["COHOST_unsupported_fixture"]);
        assert!(fixture.run(&document).is_err()); assert!(fixture.empty_target());
        fs::write(fixture.destination(), b"existing avatar must survive").unwrap();
        assert!(fixture.run(&base()).is_err());
        assert_eq!(fs::read(fixture.destination()).unwrap(), b"existing avatar must survive");
        assert_eq!(fs::read_dir(fixture.root.join("store/avatars")).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlink_files_directories_sources_and_open_handles_outside_the_grant() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new(); fs::create_dir(fixture.root.join("outside")).unwrap();
        fs::write(fixture.root.join("outside/secret.bin"), [42]).unwrap();
        symlink(fixture.root.join("outside/secret.bin"), fixture.root.join("source/link.bin")).unwrap();
        symlink(fixture.root.join("outside"), fixture.root.join("source/linked")).unwrap();
        for uri in ["link.bin", "linked/secret.bin"] {
            let mut document = base(); document["buffers"] = json!([{"uri":uri,"byteLength":1}]);
            assert!(fixture.run(&document).unwrap_err().contains("Symlink")); assert!(fixture.empty_target());
        }
        let source = fixture.root.join("source/source-link.gltf");
        fs::write(fixture.root.join("outside/model.gltf"), serde_json::to_vec(&base()).unwrap()).unwrap();
        symlink(fixture.root.join("outside/model.gltf"), &source).unwrap();
        assert!(import(&source, &fixture.destination()).is_err()); assert!(fixture.empty_target());
        let handle = File::open(fixture.root.join("outside/secret.bin")).unwrap();
        assert!(verify_handle(&handle, &fixture.root.join("source"), &fixture.root.join("source/secret.bin")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_fifos_before_open_and_keeps_race_safe_nonblocking_nofollow_flags() {
        use std::{ffi::CString, os::{fd::AsRawFd, unix::{ffi::OsStrExt, fs::symlink}}};
        let fixture = Fixture::new(); let root = fixture.root.join("source");
        for name in ["pipe.bin", "pipe.gltf"] {
            let fifo = root.join(name); let encoded = CString::new(fifo.as_os_str().as_bytes()).unwrap();
            assert_eq!(unsafe { libc::mkfifo(encoded.as_ptr(), 0o600) }, 0);
            // Assert the metadata-only guard first: a regression fails here,
            // before a test can accidentally perform a blocking FIFO open.
            assert!(checked_path(&root, Path::new(name)).unwrap_err().contains("reguläre"));
            assert!(read_local(&root, Path::new(name), 16).is_err());
        }
        let path = root.join("regular.bin"); fs::write(&path, [1]).unwrap();
        let file = resource_open_options().open(&path).unwrap();
        let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFL) };
        assert!(flags >= 0 && flags & libc::O_NONBLOCK != 0);
        symlink(&path, root.join("swapped.bin")).unwrap();
        assert!(resource_open_options().open(root.join("swapped.bin")).is_err());
        assert!(fixture.empty_target());
    }

    #[test]
    fn store_imports_gltf_as_portable_glb_for_existing_profile_exports() {
        let fixture = Fixture::new(); let document = character(&fixture); let source = fixture.source(&document);
        let store = crate::storage::Store::new(fixture.root.join("store")).unwrap();
        let name = store.import_avatar(&source).unwrap(); assert!(name.ends_with(".glb")); assert!(crate::avatar_metadata::imported_asset_name(&name));
        let mut profile = store.profile().unwrap(); profile["avatar"]["assetPath"] = json!(name); store.save_profile(profile).unwrap();
        let package = fixture.root.join("portable-profile.json"); store.export(&package).unwrap();
        let bundle: Value = serde_json::from_slice(&fs::read(package).unwrap()).unwrap();
        assert!(bundle["avatar"]["name"].as_str().unwrap().ends_with(".glb"));
        let bytes = STANDARD.decode(bundle["avatar"]["base64"].as_str().unwrap()).unwrap();
        assert_eq!(crate::avatar_metadata::inspect_bytes(&bytes).unwrap().animations[0].name, "Wave");
    }
    #[test]
    fn actual_compressed_gltf_sources_pack_with_preserved_streams_and_texture_data() {
        let directory=Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/compressed-avatar");
        let fixture=Fixture::new();
        let output=std::env::var_os("ORBITPAL_NATIVE_FIXTURE_DIR").map(PathBuf::from);
        if let Some(output)=&output { fs::create_dir_all(output).unwrap(); }
        for name in ["draco","meshopt-ext","meshopt-khr","meshopt-ext-optional","meshopt-khr-optional","meshopt-ext-virtual","meshopt-khr-virtual","basisu","basisu-optional","basisu-external"] {
            let source=directory.join(format!("{name}.gltf"));let destination=fixture.root.join("store/avatars").join(format!("{name}.glb"));
            import(&source,&destination).unwrap_or_else(|error|panic!("{name}: {error}"));
            let info=crate::avatar_metadata::inspect_file(&destination).unwrap();assert!(info.warnings.is_empty());assert!(info.external_resources.is_empty());
            if let Some(output)=&output { fs::copy(&destination,output.join(format!("{name}.glb"))).unwrap(); }
        }
    }

}
