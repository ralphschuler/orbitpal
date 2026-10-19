// SPDX-License-Identifier: MIT
//! One host-owned action catalogue for dispatch, tools and the editor.
//! Character/profile declarations never create an animation that is not present.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeSet, path::Path};
use crate::avatar_metadata::AvatarInfo;

const UNAVAILABLE: &str = "Diese Emote-Aktion ist für die aktuelle Figur nicht verfügbar.";
const AMBIGUOUS: &str = "Diese Emote-Aktion ist nicht eindeutig. Bitte einen eindeutigen Clipnamen auswählen.";
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub action: String,
    pub name: String,
    pub kind: &'static str,
    pub duration: Option<f64>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub bot_id: String,
    pub avatar_binding: Value,
    pub actions: Vec<Action>,
}

/// Imported assets/motions use immutable managed UUID names. This binding also
/// changes on bot/asset/motion assignment changes, without trusting UI metadata.
pub fn binding(profile: &Value) -> Value {
    json!([profile["id"], profile["avatar"]["assetPath"].as_str().unwrap_or("nova"),
        profile["motions"].as_array().into_iter().flatten().map(|motion| motion["id"].clone()).collect::<Vec<_>>()])
}
fn aliases(action: &str) -> &'static [&'static str] {
    match action { "wave" => &["wave"], "celebrate" => &["cheer","jump","jump_full_short"], "nod" => &["yes"], "dance" => &["dance"], _ => &[] }
}
pub fn resolve<'a>(capabilities: &'a Capabilities, action: &str) -> Result<&'a Action, String> {
    if action.is_empty() || action.chars().count() > 128 || action.chars().any(char::is_control) { return Err(UNAVAILABLE.into()); }
    let exact: Vec<_> = capabilities.actions.iter().filter(|item| item.action == action).collect();
    if exact.len() == 1 { return Ok(exact[0]); }
    if exact.len() > 1 { return Err(AMBIGUOUS.into()); }
    let insensitive: Vec<_> = capabilities.actions.iter().filter(|item| item.action.to_lowercase() == action.to_lowercase()).collect();
    if insensitive.len() == 1 { return Ok(insensitive[0]); }
    if insensitive.len() > 1 { return Err(AMBIGUOUS.into()); }
    for alias in aliases(action) {
        let matches: Vec<_> = capabilities.actions.iter().filter(|item| item.kind == "clip" && item.action.to_lowercase() == *alias).collect();
        if matches.len() == 1 { return Ok(matches[0]); }
        if matches.len() > 1 { return Err(AMBIGUOUS.into()); }
    }
    Err(UNAVAILABLE.into())
}
fn usable_name(name: &str) -> bool { !name.trim().is_empty() && name.chars().count() <= 128 && !name.chars().any(char::is_control) }

fn from_info(profile: &Value, info: &AvatarInfo) -> Capabilities {
    let mut actions = Vec::new();
    for clip in &info.animations {
        if usable_name(&clip.name) && info.animations.iter().filter(|other| other.name == clip.name).count() == 1
            && clip.duration.is_some_and(|duration| duration.is_finite() && duration > 0.0) {
            actions.push(Action { action: clip.name.clone(), name: clip.name.clone(), kind: "clip", duration: clip.duration });
        }
    }
    let bones: BTreeSet<_> = info.bones.iter().map(String::as_str).collect();
    let has = |names: &[&str]| names.iter().all(|name| bones.contains(name));
    let arms = has(&["leftUpperArm","leftLowerArm","rightUpperArm","rightLowerArm"]);
    let vrm = matches!(info.format.as_str(), "vrm0" | "vrm1");
    for (action, name, supported) in [
        ("wave","Winken",arms), ("celebrate","Freuen",arms), ("nod","Nicken",has(&["head"])),
        ("dance","Tanzen",arms && has(&["spine"])), ("hide","Verstecken",true),
    ] {
        // Generic GLB skeleton heuristics are not proof of a procedural gesture.
        // Their authored clips remain available; every visible model can hide.
        let supported = info.format == "builtin" || action == "hide" || vrm && supported;
        let collides = info.animations.iter().any(|clip| clip.name.to_lowercase() == action
            || aliases(action).iter().any(|alias| clip.name.to_lowercase() == *alias));
        if supported && !collides { actions.push(Action { action: action.into(), name: name.into(), kind: "gesture", duration: None }); }
    }
    Capabilities { bot_id: profile["id"].as_str().unwrap_or_default().into(), avatar_binding: binding(profile), actions }
}

