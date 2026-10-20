//! Non-Core native output revocation. Native events only cancel; they never
//! grant output presence. GUI sampling precedes registration, fenced by ticket.
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Weak,
    },
};
const LIMIT: usize = 64;
struct Watch {
    bot: String,
    window: String,
    cancel: Weak<AtomicBool>,
}
struct SurfaceWatch {
    bot: String,
    window: String,
    connection: String,
    binding: u64,
    cancel: Weak<AtomicBool>,
}
#[derive(Default)]
struct State {
    generation: u64,
    surfaces: BTreeMap<String, SurfaceWatch>,
    watches: BTreeMap<usize, Watch>,
    ui: Option<(String, u64, u64)>,
}
#[derive(Default)]
pub(crate) struct OutputGate {
    state: Mutex<State>,
}
impl OutputGate {
    pub(crate) fn ticket(&self) -> Option<u64> {
        self.state.lock().ok().map(|s| s.generation)
    }
    pub(crate) fn watch(
        &self,
        ticket: u64,
        bot: &str,
        window: &str,
        cancel: &Arc<AtomicBool>,
        ui: (&str, u64),
    ) -> bool {
        let Ok(mut s) = self.state.lock() else {
            cancel.store(true, Ordering::SeqCst);
            return false;
        };
        s.watches.retain(|_, w| {
            w.cancel
                .upgrade()
                .is_some_and(|c| !c.load(Ordering::SeqCst))
        });
        let id = Arc::as_ptr(cancel) as usize;
        if s.ui
            .as_ref()
            .is_none_or(|(connection, sequence, last_revoke)| {
                connection != ui.0 || ui.1 == 0 || *sequence < ui.1 || *last_revoke > ui.1
            })
            || s.generation != ticket
            || ticket >= crate::life_domain::MAX_COUNTER
            || bot.is_empty()
            || bot.len() > 96
            || window.len() > 110
            || (!s.watches.contains_key(&id) && s.watches.len() >= LIMIT)
        {
            cancel.store(true, Ordering::SeqCst);
            return false;
        }
        if let Some(old) = s.watches.get(&id) {
            if old.bot != bot || old.window != window {
                cancel.store(true, Ordering::SeqCst);
                return false;
            }
        }
        s.watches.insert(
            id,
            Watch {
                bot: bot.into(),
                window: window.into(),
                cancel: Arc::downgrade(cancel),
            },
        );
        true
    }
    /// Core is already held. Events release this latch before acquiring Core.
    /// The closure mutates only the pure registry: no Store, GUI, await or emit.
    /// Stale mount/ticket requests are rejected BEFORE they can replace a binding.
    pub(crate) fn admit_surface(
        &self,
        ticket: u64,
        bot: &str,
        window: &str,
        ui: (&str, u64),
        create: impl FnOnce() -> Result<
            (
                crate::world_output::RegisteredWorldActor,
                Arc<AtomicBool>,
                bool,
            ),
            crate::world_output::RenderReason,
        >,
    ) -> Result<crate::world_output::RegisteredWorldActor, crate::world_output::RenderReason> {
        use crate::world_output::RenderReason;
        let mut state = self.state.lock().map_err(|_| RenderReason::Cancelled)?;
        state.surfaces.retain(|_, watch| {
            watch
                .cancel
                .upgrade()
                .is_some_and(|c| !c.load(Ordering::SeqCst))
        });
        if state.generation != ticket
            || ticket >= crate::life_domain::MAX_COUNTER
            || !crate::storage::valid_id(bot)
            || window != "main"
            || ui.1 == 0
            || state
                .ui
                .as_ref()
                .is_none_or(|(id, seq, last)| id != ui.0 || *seq < ui.1 || *last > ui.1)
        {
            return Err(RenderReason::StaleBinding);
        }
        // Production's one Studio registry has at most eight layers. Never
        // allocate another unobserved watch if an invariant is violated.
        if state.surfaces.len() >= LIMIT {
            return Err(RenderReason::Capacity);
        }
        let (reply, cancel, created) = create()?;
        if created {
            // A newly-issued binding fences registrations already awaiting an
            // earlier topology/Main sample, including an old mount in this UI.
            state.generation = state
                .generation
                .saturating_add(1)
                .min(crate::life_domain::MAX_COUNTER);
        }
        state.surfaces.insert(
            reply.render_session_id.clone(),
            SurfaceWatch {
                bot: bot.into(),
                window: window.into(),
                connection: ui.0.into(),
                binding: reply.binding_sequence,
                cancel: Arc::downgrade(&cancel),
            },
        );
        Ok(reply)
    }
    #[cfg(test)]
    pub(crate) fn watch_surface(
        &self,
        ticket: u64,
        bot: &str,
        window: &str,
        render: &str,
        binding: u64,
        cancel: &Arc<AtomicBool>,
        ui: (&str, u64),
    ) -> bool {
        let Ok(mut s) = self.state.lock() else {
            cancel.store(true, Ordering::SeqCst);
            return false;
        };
        s.surfaces.retain(|_, w| {
            w.cancel
                .upgrade()
                .is_some_and(|c| !c.load(Ordering::SeqCst))
        });
        let valid = s.generation == ticket
            && ticket < crate::life_domain::MAX_COUNTER
            && s.ui
                .as_ref()
                .is_some_and(|(id, seq, last)| id == ui.0 && *seq >= ui.1 && *last <= ui.1)
            && crate::world_output::id(render)
            && crate::world_output::counter(binding)
            && (s.surfaces.contains_key(render) || s.surfaces.len() < LIMIT);
        // A stale metadata renewal cannot revoke an unchanged live surface.
        // The caller cancels a newly-created, unregistered candidate only.
        if !valid {
            return false;
        }
        if let Some(old) = s.surfaces.get(render) {
            if old.bot != bot
                || old.window != window
                || old.connection != ui.0
                || old.binding != binding
                || !old
                    .cancel
                    .upgrade()
                    .is_some_and(|c| Arc::ptr_eq(&c, cancel))
            {
                return false;
            }
        }
        s.surfaces.insert(
            render.into(),
            SurfaceWatch {
                bot: bot.into(),
                window: window.into(),
                connection: ui.0.into(),
                binding,
                cancel: Arc::downgrade(cancel),
            },
        );
        true
    }
    fn surface_revoke(s: &mut State, predicate: impl Fn(&SurfaceWatch) -> bool) {
        s.surfaces.retain(|_, w| {
            let Some(c) = w.cancel.upgrade() else {
                return false;
            };
            if predicate(w) {
                c.store(true, Ordering::SeqCst);
            }
            !c.load(Ordering::SeqCst)
        });
    }
    /// Geometry/profile/window invalidation is cancellation-only and precedes Core.
    pub(crate) fn geometry(&self, bot: Option<&str>) {
        if let Ok(mut s) = self.state.lock() {
            Self::revoke_locked(&mut s, |w| bot.is_none_or(|b| b == w.bot));
            Self::surface_revoke(&mut s, |w| bot.is_none_or(|b| b == w.bot));
        }
    }
    pub(crate) fn unregister_surface(&self, window: &str, render: &str, binding: u64) -> bool {
        let Ok(mut s) = self.state.lock() else {
            return false;
        };
        let ui = s.ui.as_ref().map(|v| v.0.clone());
        let valid = s.surfaces.get(render).is_some_and(|w| {
            w.window == window && w.binding == binding && Some(&w.connection) == ui.as_ref()
        });
        if valid {
            if let Some(w) = s.surfaces.remove(render) {
                if let Some(c) = w.cancel.upgrade() {
                    c.store(true, Ordering::SeqCst);
                }
            }
        }
        valid
    }
    fn revoke_locked(s: &mut State, predicate: impl Fn(&Watch) -> bool) {
        s.generation = s
            .generation
            .saturating_add(1)
            .min(crate::life_domain::MAX_COUNTER);
        s.watches.retain(|_, w| {
            let Some(c) = w.cancel.upgrade() else {
                return false;
            };
            if predicate(w) {
                c.store(true, Ordering::SeqCst);
            }
            !c.load(Ordering::SeqCst)
        });
    }
    fn revoke(&self, predicate: impl Fn(&Watch) -> bool) {
        if let Ok(mut s) = self.state.lock() {
            Self::revoke_locked(&mut s, predicate);
        }
    }
    pub(crate) fn ui_connect(&self, id: &str) {
        if id.is_empty() || id.len() > 96 {
            return;
        }
        if let Ok(mut s) = self.state.lock() {
            Self::revoke_locked(&mut s, |_| true);
            Self::surface_revoke(&mut s, |_| true);
            s.ui = Some((id.into(), 0, 0));
        }
    }
    pub(crate) fn ui_disconnect(&self, id: &str) {
        if let Ok(mut s) = self.state.lock() {
            if s.ui.as_ref().is_some_and(|(current, _, _)| current == id) {
                Self::revoke_locked(&mut s, |_| true);
                Self::surface_revoke(&mut s, |_| true);
                s.ui = None;
            }
        }
    }
    /// This mirrored native nonce/sequence is cancellation-only. It cannot
    /// accept a Core report, clear a block or grant output/session authority.
    pub(crate) fn ui_report(&self, id: &str, sequence: u64, block_or_activity: bool) {
        if let Ok(mut s) = self.state.lock() {
            let Some((current, prior, last_revoke)) = &mut s.ui else {
                return;
            };
            if current != id || sequence <= *prior || sequence > crate::life_domain::MAX_COUNTER {
                return;
            }
            *prior = sequence;
            if block_or_activity {
                *last_revoke = sequence;
                Self::revoke_locked(&mut s, |_| true);
            }
        }
    }
    pub(crate) fn window(&self, label: &str) {
        self.revoke(|w| label == "main" || w.window == label);
        if let Ok(mut s) = self.state.lock() {
            Self::surface_revoke(&mut s, |w| label == "main" || w.window == label);
        }
    }
    pub(crate) fn bot(&self, bot: Option<&str>) {
        self.revoke(|w| bot.is_none_or(|id| id == w.bot));
    }
}
