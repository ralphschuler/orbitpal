//! Practical current-session compatibility adapter. CGSessionCopyCurrentDictionary
//! is public; CGSSessionScreenIsLocked is a source-backed PRIVATE key, not an
//! Apple screen-lock guarantee. No session dictionary or identity is retained.
use super::{
    macos_state::{self, SessionSample},
    AvailabilityGate, Observation, Reason,
};
use block2::RcBlock;
use core_foundation_sys::{
    base::{CFGetTypeID, CFRelease},
    dictionary::{CFDictionaryGetTypeID, CFDictionaryGetValue, CFDictionaryRef},
    number::{
        kCFNumberSInt64Type, CFBooleanGetTypeID, CFBooleanGetValue, CFNumberGetTypeID,
        CFNumberGetValue,
    },
    string::{kCFStringEncodingUTF8, CFStringCreateWithCString},
};
use objc2::{
    msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, Bool},
};
use objc2_foundation::{
    NSDistributedNotificationCenter, NSNotification, NSNotificationCenter, NSOperationQueue,
    NSString,
};
use std::{
    cell::RefCell,
    collections::HashMap,
    ffi::{c_void, CStr},
    ptr::NonNull,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGSessionCopyCurrentDictionary() -> CFDictionaryRef;
}
#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {
    static NSWorkspaceWillSleepNotification: &'static NSString;
    static NSWorkspaceDidWakeNotification: &'static NSString;
    static NSWorkspaceSessionDidResignActiveNotification: &'static NSString;
    static NSWorkspaceSessionDidBecomeActiveNotification: &'static NSString;
    static NSApplicationProtectedDataWillBecomeUnavailableNotification: &'static NSString;
    static NSApplicationProtectedDataDidBecomeAvailableNotification: &'static NSString;
}
#[derive(Clone, Copy, Default)]
struct Blockers {
    sleeping: bool,
    inactive: bool,
    protected: bool,
}
struct Observer {
    center: Retained<NSNotificationCenter>,
    token: Retained<AnyObject>,
}
impl Drop for Observer {
    fn drop(&mut self) {
        unsafe {
            self.center.removeObserver(&self.token);
        }
    }
}
thread_local! {static OBSERVERS:RefCell<HashMap<String,Vec<Observer>>>=RefCell::new(HashMap::new());}
pub(super) struct Monitor {
    app: tauri::AppHandle,
    id: String,
    active: Arc<AtomicBool>,
    installed: Arc<AtomicBool>,
    blockers: Arc<Mutex<Blockers>>,
}
#[derive(Clone, Copy)]
enum Event {
    Sleep,
    Wake,
    Inactive,
    Active,
    Protected,
    Unprotected,
    Lock,
    Unlock,
}

