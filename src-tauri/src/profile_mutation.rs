//! Cancellation-only, process/Store-family observation fence. This does not
//! serialize profile writes or authorize a native policy/kernel operation.
use std::sync::{atomic::{AtomicBool,AtomicU64,Ordering},Arc};

#[derive(Default)]
pub(crate) struct ProfileMutations {generation:AtomicU64,writers:AtomicU64,invalid:AtomicBool}
pub(crate) struct ProfileWrite {fence:Arc<ProfileMutations>,counted:bool}
pub(crate) struct PolicyReceipt {fence:Arc<ProfileMutations>,generation:u64}
pub(crate) struct NativeLayoutPolicy {pub(crate) clicks:bool,pub(crate) top:bool,receipt:PolicyReceipt}
impl NativeLayoutPolicy {
    pub(crate) fn current(&self)->bool{self.receipt.current()}
    pub(super) fn new(clicks:bool,top:bool,receipt:PolicyReceipt)->Self{Self{clicks,top,receipt}}
}
impl ProfileMutations {
    fn advance(&self){if self.generation.fetch_update(Ordering::SeqCst,Ordering::SeqCst,|n|n.checked_add(1)).is_err(){self.invalid.store(true,Ordering::SeqCst);}}
    pub(super) fn begin(self:&Arc<Self>)->ProfileWrite{
        // Active becomes visible BEFORE generation advances and before any
        // validation/DB wait/write. Overlapping writers cannot expose stable
        // old DB values under a new generation.
        let counted=self.writers.fetch_update(Ordering::SeqCst,Ordering::SeqCst,|n|n.checked_add(1)).is_ok();
        if !counted{self.invalid.store(true,Ordering::SeqCst);}
        self.advance();ProfileWrite{fence:self.clone(),counted}
    }
    pub(super) fn receipt(self:&Arc<Self>)->Option<PolicyReceipt>{
        if self.invalid.load(Ordering::SeqCst)||self.writers.load(Ordering::SeqCst)!=0{return None;}
        let generation=self.generation.load(Ordering::SeqCst);
        let receipt=PolicyReceipt{fence:self.clone(),generation};
        receipt.current().then_some(receipt)
    }
}
impl Drop for ProfileWrite {fn drop(&mut self){self.fence.advance();if self.counted{self.fence.writers.fetch_sub(1,Ordering::SeqCst);}}}
impl PolicyReceipt {
    pub(crate) fn current(&self)->bool{
        !self.fence.invalid.load(Ordering::SeqCst)
            &&self.fence.writers.load(Ordering::SeqCst)==0
            &&self.fence.generation.load(Ordering::SeqCst)==self.generation
            &&self.fence.writers.load(Ordering::SeqCst)==0
            &&self.fence.generation.load(Ordering::SeqCst)==self.generation
            &&!self.fence.invalid.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlapping_writes_never_publish_an_intermediate_stable_receipt(){
        let f=Arc::new(ProfileMutations::default());let old=f.receipt().unwrap();let a=f.begin();assert!(!old.current());assert!(f.receipt().is_none());let b=f.begin();drop(a);assert!(f.receipt().is_none());drop(b);assert!(f.receipt().unwrap().current());assert!(!old.current());
    }
    #[test]
    fn exhausted_counters_never_wrap_or_restore_native_policy_availability(){
        for writers in [false,true]{let f=Arc::new(ProfileMutations::default());let old=f.receipt().unwrap();if writers{f.writers.store(u64::MAX,Ordering::SeqCst);}else{f.generation.store(u64::MAX,Ordering::SeqCst);}let write=f.begin();assert!(!old.current());drop(write);assert!(f.receipt().is_none());assert!(f.invalid.load(Ordering::SeqCst));}
    }
    #[test]
    fn actual_store_scopes_share_fence_across_overlapping_blocked_profile_writes(){
        let root=std::env::temp_dir().join(format!("orbitpal-layout-policy-{}",uuid::Uuid::new_v4()));
        let store=super::super::Store::new(root.clone()).unwrap();
        let original=store.profile().unwrap();let id=original["id"].as_str().unwrap().to_owned();
        let a=store.scoped(&id).unwrap();let b=store.scoped(&id).unwrap();
        assert!(Arc::ptr_eq(&store.profile_mutations,&a.profile_mutations));assert!(Arc::ptr_eq(&a.profile_mutations,&b.profile_mutations));
        let old=store.native_layout_policy(&id).unwrap();
        let mut pa=original.clone();pa["output"]["clickThrough"]=serde_json::json!(true);
        let mut pb=original;pb["output"]["alwaysOnTop"]=serde_json::json!(true);
        // A real Store connection mutex holds the first writer inside its
        // normal save path; the second also enters the fence before waiting
        // for the unchanged existing extension-mutation lock.
        let db=a.db.lock().unwrap();
        std::thread::scope(|scope|{
            let first=scope.spawn(||a.save_profile(pa));let second=scope.spawn(||b.save_profile(pb));
            let until=std::time::Instant::now()+std::time::Duration::from_secs(3);
            while store.profile_mutations.writers.load(Ordering::SeqCst)!=2{assert!(std::time::Instant::now()<until,"both actual Store writes must enter the fence");std::thread::yield_now();}
            assert!(!old.current());assert!(store.native_layout_policy(&id).is_none());
            drop(db);first.join().unwrap().unwrap();second.join().unwrap().unwrap();
        });
        let policy=store.native_layout_policy(&id).unwrap();let actual=store.bot_profile(&id).unwrap();
        assert!(policy.current());assert_eq!(policy.clicks,actual["output"]["clickThrough"].as_bool().unwrap());assert_eq!(policy.top,actual["output"]["alwaysOnTop"].as_bool().unwrap());
        // Optional fence exhaustion never changes normal write success.
        store.profile_mutations.generation.store(u64::MAX,Ordering::SeqCst);
        store.save_profile(actual).unwrap();assert!(store.native_layout_policy(&id).is_none());
        drop((a,b,store));let _=std::fs::remove_dir_all(root);
    }
}
