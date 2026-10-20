//! Current-process WTS and suspend/resume monitor. No session enumeration,
//! screenshots, input hooks, desktop switching or privilege changes.
//!
//! https://learn.microsoft.com/en-us/windows/win32/api/wtsapi32/nf-wtsapi32-wtsregistersessionnotification
//! https://learn.microsoft.com/en-us/windows/win32/api/wtsapi32/ns-wtsapi32-wtsinfoex_level1_w
//! https://learn.microsoft.com/en-us/windows/win32/power/wm-powerbroadcast
use super::{windows_state, Availability, AvailabilityGate, Observation, Reason};
use std::{
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    System::{
        LibraryLoader::GetModuleHandleW,
        Power::{RegisterSuspendResumeNotification, UnregisterSuspendResumeNotification},
        RemoteDesktop::{
            ProcessIdToSessionId, WTSFreeMemory, WTSQuerySessionInformationW,
            WTSRegisterSessionNotification, WTSSessionInfoEx, WTSUnRegisterSessionNotification,
            NOTIFY_FOR_THIS_SESSION, WTSINFOEXW, WTS_CURRENT_SERVER_HANDLE,
        },
    },
    UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetWindowLongPtrW,
        MsgWaitForMultipleObjectsEx, PeekMessageW, RegisterClassW, SetWindowLongPtrW,
        TranslateMessage, UnregisterClassW, CREATESTRUCTW, DEVICE_NOTIFY_WINDOW_HANDLE,
        GWLP_USERDATA, MSG, MWMO_INPUTAVAILABLE, PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND,
        PBT_APMSUSPEND, PM_REMOVE, QS_ALLINPUT, WM_NCCREATE, WM_NCDESTROY, WM_POWERBROADCAST,
        WM_QUIT, WM_WTSSESSION_CHANGE, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
    },
};

struct Control {
    gate: Arc<AvailabilityGate>,
    stopped: AtomicBool,
    ready: AtomicBool,
    sleeping: AtomicBool,
    sampling: AtomicBool,
    session: AtomicU32,
}

pub(super) struct Monitor {
    control: Arc<Control>,
}

fn unknown() -> Observation {
    Observation {
        state: Availability::Unknown,
        reason: Reason::ObservationUnavailable,
    }
}

impl Monitor {
    pub(super) fn start(_app: tauri::AppHandle, gate: Arc<AvailabilityGate>) -> Self {
        let control = Arc::new(Control {
            gate,
            stopped: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            sleeping: AtomicBool::new(false),
            sampling: AtomicBool::new(false),
            session: AtomicU32::new(u32::MAX),
        });
        let worker = Arc::clone(&control);
        if std::thread::Builder::new()
            .name("orbitpal-life-wts".into())
            .spawn(move || {
                struct Disconnect(Arc<Control>);
                impl Drop for Disconnect {
                    fn drop(&mut self) {
                        self.0.ready.store(false, Ordering::SeqCst);
                        self.0.gate.block(Reason::MonitorDisconnected);
                    }
                }
                let _disconnect = Disconnect(Arc::clone(&worker));
                // All HWND ownership/registration/teardown stays on this thread.
                unsafe {
                    observe(&worker);
                }
            })
            .is_err()
        {
            control.stopped.store(true, Ordering::SeqCst);
            control.gate.block(Reason::MonitorDisconnected);
        }
        Self { control }
    }

    pub(super) async fn sample(&self) -> Observation {
        let state = &self.control;
        if state.stopped.load(Ordering::SeqCst) || !state.ready.load(Ordering::SeqCst) {
            return unknown();
        }
        if state.sleeping.load(Ordering::SeqCst) {
            return Observation {
                state: Availability::SuspendedLocked,
                reason: Reason::Sleeping,
            };
        }
        // A timed-out native call cannot accumulate unbounded blocking workers.
        if state
            .sampling
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return unknown();
        }
        let control = Arc::clone(state);
        let query = tokio::task::spawn_blocking(move || {
            struct Reset(Arc<Control>);
            impl Drop for Reset {
                fn drop(&mut self) {
                    self.0.sampling.store(false, Ordering::SeqCst);
                }
            }
            let _reset = Reset(Arc::clone(&control));
            if control.stopped.load(Ordering::SeqCst) || !control.ready.load(Ordering::SeqCst) {
                return unknown();
            }
            let result = unsafe { current_session(control.session.load(Ordering::SeqCst)) };
            if control.stopped.load(Ordering::SeqCst) || !control.ready.load(Ordering::SeqCst) {
                return unknown();
            }
            if control.sleeping.load(Ordering::SeqCst) {
                return Observation {
                    state: Availability::SuspendedLocked,
                    reason: Reason::Sleeping,
                };
            }
            result
        });
        match tokio::time::timeout(Duration::from_secs(2), query).await {
            Ok(Ok(result)) => result,
            _ => unknown(),
        }
    }

    pub(super) fn stop(&self) {
        self.control.stopped.store(true, Ordering::SeqCst);
        self.control.ready.store(false, Ordering::SeqCst);
        self.control.gate.block(Reason::MonitorDisconnected);
        // The observer wakes at most250ms later, unregisters, then destroys its
        // own HWND. Never post WM_CLOSE to a possibly recycled external HWND.
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        self.stop();
    }
}