#[derive(Default, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all="camelCase")]
struct Target { bones: Vec<String>, expressions: Vec<String>, look_at: bool, clips:Vec<usize>, names:Vec<String> }
fn preset(name: &str) -> &str {
    match name {"a"=>"aa","i"=>"ih","u"=>"ou","e"=>"ee","o"=>"oh","joy"=>"happy","sorrow"=>"sad","fun"=>"relaxed","lookup"=>"lookUp","lookdown"=>"lookDown","lookleft"=>"lookLeft","lookright"=>"lookRight","blink_l"=>"blinkLeft","blink_r"=>"blinkRight",_=>name}
}
fn target(document: &Value) -> Target {
    let legacy=document["extensions"].get("VRM");
    let modern=document["extensions"].get("VRMC_vrm");
    let names=document["animations"].as_array().into_iter().flatten().enumerate().map(|(index,animation)|animation["name"].as_str().filter(|name|!name.is_empty()).map(str::to_owned).unwrap_or_else(||format!("animation_{index}"))).collect();
    let clips=document["animations"].as_array().into_iter().flatten().enumerate().filter(|(_,animation)|animation["channels"].as_array().into_iter().flatten().any(|channel|{
        let valid_node=channel["target"]["node"].as_u64().is_some_and(|node|document["nodes"].get(node as usize).is_some());
        let valid_path=channel["target"]["path"].as_str().is_some_and(|path|["translation","rotation","scale","weights"].contains(&path));
        let valid_sampler=channel["sampler"].as_u64().and_then(|index|animation["samplers"].get(index as usize)).is_some_and(|sampler|["input","output"].iter().all(|key|sampler[*key].as_u64().is_some_and(|index|document["accessors"].get(index as usize).is_some())));
        valid_node&&valid_path&&valid_sampler
    })).map(|(index,_)|index).collect();
    let Some(vrm)=legacy.or(modern) else { return Target{clips,names,..Target::default()}; };
    let nodes=document["nodes"].as_array().map(Vec::as_slice).unwrap_or_default();
    let pairs:Vec<(&str,usize)>=if legacy.is_some(){vrm["humanoid"]["humanBones"].as_array().into_iter().flatten().filter_map(|bone|Some((bone["bone"].as_str()?,bone["node"].as_u64()? as usize))).collect()}
        else{vrm["humanoid"]["humanBones"].as_object().into_iter().flatten().filter_map(|(name,bone)|Some((name.as_str(),bone["node"].as_u64()? as usize))).collect()};
    // Neither a fabricated name nor two humanoid bones mapped to the same node
    // establishes a usable target. The renderer rechecks its actual loaded rig.
    let bones:Vec<String>=pairs.iter().filter(|(name,index)|*index<nodes.len()&&pairs.iter().filter(|(n,i)|n==name||i==index).count()==1).map(|(name,_)|(*name).into()).collect::<BTreeSet<_>>().into_iter().collect();
    let morph=|mesh:&Value,index:&Value|->bool{
        let (Some(mesh),Some(index))=(mesh.as_u64(),index.as_u64()) else{return false};
        document["meshes"].get(mesh as usize).is_some_and(|mesh|mesh["primitives"].as_array().into_iter().flatten().any(|primitive|primitive["targets"].get(index as usize).is_some()))
    };
    let mut expressions=BTreeSet::new();
    if legacy.is_some(){
        for group in vrm["blendShapeMaster"]["blendShapeGroups"].as_array().into_iter().flatten(){
            let works=group["binds"].as_array().into_iter().flatten().any(|bind|bind["weight"].as_f64().is_some_and(|weight|weight.is_finite()&&weight!=0.0)&&morph(&bind["mesh"],&bind["index"]))
                ||group["materialValues"].as_array().into_iter().flatten().any(|bind|bind["materialName"].as_str().is_some_and(|name|document["materials"].as_array().into_iter().flatten().any(|material|material["name"].as_str()==Some(name)))&&bind["propertyName"].is_string()&&bind["targetValue"].as_array().is_some_and(|values|!values.is_empty()));
            if works{let name=group["presetName"].as_str().filter(|name|*name!="unknown").map(preset).or_else(||group["name"].as_str()).unwrap_or_default();if !name.is_empty(){expressions.insert(name.to_owned());}}
        }
    }else{
        for kind in ["preset","custom"]{for (name,expression) in vrm["expressions"][kind].as_object().into_iter().flatten(){
            let works=expression["morphTargetBinds"].as_array().into_iter().flatten().any(|bind|bind["weight"].as_f64().is_some_and(|weight|weight.is_finite()&&weight!=0.0)&&bind["node"].as_u64().and_then(|index|nodes.get(index as usize)).is_some_and(|node|morph(&node["mesh"],&bind["index"])))
                ||["materialColorBinds","textureTransformBinds"].iter().any(|kind|expression[*kind].as_array().into_iter().flatten().any(|bind|bind["material"].as_u64().is_some_and(|index|document["materials"].get(index as usize).is_some())));
            if works{expressions.insert(name.clone());}
        }}
    }
    let expression_look=if legacy.is_some(){vrm["firstPerson"]["lookAtTypeName"]=="BlendShape"}else{vrm["lookAt"]["type"]=="expression"};
    let has_look=if legacy.is_some(){vrm["firstPerson"].is_object()}else{vrm["lookAt"].is_object()&&document["extensionsUsed"].as_array().into_iter().flatten().any(|name|name=="VRMC_vrm")};
    let look_at=has_look&&bones.iter().any(|name|name=="head")&&if expression_look{["lookUp","lookDown","lookLeft","lookRight"].iter().any(|name|expressions.contains(*name))}else{bones.iter().any(|name|["leftEye","rightEye"].contains(&name.as_str()))};
    Target{bones,expressions:expressions.into_iter().collect(),look_at,clips,names}
}
fn target_for(root:&Path,asset:Option<&str>)->Result<Target,String>{
    let asset=asset.unwrap_or("nova");
    if asset=="nova"||asset.starts_with("builtin:"){
        let catalog:Value=serde_json::from_str(include_str!("../../public/character-emote-targets.json")).map_err(|_|"Die mitgelieferten Modelldaten sind ungültig.")?;
        serde_json::from_value(catalog[asset].clone()).map_err(|_|"Die mitgelieferte Figur ist unbekannt.".into())
    }else{Ok(target(&crate::avatar_metadata::imported_document(root,asset)?))}
}

