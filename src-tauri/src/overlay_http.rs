//! Browser policy for the loopback OBS frontend. This is deliberately separate
//! from the native WebView's IPC/file permissions and needs no dynamic hosts.
use axum::{extract::Request, http::{header, HeaderValue}, middleware::{self, Next}, response::Response, Router};

const OBS_CSP: &str = concat!(
    "default-src 'none'; ",
    "script-src 'self' 'wasm-unsafe-eval'; ",
    // Svelte uses inline style properties for the avatar/subtitle presentation.
    "style-src 'self' 'unsafe-inline'; ",
    "img-src 'self' data: blob:; ",
    "font-src 'self' data:; ",
    "media-src 'self' data: blob:; ",
    // FileLoader fetches embedded data/blob buffers; SSE and audio stay on this
    // exact origin. No broad loopback host/port, WebSocket or native IPC grant.
    "connect-src 'self' data: blob:; ",
    "base-uri 'none'; object-src 'none'; frame-src 'none'; ",
    "worker-src blob:; form-action 'none'"
);

/// Apply to every response, including HTML, assets, SSE and authorization errors.
/// Existing MIME, cache, CORS, status and streaming bodies remain untouched.
pub fn with_security(router: Router) -> Router {
    router.layer(middleware::from_fn(security_headers))
}
async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers=response.headers_mut();
    headers.insert(header::CONTENT_SECURITY_POLICY,HeaderValue::from_static(OBS_CSP));
    headers.insert(header::REFERRER_POLICY,HeaderValue::from_static("no-referrer"));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS,HeaderValue::from_static("nosniff"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::StatusCode, response::sse::{Event,Sse}, routing::{get,post}, Json};
    use serde_json::{json,Value};
    use std::convert::Infallible;

    async fn fixture() -> (String,tokio::task::JoinHandle<()>) {
        let router=Router::new()
            .route("/overlay",get(||async {([(header::CONTENT_TYPE,"text/html; charset=utf-8")],"<!doctype html><script type=module src=/assets/app.js></script>")}))
            .route("/assets/app.js",get(||async {([(header::CONTENT_TYPE,"application/javascript")],"window.fixtureLoaded=true;")}))
            .route("/assets/avatar.glb",get(||async {([(header::CONTENT_TYPE,"model/gltf-binary"),(header::CACHE_CONTROL,"public, max-age=3600"),(header::ACCESS_CONTROL_ALLOW_ORIGIN,"*")],b"glTF fixture bytes".as_slice())}))
            .route("/speech/fixture",get(||async {Json(json!({"mimeType":"audio/wav","base64":"Zml4dHVyZQ=="}))}))
            .route("/emote-sound/fixture",get(||async {Json(json!({"mimeType":"audio/wav","base64":"Zml4dHVyZQ=="}))}))
            .route("/events",get(||async {Sse::new(tokio_stream::iter([Ok::<_,Infallible>(Event::default().data("{\"kind\":\"fixture\"}"))]))}))
            .route("/playback",post(|Json(body):Json<Value>|async move {Json(body)}))
            .route("/private",get(||async {StatusCode::UNAUTHORIZED}));
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address=format!("http://{}",listener.local_addr().unwrap());
        let task=tokio::spawn(async move {axum::serve(listener,with_security(router)).await.unwrap();});
        (address,task)
    }
    fn headers(response:&reqwest::Response) {
        assert_eq!(response.headers()[header::CONTENT_SECURITY_POLICY],OBS_CSP);
        assert_eq!(response.headers()[header::REFERRER_POLICY],"no-referrer");
        assert_eq!(response.headers()[header::X_CONTENT_TYPE_OPTIONS],"nosniff");
    }
    #[tokio::test]
    async fn loopback_html_scripts_avatars_audio_and_errors_keep_their_headers_and_bodies() {
        let (address,server)=fixture().await;let client=reqwest::Client::builder().no_proxy().timeout(std::time::Duration::from_secs(5)).build().unwrap();
        for (path,mime) in [("/overlay","text/html; charset=utf-8"),("/assets/app.js","application/javascript"),("/assets/avatar.glb","model/gltf-binary"),("/speech/fixture","application/json"),("/emote-sound/fixture","application/json")] {
            let response=client.get(format!("{address}{path}?session=fixture-only")).send().await.unwrap();
            assert_eq!(response.status(),StatusCode::OK);headers(&response);assert_eq!(response.headers()[header::CONTENT_TYPE],mime);
            if path.ends_with(".glb") {assert_eq!(response.headers()[header::CACHE_CONTROL],"public, max-age=3600");assert_eq!(response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],"*");assert_eq!(response.bytes().await.unwrap(),b"glTF fixture bytes".as_slice());}
            else {assert!(!response.bytes().await.unwrap().is_empty());}
        }
        for (path,status) in [("/private",StatusCode::UNAUTHORIZED),("/missing",StatusCode::NOT_FOUND)] {
            let response=client.get(format!("{address}{path}")).send().await.unwrap();assert_eq!(response.status(),status);headers(&response);
        }
        server.abort();
    }
    #[tokio::test]
    async fn same_origin_sse_and_playback_posts_remain_streamable_and_unchanged() {
        let (address,server)=fixture().await;let client=reqwest::Client::builder().no_proxy().timeout(std::time::Duration::from_secs(5)).build().unwrap();
        let response=client.get(format!("{address}/events?session=fixture-only&bot=nova")).send().await.unwrap();headers(&response);
        assert_eq!(response.headers()[header::CONTENT_TYPE],"text/event-stream");assert_eq!(response.text().await.unwrap(),"data: {\"kind\":\"fixture\"}\n\n");
        let payload=json!({"id":"fixture","botId":"nova","speaking":true});
        let response=client.post(format!("{address}/playback?session=fixture-only&bot=nova")).json(&payload).send().await.unwrap();headers(&response);assert_eq!(response.json::<Value>().await.unwrap(),payload);
        server.abort();
    }
    #[test]
    fn browser_policy_has_only_reviewed_sources_and_never_adds_native_or_external_grants() {
        let directives:std::collections::BTreeMap<_,_>=OBS_CSP.split(';').map(str::trim).filter(|part|!part.is_empty()).map(|part|{let mut words=part.split_whitespace();(words.next().unwrap(),words.collect::<Vec<_>>())}).collect();
        assert_eq!(directives["default-src"],["'none'"]);assert_eq!(directives["script-src"],["'self'","'wasm-unsafe-eval'"]);
        assert_eq!(directives["connect-src"],["'self'","data:","blob:"]);
        assert_eq!(directives["media-src"],["'self'","data:","blob:"]);
        for disabled in ["base-uri","object-src","frame-src","form-action"]{assert_eq!(directives[disabled],["'none'"]);}
        assert_eq!(directives["worker-src"],["blob:"]);
        for sources in directives.values(){for source in sources{assert!(matches!(*source,"'self'"|"'none'"|"'unsafe-inline'"|"'wasm-unsafe-eval'"|"data:"|"blob:"));}}
        assert!(!directives["script-src"].contains(&"'unsafe-inline'"));assert!(!directives["script-src"].contains(&"'unsafe-eval'"));
    }
}
