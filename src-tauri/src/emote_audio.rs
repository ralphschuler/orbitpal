//! Bot-scoped optional emote clips. Built-in effects are synthesized in WebAudio.
use base64::Engine;
use serde_json::{json, Value};
use std::{fs, io::{Read, Write}, path::{Path, PathBuf}};

const MAX_BYTES: usize = 5 * 1024 * 1024;
fn error(value: impl std::fmt::Display) -> String { value.to_string() }
fn valid_profile_id(id: &str) -> bool { !id.is_empty() && id.len() <= 96 && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')) }
fn mime(extension: &str) -> Option<&'static str> { match extension { "wav" => Some("audio/wav"), "mp3" => Some("audio/mpeg"), "ogg" => Some("audio/ogg"), "webm" => Some("audio/webm"), "m4a" => Some("audio/mp4"), _ => None } }

pub fn custom_filename(id: &str) -> Option<&str> {
    let filename = id.strip_prefix("custom:")?;
    let (uuid, extension) = filename.split_once('.')?;
    let parsed = uuid::Uuid::parse_str(uuid).ok()?;
    if parsed.to_string() != uuid || mime(extension).is_none() { return None; }
    Some(filename)
}
pub fn valid_sound_id(id: &str) -> bool { matches!(id, "" | "builtin:wave" | "builtin:cheer" | "builtin:whoosh") || custom_filename(id).is_some() }

fn sound_directory(app_root: &Path, profile_id: &str, create: bool) -> Result<PathBuf, String> {
    if !valid_profile_id(profile_id) { return Err("Ungültige Bot-ID für den Emote-Sound".into()); }
    let root = app_root.canonicalize().map_err(error)?;
    let mut directory = root.clone();
    for (position, part) in ["profiles", profile_id, "sounds"].into_iter().enumerate() {
        directory.push(part);
        if create && position == 2 && fs::symlink_metadata(&directory).is_err() {
            match fs::create_dir(&directory) { Ok(()) => (), Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (), Err(value) => return Err(error(value)) }
        }
        let metadata = fs::symlink_metadata(&directory).map_err(|_| "Soundordner dieses Bots wurde nicht gefunden".to_owned())?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() { return Err("Emote-Sounds dürfen nicht über Symlinks umgeleitet werden".into()); }
    }
    let directory = directory.canonicalize().map_err(error)?;
    if !directory.starts_with(root.join("profiles").join(profile_id)) { return Err("Soundordner liegt außerhalb dieses Bots".into()); }
    Ok(directory)
}
fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(error)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() { return Err("Sound muss eine reguläre Datei sein; Symlinks sind nicht erlaubt".into()); }
    if metadata.len() > MAX_BYTES as u64 { return Err("Ein Emote-Sound darf höchstens 5 MiB groß sein".into()); }
    let mut bytes = Vec::new(); fs::File::open(path).map_err(error)?.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(error)?;
    if bytes.len() > MAX_BYTES { return Err("Ein Emote-Sound darf höchstens 5 MiB groß sein".into()); } Ok(bytes)
}
fn mp3_frame(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0 && bytes[1] & 0x18 != 0x08
        && bytes[1] & 0x06 != 0 && bytes[2] & 0xf0 != 0xf0 && bytes[2] & 0x0c != 0x0c
}
fn valid_header(bytes: &[u8], extension: &str) -> bool {
    match extension {
        "wav" => bytes.len() >= 44 && bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE"),
        "mp3" => {
            if mp3_frame(bytes) { return true; }
            if bytes.len() < 10 || !bytes.starts_with(b"ID3") || !(2..=4).contains(&bytes[3]) || bytes[6..10].iter().any(|byte| byte & 0x80 != 0) { return false; }
            let size = bytes[6..10].iter().fold(0usize, |size, byte| (size << 7) | usize::from(*byte));
            let offset = 10usize.saturating_add(size).saturating_add(if bytes[3] == 4 && bytes[5] & 0x10 != 0 { 10 } else { 0 });
            bytes.get(offset..).is_some_and(|tail| tail.iter().take(4096).enumerate().any(|(at, _)| mp3_frame(&tail[at..])))
        },
        "ogg" => bytes.len() >= 27 && bytes.starts_with(b"OggS\0") && bytes[..bytes.len().min(1024)].windows(8).any(|window| window == b"OpusHead")
            || bytes.len() >= 27 && bytes.starts_with(b"OggS\0") && bytes[..bytes.len().min(1024)].windows(7).any(|window| window == b"\x01vorbis"),
        "webm" => bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) && bytes[..bytes.len().min(4096)].windows(4).any(|window| window == b"webm")
            && (bytes[..bytes.len().min(4096)].windows(6).any(|window| window == b"A_OPUS") || bytes[..bytes.len().min(4096)].windows(8).any(|window| window == b"A_VORBIS")),
        "m4a" => bytes.len() >= 24 && bytes.get(4..8) == Some(b"ftyp"),
        _ => false,
    }
}
fn validate_audio(bytes: &[u8], extension: &str) -> Result<&'static str, String> {
    let mime = mime(extension).ok_or("Emote-Sounds: .wav, .mp3, .ogg, .webm oder .m4a")?;
    if !valid_header(bytes, extension) { return Err("Dateiinhalt passt nicht zum gewählten Audioformat oder enthält keinen erkennbaren Audiostream".into()); }
    Ok(mime)
}

