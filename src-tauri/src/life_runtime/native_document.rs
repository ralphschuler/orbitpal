//! Private authentication for one actual native WebView document. A document
//! capability is not a Scene, layout, renderer-readiness or action grant.
//! Callbacks use this short lock before Core; no caller code is invoked here.
use crate::{life_domain::{ActorId, MAX_COUNTER}, window_signals::WindowSignals};
use std::{collections::BTreeMap, sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex, MutexGuard, OnceLock}};

const MAX_DOCUMENTS: usize = 64;
const FIRST_REGISTER_MS: u64 = 5_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DocumentError { Invalid, Stale, Pending, Expired, Capacity, Unavailable }

#[derive(Clone)]
pub(crate) struct DocumentProof {
    bot: String,
    signals: Arc<WindowSignals>,
    epoch: u64,
    cancel: Arc<AtomicBool>,
}
impl DocumentProof {
    pub(crate) fn bot(&self) -> &str { &self.bot }
    pub(crate) fn signals(&self) -> &Arc<WindowSignals> { &self.signals }
    pub(crate) fn current(&self) -> bool { !self.cancel.load(Ordering::SeqCst) }
    pub(crate) fn same_document(&self, other: &Self) -> bool {
        self.bot == other.bot && self.epoch == other.epoch
            && Arc::ptr_eq(&self.signals, &other.signals) && Arc::ptr_eq(&self.cancel, &other.cancel)
    }
}
/// Deliberately no Debug/serde implementation: only retained native bootstrap
/// code receives the capability, and may retry evaluation with the same value.
#[derive(Clone)]
pub(crate) struct DocumentGrant { capability: String, proof: DocumentProof, issued_at: u64, delivery: Arc<Delivery> }
impl DocumentGrant {
    pub(crate) fn capability(&self) -> &str { &self.capability }
    pub(crate) fn proof(&self) -> DocumentProof { self.proof.clone() }
    pub(crate) fn issued_at(&self) -> u64 { self.issued_at }
    /// One cancellation identity per capability, allocated before any worker
    /// can wait or evaluate it. A retry cannot manufacture a new watch atom.
    pub(crate) fn delivery_cancel(&self) -> Arc<AtomicBool> { self.delivery.cancel.clone() }
    /// May be inspected before the exact lease is bound. This is cancellation
    /// only, not authentication or native scene authority.
    pub(crate) fn delivery_current(&self) -> bool { !self.delivery.cancel.load(Ordering::SeqCst) }
}
struct Delivery {
    cancel: Arc<AtomicBool>,
    lease: OnceLock<Arc<AtomicBool>>,
}
impl Default for Delivery {
    fn default() -> Self { Self { cancel: Arc::new(AtomicBool::new(false)), lease: OnceLock::new() } }
}
impl Delivery {
    fn bound_current(&self) -> bool {
        !self.cancel.load(Ordering::SeqCst)
            && self.lease.get().is_some_and(|lease| !lease.load(Ordering::SeqCst))
    }
}
struct Authentication {
    document: DocumentProof,
    delivery: Arc<Delivery>,
    mount: String,
    warm_frame: u64,
    cancel: AtomicBool,
}
#[derive(Clone)]
pub(crate) struct AuthReceipt(Arc<Authentication>);
impl AuthReceipt {
    pub(crate) fn document(&self) -> &DocumentProof { &self.0.document }
    pub(crate) fn mount_id(&self) -> &str { &self.0.mount }
    pub(crate) fn normal_warm_frame_sequence(&self) -> u64 { self.0.warm_frame }
    pub(crate) fn current(&self) -> bool {
        self.0.document.current() && self.0.delivery.bound_current() && !self.0.cancel.load(Ordering::SeqCst)
    }
    pub(crate) fn same_auth(&self, other: &Self) -> bool { Arc::ptr_eq(&self.0, &other.0) }
}
enum Phase {
    Pending,
    Finished { issued_at: u64, capability: String, delivery: Arc<Delivery>, pinned: Option<AuthReceipt> },
}
struct Document { epoch: u64, cancel: Arc<AtomicBool>, phase: Phase }
impl Document {
    fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Phase::Finished { delivery, pinned, .. } = &self.phase {
            delivery.cancel.store(true, Ordering::SeqCst);
            if let Some(auth) = pinned { auth.0.cancel.store(true, Ordering::SeqCst); }
        }
    }
}
impl Drop for Document { fn drop(&mut self) { self.cancel(); } }
struct Entry { signals: Arc<WindowSignals>, document: Option<Document> }
struct State { entries: BTreeMap<String, Entry>, next: u64, unavailable: bool }
impl Default for State {
    fn default() -> Self { Self { entries: BTreeMap::new(), next: 1, unavailable: false } }
}
impl State {
    fn cancel_all(&mut self) {
        self.unavailable = true;
        for entry in self.entries.values() {
            if let Some(document) = &entry.document { document.cancel(); }
        }
    }
    fn epoch(&mut self) -> Result<u64, DocumentError> {
        let epoch = self.next;
        let Some(next) = epoch.checked_add(1).filter(|next| *next <= MAX_COUNTER) else {
            self.cancel_all(); return Err(DocumentError::Unavailable);
        };
        self.next = next;
        Ok(epoch)
    }
}
#[derive(Default)]
pub(crate) struct Documents { state: Mutex<State> }

