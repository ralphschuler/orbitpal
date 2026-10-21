//! Rendering-cost preferences are installation-local presentation data. Writing
//! them never edits a profile, revokes a Life grant or supplies action evidence.
use super::Store;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::Manager;
const ERROR: &str = "render_settings_unavailable";
const MAX_REVISION: u64 = 9_007_199_254_740_991;
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub(crate) enum Quality { #[default] Balanced, Energy }
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Record { version:u8, revision:u64, quality:Quality }
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct Snapshot { version:u8, bot_id:String, revision:u64, quality:Quality }
impl Record {
    fn snapshot(self,bot:&str)->Snapshot {Snapshot {version:self.version,bot_id:bot.into(),revision:self.revision,quality:self.quality}}
}
fn read(db:&Connection,bot:&str)->Result<Record,String>{
    if !super::valid_id(bot) || !db.query_row("SELECT EXISTS(SELECT 1 FROM bot_profiles WHERE id=?1)",[bot],|row|row.get::<_,bool>(0)).map_err(|_|ERROR)? { return Err(ERROR.into()); }
    let row:Option<Option<String>>=db.query_row("SELECT CASE WHEN typeof(value)='text' AND length(CAST(value AS BLOB)) BETWEEN 1 AND 256 THEN value ELSE NULL END FROM settings WHERE key=?1",[format!("render_quality:{bot}")],|row|row.get(0)).optional().map_err(|_|ERROR)?;
    let value=match row {None=>Record {version:1,revision:0,quality:Quality::Balanced},Some(Some(text))=>serde_json::from_str::<Record>(&text).map_err(|_|ERROR)?,Some(None)=>return Err(ERROR.into())};
    if value.version!=1 || value.revision>MAX_REVISION {return Err(ERROR.into());} Ok(value)
}
impl Store {
    pub(crate) fn render_settings(&self,bot:&str)->Result<Snapshot,String>{
        if self.fixed_profile.as_deref().is_some_and(|id|id!=bot){return Err(ERROR.into());}
        let db=self.db.lock().map_err(|_|ERROR)?;
        read(&db,bot).map(|record|record.snapshot(bot))
    }
    pub(crate) fn set_render_quality(&self,bot:&str,expected:u64,quality:Quality)->Result<Snapshot,String>{
        if self.fixed_profile.as_deref().is_some_and(|id|id!=bot){return Err(ERROR.into());}
        let mut db=self.db.lock().map_err(|_|ERROR)?;let tx=db.transaction().map_err(|_|ERROR)?;
        let mut record=read(&tx,bot)?;
        if record.revision!=expected {return Err("render_settings_stale".into());}
        if record.quality!=quality {
            record.revision=record.revision.checked_add(1).filter(|revision|*revision<=MAX_REVISION).ok_or(ERROR)?;
            record.quality=quality;
            tx.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![format!("render_quality:{bot}"),serde_json::to_string(&record).map_err(|_|ERROR)?]).map_err(|_|ERROR)?;
        }
        tx.commit().map_err(|_|ERROR)?;Ok(record.snapshot(bot))
    }
}
fn main_only(label:&str)->Result<(),String>{if label=="main"{Ok(())}else{Err(ERROR.into())}}
fn read_window(label:&str,bot:&str)->Result<(),String>{
    if label=="main" || label==format!("avatar-{bot}"){Ok(())}else{Err(ERROR.into())}
}
#[tauri::command(async)]
pub(crate) fn get_render_settings(window:tauri::WebviewWindow,state:tauri::State<'_,crate::AppState>,bot_id:String)->Result<Snapshot,String>{
    read_window(window.label(),&bot_id)?;
    state.store.render_settings(&bot_id)
}
#[tauri::command]
pub(crate) fn set_render_quality(window:tauri::WebviewWindow,app:tauri::AppHandle,bot_id:String,expected_revision:u64,quality:Quality)->Result<Snapshot,String>{
    main_only(window.label())?;let state=app.state::<crate::AppState>();state.updater.ensure_idle()?;
    let snapshot=state.store.set_render_quality(&bot_id,expected_revision,quality)?;
    let mut event=serde_json::to_value(&snapshot).map_err(|_|ERROR)?;event["kind"]=serde_json::json!("render-settings");
    state.overlay.publish(&app,event);Ok(snapshot)
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(std::path::PathBuf);
    impl Fixture {fn new()->Self{Self(std::env::temp_dir().join(format!("orbitpal-render-settings-{}",uuid::Uuid::new_v4())))}}
    impl Drop for Fixture {fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
    #[test]
    fn avatar_read_cannot_cross_bot_windows_or_acquire_write_authority(){
        assert!(read_window("main","nova").is_ok());
        assert!(read_window("avatar-nova","nova").is_ok());
        for label in ["avatar-friend","avatar-nova-extra","obs-nova",""] {
            assert!(read_window(label,"nova").is_err());
        }
        assert!(main_only("avatar-nova").is_err());
    }
    #[test]
    fn detached_render_write_preserves_profile_tokens_and_native_policy_receipts(){
        let fixture=Fixture::new();let store=Store::new(fixture.0.clone()).unwrap();let profile=store.profile().unwrap();let bot=profile["id"].as_str().unwrap();
        let token=store.action_profile_token(bot,&profile).unwrap();let receipt=store.native_layout_policy(bot).unwrap();
        let snapshot=store.set_render_quality(bot,0,Quality::Energy).unwrap();assert_eq!(snapshot.revision,1);
        assert_eq!(store.bot_profile(bot).unwrap(),profile);assert_eq!(store.action_profile_token(bot,&profile).unwrap(),token);assert!(receipt.current());
        let other=store.create_bot("Other",false).unwrap()["id"].as_str().unwrap().to_owned();
        assert_eq!(store.render_settings(&other).unwrap().quality,Quality::Balanced);
        store.select_bot(bot).unwrap();
        let copied=store.create_bot("Copied",true).unwrap()["id"].as_str().unwrap().to_owned();
        assert_eq!(store.render_settings(&copied).unwrap().quality,Quality::Balanced);
        let bundle=fixture.0.join("profile-export.json");store.scoped(bot).unwrap().export(&bundle).unwrap();
        let imported=store.import(&bundle).unwrap()["id"].as_str().unwrap().to_owned();
        assert_ne!(imported,bot);assert_eq!(store.render_settings(&imported).unwrap().quality,Quality::Balanced);
        drop(store);let restarted=Store::new(fixture.0.clone()).unwrap();assert_eq!(restarted.render_settings(bot).unwrap().quality,Quality::Energy);
    }
    #[test]
    fn render_settings_cas_scope_and_bounded_schema_fail_closed(){
        let fixture=Fixture::new();let store=Store::new(fixture.0.clone()).unwrap();let bot=store.profile().unwrap()["id"].as_str().unwrap().to_owned();
        store.set_render_quality(&bot,0,Quality::Energy).unwrap();assert!(store.set_render_quality(&bot,0,Quality::Balanced).is_err());
        assert_eq!(store.set_render_quality(&bot,1,Quality::Energy).unwrap().revision,1);
        assert!(store.set_render_quality("missing",0,Quality::Energy).is_err());assert!(main_only("avatar-nova").is_err());assert!(main_only("main").is_ok());
        assert!(serde_json::from_str::<Quality>("\"unknown\"").is_err());
        for value in ["x".repeat(257),"{\"version\":2,\"revision\":0,\"quality\":\"energy\"}".into(),"{\"version\":1,\"revision\":1,\"quality\":\"energy\",\"grant\":true}".into()] {
            store.db.lock().unwrap().execute("UPDATE settings SET value=?1 WHERE key=?2",params![value,format!("render_quality:{bot}")]).unwrap();assert!(store.render_settings(&bot).is_err());
        }
    }
}
