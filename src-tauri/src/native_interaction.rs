//! A genuine native context menu; avatar click-through and layout are never changed.
use crate::{
    experience::{InteractionKind, InteractionRequest},
    AppState,
};
use serde::Serialize;
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};
use tauri::{
    menu::{Menu, MenuItem},
    Emitter, Manager, State,
};

/// Only the native owner adapter constructs this from the retained actual output.
/// The closure must join constructor, binding, route, local fence and cancellation.
#[derive(Clone)]
pub(crate) struct MenuOwner {
    bot: String,
    window: tauri::WebviewWindow,
    scope: String,
    signals: Arc<crate::window_signals::WindowSignals>,
    point: (f64, f64),
    current: Arc<dyn Fn() -> bool + Send + Sync>,
}
impl MenuOwner {
    pub(crate) fn is_current(&self)->bool { (self.current)() }
    pub(crate) fn new(
        bot: String,
        window: tauri::WebviewWindow,
        scope: String,
        signals: Arc<crate::window_signals::WindowSignals>,
        point: (f64, f64),
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<Self, String> {
        if !crate::experience::valid_id(&bot)
            || window.label() != format!("avatar-{bot}")
            || scope.is_empty()
            || scope.len() > 512
            || !point.0.is_finite()
            || !point.1.is_finite()
            || point.0 < 0.
            || point.1 < 0.
            || !current()
        {
            return Err("interaction_unavailable".into());
        }
        Ok(Self {
            bot,
            window,
            scope,
            signals,
            point,
            current,
        })
    }
}
#[derive(Default)]
pub struct NativeInteraction {
    lease: Mutex<Option<Lease>>,
}
struct Lease {
    gate: MenuGate,
    owner: MenuOwner,
    play_scope: Option<crate::life_runtime::social_host::PlayRequestScope>,
    items: Vec<MenuItem<tauri::Wry>>,
    _menu: Menu<tauri::Wry>,
}
#[derive(Clone)]
struct MenuGate {
    nonce: String,
    bot: String,
    scope: String,
    target_actor_id: Option<String>,
    opened: Instant,
    claimed: bool,
}
impl MenuGate {
    fn request(&self, kind: InteractionKind) -> InteractionRequest {
        InteractionRequest { kind, target_actor_id: if kind == InteractionKind::Play {
            self.target_actor_id.clone()
        } else { None } }
    }
    fn claim(
        &mut self,
        event: &str,
        bot: &str,
        scope: &str,
        current: bool,
        age_ms: u128,
    ) -> Option<InteractionKind> {
        if self.claimed || !current || self.bot != bot || self.scope != scope || age_ms > 15_000 {
            return None;
        }
        let suffix = event.strip_prefix(&format!("experience-menu-{}-", self.nonce))?;
        let kind = match suffix {
            "pet" => InteractionKind::Pet,
            "play" => InteractionKind::Play,
            "home" => InteractionKind::Home,
            "pause" => InteractionKind::Pause,
            _ => return None,
        };
        self.claimed = true;
        Some(kind)
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuOpened {
    version: u8,
    bot_id: String,
    menu_id: String,
    expires_in_ms: u16,
}
fn labels(language: &str) -> [&'static str; 4] {
    match language {
        "de" => [
            "Streicheln",
            "Zum Spielen einladen",
            "Zum Ruheplatz",
            "Pause anfordern",
        ],
        "fr" => [
            "Caresser",
            "Inviter à jouer",
            "Lieu de repos",
            "Demander une pause",
        ],
        "es" => [
            "Acariciar",
            "Invitar a jugar",
            "Lugar de descanso",
            "Pedir una pausa",
        ],
        _ => [
            "Pet",
            "Invite to play",
            "Go to rest spot",
            "Request a break",
        ],
    }
}
/// Register once during shared app setup, after managing NativeInteraction.
pub(crate) fn install(app: &tauri::AppHandle) {
    app.on_menu_event(|app,event|{
    if !event.id.as_ref().starts_with("experience-menu-"){return}
    let runtime=app.state::<NativeInteraction>();
    let selected={let Ok(mut current)=runtime.lease.lock()else{return};let Some(lease)=current.as_mut()else{return};let valid=(lease.owner.current)();let bot=lease.owner.bot.clone();let scope=lease.owner.scope.clone();let age=lease.gate.opened.elapsed().as_millis();let kind=lease.gate.claim(event.id.as_ref(),&bot,&scope,valid,age);kind.map(|kind|{let lease=current.take().unwrap();let request=lease.gate.request(kind);(lease.owner,request,lease.play_scope)})};
    let Some((owner,request,play_scope))=selected else{return};let app=app.clone();tauri::async_runtime::spawn(async move{
        if !(owner.current)(){return}let state=app.state::<AppState>();if state.updater.ensure_idle().is_err(){return}let kind=request.kind;
        let outcome=if kind==InteractionKind::Play {
            match request.target_actor_id.as_deref().zip(play_scope.as_ref()) {
                Some((target,scope))=>crate::life_runtime::social_host::request_selected_play(&app,&owner.bot,target,scope)
                    .map(|_|serde_json::json!({"version":1,"botId":owner.bot,"kind":kind,"status":"waiting","reason":null})),
                None=>Err("interaction_unavailable".into()),
            }
        }else{crate::life_runtime::request_character_interaction(&app,&owner.bot,&request).await};
        match outcome{Ok(result)=>{let _=app.emit_to("main","experience-interaction-result",result);},Err(_)=>{let _=app.emit_to("main","experience-interaction-result",serde_json::json!({"version":1,"botId":owner.bot,"kind":kind,"status":"declined","reason":"unavailable"}));}}
    });
});
}
/// Lifecycle invalidation is synchronous authority retirement; native item updates
/// are queued after releasing the short lock. Never call with Core/Store held.
pub(crate) fn cancel_window(
    app: &tauri::AppHandle,
    bot: &str,
    signals: &Arc<crate::window_signals::WindowSignals>,
) {
    let runtime = app.state::<NativeInteraction>();
    let nonce = runtime.lease.lock().ok().and_then(|lease| {
        lease
            .as_ref()
            .filter(|l| l.owner.bot == bot && Arc::ptr_eq(&l.owner.signals, signals))
            .map(|l| l.gate.nonce.clone())
    });
    if let Some(nonce) = nonce {
        cancel_exact(app, Some(bot), Some(&nonce));
    }
}
pub(crate) fn cancel(app: &tauri::AppHandle, bot: Option<&str>) {
    cancel_exact(app, bot, None);
}
fn cancel_exact(app: &tauri::AppHandle, bot: Option<&str>, nonce: Option<&str>) {
    let runtime = app.state::<NativeInteraction>();
    let removed = {
        let Ok(mut current) = runtime.lease.lock() else {
            return;
        };
        if current.as_ref().is_some_and(|l| {
            bot.is_none_or(|id| l.owner.bot == id) && nonce.is_none_or(|id| l.gate.nonce == id)
        }) {
            current.take()
        } else {
            None
        }
    };
    if let Some(lease) = removed {
        let _ = app.run_on_main_thread(move || {
            for item in lease.items {
                let _ = item.set_enabled(false);
            }
        });
    }
}
#[tauri::command]
pub async fn open_native_character_menu(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    bot_id: String,
    language: String,
    target_actor_id: Option<String>,
) -> Result<MenuOpened, String> {
    if window.label() != "main" {
        return Err("experience_main_only".into());
    }
    if target_actor_id.as_deref().is_some_and(|target|
        target == bot_id || !crate::experience::valid_id(target)) {
        return Err("interaction_unavailable".into());
    }
    state.updater.ensure_idle()?;
    let owner = crate::life_runtime::native_interaction_owner(&app, &bot_id)?;
    if owner.bot != bot_id || !(owner.current)() {
        return Err("interaction_unavailable".into());
    }
    let size = owner
        .window
        .inner_size()
        .map_err(|_| "interaction_unavailable")?;
    if owner.point.0 > f64::from(size.width) || owner.point.1 > f64::from(size.height) {
        return Err("interaction_unavailable".into());
    }
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let texts = labels(&language);
    let mut choices = crate::life_runtime::native_interaction_choices(&app, &bot_id);
    // The observer runs after choices has released Owned/Core; a retained scope
    // supplies no reservation and cannot survive either actor/output replacement.
    let play_scope = if choices[1] {crate::life_runtime::social_host::prepare_play(&app,&bot_id,target_actor_id.as_deref())} else {None};
    choices[1] &= play_scope.is_some();
    let mut items = vec![];
    for (index, kind) in ["pet", "play", "home", "pause"].iter().enumerate() {
        items.push(
            MenuItem::with_id(
                &app,
                format!("experience-menu-{nonce}-{kind}"),
                texts[index],
                choices[index],
                None::<&str>,
            )
            .map_err(|_| "interaction_unavailable")?,
        );
    }
    let menu = Menu::with_items(&app, &[&items[0], &items[1], &items[2], &items[3]])
        .map_err(|_| "interaction_unavailable")?;
    cancel(&app, None);
    if !(owner.current)() {
        return Err("interaction_unavailable".into());
    }
    let runtime = app.state::<NativeInteraction>();
    {
        let mut lease = runtime
            .lease
            .lock()
            .map_err(|_| "interaction_unavailable")?;
        *lease = Some(Lease {
            gate: MenuGate {
                nonce: nonce.clone(),
                bot: bot_id.clone(),
                scope: owner.scope.clone(),
                target_actor_id,
                opened: Instant::now(),
                claimed: false,
            },
            owner: owner.clone(),
            play_scope,
            items,
            _menu: menu.clone(),
        });
    }
    let expiry_app = app.clone();
    let expiry_nonce = nonce.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
        cancel_exact(&expiry_app, None, Some(&expiry_nonce));
    });
    if !(owner.current)() {
        cancel_exact(&app, Some(&bot_id), Some(&nonce));
        return Err("interaction_unavailable".into());
    }
    let result = owner.window.popup_menu_at(
        &menu,
        tauri::PhysicalPosition::new(owner.point.0, owner.point.1),
    );
    if result.is_err() {
        cancel_exact(&app, Some(&bot_id), Some(&nonce));
        return Err("interaction_unavailable".into());
    }
    Ok(MenuOpened {
        version: 1,
        bot_id,
        menu_id: nonce,
        expires_in_ms: 15_000,
    })
}
#[tauri::command]
pub fn close_native_character_menu(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    bot_id: String,
) -> Result<(), String> {
    if window.label() != "main" && window.label() != format!("avatar-{bot_id}") {
        return Err("experience_main_only".into());
    }
    if !crate::experience::valid_id(&bot_id) {
        return Err("interaction_unavailable".into());
    }
    cancel(&app, Some(&bot_id));
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn gate() -> MenuGate {
        MenuGate {
            nonce: "nonce".into(),
            bot: "nova".into(),
            scope: "owned_scope".into(),
            target_actor_id: Some("friend".into()),
            opened: Instant::now(),
            claimed: false,
        }
    }
    #[test]
    fn play_menu_retains_the_explicit_peer_and_other_choices_never_inherit_it() {
        let g = gate();
        assert_eq!(g.request(InteractionKind::Play).target_actor_id.as_deref(), Some("friend"));
        for kind in [InteractionKind::Pet, InteractionKind::Home, InteractionKind::Pause] {
            assert_eq!(g.request(kind).target_actor_id, None);
        }
        let mut no_peer = gate(); no_peer.target_actor_id = None;
        assert_eq!(no_peer.request(InteractionKind::Play).target_actor_id, None);
    }
    #[test]
    fn exact_native_menu_choice_is_claimed_once() {
        let mut g = gate();
        assert_eq!(
            g.claim("experience-menu-nonce-play", "nova", "owned_scope", true, 1),
            Some(InteractionKind::Play)
        );
        assert!(g
            .claim("experience-menu-nonce-play", "nova", "owned_scope", true, 2)
            .is_none());
    }
    #[test]
    fn stale_owner_route_nonce_scope_and_expiry_do_not_claim() {
        for (event, bot, scope, current, age) in [
            (
                "experience-menu-foreign-play",
                "nova",
                "owned_scope",
                true,
                1,
            ),
            ("experience-menu-nonce-play", "blue", "owned_scope", true, 1),
            ("experience-menu-nonce-play", "nova", "new_scope", true, 1),
            (
                "experience-menu-nonce-play",
                "nova",
                "owned_scope",
                false,
                1,
            ),
            (
                "experience-menu-nonce-play",
                "nova",
                "owned_scope",
                true,
                15001,
            ),
            (
                "experience-menu-nonce-shell",
                "nova",
                "owned_scope",
                true,
                1,
            ),
        ] {
            let mut g = gate();
            assert!(g.claim(event, bot, scope, current, age).is_none());
            assert!(!g.claimed);
        }
    }
}
