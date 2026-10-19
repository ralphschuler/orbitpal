//! OrbitPal's versioned subset of the pinned, unversioned MSFT audio draft.
//! Inspection never fetches resources or starts playback.
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const EXTENSION: &str = "MSFT_audio_emitter";
pub const PROFILE: &str = "orbitpal-msft-audio-v1";
pub const MAX_CLIP_BYTES: u64 = 5 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 16 * 1024 * 1024;
const UNSUPPORTED: &str = "Modell-Audio verwendet Funktionen oder eine Version außerhalb des unterstützten MSFT-Draft-Profils.";
const REFERENCE: &str = "Modell-Audio enthält ungültige Clip-, Puffer-, Emitter- oder Animationsreferenzen.";
const RESOURCE: &str = "Modell-Audio muss als WAV-Daten-URI oder eingebetteter bufferView vorliegen; externe Ressourcen sind nicht erlaubt.";
const BUDGET: &str = "Modell-Audio überschreitet das Profilbudget: 8 Clips, 4 Emitter, 128 Ereignisse, 5 MiB pro Clip und 16 MiB insgesamt.";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioSummary { pub profile: String, pub clips: usize, pub events: usize }

fn index(value: &Value, length: usize) -> Result<usize, String> {
    value.as_u64().and_then(|n| usize::try_from(n).ok()).filter(|n| *n < length).ok_or_else(|| REFERENCE.into())
}
fn fields(value: &Value, allowed: &[&str]) -> Result<(), String> {
    let fields = value.as_object().ok_or(REFERENCE)?;
    if fields.keys().any(|name| !allowed.contains(&name.as_str())) { return Err(UNSUPPORTED.into()); }
    Ok(())
}
fn finite(value: &Value, default: f64, maximum: f64) -> Result<f64, String> {
    let number = if value.is_null() { default } else { value.as_f64().ok_or(REFERENCE)? };
    if !number.is_finite() || number < 0.0 || number > maximum { return Err(REFERENCE.into()); }
    Ok(number)
}

