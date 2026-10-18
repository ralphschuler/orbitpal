//! Multipart speech recognition adapters. The frontend supplies a bounded microphone recording.
//! Local mode never follows redirects or uses system HTTP proxies.
use base64::{engine::general_purpose::STANDARD, Engine};
use futures_util::StreamExt;
use reqwest::{multipart::{Form, Part}, Client, Url};
use serde_json::Value;
use std::{net::{IpAddr, Ipv4Addr, SocketAddr}, time::Duration};

const MAX_AUDIO: usize = 25 * 1024 * 1024;
const MAX_RESPONSE: usize = 2 * 1024 * 1024;

fn field<'a>(config: &'a Value, name: &str, default: &'a str) -> &'a str {
    config[name].as_str().map(str::trim).filter(|value| !value.is_empty()).unwrap_or(default)
}

fn endpoint(config: &Value, provider: &str, whisper_cpp: bool) -> Result<Url, String> {
    let default = match provider {
        "whisper-local" if whisper_cpp => "http://127.0.0.1:8080",
        "whisper-local" => "http://127.0.0.1:8080/v1",
        "elevenlabs" => "https://api.elevenlabs.io/v1",
        "openai" => "https://api.openai.com/v1",
        _ => "http://127.0.0.1:8000/v1",
    };
    let mut url = Url::parse(field(config, "baseUrl", default)).map_err(|_| "Die Spracherkennungs-URL ist ungültig.".to_string())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none()
        || !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some() {
        return Err("Die Spracherkennungs-URL muss HTTP(S) verwenden und darf keine Zugangsdaten, Query oder Sprungmarke enthalten.".into());
    }
    if provider == "whisper-local" {
        let local = match url.host() {
            Some(url::Host::Ipv4(address)) => address.is_loopback(),
            Some(url::Host::Ipv6(address)) => address.is_loopback(),
            Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
            None => false,
        };
        if !local { return Err("Lokales Whisper akzeptiert ausschließlich Loopback-Adressen: 127.0.0.1, ::1 oder localhost.".into()); }
    }
    let suffix = if whisper_cpp { "inference" } else if provider == "elevenlabs" { "speech-to-text" } else { "audio/transcriptions" };
    let current = url.path().trim_end_matches('/');
    // Also accept a full endpoint pasted into the base URL field without appending it twice.
    if !current.ends_with(&format!("/{suffix}")) {
        let prefix = if current.is_empty() && !whisper_cpp { "/v1" } else { current };
        url.set_path(&format!("{prefix}/{suffix}"));
    }
    Ok(url)
}

fn http_client(local: bool) -> Result<Client, String> {
    let mut builder = Client::builder().connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(180)).redirect(reqwest::redirect::Policy::none());
    if local {
        builder = builder.no_proxy()
            // Pin localhost instead of letting DNS or a proxy turn local mode into a remote upload.
            .resolve("localhost", SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0));
    }
    builder.build().map_err(|_| "Der HTTP-Client für Spracherkennung konnte nicht erstellt werden.".into())
}

fn audio_file(audio_base64: &str, mime_type: &str) -> Result<Part, String> {
    let maximum_encoded = ((MAX_AUDIO + 2) / 3) * 4;
    if audio_base64.len() > maximum_encoded + 256 { return Err("Die Mikrofonaufnahme ist zu groß (maximal 25 MiB).".into()); }
    let encoded = if audio_base64.starts_with("data:") {
        audio_base64.split_once(',').map(|(_, data)| data).ok_or_else(|| "Die Mikrofonaufnahme enthält ungültige Base64-Daten.".to_string())?
    } else { audio_base64 };
    if encoded.len() > maximum_encoded { return Err("Die Mikrofonaufnahme ist zu groß (maximal 25 MiB).".into()); }
    let bytes = STANDARD.decode(encoded).map_err(|_| "Die Mikrofonaufnahme enthält ungültige Base64-Daten.".to_string())?;
    if bytes.is_empty() { return Err("Die Mikrofonaufnahme ist leer.".into()); }
    if bytes.len() > MAX_AUDIO { return Err("Die Mikrofonaufnahme ist zu groß (maximal 25 MiB).".into()); }
    let mime = mime_type.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    let (extension, mime) = match mime.as_str() {
        "audio/wav" | "audio/wave" | "audio/x-wav" => ("wav", "audio/wav"),
        "audio/webm" | "video/webm" => ("webm", "audio/webm"),
        "audio/mpeg" | "audio/mp3" => ("mp3", "audio/mpeg"),
        "audio/mp4" | "audio/m4a" | "audio/x-m4a" | "video/mp4" => ("m4a", "audio/mp4"),
        "audio/ogg" | "application/ogg" => ("ogg", "audio/ogg"),
        "audio/flac" | "audio/x-flac" => ("flac", "audio/flac"),
        _ => return Err("Das Aufnahmeformat wird nicht unterstützt. Bitte WAV, WebM, MP3, M4A, Ogg oder FLAC verwenden.".into()),
    };
    Part::bytes(bytes).file_name(format!("microphone.{extension}")).mime_str(mime)
        .map_err(|_| "Das Audioformat konnte nicht für den Upload vorbereitet werden.".into())
}

