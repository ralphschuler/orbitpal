//! Shared installed Skill packages. Bot grants remain in the profile database.
//! This module never enables a Skill or executes its scripts.
use serde_json::{json, Value};
use std::{collections::{BTreeMap, BTreeSet}, fs, io::{Read, Write}, path::{Component, Path, PathBuf}, sync::Mutex};

const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_DOCUMENT: usize = 512 * 1024;
const MAX_RESOURCE: usize = 1024 * 1024;
const MAX_FILES: usize = 512;
const MAX_SKILLS: usize = 512;
static LIBRARY_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone)]
pub struct SkillLibrary { root: PathBuf }
fn error(value: impl std::fmt::Display) -> String { value.to_string() }
fn valid_id(id: &str) -> bool { !id.is_empty() && id.len() <= 96 && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')) }
fn relative_name(path: &Path) -> Result<String, String> {
    let name = path.to_str().ok_or("Skillpfade müssen gültiges UTF-8 enthalten")?;
    if name.is_empty() || name.len() > 1024 || (std::path::MAIN_SEPARATOR != '\\' && name.contains('\\')) || path.is_absolute()
        || path.components().count() > 8
        || !path.components().all(|part| matches!(part, Component::Normal(value) if value.to_str().is_some_and(|value| !value.starts_with('.') && value.len() <= 255 && !value.chars().any(char::is_control)))) {
        return Err("Skillpfad muss relativ sein und darf keine versteckten Dateien oder übergeordneten Ordner enthalten".into());
    }
    Ok(name.replace(std::path::MAIN_SEPARATOR, "/"))
}
fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(error)?;
    if file.metadata().map_err(error)?.len() > limit as u64 { return Err("Skilldatei überschreitet die zulässige Größe".into()); }
    let mut bytes = Vec::new(); file.take(limit as u64 + 1).read_to_end(&mut bytes).map_err(error)?;
    if bytes.len() > limit { return Err("Skilldatei überschreitet die zulässige Größe".into()); }
    Ok(bytes)
}
fn ensure_directory(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => Err("Skillbibliothek darf nicht über Symlinks oder Dateien umgeleitet werden".into()),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(path).map_err(crate_error),
        Err(value) => Err(error(value)),
    }
}
fn crate_error(value: std::io::Error) -> String { value.to_string() }
fn checked_directory(path: &Path) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(path).map_err(error)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() { return Err("Skillordner fehlt oder ist ein Symlink".into()); }
    path.canonicalize().map_err(error)
}
fn checked_file(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    relative_name(relative)?;
    let mut current = root.to_path_buf();
    for part in relative.components() {
        current.push(part.as_os_str());
        if fs::symlink_metadata(&current).map_err(error)?.file_type().is_symlink() { return Err("Skillressourcen über Symlinks sind nicht erlaubt".into()); }
    }
    let canonical = current.canonicalize().map_err(error)?;
    if !canonical.starts_with(root) || !canonical.is_file() { return Err("Ressource liegt außerhalb des Skillordners oder ist keine Datei".into()); }
    Ok(canonical)
}

