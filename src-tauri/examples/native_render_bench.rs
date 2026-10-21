//! Opt-in, nonshipping Linux renderer measurement. Never calls orbitpal_lib::run.
//! All normal application backends and plugin/IPC tables are absent.
#[path = "../benchmark/protocol.rs"]
mod protocol;
use protocol::{Command, Packet, PhaseGate, Protocol, Reply};
use serde_json::json;
use std::{fs::{self, File, OpenOptions}, io::Write, path::PathBuf,
    sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex}, time::{Duration, Instant}};
use tauri::{Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

const ID: &str = "studio.orbitpal.native-render-benchmark";
const MAX_LOG: usize = 32 * 1024 * 1024;
type Error = &'static str;

fn fixture_navigation(url: &url::Url, query: &str) -> bool {
    ((url.scheme()=="tauri" && url.host_str()==Some("localhost"))
        || (url.scheme()=="http" && url.host_str()==Some("tauri.localhost")))
        && url.path()=="/benchmark.html" && url.query()==Some(query)
        && url.fragment().is_none() && url.username().is_empty() && url.password().is_none() && url.port().is_none()
}

struct Log { file: File, bytes: usize }
impl Log {
    fn write(&mut self, value: serde_json::Value) -> Result<(), Error> {
        let mut bytes = serde_json::to_vec(&value).map_err(|_| "numeric_log_encode_failed")?;
        bytes.push(b'\n');
        self.bytes = self.bytes.checked_add(bytes.len()).filter(|n| *n <= MAX_LOG).ok_or("numeric_log_limit")?;
        self.file.write_all(&bytes).map_err(|_| "numeric_log_write_failed")
    }
}
struct Mount {
    protocol: Protocol,
    label: String,
    window: Option<WebviewWindow>,
    destroyed: Arc<AtomicBool>,
    last_batch: Option<Instant>,
    first_draw: bool,
    phase: &'static str,
    phase_gate: Option<PhaseGate>,
    stable_phase: bool,
}
struct Shared {
    root: PathBuf,
    start: Instant,
    smoke: bool,
    actors: u64,
    mounts: Mutex<Vec<Mount>>,
    log: Mutex<Log>,
    failed: AtomicBool,
    finished: AtomicBool,
}
impl Shared {
    fn log(&self, event: serde_json::Value) -> Result<(), Error> {
        self.log.lock().map_err(|_| "numeric_log_unavailable")?.write(json!({"nativeMs":self.start.elapsed().as_millis() as u64,"event":event}))
    }
    fn ensure_live(&self) -> Result<(), Error> {
        if self.failed.load(Ordering::SeqCst) || self.finished.load(Ordering::SeqCst) || self.start.elapsed() > Duration::from_secs(55 * 60) {
            Err("benchmark_cancelled_or_expired")
        } else { Ok(()) }
    }
}

/// No environment values, paths, exception strings or external content enter logs.
#[cfg(target_os = "linux")]
fn fixture_root() -> Result<(PathBuf, bool, u64), Error> {
    use std::os::unix::fs::MetadataExt;
    for (key, value) in [("GITHUB_ACTIONS","true"), ("RUNNER_ENVIRONMENT","github-hosted"), ("RUNNER_OS","Linux"),
        ("ORBITPAL_NATIVE_RENDER_BENCH","disposable-linux-only"), ("GDK_BACKEND","wayland"), ("WAYLAND_DISPLAY","orbitpal-benchmark")] {
        if std::env::var(key).ok().as_deref() != Some(value) { return Err("not_disposable_benchmark_session"); }
    }
    if std::env::var_os("TAURI_CONFIG").is_some() || std::env::var_os("DISPLAY").is_some() || unsafe { libc::geteuid() } == 0 {
        return Err("unsafe_benchmark_environment");
    }
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 || args[1] != "--mode" { return Err("invalid_benchmark_arguments"); }
    let (smoke, actors) = measurement_mode(&args[2]).ok_or("invalid_benchmark_arguments")?;
    let root = PathBuf::from(std::env::var_os("ORBITPAL_BENCH_ROOT").ok_or("missing_fixture_root")?);
    let name = root.file_name().and_then(|s| s.to_str()).ok_or("invalid_fixture_root")?;
    let nonce = name.strip_prefix("orbitpal-render-").ok_or("invalid_fixture_root")?;
    if root.parent() != Some(std::path::Path::new("/tmp")) || nonce.len() != 16 || !nonce.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid_fixture_root");
    }
    let meta = fs::symlink_metadata(&root).map_err(|_| "missing_fixture_root")?;
    if !meta.is_dir() || meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o077 != 0 || fs::canonicalize(&root).map_err(|_| "invalid_fixture_root")? != root {
        return Err("foreign_fixture_root");
    }
    for (key, suffix) in [("HOME","home"),("XDG_DATA_HOME","data"),("XDG_CONFIG_HOME","config"),("XDG_CACHE_HOME","cache"),("XDG_RUNTIME_DIR","runtime")] {
        if std::env::var_os(key).map(PathBuf::from) != Some(root.join(suffix)) { return Err("foreign_fixture_directory"); }
    }
    if fs::read(root.join("fixture.marker")).map_err(|_| "missing_fixture_marker")? != nonce.as_bytes() { return Err("invalid_fixture_marker"); }
    let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").map_err(|_| "missing_private_bus")?;
    let expected = format!("unix:path={}/runtime/session-bus", root.display());
    if address != expected && !address.strip_prefix(&(expected + ",guid=")).is_some_and(|s|s.len()==32&&s.bytes().all(|b|b.is_ascii_hexdigit())) {
        return Err("foreign_session_bus");
    }
    let host_net = std::env::var_os("ORBITPAL_BENCH_HOST_NETNS").ok_or("missing_network_scope")?;
    if fs::read_link("/proc/self/ns/net").map_err(|_| "network_scope_unavailable")?.as_os_str() == host_net {
        return Err("network_scope_not_isolated");
    }
    let expected_group = format!("0::/{}\n",name);
    if fs::read_to_string("/proc/self/cgroup").map_err(|_| "process_scope_unavailable")? != expected_group {
        return Err("foreign_process_scope");
    }
    Ok((root, smoke, actors))
}
#[cfg(not(target_os = "linux"))]
fn fixture_root() -> Result<(PathBuf, bool, u64), Error> { Err("linux_disposable_session_required") }

