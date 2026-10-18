//! Local, bounded source retrieval. PDF extraction happens only during import.
//! Originals and the portable text index stay together inside `knowledge/`.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, fs, io::{Read, Write}, path::{Path, PathBuf}, sync::{Arc, Mutex, OnceLock}, time::SystemTime};

const TEXT_LIMIT: usize = 4 * 1024 * 1024;
const PDF_LIMIT: usize = 20 * 1024 * 1024;
const INDEX_LIMIT: usize = 8 * 1024 * 1024;
const CORPUS_LIMIT: usize = 16 * 1024 * 1024;
const FILE_LIMIT: usize = 128;
const CHUNK_LIMIT: usize = 12_000;
const INDEX_DIR: &str = "_index";

#[derive(Clone, Serialize, Deserialize)]
struct Chunk {
    #[serde(skip_serializing_if="Option::is_none", default)] page: Option<u32>,
    line: usize,
    #[serde(rename="endLine")] end_line: usize,
    text: String,
}
#[derive(Serialize, Deserialize)]
struct Index {
    version: u8,
    text_bytes: usize,
    pages: usize,
    chunks: Vec<Chunk>,
}
struct CachedDocument { chunks: Vec<(Chunk, String)>, text_bytes: usize }
struct CacheEntry { modified: Option<SystemTime>, size: u64, document: Arc<CachedDocument> }
#[derive(Default)]
struct Cache { entries: HashMap<PathBuf, CacheEntry>, bytes: usize }
static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();

fn err(e: impl std::fmt::Display) -> String { e.to_string() }
fn extension(path: &Path) -> String { path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase() }
fn supported(path: &Path) -> bool { ["pdf", "txt", "md", "json", "csv"].contains(&extension(path).as_str()) }
fn safe_relative(name: &str) -> bool {
    !name.is_empty() && !name.contains('\\') && !Path::new(name).is_absolute()
        && Path::new(name).components().all(|p| matches!(p, std::path::Component::Normal(_)))
        && Path::new(name).components().next().is_some_and(|p| p.as_os_str() != INDEX_DIR)
}
fn index_path(root: &Path, name: &str) -> PathBuf { root.join(INDEX_DIR).join(format!("{name}.index.json")) }
fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(err)?;
    if file.metadata().map_err(err)?.len() > limit as u64 { return Err("Wissensdatei überschreitet die zulässige Größe".into()); }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes).map_err(err)?;
    if bytes.len() > limit { return Err("Wissensdatei überschreitet die zulässige Größe".into()); }
    Ok(bytes)
}
fn inside(root: &Path, path: &Path) -> bool {
    !path.symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(true)
        && path.canonicalize().map(|p| p.starts_with(root)).unwrap_or(false)
}

pub fn list(root: &Path) -> Result<Vec<Value>, String> {
    let root = root.canonicalize().map_err(err)?;
    let mut files = Vec::new();
    fn visit(root: &Path, dir: &Path, depth: usize, files: &mut Vec<Value>) -> Result<(), String> {
        if depth > 8 || files.len() >= 256 { return Ok(()); }
        let mut entries = fs::read_dir(dir).map_err(err)?.collect::<Result<Vec<_>, _>>().map_err(err)?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            if entry.file_name().to_string_lossy().starts_with('.') { continue; }
            if depth == 0 && entry.file_name() == INDEX_DIR { continue; }
            let kind = entry.file_type().map_err(err)?;
            if kind.is_symlink() { continue; }
            if kind.is_dir() { visit(root, &entry.path(), depth + 1, files)?; }
            else if kind.is_file() && supported(&entry.path()) {
                let name = entry.path().strip_prefix(root).map_err(err)?.to_string_lossy().replace('\\', "/");
                let indexed = inside(root, &index_path(root, &name));
                files.push(json!({"name":name,"size":entry.metadata().map_err(err)?.len(),"indexed":indexed || extension(&entry.path()) != "pdf"}));
            }
            if files.len() >= 256 { break; }
        }
        Ok(())
    }
    visit(&root, &root, 0, &mut files)?;
    files.sort_by_key(|file| file["name"].as_str().unwrap_or_default().to_owned());
    Ok(files)
}

