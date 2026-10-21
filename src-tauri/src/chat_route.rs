//! Native-only provenance for the app-global chat connection. The browser sees
//! an opaque session identifier, never destination selectors or credentials.
use serde_json::{json, Value};

fn text<'a>(value: &'a Value, key: &str) -> &'a str { value[key].as_str().unwrap_or("").trim() }
fn binding(profile: &Value, platform: &str) -> Value {
    let config = &profile["chat"][platform];
    let keys: &[&str] = if platform == "twitch" { &["clientId", "broadcasterId", "userId", "secretId"] }
        else { &["videoId", "liveChatId", "apiKeyId", "sendSecretId"] };
    let mut result = serde_json::Map::new();
    for key in keys { result.insert((*key).into(), json!(text(config, key))); }
    Value::Object(result)
}

/// Exact native route cancellation; jobs retain their own cancellation flags.
#[derive(Default)]
struct ImpulseRoute {
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    jobs: std::sync::Mutex<Vec<std::sync::Weak<std::sync::atomic::AtomicBool>>>,
}
impl ImpulseRoute {
    fn cancel(&self) {
        self.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
        let mut jobs = self.jobs.lock().unwrap_or_else(|p| p.into_inner());
        for weak in jobs.drain(..) {
            if let Some(job) = weak.upgrade() {
                job.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
    }
    fn bind(&self, job: &std::sync::Arc<std::sync::atomic::AtomicBool>) -> Result<(), String> {
        use std::sync::atomic::Ordering::SeqCst;
        if self.cancel.load(SeqCst) {
            job.store(true, SeqCst);
            return Err("impulse_source_cancelled".into());
        }
        let Ok(mut jobs) = self.jobs.lock() else {
            job.store(true, SeqCst);
            return Err("impulse_source_unavailable".into());
        };
        jobs.retain(|weak| weak.upgrade().is_some_and(|job| !job.load(SeqCst)));
        if jobs.len() >= 64 || self.cancel.load(SeqCst) {
            job.store(true, SeqCst);
            return Err("impulse_source_cancelled".into());
        }
        jobs.push(std::sync::Arc::downgrade(job));
        if self.cancel.load(SeqCst) {
            job.store(true, SeqCst);
            return Err("impulse_source_cancelled".into());
        }
        Ok(())
    }
}
#[derive(Default)]
pub(crate) struct ImpulseCancellation(std::sync::Mutex<std::sync::Arc<ImpulseRoute>>);
impl ImpulseCancellation {
    pub(crate) fn cancel(&self) {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).cancel();
    }
    fn renew(&self) {
        match self.0.lock() {
            Ok(mut route) => {
                route.cancel();
                *route = std::sync::Arc::new(ImpulseRoute::default());
            }
            // A poisoned registry never yields a new source, but must still
            // revoke jobs already retained by the old route.
            Err(poison) => poison.into_inner().cancel(),
        }
    }
    fn token(&self) -> Option<std::sync::Arc<ImpulseRoute>> {
        self.0.lock().ok().map(|route| route.clone())
    }
}
/// Native-only, never serialized, never constructed from a browser route string.
#[derive(Clone)]
pub(crate) struct ImpulseSource {
    platform: String,
    owner: String,
    generation: String,
    config: Value,
    route_cancel: std::sync::Arc<ImpulseRoute>,
    connection_cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl ImpulseSource {
    pub(crate) fn owner(&self) -> &str {
        &self.owner
    }
    pub(crate) fn platform(&self) -> &str {
        &self.platform
    }
    pub(crate) fn current_profile(&self, profile: &Value) -> bool {
        !self
            .route_cancel
            .cancel
            .load(std::sync::atomic::Ordering::SeqCst)
            && !self
                .connection_cancel
                .load(std::sync::atomic::Ordering::SeqCst)
            && profile["id"].as_str() == Some(self.owner.as_str())
            && binding(profile, &self.platform) == self.config
    }
    #[cfg(test)]
    fn cancellation(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.route_cancel.cancel.clone()
    }
    pub(crate) fn bind_cancel(
        &self,
        job: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<(), String> {
        if self
            .connection_cancel
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            job.store(true, std::sync::atomic::Ordering::SeqCst);
            return Err("impulse_source_cancelled".into());
        }
        self.route_cancel.bind(job)?;
        if self.connection_cancel.load(std::sync::atomic::Ordering::SeqCst) {
            job.store(true, std::sync::atomic::Ordering::SeqCst);
            return Err("impulse_source_cancelled".into());
        }
        Ok(())
    }
    pub(crate) fn same_route(&self, other: &Self) -> bool {
        self.platform == other.platform
            && self.owner == other.owner
            && self.generation == other.generation
            && self.config == other.config
            && std::sync::Arc::ptr_eq(&self.route_cancel, &other.route_cancel)
    }
}
pub struct ConnectionContext {
    pub connected: bool,
    pub user_id: Option<String>,
    pub cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    platform: String,
    owner_id: String,
    owner_name: String,
    config: Value,
    write_enabled: bool,
    write_token: Option<String>,
    generation: String,
    channel: String,
    impulse_cancel:std::sync::Arc<ImpulseCancellation>,
}

/// Never serialized or returned to a model/renderer. Captured when a real queued
/// message is selected, then retained with that exact response until playback.
#[derive(Clone)]
pub struct ChatRoute {
    platform: String,
    owner_id: String,
    generation: String,
    channel: String,
    account: Option<String>,
    responder_id: String,
    responder_binding: Value,
    responder_enabled: bool,
}
impl ChatRoute { pub fn owner_id(&self) -> &str { &self.owner_id } pub fn platform(&self) -> &str { &self.platform } pub fn responder_id(&self) -> &str { &self.responder_id } }

pub struct PreparedReply {
    pub platform: String,
    pub client_id: String,
    pub channel: String,
    pub sender: String,
    pub token: String,
    pub message: String,
}

/// Acquire every asynchronous chat lock before reading current permissions.
/// A profile edit while the post waits must never be checked against an earlier
/// caller snapshot. Reads and validation below contain no asynchronous gap.
pub async fn prepare_current(
    context:&tokio::sync::Mutex<ConnectionContext>, route:&ChatRoute,
    read_profiles:impl FnOnce()->Result<(Value,Value),String>, message:&str,
)->Result<PreparedReply,String> {
    let current=context.lock().await;
    let (responder,owner)=read_profiles()?;
    current.prepare_reply(route,&responder,&owner,message)
}

impl ConnectionContext {
    pub fn new(platform: &str, owner: &Value, write_token: Option<String>, cancel: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Result<Self, String> {
        if !matches!(platform,"twitch"|"youtube") { return Err("Unbekannte Chatplattform.".into()); }
        let owner_id = owner["id"].as_str().filter(|id|crate::storage::valid_id(id)).ok_or("Bot-ID der Chatverbindung fehlt.")?;
        Ok(Self { connected:false, user_id:None, platform:platform.into(), owner_id:owner_id.into(),
            owner_name:owner["name"].as_str().unwrap_or("Cohost").into(), config:binding(owner,platform),
            write_enabled:owner["chat"][platform]["sendReplies"]==true,
            write_token:write_token.filter(|token|!token.trim().is_empty()), generation:String::new(), channel:String::new(), impulse_cancel:std::sync::Arc::new(ImpulseCancellation::default()), cancel })
    }
    pub(crate) fn impulse_cancellation(&self) -> std::sync::Arc<ImpulseCancellation> {
        self.impulse_cancel.clone()
    }
    pub(crate) fn owner_id(&self) -> &str {
        &self.owner_id
    }
    pub(crate) fn impulse_source(
        &self,
        platform: &str,
        route: &str,
        owner: &Value,
    ) -> Option<ImpulseSource> {
        if platform != self.platform
            || self.route_id() != Some(route)
            || owner["id"].as_str() != Some(self.owner_id.as_str())
            || binding(owner, platform) != self.config
            || self.cancel.load(std::sync::atomic::Ordering::SeqCst)
            || serde_json::to_vec(&self.config).ok()?.len() > 8192
        {
            return None;
        }
        let source = ImpulseSource {
            platform: self.platform.clone(),
            owner: self.owner_id.clone(),
            generation: self.generation.clone(),
            config: self.config.clone(),
            route_cancel: self.impulse_cancel.token()?,
            connection_cancel: self.cancel.clone(),
        };
        source.current_profile(owner).then_some(source)
    }
    pub(crate) fn impulse_source_current(&self, source: &ImpulseSource, owner: &Value) -> bool {
        self.impulse_source(&source.platform, &source.generation, owner)
            .is_some_and(|current| current.same_route(source))
    }
    pub fn write_token(&self) -> Option<&str> { if self.write_enabled { self.write_token.as_deref() } else { None } }
    pub fn mark_connected(&mut self, channel: &str) {
        // A retry/reconnect never revives a queued message from a prior session.
        if !self.connected || self.channel != channel { self.impulse_cancel.renew();self.generation=uuid::Uuid::new_v4().to_string(); }
        self.connected=true; self.channel=channel.into();
    }
    pub fn disconnected(&mut self) { self.impulse_cancel.cancel();self.connected=false; self.generation.clear(); }
    pub fn public_owner(&self) -> Value { json!({"ownerBotId":self.owner_id,"ownerName":self.owner_name}) }
    pub fn incoming_route(&self, channel:&str)->Option<String> {
        if self.cancel.load(std::sync::atomic::Ordering::SeqCst) || channel!=self.channel {return None;}
        self.route_id().map(str::to_owned)
    }
    pub fn route_id(&self) -> Option<&str> { self.connected.then_some(self.generation.as_str()).filter(|id|!id.is_empty()) }
    pub fn route(&self, platform:&str, route_id:Option<&str>, responder:&Value) -> Result<ChatRoute,String> {
        if platform!=self.platform || route_id.is_none() || route_id!=self.route_id() || self.cancel.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("Die Chatnachricht gehört nicht mehr zur aktiven Verbindung. Bitte eine neue Nachricht auswählen.".into());
        }
        Ok(ChatRoute { platform:self.platform.clone(), owner_id:self.owner_id.clone(), generation:self.generation.clone(), channel:self.channel.clone(), account:self.user_id.clone(),
            responder_id:responder["id"].as_str().ok_or("Antwortender Bot fehlt.")?.into(), responder_binding:binding(responder,platform), responder_enabled:responder["chat"][platform]["sendReplies"]==true })
    }
    pub fn prepare_reply(&self, route:&ChatRoute, responder:&Value, owner:&Value, message:&str) -> Result<PreparedReply,String> {
        let stale="Die Chatverbindung oder ihr Kanal/Konto hat sich geändert. Die Antwort wird nicht in den Chat geschrieben.";
        if !self.connected || self.cancel.load(std::sync::atomic::Ordering::SeqCst) || route.platform!=self.platform || route.owner_id!=self.owner_id || route.generation!=self.generation || route.channel!=self.channel || route.account!=self.user_id { return Err(stale.into()); }
        if owner["id"].as_str()!=Some(&self.owner_id) || binding(owner,&self.platform)!=self.config || responder["id"].as_str()!=Some(&route.responder_id)
            || binding(responder,&self.platform)!=route.responder_binding || route.responder_binding!=self.config { return Err(stale.into()); }
        if !self.write_enabled || !route.responder_enabled || owner["chat"][&self.platform]["sendReplies"]!=true || responder["chat"][&self.platform]["sendReplies"]!=true {
            return Err("Geschriebene Chatantworten sind für diesen Bot oder den Verbindungsinhaber deaktiviert. Nach Änderungen bitte neu verbinden.".into());
        }
        let token=self.write_token.as_deref().ok_or("Für diese Verbindung fehlt der OAuth-Schreibtoken. Bitte speichern und neu verbinden.")?;
        let sender=route.account.as_deref().ok_or("Das Schreibkonto dieser Verbindung ist nicht aufgelöst. Bitte neu verbinden.")?;
        let message=message.split_whitespace().collect::<Vec<_>>().join(" ");
        if message.is_empty() { return Err("Chatantwort ist leer.".into()); }
        let limit=if self.platform=="twitch" {500} else {200};
        let message=if message.chars().count()>limit {message.chars().take(limit-1).collect::<String>()+"…"}else{message};
        Ok(PreparedReply{platform:self.platform.clone(),client_id:text(&self.config,"clientId").into(),channel:self.channel.clone(),sender:sender.into(),token:token.trim().strip_prefix("oauth:").unwrap_or(token.trim()).into(),message})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc,atomic::{AtomicBool,Ordering}};
    fn profile(id:&str,platform:&str,channel:&str,account:&str)->Value {
        let config=if platform=="twitch" {json!({"clientId":"fixture-client","broadcasterId":channel,"userId":account,"secretId":account,"sendReplies":true})}
            else {json!({"liveChatId":channel,"videoId":"","apiKeyId":"fixture-reader","sendSecretId":account,"sendReplies":true})};
        json!({"id":id,"name":id,"chat":{platform:config}})
    }
    fn connected(platform:&str,owner:&Value,channel:&str,account:&str)->ConnectionContext {
        let mut context=ConnectionContext::new(platform,owner,Some("fixture-owner-token".into()),Arc::new(AtomicBool::new(false))).unwrap();
        context.user_id=Some(account.into());context.mark_connected(channel);context
    }
    #[test] fn both_platforms_keep_the_original_channel_account_and_owner_credentials_for_multiple_bots() {
        for platform in ["twitch","youtube"] {
            let owner=profile("owner",platform,"channel-a","account-a");let mut responder=owner.clone();responder["id"]=json!("second-bot");
            let context=connected(platform,&owner,"channel-a","account-a");let route=context.route(platform,context.route_id(),&responder).unwrap();
            let send=context.prepare_reply(&route,&responder,&owner,"Hello chat").unwrap();
            assert_eq!(send.channel,"channel-a");assert_eq!(send.sender,"account-a");assert_eq!(send.token,"fixture-owner-token");assert_eq!(route.owner_id(),"owner");
            let other=profile("second-bot",platform,"channel-b","account-b");
            assert!(context.prepare_reply(&route,&other,&owner,"no").is_err());
            let wrong_route=context.route(platform,context.route_id(),&other).unwrap();assert!(context.prepare_reply(&wrong_route,&other,&owner,"no").is_err());
        }
    }
    #[test] fn stale_queued_and_pending_routes_never_survive_disconnect_reconnect_or_replacement() {
        for platform in ["twitch","youtube"] {
            let owner=profile("owner",platform,"channel-a","account-a");let mut context=connected(platform,&owner,"channel-a","account-a");
            let token=context.route_id().unwrap().to_owned();let route=context.route(platform,Some(&token),&owner).unwrap();
            context.disconnected();assert!(context.route(platform,Some(&token),&owner).is_err());assert!(context.prepare_reply(&route,&owner,&owner,"no").is_err());
            context.mark_connected("channel-a");assert_ne!(context.route_id(),Some(token.as_str()));assert!(context.prepare_reply(&route,&owner,&owner,"no").is_err());
            let replacement=connected(platform,&owner,"channel-a","account-a");assert!(replacement.prepare_reply(&route,&owner,&owner,"no").is_err());
        }
    }
    #[test] fn pending_replies_reject_owner_responder_channel_and_account_changes() {
        for platform in ["twitch","youtube"] {
            let owner=profile("owner",platform,"channel-a","account-a");let mut context=connected(platform,&owner,"channel-a","account-a");let route=context.route(platform,context.route_id(),&owner).unwrap();
            for changed in [profile("other",platform,"channel-a","account-a"),profile("owner",platform,"channel-b","account-a"),profile("owner",platform,"channel-a","account-b")] {
                assert!(context.prepare_reply(&route,&changed,&owner,"no").is_err());assert!(context.prepare_reply(&route,&owner,&changed,"no").is_err());
            }
            context.user_id=Some("new-account".into());assert!(context.prepare_reply(&route,&owner,&owner,"no").is_err());
        }
    }
    #[test] fn fabricated_missing_and_cross_platform_routes_never_authorize_writes() {
        let owner=profile("owner","youtube","channel-a","account-a");let context=connected("youtube",&owner,"channel-a","account-a");
        assert!(context.route("youtube",None,&owner).is_err());assert!(context.route("youtube",Some("fabricated"),&owner).is_err());assert!(context.route("twitch",context.route_id(),&owner).is_err());
    }
    #[test] fn disabled_writes_at_admission_connection_or_playback_cannot_be_enabled_by_late_results() {
        let owner=profile("owner","youtube","channel-a","account-a");let context=connected("youtube",&owner,"channel-a","account-a");
        let mut disabled=owner.clone();disabled["chat"]["youtube"]["sendReplies"]=json!(false);
        let route=context.route("youtube",context.route_id(),&owner).unwrap();assert!(context.prepare_reply(&route,&disabled,&owner,"no").is_err());assert!(context.prepare_reply(&route,&owner,&disabled,"no").is_err());
        let route=context.route("youtube",context.route_id(),&disabled).unwrap();assert!(context.prepare_reply(&route,&owner,&owner,"no").is_err());
        let context=connected("youtube",&disabled,"channel-a","account-a");let route=context.route("youtube",context.route_id(),&owner).unwrap();assert!(context.prepare_reply(&route,&owner,&owner,"no").is_err());
    }
    #[test] fn cancellation_and_missing_writer_identity_fail_closed() {
        let owner=profile("owner","youtube","channel-a","account-a");let mut context=connected("youtube",&owner,"channel-a","account-a");
        context.user_id=None;let route=context.route("youtube",context.route_id(),&owner).unwrap();assert!(context.prepare_reply(&route,&owner,&owner,"no").is_err());
        context.user_id=Some("account-a".into());let route=context.route("youtube",context.route_id(),&owner).unwrap();context.cancel.store(true,Ordering::SeqCst);assert!(context.prepare_reply(&route,&owner,&owner,"no").is_err());assert!(context.route("youtube",context.route_id(),&owner).is_err());
    }
    #[test] fn reply_limits_count_unicode_and_only_public_owner_metadata_is_exposed() {
        for (platform,limit) in [("twitch",500),("youtube",200)] {
            let owner=profile("owner",platform,"channel-a","account-a");let context=connected(platform,&owner,"channel-a","account-a");let route=context.route(platform,context.route_id(),&owner).unwrap();
            assert_eq!(context.prepare_reply(&route,&owner,&owner,&"🦊".repeat(600)).unwrap().message.chars().count(),limit);
            assert!(context.prepare_reply(&route,&owner,&owner," \n\t").is_err());
            assert_eq!(context.public_owner(),json!({"ownerBotId":"owner","ownerName":"owner"}));
        }
    }
    #[tokio::test]
    async fn posts_waiting_for_a_chat_lock_read_fresh_store_permissions_channel_and_accounts() {
        use std::sync::Mutex as StdMutex;
        // The actual Store remains local and contains only fixture references;
        // no OS credential values or platform requests are used by this test.
        for platform in ["twitch","youtube"] {
            for changed_key in ["sendReplies",if platform=="twitch" {"broadcasterId"}else{"liveChatId"},if platform=="twitch" {"secretId"}else{"sendSecretId"}] {
                let root=std::env::temp_dir().join(format!("orbitpal-chat-route-{}",uuid::Uuid::new_v4()));
                let store=Arc::new(crate::storage::Store::new(root.clone()).unwrap());
                let mut owner=store.profile().unwrap();
                owner["chat"][platform]=profile("fixture",platform,"channel-a","account-a")["chat"][platform].clone();
                store.save_profile(owner.clone()).unwrap();
                let context=Arc::new(tokio::sync::Mutex::new(connected(platform,&owner,"channel-a","account-a")));
                let held=context.lock().await;
                let route=held.route(platform,held.route_id(),&owner).unwrap();
                // The previously captured values would approve the stale write.
                assert!(held.prepare_reply(&route,&owner,&owner,"Hello").is_ok());
                let reads=Arc::new(StdMutex::new(0));let read_count=reads.clone();let current=store.clone();let pending=context.clone();
                let (started,waiting)=tokio::sync::oneshot::channel();
                let task=tokio::spawn(async move {
                    started.send(()).unwrap();
                    prepare_current(&pending,&route,||{
                        *read_count.lock().unwrap()+=1;
                        Ok((current.bot_profile(route.responder_id())?,current.bot_profile(route.owner_id())?))
                    },"Hello").await
                });
                waiting.await.unwrap();assert_eq!(*reads.lock().unwrap(),0);
                let mut revoked=owner.clone();revoked["chat"][platform][changed_key]=if changed_key=="sendReplies" {json!(false)}else{json!("changed-after-admission")};
                store.save_profile(revoked).unwrap();drop(held);
                assert!(task.await.unwrap().is_err(),"{platform} {changed_key}");assert_eq!(*reads.lock().unwrap(),1);
                drop(store);std::fs::remove_dir_all(root).unwrap();
            }
        }
    }

}

