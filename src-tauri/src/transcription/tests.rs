use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// One disposable loopback endpoint, with bounded request/response lifetime.
/// The synthetic marker is never an OS credential or a provider account.
async fn endpoint_once(status: &str, extra: &str) -> (String, tokio::task::JoinHandle<String>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let response=format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: 16\r\nConnection: close\r\n{extra}\r\n{{\"text\":\"heard\"}}");
    let task = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(5), async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut part = [0; 4096];
                let count = socket.read(&mut part).await.unwrap();
                assert_ne!(count, 0);
                bytes.extend_from_slice(&part[..count]);
                assert!(bytes.len() < 65536);
                if let Some(header_end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers =
                        String::from_utf8_lossy(&bytes[..header_end]).to_ascii_lowercase();
                    let length = headers
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:").map(str::trim))
                        .unwrap()
                        .parse::<usize>()
                        .unwrap();
                    if bytes.len() >= header_end + 4 + length {
                        break;
                    }
                }
            }
            socket.write_all(response.as_bytes()).await.unwrap();
            String::from_utf8(bytes).unwrap()
        })
        .await
        .unwrap()
    });
    (url, task)
}
#[tokio::test]
async fn multipart_wire_uses_exact_scoped_path_and_bearer_for_full_openai_endpoint() {
    let (base, wire) = endpoint_once("200 OK", "").await;
    let config = serde_json::json!({"provider":"openai","baseUrl":format!("{base}/tenant/v1/audio/transcriptions///"),"model":"whisper-1","language":"de"});
    assert_eq!(
        transcribe(
            &config,
            "UklGRg==",
            "audio/wav",
            Some("synthetic-stt-marker".into())
        )
        .await
        .unwrap(),
        "heard"
    );
    let request = wire.await.unwrap();
    let lower = request.to_ascii_lowercase();
    assert!(request.starts_with("POST /tenant/v1/audio/transcriptions HTTP/1.1\r\n"));
    assert!(lower.contains("authorization: bearer synthetic-stt-marker\r\n"));
    assert!(!lower.contains("xi-api-key:"));
    assert!(request.contains("name=\"model\"\r\n\r\nwhisper-1"));
    assert!(request.contains("filename=\"microphone.wav\""));
}
#[tokio::test]
async fn elevenlabs_wire_uses_xi_header_and_shared_full_endpoint() {
    let (base, wire) = endpoint_once("200 OK", "").await;
    let config = serde_json::json!({"provider":"elevenlabs","baseUrl":format!("{base}/team/v1/speech-to-text///"),"model":"scribe_v2","language":"de"});
    transcribe(
        &config,
        "UklGRg==",
        "audio/wav",
        Some("synthetic-eleven-marker".into()),
    )
    .await
    .unwrap();
    let request = wire.await.unwrap();
    let lower = request.to_ascii_lowercase();
    assert!(request.starts_with("POST /team/v1/speech-to-text HTTP/1.1\r\n"));
    assert!(lower.contains("xi-api-key: synthetic-eleven-marker\r\n"));
    assert!(!lower.contains("authorization:"));
    assert!(request.contains("name=\"model_id\"\r\n\r\nscribe_v2"));
    assert!(request.contains("name=\"language_code\"\r\n\r\ndeu"));
}
#[tokio::test]
async fn authenticated_local_whisper_uses_its_real_backend_route() {
    for backend in ["whispercpp", "compatible"] {
        let (base, wire) = endpoint_once("200 OK", "").await;
        let config = serde_json::json!({"provider":"whisper-local","localBackend":backend,"baseUrl":base,"language":"auto"});
        transcribe(
            &config,
            "UklGRg==",
            "audio/wav",
            Some("synthetic-local-marker".into()),
        )
        .await
        .unwrap();
        let request = wire.await.unwrap();
        let path = if backend == "whispercpp" {
            "/inference"
        } else {
            "/v1/audio/transcriptions"
        };
        assert!(request.starts_with(&format!("POST {path} HTTP/1.1\r\n")));
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: bearer synthetic-local-marker\r\n"));
    }
}
#[tokio::test]
async fn redirect_does_not_forward_audio_or_auth_to_another_destination() {
    let sink = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let location = format!("Location: http://{}/leak\r\n", sink.local_addr().unwrap());
    let (base, wire) = endpoint_once("302 Found", &location).await;
    let config = serde_json::json!({"provider":"openai","baseUrl":format!("{base}/v1")});
    let error = transcribe(
        &config,
        "UklGRg==",
        "audio/wav",
        Some("synthetic-redirect-marker".into()),
    )
    .await
    .unwrap_err();
    assert!(error.contains("HTTP 302"));
    assert!(wire
        .await
        .unwrap()
        .to_ascii_lowercase()
        .contains("authorization: bearer synthetic-redirect-marker\r\n"));
    assert!(
        tokio::time::timeout(Duration::from_millis(200), sink.accept())
            .await
            .is_err()
    );
}
