//! Opt-in real portal prototype. The standalone workflow supplies an isolated
//! synthetic Wayland session; ordinary cargo test never opens a chooser.
use super::*;
use base64::Engine;
use std::{os::unix::fs::{MetadataExt, PermissionsExt}, path::PathBuf};

struct Ui { root: PathBuf, sequence: u32 }
impl Ui {
    fn isolated() -> Self {
        for (key, expected) in [("ORBITPAL_WAYLAND_ACCEPTANCE", "synthetic-desktop-only"), ("GITHUB_ACTIONS", "true"), ("RUNNER_ENVIRONMENT", "github-hosted"), ("RUNNER_OS", "Linux")] {
            assert_eq!(std::env::var(key).as_deref(), Ok(expected), "Real desktop acceptance is restricted to its disposable hosted job: {key}");
        }
        let root = PathBuf::from(std::env::var_os("ORBITPAL_ACCEPTANCE_ROOT").expect("Missing private root"));
        assert!(root.is_absolute() && root.file_name().unwrap().to_string_lossy().starts_with("orbitpal-wayland."));
        let metadata = std::fs::symlink_metadata(&root).unwrap();
        assert!(metadata.is_dir() && !metadata.file_type().is_symlink());
        assert_eq!(metadata.uid(), unsafe { libc::geteuid() });
        assert_eq!(metadata.permissions().mode() & 0o077, 0, "Private root must not be shared");
        let bus = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap();
        assert!(bus.starts_with(&format!("unix:path={}/runtime/session-bus", root.display())), "Unexpected session bus");
        assert!(std::env::var("WAYLAND_DISPLAY").unwrap().starts_with("orbitpal-"));
        assert!(root.join("control/ready").is_file(), "Synthetic desktop controller is not ready");
        Self { root, sequence: 0 }
    }
    fn request(&mut self, action: &str) -> u32 {
        self.sequence += 1;
        let value = serde_json::json!({"sequence":self.sequence,"action":action});
        let temporary = self.root.join("control/command.tmp");
        std::fs::write(&temporary, value.to_string()).unwrap();
        std::fs::rename(temporary, self.root.join("control/command.json")).unwrap();
        self.sequence
    }
    async fn acknowledged(&self, sequence: u32) {
        let deadline = Instant::now() + Duration::from_secs(35);
        loop {
            if let Ok(bytes) = std::fs::read(self.root.join("control/ack.json")) {
                let value: Value = serde_json::from_slice(&bytes).unwrap();
                if value["sequence"] == sequence {
                    assert_eq!(value["ok"], true, "Chooser/pattern action failed: {}", value["error"]);
                    return;
                }
            }
            assert!(Instant::now() < deadline, "Synthetic UI action {sequence} timed out");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    async fn color(&mut self, name: &str) {
        let sequence = self.request(name);
        self.acknowledged(sequence).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}
struct Cleanup<'a>(&'a PortalState);
impl Drop for Cleanup<'_> { fn drop(&mut self) { self.0.close_all(); } }

async fn choose(hub: &PortalState, ui: &mut Ui, action: &str) -> Result<Status, String> {
    let sequence = ui.request(action);
    let selected = tokio::time::timeout(Duration::from_secs(45), hub.select("acceptance-a", serde_json::json!({"model":"synthetic-image-fixture"})))
        .await.expect("Real portal selection exceeded the prototype deadline");
    ui.acknowledged(sequence).await;
    selected
}
fn profile(status: &Status) -> Value {
    serde_json::json!({"id":"acceptance-a","screenContext":{"enabled":true,"displayId":status.display_id},"model":{"model":"synthetic-image-fixture"}})
}
fn color_matches(frame: &crate::vision::ValidatedImage, red: bool) {
    let bytes = base64::engine::general_purpose::STANDARD.decode(&frame.base64).unwrap();
    let image = image::load_from_memory(&bytes).expect("Production capture must deliver a decodable JPEG").to_rgb8();
    assert!(image.width() > 0 && image.height() > 0 && image.width().max(image.height()) <= 1280);
    let pixel = image.get_pixel(image.width()/2, image.height()/2).0;
    let (dominant, other) = if red { (pixel[0], pixel[2]) } else { (pixel[2], pixel[0]) };
    assert!(dominant > 160 && other < 100, "Capture did not match the current synthetic pattern: {pixel:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "Requires the opt-in disposable real GNOME/KDE portal workflow"]
async fn real_portal_prototype() {
    let mut ui = Ui::isolated();
    let hub = PortalState::default(); let _cleanup = Cleanup(&hub);
    ui.color("red").await;
    let first = choose(&hub, &mut ui, "allow").await.expect("Real chooser did not grant a monitor");
    assert_eq!(first.phase, "ready");
    let current = profile(&first);
    let read = || hub.capture_for(&current, Arc::new(AtomicBool::new(false))).unwrap()();
    let red = read().expect("Real PipeWire red frame failed"); color_matches(&red, true);
    ui.color("blue").await;
    let blue = read().expect("Real PipeWire blue frame failed"); color_matches(&blue, false);
    // A boolean assertion never includes the compared pixel/base64 values.
    assert!(red.base64 != blue.base64, "Two requests returned a stale image");
    drop(red); drop(blue); // Never write pixels into a log, artifact or history.
    let mut other = current.clone(); other["id"] = serde_json::json!("acceptance-b");
    assert!(hub.capture_for(&other, Arc::new(AtomicBool::new(false))).is_err());
    let cancel = Arc::new(AtomicBool::new(false));
    let stale = hub.capture_for(&current, cancel.clone()).unwrap();
    let mut changed = current.clone(); changed["model"]["model"] = serde_json::json!("changed-fixture");
    hub.reconcile(&changed).unwrap();
    assert!(cancel.load(Ordering::SeqCst)); assert!(stale().is_err());
    assert_eq!(hub.status("acceptance-a").unwrap().phase, "closed");
    assert!(choose(&hub, &mut ui, "cancel").await.is_err(), "Real Cancel must not grant a source");
    assert_eq!(hub.status("acceptance-a").unwrap().phase, "closed");
    let second = choose(&hub, &mut ui, "allow").await.expect("Explicit real reselection failed");
    assert_ne!(first.display_id, second.display_id);
    let current = profile(&second);
    let stale = hub.capture_for(&current, Arc::new(AtomicBool::new(false))).unwrap();
    hub.close_all(); assert!(stale().is_err());
    assert_eq!(hub.status("acceptance-a").unwrap().phase, "closed");
    std::fs::write(ui.root.join("control/prototype-passed"), "real-portal-prototype-complete\n").unwrap();
    println!("Real portal prototype passed: chooser allow/cancel/reselect, two fresh colored JPEGs, bot/model isolation, close invalidation. This is not full issue #26 acceptance.");
}