fn push_chunk(chunks: &mut Vec<Chunk>, page: Option<u32>, line: usize, end_line: usize, text: &mut String) -> Result<(), String> {
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        if chunks.len() >= CHUNK_LIMIT { return Err("Die Textstruktur enthält zu viele Abschnitte. Bitte das Dokument in kleinere Dateien aufteilen.".into()); }
        chunks.push(Chunk { page, line, end_line, text: trimmed.to_owned() });
    }
    text.clear();
    Ok(())
}
fn chunks_for_text(text: &str, page: Option<u32>, chunks: &mut Vec<Chunk>) -> Result<(), String> {
    let mut buffer = String::new();
    let mut start = 1;
    let mut end = 1;
    let mut count = 0;
    for (offset, line) in text.lines().enumerate() {
        let number = offset + 1;
        if buffer.is_empty() { start = number; }
        for ch in line.chars() {
            if ch.is_control() && ch != '\t' { continue; }
            if count >= 1200 {
                push_chunk(chunks, page, start, number, &mut buffer)?;
                start = number; count = 0;
            }
            buffer.push(ch); count += 1;
        }
        buffer.push('\n'); count += 1; end = number;
        if number.saturating_sub(start) >= 31 || count >= 1200 {
            push_chunk(chunks, page, start, number, &mut buffer)?; count = 0;
        }
    }
    push_chunk(chunks, page, start, end, &mut buffer)
}

struct LimitedWriter { bytes: Vec<u8>, limit: usize, exceeded: bool }
impl Write for LimitedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.bytes.len().saturating_add(bytes.len()) > self.limit {
            self.exceeded = true;
            return Err(std::io::Error::new(std::io::ErrorKind::FileTooLarge, "Textindex zu groß"));
        }
        self.bytes.extend_from_slice(bytes); Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}
fn extract_pdf(bytes: &[u8]) -> Result<Index, String> {
    // Use the public page writer API rather than an unbounded Vec<String>:
    // https://docs.rs/pdf-extract/0.12.1/pdf_extract/fn.output_doc_page.html
    std::panic::catch_unwind(|| {
        let doc = pdf_extract::Document::load_mem(bytes).map_err(|_| "Die PDF konnte nicht gelesen werden. Bitte eine gültige Text-PDF verwenden.".to_owned())?;
        if doc.is_encrypted() { return Err("Verschlüsselte PDFs werden nicht unterstützt. Bitte eine unverschlüsselte Text-PDF importieren.".into()); }
        let pages = doc.get_pages();
        if pages.len() > 500 { return Err("PDFs dürfen höchstens 500 Seiten enthalten. Bitte die Datei aufteilen.".into()); }
        let mut index = Index { version: 1, text_bytes: 0, pages: pages.len(), chunks: Vec::new() };
        for page in pages.keys() {
            let mut writer = LimitedWriter { bytes: Vec::new(), limit: TEXT_LIMIT.saturating_sub(index.text_bytes), exceeded: false };
            let result = {
                let output: &mut dyn Write = &mut writer;
                let mut output = pdf_extract::PlainTextOutput::new(output);
                pdf_extract::output_doc_page(&doc, &mut output, *page)
            };
            if writer.exceeded { return Err("Der extrahierte PDF-Text überschreitet 4 MiB. Bitte die Datei aufteilen.".into()); }
            result.map_err(|_| "Der PDF-Text konnte nicht vollständig extrahiert werden. Bitte eine andere Text-PDF oder einen UTF-8-Textexport verwenden.".to_owned())?;
            index.text_bytes += writer.bytes.len();
            let text = String::from_utf8(writer.bytes).map_err(|_| "Der PDF-Text enthält eine ungültige Zeichenkodierung".to_owned())?;
            chunks_for_text(&text, Some(*page), &mut index.chunks)?;
        }
        if index.chunks.is_empty() { return Err("Diese PDF enthält keinen extrahierbaren Text. Gescannte oder bildbasierte PDFs brauchen zuvor OCR; OCR ist hier nicht enthalten.".into()); }
        Ok(index)
    }).map_err(|_| "Die PDF konnte nicht sicher verarbeitet werden. Bitte einen UTF-8-Textexport oder eine andere PDF verwenden.".to_owned())?
}

