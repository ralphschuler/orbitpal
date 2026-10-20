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
#[derive(Default)]
struct State {
    generation: u64,
    watches: BTreeMap<usize, Watch>,
    ui: Option<(String, u64)>,
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
        if s.generation != ticket
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
            s.ui = Some((id.into(), 0));
        }
    }
    pub(crate) fn ui_disconnect(&self, id: &str) {
        if let Ok(mut s) = self.state.lock() {
            if s.ui.as_ref().is_some_and(|(current, _)| current == id) {
                Self::revoke_locked(&mut s, |_| true);
                s.ui = None;
            }
        }
    }
    /// This mirrored native nonce/sequence is cancellation-only. It cannot
    /// accept a Core report, clear a block or grant output/session authority.
    pub(crate) fn ui_report(&self, id: &str, sequence: u64, block_or_activity: bool) {
        if let Ok(mut s) = self.state.lock() {
            let Some((current, prior)) = &mut s.ui else {
                return;
            };
            if current != id || sequence <= *prior || sequence > crate::life_domain::MAX_COUNTER {
                return;
            }
            *prior = sequence;
            if block_or_activity {
                Self::revoke_locked(&mut s, |_| true);
            }
        }
    }
    pub(crate) fn window(&self, label: &str) {
        self.revoke(|w| label == "main" || w.window == label);
    }
    pub(crate) fn bot(&self, bot: Option<&str>) {
        self.revoke(|w| bot.is_none_or(|id| id == w.bot));
    }
}
