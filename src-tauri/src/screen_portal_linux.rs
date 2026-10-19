//! XDG ScreenCast and a restricted PipeWire remote. Never connect to the
//! default PipeWire socket, restore a grant or call the file Screenshot portal.
use super::{FrameRequest, Session};
use futures_util::StreamExt;
use pipewire as pw;
use pw::{properties::properties, spa};
use std::{cell::RefCell, collections::HashMap, os::fd::OwnedFd, rc::Rc,
    sync::{Arc, atomic::Ordering, mpsc}, time::{Duration, Instant}};
use zbus::{Connection, OwnedObjectPath, OwnedVariant, Proxy, Variant};

const SERVICE: &str = "org.freedesktop.portal.Desktop";
const ROOT: &str = "/org/freedesktop/portal/desktop";
type Dict = HashMap<String, OwnedVariant>;

async fn cancelled(session: &Session) {
    while !session.cancelled.load(Ordering::SeqCst) { tokio::time::sleep(Duration::from_millis(30)).await; }
}

fn token() -> String { format!("orbitpal_{}", uuid::Uuid::new_v4().simple()) }
fn client_path(connection: &Connection) -> Result<String, String> {
    Ok(connection.unique_name().ok_or("Wayland-Portal hat keine D-Bus-Verbindung")?.as_str().trim_start_matches(':').replace('.', "_"))
}

async fn release(lease: &Proxy<'_>, connection: &Connection) {
    let _ = tokio::time::timeout(Duration::from_secs(2), async {
        let _: Result<(), _> = lease.call("Close", &()).await;
    }).await;
    let _ = connection.clone().close().await;
}

/// Register Response before invoking the portal, including fast synchronous
/// responses. handle_token fixes the exact request path; a changed path fails.
async fn request<B: serde::Serialize + zbus::DynamicType>(connection: &Connection, portal: &Proxy<'_>, method: &str, handle_token: &str, body: &B) -> Result<Dict, String> {
    let path = format!("{ROOT}/request/{}/{}", client_path(connection)?, handle_token);
    let request = Proxy::new(connection, SERVICE, path.as_str(), "org.freedesktop.portal.Request").await.map_err(|_| "Wayland-Anfrage konnte nicht angelegt werden")?;
    let mut responses = request.receive_signal("Response").await.map_err(|_| "Wayland-Antwort konnte nicht abonniert werden")?;
    let actual: OwnedObjectPath = portal.call(method, body).await.map_err(|_| "Wayland-Portalaufruf fehlgeschlagen. Bitte den Desktop-Portal-Dienst prüfen.")?;
    if actual.as_str() != path { let _: Result<(), _> = request.call("Close", &()).await; return Err("Wayland-Portal hat eine unerwartete Anfrage geliefert".into()); }
    let response = responses.next().await.ok_or("Wayland-Portalverbindung wurde beendet")?;
    let (code, result): (u32, Dict) = response.body().deserialize().map_err(|_| "Ungültige Antwort des Wayland-Portals")?;
    match code { 0 => Ok(result), 1 => Err("Wayland-Auswahl wurde im Freigabedialog abgebrochen".into()), _ => Err("Wayland-Freigabe wurde vom Desktop abgelehnt".into()) }
}

pub(super) async fn select(session: Arc<Session>) -> Result<(), String> {
    let connection = Connection::session().await.map_err(|_| "D-Bus-Sitzung für das Wayland-Portal ist nicht verfügbar")?;
    let session_token = token();
    let path = format!("{ROOT}/session/{}/{}", client_path(&connection)?, session_token);
    let lease = Proxy::new_owned(connection.clone(), SERVICE, path.clone(), "org.freedesktop.portal.Session").await.map_err(|_| "Wayland-Sitzung konnte nicht vorbereitet werden")?;
    // Subscribe before CreateSession so an early revoke cannot be missed.
    let mut closed = lease.receive_signal("Closed").await.map_err(|_| "Wayland-Sitzungsende konnte nicht abonniert werden")?;
    let setup = setup(&connection, &session_token, &path, session.clone());
    let result = tokio::select! {
        result = setup => result,
        _ = cancelled(&session) => Err("Wayland-Auswahl wurde abgebrochen".into()),
        _ = closed.next() => Err("Wayland-Freigabe wurde vom Desktop beendet".into()),
        _ = connection.closed() => Err("Wayland-Portalverbindung wurde beendet".into()),
        _ = tokio::time::sleep(Duration::from_secs(120)) => Err("Wayland-Auswahl hat zu lange gedauert. Bitte erneut auswählen.".into()),
    };
    if result.is_err() {
        session.close("Wayland-Auswahl wurde beendet");
        release(&lease,&connection).await;
        return result;
    }
    // This task owns only the portal lease and cancellation flags, never pixels.
    tauri::async_runtime::spawn(async move {
        let reason = tokio::select! {
            _ = cancelled(&session) => "Wayland-Freigabe wurde geschlossen",
            _ = closed.next() => "Wayland-Freigabe wurde vom Desktop beendet. Bitte erneut auswählen.",
            _ = connection.closed() => "Wayland-Portalverbindung wurde getrennt. Bitte erneut auswählen.",
        };
        session.close(reason);
        release(&lease,&connection).await;
    });
    Ok(())
}