pub fn import(root: &Path, source: &Path) -> Result<Value, String> {
    let name = source.file_name().and_then(|name| name.to_str()).ok_or("Dateiname fehlt")?;
    if name.starts_with('.') || name.len() > 180 || name.chars().any(char::is_control) || !safe_relative(name) {
        return Err("Ungültiger Wissensdateiname".into());
    }
    if !supported(source) { return Err("Wissensdateien: .pdf, .txt, .md, .json oder .csv".into()); }
    let root = root.canonicalize().map_err(err)?;
    let target = root.join(name);
    let target_index = index_path(&root, name);
    if target.symlink_metadata().is_ok() || target_index.symlink_metadata().is_ok() { return Err("Eine Wissensdatei mit diesem Namen existiert bereits".into()); }
    let existing = list(&root)?;
    if existing.len() >= FILE_LIMIT { return Err("Pro Bot können höchstens 128 Wissensdokumente importiert werden".into()); }
    if !fs::metadata(source).map_err(err)?.is_file() { return Err("Bitte eine Wissensdatei auswählen".into()); }
    let pdf = extension(source) == "pdf";
    let limit = if pdf { PDF_LIMIT } else { TEXT_LIMIT };
    if fs::metadata(source).map_err(err)?.len() > limit as u64 {
        return Err(if pdf { "Eine PDF darf höchstens 20 MiB groß sein" } else { "Eine Textdatei darf höchstens 4 MiB groß sein" }.into());
    }
    let bytes = read_bounded(source, limit)?;
    let index = if pdf { extract_pdf(&bytes)? } else {
        let text = std::str::from_utf8(&bytes).map_err(|_| "Wissensdateien müssen UTF-8-Text enthalten".to_owned())?.trim_start_matches('\u{feff}');
        if text.contains('\0') { return Err("Diese Wissensdatei enthält Binärdaten statt UTF-8-Text".into()); }
        let mut chunks = Vec::new(); chunks_for_text(text, None, &mut chunks)?;
        if chunks.is_empty() { return Err("Die Wissensdatei enthält keinen lesbaren Text".into()); }
        Index { version: 1, text_bytes: text.len(), pages: 0, chunks }
    };
    let mut total = index.text_bytes;
    for file in existing {
        let Some(name) = file["name"].as_str() else { continue; };
        let path = index_path(&root, name);
        let size = if inside(&root, &path) {
            read_bounded(&path, INDEX_LIMIT).ok().and_then(|bytes| serde_json::from_slice::<Index>(&bytes).ok()).map(|index| index.text_bytes).unwrap_or(TEXT_LIMIT)
        } else { file["size"].as_u64().unwrap_or(0).min(TEXT_LIMIT as u64) as usize };
        total = total.saturating_add(size);
        if total > CORPUS_LIMIT { return Err("Der Wissensindex dieses Bots würde 16 MiB Text überschreiten. Bitte nicht mehr benötigte Dokumente entfernen oder Dateien aufteilen.".into()); }
    }
    let encoded = serde_json::to_vec(&index).map_err(err)?;
    if encoded.len() > INDEX_LIMIT { return Err("Der Textindex ist zu groß. Bitte das Dokument aufteilen.".into()); }
    let parent = target_index.parent().ok_or("Indexordner fehlt")?;
    fs::create_dir_all(parent).map_err(err)?;
    if !parent.canonicalize().map_err(err)?.starts_with(&root) { return Err("Wissensindex liegt außerhalb des Profilordners".into()); }
    let mut original = fs::OpenOptions::new().write(true).create_new(true).open(&target).map_err(err)?;
    let result = (|| {
        original.write_all(&bytes).map_err(err)?;
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&target_index).map_err(err)?;
        file.write_all(&encoded).map_err(err)
    })();
    if let Err(error) = result { drop(original); let _ = fs::remove_file(&target); let _ = fs::remove_file(&target_index); return Err(error); }
    Ok(json!({"name":name,"size":bytes.len(),"indexed":true,"pages":index.pages,"textBytes":index.text_bytes}))
}