#[cfg(test)]
mod impulse_route_tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    fn profile() -> Value {
        json!({"id":"owner","chat":{"twitch":{"clientId":"client","broadcasterId":"channel","userId":"account","secretId":"reference","sendReplies":false}}})
    }
    #[test]
    fn native_read_source_requires_actual_route_and_owner_binding_not_reply_rights() {
        let p = profile();
        let mut c =
            ConnectionContext::new("twitch", &p, None, Arc::new(AtomicBool::new(false))).unwrap();
        assert!(c.impulse_source("twitch", "invented", &p).is_none());
        c.mark_connected("channel");
        let token = c.route_id().unwrap().to_owned();
        let source = c.impulse_source("twitch", &token, &p).unwrap();
        assert!(source.current_profile(&p));
        let mut changed = p.clone();
        changed["chat"]["twitch"]["broadcasterId"] = json!("other");
        assert!(!source.current_profile(&changed));
        assert!(c.impulse_source("youtube", &token, &p).is_none());
    }
    #[test]
    fn exact_disconnect_and_reconnect_synchronously_cancel_old_native_job() {
        let p = profile();
        let mut c =
            ConnectionContext::new("twitch", &p, None, Arc::new(AtomicBool::new(false))).unwrap();
        c.mark_connected("channel");
        let old = c
            .impulse_source("twitch", c.route_id().unwrap(), &p)
            .unwrap();
        let retained = old.cancellation();
        assert!(!retained.load(Ordering::SeqCst));
        c.disconnected();
        assert!(retained.load(Ordering::SeqCst));
        c.mark_connected("channel");
        let new = c
            .impulse_source("twitch", c.route_id().unwrap(), &p)
            .unwrap();
        assert!(!old.same_route(&new));
        assert!(!old.current_profile(&p));
        assert!(new.current_profile(&p));
        c.impulse_cancellation().cancel();
        assert!(new.cancellation().load(Ordering::SeqCst));
    }
    #[test]
    fn receive_source_cannot_borrow_another_owner_or_invented_route() {
        let p = profile();
        let mut c = ConnectionContext::new("twitch", &p, None, Arc::new(AtomicBool::new(false))).unwrap();
        c.mark_connected("channel");
        let mut other = p.clone();
        other["id"] = json!("other");
        assert!(c.impulse_source("twitch", c.route_id().unwrap(), &other).is_none());
        assert!(c.impulse_source("twitch", "browser-forged", &p).is_none());
        let source = c.impulse_source("twitch", c.route_id().unwrap(), &p).unwrap();
        let job = Arc::new(AtomicBool::new(false));
        source.bind_cancel(&job).unwrap();
        c.mark_connected("replacement-channel");
        assert!(job.load(Ordering::SeqCst));
        assert!(!source.current_profile(&p));
    }
    #[test]
    fn cancellation_still_closes_retained_token_after_registry_poison() {
        let p = profile();
        let mut c =
            ConnectionContext::new("twitch", &p, None, Arc::new(AtomicBool::new(false))).unwrap();
        c.mark_connected("channel");
        let source = c
            .impulse_source("twitch", c.route_id().unwrap(), &p)
            .unwrap();
        let registry = c.impulse_cancellation();
        let poison = registry.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poison.0.lock().unwrap();
            panic!("controlled fixture poison")
        })
        .join();
        // Even reconnect cannot revive a poisoned cancellation registry.
        c.disconnected();
        c.mark_connected("channel");
        assert!(source.cancellation().load(Ordering::SeqCst));
        assert!(c.impulse_source("twitch", c.route_id().unwrap(), &p).is_none());
        registry.cancel();
        assert!(source.cancellation().load(Ordering::SeqCst));
    }
}

