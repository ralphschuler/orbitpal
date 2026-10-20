use super::*;
use serde_json::json;
#[test]
fn openai_dialog_and_full_stt_endpoint_share_the_same_binding() {
    let a = Binding::request(
        &json!({"provider":"openai","baseUrl":"https://API.OPENAI.COM:443/v1/"}),
        Purpose::Dialog,
    )
    .unwrap();
    let b = Binding::request(
        &json!({"provider":"openai","baseUrl":"https://api.openai.com/v1/audio/transcriptions/"}),
        Purpose::Transcription,
    )
    .unwrap();
    assert_eq!(a, b);
    assert_eq!(a.unwrap().service, Service::Openai);
}
#[test]
fn elevenlabs_speech_and_transcription_share_header_scoped_account() {
    assert_eq!(Binding::request(&json!({"provider":"elevenlabs"}),Purpose::Speech).unwrap(),Binding::request(&json!({"provider":"elevenlabs","baseUrl":"https://api.elevenlabs.io/v1/speech-to-text"}),Purpose::Transcription).unwrap());
}
#[test]
fn custom_destinations_ports_paths_and_auth_headers_remain_distinct() {
    let official = Binding::request(&json!({"provider":"openai"}), Purpose::Dialog)
        .unwrap()
        .unwrap();
    for url in [
        "https://proxy.invalid/v1",
        "https://api.openai.com:444/v1",
        "https://api.openai.com/v2",
    ] {
        let binding =
            Binding::request(&json!({"provider":"openai","baseUrl":url}), Purpose::Dialog)
                .unwrap()
                .unwrap();
        assert_ne!(binding, official);
        assert_eq!(binding.service, Service::Compatible);
    }
    assert_ne!(
        Binding::request(
            &json!({"provider":"anthropic","baseUrl":"https://proxy.invalid/v1"}),
            Purpose::Dialog
        )
        .unwrap(),
        Binding::request(
            &json!({"provider":"compatible","baseUrl":"https://proxy.invalid/v1"}),
            Purpose::Dialog
        )
        .unwrap()
    );
}
#[test]
fn invalid_urls_controls_and_local_backend_fail_before_resolution() {
    for url in [
        "https://user:secret@api.openai.com/v1",
        "https://api.openai.com/v1?q=secret",
        "https://api.openai.com/v1#secret",
        "\nhttps://api.openai.com/v1\n",
        "file:///private",
    ] {
        assert!(
            Binding::request(&json!({"provider":"openai","baseUrl":url}), Purpose::Dialog).is_err()
        );
    }
    assert!(Binding::request(&json!({"provider":"whisper-local","baseUrl":"http://127.0.0.1:8080","localBackend":"invalid"}),Purpose::Transcription).is_err());
    assert!(Binding::request(
        &json!({"provider":"whisper-local","baseUrl":"https://remote.invalid"}),
        Purpose::Transcription
    )
    .is_err());
}
#[test]
fn actual_hindsight_and_stt_builders_consume_the_canonical_trailing_slashes() {
    let config = json!({"baseUrl":"http://127.0.0.1:8888/prefix///"});
    assert_eq!(hindsight_base(&config).unwrap().path(), "/prefix");
    let config =
        json!({"provider":"openai","baseUrl":"https://api.openai.com/v1/audio/transcriptions///"});
    assert_eq!(
        transcription_endpoint(&config, "openai", false)
            .unwrap()
            .path(),
        "/v1/audio/transcriptions"
    );
}
#[test]
fn unused_integrations_have_no_destination_and_youtube_is_fixed() {
    for provider in ["codex", "claude-code"] {
        assert_eq!(
            Binding::request(
                &json!({"provider":provider,"baseUrl":"malformed"}),
                Purpose::Dialog
            )
            .unwrap(),
            None
        );
    }
    assert_eq!(
        Binding::request(&json!({"provider":"system"}), Purpose::Speech).unwrap(),
        None
    );
    let youtube = Binding::request(
        &json!({"baseUrl":"https://attacker.invalid"}),
        Purpose::YoutubeData,
    )
    .unwrap()
    .unwrap();
    assert_eq!(youtube.destination, "https://www.googleapis.com/youtube/v3");
}
