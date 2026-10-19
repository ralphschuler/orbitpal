//! Native, one-frame desktop capture. Call only after the host's current consent
//! checks and on spawn_blocking. No files, shell commands, MCP or capture cache.
//!
//! macOS uses CoreGraphics, Windows uses GDI, Linux uses pure-Rust X11/RandR.
//! Wayland uses the separate explicit ScreenCast/PipeWire session backend;
//! this module never substitutes an XWayland or filesystem screenshot.
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{codecs::jpeg::JpegEncoder, imageops::{resize, FilterType}, ExtendedColorType, RgbImage};
use serde::Serialize;
use std::io::{self, Write};

const MAX_EDGE:u32=1280;
const MAX_SOURCE_EDGE:u64=16_384;
const MAX_SOURCE_PIXELS:u64=64*1024*1024;

#[derive(Clone, Debug, Serialize)]
pub struct CaptureDisplay { pub id:String, pub name:String, pub primary:bool }

/// Enumeration does not take a screenshot or request macOS capture permission.
pub fn displays()->Result<Vec<CaptureDisplay>,String>{platform::displays()}
/// None/empty selects the OS primary monitor, never simply the first monitor.
/// A disconnected/unknown explicit monitor is an error, never a fallback.
pub fn capture(display_id:Option<&str>)->Result<crate::vision::ValidatedImage,String>{encode(platform::capture(display_id)?) }

fn selected(displays:&[CaptureDisplay],requested:Option<&str>)->Result<usize,String>{
    let requested=requested.filter(|id|!id.is_empty());
    let mut matches=displays.iter().enumerate().filter(|(_,display)|requested.map(|id|display.id==id).unwrap_or(display.primary));
    let index=matches.next().map(|(index,_)|index).ok_or_else(||if requested.is_some(){"Der ausgewählte Bildschirm ist nicht mehr verfügbar. Bitte die Bildschirmauswahl aktualisieren."}else{"Der primäre Bildschirm konnte nicht ermittelt werden. Bitte einen Bildschirm ausdrücklich auswählen."}.to_owned())?;
    if matches.next().is_some(){return Err("Die Bildschirmauswahl ist nicht eindeutig. Bitte die verfügbaren Bildschirme aktualisieren.".into());}
    Ok(index)
}
pub(crate) fn dimensions(width:u64,height:u64)->Result<usize,String>{
    if width==0||height==0||width>MAX_SOURCE_EDGE||height>MAX_SOURCE_EDGE||width.checked_mul(height).is_none_or(|pixels|pixels>MAX_SOURCE_PIXELS){return Err("Bildschirmgröße wird nicht unterstützt (maximal 16.384 Pixel je Kante und 64 Megapixel).".into());}
    usize::try_from(width*height*4).map_err(|_|"Bildschirmgröße überschreitet den verfügbaren Adressraum.".into())
}
pub(crate) fn rgb_from_rows(bytes:&[u8],width:u32,height:u32,stride:usize,channels:[usize;3])->Result<RgbImage,String>{
    dimensions(width.into(),height.into())?;
    let row=width as usize*4;
    let required=stride.checked_mul(height as usize-1).and_then(|length|length.checked_add(row)).ok_or("Ungültige Zeilenlänge der Bildschirmaufnahme")?;
    if stride<row||required>bytes.len()||stride>row+4096||channels.iter().any(|offset|*offset>3){return Err("Unvollständige oder ungültige Pixeldaten der Bildschirmaufnahme.".into());}
    let mut rgb=Vec::with_capacity(width as usize*height as usize*3);
    for y in 0..height as usize{for pixel in bytes[y*stride..y*stride+row].chunks_exact(4){rgb.extend_from_slice(&[pixel[channels[0]],pixel[channels[1]],pixel[channels[2]]]);}}
    RgbImage::from_raw(width,height,rgb).ok_or("Bildschirm-Pixeldaten konnten nicht gelesen werden.".into())
}
fn fitted(width:u32,height:u32)->(u32,u32){
    let edge=width.max(height);if edge<=MAX_EDGE{return (width,height);}
    (((u64::from(width)*u64::from(MAX_EDGE)/u64::from(edge)).max(1)) as u32,((u64::from(height)*u64::from(MAX_EDGE)/u64::from(edge)).max(1)) as u32)
}
struct BoundedJpeg(Vec<u8>);
impl Write for BoundedJpeg{
    fn write(&mut self,bytes:&[u8])->io::Result<usize>{
        if self.0.len().checked_add(bytes.len()).is_none_or(|length|length>crate::vision::MAX_IMAGE_BYTES){return Err(io::Error::other("Bildschirm-JPEG überschreitet 5 MiB"));}
        self.0.extend_from_slice(bytes);Ok(bytes.len())
    }
    fn flush(&mut self)->io::Result<()>{Ok(())}
}
pub(crate) fn encode(image:RgbImage)->Result<crate::vision::ValidatedImage,String>{
    dimensions(image.width().into(),image.height().into())?;
    let size=fitted(image.width(),image.height());
    let image=if size==(image.width(),image.height()){image}else{resize(&image,size.0,size.1,FilterType::Lanczos3)};
    let mut output=BoundedJpeg(Vec::new());
    JpegEncoder::new_with_quality(&mut output,88).encode(image.as_raw(),image.width(),image.height(),ExtendedColorType::Rgb8).map_err(|_|"Bildschirmaufnahme konnte nicht als begrenztes JPEG kodiert werden.".to_owned())?;
    // Use the shared MIME/header/byte-budget validator used by all providers.
    crate::vision::parse_data_uri(&format!("data:image/jpeg;base64,{}",STANDARD.encode(output.0)))
}