pub fn import(app_root: &Path, profile_id: &str, source: &Path) -> Result<Value, String> {
    let extension = source.extension().and_then(|value| value.to_str()).unwrap_or_default().to_ascii_lowercase();
    if mime(&extension).is_none() { return Err("Emote-Sounds: .wav, .mp3, .ogg, .webm oder .m4a".into()); }
    let name = source.file_name().and_then(|value| value.to_str()).ok_or("Sounddateiname fehlt")?;
    if name.len() > 180 || name.chars().any(char::is_control) { return Err("Ungültiger Sounddateiname".into()); }
    let bytes = read_bounded(source)?; let mime = validate_audio(&bytes, &extension)?;
    let directory = sound_directory(app_root, profile_id, true)?;
    let uuid = uuid::Uuid::new_v4(); let filename = format!("{uuid}.{extension}"); let temporary = directory.join(format!(".import-{uuid}"));
    let result = (|| { let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&temporary).map_err(error)?; file.write_all(&bytes).map_err(error)?; file.sync_all().map_err(error)?; drop(file); fs::rename(&temporary, directory.join(&filename)).map_err(error) })();
    if let Err(error) = result { let _ = fs::remove_file(temporary); return Err(error); }
    Ok(json!({"id":format!("custom:{filename}"),"name":name,"bytes":bytes.len(),"mimeType":mime}))
}