fn measurement_mode(mode: &str) -> Option<(bool, u64)> {
    match mode { "smoke" => Some((true, 1)), "pilot" => Some((false, 1)),
        "pilot3" => Some((false, 3)), "pilot5" => Some((false, 5)), _ => None }
}
fn figure_epoch(cycle: u64, ordinal: u64, actors: u64) -> Result<u64, Error> {
    if !matches!(actors, 1 | 3 | 5) || ordinal >= actors || cycle > 20 { return Err("invalid_figure_bank"); }
    Ok(cycle * 5 + ordinal + 1)
}

#[tauri::command]
async fn bench_batch(window: WebviewWindow, state: tauri::State<'_, Arc<Shared>>, packet: Packet) -> Result<Reply, Error> {
    state.ensure_live()?;
    let mut guard = state.mounts.lock().map_err(|_| "mount_unavailable")?;
    let mount = guard.iter_mut().find(|mount| window.label() == mount.label).ok_or("foreign_benchmark_window")?;
    let now = Instant::now();
    let reply = mount.protocol.accept(packet).inspect_err(|_| { state.failed.store(true, Ordering::SeqCst); })?;
    mount.last_batch = Some(now);
    let packet = mount.protocol.previous.as_ref().ok_or("missing_measurement")?;
    if let Some(gate) = mount.phase_gate.as_mut() {
        gate.observe(packet, &reply).inspect_err(|_| { state.failed.store(true, Ordering::SeqCst); })?;
    }
    mount.first_draw |= packet.counters.submitted > 0;
    state.log(json!({"kind":"batch","phase":mount.phase,"stablePhase":mount.stable_phase,"packet":packet}))?;
    Ok(reply)
}