async fn setup(connection: &Connection, session_token: &str, path: &str, session: Arc<Session>) -> Result<(), String> {
    let portal = Proxy::new(connection, SERVICE, ROOT, "org.freedesktop.portal.ScreenCast").await.map_err(|_| "ScreenCast-Portal ist nicht verfügbar")?;
    let version: u32 = portal.get_property("version").await.map_err(|_| "ScreenCast-Portalversion ist nicht verfügbar")?;
    let available: u32 = portal.get_property("AvailableSourceTypes").await.map_err(|_| "Wayland-Bildschirmquellen sind nicht verfügbar")?;
    if version == 0 || available & 1 == 0 { return Err("Dieser Desktop bietet keine Monitorfreigabe über ScreenCast an".into()); }
    let handle = token();
    let options = HashMap::from([("handle_token", Variant::from(handle.as_str())), ("session_handle_token", Variant::from(session_token))]);
    let created = request(connection, &portal, "CreateSession", &handle, &(options,)).await?;
    if created.get("session_handle").and_then(|value| <&str>::try_from(value).ok()) != Some(path) { return Err("ScreenCast-Portal hat eine unerwartete Sitzung geliefert".into()); }
    let handle = token();
    let mut options = HashMap::from([("handle_token", Variant::from(handle.as_str())), ("types", Variant::from(1u32)), ("multiple", Variant::from(false))]);
    if version >= 4 { options.insert("persist_mode", Variant::from(0u32)); }
    let session_path = zbus::ObjectPath::try_from(path).map_err(|_| "Ungültige Wayland-Sitzung")?;
    request(connection, &portal, "SelectSources", &handle, &(session_path.clone(), options)).await?;
    let handle = token();
    let options = HashMap::from([("handle_token", Variant::from(handle.as_str()))]);
    let mut started = request(connection, &portal, "Start", &handle, &(session_path.clone(), "", options)).await?;
    let mut streams = Vec::<(u32, Dict)>::try_from(started.remove("streams").ok_or("ScreenCast-Portal hat keine Quelle geliefert")?).map_err(|_| "Ungültige ScreenCast-Quellen")?;
    if streams.len() != 1 { return Err("Bitte genau einen Wayland-Bildschirm auswählen".into()); }
    let (node, properties) = streams.remove(0);
    let serial = properties.get("pipewire-serial").map(|value| u64::try_from(value).map_err(|_| "Ungültige PipeWire-Seriennummer")).transpose()?;
    if version >= 6 && serial.is_none_or(|serial| serial == 0) { return Err("ScreenCast v6 hat keine sichere PipeWire-Seriennummer geliefert".into()); }
    if properties.get("source_type").is_some_and(|value| u32::try_from(value).ok() != Some(1)) { return Err("Der Desktop hat keine Monitorquelle freigegeben".into()); }
    let options: HashMap<&str, Variant<'_>> = HashMap::new();
    let remote: zbus::OwnedFd = portal.call("OpenPipeWireRemote", &(session_path, options)).await.map_err(|_| "Freigegebener PipeWire-Zugang konnte nicht geöffnet werden")?;
    let (ready, response) = tokio::sync::oneshot::channel();
    let worker_session = session.clone();
    std::thread::Builder::new().name("orbitpal-screen-portal".into()).spawn(move || {
        let result = pipewire(remote.into(), node, serial, worker_session.clone(), ready);
        if let Err(error) = result { worker_session.close(&error); }
    }).map_err(|_| "PipeWire-Aufnahmethread konnte nicht gestartet werden")?;
    response.await.map_err(|_| "PipeWire-Quelle konnte nicht geöffnet werden")?
}