fn load_document(root: &Path, name: &str) -> Result<Option<Arc<CachedDocument>>, String> {
    if !safe_relative(name) { return Ok(None); }
    let original = root.join(name);
    if !inside(root, &original) { return Ok(None); }
    let sidecar = index_path(root, name);
    let indexed = inside(root, &sidecar);
    if !indexed && extension(&original) == "pdf" { return Ok(None); }
    let path = if indexed { sidecar } else { original };
    let meta = fs::metadata(&path).map_err(err)?;
    let modified = meta.modified().ok();
    let cache = CACHE.get_or_init(|| Mutex::new(Cache::default()));
    if let Ok(cache) = cache.lock() {
        if let Some(entry) = cache.entries.get(&path).filter(|entry| entry.modified == modified && entry.size == meta.len()) { return Ok(Some(entry.document.clone())); }
    }
    let (chunks, text_bytes) = if indexed {
        let index: Index = serde_json::from_slice(&read_bounded(&path, INDEX_LIMIT)?).map_err(|_| "Wissensindex ist beschädigt. Bitte die Datei erneut importieren.".to_owned())?;
        if index.version != 1 || index.chunks.len() > CHUNK_LIMIT || index.text_bytes > TEXT_LIMIT { return Err("Wissensindex ist ungültig oder zu groß. Bitte die Datei erneut importieren.".into()); }
        let actual = index.chunks.iter().map(|chunk| chunk.text.len()).sum::<usize>();
        if actual > TEXT_LIMIT || index.chunks.iter().any(|chunk| chunk.text.chars().count() > 1201 || chunk.line == 0 || chunk.end_line < chunk.line) { return Err("Wissensindex enthält ungültige Textstellen".into()); }
        (index.chunks, index.text_bytes.max(actual))
    } else {
        let text = String::from_utf8(read_bounded(&path, TEXT_LIMIT)?).map_err(|_| "Wissensdatei ist kein UTF-8-Text".to_owned())?;
        let mut chunks = Vec::new(); chunks_for_text(&text, None, &mut chunks)?; (chunks, text.len())
    };
    let document = Arc::new(CachedDocument { chunks: chunks.into_iter().map(|chunk| { let lower = chunk.text.to_lowercase(); (chunk, lower) }).collect(), text_bytes });
    if let Ok(mut cache) = cache.lock() {
        // Limit the process-wide warm cache across all bot profiles.
        if cache.bytes.saturating_add(text_bytes.saturating_mul(2)) > 64 * 1024 * 1024 { cache.entries.clear(); cache.bytes = 0; }
        if let Some(previous) = cache.entries.remove(&path) { cache.bytes = cache.bytes.saturating_sub(previous.document.text_bytes.saturating_mul(2)); }
        cache.bytes = cache.bytes.saturating_add(text_bytes.saturating_mul(2));
        cache.entries.insert(path, CacheEntry { modified, size: meta.len(), document: document.clone() });
    }
    Ok(Some(document))
}

fn terms(query: &str) -> Vec<String> {
    let lower: String = query.chars().take(2000).flat_map(char::to_lowercase).collect();
    let stop = ["der","die","das","den","dem","des","ein","eine","einer","eines","einem","einen","und","oder","aber","mit","von","für","auf","aus","bei","als","was","wie","wer","wo","wann","warum","welche","welcher","welches","ist","sind","war","sein","hat","habe","haben","ich","du","er","sie","wir","ihr","mir","mich","dich","dein","deine","mein","meine","bitte","kann","kannst","können","nicht","noch","auch","zum","zur","über","the","and","for","with","what","who","when","where","why","how","are","was","this","that","you","your","please","can","tell","about"];
    let mut terms = Vec::new();
    for term in lower.split(|c: char| !c.is_alphanumeric()) {
        if term.chars().count() < 2 || stop.contains(&term) || terms.iter().any(|known| known == term) { continue; }
        terms.push(term.to_owned()); if terms.len() == 20 { break; }
    }
    terms
}
fn occurrences(text: &str, term: &str) -> usize {
    text.match_indices(term).filter(|(at, _)| {
        let end = at + term.len();
        !text[..*at].chars().next_back().is_some_and(char::is_alphanumeric)
            && !text[end..].chars().next().is_some_and(char::is_alphanumeric)
    }).take(6).count()
}