pub fn inspect(document: &Value, binary_length: Option<u64>) -> Result<Option<AudioSummary>, String> {
    let Some(extension) = document["extensions"].get(EXTENSION) else { return Ok(None); };
    // The source draft has no specVersion field. Unknown revisions/features must
    // not be interpreted as this pinned profile by guessing at their meaning.
    fields(extension, &["clips", "emitters", "extras"])?;
    let clips = extension["clips"].as_array().ok_or(REFERENCE)?;
    let emitters = extension["emitters"].as_array().ok_or(REFERENCE)?;
    if clips.is_empty() || clips.len() > 8 || emitters.is_empty() || emitters.len() > 4 { return Err(BUDGET.into()); }
    let mut total = 0u64;
    for clip in clips {
        fields(clip, &["uri", "bufferView", "mimeType", "extras"])?;
        if clip["mimeType"].as_str().is_some_and(|mime| mime != "audio/wav") { return Err(RESOURCE.into()); }
        let size = if let Some(uri) = clip["uri"].as_str() {
            if !clip["bufferView"].is_null() { return Err(REFERENCE.into()); }
            let encoded = uri.strip_prefix("data:audio/wav;base64,").ok_or(RESOURCE)?;
            if encoded.len() as u64 > MAX_CLIP_BYTES.div_ceil(3) * 4 { return Err(BUDGET.into()); }
            let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).map_err(|_| RESOURCE.to_owned())?;
            bytes.len() as u64
        } else {
            if !clip["uri"].is_null() || clip["mimeType"].as_str() != Some("audio/wav") { return Err(RESOURCE.into()); }
            let views = document["bufferViews"].as_array().ok_or(REFERENCE)?;
            let view = &views[index(&clip["bufferView"], views.len())?];
            if view.get("extensions").is_some() { return Err(UNSUPPORTED.into()); }
            let buffers = document["buffers"].as_array().ok_or(REFERENCE)?;
            let buffer_index = index(&view["buffer"], buffers.len())?;
            let buffer = &buffers[buffer_index];
            let length = buffer["byteLength"].as_u64().ok_or(REFERENCE)?;
            if let Some(uri) = buffer["uri"].as_str() {
                if !uri.starts_with("data:application/octet-stream;base64,") && !uri.starts_with("data:application/gltf-buffer;base64,") { return Err(RESOURCE.into()); }
            } else if buffer_index != 0 || binary_length.is_none_or(|size| length > size || size - length > 3) { return Err(RESOURCE.into()); }
            let offset = if view["byteOffset"].is_null() { 0 } else { view["byteOffset"].as_u64().ok_or(REFERENCE)? };
            let size = view["byteLength"].as_u64().ok_or(REFERENCE)?;
            if offset.checked_add(size).is_none_or(|end| end > length) { return Err(REFERENCE.into()); }
            size
        };
        if size == 0 || size > MAX_CLIP_BYTES { return Err(BUDGET.into()); }
        total = total.checked_add(size).ok_or(BUDGET)?;
        if total > MAX_TOTAL_BYTES { return Err(BUDGET.into()); }
    }
    for emitter in emitters {
        fields(emitter, &["name", "clips", "volume", "loop", "extras"])?;
        if !emitter["loop"].is_null() && emitter["loop"].as_bool() != Some(false) { return Err(UNSUPPORTED.into()); }
        finite(&emitter["volume"], 1.0, 1.0)?;
        let references = emitter["clips"].as_array().ok_or(REFERENCE)?;
        if references.len() != 1 { return Err(UNSUPPORTED.into()); }
        fields(&references[0], &["clip", "weight"])?;
        index(&references[0]["clip"], clips.len())?;
        if finite(&references[0]["weight"], 1.0, 1.0)? != 1.0 { return Err(UNSUPPORTED.into()); }
    }
    // Positional/ambient emitters have no representation in the cohost's output
    // routing. Only explicit animation events authorize a sound presentation.
    for kind in ["nodes", "scenes"] {
        if document[kind].as_array().is_some_and(|items| items.iter().any(|item| item["extensions"].get(EXTENSION).is_some())) { return Err(UNSUPPORTED.into()); }
    }
    let mut events = 0usize;
    for animation in document["animations"].as_array().into_iter().flatten() {
        let Some(extension) = animation["extensions"].get(EXTENSION) else { continue; };
        fields(extension, &["events", "extras"])?;
        let items = extension["events"].as_array().ok_or(REFERENCE)?;
        events += items.len();
        if events > 128 { return Err(BUDGET.into()); }
        let mut previous = -1.0;
        for event in items {
            fields(event, &["time", "action", "emitter", "startOffset", "extras"])?;
            if !matches!(event["action"].as_str(), Some("play" | "stop")) { return Err(UNSUPPORTED.into()); }
            let time = event["time"].as_f64().ok_or(REFERENCE)?;
            if !time.is_finite() || !(0.0..=120.0).contains(&time) || time < previous { return Err(REFERENCE.into()); }
            previous = time;
            finite(&event["startOffset"], 0.0, 15.0)?;
            index(&event["emitter"], emitters.len())?;
        }
    }
    if events == 0 { return Err(UNSUPPORTED.into()); }
    Ok(Some(AudioSummary { profile: PROFILE.into(), clips: clips.len(), events }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn model() -> Value { json!({
        "extensions":{EXTENSION:{"clips":[{"bufferView":0,"mimeType":"audio/wav"}],"emitters":[{"clips":[{"clip":0}]}]}},
        "buffers":[{"byteLength":48}],"bufferViews":[{"buffer":0,"byteLength":48}],
        "animations":[{"extensions":{EXTENSION:{"events":[{"time":0,"action":"play","emitter":0},{"time":1,"action":"stop","emitter":0}]}}}]
    }) }
    #[test]
    fn versioned_subset_accepts_only_embedded_animation_audio() {
        let value = model(); assert_eq!(inspect(&value,Some(48)).unwrap().unwrap().profile,PROFILE);
        for patch in [json!({"specVersion":"1.0"}),json!({"version":"2"}),json!({"extensions":{}})] {
            let mut value = model(); value["extensions"][EXTENSION].as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());
            assert!(inspect(&value,Some(48)).unwrap_err().contains("Version"));
        }
        for action in ["pause","autoplay","unknown"] { let mut value=model();value["animations"][0]["extensions"][EXTENSION]["events"][0]["action"]=json!(action);assert!(inspect(&value,Some(48)).is_err()); }
        let mut value=model();value["nodes"]=json!([{"extensions":{EXTENSION:{"emitters":[0]}}}]);assert!(inspect(&value,Some(48)).is_err());
    }
    #[test]
    fn invalid_references_external_resources_and_audio_budgets_fail_closed() {
        for clip in [json!({"uri":"https://example.invalid/a.wav"}),json!({"uri":"../a.wav"}),json!({"uri":"file:///a.wav"}),json!({"uri":"data:audio/mpeg;base64,AAAA"}),json!({"bufferView":99,"mimeType":"audio/wav"}),json!({"bufferView":0,"mimeType":"audio/wav","uri":"data:audio/wav;base64,AAAA"})] {
            let mut value=model();value["extensions"][EXTENSION]["clips"][0]=clip;assert!(inspect(&value,Some(48)).is_err());
        }
        let mut value=model();value["bufferViews"][0]["byteLength"]=json!(MAX_CLIP_BYTES+1);value["buffers"][0]["byteLength"]=json!(MAX_CLIP_BYTES+1);assert!(inspect(&value,Some(MAX_CLIP_BYTES+1)).unwrap_err().contains("Profilbudget"));
        let mut value=model();value["extensions"][EXTENSION]["emitters"][0]["clips"][0]["clip"]=json!(4);assert!(inspect(&value,Some(48)).is_err());
        let mut value=model();value["bufferViews"][0]["byteOffset"]=json!(u64::MAX);assert!(inspect(&value,Some(48)).is_err());
        let mut value=model();value["animations"][0]["extensions"][EXTENSION]["events"][0]["time"]=json!(-1);assert!(inspect(&value,Some(48)).is_err());
    }
}