pub fn inspect(root: &Path, profile: &Value) -> Result<Capabilities, String> {
    let mut info = crate::avatar_metadata::for_asset(root, profile["avatar"]["assetPath"].as_str())?;
    let target=target_for(root,profile["avatar"]["assetPath"].as_str())?;
    if matches!(info.format.as_str(),"vrm0"|"vrm1"){info.bones=target.bones;info.expressions=target.expressions;}
    // Display metadata is sanitized/truncated; action identity always uses the
    // original clip name that GLTFLoader will load, never its display label.
    info.animations.retain(|clip|target.clips.contains(&clip.index));
    for clip in &mut info.animations{if let Some(name)=target.names.get(clip.index){clip.name=name.clone();}}

    let mut capabilities = from_info(profile, &info);
    capabilities.actions.retain(|action|action.kind!="gesture"||!target.names.iter().any(|name|name.to_lowercase()==action.action||aliases(&action.action).iter().any(|alias|name.to_lowercase()==*alias)));
    if matches!(info.format.as_str(), "vrm0" | "vrm1") {
        let mut total = 0usize;
        // Reinspect the assigned files rather than trusting client/stored clip labels.
        for motion in profile["motions"].as_array().into_iter().flatten().take(crate::motions::MAX_MOTIONS) {
            let Some(id) = motion["id"].as_str() else { continue; };
            let Ok(bytes) = crate::motions::read(root, profile, id) else { continue; };
            total = total.saturating_add(bytes.len());
            if total > 64 * 1024 * 1024 { break; }
            let Ok(clips) = crate::motions::inspect(&bytes) else { continue; };
            for clip in clips {
                let body = clip.body.iter().any(|name| info.bones.contains(name));
                let expressions = clip.expressions.iter().any(|name| info.expressions.contains(name));
                // Gaze requires a real look-at declaration and a usable target.
                if !body && !expressions && !(clip.look_at && target.look_at) { continue; }
                capabilities.actions.push(Action { action: crate::motions::action(id,clip.index), name: clip.name,
                    kind: "motion", duration: Some(clip.duration) });
            }
        }
    }
    Ok(capabilities)
}