fn valid_bot(bot: &str) -> bool { ActorId::new(bot).is_ok() }
fn valid_mount(mount: &str) -> bool {
    !mount.is_empty() && mount.len() <= 96
        && mount.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
}
fn valid_capability(capability: &str) -> bool {
    capability.len() == 64 && capability.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn same_capability(a: &str, b: &str) -> bool {
    // Callers validate equal fixed length before this comparison.
    a.as_bytes().iter().zip(b.as_bytes()).fold(0u8, |different, (a, b)| different | (a ^ b)) == 0
}
fn capability() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}
fn proof(bot: &str, entry: &Entry, document: &Document) -> DocumentProof {
    DocumentProof { bot: bot.into(), signals: entry.signals.clone(), epoch: document.epoch, cancel: document.cancel.clone() }
}

impl Documents {
    fn lock(&self) -> Result<MutexGuard<'_, State>, DocumentError> {
        let state = match self.state.lock() {
            Ok(state) => state,
            Err(error) => {
                error.into_inner().cancel_all();
                return Err(DocumentError::Unavailable);
            }
        };
        if state.unavailable { Err(DocumentError::Unavailable) } else { Ok(state) }
    }
    /// Must precede builder navigation callbacks. Repeated same-instance
    /// tracking is inert; a replacement first drops/cancels the old document.
    pub(crate) fn track(&self, bot: &str, signals: &Arc<WindowSignals>) -> bool {
        if !valid_bot(bot) { return false; }
        let Ok(mut state) = self.lock() else { return false; };
        if state.entries.get(bot).is_some_and(|entry| Arc::ptr_eq(&entry.signals, signals)) { return true; }
        if !state.entries.contains_key(bot) && state.entries.len() >= MAX_DOCUMENTS { return false; }
        // Removal cancels before the new instance is made discoverable.
        state.entries.remove(bot);
        state.entries.insert(bot.into(), Entry { signals: signals.clone(), document: None });
        true
    }
    pub(crate) fn started(&self, bot: &str, signals: &Arc<WindowSignals>) -> bool {
        let Ok(mut state) = self.lock() else { return false; };
        if state.entries.get(bot).is_none_or(|entry| !Arc::ptr_eq(&entry.signals, signals)) { return false; }
        // Revocation precedes allocation/publication of the next page epoch.
        state.entries.get_mut(bot).unwrap().document.take();
        let Ok(epoch) = state.epoch() else { return false; };
        state.entries.get_mut(bot).unwrap().document = Some(Document {
            epoch, cancel: Arc::new(AtomicBool::new(false)), phase: Phase::Pending,
        });
        true
    }
    pub(crate) fn finished(&self, bot: &str, signals: &Arc<WindowSignals>, now: u64) -> Result<DocumentGrant, DocumentError> {
        if now > MAX_COUNTER { return Err(DocumentError::Invalid); }
        let mut state = self.lock()?;
        let entry = state.entries.get_mut(bot).filter(|entry| Arc::ptr_eq(&entry.signals, signals)).ok_or(DocumentError::Stale)?;
        let document = entry.document.as_mut().ok_or(DocumentError::Pending)?;
        if document.cancel.load(Ordering::SeqCst) { return Err(DocumentError::Stale); }
        if matches!(&document.phase, Phase::Pending) {
            document.phase = Phase::Finished { issued_at: now, capability: capability(), delivery: Arc::new(Delivery::default()), pinned: None };
        }
        let Phase::Finished { issued_at, capability, delivery, .. } = &document.phase else { unreachable!() };
        if now < *issued_at { return Err(DocumentError::Expired); }
        let grant = DocumentGrant {
            capability: capability.clone(),
            proof: DocumentProof { bot: bot.into(), signals: entry.signals.clone(), epoch: document.epoch, cancel: document.cancel.clone() },
            issued_at: *issued_at,
            delivery: delivery.clone(),
        };
        Ok(grant)
    }
    /// Native admission/denial can revoke a current document before any Core
    /// wait without pretending that a page-load event has occurred.
    pub(crate) fn invalidate(&self, bot: &str, signals: &Arc<WindowSignals>) -> bool {
        let Ok(state) = self.lock() else { return false; };
        let Some(entry) = state.entries.get(bot).filter(|entry| Arc::ptr_eq(&entry.signals, signals)) else { return false; };
        let Some(document) = &entry.document else { return false; };
        document.cancel(); true
    }
    /// Observe an already Finished page only. Operator reconciliation must
    /// never manufacture a Finished event for a page still loading.
    pub(crate) fn current_grant(&self, bot: &str, signals: &Arc<WindowSignals>, now: u64) -> Result<DocumentGrant, DocumentError> {
        if now > MAX_COUNTER { return Err(DocumentError::Invalid); }
        let state = self.lock()?;
        let entry = state.entries.get(bot).filter(|entry| Arc::ptr_eq(&entry.signals, signals)).ok_or(DocumentError::Stale)?;
        let document = entry.document.as_ref().ok_or(DocumentError::Pending)?;
        if document.cancel.load(Ordering::SeqCst) { return Err(DocumentError::Stale); }
        let Phase::Finished { issued_at, capability, delivery, .. } = &document.phase else { return Err(DocumentError::Pending); };
        if now < *issued_at { return Err(DocumentError::Expired); }
        Ok(DocumentGrant { capability: capability.clone(), proof: proof(bot, entry, document), issued_at: *issued_at, delivery: delivery.clone() })
    }
    /// Native semantic rotation in the same actually loaded document. Root
    /// must first complete old surface/physical cleanup and recheck its scene.
    /// A real navigation still Pending cannot be promoted by this operation.
    pub(crate) fn rotate(&self, bot: &str, signals: &Arc<WindowSignals>, now: u64) -> Result<DocumentGrant, DocumentError> {
        if now > MAX_COUNTER { return Err(DocumentError::Invalid); }
        let mut state = self.lock()?;
        let entry = state.entries.get(bot).filter(|entry| Arc::ptr_eq(&entry.signals, signals)).ok_or(DocumentError::Stale)?;
        let old = entry.document.as_ref().ok_or(DocumentError::Pending)?;
        if old.cancel.load(Ordering::SeqCst) { return Err(DocumentError::Stale); }
        match &old.phase {
            Phase::Pending => return Err(DocumentError::Pending),
            Phase::Finished { issued_at, .. } if now < *issued_at => return Err(DocumentError::Expired),
            _ => (),
        }
        let epoch = state.epoch()?;
        let entry = state.entries.get_mut(bot).unwrap();
        entry.document.take();
        let document = Document { epoch, cancel: Arc::new(AtomicBool::new(false)), phase: Phase::Finished { issued_at: now, capability: capability(), delivery: Arc::new(Delivery::default()), pinned: None } };
        let Phase::Finished { capability, delivery, .. } = &document.phase else { unreachable!() };
        let grant = DocumentGrant { capability: capability.clone(), proof: proof(bot, entry, &document), issued_at: now, delivery: delivery.clone() };
        entry.document = Some(document);
        Ok(grant)
    }
    /// Attach only the original delivery's exact lease. OutputGate/system
    /// watches are registered before this short lock; no gate/Core/GUI work is
    /// called here. Even an already-cancelled first lease is recorded forever.
    pub(crate) fn bind_delivery(&self, grant: &DocumentGrant, delivery_cancel: Arc<AtomicBool>, lease_cancel: Arc<AtomicBool>) -> Result<(), DocumentError> {
        let state = self.lock()?;
        let entry = state.entries.get(grant.proof.bot()).filter(|entry| Arc::ptr_eq(&entry.signals, grant.proof.signals())).ok_or(DocumentError::Stale)?;
        let document = entry.document.as_ref().ok_or(DocumentError::Pending)?;
        if !proof(grant.proof.bot(), entry, document).same_document(&grant.proof) || !grant.proof.current() { return Err(DocumentError::Stale); }
        let Phase::Finished { capability, delivery, .. } = &document.phase else { return Err(DocumentError::Pending); };
        if !same_capability(capability, grant.capability()) || !Arc::ptr_eq(delivery, &grant.delivery)
            || !Arc::ptr_eq(&delivery.cancel, &delivery_cancel) { return Err(DocumentError::Stale); }
        if let Some(original) = delivery.lease.get() {
            if !Arc::ptr_eq(original, &lease_cancel) { return Err(DocumentError::Stale); }
        } else {
            // Documents serializes all bindings. OnceLock prevents later code
            // from replacing the lease or clearing a terminal cancellation.
            delivery.lease.set(lease_cancel).map_err(|_| DocumentError::Stale)?;
        }
        if !delivery.bound_current() { return Err(DocumentError::Stale); }
        Ok(())
    }
    pub(crate) fn authenticate(&self, label: &str, capability: &str, mount: &str, warm: Option<u64>, now: u64) -> Result<AuthReceipt, DocumentError> {
        let bot = label.strip_prefix("avatar-").filter(|bot| valid_bot(bot)).ok_or(DocumentError::Invalid)?;
        if !valid_capability(capability) || !valid_mount(mount) || now > MAX_COUNTER
            || warm.is_some_and(|frame| frame == 0 || frame > MAX_COUNTER) { return Err(DocumentError::Invalid); }
        let mut state = self.lock()?;
        let entry = state.entries.get_mut(bot).ok_or(DocumentError::Stale)?;
        let document = entry.document.as_mut().ok_or(DocumentError::Pending)?;
        if document.cancel.load(Ordering::SeqCst) { return Err(DocumentError::Stale); }
        let Phase::Finished { issued_at, capability: expected, delivery, pinned } = &mut document.phase else { return Err(DocumentError::Pending); };
        if !same_capability(expected, capability) { return Err(DocumentError::Stale); }
        if now < *issued_at { return Err(DocumentError::Expired); }
        if let Some(auth) = pinned {
            if auth.mount_id() != mount || warm.is_some_and(|frame| frame != auth.normal_warm_frame_sequence()) || !auth.current() { return Err(DocumentError::Stale); }
            return Ok(auth.clone());
        }
        if now - *issued_at >= FIRST_REGISTER_MS { return Err(DocumentError::Expired); }
        let warm_frame = warm.ok_or(DocumentError::Invalid)?;
        if !delivery.bound_current() { return Err(DocumentError::Stale); }
        let auth = AuthReceipt(Arc::new(Authentication {
            document: DocumentProof { bot: bot.into(), signals: entry.signals.clone(), epoch: document.epoch, cancel: document.cancel.clone() },
            delivery: delivery.clone(),
            mount: mount.into(), warm_frame, cancel: AtomicBool::new(false),
        }));
        *pinned = Some(auth.clone());
        Ok(auth)
    }
    /// Reports cannot establish or guess a mount. Only the current document's
    /// already pinned exact authentication may be observed through this seam.
    pub(crate) fn authenticate_bound(&self, label: &str, capability: &str, now: u64) -> Result<AuthReceipt, DocumentError> {
        let bot = label.strip_prefix("avatar-").filter(|bot| valid_bot(bot)).ok_or(DocumentError::Invalid)?;
        if !valid_capability(capability) || now > MAX_COUNTER { return Err(DocumentError::Invalid); }
        let state = self.lock()?;
        let entry = state.entries.get(bot).ok_or(DocumentError::Stale)?;
        let document = entry.document.as_ref().ok_or(DocumentError::Pending)?;
        if document.cancel.load(Ordering::SeqCst) { return Err(DocumentError::Stale); }
        let Phase::Finished { issued_at, capability: expected, pinned, .. } = &document.phase else { return Err(DocumentError::Pending); };
        if !same_capability(expected, capability) { return Err(DocumentError::Stale); }
        if now < *issued_at { return Err(DocumentError::Expired); }
        pinned.as_ref().filter(|auth| auth.current()).cloned().ok_or(DocumentError::Stale)
    }
    /// Same capability/mount remains terminal. Native rotate (or a real new
    /// document) is required for new authority; stale unregister is inert.
    pub(crate) fn unregister(&self, auth: &AuthReceipt) -> bool {
        let Ok(state) = self.lock() else { return false; };
        let Some(entry) = state.entries.get(auth.document().bot()).filter(|entry| Arc::ptr_eq(&entry.signals, auth.document().signals())) else { return false; };
        let Some(document) = &entry.document else { return false; };
        if !proof(auth.document().bot(), entry, document).same_document(auth.document()) { return false; }
        let Phase::Finished { pinned: Some(current), .. } = &document.phase else { return false; };
        current.same_auth(auth) && !current.0.cancel.swap(true, Ordering::SeqCst)
    }
    pub(crate) fn retire(&self, bot: &str, signals: &Arc<WindowSignals>) -> bool {
        let Ok(mut state) = self.lock() else { return false; };
        if state.entries.get(bot).is_none_or(|entry| !Arc::ptr_eq(&entry.signals, signals)) { return false; }
        state.entries.remove(bot);
        true
    }
}
impl Drop for Documents {
    fn drop(&mut self) {
        match self.state.get_mut() {
            Ok(state) => state.cancel_all(),
            Err(error) => error.into_inner().cancel_all(),
        }
    }
}

#[cfg(test)]
#[path = "native_document_tests.rs"]
mod tests;