#[cfg(target_os="macos")]
mod platform{
    use super::*;
    use core_graphics::{color_space::CGColorSpace,context::{CGContext,CGInterpolationQuality},display::CGDisplay,geometry::{CGPoint,CGRect,CGSize},image::CGImage};
    use foreign_types::ForeignType;
    #[link(name="CoreGraphics",kind="framework")]
    unsafe extern "C"{
        fn CGPreflightScreenCaptureAccess()->bool;
        fn CGBitmapContextCreate(data:*mut std::ffi::c_void,width:usize,height:usize,bits:usize,stride:usize,space:*mut std::ffi::c_void,info:u32)->*mut std::ffi::c_void;
    }
    fn monitors()->Result<Vec<(CaptureDisplay,CGDisplay)>,String>{
        let ids=CGDisplay::active_displays().map_err(|error|format!("macOS-Bildschirme konnten nicht ermittelt werden: {error:?}"))?;
        Ok(ids.into_iter().enumerate().map(|(index,id)|{let native=CGDisplay::new(id);(CaptureDisplay{id:format!("macos:{id}"),name:format!("Bildschirm {}",index+1),primary:native.is_main()},native)}).collect())
    }
    pub fn displays()->Result<Vec<CaptureDisplay>,String>{Ok(monitors()?.into_iter().map(|(info,_)|info).collect())}
    pub fn capture(requested:Option<&str>)->Result<RgbImage,String>{
        let monitors=monitors()?;let index=selected(&monitors.iter().map(|(info,_)|info.clone()).collect::<Vec<_>>(),requested)?;let native=&monitors[index].1;
        dimensions(native.pixels_wide(),native.pixels_high())?;
        // Preflight does not open a permission prompt. Never return a partial
        // wallpaper-only image when macOS has withheld screen-recording access.
        if !unsafe{CGPreflightScreenCaptureAccess()}{return Err("macOS hat die Bildschirmaufnahme nicht freigegeben. Bitte OrbitPal unter Systemeinstellungen → Datenschutz & Sicherheit → Bildschirmaufnahme erlauben und die App gegebenenfalls neu starten.".into());}
        let image=native.image().ok_or("macOS hat keine Bildschirmaufnahme geliefert. Berechtigung, gesperrten Bildschirm und Bildschirmverbindung prüfen.")?;
        convert(&image)
    }
    fn convert(image:&CGImage)->Result<RgbImage,String>{
        dimensions(image.width() as u64,image.height() as u64)?;
        // Normalize through Quartz instead of guessing byte order, alpha/HDR
        // layout or color space. Only the bounded output bitmap is allocated.
        let (width,height)=fitted(image.width() as u32,image.height() as u32);let stride=width as usize*4;
        let mut pixels=vec![0u8;stride*height as usize];let space=CGColorSpace::create_device_rgb();
        // Explicit RGBA, 8-bit components, big-endian byte order.
        let context=unsafe{CGBitmapContextCreate(pixels.as_mut_ptr().cast(),width as usize,height as usize,8,stride,space.as_ptr().cast(),0x4000|1)};
        if context.is_null(){return Err("macOS konnte keinen begrenzten Aufnahme-Puffer anlegen.".into());}
        let context=unsafe{CGContext::from_ptr(context.cast())};
        context.set_interpolation_quality(CGInterpolationQuality::CGInterpolationQualityHigh);context.draw_image(CGRect::new(&CGPoint::new(0.0,0.0),&CGSize::new(width.into(),height.into())),image);context.flush();drop(context);
        rgb_from_rows(&pixels,width,height,stride,[0,1,2])
    }
    #[cfg(test)]
    mod tests{
        use super::*;
        use core_graphics::data_provider::CGDataProvider;
        #[test]fn synthetic_quartz_image_preserves_colors_and_row_order_without_screen_access(){
            // This is an in-memory image/context test, never CGDisplay::image
            // or CGPreflightScreenCaptureAccess and never a permission request.
            let rgba=vec![255,0,0,255,0,255,0,255,0,0,255,255,255,255,255,255];
            let image=CGImage::new(2,2,8,32,8,&CGColorSpace::create_device_rgb(),0x4000|1,&CGDataProvider::from_buffer(std::sync::Arc::new(rgba)),false,0);
            let rgb=convert(&image).unwrap();assert_eq!(rgb.as_raw(),&[255,0,0,0,255,0,0,0,255,255,255,255]);
        }
    }
}