fn valid_configuration(profile:&Value,emote:&Value)->Result<(),String>{
    if !emote["id"].as_str().is_some_and(crate::storage::valid_id)||profile["emotes"].as_array().into_iter().flatten().filter(|other|other["id"]==emote["id"]).count()!=1{return Err("Emote wurde im Charakterprofil nicht gefunden oder ist nicht eindeutig".into());}
    let sound=if emote["sound"].is_null(){""}else{emote["sound"].as_str().ok_or("Emote-Sound muss eine Sound-ID sein")?};
    if !crate::emote_audio::valid_sound_id(sound){return Err("Emote-Sound-ID ist ungültig".into());}
    Ok(())
}
pub fn configured(capabilities: &Capabilities, profile: &Value) -> Value {
    let emotes: Vec<_> = profile["emotes"].as_array().into_iter().flatten().map(|emote| {
        match valid_configuration(profile,emote).and_then(|_|resolve(capabilities, emote["action"].as_str().unwrap_or_default())) {
            Ok(action) => json!({"id":emote["id"],"available":true,"action":action.action,"kind":action.kind,"duration":action.duration}),
            Err(error) => json!({"id":emote["id"],"available":false,"reason":error}),
        }
    }).collect();
    json!({"botId":capabilities.bot_id,"avatarBinding":capabilities.avatar_binding,"actions":capabilities.actions,"emotes":emotes})
}

pub fn emote_binding(emote:&Value)->Value { json!([emote["id"],emote["action"],emote["sound"].as_str().unwrap_or(""),emote["soundVolume"].as_f64().unwrap_or(0.7).clamp(0.0,1.0),emote["duration"].as_f64().unwrap_or(2.5).clamp(0.2,15.0),emote["intensity"].as_f64().unwrap_or(1.0).clamp(0.0,2.0),emote["allowChat"].as_bool().unwrap_or(false)]) }
pub fn available_emotes(root:&Path,profile:&Value,viewer:bool)->Vec<Value>{
    let Ok(capabilities)=inspect(root,profile) else{return Vec::new()};
    profile["emotes"].as_array().into_iter().flatten().filter(|emote|(!viewer||emote["allowChat"].as_bool()==Some(true))&&valid_configuration(profile,emote).is_ok()&&resolve(&capabilities,emote["action"].as_str().unwrap_or_default()).is_ok()).cloned().collect()
}
/// Queued presentation payloads never acquire a newly edited action or sound.
fn current_payload_in(capabilities:&Capabilities,profile:&Value,payload:&Value)->bool{
    if payload["botId"]!=profile["id"]||payload["avatarBinding"]!=capabilities.avatar_binding{return false;}
    let emotes:Vec<_>=profile["emotes"].as_array().into_iter().flatten().filter(|emote|emote["id"]==payload["id"]).collect();
    let [emote]=emotes.as_slice() else{return false;};
    payload["emoteBinding"]==emote_binding(emote)&&resolve(capabilities,emote["action"].as_str().unwrap_or_default()).is_ok_and(|action|payload["action"].as_str()==Some(action.action.as_str()))
}
pub fn pending(root:&Path,profile:&Value,payloads:&[Value])->Vec<Value>{
    let Ok(capabilities)=inspect(root,profile) else{return Vec::new()};
    payloads.iter().filter(|payload|current_payload_in(&capabilities,profile,payload)).cloned().collect()
}