pub fn search(root: &Path, query: &str) -> Result<Value, String> {
    let root = root.canonicalize().map_err(err)?;
    let mut files = list(&root)?;
    let terms = terms(query);
    if terms.is_empty() { return Ok(json!({"query":query.chars().take(2000).collect::<String>(),"matches":[],"method":"bounded local keyword retrieval","truncated":false})); }
    // Filename matches choose the scan order if an imported legacy profile is
    // larger than the current corpus limit. No PDF parsing occurs here.
    files.sort_by_key(|file| { let name = file["name"].as_str().unwrap_or_default().to_lowercase(); std::cmp::Reverse(terms.iter().filter(|term| name.contains(term.as_str())).count()) });
    let mut candidates: Vec<(u64, String, Chunk)> = Vec::new();
    let mut read = 0usize;
    let mut truncated = files.len() > FILE_LIMIT;
    let mut skipped = Vec::new();
    for file in files.into_iter().take(FILE_LIMIT) {
        let Some(name) = file["name"].as_str() else { continue; };
        let document = match load_document(&root, name) {
            Ok(Some(document)) => document,
            Ok(None) => { skipped.push(name.to_owned()); continue; },
            Err(_) => { skipped.push(name.to_owned()); continue; },
        };
        if read.saturating_add(document.text_bytes) > CORPUS_LIMIT { truncated = true; continue; }
        read += document.text_bytes;
        let mut matches = Vec::new();
        for (chunk, lower) in &document.chunks {
            let counts: Vec<_> = terms.iter().map(|term| occurrences(lower, term)).collect();
            let distinct = counts.iter().filter(|count| **count > 0).count() as u64;
            if distinct == 0 { continue; }
            let score = distinct * 12 + counts.iter().sum::<usize>() as u64;
            matches.push((score, name.to_owned(), chunk.clone()));
        }
        matches.sort_by_key(|(score, _, _)| std::cmp::Reverse(*score));
        candidates.extend(matches.into_iter().take(3));
    }
    candidates.sort_by_key(|(score, _, _)| std::cmp::Reverse(*score)); candidates.truncate(6);
    let mut matches: Vec<Value> = Vec::new();
    for (score, file, chunk) in candidates {
        let excerpt = serde_json::to_value(chunk).map_err(err)?;
        if let Some(found) = matches.iter_mut().find(|value| value["file"].as_str() == Some(file.as_str())) {
            let total = found["score"].as_u64().unwrap_or(0) + score; found["score"] = json!(total);
            found["excerpts"].as_array_mut().expect("internal excerpts array").push(excerpt);
        } else { matches.push(json!({"file":file,"score":score,"excerpts":[excerpt]})); }
    }
    Ok(json!({"query":query.chars().take(2000).collect::<String>(),"matches":matches,"method":"bounded local keyword retrieval","truncated":truncated,"skipped":skipped}))
}