#[cfg(target_os="windows")]
mod platform{
    use super::*;
    use std::{mem::{size_of,zeroed},ptr};
    use windows_sys::Win32::{Foundation::{LPARAM,RECT},Graphics::Gdi::*};
    struct Monitor{info:CaptureDisplay,device:[u16;32]}
    struct Enumeration{monitors:Vec<Monitor>,failed:bool}
    unsafe extern "system" fn enumerate(monitor:HMONITOR,_:HDC,_:*mut RECT,data:LPARAM)->i32{
        // EnumDisplayMonitors invokes this synchronously; the caller owns the
        // context for the full invocation. No panic or reference escapes FFI.
        let context=unsafe{&mut *(data as *mut Enumeration)};
        let mut info:MONITORINFOEXW=unsafe{zeroed()};info.monitorInfo.cbSize=size_of::<MONITORINFOEXW>() as u32;
        if unsafe{GetMonitorInfoW(monitor,&mut info.monitorInfo)}==0{context.failed=true;return 0;}
        let end=info.szDevice.iter().position(|ch|*ch==0).unwrap_or(info.szDevice.len());let device=String::from_utf16_lossy(&info.szDevice[..end]);
        if device.is_empty(){context.failed=true;return 0;}
        context.monitors.push(Monitor{info:CaptureDisplay{id:format!("windows:{device}"),name:device.trim_start_matches("\\\\.\\").to_owned(),primary:info.monitorInfo.dwFlags&1!=0},device:info.szDevice});1
    }
    fn monitors()->Result<Vec<Monitor>,String>{
        let mut context=Enumeration{monitors:Vec::new(),failed:false};
        if unsafe{EnumDisplayMonitors(ptr::null_mut(),ptr::null(),Some(enumerate),&mut context as *mut _ as LPARAM)}==0||context.failed{return Err(format!("Windows-Bildschirme konnten nicht ermittelt werden: {}",io::Error::last_os_error()));}Ok(context.monitors)
    }
    struct Dc(HDC);impl Drop for Dc{fn drop(&mut self){unsafe{DeleteDC(self.0);}}}
    struct Bitmap(HBITMAP);impl Drop for Bitmap{fn drop(&mut self){unsafe{DeleteObject(self.0);}}}
    struct Selection{dc:HDC,previous:HGDIOBJ}impl Drop for Selection{fn drop(&mut self){unsafe{SelectObject(self.dc,self.previous);}}}
    pub fn displays()->Result<Vec<CaptureDisplay>,String>{Ok(monitors()?.into_iter().map(|monitor|monitor.info).collect())}
    pub fn capture(requested:Option<&str>)->Result<RgbImage,String>{
        let monitors=monitors()?;let index=selected(&monitors.iter().map(|monitor|monitor.info.clone()).collect::<Vec<_>>(),requested)?;let monitor=&monitors[index];
        let driver:Vec<u16>="DISPLAY\0".encode_utf16().collect();let screen=unsafe{CreateDCW(driver.as_ptr(),monitor.device.as_ptr(),ptr::null(),ptr::null())};
        if screen.is_null(){return Err("Windows konnte den ausgewählten Bildschirm nicht öffnen.".into());}let screen=Dc(screen);
        let width=unsafe{GetDeviceCaps(screen.0,HORZRES as i32)};let height=unsafe{GetDeviceCaps(screen.0,VERTRES as i32)};
        if width<=0||height<=0{return Err("Windows meldet eine ungültige Bildschirmgröße.".into());}let bytes=dimensions(width as u64,height as u64)?;
        let target=unsafe{CreateCompatibleDC(screen.0)};if target.is_null(){return Err("Windows konnte keinen Aufnahme-Kontext anlegen.".into());}let target=Dc(target);
        let bitmap=unsafe{CreateCompatibleBitmap(screen.0,width,height)};if bitmap.is_null(){return Err("Windows konnte keinen begrenzten Bildschirm-Puffer anlegen.".into());}let bitmap=Bitmap(bitmap);
        let previous=unsafe{SelectObject(target.0,bitmap.0)};if previous.is_null()||previous as isize == -1{return Err("Windows konnte den Aufnahme-Puffer nicht auswählen.".into());}let selection=Selection{dc:target.0,previous};
        if unsafe{BitBlt(target.0,0,0,width,height,screen.0,0,0,SRCCOPY|CAPTUREBLT)}==0{return Err(format!("Windows-Bildschirmaufnahme fehlgeschlagen: {}",io::Error::last_os_error()));}
        // GetDIBits requires the bitmap to be unselected. Restore it on every
        // error path before the owned bitmap/DC handles are released.
        drop(selection);
        let mut pixels=vec![0u8;bytes];let mut info:BITMAPINFO=unsafe{zeroed()};info.bmiHeader.biSize=size_of::<BITMAPINFOHEADER>() as u32;info.bmiHeader.biWidth=width;info.bmiHeader.biHeight=-height;info.bmiHeader.biPlanes=1;info.bmiHeader.biBitCount=32;info.bmiHeader.biCompression=BI_RGB;
        if unsafe{GetDIBits(screen.0,bitmap.0,0,height as u32,pixels.as_mut_ptr().cast(),&mut info,DIB_RGB_COLORS)}!=height{return Err("Windows hat keine vollständige Bildschirmaufnahme geliefert.".into());}
        rgb_from_rows(&pixels,width as u32,height as u32,width as usize*4,[2,1,0])
    }
}