pub fn admit(root: &Path, profile: &Value, emote: &Value) -> Result<(Action, Value), String> {
    valid_configuration(profile,emote)?;
    let capabilities = inspect(root,profile)?;
    let action = resolve(&capabilities, emote["action"].as_str().unwrap_or_default())?.clone();
    Ok((action, capabilities.avatar_binding))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn capabilities(animations: &[(&str,f64)], format: &str, bones: &[&str]) -> Capabilities {
        let mut info = crate::avatar_metadata::for_asset(Path::new("unused"),None).unwrap();
        info.format=format.into();info.bones=bones.iter().map(|name|(*name).into()).collect();
        info.animations=animations.iter().enumerate().map(|(index,(name,duration))|crate::avatar_metadata::AnimationInfo{name:(*name).into(),index,duration:Some(*duration)}).collect();
        from_info(&json!({"id":"nova"}),&info)
    }
    #[test]
    fn nonexistent_ambiguous_and_unusable_clips_never_become_actions() {
        let caps=capabilities(&[("Wave",1.0),("Wave",2.0),("Jump",1.0),("NoTime",0.0)],"glb2",&[]);
        for action in ["wave","Wave","NoTime","unknown","vrma:fake:0"] { assert!(resolve(&caps,action).is_err(),"{action}"); }
        assert_eq!(resolve(&caps,"celebrate").unwrap().action,"Jump");
        assert_eq!(resolve(&caps,"hide").unwrap().kind,"gesture");
        let caps=capabilities(&[("Wave",1.0),("wave",1.0)],"glb2",&[]);
        assert_eq!(resolve(&caps,"Wave").unwrap().action,"Wave");assert!(resolve(&caps,"WAVE").is_err());
    }
    #[test]
    fn procedural_gestures_require_the_actual_target_bones_and_do_not_shadow_clips() {
        let caps=capabilities(&[("Yes",1.0)],"vrm1",&["head"]);
        assert_eq!(resolve(&caps,"nod").unwrap().action,"Yes");assert!(resolve(&caps,"wave").is_err());
        let caps=capabilities(&[],"vrm1",&["leftUpperArm","leftLowerArm","rightUpperArm","rightLowerArm","spine"]);
        assert!(resolve(&caps,"wave").is_ok());assert!(resolve(&caps,"dance").is_ok());assert!(resolve(&caps,"nod").is_err());
        let caps=capabilities(&[],"glb2",&["head","leftUpperArm","leftLowerArm","rightUpperArm","rightLowerArm","spine"]);
        assert_eq!(caps.actions.len(),1);
        let caps=capabilities(&[],"builtin",&[]);assert_eq!(caps.actions.len(),5);
    }
    #[test]
    fn stale_configurations_are_described_without_granting_a_dispatch() {
        let profile=json!({"id":"nova","avatar":{"assetPath":"nova"},"emotes":[{"id":"valid","action":"wave"},{"id":"stale","action":"missing"}]});
        let caps=inspect(Path::new("unused"),&profile).unwrap();let summary=configured(&caps,&profile);
        assert_eq!(summary["emotes"][0]["available"],true);assert_eq!(summary["emotes"][1]["available"],false);
        let mut changed=profile.clone();changed["avatar"]["assetPath"]=json!("builtin:fox");assert_ne!(binding(&profile),binding(&changed));
        changed=profile.clone();changed["id"]=json!("fox");assert_ne!(binding(&profile),binding(&changed));
        changed=profile.clone();changed["motions"]=json!([{"id":"new.vrma"}]);assert_ne!(binding(&profile),binding(&changed));
    }
    fn replace_document(bytes:&[u8],edit:impl FnOnce(&mut Value))->Vec<u8>{
        let size=u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let mut document:Value=serde_json::from_slice(&bytes[20..20+size]).unwrap();edit(&mut document);
        let mut json=serde_json::to_vec(&document).unwrap();while json.len()%4!=0{json.push(b' ');}
        let mut result=b"glTF".to_vec();result.extend(2u32.to_le_bytes());result.extend([0;4]);result.extend((json.len()as u32).to_le_bytes());result.extend(0x4e4f534au32.to_le_bytes());result.extend(json);result.extend(&bytes[20+size..]);let size=result.len()as u32;result[8..12].copy_from_slice(&size.to_le_bytes());result
    }
    #[test]
    fn bundled_target_catalog_matches_all_actual_shipped_documents(){
        let files=[("builtin:robot-expressive","RobotExpressive.glb"),("builtin:fox","Fox.glb"),("builtin:kaykit-knight","KayKitKnight.glb"),("builtin:kaykit-mage","KayKitMage.glb"),("builtin:kaykit-rogue","KayKitRogue.glb"),("builtin:kaykit-skeleton","KayKitSkeleton.glb"),("builtin:osa-mushy","osa/Mushy.vrm"),("builtin:osa-dracula","osa/Dracula.vrm"),("builtin:osa-always-watching","osa/AlwaysWatching.vrm"),("builtin:osa-conehead-being","osa/ConeheadBeing.vrm"),("builtin:osa-cool-alien","osa/CoolAlien.vrm"),("builtin:osa-cool-tiger","osa/CoolTiger.vrm"),("builtin:osa-ghost-pumpking","osa/GhostPumpking.vrm"),("builtin:osa-max-hax","osa/MaxHax.vrm"),("builtin:osa-megan-the-fox","osa/MeganTheFox.vrm"),("builtin:osa-paco","osa/Paco.vrm")];
        for(id,file)in files{let bytes=std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../public/characters").join(file)).unwrap();let(document,_)=crate::avatar_metadata::container_bytes(&bytes).unwrap();assert_eq!(target_for(Path::new("unused"),Some(id)).unwrap(),target(&document),"{id}");}
    }
    #[test]
    fn actual_vrm0_and_vrm1_targets_reject_fabricated_nodes_and_normalize_expressions(){
        for bytes in [include_bytes!("../../src/lib/fixtures/target-vrm0.vrm").as_slice(),include_bytes!("../../src/lib/fixtures/target-vrm1.vrm").as_slice()]{
            let(mut document,_)=crate::avatar_metadata::container_bytes(bytes).unwrap();let targets=target(&document);
            assert!(targets.expressions.contains(&"happy".into()));assert!(!targets.expressions.contains(&"joy".into()));assert!(!targets.look_at);
            if document["extensions"]["VRM"].is_object(){document["extensions"]["VRM"]["humanoid"]["humanBones"][3]["node"]=json!(99999);}else{document["extensions"]["VRMC_vrm"]["humanoid"]["humanBones"]["leftUpperArm"]["node"]=json!(99999);}
            assert!(!target(&document).bones.contains(&"leftUpperArm".into()));
        }
    }
    #[test]
    fn actual_assigned_vrma_is_per_bot_and_survives_only_current_usable_targets(){
        let root=std::env::temp_dir().join(format!("orbitpal-capabilities-{}",uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("profiles/nova")).unwrap();std::fs::create_dir_all(root.join("profiles/other")).unwrap();std::fs::create_dir(root.join("avatars")).unwrap();
        let source=root.join("motion.vrma");let source_bytes=include_bytes!("../../src/lib/fixtures/greeting.vrma");std::fs::write(&source,source_bytes).unwrap();
        let motion=crate::motions::import(&root,"nova",&source).unwrap();
        for bytes in [include_bytes!("../../src/lib/fixtures/target-vrm0.vrm").as_slice(),include_bytes!("../../src/lib/fixtures/target-vrm1.vrm").as_slice()]{
            let name=format!("{}.vrm",uuid::Uuid::new_v4());std::fs::write(root.join("avatars").join(&name),bytes).unwrap();
            let profile=json!({"id":"nova","avatar":{"assetPath":name},"motions":[motion],"emotes":[{"id":"hello","action":crate::motions::action(&motion.id,1),"sound":"builtin:wave","allowChat":true}]});
            let caps=inspect(&root,&profile).unwrap();assert_eq!(resolve(&caps,&crate::motions::action(&motion.id,1)).unwrap().kind,"motion");
            let payload=crate::overlay::prepare_emote_profile(&root,&profile,"hello",true).unwrap();assert_eq!(pending(&root,&profile,&[payload.clone()]).len(),1);
            for pointer in ["/avatar/assetPath","/emotes/0/action","/emotes/0/sound","/emotes/0/allowChat","/id"]{
                let mut changed=profile.clone();*changed.pointer_mut(pointer).unwrap()=json!("changed");assert!(pending(&root,&changed,&[payload.clone()]).is_empty(),"{pointer}");
            }
            let mut other=profile.clone();other["id"]=json!("other");assert!(resolve(&inspect(&root,&other).unwrap(),&crate::motions::action(&motion.id,0)).is_err());
            // A gaze-only file is accepted only when the VRM has an eye target.
            let gaze=replace_document(source_bytes,|document|{document["animations"]=json!([{"name":"Gaze","samplers":[document["animations"][0]["samplers"][3].clone()],"channels":[{"sampler":0,"target":{"node":16,"path":"rotation"}}]}]);});
            std::fs::write(root.join("profiles/nova/motions").join(&motion.id),&gaze).unwrap();
            assert!(resolve(&inspect(&root,&profile).unwrap(),&crate::motions::action(&motion.id,0)).is_err());
            let eye=replace_document(bytes,|document|{let nodes=document["nodes"].as_array_mut().unwrap();let index=nodes.len();nodes.push(json!({"name":"Eye"}));document["nodes"][2]["children"]=json!([index]);if document["extensions"]["VRM"].is_object(){document["extensions"]["VRM"]["humanoid"]["humanBones"].as_array_mut().unwrap().push(json!({"bone":"leftEye","node":index}));}else{document["extensions"]["VRMC_vrm"]["humanoid"]["humanBones"]["leftEye"]=json!({"node":index});}});
            std::fs::write(root.join("avatars").join(&name),eye).unwrap();assert!(resolve(&inspect(&root,&profile).unwrap(),&crate::motions::action(&motion.id,0)).is_ok());
            std::fs::write(root.join("profiles/nova/motions").join(&motion.id),source_bytes).unwrap();
        }
        std::fs::remove_file(root.join("profiles/nova/motions").join(&motion.id)).unwrap();
        let profile=json!({"id":"nova","motions":[motion],"avatar":{"assetPath":"builtin:osa-mushy"}});assert!(!inspect(&root,&profile).unwrap().actions.iter().any(|action|action.kind=="motion"));
        std::fs::remove_dir_all(root).unwrap();
    }

}
