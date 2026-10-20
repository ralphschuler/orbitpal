//! Native current-session availability. Adapters retain only closed categories;
//! raw session dictionaries, identities and native payloads are never emitted.
#[path = "life_availability/core.rs"]
mod core;
pub use core::*;
use std::sync::Arc;
#[cfg(any(target_os = "linux", test))]
#[path = "life_availability/linux_state.rs"]
mod linux_state;
#[cfg(any(target_os = "macos", test))]
#[path = "life_availability/macos_state.rs"]
mod macos_state;
#[cfg(target_os = "macos")]
#[path = "life_availability/macos.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "life_availability/windows.rs"]
mod platform;
#[cfg(target_os = "linux")]
#[path = "life_availability/linux.rs"]
mod platform;
#[path = "life_availability/sampling.rs"]
mod sampling;
#[cfg(any(target_os = "linux", test))]
#[path = "life_availability/task.rs"]
mod task;
#[cfg(any(target_os = "windows", test))]
#[path = "life_availability/windows_state.rs"]
mod windows_state;

pub struct NativeMonitor {
    pub gate: Arc<AvailabilityGate>,
    platform: platform::Monitor,
    sampler: sampling::SerializedSampler,
}
impl NativeMonitor {
    pub fn start(app: tauri::AppHandle) -> Self {
        #[cfg(target_os = "macos")]
        let source = Source::MacosCgSessionCompatibility;
        #[cfg(target_os = "windows")]
        let source = Source::WindowsWts;
        #[cfg(target_os = "linux")]
        let source = Source::LinuxLogind;
        let gate = Arc::new(AvailabilityGate::new(source));
        let platform = platform::Monitor::start(app, gate.clone());
        Self {
            gate,
            platform,
            sampler: sampling::SerializedSampler::default(),
        }
    }
    pub async fn refresh(&self) -> Snapshot {
        self.sampler
            .sample(&self.gate, self.platform.sample())
            .await
    }
    pub fn stop(&self) {
        self.gate.stop();
        self.platform.stop();
    }
}