#[cfg(target_os="linux")]
mod platform{
    use super::*;
    use x11rb::{connection::Connection,image::{Image,PixelLayout},protocol::{randr::{ConnectionExt as _,MonitorInfo},xproto::ConnectionExt as _},rust_connection::RustConnection};
    struct Monitor{info:CaptureDisplay,geometry:MonitorInfo}
    fn monitors()->Result<(RustConnection,usize,Vec<Monitor>),String>{
        if crate::screen_portal::is_wayland(){return Err("Unter Wayland wird die Quelle ausdrücklich im ScreenCast-Freigabedialog ausgewählt; es gibt keinen automatischen Hauptbildschirm oder XWayland-Ersatz.".into());}
        let (connection,screen)=x11rb::connect(None).map_err(|_|"X11-Bildschirmverbindung nicht verfügbar. Bitte eine laufende lokale X11-Sitzung verwenden.".to_owned())?;
        let version=connection.randr_query_version(1,5).map_err(|_|"X11/RandR ist nicht verfügbar")?.reply().map_err(|_|"X11/RandR-Version konnte nicht ermittelt werden")?;
        if version.major_version<1||(version.major_version==1&&version.minor_version<5){return Err("Für die native Bildschirmauswahl wird X11/RandR 1.5 oder neuer benötigt.".into());}
        let root=connection.setup().roots.get(screen).ok_or("X11-Hauptbildschirm fehlt")?.root;
        let reply=connection.randr_get_monitors(root,true).map_err(|_|"X11/RandR-Bildschirme konnten nicht angefordert werden")?.reply().map_err(|_|"Die X11-Sitzung unterstützt keine RandR-Bildschirmauswahl")?;
        let mut monitors=Vec::new();
        for geometry in reply.monitors{
            let name=connection.get_atom_name(geometry.name).map_err(|_|"X11-Bildschirmname fehlt")?.reply().map_err(|_|"X11-Bildschirmname nicht verfügbar")?.name;
            let name=String::from_utf8_lossy(&name).into_owned();if name.is_empty(){return Err("X11 hat einen Bildschirm ohne eindeutigen Namen gemeldet.".into());}
            monitors.push(Monitor{info:CaptureDisplay{id:format!("x11:{name}"),name,primary:geometry.primary},geometry});
        }
        Ok((connection,screen,monitors))
    }
    pub fn displays()->Result<Vec<CaptureDisplay>,String>{Ok(monitors()?.2.into_iter().map(|monitor|monitor.info).collect())}
    pub fn capture(requested:Option<&str>)->Result<RgbImage,String>{
        let (connection,screen,monitors)=monitors()?;let index=selected(&monitors.iter().map(|monitor|monitor.info.clone()).collect::<Vec<_>>(),requested)?;let geometry=&monitors[index].geometry;
        dimensions(geometry.width.into(),geometry.height.into())?;
        let root=connection.setup().roots.get(screen).ok_or("X11-Hauptbildschirm fehlt")?;
        let (image,visual)=Image::get(&connection,root.root,geometry.x,geometry.y,geometry.width,geometry.height).map_err(|_|"X11 hat keine vollständige Bildschirmaufnahme geliefert")?;
        let visual=root.allowed_depths.iter().flat_map(|depth|depth.visuals.iter()).find(|entry|entry.visual_id==visual).ok_or("X11-Pixelformat ist nicht verfügbar")?;
        let layout=PixelLayout::from_visual_type(*visual).map_err(|_|"Das X11-Pixelformat wird nicht unterstützt")?;
        let mut rgb=RgbImage::new(geometry.width.into(),geometry.height.into());
        for (x,y,pixel) in rgb.enumerate_pixels_mut(){let (red,green,blue)=layout.decode(image.get_pixel(x as u16,y as u16));*pixel=image::Rgb([(red>>8) as u8,(green>>8) as u8,(blue>>8) as u8]);}Ok(rgb)
    }
}