async fn on_main<T: Send + 'static>(app: &tauri::AppHandle, state: &Arc<Shared>, operation: impl FnOnce(&tauri::AppHandle) -> Result<T, Error> + Send + 'static) -> Result<T, Error> {
    let (send, receive) = tokio::sync::oneshot::channel();
    let handle = app.clone(); let current = state.clone();
    let cancelled = Arc::new(AtomicBool::new(false)); let queued_cancel = cancelled.clone();
    app.run_on_main_thread(move || {
        let result = if queued_cancel.load(Ordering::SeqCst) { Err("main_operation_cancelled") }
            else { current.ensure_live().and_then(|_| operation(&handle)) };
        let _ = send.send(result);
    }).map_err(|_| "main_queue_failed")?;
    match tokio::time::timeout(Duration::from_secs(20), receive).await {
        Ok(Ok(result)) => result,
        _ => { cancelled.store(true, Ordering::SeqCst); Err("main_operation_unconfirmed") }
    }
}
async fn sleep_checked(state: &Arc<Shared>, duration: Duration) -> Result<(), Error> {
    let until = Instant::now() + duration;
    while Instant::now() < until {
        state.ensure_live()?;
        { let guard = state.mounts.lock().map_err(|_| "mount_unavailable")?;
          for mount in guard.iter() {
              if mount.destroyed.load(Ordering::SeqCst) { return Err("unexpected_figure_destruction"); }
              if mount.last_batch.is_some_and(|at|at.elapsed()>Duration::from_secs(8)) { return Err("measurement_batch_stalled"); }
          }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Ok(())
}
async fn create_mount(app: &tauri::AppHandle, state: &Arc<Shared>, epoch: u64, ordinal: u64) -> Result<(), Error> {
    let label = format!("bench-figure-{epoch}"); let destroyed = Arc::new(AtomicBool::new(false));
    { let mut guard = state.mounts.lock().map_err(|_| "mount_unavailable")?;
      if ordinal >= state.actors || guard.len() as u64 != ordinal || guard.iter().any(|m| m.protocol.epoch == epoch) { return Err("overlapping_figure"); }
      guard.push(Mount { protocol: Protocol::new(epoch)?, label:label.clone(),window:None,destroyed:destroyed.clone(),last_batch:None,first_draw:false,phase:"initial_draw",phase_gate:None,stable_phase:false });
    }
    let root = state.root.clone();
    let fps = if state.actors == 5 { 20 } else { 30 };
    let window = on_main(app, state, move |app| {
        let query=format!("benchmark=figure&epoch={epoch}&fps={fps}");
        let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::App(format!("benchmark.html?{query}").into()))
            .title("OrbitPal renderer fixture").inner_size(480.0,480.0).position((ordinal % 3) as f64 * 480.0, (ordinal / 3) as f64 * 480.0).resizable(false)
            .data_directory(root.join(format!("webview-{epoch}")))
            .on_navigation(move |url| fixture_navigation(url,&query))
            .build().map_err(|_| "figure_window_failed")?;
        window.on_window_event(move |event| { if matches!(event,tauri::WindowEvent::Destroyed) { destroyed.store(true,Ordering::SeqCst); } });
        Ok(window)
    }).await?;
    state.mounts.lock().map_err(|_| "mount_unavailable")?.iter_mut().find(|m| m.protocol.epoch == epoch).ok_or("mount_missing")?.window = Some(window);
    let until = Instant::now()+Duration::from_secs(30);
    loop {
        sleep_checked(state,Duration::from_millis(100)).await?;
        if state.mounts.lock().map_err(|_| "mount_unavailable")?.iter().any(|m|m.protocol.epoch == epoch && m.first_draw) { break; }
        if Instant::now()>until { return Err("first_actual_submission_missing"); }
    }
    state.log(json!({"kind":"first_actual_submission","epoch":epoch}))
}
async fn phase(app:&tauri::AppHandle,state:&Arc<Shared>,name:&'static str,command:Command,control_visible:bool,duration:Duration)->Result<(),Error> {
    let transition_start=Instant::now();
    { let mut guard=state.mounts.lock().map_err(|_|"mount_unavailable")?;
      if guard.len() as u64 != state.actors { return Err("mount_missing"); }
      for mount in guard.iter_mut() { mount.phase=name;mount.protocol.command=command;mount.phase_gate=Some(PhaseGate::new(mount.protocol.epoch,command)?);mount.stable_phase=false; } }
    state.log(json!({"kind":"phase_transition_start","phase":name,"requestedDomOut":command==Command::DomOut,"requestedControlVisible":control_visible}))?;
    on_main(app,state,move|app| {let control=app.get_webview_window("bench-control").ok_or("control_missing")?;
        if control_visible {control.show()} else {control.hide()}.map_err(|_|"control_visibility_failed")?;
        if control.is_visible().map_err(|_|"control_visibility_unavailable")?!=control_visible {return Err("control_visibility_unconfirmed");}Ok(())}).await?;
    let ack_until=Instant::now()+Duration::from_secs(30);
    loop {
        sleep_checked(state,Duration::from_millis(100)).await?;
        let mut guard=state.mounts.lock().map_err(|_|"mount_unavailable")?;
        if guard.len() as u64 != state.actors { return Err("mount_missing"); }
        if guard.iter().all(|mount| mount.phase_gate.as_ref().is_some_and(PhaseGate::acknowledged)) {
            for mount in guard.iter_mut() { mount.stable_phase=true; } break;
        }
        if Instant::now()>ack_until {return Err("phase_acknowledgement_missing");}
    }
    state.log(json!({"kind":"phase_start","phase":name,"durationMs":duration.as_millis() as u64,"transitionMs":transition_start.elapsed().as_millis() as u64,"laterDomAck":true,"freshSubmissionAfterCommandRequired":command==Command::DomIn}))?;
    sleep_checked(state,duration).await
}
async fn close_mount(app:&tauri::AppHandle,state:&Arc<Shared>)->Result<(),Error> {
    {let mut guard=state.mounts.lock().map_err(|_|"mount_unavailable")?;
     if guard.len() as u64 != state.actors { return Err("mount_missing"); }
     for mount in guard.iter_mut() {mount.phase="orderly_dispose";mount.protocol.command=Command::Dispose;mount.phase_gate=None;mount.stable_phase=false;} }
    let until=Instant::now()+Duration::from_secs(15);
    let windows=loop {
        sleep_checked(state,Duration::from_millis(100)).await?;
        let guard=state.mounts.lock().map_err(|_|"mount_unavailable")?;
        if guard.len() as u64 != state.actors { return Err("mount_missing"); }
        if guard.iter().all(|mount|mount.protocol.previous.as_ref().is_some_and(|p|p.disposed)) {
            break guard.iter().map(|mount|Ok((mount.window.clone().ok_or("figure_handle_missing")?,mount.destroyed.clone(),mount.protocol.epoch))).collect::<Result<Vec<_>,Error>>()?;
        }
        if Instant::now()>until {return Err("actual_dispose_ack_missing");}
    };
    let closing=windows.iter().map(|(window,_,_)|window.clone()).collect::<Vec<_>>();
    on_main(app,state,move|_| {for window in closing {window.close().map_err(|_|"figure_close_failed")?;} Ok(())}).await?;
    let until=Instant::now()+Duration::from_secs(15);
    while !windows.iter().all(|(_,destroyed,_)|destroyed.load(Ordering::SeqCst)) {
        state.ensure_live()?; if Instant::now()>until{return Err("native_destroyed_not_observed");}
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    state.mounts.lock().map_err(|_|"mount_unavailable")?.clear();
    for (_,_,epoch) in windows {state.log(json!({"kind":"native_destroyed_after_actual_dispose","epoch":epoch}))?;} Ok(())
}

async fn create_bank(app:&tauri::AppHandle,state:&Arc<Shared>,cycle:u64)->Result<(),Error> {
    for ordinal in 0..state.actors { create_mount(app,state,figure_epoch(cycle,ordinal,state.actors)?,ordinal).await?; } Ok(())
}
async fn drive(app:tauri::AppHandle,state:Arc<Shared>)->Result<(),Error> {
    let ready_until=Instant::now()+Duration::from_secs(30);
    loop {
        state.ensure_live()?;
        if fs::read(state.root.join("collector.ready")).ok().as_deref()==Some(b"numeric-process-scope-ready\n") { break; }
        if Instant::now()>ready_until {return Err("numeric_collector_not_ready");}
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let baseline=Duration::from_secs(if state.smoke{1}else{60});
    let warm=Duration::from_secs(if state.smoke{2}else{600});
    let quarter=Duration::from_secs(if state.smoke{2}else{450});
    let mode=if state.smoke{"smoke"}else{match state.actors {3=>"pilot3",5=>"pilot5",_=>"pilot"}};
    state.log(json!({"kind":"scope","version":1,"actors":state.actors,"simultaneousStages":state.actors,"nativeWebviews":state.actors+1,"framesPerSecond":if state.actors==5{20}else{30},"requestedFigureLogicalWidth":480,"requestedFigureLogicalHeight":480,"nativePlacementConfirmed":false,"mode":mode,"softwareGpuQualificationOnly":true,"gpuDuration":null,"gpuUtilization":null,"displayPresentTiming":null,"instrumentationOverhead":null,"missingMetricsReason":"not_qualified","normalAppBootstrap":false,"fullIssue66Acceptance":false}))?;
    state.log(json!({"kind":"control_only_before"}))?;sleep_checked(&state,baseline).await?;
    create_bank(&app,&state,0).await?;
    phase(&app,&state,"warmup",Command::DomIn,true,warm).await?;
    for (name,command,shown) in [("control_shown_dom_in",Command::DomIn,true),("control_shown_dom_out",Command::DomOut,true),("control_hidden_dom_out",Command::DomOut,false),("control_hidden_dom_in",Command::DomIn,false)] {
        phase(&app,&state,name,command,shown,quarter).await?;
    }
    close_mount(&app,&state).await?;
    let cycles=if state.smoke{3}else{20};
    for cycle in 1..=cycles {create_bank(&app,&state,cycle).await?;close_mount(&app,&state).await?;}
    state.log(json!({"kind":"control_only_after","additionalOrderlyCycles":cycles}))?;
    sleep_checked(&state,baseline).await?;
    state.log(json!({"kind":"completed","activeFixtureStages":0,"activeFigureWindows":0,"additionalOrderlyCycles":cycles,"fullAppOrRawCloseCleanupProven":false}))?;
    fs::write(state.root.join("completed.marker"),b"native-renderer-lane-only\n").map_err(|_|"completion_write_failed")?;
    state.finished.store(true,Ordering::SeqCst);app.exit(0);Ok(())
}
fn main() {
    let (root,smoke,actors)=match fixture_root(){Ok(value)=>value,Err(code)=>{eprintln!("benchmark-refused: {code}");std::process::exit(1)}};
    if fs::write(root.join("app.pid"),std::process::id().to_string()).is_err() { eprintln!("benchmark-refused: fixture_pid_unavailable");std::process::exit(1); }
    let context=tauri::generate_context!("benchmark/tauri.conf.json");
    if context.config().identifier!=ID || !context.config().app.windows.is_empty() || context.config().build.dev_url.is_some() {
        eprintln!("benchmark-refused: unexpected_embedded_context");std::process::exit(1);
    }
    let file=match OpenOptions::new().write(true).create_new(true).open(root.join("native.ndjson")) {Ok(file)=>file,Err(_)=>{eprintln!("benchmark-refused: fixture_log_exists_or_unavailable");std::process::exit(1)}};
    let state=Arc::new(Shared{root,start:Instant::now(),smoke,actors,mounts:Mutex::new(Vec::new()),log:Mutex::new(Log{file,bytes:0}),failed:AtomicBool::new(false),finished:AtomicBool::new(false)});
    let setup_state=state.clone();let exit_state=state.clone();
    let app=tauri::Builder::default().manage(state).invoke_handler(tauri::generate_handler![bench_batch]).setup(move|app| {
        let root=setup_state.root.clone();
        let control=WebviewWindowBuilder::new(app,"bench-control",WebviewUrl::App("benchmark.html?benchmark=control".into()))
            .title("OrbitPal renderer measurement").inner_size(420.0,180.0).resizable(false).data_directory(root.join("control-webview"))
            .on_navigation(|url|fixture_navigation(url,"benchmark=control"))
            .build()?;
        let control_state=setup_state.clone();
        control.on_window_event(move|event| {if matches!(event,tauri::WindowEvent::Destroyed)&&!control_state.finished.load(Ordering::SeqCst){control_state.failed.store(true,Ordering::SeqCst);}});
        let handle=app.handle().clone();let worker_state=setup_state.clone();
        tauri::async_runtime::spawn(async move {if let Err(code)=drive(handle.clone(),worker_state.clone()).await {worker_state.failed.store(true,Ordering::SeqCst);let _=worker_state.log(json!({"kind":"failed","reason":code}));handle.exit(1);}});
        Ok(())
    }).build(context);
    match app {
        Ok(app)=>app.run(move|app,event| {
            if let tauri::RunEvent::ExitRequested{code,api,..}=event {
                if code.is_none()&&!exit_state.finished.load(Ordering::SeqCst) {
                    exit_state.failed.store(true,Ordering::SeqCst);let _=exit_state.log(json!({"kind":"interrupted"}));api.prevent_exit();app.exit(1);
                }
            }
        }),
        Err(_)=>{eprintln!("benchmark-refused: isolated_application_build_failed");std::process::exit(1)}
    }
}

#[cfg(test)]
mod navigation_tests {
    use super::*;
    #[test]
    fn pilot_actor_counts_are_closed_and_every_retired_bank_has_new_epochs() {
        assert_eq!(measurement_mode("smoke"), Some((true,1)));
        assert_eq!(measurement_mode("pilot"), Some((false,1)));
        assert_eq!(measurement_mode("pilot3"), Some((false,3)));
        assert_eq!(measurement_mode("pilot5"), Some((false,5)));
        for mode in ["pilot2", "pilot50", "3", "--actors=5", "pilot5 "] { assert!(measurement_mode(mode).is_none()); }
        for actors in [1,3,5] {
            let mut epochs=std::collections::BTreeSet::new();
            for cycle in 0..=20 {for ordinal in 0..actors {assert!(epochs.insert(figure_epoch(cycle,ordinal,actors).unwrap()));}}
            assert_eq!(epochs.len() as u64,actors*21);
            assert!(figure_epoch(0,actors,actors).is_err());
            assert!(figure_epoch(21,0,actors).is_err());
        }
        assert!(figure_epoch(0,0,2).is_err());
    }
    #[test]
    fn navigation_is_only_the_exact_embedded_role_and_epoch() {
        for prefix in ["tauri://localhost", "http://tauri.localhost"] {
            let exact=format!("{prefix}/benchmark.html?benchmark=figure&epoch=1");
            assert!(fixture_navigation(&url::Url::parse(&exact).unwrap(),"benchmark=figure&epoch=1"));
            for suffix in ["/index.html?benchmark=figure&epoch=1", "/benchmark.html?benchmark=figure&epoch=2",
                "/benchmark.html?benchmark=control", "/benchmark.html?benchmark=figure&epoch=1#other",
                "/benchmark.html?benchmark=figure&epoch=1&path=other"] {
                assert!(!fixture_navigation(&url::Url::parse(&format!("{prefix}{suffix}")).unwrap(),"benchmark=figure&epoch=1"));
            }
        }
        let query="benchmark=figure&epoch=7&fps=20";
        assert!(fixture_navigation(&url::Url::parse(&format!("tauri://localhost/benchmark.html?{query}")).unwrap(),query));
        for wrong in ["benchmark=figure&epoch=7&fps=30", "benchmark=figure&epoch=2&fps=20", "benchmark=figure&epoch=7&fps=20&actors=5"] {
            assert!(!fixture_navigation(&url::Url::parse(&format!("tauri://localhost/benchmark.html?{wrong}")).unwrap(),query));
        }
        for raw in ["tauri://other/benchmark.html?benchmark=control", "http://tauri.localhost:8080/benchmark.html?benchmark=control",
            "http://user@tauri.localhost/benchmark.html?benchmark=control", "https://example.com/benchmark.html?benchmark=control"] {
            assert!(!fixture_navigation(&url::Url::parse(raw).unwrap(),"benchmark=control"));
        }
    }
}
