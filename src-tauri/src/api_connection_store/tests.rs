use super::*;
use std::{fs, path::PathBuf};
struct Fixture(PathBuf);
impl Fixture {
    fn new(profiles: Vec<Value>, ids: &[&str]) -> Self {
        let root =
            std::env::temp_dir().join(format!("orbitpal-global-api-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let db = Connection::open(root.join("cohost.sqlite3")).unwrap();
        db.execute_batch("CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);CREATE TABLE bot_profiles(id TEXT PRIMARY KEY,profile TEXT NOT NULL);CREATE TABLE secret_ids(id TEXT PRIMARY KEY);CREATE TABLE mcp_library(id TEXT PRIMARY KEY,server TEXT NOT NULL);INSERT INTO settings VALUES('extension_library_v1','true');").unwrap();
        for p in &profiles {
            db.execute(
                "INSERT INTO bot_profiles VALUES(?1,?2)",
                params![p["id"].as_str().unwrap(), p.to_string()],
            )
            .unwrap();
        }
        db.execute(
            "INSERT INTO settings VALUES('profile',?1)",
            [profiles[0].to_string()],
        )
        .unwrap();
        for id in ids {
            db.execute("INSERT INTO secret_ids VALUES(?1)", [id])
                .unwrap();
        }
        Self(root)
    }
    fn store(&self) -> Store {
        Store::new(self.0.clone()).unwrap()
    }
    fn db(&self) -> Connection {
        Connection::open(self.0.join("cohost.sqlite3")).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn profile(id: &str, key: &str) -> Value {
    let mut p: Value =
        serde_json::from_str(include_str!("../../../public/default-profile.json")).unwrap();
    p["id"] = json!(id);
    p["model"]["provider"] = json!("openai");
    p["model"]["baseUrl"] = json!("https://api.openai.com/v1");
    p["model"]["apiKeyId"] = json!(key);
    p["stt"]["provider"] = json!("openai");
    p["stt"]["baseUrl"] = json!("https://api.openai.com/v1/audio/transcriptions");
    p["stt"]["apiKeyId"] = json!(key);
    p
}
fn raw(store: &Store, id: &str) -> Value {
    let db = store.db.lock().unwrap();
    let text: String = db
        .query_row("SELECT profile FROM bot_profiles WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
#[test]
fn migration_roundtrip_deduplicates_shared_api_account_without_secret_reads() {
    let f = Fixture::new(
        vec![
            profile("nova", "shared-old"),
            profile("second", "shared-old"),
        ],
        &["shared-old"],
    );
    let store = f.store();
    let snapshot = store.api_snapshot();
    assert_eq!(snapshot["availability"], "ready");
    assert_eq!(snapshot["connections"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["connections"][0]["writePolicy"], "replaceable");
    let a = raw(&store, "nova");
    let b = raw(&store, "second");
    assert_eq!(a["model"]["apiKeyId"], a["stt"]["apiKeyId"]);
    assert_eq!(a["model"]["apiKeyId"], b["model"]["apiKeyId"]);
    assert_eq!(
        snapshot["defaults"][0]["connectionId"],
        a["model"]["apiKeyId"]
    );
    let before = f
        .db()
        .query_row("SELECT metadata FROM global_api_connections", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap();
    store.migrate_api_connections().unwrap();
    assert_eq!(
        before,
        f.db()
            .query_row("SELECT metadata FROM global_api_connections", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap()
    );
    assert_eq!(snapshot, f.store().api_snapshot());
    assert!(!snapshot.to_string().contains("shared-old"));
}
#[test]
fn conflicting_old_accounts_preserve_choices_without_default_guess() {
    let f = Fixture::new(
        vec![profile("nova", "one"), profile("second", "two")],
        &["one", "two"],
    );
    let store = f.store();
    let snapshot = store.api_snapshot();
    assert_eq!(snapshot["connections"].as_array().unwrap().len(), 2);
    assert!(snapshot["defaults"][0]["connectionId"].is_null());
    assert_ne!(
        raw(&store, "nova")["model"]["apiKeyId"],
        raw(&store, "second")["model"]["apiKeyId"]
    );
}
#[test]
fn malformed_oversized_invalid_utf8_profiles_are_preserved_and_isolated() {
    let f = Fixture::new(vec![profile("nova", "one")], &["one"]);
    let db = f.db();
    for (id, text) in [
        ("bad-json", vec![b'{']),
        ("oversized", vec![b'x'; MAX_PROFILE + 1]),
        ("bad-utf8", vec![255]),
    ] {
        db.execute(
            "INSERT INTO bot_profiles VALUES(?1,CAST(?2 AS TEXT))",
            params![id, text],
        )
        .unwrap();
    }
    let before = db
        .prepare("SELECT id,CAST(profile AS BLOB) FROM bot_profiles WHERE id<>'nova' ORDER BY id")
        .unwrap()
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })
        .unwrap()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();
    let store = f.store();
    let snapshot = store.api_snapshot();
    assert_eq!(snapshot["availability"], "ready");
    assert_eq!(snapshot["legacyUnboundCount"], 3);
    assert_eq!(
        snapshot["connections"][0]["writePolicy"],
        "shared_legacy_read_only"
    );
    let after = db
        .prepare("SELECT id,CAST(profile AS BLOB) FROM bot_profiles WHERE id<>'nova' ORDER BY id")
        .unwrap()
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })
        .unwrap()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(before, after);
}
#[test]
fn valid_json_bad_destination_preserves_raw_shared_reference_and_denies_rotation() {
    let mut bad = profile("broken", "one");
    bad["model"]["baseUrl"] = json!("https://user:password@bad.invalid/v1");
    let original = bad.to_string();
    let f = Fixture::new(vec![profile("nova", "one"), bad], &["one"]);
    let store = f.store();
    let snapshot = store.api_snapshot();
    assert_eq!(snapshot["connections"][0]["externalReferences"], true);
    let id = snapshot["connections"][0]["id"].as_str().unwrap();
    assert_eq!(
        store
            .set_api_connection_secret(0, id, "must-not-be-written")
            .unwrap_err(),
        api::SHARED
    );
    assert_eq!(
        f.db()
            .query_row(
                "SELECT profile FROM bot_profiles WHERE id='broken'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        original
    );
}
#[test]
fn oauth_or_mcp_aliases_are_not_silently_rotated() {
    let mut p = profile("nova", "one");
    p["chat"]["youtube"]["sendSecretId"] = json!("one");
    let f = Fixture::new(vec![p], &["one"]);
    let store = f.store();
    let snapshot = store.api_snapshot();
    assert_eq!(snapshot["connections"][0]["externalReferences"], true);
    let id = snapshot["connections"][0]["id"].as_str().unwrap();
    assert_eq!(
        store
            .set_api_connection_secret(0, id, "must-not-be-written")
            .unwrap_err(),
        api::SHARED
    );
    assert_eq!(
        raw(&store, "nova")["chat"]["youtube"]["sendSecretId"],
        "one"
    );
}
#[test]
fn exact_destination_checked_before_broker_and_default_rechecked_after() {
    let f = Fixture::new(
        vec![profile("nova", "one"), profile("other", "two")],
        &["one", "two"],
    );
    let store = f.store();
    let a = raw(&store, "nova");
    let key = store
        .api_key_with(&a["model"], Purpose::Dialog, |id| {
            assert_eq!(id, "one");
            Ok(Some("synthetic-one".into()))
        })
        .unwrap();
    assert_eq!(key.as_deref(), Some("synthetic-one"));
    let mut wrong = a["model"].clone();
    wrong["baseUrl"] = json!("https://proxy.invalid/v1");
    assert!(store
        .api_key_with(&wrong, Purpose::Dialog, |_| panic!(
            "wrong destination read broker"
        ))
        .is_err());
    let snapshot = store.api_snapshot();
    let alias = snapshot["defaults"][0]["id"].as_str().unwrap();
    let binding = Binding::request(&a["model"], Purpose::Dialog)
        .unwrap()
        .unwrap();
    let first = a["model"]["apiKeyId"].as_str().unwrap();
    let second = raw(&store, "other")["model"]["apiKeyId"]
        .as_str()
        .unwrap()
        .to_owned();
    store
        .default_api_connection(0, binding.clone(), Some(first))
        .unwrap();
    let mut config = a["model"].clone();
    config["apiKeyId"] = json!(alias);
    assert!(store
        .api_key_with(&config, Purpose::Dialog, |id| {
            assert_eq!(id, "one");
            store
                .default_api_connection(1, binding.clone(), Some(&second))
                .unwrap();
            Ok(Some("do-not-return-after-change".into()))
        })
        .is_err());
}
#[test]
fn removal_keeps_os_registry_and_invalidates_named_and_default_refs() {
    let f = Fixture::new(vec![profile("nova", "one")], &["one"]);
    let store = f.store();
    let profile = raw(&store, "nova");
    let id = profile["model"]["apiKeyId"].as_str().unwrap();
    let snapshot = store.api_snapshot();
    let alias = snapshot["defaults"][0]["id"].as_str().unwrap();
    store.remove_api_connection(0, id).unwrap();
    assert!(registered(&f.db()).unwrap().contains("one"));
    assert!(store
        .api_key_with(&profile["model"], Purpose::Dialog, |_| panic!(
            "removed connection read broker"
        ))
        .is_err());
    let mut config = profile["model"].clone();
    config["apiKeyId"] = json!(alias);
    assert_eq!(
        store.api_binding(&config, Purpose::Dialog)["reason"],
        "default_missing"
    );
}
#[test]
fn corrupted_registry_fails_closed_but_unused_cli_never_reads() {
    let f = Fixture::new(vec![profile("nova", "one")], &["one"]);
    let store = f.store();
    f.db()
        .execute(
            "UPDATE global_api_connections SET metadata='invalid-marker'",
            [],
        )
        .unwrap();
    assert_eq!(store.api_snapshot()["availability"], "unavailable");
    assert_eq!(store.api_snapshot()["revision"], Value::Null);
    assert_eq!(
        store
            .api_key_with(
                &json!({"provider":"codex","apiKeyId":"one"}),
                Purpose::Dialog,
                |_| panic!("CLI read broker")
            )
            .unwrap(),
        None
    );
    assert!(raw(&store, "nova").is_object());
}

#[test]
fn non_string_managed_references_do_not_become_anonymous_requests() {
    let f = Fixture::new(vec![profile("nova", "one")], &["one"]);
    let store = f.store();
    for value in [json!(12), json!({"reference":"one"}), Value::Null] {
        let mut config = raw(&store, "nova")["model"].clone();
        config["apiKeyId"] = value;
        assert!(store
            .api_key_with(&config, Purpose::Dialog, |_| panic!(
                "invalid reference reached broker"
            ))
            .is_err());
        assert_eq!(
            store.api_binding(&config, Purpose::Dialog)["reason"],
            "invalid_reference"
        );
    }
}
#[test]
fn valid_migration_capacity_exhaustion_rolls_back_every_rewritten_profile() {
    let mut profiles = Vec::new();
    let mut keys = Vec::new();
    for index in 0..820 {
        let id = if index == 0 {
            "nova".to_owned()
        } else {
            format!("bot-{index}")
        };
        let mut p = profile(&id, "");
        p["voice"]["provider"] = json!("elevenlabs");
        for (section, tag) in [
            ("model", "model"),
            ("voice", "voice"),
            ("stt", "stt"),
            ("hindsight", "memory"),
        ] {
            let key = format!("legacy-{index}-{tag}");
            p[section]["apiKeyId"] = json!(key);
            keys.push(key);
        }
        let key = format!("legacy-{index}-youtube");
        p["chat"]["youtube"]["apiKeyId"] = json!(key);
        keys.push(key);
        profiles.push(p);
    }
    let original = profiles[0].to_string();
    let refs = keys.iter().map(String::as_str).collect::<Vec<_>>();
    let f = Fixture::new(profiles, &refs);
    let store = f.store();
    let snapshot = store.api_snapshot();
    assert_eq!(snapshot["availability"], "unavailable");
    assert_eq!(snapshot["reason"], "migration_incomplete");
    let db = f.db();
    assert_eq!(
        db.query_row(
            "SELECT profile FROM bot_profiles WHERE id='nova'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        original
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='global_api_connections'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM secret_ids", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        4100
    );
}

#[tokio::test]
async fn stt_scope_resolution_and_actual_multipart_transport_share_destination_and_header() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for provider in ["openai", "elevenlabs"] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/tenant/v1", listener.local_addr().unwrap());
        let suffix = if provider == "elevenlabs" {
            "speech-to-text"
        } else {
            "audio/transcriptions"
        };
        let mut p = profile("nova", "synthetic-legacy-stt");
        p["stt"]["provider"] = json!(provider);
        p["stt"]["baseUrl"] = json!(format!("{base}/{suffix}///"));
        let fixture = Fixture::new(vec![p], &["synthetic-legacy-stt"]);
        let store = fixture.store();
        let p = raw(&store, "nova");
        let config = &p["stt"];
        let reads = AtomicUsize::new(0);
        let key = store
            .api_key_with(config, Purpose::Transcription, |id| {
                assert_eq!(id, "synthetic-legacy-stt");
                reads.fetch_add(1, Ordering::SeqCst);
                Ok(Some("synthetic-resolved-stt".into()))
            })
            .unwrap();
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        let server = tokio::spawn(async move {
            tokio::time::timeout(std::time::Duration::from_secs(5),async move{
            let(mut socket,_)=listener.accept().await.unwrap();let mut bytes=Vec::new();loop{let mut chunk=[0;4096];let n=socket.read(&mut chunk).await.unwrap();assert_ne!(n,0);bytes.extend_from_slice(&chunk[..n]);assert!(bytes.len()<65536);if let Some(end)=bytes.windows(4).position(|w|w==b"\r\n\r\n"){let headers=String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();let size=headers.lines().find_map(|line|line.strip_prefix("content-length:")).unwrap().trim().parse::<usize>().unwrap();if bytes.len()>=end+4+size{break;}}}
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 16\r\nConnection: close\r\n\r\n{\"text\":\"heard\"}").await.unwrap();String::from_utf8(bytes).unwrap()
        }).await.unwrap()
        });
        assert_eq!(
            crate::transcription::transcribe(config, "UklGRg==", "audio/wav", key)
                .await
                .unwrap(),
            "heard"
        );
        let request = server.await.unwrap();
        assert!(request.starts_with(&format!("POST /tenant/v1/{suffix} HTTP/1.1\r\n")));
        let header = if provider == "elevenlabs" {
            "xi-api-key: synthetic-resolved-stt"
        } else {
            "authorization: bearer synthetic-resolved-stt"
        };
        assert!(request.to_ascii_lowercase().contains(header));
        let mut wrong = config.clone();
        wrong["baseUrl"] = json!(format!("{base}/wrong"));
        assert!(store
            .api_key_with(&wrong, Purpose::Transcription, |_| panic!(
                "wrong destination reached secret backend"
            ))
            .is_err());
    }
}