pub fn read_bytes(app_root: &Path, profile: &Value, sound_id: &str) -> Result<(Vec<u8>, &'static str), String> {
    let filename = custom_filename(sound_id).ok_or("Ungültige oder nicht dateibasierte Emote-Sound-ID")?;
    if !profile["emotes"].as_array().is_some_and(|emotes| emotes.iter().any(|emote| emote["sound"].as_str() == Some(sound_id))) { return Err("Dieser Sound ist keinem Emote dieses Bots zugewiesen".into()); }
    let profile_id = profile["id"].as_str().ok_or("Bot-ID fehlt")?;
    let directory = sound_directory(app_root, profile_id, false)?;
    let path = directory.join(filename); let bytes = read_bounded(&path)?;
    let extension = filename.rsplit_once('.').ok_or("Soundformat fehlt")?.1;
    let mime = validate_audio(&bytes, extension)?;
    Ok((bytes, mime))
}
pub fn read(app_root: &Path, profile: &Value, sound_id: &str) -> Result<Value, String> {
    let (bytes, mime) = read_bytes(app_root, profile, sound_id)?;
    Ok(json!({"id":sound_id,"base64":base64::engine::general_purpose::STANDARD.encode(bytes),"mimeType":mime}))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self { let root = std::env::temp_dir().join(format!("cohost-emote-audio-{}", uuid::Uuid::new_v4())); for id in ["nova", "fox"] { fs::create_dir_all(root.join("profiles").join(id)).unwrap(); } Self(root) }
        fn wav(&self) -> PathBuf { let mut bytes = vec![0; 48]; bytes[..4].copy_from_slice(b"RIFF"); bytes[4..8].copy_from_slice(&40u32.to_le_bytes()); bytes[8..12].copy_from_slice(b"WAVE"); bytes[12..16].copy_from_slice(b"fmt "); bytes[16..20].copy_from_slice(&16u32.to_le_bytes()); bytes[20..22].copy_from_slice(&1u16.to_le_bytes()); bytes[22..24].copy_from_slice(&1u16.to_le_bytes()); bytes[24..28].copy_from_slice(&16000u32.to_le_bytes()); bytes[28..32].copy_from_slice(&32000u32.to_le_bytes()); bytes[32..34].copy_from_slice(&2u16.to_le_bytes()); bytes[34..36].copy_from_slice(&16u16.to_le_bytes()); bytes[36..40].copy_from_slice(b"data"); bytes[40..44].copy_from_slice(&4u32.to_le_bytes()); let path = self.0.join("ding.wav"); fs::write(&path, bytes).unwrap(); path }
    }
    impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
    #[test]
    fn only_assigned_bot_scoped_files_can_be_read() {
        let fixture = Fixture::new(); let imported = import(&fixture.0, "nova", &fixture.wav()).unwrap(); let id = imported["id"].as_str().unwrap();
        assert!(valid_sound_id(id)); assert!(read(&fixture.0, &json!({"id":"nova","emotes":[]}), id).is_err());
        let profile = json!({"id":"nova","emotes":[{"sound":id}]}); assert_eq!(read(&fixture.0, &profile, id).unwrap()["mimeType"], "audio/wav");
        assert!(read(&fixture.0, &json!({"id":"fox","emotes":[{"sound":id}]}), id).is_err());
        assert!(read(&fixture.0, &json!({"id":"../nova","emotes":[{"sound":id}]}), id).is_err());
    }
    #[test]
    fn unknown_ids_and_path_injection_are_rejected() {
        for id in ["custom:../secret.wav", "custom:00000000000000000000000000000000.wav", "custom:00000000-0000-0000-0000-000000000000.exe", "builtin:unknown", "https://example.org/a.mp3"] { assert!(!valid_sound_id(id), "{id}"); }
        for id in ["", "builtin:wave", "builtin:cheer", "builtin:whoosh"] { assert!(valid_sound_id(id)); }
    }
    #[test]
    fn invalid_or_oversized_audio_is_rejected_without_partial_files() {
        let fixture = Fixture::new(); let source = fixture.0.join("fake.wav"); fs::write(&source, b"private non-audio text").unwrap(); assert!(import(&fixture.0, "nova", &source).is_err());
        fs::write(&source, vec![0; MAX_BYTES + 1]).unwrap(); assert!(import(&fixture.0, "nova", &source).unwrap_err().contains("5 MiB"));
        assert!(!fixture.0.join("profiles/nova/sounds").exists());
    }
    #[test]
    fn restored_files_are_revalidated_before_serving() {
        let fixture = Fixture::new(); let imported = import(&fixture.0, "nova", &fixture.wav()).unwrap(); let id = imported["id"].as_str().unwrap();
        fs::write(fixture.0.join("profiles/nova/sounds").join(custom_filename(id).unwrap()), b"tampered").unwrap();
        assert!(read(&fixture.0, &json!({"id":"nova","emotes":[{"sound":id}]}), id).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn symlinked_audio_and_parent_folders_are_not_followed() {
        let fixture = Fixture::new(); let source = fixture.wav(); let link = fixture.0.join("link.wav"); std::os::unix::fs::symlink(&source, &link).unwrap(); assert!(import(&fixture.0, "nova", &link).is_err());
        std::os::unix::fs::symlink(fixture.0.join("profiles/nova"), fixture.0.join("profiles/alias")).unwrap(); assert!(import(&fixture.0, "alias", &source).is_err());
    }
}