/// The restricted remote and registry stay alive, but no video stream exists
/// between inference requests. Each request creates and destroys its own stream
/// and receives a newly produced frame, rather than returning an idle cache.
fn pipewire(fd: OwnedFd, original_node: u32, expected_serial: Option<u64>, session: Arc<Session>, ready: tokio::sync::oneshot::Sender<Result<(), String>>) -> Result<(), String> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(|_| "PipeWire-Schleife ist nicht verfügbar")?;
    let context = pw::context::ContextRc::new(&mainloop, None).map_err(|_| "PipeWire-Kontext ist nicht verfügbar")?;
    let core = context.connect_fd_rc(fd, None).map_err(|_| "Freigegebene PipeWire-Verbindung ist nicht verfügbar")?;
    let registry = core.get_registry().map_err(|_| "PipeWire-Quellenliste ist nicht verfügbar")?;
    let target = Rc::new(RefCell::new(None::<(u32, u64)>));
    let found = target.clone();
    let removed = target.clone();
    let removed_session = session.clone();
    let _registry_listener = registry.add_listener_local().global(move |global| {
        if global.type_ != pw::types::ObjectType::Node { return; }
        let serial = global.props.and_then(|properties| properties.get("object.serial")).and_then(|value| value.parse::<u64>().ok());
        if serial.is_some_and(|serial| serial > 0 && expected_serial.map(|expected| serial == expected).unwrap_or(global.id == original_node)) && found.borrow().is_none() {
            *found.borrow_mut() = Some((global.id, serial.unwrap()));
        }
    }).global_remove(move |node| {
        if removed.borrow().is_some_and(|(selected, _)| selected == node) { removed_session.close("Wayland-Quelle wurde entfernt oder geändert. Bitte erneut auswählen."); }
    }).register();
    let error_session = session.clone();
    let _core_listener = core.add_listener_local().error(move |_, _, _, _| error_session.close("PipeWire-Verbindung wurde beendet. Bitte erneut auswählen.")).register();
    let deadline = Instant::now() + Duration::from_secs(5);
    while target.borrow().is_none() && !session.cancelled.load(Ordering::SeqCst) && Instant::now() < deadline {
        mainloop.loop_().iterate(pw::loop_::Timeout::Finite(Duration::from_millis(30)));
    }
    let (_, serial) = (*target.borrow()).ok_or("Freigegebene PipeWire-Quelle ist nicht mehr verfügbar. Bitte erneut auswählen.")?;
    if session.cancelled.load(Ordering::SeqCst) { return Err("Wayland-Auswahl wurde abgebrochen".into()); }
    let (sender, requests) = mpsc::sync_channel(1);
    session.ready(sender)?;
    let _ = ready.send(Ok(()));
    while !session.cancelled.load(Ordering::SeqCst) {
        match requests.try_recv() {
            Ok(request) => {
                let image = frame(&core, &mainloop, serial, &session, &request.cancel);
                if image.is_err() && !request.cancel.load(Ordering::SeqCst) { session.close("Wayland-Aufnahme fehlgeschlagen. Bitte die Quelle erneut auswählen."); }
                let _ = request.reply.send(image);
            }
            Err(mpsc::TryRecvError::Disconnected) => break,
            Err(mpsc::TryRecvError::Empty) => { mainloop.loop_().iterate(pw::loop_::Timeout::Finite(Duration::from_millis(30))); }
        }
    }
    // stream/listeners, core, FD, context and loop are owned and released here.
    Ok(())
}

struct Video { format: spa::param::video::VideoInfoRaw, result: Rc<RefCell<Option<Result<image::RgbImage, String>>>> }

