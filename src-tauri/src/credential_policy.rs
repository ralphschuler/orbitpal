//! Resolve credentials by the operation that actually consumes them.
//! Keep provider JSON separate from the standalone native-store fixture.
use serde_json::Value;

/// Select a credential only when this operation's integration consumes it.
/// HTTP integrations may authenticate even on loopback; locality alone never
/// authorizes dropping an explicitly configured credential.
#[derive(Clone, Copy)]
pub(crate) enum Purpose {
    Http,
    Dialog,
    ModelCatalogue,
    Speech,
}

pub(crate) fn resolve_for(
    config: &Value,
    purpose: Purpose,
    read: impl FnOnce(&str) -> Result<Option<String>, String>,
) -> Result<Option<String>, String> {
    let provider = config["provider"]
        .as_str()
        .filter(|value| !value.trim().is_empty());
    let unused = match purpose {
        Purpose::Dialog | Purpose::ModelCatalogue => {
            matches!(provider, Some("codex" | "claude-code"))
        }
        // The default voice and explicit system voice both run in the browser.
        Purpose::Speech => provider.is_none() || provider == Some("system"),
        Purpose::Http => false,
    };
    if unused {
        return Ok(None);
    }
    let id = config["apiKeyId"]
        .as_str()
        .or_else(|| config["secretId"].as_str())
        .unwrap_or_default();
    if id.is_empty() {
        return Ok(None);
    }
    read(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_dialog_and_model_catalogue_never_resolve_a_stale_api_key() {
        for provider in ["codex", "claude-code"] {
            for purpose in [Purpose::Dialog, Purpose::ModelCatalogue] {
                let config =
                    serde_json::json!({"provider":provider,"apiKeyId":"saved-old-api-key"});
                let key = resolve_for(&config, purpose, |_| {
                    panic!("CLI auth must not read the OS credential store")
                });
                assert_eq!(key.unwrap(), None);
            }
        }
    }

    #[test]
    fn system_or_default_browser_speech_never_resolves_an_api_key() {
        for provider in [
            Value::Null,
            serde_json::json!("system"),
            serde_json::json!(""),
            serde_json::json!("  "),
        ] {
            let config = serde_json::json!({"provider":provider,"apiKeyId":"saved-old-tts-key"});
            assert_eq!(
                resolve_for(&config, Purpose::Speech, |_| panic!(
                    "Browser speech has no native HTTP credential consumer"
                ))
                .unwrap(),
                None
            );
        }
    }

    #[test]
    fn http_dialog_catalogue_and_speech_keep_the_exact_configured_credentials() {
        for (purpose, providers) in [
            (
                Purpose::Dialog,
                vec!["openai", "anthropic", "compatible", "ollama", "lmstudio"],
            ),
            (
                Purpose::ModelCatalogue,
                vec!["openai", "anthropic", "compatible", "ollama", "lmstudio"],
            ),
            (Purpose::Speech, vec!["elevenlabs", "compatible", "openai"]),
            (
                Purpose::Http,
                vec![
                    "whisper-local",
                    "compatible",
                    "openai",
                    "elevenlabs",
                    "local-memory",
                    "http-mcp",
                ],
            ),
        ] {
            for provider in providers {
                let config = serde_json::json!({"provider":provider,"baseUrl":"http://127.0.0.1:8000/v1","apiKeyId":"chosen-http-key"});
                let mut reads = 0;
                let key = resolve_for(&config, purpose, |id| {
                    reads += 1;
                    assert_eq!(id, "chosen-http-key");
                    Ok(Some("synthetic-authorized-value".into()))
                })
                .unwrap();
                assert_eq!(reads, 1);
                assert_eq!(key.as_deref(), Some("synthetic-authorized-value"));
            }
        }
    }

    #[test]
    fn generic_http_lookup_keeps_secret_id_fallback_and_propagates_denial() {
        let config = serde_json::json!({"secretId":"chosen-platform-or-mcp-token"});
        assert_eq!(
            resolve_for(&config, Purpose::Http, |id| {
                assert_eq!(id, "chosen-platform-or-mcp-token");
                Err("synthetic-denial".into())
            }),
            Err("synthetic-denial".into())
        );
        assert_eq!(
            resolve_for(&serde_json::json!({}), Purpose::Dialog, |_| panic!(
                "No credential reference"
            ))
            .unwrap(),
            None
        );
        // An explicitly empty API reference remains empty, as before; it must
        // not silently select a different fallback account.
        assert_eq!(
            resolve_for(
                &serde_json::json!({"apiKeyId":"","secretId":"other"}),
                Purpose::Http,
                |_| panic!("Empty configured reference")
            )
            .unwrap(),
            None
        );
    }
}