/// A compact mapping also accepts existing profiles containing locale strings or ISO-639-3.
fn language(config: &Value, elevenlabs: bool) -> Result<Option<String>, String> {
    let raw = config["language"].as_str().unwrap_or("de").trim().to_ascii_lowercase();
    if raw.is_empty() || raw == "auto" { return Ok(None); }
    let code = raw.split(['-', '_']).next().unwrap_or("");
    if !(2..=3).contains(&code.len()) || !code.bytes().all(|character| character.is_ascii_alphabetic()) {
        return Err("Die Erkennungssprache muss ein Sprachcode wie de, en oder auto sein.".into());
    }
    let pairs = [
        ("de","deu"),("en","eng"),("fr","fra"),("es","spa"),("it","ita"),("pt","por"),
        ("nl","nld"),("pl","pol"),("ru","rus"),("uk","ukr"),("tr","tur"),("ja","jpn"),
        ("zh","zho"),("ko","kor"),("ar","ara"),("hi","hin"),("sv","swe"),("no","nor"),
        ("da","dan"),("fi","fin"),("cs","ces"),("el","ell"),("he","heb"),("hu","hun"),
        ("ro","ron"),("sk","slk"),("sl","slv"),("hr","hrv"),("bg","bul"),("id","ind"),
        ("vi","vie"),("th","tha"),("ca","cat"),("eu","eus"),("gl","glg"),("fa","fas"),
        ("ur","urd"),("ta","tam"),("te","tel"),("ms","msa"),("is","isl"),("et","est"),
        ("lv","lav"),("lt","lit"),("af","afr"),("bn","ben"),("bs","bos"),("mk","mkd"),
        ("sw","swa"),("sr","srp"),("cy","cym"),("ne","nep"),("hy","hye"),("ka","kat"),
    ];
    if elevenlabs {
        // Scribe accepts both ISO-639-1 and ISO-639-3; prefer ISO-639-3 when known.
        return Ok(Some(pairs.iter().find(|(two, _)| *two == code).map(|(_, three)| *three).unwrap_or(code).to_string()));
    }
    let normalized = pairs.iter().find(|(_, three)| *three == code).map(|(two, _)| *two).unwrap_or(code);
    if normalized.len() != 2 { return Err("Whisper benötigt einen zweistelligen Sprachcode, zum Beispiel de oder en.".into()); }
    Ok(Some(normalized.to_string()))
}