unsafe fn current_session(expected: u32) -> Observation {
    if expected == u32::MAX {
        return unknown();
    }
    let mut pointer = null_mut();
    let mut bytes = 0;
    let success = WTSQuerySessionInformationW(
        WTS_CURRENT_SERVER_HANDLE,
        expected,
        WTSSessionInfoEx,
        &mut pointer,
        &mut bytes,
    );
    // WTSFreeMemory is required for the returned buffer, including malformed
    // replies. Never log or project any username/domain/connection payload.
    struct Buffer(*mut u16);
    impl Drop for Buffer {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    WTSFreeMemory(self.0.cast());
                }
            }
        }
    }
    let buffer = Buffer(pointer);
    if success == 0
        || buffer.0.is_null()
        || (bytes as usize) < size_of::<WTSINFOEXW>()
        || bytes > 16_384
    {
        return unknown();
    }
    interpret_buffer(
        std::slice::from_raw_parts(buffer.0.cast::<u8>(), bytes as usize),
        expected,
    )
}

fn interpret_buffer(bytes: &[u8], expected: u32) -> Observation {
    if bytes.len() < size_of::<WTSINFOEXW>() || bytes.len() > 16_384 {
        return unknown();
    }
    // The native ABI is supplied by windows-sys, including the union's8-byte
    // alignment; do not guess offsets from the first DWORD. Unaligned reads
    // also let pure fixtures exercise truncated and offset input safely.
    let info = unsafe { std::ptr::read_unaligned(bytes.as_ptr().cast::<WTSINFOEXW>()) };
    if info.Level != 1 {
        return unknown();
    }
    let session = unsafe { info.Data.WTSInfoExLevel1 };
    windows_state::interpret(
        expected,
        session.SessionId,
        info.Level,
        session.SessionState,
        session.SessionFlags,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::offset_of;
    use windows_sys::Win32::System::RemoteDesktop::WTSINFOEX_LEVEL1_W;

    fn fixture(level: u32, session: u32, connection: i32, flags: i32) -> Vec<u8> {
        let mut bytes = vec![0; size_of::<WTSINFOEXW>()];
        let data = offset_of!(WTSINFOEXW, Data);
        for (offset, value) in [
            (offset_of!(WTSINFOEXW, Level), level),
            (data + offset_of!(WTSINFOEX_LEVEL1_W, SessionId), session),
            (
                data + offset_of!(WTSINFOEX_LEVEL1_W, SessionState),
                connection as u32,
            ),
            (
                data + offset_of!(WTSINFOEX_LEVEL1_W, SessionFlags),
                flags as u32,
            ),
        ] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn real_windows_abi_synthetic_buffer_proves_only_the_current_active_unlocked_session() {
        assert_eq!(
            interpret_buffer(&fixture(1, 42, 0, 1), 42).reason,
            Reason::Ready
        );
        assert_eq!(
            interpret_buffer(&fixture(1, 42, 0, 0), 42).reason,
            Reason::Locked
        );
        assert_eq!(
            interpret_buffer(&fixture(1, 42, 4, 1), 42).reason,
            Reason::InactiveSession
        );
        for bytes in [
            fixture(2, 42, 0, 1),
            fixture(1, 43, 0, 1),
            fixture(1, 42, 0, -1),
        ] {
            assert_eq!(interpret_buffer(&bytes, 42).state, Availability::Unknown);
        }
    }

    #[test]
    fn truncated_oversized_and_unaligned_payloads_are_bounded_without_native_calls() {
        let sample = fixture(1, 42, 0, 1);
        for length in [0, 1, sample.len() - 1] {
            assert_eq!(
                interpret_buffer(&sample[..length], 42).state,
                Availability::Unknown
            );
        }
        assert_eq!(
            interpret_buffer(&vec![0; 16_385], 42).state,
            Availability::Unknown
        );
        let mut offset = vec![0];
        offset.extend_from_slice(&sample);
        assert_eq!(
            interpret_buffer(&offset[1..], 42).state,
            Availability::AwakeUnlocked
        );
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        if lparam == 0 {
            return 0;
        }
        let create = &*(lparam as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Control;
    if !pointer.is_null() {
        let state = &*pointer;
        if !state.stopped.load(Ordering::SeqCst) {
            if message == WM_POWERBROADCAST {
                match wparam as u32 {
                    PBT_APMSUSPEND => {
                        state.sleeping.store(true, Ordering::SeqCst);
                        state.gate.block(Reason::Sleeping);
                    }
                    PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => {
                        state.sleeping.store(false, Ordering::SeqCst);
                        state.gate.block(Reason::Initializing);
                    }
                    _ => {}
                }
                return 1;
            }
            if message == WM_WTSSESSION_CHANGE
                && lparam >= 0
                && lparam as u32 == state.session.load(Ordering::SeqCst)
            {
                match wparam as u32 {
                    7 => {
                        state.gate.block(Reason::Locked);
                    }
                    2 | 4 | 6 => {
                        state.gate.block(Reason::InactiveSession);
                    }
                    // Reconnect, login, unlock and desktop-ready never grant;
                    // common native scheduling must take a new fenced sample.
                    1 | 3 | 5 | 8 | 9 | 15 => {
                        state.gate.block(Reason::Initializing);
                    }
                    _ => {}
                }
                return 0;
            }
        }
    }
    if message == WM_NCDESTROY {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
    }
    DefWindowProcW(hwnd, message, wparam, lparam)
}

unsafe fn observe(control: &Arc<Control>) {
    let mut session = u32::MAX;
    if ProcessIdToSessionId(std::process::id(), &mut session) == 0 || session == u32::MAX {
        return;
    }
    control.session.store(session, Ordering::SeqCst);
    let instance = GetModuleHandleW(null());
    if instance.is_null() {
        return;
    }
    let class: Vec<u16> = format!("OrbitPal.LifeAvailability.{}\0", uuid::Uuid::new_v4())
        .encode_utf16()
        .collect();
    let descriptor = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: class.as_ptr(),
        ..zeroed()
    };
    if RegisterClassW(&descriptor) == 0 {
        return;
    }
    // A message-only HWND does not receive broadcasts. This top-level window
    // is never shown, activated, placed in the taskbar or used to render.
    let hwnd = CreateWindowExW(
        WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        class.as_ptr(),
        class.as_ptr(),
        WS_POPUP,
        0,
        0,
        0,
        0,
        null_mut(),
        null_mut(),
        instance,
        Arc::as_ptr(control).cast(),
    );
    if hwnd.is_null() {
        UnregisterClassW(class.as_ptr(), instance);
        return;
    }
    let registered = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) != 0;
    let power = RegisterSuspendResumeNotification(hwnd, DEVICE_NOTIFY_WINDOW_HANDLE);
    if registered && power != 0 && !control.stopped.load(Ordering::SeqCst) {
        control.ready.store(true, Ordering::SeqCst);
        let mut message: MSG = zeroed();
        'monitor: while !control.stopped.load(Ordering::SeqCst) {
            // Bound shutdown without posting to an HWND after its lifetime.
            if MsgWaitForMultipleObjectsEx(0, null(), 250, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
                == u32::MAX
            {
                break;
            }
            // Bound a message flood so stop is still observed.
            for _ in 0..256 {
                if PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) == 0 {
                    break;
                }
                if message.message == WM_QUIT {
                    break 'monitor;
                }
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    control.ready.store(false, Ordering::SeqCst);
    control.gate.block(Reason::MonitorDisconnected);
    if power != 0 {
        UnregisterSuspendResumeNotification(power);
    }
    if registered {
        WTSUnRegisterSessionNotification(hwnd);
    }
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
    DestroyWindow(hwnd);
    UnregisterClassW(class.as_ptr(), instance);
}