#[cfg(not(any(target_os="macos",target_os="windows",target_os="linux")))]
mod platform{use super::*;pub fn displays()->Result<Vec<CaptureDisplay>,String>{Err("Native Bildschirmaufnahme ist auf diesem System nicht verfügbar.".into())}pub fn capture(_:Option<&str>)->Result<RgbImage,String>{Err("Native Bildschirmaufnahme ist auf diesem System nicht verfügbar.".into())}}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn selection_uses_real_primary_and_never_falls_back_for_missing_explicit_ids(){
        let mut displays=vec![CaptureDisplay{id:"secondary".into(),name:"Second".into(),primary:false},CaptureDisplay{id:"primary".into(),name:"First".into(),primary:true}];
        assert_eq!(selected(&displays,None).unwrap(),1);assert_eq!(selected(&displays,Some("")).unwrap(),1);assert_eq!(selected(&displays,Some("secondary")).unwrap(),0);
        for missing in ["unplugged"," ","Primary"]{assert!(selected(&displays,Some(missing)).is_err());}assert!(selected(&[],None).is_err());displays[1].primary=false;assert!(selected(&displays,None).is_err());displays[0].primary=true;displays[1].primary=true;assert!(selected(&displays,None).is_err());
    }
    #[test]fn dimensions_are_bounded_before_pixel_allocations(){
        for (width,height) in [(0,1),(1,0),(16_385,1),(1,16_385),(16_384,16_384),(u64::MAX,u64::MAX)]{assert!(dimensions(width,height).is_err());}
        assert_eq!(dimensions(7680,4320).unwrap(),7680*4320*4);
    }
    #[test]fn native_row_padding_color_channels_and_incomplete_frames_are_checked(){
        let bytes=[0,0,255,0, 0,255,0,0, 42,42,42,42, 255,0,0,0, 255,255,255,0, 42,42,42,42];
        let image=rgb_from_rows(&bytes,2,2,12,[2,1,0]).unwrap();assert_eq!(image.as_raw(),&[255,0,0,0,255,0,0,0,255,255,255,255]);
        assert!(rgb_from_rows(&bytes[..15],2,2,12,[2,1,0]).is_err());assert!(rgb_from_rows(&bytes,2,2,7,[2,1,0]).is_err());assert!(rgb_from_rows(&bytes,2,2,12,[4,1,0]).is_err());
    }
    #[test]fn jpeg_is_real_bounded_and_resized_without_upscaling_or_stretching(){
        assert_eq!(fitted(3840,2160),(1280,720));assert_eq!(fitted(1200,2400),(640,1280));assert_eq!(fitted(320,200),(320,200));
        let image=RgbImage::from_fn(1920,1080,|x,y|image::Rgb([(x%256) as u8,(y%256) as u8,110]));let jpeg=encode(image).unwrap();assert_eq!(jpeg.mime_type,"image/jpeg");assert!(jpeg.bytes>0&&jpeg.bytes<=crate::vision::MAX_IMAGE_BYTES);
        let bytes=STANDARD.decode(&jpeg.base64).unwrap();let decoded=image::load_from_memory_with_format(&bytes,image::ImageFormat::Jpeg).unwrap();assert_eq!((decoded.width(),decoded.height()),(1280,720));assert_eq!(bytes.len(),jpeg.bytes);assert!(crate::vision::parse_data_uri(&jpeg.data_uri()).is_ok());
    }
    #[test]fn jpeg_writer_refuses_oversize_data_without_partial_append(){
        let mut output=BoundedJpeg(vec![0;crate::vision::MAX_IMAGE_BYTES-2]);assert!(output.write_all(&[1,2,3]).is_err());assert_eq!(output.0.len(),crate::vision::MAX_IMAGE_BYTES-2);output.write_all(&[1,2]).unwrap();assert!(output.write_all(&[3]).is_err());
    }
}