async fn response_text(response: reqwest::Response, local: bool, whisper_cpp: bool) -> Result<String, String> {
    let status = response.status();
    if !status.is_success() {
        let hint = match status.as_u16() {
            301..=399 => "Weiterleitungen sind deaktiviert; bitte die endgültige API-URL eintragen",
            400 | 415 | 422 if whisper_cpp => "WAV-Aufnahme, Sprache und whisper.cpp-Konfiguration prüfen; andere Audioformate benötigen gegebenenfalls --convert und ffmpeg",
            400 | 415 | 422 => "Audioformat, Modell und Sprache prüfen",
            401 | 403 => "API-Key und dessen Berechtigung für Spracherkennung prüfen",
            402 => "Beim Anbieter ist kein ausreichendes Kontingent verfügbar",
            404 if local => "Lokalen Server und Backend prüfen: whisper.cpp verwendet /inference, kompatible Server /v1/audio/transcriptions",
            404 => "Basis-URL und ausgewähltes Sprachmodell prüfen",
            413 => "Die Aufnahme überschreitet das Uploadlimit des Anbieters",
            429 => "Anfragelimit oder Kontingent des Anbieters erreicht; später erneut versuchen",
            503 if local => "Der lokale Server ist noch nicht bereit; geladenes Whisper-Modell prüfen",
            _ => "Sprachanbieter oder lokalen Server prüfen",
        };
        // Never relay response bodies; a proxy may echo audio, secrets or private request details.
        return Err(format!("Spracherkennung: HTTP {}; {hint}.", status.as_u16()));
    }
    if response.content_length().is_some_and(|length| length > MAX_RESPONSE as u64) {
        return Err("Die Antwort der Spracherkennung ist zu groß (maximal 2 MiB).".into());
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "Die Verbindung zur Spracherkennung wurde während der Antwort unterbrochen.".to_string())?;
        if bytes.len() + chunk.len() > MAX_RESPONSE { return Err("Die Antwort der Spracherkennung ist zu groß (maximal 2 MiB).".into()); }
        bytes.extend_from_slice(&chunk);
    }
    let result: Value = serde_json::from_slice(&bytes).map_err(|_| "Der Sprachanbieter lieferte keine gültige JSON-Transkription.".to_string())?;
    let text = result["text"].as_str().ok_or_else(|| "Der Sprachanbieter lieferte keine Transkription im Feld text.".to_string())?.trim();
    if text.is_empty() { return Err("In der Mikrofonaufnahme wurde keine Sprache erkannt.".into()); }
    Ok(text.to_string())
}

pub async fn transcribe(config: &Value, audio_base64: &str, mime_type: &str, api_key: Option<String>) -> Result<String, String> {
    let provider = field(config, "provider", "compatible");
    if !matches!(provider, "whisper-local" | "openai" | "elevenlabs" | "compatible") { return Err("Unbekannte Spracherkennungs-Integration. Bitte Whisper lokal, OpenAI, ElevenLabs oder kompatiblen Server auswählen.".into()); }
    let local = provider == "whisper-local";
    let backend = field(config, "localBackend", "whispercpp");
    if local && !matches!(backend, "whispercpp" | "compatible") { return Err("Unbekanntes lokales Backend. Bitte whispercpp oder compatible auswählen.".into()); }
    let whisper_cpp = local && backend == "whispercpp";
    let elevenlabs = provider == "elevenlabs";
    let endpoint = endpoint(config, provider, whisper_cpp)?;
    let key = api_key.as_deref().map(str::trim).filter(|key| !key.is_empty());
    if matches!(provider, "openai" | "elevenlabs") && key.is_none() {
        return Err(format!("Für {}-Spracherkennung muss zuerst ein API-Key gespeichert werden.", if elevenlabs { "ElevenLabs" } else { "OpenAI" }));
    }
    let mut form = Form::new().part("file", audio_file(audio_base64, mime_type)?);
    let language = language(config, elevenlabs)?;
    if elevenlabs {
        form = form.text("model_id", field(config, "model", "scribe_v2").to_string())
            .text("diarize", "false").text("tag_audio_events", "false");
        if let Some(language) = language { form = form.text("language_code", language); }
    } else {
        form = form.text("response_format", "json").text("temperature", "0");
        if !whisper_cpp { form = form.text("model", field(config, "model", "whisper-1").to_string()); }
        if let Some(language) = language { form = form.text("language", language); }
        else if whisper_cpp { form = form.text("language", "auto"); }
    }
    let mut request = http_client(local)?.post(endpoint).header("Accept", "application/json").multipart(form);
    if let Some(key) = key {
        request = if elevenlabs { request.header("xi-api-key", key) } else { request.bearer_auth(key) };
    }
    let response = request.send().await.map_err(|error| {
        if error.is_timeout() { "Die Spracherkennung hat das Zeitlimit von 180 Sekunden überschritten.".to_string() }
        else if local { "Lokales Whisper ist nicht erreichbar. whisper-server starten, ein Modell laden und Loopback-Adresse sowie Port prüfen.".to_string() }
        else { "Der Sprachanbieter ist nicht erreichbar. Internetverbindung und API-URL prüfen.".to_string() }
    })?;
    response_text(response, local, whisper_cpp).await
}