pub fn delete(root: &Path, name: &str) -> Result<(), String> {
    if !safe_relative(name) { return Err("Ungültiger Wissensdateiname".into()); }
    let root = root.canonicalize().map_err(err)?;
    let target = root.join(name);
    if !inside(&root, &target) || !target.is_file() || !supported(&target) { return Err("Datei liegt außerhalb des Wissensordners".into()); }
    fs::remove_file(&target).map_err(err)?;
    let index = index_path(&root, name);
    if inside(&root, &index) { fs::remove_file(&index).map_err(err)?; }
    if let Some(cache) = CACHE.get() { if let Ok(mut cache) = cache.lock() { for path in [target, index] { if let Some(entry) = cache.entries.remove(&path) { cache.bytes = cache.bytes.saturating_sub(entry.document.text_bytes.saturating_mul(2)); } } } }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture { root: PathBuf }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("cohost-knowledge-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(root.join("source")).unwrap();
            Self { root }
        }
        fn knowledge(&self, bot: &str) -> PathBuf {
            let root = self.root.join(bot).join("knowledge"); fs::create_dir_all(&root).unwrap(); root
        }
        fn source(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.root.join("source").join(name); fs::write(&path, bytes).unwrap(); path
        }
    }
    impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.root); } }

    /// Tiny real PDF fixture, generated locally with an explicit xref table.
    /// No renderer, network service, external account or fixture download.
    fn pdf(pages: &[&str]) -> Vec<u8> {
        let kids = (0..pages.len()).map(|i| format!("{} 0 R", 4 + i * 2)).collect::<Vec<_>>().join(" ");
        let mut objects = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            format!("<< /Type /Pages /Count {} /Kids [{}] >>", pages.len(), kids),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        ];
        for (i, text) in pages.iter().enumerate() {
            let escaped = text.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
            let stream = if text.is_empty() { "q Q".to_owned() } else { format!("BT /F1 12 Tf 50 750 Td ({escaped}) Tj ET") };
            objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>", 5 + i * 2));
            objects.push(format!("<< /Length {} >>\nstream\n{}\nendstream", stream.len(), stream));
        }
        let mut bytes = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, object) in objects.iter().enumerate() {
            offsets.push(bytes.len()); bytes.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", i + 1, object).as_bytes());
        }
        let xref = bytes.len();
        bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for offset in offsets { bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes()); }
        bytes.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", objects.len() + 1, xref).as_bytes());
        bytes
    }

    #[test]
    fn pdf_import_preserves_original_and_retrieves_correct_page_from_portable_index() {
        let fixture = Fixture::new(); let root = fixture.knowledge("nova");
        let bytes = pdf(&["The channel broadcasts on Tuesday.", "Our mascot is a purple ferret."]);
        let source = fixture.source("Channel.pdf", &bytes);
        let result = import(&root, &source).unwrap();
        assert_eq!(result["pages"], 2);
        assert_eq!(fs::read(root.join("Channel.pdf")).unwrap(), bytes);
        let found = search(&root, "What is the purple ferret mascot?").unwrap();
        assert_eq!(found["matches"][0]["file"], "Channel.pdf");
        assert_eq!(found["matches"][0]["excerpts"][0]["page"], 2);
        assert!(found["matches"][0]["excerpts"][0]["line"].as_u64().unwrap() >= 1);
        assert!(found["matches"][0]["excerpts"][0]["text"].as_str().unwrap().contains("ferret"));
        assert_eq!(list(&root).unwrap().len(), 1, "internal JSON indices must never appear as user documents");
        // An index copied with the profile remains usable without parsing PDF.
        let copied = fixture.knowledge("copied"); fs::create_dir_all(copied.join(INDEX_DIR)).unwrap();
        fs::copy(root.join("Channel.pdf"), copied.join("Channel.pdf")).unwrap();
        fs::copy(index_path(&root, "Channel.pdf"), index_path(&copied, "Channel.pdf")).unwrap();
        assert_eq!(search(&copied, "ferret").unwrap()["matches"][0]["excerpts"][0]["page"], 2);
        delete(&root, "Channel.pdf").unwrap();
        assert!(!index_path(&root, "Channel.pdf").exists());
        assert!(search(&root, "ferret").unwrap()["matches"].as_array().unwrap().is_empty());
    }

    #[test]
    fn image_only_pdf_is_rejected_without_partial_import() {
        let fixture = Fixture::new(); let root = fixture.knowledge("nova");
        let source = fixture.source("Scan.pdf", &pdf(&[""]));
        let error = import(&root, &source).unwrap_err();
        assert!(error.contains("OCR"));
        assert!(!root.join("Scan.pdf").exists());
        assert!(!index_path(&root, "Scan.pdf").exists());
    }

    #[test]
    fn natural_question_ignores_stop_words_and_respects_word_boundaries() {
        let fixture = Fixture::new(); let root = fixture.knowledge("nova");
        let source = fixture.source("Fakten.md", "Die Kanalisation ist lang.\nDer Kanal heißt Nebelwald.\n".as_bytes());
        import(&root, &source).unwrap();
        let found = search(&root, "Wie heißt bitte der Kanal?").unwrap();
        let excerpt = &found["matches"][0]["excerpts"][0];
        assert_eq!(excerpt["line"], 1); assert_eq!(excerpt["endLine"], 2);
        assert!(excerpt.get("page").is_none());
        assert!(excerpt["text"].as_str().unwrap().contains("Nebelwald"));
        assert_eq!(occurrences("kanalisation kanal kanäle", "kanal"), 1);
        assert!(search(&root, "wie ist der und die bitte").unwrap()["matches"].as_array().unwrap().is_empty());
    }

    #[test]
    fn retrieval_is_bounded_and_keeps_utf8_intact() {
        let fixture = Fixture::new(); let root = fixture.knowledge("nova");
        for number in 0..4 {
            let text = format!("{}\n{}\n{}", "Äpfel Sternenstaub ".repeat(100), "Äpfel Sternenstaub ".repeat(100), "Äpfel Sternenstaub ".repeat(100));
            let source = fixture.source(&format!("Fakten{number}.txt"), text.as_bytes()); import(&root, &source).unwrap();
        }
        let found = search(&root, "Äpfel Sternenstaub").unwrap();
        let excerpts = found["matches"].as_array().unwrap().iter().flat_map(|file| file["excerpts"].as_array().unwrap()).collect::<Vec<_>>();
        assert_eq!(excerpts.len(), 6);
        assert!(excerpts.iter().all(|excerpt| excerpt["text"].as_str().unwrap().chars().count() <= 1201));
        assert!(excerpts.iter().all(|excerpt| !excerpt["text"].as_str().unwrap().contains('\u{fffd}')));
    }

    #[test]
    fn bots_are_isolated_and_delete_cannot_escape_knowledge_directory() {
        let fixture = Fixture::new(); let nova = fixture.knowledge("nova"); let fox = fixture.knowledge("fox");
        import(&nova, &fixture.source("Secrets.txt", b"The secret passphrase is marmalade.")).unwrap();
        assert_eq!(search(&nova, "marmalade").unwrap()["matches"].as_array().unwrap().len(), 1);
        assert!(search(&fox, "marmalade").unwrap()["matches"].as_array().unwrap().is_empty());
        assert!(delete(&nova, "../source/Secrets.txt").is_err());
        assert!(delete(&nova, "_index/Secrets.txt.index.json").is_err());
        assert!(fixture.root.join("source/Secrets.txt").exists());
    }

    #[test]
    fn invalid_or_oversized_text_does_not_create_partial_files_or_replace_documents() {
        let fixture = Fixture::new(); let root = fixture.knowledge("nova");
        assert!(import(&root, &fixture.source("Binary.txt", &[0xff, 0xfe, 0])).is_err());
        assert!(!root.join("Binary.txt").exists());
        let large = fixture.source("Large.txt", &vec![b'a'; TEXT_LIMIT + 1]);
        assert!(import(&root, &large).unwrap_err().contains("4 MiB"));
        assert!(!root.join("Large.txt").exists());
        let source = fixture.source("Facts.csv", b"channel,mascot\nNebula,otter\n");
        import(&root, &source).unwrap();
        fs::write(&source, b"channel,mascot\nNebula,dragon\n").unwrap();
        assert!(import(&root, &source).is_err());
        assert!(search(&root, "otter").unwrap()["matches"][0]["excerpts"][0]["text"].as_str().unwrap().contains("otter"));
        assert!(search(&root, "dragon").unwrap()["matches"].as_array().unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_sources_and_indices_are_not_followed() {
        let fixture = Fixture::new(); let root = fixture.knowledge("nova");
        let outside = fixture.source("Outside.txt", b"Private pineapple credential.");
        std::os::unix::fs::symlink(&outside, root.join("Leak.txt")).unwrap();
        assert!(list(&root).unwrap().is_empty());
        assert!(search(&root, "pineapple").unwrap()["matches"].as_array().unwrap().is_empty());
        assert!(delete(&root, "Leak.txt").is_err());
        assert!(outside.exists());
        std::os::unix::fs::symlink(fixture.root.join("source"), root.join(INDEX_DIR)).unwrap();
        assert!(import(&root, &fixture.source("Facts.txt", b"Public pineapple trivia.")).is_err());
        assert!(!root.join("Facts.txt").exists());
    }
}