fn catalog(root: &Path) -> Result<Vec<(PathBuf, u64)>, String> {
    fn visit(root: &Path, directory: &Path, depth: usize, total: &mut u64, files: &mut Vec<(PathBuf, u64)>) -> Result<(), String> {
        if depth > 8 { return Err("Skillordner enthält zu viele Verzeichnisebenen".into()); }
        let mut entries = fs::read_dir(directory).map_err(error)?.collect::<Result<Vec<_>, _>>().map_err(error)?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            if entry.file_name().to_string_lossy().starts_with('.') { continue; }
            let kind = entry.file_type().map_err(error)?;
            if kind.is_symlink() { return Err("Skillpakete dürfen keine Symlinks enthalten".into()); }
            if kind.is_dir() { visit(root, &entry.path(), depth + 1, total, files)?; }
            else if kind.is_file() {
                let relative = entry.path().strip_prefix(root).map_err(error)?.to_path_buf(); relative_name(&relative)?;
                let size = entry.metadata().map_err(error)?.len(); *total = total.saturating_add(size);
                if size > MAX_BYTES as u64 || *total > MAX_BYTES as u64 || files.len() >= MAX_FILES { return Err("Ein Skillpaket darf höchstens 64 MiB und 512 Dateien enthalten".into()); }
                files.push((relative, size));
            }
        }
        Ok(())
    }
    let mut files = Vec::new(); visit(root, root, 0, &mut 0, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0)); Ok(files)
}
fn collect(root: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
    let mut files = Vec::new(); let mut remaining = MAX_BYTES;
    for (path, _) in catalog(root)? {
        let bytes = read_bounded(&checked_file(root, &path)?, remaining)?;
        remaining = remaining.checked_sub(bytes.len()).ok_or("Skillpaket überschreitet 64 MiB")?;
        files.push((path, bytes));
    }
    Ok(files)
}
fn validate_files(files: &[(PathBuf, Vec<u8>)]) -> Result<(), String> {
    if files.len() > MAX_FILES { return Err("Ein Skillpaket darf höchstens 512 Dateien enthalten".into()); }
    let mut total = 0usize; let mut names = BTreeSet::new(); let mut document = None;
    for (path, bytes) in files {
        let name = relative_name(path)?;
        if !names.insert(name.to_lowercase()) { return Err("Skillpaket enthält doppelte Dateipfade (einschließlich Groß-/Kleinschreibung)".into()); }
        total = total.saturating_add(bytes.len()); if total > MAX_BYTES { return Err("Ein Skillpaket darf höchstens 64 MiB enthalten".into()); }
        if name == "SKILL.md" { document = Some(bytes); }
    }
    validate_document(document.ok_or("Skillpaket enthält keine SKILL.md")?)
}
fn validate_document(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > MAX_DOCUMENT { return Err("SKILL.md darf höchstens 512 KiB enthalten".into()); }
    let content = std::str::from_utf8(bytes).map_err(|_| "SKILL.md muss UTF-8-Text enthalten".to_owned())?;
    if content.trim().is_empty() || content.contains('\0') { return Err("SKILL.md enthält keinen gültigen Anweisungstext".into()); }
    Ok(())
}
fn metadata(id: &str, content: &str) -> Value {
    let mut name = id.to_owned(); let mut description = String::new();
    let mut lines = content.trim_start_matches('\u{feff}').lines().peekable();
    if lines.next().is_some_and(|line| line.trim() == "---") {
        while let Some(line) = lines.next() {
            if line.trim() == "---" { break; }
            let Some((key, value)) = line.split_once(':') else { continue; };
            if !["name", "description"].contains(&key.trim()) { continue; }
            let mut value = value.trim().to_owned();
            if value == ">" || value == "|" || value == ">-" || value == "|-" {
                let separator = if value.starts_with('|') { "\n" } else { " " }; let mut parts = Vec::new();
                while lines.peek().is_some_and(|line| line.starts_with(' ') || line.starts_with('\t')) { parts.push(lines.next().unwrap_or_default().trim().to_owned()); }
                value = parts.join(separator);
            } else if value.starts_with('"') && value.ends_with('"') { value = serde_json::from_str::<String>(&value).unwrap_or_else(|_| value.trim_matches('"').to_owned()); }
            else if value.starts_with('\'') && value.ends_with('\'') { value = value.trim_matches('\'').replace("''", "'"); }
            if key.trim() == "name" && !value.trim().is_empty() { name = value.chars().take(160).collect(); }
            if key.trim() == "description" { description = value.chars().take(1200).collect(); }
        }
    }
    json!({"id":id,"name":name,"description":description})
}
fn fingerprint(files: &[(PathBuf, Vec<u8>)]) -> u64 {
    // A stable naming hint, NOT a security hash. Every reuse is also checked by
    // full path/byte equality, so even a hash collision cannot overwrite data.
    let mut hash = 0xcbf29ce484222325u64;
    for (path, bytes) in files {
        let portable_path = path.to_string_lossy().replace('\\', "/");
        for byte in portable_path.as_bytes().iter().copied().chain([0]).chain((bytes.len() as u64).to_le_bytes()).chain(bytes.iter().copied()) {
            hash ^= u64::from(byte); hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

impl SkillLibrary {
    pub fn new(app_root: &Path) -> Result<Self, String> {
        fs::create_dir_all(app_root).map_err(error)?;
        let app_root = app_root.canonicalize().map_err(error)?;
        ensure_directory(&app_root.join("library"))?; ensure_directory(&app_root.join("library/skills"))?;
        Ok(Self { root: app_root.join("library/skills") })
    }
    fn ids(&self) -> Result<Vec<String>, String> {
        let mut ids = Vec::new();
        for entry in fs::read_dir(&self.root).map_err(error)? {
            let entry = entry.map_err(error)?; let id = entry.file_name().to_string_lossy().into_owned();
            if !valid_id(&id) { continue; }
            let kind = entry.file_type().map_err(error)?;
            if kind.is_dir() && !kind.is_symlink() { ids.push(id); }
        }
        ids.sort(); if ids.len() > MAX_SKILLS { return Err("Die Skillbibliothek enthält mehr als 512 Skills".into()); } Ok(ids)
    }
    fn skill_root(&self, id: &str) -> Result<PathBuf, String> {
        if !valid_id(id) { return Err("Ungültige Skill-ID".into()); }
        let path = checked_directory(&self.root.join(id))?;
        if !path.starts_with(&self.root) { return Err("Skill liegt außerhalb der Bibliothek".into()); }
        let document = checked_file(&path, Path::new("SKILL.md"))?; validate_document(&read_bounded(&document, MAX_DOCUMENT)?)?; Ok(path)
    }
    fn metadata_for(&self, id: &str) -> Result<Value, String> {
        let root = self.skill_root(id)?;
        let bytes = read_bounded(&root.join("SKILL.md"), MAX_DOCUMENT)?;
        Ok(metadata(id, std::str::from_utf8(&bytes).map_err(error)?))
    }
    pub fn list(&self) -> Result<Vec<Value>, String> {
        let _guard = LIBRARY_LOCK.lock().map_err(error)?;
        self.ids()?.into_iter().map(|id| self.metadata_for(&id)).collect()
    }
    pub fn read(&self, id: &str) -> Result<Value, String> {
        let _guard = LIBRARY_LOCK.lock().map_err(error)?; let root = self.skill_root(id)?;
        let content = String::from_utf8(read_bounded(&root.join("SKILL.md"), MAX_DOCUMENT)?).map_err(error)?;
        let mut item = metadata(id, &content); item["content"] = json!(content);
        item["resources"] = json!(catalog(&root)?.into_iter().filter(|(path, _)| path != Path::new("SKILL.md")).map(|(path, size)| json!({"path":path.to_string_lossy().replace('\\',"/"),"size":size})).collect::<Vec<_>>());
        Ok(item)
    }
    #[cfg(test)]
    pub fn path(&self, id: &str) -> Result<PathBuf, String> { let _guard = LIBRARY_LOCK.lock().map_err(error)?; self.skill_root(id) }
    pub fn resource_path(&self, id: &str, relative: &str) -> Result<PathBuf, String> {
        let _guard = LIBRARY_LOCK.lock().map_err(error)?; checked_file(&self.skill_root(id)?, Path::new(relative))
    }
    pub fn read_resource(&self, id: &str, relative: &str) -> Result<String, String> {
        let _guard = LIBRARY_LOCK.lock().map_err(error)?;
        let path = checked_file(&self.skill_root(id)?, Path::new(relative))?;
        let bytes = read_bounded(&path, MAX_RESOURCE)?;
        let content = String::from_utf8(bytes).map_err(|_| "Diese Skillressource ist kein UTF-8-Text".to_owned())?;
        if content.contains('\0') { return Err("Diese Skillressource enthält Binärdaten".into()); } Ok(content)
    }
    pub fn files(&self, id: &str) -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
        let _guard = LIBRARY_LOCK.lock().map_err(error)?; collect(&self.skill_root(id)?)
    }
    fn install(&self, requested_id: &str, files: &[(PathBuf, Vec<u8>)]) -> Result<Value, String> {
        if !valid_id(requested_id) { return Err("Skill benötigt eine ID aus Buchstaben, Zahlen, - oder _ (maximal 96 Zeichen)".into()); }
        validate_files(files)?; let mut files = files.to_vec(); files.sort_by(|left, right| left.0.cmp(&right.0));
        let ids = self.ids()?;
        // Compare the requested name first, then all other installed packages.
        let mut candidates = ids.clone(); candidates.sort_by_key(|id| !id.eq_ignore_ascii_case(requested_id));
        for id in &candidates {
            let Ok(root) = self.skill_root(id) else { continue; };
            let Ok(existing_catalog) = catalog(&root) else { continue; };
            if existing_catalog.len() != files.len() || !existing_catalog.iter().zip(&files).all(|((path, size), (incoming, bytes))| path == incoming && *size == bytes.len() as u64) { continue; }
            if collect(&root)? == files {
                let mut item = self.metadata_for(id)?; item["reused"] = json!(true); item["conflict"] = json!(!id.eq_ignore_ascii_case(requested_id)); return Ok(item);
            }
        }
        if ids.len() >= MAX_SKILLS { return Err("Die gemeinsame Bibliothek darf höchstens 512 Skills enthalten".into()); }
        let collision = self.root.join(requested_id).symlink_metadata().is_ok() || ids.iter().any(|id| id.eq_ignore_ascii_case(requested_id));
        let mut id = requested_id.to_owned();
        if collision {
            let suffix = format!("-{:016x}", fingerprint(&files)); let stem: String = requested_id.chars().take(96 - suffix.len() - 5).collect();
            id = format!("{stem}{suffix}"); let mut number = 1;
            while self.root.join(&id).symlink_metadata().is_ok() || ids.iter().any(|known| known.eq_ignore_ascii_case(&id)) {
                id = format!("{stem}{suffix}-{number}"); number += 1; if number > 1024 { return Err("Keine freie Skill-ID für diese Kollision gefunden".into()); }
            }
        }
        let stage = self.root.join(format!(".install-{}", uuid::Uuid::new_v4())); fs::create_dir(&stage).map_err(error)?;
        let result = (|| {
            for (relative, bytes) in &files {
                let path = stage.join(relative); if let Some(parent) = path.parent() { fs::create_dir_all(parent).map_err(error)?; }
                let mut file = fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(error)?; file.write_all(bytes).map_err(error)?;
            }
            if self.root.join(&id).symlink_metadata().is_ok() { return Err("Die Skillbibliothek wurde parallel geändert. Bitte erneut importieren.".into()); }
            fs::rename(&stage, self.root.join(&id)).map_err(error)
        })();
        if let Err(error) = result { let _ = fs::remove_dir_all(stage); return Err(error); }
        let mut item = self.metadata_for(&id)?; item["reused"] = json!(false); item["conflict"] = json!(collision); Ok(item)
    }
    pub fn install_files(&self, requested_id: &str, files: &[(PathBuf, Vec<u8>)]) -> Result<Value, String> {
        let _guard = LIBRARY_LOCK.lock().map_err(error)?; self.install(requested_id, files)
    }
    pub fn import(&self, source: &Path) -> Result<Value, String> {
        let _guard = LIBRARY_LOCK.lock().map_err(error)?;
        let directory = if source.is_dir() { source.to_path_buf() } else {
            if source.file_name().and_then(|value| value.to_str()) != Some("SKILL.md") { return Err("Bitte einen Skillordner oder dessen SKILL.md auswählen".into()); }
            source.parent().ok_or("Skillordner fehlt")?.to_path_buf()
        };
        let directory = checked_directory(&directory)?; let files = collect(&directory)?; validate_files(&files)?;
        let fallback = directory.file_name().and_then(|value| value.to_str()).unwrap_or_default();
        let content = files.iter().find(|(path, _)| path == Path::new("SKILL.md")).ok_or("SKILL.md fehlt")?;
        let item = metadata(fallback, std::str::from_utf8(&content.1).map_err(error)?);
        let name = item["name"].as_str().filter(|name| valid_id(name)).unwrap_or(fallback); self.install(name, &files)
    }
    pub fn save(&self, id: &str, content: &str) -> Result<Value, String> {
        let _guard = LIBRARY_LOCK.lock().map_err(error)?; validate_document(content.as_bytes())?;
        if !valid_id(id) { return Err("Ungültige Skill-ID".into()); }
        if self.root.join(id).symlink_metadata().is_err() { return self.install(id, &[(PathBuf::from("SKILL.md"), content.as_bytes().to_vec())]); }
        let directory = self.skill_root(id)?; let temporary = directory.join(format!(".write-{}", uuid::Uuid::new_v4()));
        let result = (|| { let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&temporary).map_err(error)?; file.write_all(content.as_bytes()).map_err(error)?; file.sync_all().map_err(error)?; drop(file); fs::rename(&temporary, directory.join("SKILL.md")).map_err(error) })();
        if let Err(error) = result { let _ = fs::remove_file(temporary); return Err(error); } self.metadata_for(id)
    }
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let _guard = LIBRARY_LOCK.lock().map_err(error)?; let directory = self.skill_root(id)?;
        let removed = self.root.join(format!(".deleted-{}", uuid::Uuid::new_v4())); fs::rename(directory, &removed).map_err(error)?; fs::remove_dir_all(removed).map_err(error)
    }
}