fn frame(core: &pw::core::Core, mainloop: &pw::main_loop::MainLoop, serial: u64, session: &Session, cancel: &std::sync::atomic::AtomicBool) -> Result<image::RgbImage, String> {
    let result = Rc::new(RefCell::new(None));
    let stream = pw::stream::StreamBox::new(core, "OrbitPal per-request screen", properties! {
        "media.type" => "Video", "media.category" => "Capture", "media.role" => "Screen",
        "target.object" => serial.to_string(),
    }).map_err(|_| "PipeWire-Aufnahme konnte nicht angelegt werden")?;
    let video = Video { format: Default::default(), result: result.clone() };
    let _listener = stream.add_local_listener_with_user_data(video)
        .state_changed(|_, data, old, new| {
            if matches!(new, pw::stream::StreamState::Error(_)) || (new == pw::stream::StreamState::Unconnected && old != pw::stream::StreamState::Unconnected) {
                *data.result.borrow_mut() = Some(Err("PipeWire-Aufnahme wurde getrennt".into()));
            }
        })
        .param_changed(|_, data, id, parameter| {
            if id != spa::param::ParamType::Format.as_raw() { return; }
            let valid = parameter.is_some_and(|parameter| data.format.parse(parameter).is_ok())
                && crate::screen_capture::dimensions(data.format.size().width.into(), data.format.size().height.into()).is_ok()
                && [spa::param::video::VideoFormat::BGRx, spa::param::video::VideoFormat::RGBx, spa::param::video::VideoFormat::BGRA, spa::param::video::VideoFormat::RGBA].contains(&data.format.format());
            if !valid { *data.result.borrow_mut() = Some(Err("PipeWire-Pixelformat oder Bildschirmgröße wird nicht unterstützt".into())); }
        })
        .process(|stream, data| {
            if data.result.borrow().is_some() { return; }
            let Some(mut buffer) = stream.dequeue_buffer() else { return; };
            let image: Result<image::RgbImage,String> = (|| {
                let pixels = buffer.datas_mut();
                if pixels.len() != 1 { return Err("PipeWire hat kein einteiliges CPU-Bild geliefert".into()); }
                let pixels = &mut pixels[0];
                if ![spa::buffer::DataType::MemPtr,spa::buffer::DataType::MemFd].contains(&pixels.type_()) { return Err("PipeWire hat kein CPU-Bild geliefert; DMA-BUF wird nicht übernommen".into()); }
                if pixels.chunk().flags().contains(spa::buffer::ChunkFlags::CORRUPTED) { return Err("PipeWire-Bild ist beschädigt".into()); }
                let offset = pixels.chunk().offset() as usize; let length = pixels.chunk().size() as usize;
                let stride = usize::try_from(pixels.chunk().stride()).map_err(|_| "PipeWire-Bild hat eine ungültige Zeilenlänge")?;
                let bytes = pixels.data().ok_or("PipeWire hat kein lesbares CPU-Bild geliefert (DMA-BUF wird nicht übernommen)")?;
                let end = offset.checked_add(length).ok_or("Ungültige PipeWire-Bildlänge")?;
                let bytes = bytes.get(offset..end).ok_or("PipeWire-Bild ist abgeschnitten")?;
                let channels = if [spa::param::video::VideoFormat::BGRx, spa::param::video::VideoFormat::BGRA].contains(&data.format.format()) { [2,1,0] } else { [0,1,2] };
                crate::screen_capture::rgb_from_rows(bytes, data.format.size().width, data.format.size().height, stride, channels)
            })();
            *data.result.borrow_mut() = Some(image);
        }).register().map_err(|_| "PipeWire-Bildempfang konnte nicht vorbereitet werden")?;
    let format = spa::pod::object!(spa::utils::SpaTypes::ObjectParamFormat, spa::param::ParamType::EnumFormat,
        spa::pod::property!(spa::param::format::FormatProperties::MediaType, Id, spa::param::format::MediaType::Video),
        spa::pod::property!(spa::param::format::FormatProperties::MediaSubtype, Id, spa::param::format::MediaSubtype::Raw),
        spa::pod::property!(spa::param::format::FormatProperties::VideoFormat, Choice, Enum, Id,
            spa::param::video::VideoFormat::BGRx, spa::param::video::VideoFormat::BGRx, spa::param::video::VideoFormat::RGBx, spa::param::video::VideoFormat::BGRA, spa::param::video::VideoFormat::RGBA),
        spa::pod::property!(spa::param::format::FormatProperties::VideoSize, Choice, Range, Rectangle,
            spa::utils::Rectangle { width:1280, height:720 }, spa::utils::Rectangle { width:1, height:1 }, spa::utils::Rectangle { width:16384, height:16384 }),
        spa::pod::property!(spa::param::format::FormatProperties::VideoFramerate, Choice, Range, Fraction,
            spa::utils::Fraction { num:30, denom:1 }, spa::utils::Fraction { num:0, denom:1 }, spa::utils::Fraction { num:60, denom:1 }));
    let format = spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &spa::pod::Value::Object(format)).map_err(|_| "PipeWire-Format konnte nicht erstellt werden")?.0.into_inner();
    let mut parameters = [spa::pod::Pod::from_bytes(&format).ok_or("PipeWire-Format ist ungültig")?];
    // Always target the immutable serial, including older portals resolved once
    // from their restricted registry. Never reconnect to a reused numeric ID.
    stream.connect(spa::utils::Direction::Input, None,
        pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS | pw::stream::StreamFlags::DONT_RECONNECT,
        &mut parameters).map_err(|_| "PipeWire-Quelle konnte nicht verbunden werden")?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if session.cancelled.load(Ordering::SeqCst) || cancel.load(Ordering::SeqCst) { return Err("Wayland-Aufnahme abgebrochen".into()); }
        if let Some(result) = result.borrow_mut().take() { return result; }
        if Instant::now() >= deadline { return Err("Wayland hat kein frisches PipeWire-Bild geliefert".into()); }
        mainloop.loop_().iterate(pw::loop_::Timeout::Finite(Duration::from_millis(30)));
    }
}
