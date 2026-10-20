//! Installation API connection metadata and actual transport scope. No OS,
//! network, filesystem or provider call occurs in this module.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;
pub(crate) const MAX_ENTRIES: usize = 4096;
pub(crate) const MAX_COUNTER: u64 = 9_007_199_254_740_991;
pub(crate) const INVALID:&str="Der API-Zugang passt nicht zu Anbieter und Zieladresse. Bitte einen passenden globalen Zugang auswählen.";
pub(crate) const UNAVAILABLE: &str =
    "Die globalen API-Zugänge sind nicht verfügbar. Bitte die Einstellungen prüfen.";
pub(crate) const STALE: &str = "Die API-Zugänge wurden inzwischen geändert. Bitte neu laden.";
pub(crate) const SHARED:&str="Dieser alte Schlüssel wird von mehreren Zugängen oder anderen Integrationen verwendet. Bitte einen unabhängigen API-Zugang mit eigenem Schlüssel anlegen.";
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Purpose {
    Dialog,
    ModelCatalogue,
    Speech,
    Transcription,
    Hindsight,
    YoutubeData,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Service {
    Openai,
    Anthropic,
    Elevenlabs,
    Hindsight,
    Compatible,
    YoutubeData,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AuthStyle {
    Bearer,
    Anthropic,
    Elevenlabs,
    YoutubeApiKey,
}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Binding {
    pub service: Service,
    pub destination: String,
    pub auth_style: AuthStyle,
}
fn field<'a>(config: &'a Value, key: &str, fallback: &'a str) -> &'a str {
    config[key]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback)
}
fn url_field<'a>(config: &'a Value, fallback: &'a str) -> Result<&'a str, String> {
    match config.get("baseUrl") {
        None | Some(Value::Null) => Ok(fallback),
        Some(Value::String(raw)) => {
            if raw.chars().any(char::is_control) {
                return Err(INVALID.into());
            }
            if raw.trim().is_empty() {
                Ok(fallback)
            } else {
                Ok(raw.trim())
            }
        }
        _ => Err(INVALID.into()),
    }
}
fn parse(raw: &str) -> Result<Url, String> {
    if raw.len() > 4096 || raw.chars().any(char::is_control) {
        return Err(INVALID.into());
    }
    let url = Url::parse(raw.trim()).map_err(|_| INVALID)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(INVALID.into());
    }
    Ok(url)
}
fn canonical(mut url: Url) -> String {
    let path = url.path().trim_end_matches('/').to_owned();
    url.set_path(&path);
    if (url.scheme() == "https" && url.port() == Some(443))
        || (url.scheme() == "http" && url.port() == Some(80))
    {
        let _ = url.set_port(None);
    }
    url.to_string().trim_end_matches('/').to_owned()
}
pub(crate) fn provider_base(config: &Value) -> Result<Url, String> {
    let default = match field(config, "provider", "compatible") {
        "ollama" => "http://localhost:11434/v1",
        "lmstudio" => "http://localhost:1234/v1",
        "anthropic" => "https://api.anthropic.com/v1",
        "elevenlabs" => "https://api.elevenlabs.io/v1",
        _ => "https://api.openai.com/v1",
    };
    let mut url = parse(url_field(config, default)?)?;
    let path = url.path().trim_end_matches('/');
    let path = if path.is_empty() { "/v1" } else { path }.to_owned();
    url.set_path(&path);
    Ok(url)
}
pub(crate) fn provider_endpoint(config: &Value, path: &str) -> Result<String, String> {
    let mut url = provider_base(config)?;
    let path = format!(
        "{}/{}",
        url.path().trim_end_matches('/'),
        path.trim_start_matches('/')
    );
    url.set_path(&path);
    Ok(url.to_string())
}
pub(crate) fn hindsight_base(config: &Value) -> Result<Url, String> {
    Url::parse(&canonical(parse(url_field(
        config,
        "http://127.0.0.1:8888",
    )?)?))
    .map_err(|_| INVALID.into())
}
/// The same function builds the actual upload destination and its auth scope.
/// Retain whisper.cpp loopback restriction; the transport also pins localhost.
pub(crate) fn transcription_endpoint(
    config: &Value,
    provider: &str,
    whisper_cpp: bool,
) -> Result<Url, String> {
    let default = match provider {
        "whisper-local" if whisper_cpp => "http://127.0.0.1:8080",
        "whisper-local" => "http://127.0.0.1:8080/v1",
        "elevenlabs" => "https://api.elevenlabs.io/v1",
        "openai" => "https://api.openai.com/v1",
        _ => "http://127.0.0.1:8000/v1",
    };
    let mut url = parse(url_field(config, default)?)?;
    if provider == "whisper-local" {
        let local = match url.host() {
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
            _ => false,
        };
        if !local {
            return Err("Lokales Whisper akzeptiert ausschließlich Loopback-Adressen: 127.0.0.1, ::1 oder localhost.".into());
        }
    }
    let suffix = if whisper_cpp {
        "inference"
    } else if provider == "elevenlabs" {
        "speech-to-text"
    } else {
        "audio/transcriptions"
    };
    let current = url.path().trim_end_matches('/');
    if !current.ends_with(&format!("/{suffix}")) {
        let prefix = if current.is_empty() && !whisper_cpp {
            "/v1"
        } else {
            current
        };
        let path = format!("{prefix}/{suffix}");
        url.set_path(&path);
    }
    Url::parse(&canonical(url)).map_err(|_| INVALID.into())
}
fn classify(destination: &str, style: AuthStyle) -> Service {
    match (destination, style) {
        ("https://api.openai.com/v1", AuthStyle::Bearer) => Service::Openai,
        ("https://api.anthropic.com/v1", AuthStyle::Anthropic) => Service::Anthropic,
        ("https://api.elevenlabs.io/v1", AuthStyle::Elevenlabs) => Service::Elevenlabs,
        _ => Service::Compatible,
    }
}
impl Binding {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let normalized = canonical(parse(&self.destination)?);
        if normalized != self.destination {
            return Err(INVALID.into());
        }
        let valid = match self.service {
            Service::Hindsight => self.auth_style == AuthStyle::Bearer,
            Service::YoutubeData => {
                self.auth_style == AuthStyle::YoutubeApiKey
                    && self.destination == "https://www.googleapis.com/youtube/v3"
            }
            Service::Compatible => {
                self.auth_style != AuthStyle::YoutubeApiKey
                    && classify(&self.destination, self.auth_style) == Service::Compatible
            }
            _ => classify(&self.destination, self.auth_style) == self.service,
        };
        if valid {
            Ok(())
        } else {
            Err(INVALID.into())
        }
    }
    pub(crate) fn normalized(
        service: Service,
        destination: &str,
        auth_style: AuthStyle,
    ) -> Result<Self, String> {
        let binding = Self {
            service,
            destination: canonical(parse(destination)?),
            auth_style,
        };
        binding.validate()?;
        Ok(binding)
    }
    pub(crate) fn request(config: &Value, purpose: Purpose) -> Result<Option<Self>, String> {
        let provider = field(
            config,
            "provider",
            if purpose == Purpose::Speech {
                "system"
            } else {
                "compatible"
            },
        );
        if matches!(purpose, Purpose::Dialog | Purpose::ModelCatalogue)
            && matches!(provider, "codex" | "claude-code")
            || purpose == Purpose::Speech && provider == "system"
        {
            return Ok(None);
        }
        let (base, style, service) = match purpose {
            Purpose::Hindsight => (
                hindsight_base(config)?,
                AuthStyle::Bearer,
                Some(Service::Hindsight),
            ),
            Purpose::YoutubeData => (
                parse("https://www.googleapis.com/youtube/v3")?,
                AuthStyle::YoutubeApiKey,
                Some(Service::YoutubeData),
            ),
            Purpose::Transcription => {
                if !matches!(
                    provider,
                    "whisper-local" | "openai" | "elevenlabs" | "compatible"
                ) {
                    return Err(INVALID.into());
                }
                let backend = field(config, "localBackend", "whispercpp");
                if provider == "whisper-local" && !matches!(backend, "whispercpp" | "compatible") {
                    return Err(INVALID.into());
                }
                let cpp = provider == "whisper-local" && backend == "whispercpp";
                let mut endpoint = transcription_endpoint(config, provider, cpp)?;
                let suffix = if cpp {
                    "/inference"
                } else if provider == "elevenlabs" {
                    "/speech-to-text"
                } else {
                    "/audio/transcriptions"
                };
                let path = endpoint
                    .path()
                    .trim_end_matches('/')
                    .strip_suffix(suffix)
                    .ok_or(INVALID)?
                    .to_owned();
                endpoint.set_path(&path);
                (
                    endpoint,
                    if provider == "elevenlabs" {
                        AuthStyle::Elevenlabs
                    } else {
                        AuthStyle::Bearer
                    },
                    None,
                )
            }
            _ => (
                provider_base(config)?,
                if purpose == Purpose::Speech && provider == "elevenlabs" {
                    AuthStyle::Elevenlabs
                } else if matches!(purpose, Purpose::Dialog | Purpose::ModelCatalogue)
                    && provider == "anthropic"
                {
                    AuthStyle::Anthropic
                } else {
                    AuthStyle::Bearer
                },
                None,
            ),
        };
        let destination = canonical(base);
        let binding = Self {
            service: service.unwrap_or_else(|| classify(&destination, style)),
            destination,
            auth_style: style,
        };
        binding.validate()?;
        Ok(Some(binding))
    }
}
pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
pub(crate) fn label(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 120 || value.chars().any(char::is_control) {
        Err("API-Zugänge benötigen einen Namen mit maximal 120 Bytes ohne Steuerzeichen.".into())
    } else {
        Ok(value.into())
    }
}
#[cfg(test)]
#[path = "api_connections/tests.rs"]
mod tests;