/// Stage legacy packages without editing any old files or bot permissions.
/// The caller commits returned ID remaps and a migration marker together in
/// its SQLite transaction. Repeating a crash-interrupted staging deduplicates.
pub fn migrate_profile(app_root: &Path, profile_id: &str) -> Result<BTreeMap<String, String>, String> {
    if !valid_id(profile_id) { return Err("Ungültige Bot-ID für Skillmigration".into()); }
    let library = SkillLibrary::new(app_root)?; let _guard = LIBRARY_LOCK.lock().map_err(error)?;
    let legacy = app_root.join("profiles").join(profile_id).join("skills");
    if !legacy.exists() { return Ok(BTreeMap::new()); }
    let mut component = app_root.canonicalize().map_err(error)?;
    for name in ["profiles", profile_id, "skills"] {
        component.push(name);
        let metadata = fs::symlink_metadata(&component).map_err(error)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() { return Err("Alte Skillordner dürfen nicht über Symlinks umgeleitet werden".into()); }
    }
    let legacy = checked_directory(&legacy)?; let mut entries = fs::read_dir(&legacy).map_err(error)?.collect::<Result<Vec<_>, _>>().map_err(error)?; entries.sort_by_key(|entry| entry.file_name());
    let mut mapping = BTreeMap::new();
    for entry in entries {
        let id = entry.file_name().to_string_lossy().into_owned(); if !valid_id(&id) { continue; }
        let kind = entry.file_type().map_err(error)?; if kind.is_symlink() { return Err("Alter Skillordner ist ein Symlink; Migration wurde nicht freigegeben".into()); } if !kind.is_dir() { continue; }
        if !entry.path().join("SKILL.md").is_file() { continue; }
        let directory = checked_directory(&entry.path())?; let installed = library.install(&id, &collect(&directory)?)?;
        mapping.insert(id, installed["id"].as_str().ok_or("Installierte Skill-ID fehlt")?.to_owned());
    }
    Ok(mapping)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self { let path = std::env::temp_dir().join(format!("cohost-skill-library-{}", uuid::Uuid::new_v4())); fs::create_dir(&path).unwrap(); Self(path) }
        fn library(&self) -> SkillLibrary { SkillLibrary::new(&self.0).unwrap() }
        fn legacy(&self, bot: &str, id: &str, content: &[u8]) -> PathBuf { let path = self.0.join("profiles").join(bot).join("skills").join(id); fs::create_dir_all(&path).unwrap(); fs::write(path.join("SKILL.md"), content).unwrap(); path }
    }
    impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
    fn package(content: &str) -> Vec<(PathBuf, Vec<u8>)> { vec![("SKILL.md".into(), content.as_bytes().to_vec()), ("references/facts.txt".into(), b"verified facts".to_vec()), ("scripts/greet.py".into(), b"print('hello')".to_vec())] }

    #[test]
    fn metadata_is_progressive_and_resources_resolve_from_the_shared_library() {
        let fixture = Fixture::new(); let library = fixture.library();
        library.install_files("greeting", &package("---\nname: Greeting\ndescription: >\n  First line\n  second line\n---\nPrivate full instructions.")).unwrap();
        let list = library.list().unwrap(); assert_eq!(list[0]["description"], "First line second line"); assert!(list[0].get("content").is_none());
        let loaded = library.read("greeting").unwrap(); assert!(loaded["content"].as_str().unwrap().contains("full instructions")); assert_eq!(loaded["resources"].as_array().unwrap().len(), 2);
        assert_eq!(library.read_resource("greeting", "references/facts.txt").unwrap(), "verified facts");
        assert!(library.resource_path("greeting", "scripts/greet.py").unwrap().starts_with(fixture.0.join("library/skills").canonicalize().unwrap()));
        assert!(!fixture.0.join("profiles").exists(), "installing never creates or grants a bot profile");
    }
    #[test]
    fn migration_deduplicates_content_remaps_collisions_and_is_repeatable() {
        let fixture = Fixture::new(); let original = fixture.legacy("nova", "hello", b"First instructions");
        fixture.legacy("fox", "hello", b"Different instructions"); fixture.legacy("robot", "another-id", b"Different instructions");
        let first = migrate_profile(&fixture.0, "nova").unwrap(); let second = migrate_profile(&fixture.0, "fox").unwrap();
        assert_eq!(first["hello"], "hello"); assert_ne!(second["hello"], "hello");
        assert_eq!(migrate_profile(&fixture.0, "fox").unwrap(), second);
        assert_eq!(migrate_profile(&fixture.0, "robot").unwrap()["another-id"], second["hello"]);
        assert_eq!(fixture.library().list().unwrap().len(), 2); assert_eq!(fs::read(original.join("SKILL.md")).unwrap(), b"First instructions");
    }
    #[test]
    fn export_and_restore_keep_resources_without_overwriting_an_existing_skill() {
        let source = Fixture::new(); source.library().install_files("same-id", &package("Imported instructions")).unwrap();
        let exported = source.library().files("same-id").unwrap(); let target = Fixture::new(); target.library().save("same-id", "Existing instructions").unwrap();
        let installed = target.library().install_files("same-id", &exported).unwrap(); let id = installed["id"].as_str().unwrap(); assert_ne!(id, "same-id");
        assert_eq!(target.library().files(id).unwrap(), exported); assert_eq!(target.library().read("same-id").unwrap()["content"], "Existing instructions");
        assert_eq!(target.library().install_files("same-id", &exported).unwrap()["id"], id);
    }
    #[test]
    fn save_changes_only_global_instructions_and_delete_keeps_legacy_files() {
        let fixture = Fixture::new(); let legacy = fixture.legacy("nova", "hello", b"Old instructions"); migrate_profile(&fixture.0, "nova").unwrap();
        fixture.library().save("hello", "New instructions").unwrap(); assert_eq!(fixture.library().read("hello").unwrap()["content"], "New instructions");
        fixture.library().delete("hello").unwrap(); assert!(fixture.library().list().unwrap().is_empty()); assert_eq!(fs::read(legacy.join("SKILL.md")).unwrap(), b"Old instructions");
    }
    #[test]
    fn rejects_traversal_hidden_files_binary_text_and_case_collisions_without_partial_install() {
        let fixture = Fixture::new(); let library = fixture.library(); let mut invalid = package("Instructions"); invalid.push(("../escape.txt".into(), vec![1])); assert!(library.install_files("bad", &invalid).is_err());
        let mut duplicate = package("Instructions"); duplicate.push(("skill.md".into(), b"duplicate".to_vec())); assert!(library.install_files("bad", &duplicate).is_err()); assert!(library.list().unwrap().is_empty());
        let mut binary = package("Instructions"); binary.push(("image.bin".into(), vec![0, 255])); library.install_files("safe", &binary).unwrap(); assert!(library.read_resource("safe", "image.bin").is_err());
        assert!(library.resource_path("safe", "../other/SKILL.md").is_err()); assert!(library.resource_path("safe", ".env").is_err()); assert!(library.path("../safe").is_err());
        assert!(!fixture.0.join("escape.txt").exists()); assert!(fs::read_dir(&library.root).unwrap().all(|entry| !entry.unwrap().file_name().to_string_lossy().starts_with(".install-")));
    }
    #[cfg(unix)]
    #[test]
    fn refuses_symlinked_resource_and_library_roots() {
        let fixture = Fixture::new(); let library = fixture.library(); library.save("safe", "Instructions").unwrap(); let outside = fixture.0.join("outside.txt"); fs::write(&outside, b"private").unwrap();
        std::os::unix::fs::symlink(&outside, library.path("safe").unwrap().join("leak.txt")).unwrap(); assert!(library.read_resource("safe", "leak.txt").is_err()); assert!(library.files("safe").is_err());
        let other = Fixture::new(); std::os::unix::fs::symlink(fixture.0.join("library"), other.0.join("library")).unwrap(); assert!(SkillLibrary::new(&other.0).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn migration_refuses_a_symlinked_profile_parent() {
        let fixture = Fixture::new(); let outside = Fixture::new(); outside.legacy("victim", "private-skill", b"Private instructions");
        fs::create_dir_all(fixture.0.join("profiles")).unwrap();
        std::os::unix::fs::symlink(outside.0.join("profiles/victim"), fixture.0.join("profiles/alias")).unwrap();
        assert!(migrate_profile(&fixture.0, "alias").is_err());
        assert!(fixture.library().list().unwrap().is_empty());
    }
}