#[cfg(test)]
mod impulse_bound_job_tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    #[test]
    fn normal_job_completion_does_not_cancel_native_connection() {
        let route = ImpulseRoute::default();
        let a = Arc::new(AtomicBool::new(false));
        route.bind(&a).unwrap();
        a.store(true, Ordering::SeqCst);
        assert!(!route.cancel.load(Ordering::SeqCst));
        let b = Arc::new(AtomicBool::new(false));
        route.bind(&b).unwrap();
        assert!(!b.load(Ordering::SeqCst));
        route.cancel();
        assert!(b.load(Ordering::SeqCst));
    }
    #[test]
    fn source_cancels_all_live_jobs_and_bounds_retention() {
        let route = ImpulseRoute::default();
        let jobs: Vec<_> = (0..64).map(|_| Arc::new(AtomicBool::new(false))).collect();
        for job in &jobs {
            route.bind(job).unwrap();
        }
        let overflow = Arc::new(AtomicBool::new(false));
        assert!(route.bind(&overflow).is_err());
        assert!(overflow.load(Ordering::SeqCst));
        route.cancel();
        assert!(jobs.iter().all(|job| job.load(Ordering::SeqCst)));
        let late = Arc::new(AtomicBool::new(false));
        assert!(route.bind(&late).is_err());
        assert!(late.load(Ordering::SeqCst));
    }
}