// Keys are fixed source constants. Extraction enforces exact CF types, including
// CFBoolean rather than NSNumber truthiness, before the pure parser sees them.
unsafe fn value(dict: CFDictionaryRef, key: &CStr) -> *const c_void {
    let key =
        unsafe { CFStringCreateWithCString(std::ptr::null(), key.as_ptr(), kCFStringEncodingUTF8) };
    if key.is_null() {
        return std::ptr::null();
    }
    let result = unsafe { CFDictionaryGetValue(dict, key.cast()) };
    unsafe { CFRelease(key.cast()) };
    result
}
unsafe fn boolean(dict: CFDictionaryRef, key: &CStr) -> Option<bool> {
    let value = unsafe { value(dict, key) };
    if value.is_null() || unsafe { CFGetTypeID(value) } != unsafe { CFBooleanGetTypeID() } {
        None
    } else {
        Some(unsafe { CFBooleanGetValue(value.cast()) })
    }
}
unsafe fn user_id(dict: CFDictionaryRef) -> Option<u32> {
    let value = unsafe { value(dict, c"kCGSSessionUserIDKey") };
    if value.is_null() || unsafe { CFGetTypeID(value) } != unsafe { CFNumberGetTypeID() } {
        return None;
    }
    let mut uid = 0i64;
    if !unsafe {
        CFNumberGetValue(
            value.cast(),
            kCFNumberSInt64Type,
            (&mut uid as *mut i64).cast(),
        )
    } {
        return None;
    }
    u32::try_from(uid).ok()
}
fn sample_now(blockers: &Mutex<Blockers>) -> Observation {
    let Ok(flags) = blockers.lock().map(|flags| *flags) else {
        return Observation::unknown(Reason::MonitorDisconnected);
    };
    let Some(class) = AnyClass::get(c"NSApplication") else {
        return Observation::unknown(Reason::ObservationUnavailable);
    };
    let protected = unsafe {
        let app: Retained<AnyObject> = msg_send![class, sharedApplication];
        let responds: Bool =
            msg_send![&*app,respondsToSelector:objc2::sel!(isProtectedDataAvailable)];
        if responds.as_bool() {
            let value: Bool = msg_send![&*app, isProtectedDataAvailable];
            Some(value.as_bool())
        } else {
            None
        }
    };
    let dict = unsafe { CGSessionCopyCurrentDictionary() };
    if dict.is_null() {
        return Observation::unknown(Reason::ObservationUnavailable);
    }
    if unsafe { CFGetTypeID(dict.cast()) } != unsafe { CFDictionaryGetTypeID() } {
        unsafe { CFRelease(dict.cast()) };
        return Observation::unknown(Reason::ObservationUnavailable);
    }
    let sample = unsafe {
        SessionSample {
            uid: user_id(dict),
            process_uid: libc::getuid(),
            on_console: boolean(dict, c"kCGSSessionOnConsoleKey"),
            login_done: boolean(dict, c"kCGSessionLoginDoneKey"),
            locked: boolean(dict, c"CGSSessionScreenIsLocked"),
            protected_data: protected,
            sleeping: flags.sleeping,
            inactive: flags.inactive,
            protected_blocked: flags.protected,
        }
    };
    unsafe { CFRelease(dict.cast()) };
    macos_state::parse(sample)
}
fn observer(
    center: Retained<NSNotificationCenter>,
    name: &NSString,
    event: Event,
    gate: Arc<AvailabilityGate>,
    active: Arc<AtomicBool>,
    blockers: Arc<Mutex<Blockers>>,
) -> Observer {
    let callback = RcBlock::new(move |_: NonNull<NSNotification>| {
        if !active.load(Ordering::SeqCst) {
            return;
        }
        let Ok(mut flags) = blockers.lock() else {
            gate.block(Reason::MonitorDisconnected);
            return;
        };
        let reason = match event {
            Event::Sleep => {
                flags.sleeping = true;
                Reason::Sleeping
            }
            Event::Inactive => {
                flags.inactive = true;
                Reason::InactiveSession
            }
            Event::Protected => {
                flags.protected = true;
                Reason::ProtectedDataUnavailable
            }
            Event::Lock => Reason::Locked,
            Event::Wake => {
                flags.sleeping = false;
                Reason::Initializing
            }
            Event::Active => {
                flags.inactive = false;
                Reason::Initializing
            }
            Event::Unprotected => {
                flags.protected = false;
                Reason::Initializing
            }
            Event::Unlock => Reason::Initializing,
        };
        drop(flags);
        gate.block(reason);
        if matches!(
            event,
            Event::Wake | Event::Active | Event::Unprotected | Event::Unlock
        ) {
            let fence = gate.sample_fence();
            gate.apply_sample(fence, sample_now(&blockers));
        }
    });
    let token = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(name),
            None,
            Some(&NSOperationQueue::mainQueue()),
            &callback,
        )
    };
    let token = unsafe { Retained::cast_unchecked::<AnyObject>(token) };
    Observer { center, token }
}
impl Monitor {
    pub(super) fn start(app: tauri::AppHandle, gate: Arc<AvailabilityGate>) -> Self {
        let monitor = Self {
            app: app.clone(),
            id: uuid::Uuid::new_v4().to_string(),
            active: Arc::new(AtomicBool::new(true)),
            installed: Arc::new(AtomicBool::new(false)),
            blockers: Arc::new(Mutex::new(Blockers::default())),
        };
        let (id, active, installed, blockers) = (
            monitor.id.clone(),
            monitor.active.clone(),
            monitor.installed.clone(),
            monitor.blockers.clone(),
        );
        let failure = gate.clone();
        if app
            .run_on_main_thread(move || {
                if !active.load(Ordering::SeqCst) {
                    return;
                }
                let Some(class) = AnyClass::get(c"NSWorkspace") else {
                    gate.block(Reason::Unsupported);
                    return;
                };
                let workspace: Retained<AnyObject> = unsafe { msg_send![class, sharedWorkspace] };
                let workspace_center: Retained<NSNotificationCenter> =
                    unsafe { msg_send![&*workspace, notificationCenter] };
                let default_center = NSNotificationCenter::defaultCenter();
                let distributed = NSDistributedNotificationCenter::defaultCenter();
                let distributed =
                    unsafe { Retained::cast_unchecked::<NSNotificationCenter>(distributed) };
                let mut tokens = Vec::with_capacity(8);
                for (name, event) in unsafe {
                    [
                        (NSWorkspaceWillSleepNotification, Event::Sleep),
                        (NSWorkspaceDidWakeNotification, Event::Wake),
                        (
                            NSWorkspaceSessionDidResignActiveNotification,
                            Event::Inactive,
                        ),
                        (NSWorkspaceSessionDidBecomeActiveNotification, Event::Active),
                    ]
                } {
                    tokens.push(observer(
                        workspace_center.clone(),
                        name,
                        event,
                        gate.clone(),
                        active.clone(),
                        blockers.clone(),
                    ));
                }
                for (name, event) in unsafe {
                    [
                        (
                            NSApplicationProtectedDataWillBecomeUnavailableNotification,
                            Event::Protected,
                        ),
                        (
                            NSApplicationProtectedDataDidBecomeAvailableNotification,
                            Event::Unprotected,
                        ),
                    ]
                } {
                    tokens.push(observer(
                        default_center.clone(),
                        name,
                        event,
                        gate.clone(),
                        active.clone(),
                        blockers.clone(),
                    ));
                }
                for (name, event) in [
                    ("com.apple.screenIsLocked", Event::Lock),
                    ("com.apple.screenIsUnlocked", Event::Unlock),
                ] {
                    tokens.push(observer(
                        distributed.clone(),
                        &NSString::from_str(name),
                        event,
                        gate.clone(),
                        active.clone(),
                        blockers.clone(),
                    ));
                }
                // Observers and generation fence precede the first actual sample.
                OBSERVERS.with(|all| all.borrow_mut().insert(id, tokens));
                installed.store(true, Ordering::SeqCst);
                let fence = gate.sample_fence();
                gate.apply_sample(fence, sample_now(&blockers));
            })
            .is_err()
        {
            failure.block(Reason::MonitorDisconnected);
        }
        monitor
    }
    pub(super) async fn sample(&self) -> Observation {
        if !self.active.load(Ordering::SeqCst) || !self.installed.load(Ordering::SeqCst) {
            return Observation::unknown(Reason::Initializing);
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        let active = self.active.clone();
        let blockers = self.blockers.clone();
        if self
            .app
            .run_on_main_thread(move || {
                let value = if active.load(Ordering::SeqCst) {
                    sample_now(&blockers)
                } else {
                    Observation::unknown(Reason::MonitorDisconnected)
                };
                let _ = tx.send(value);
            })
            .is_err()
        {
            return Observation::unknown(Reason::MonitorDisconnected);
        }
        tokio::time::timeout(Duration::from_secs(2), rx)
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_else(|| Observation::unknown(Reason::ObservationUnavailable))
    }
    pub(super) fn stop(&self) {
        self.active.store(false, Ordering::SeqCst);
        self.installed.store(false, Ordering::SeqCst);
        let id = self.id.clone();
        let _ = self.app.run_on_main_thread(move || {
            OBSERVERS.with(|all| all.borrow_mut().remove(&id));
        });
    }
}
