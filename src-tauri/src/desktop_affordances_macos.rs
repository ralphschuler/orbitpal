//! Read-only numeric window/cursor geometry; no titles, images, event taps or input.
use super::{GeometrySample, ReadContext, Rect};
use core_foundation_sys::{
    array::{CFArrayGetCount, CFArrayGetValueAtIndex},
    base::{CFGetTypeID, CFRelease, CFTypeRef},
    dictionary::{CFDictionaryGetTypeID, CFDictionaryGetValue, CFDictionaryRef},
    number::{
        kCFNumberSInt64Type, CFBooleanGetTypeID, CFBooleanGetValue, CFNumberGetTypeID,
        CFNumberGetValue,
    },
};
use core_graphics::{
    display::CGDisplay,
    geometry::CGRect,
    window::{
        kCGNullWindowID, kCGWindowBounds, kCGWindowIsOnscreen, kCGWindowLayer,
        kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly, kCGWindowOwnerPID,
        CGWindowListCopyWindowInfo,
    },
};
use objc2::{
    encode::{Encode, Encoding},
    msg_send,
    runtime::AnyClass,
};
use std::ffi::c_void;
#[repr(C)]
struct CocoaPoint {
    x: f64,
    y: f64,
}
// NSEvent.mouseLocation's ABI is NSPoint/CGPoint, two CGFloat (64-bit) fields.
unsafe impl Encode for CocoaPoint {
    const ENCODING: Encoding = Encoding::Struct("CGPoint", &[Encoding::Double, Encoding::Double]);
}
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGRectMakeWithDictionaryRepresentation(dict: CFDictionaryRef, rect: *mut CGRect) -> bool;
}
struct Owned(CFTypeRef);
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe {
            CFRelease(self.0);
        }
    }
}
unsafe fn integer(dict: CFDictionaryRef, key: *const c_void) -> Option<i64> {
    let value = CFDictionaryGetValue(dict, key);
    if value.is_null() || CFGetTypeID(value) != CFNumberGetTypeID() {
        return None;
    }
    let mut n = 0i64;
    if CFNumberGetValue(
        value.cast(),
        kCFNumberSInt64Type,
        (&mut n as *mut i64).cast(),
    ) {
        Some(n)
    } else {
        None
    }
}
unsafe fn onscreen(dict: CFDictionaryRef) -> bool {
    let value = CFDictionaryGetValue(dict, kCGWindowIsOnscreen.cast());
    !value.is_null()
        && CFGetTypeID(value) == CFBooleanGetTypeID()
        && CFBooleanGetValue(value.cast())
}
fn finite_rect(r: CGRect) -> bool {
    [r.origin.x, r.origin.y, r.size.width, r.size.height]
        .iter()
        .all(|n| n.is_finite())
        && r.size.width > 0.
        && r.size.height > 0.
        && r.size.width <= 65536.
        && r.size.height <= 65536.
}
fn rounded(value: f64) -> Option<i32> {
    if !value.is_finite()
        || value.round() < f64::from(i32::MIN)
        || value.round() > f64::from(i32::MAX)
    {
        None
    } else {
        Some(value.round() as i32)
    }
}
/// Call on the native main thread after separate opt-in and exact local fence.
pub(super) fn read(
    context: &ReadContext,
    windows: bool,
    cursor: bool,
    now: u64,
) -> Result<GeometrySample, ()> {
    if objc2::MainThreadMarker::new().is_none() || !context.valid() || (!windows && !cursor) {
        return Err(());
    }
    // Match the locked Tao monitor formula exactly, rather than assuming Retina
    // pixels and global Quartz screen points have the same origin or scale.
    let displays = CGDisplay::active_displays().map_err(|_| ())?;
    if displays.is_empty() || displays.len() > 32 {
        return Err(());
    }
    let mut matching = vec![];
    for id in displays {
        let d = CGDisplay::new(id);
        let r = d.bounds();
        if !finite_rect(r) {
            continue;
        }
        let x = rounded(r.origin.x * context.scale);
        let y = rounded(r.origin.y * context.scale);
        let width = (d.pixels_wide() as f64 * context.scale).round();
        let height = (d.pixels_high() as f64 * context.scale).round();
        let p = context.physical_display;
        if x == Some(p.x)
            && y == Some(p.y)
            && width == f64::from(p.width)
            && height == f64::from(p.height)
            && r.size.width == d.pixels_wide() as f64
            && r.size.height == d.pixels_high() as f64
        {
            matching.push(r)
        }
    }
    if matching.len() != 1 {
        return Err(());
    }
    let display = matching[0];
    let physical = |x: f64, y: f64| -> Option<(i32, i32)> {
        Some((
            rounded(
                f64::from(context.physical_display.x) + (x - display.origin.x) * context.scale,
            )?,
            rounded(
                f64::from(context.physical_display.y) + (y - display.origin.y) * context.scale,
            )?,
        ))
    };
    let mut top = None;
    if windows {
        unsafe {
            let raw = CGWindowListCopyWindowInfo(
                kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
                kCGNullWindowID,
            );
            if raw.is_null() {
                return Err(());
            }
            let owned = Owned(raw.cast());
            let count = CFArrayGetCount(raw);
            if !(0..=4096).contains(&count) {
                return Err(());
            }
            for index in 0..count {
                let raw = CFArrayGetValueAtIndex(raw, index);
                if raw.is_null() || CFGetTypeID(raw) != CFDictionaryGetTypeID() {
                    continue;
                }
                let dict = raw as CFDictionaryRef;
                let Some(pid) = integer(dict, kCGWindowOwnerPID.cast()) else {
                    continue;
                };
                if pid <= 0
                    || pid == i64::from(std::process::id())
                    || integer(dict, kCGWindowLayer.cast()) != Some(0)
                    || !onscreen(dict)
                {
                    continue;
                }
                let bounds = CFDictionaryGetValue(dict, kCGWindowBounds.cast());
                if bounds.is_null() || CFGetTypeID(bounds) != CFDictionaryGetTypeID() {
                    continue;
                }
                let mut rect = CGRect::new(
                    &core_graphics::geometry::CGPoint::new(0., 0.),
                    &core_graphics::geometry::CGSize::new(0., 0.),
                );
                if !CGRectMakeWithDictionaryRepresentation(bounds as CFDictionaryRef, &mut rect)
                    || !finite_rect(rect)
                    || rect.size.width < 100.
                    || rect.size.height < 60.
                {
                    continue;
                }
                let center = rect.origin.x + rect.size.width / 2.;
                if center < display.origin.x
                    || center > display.origin.x + display.size.width
                    || rect.origin.y < display.origin.y
                    || rect.origin.y > display.origin.y + display.size.height
                {
                    continue;
                }
                let Some((x, y)) = physical(rect.origin.x, rect.origin.y) else {
                    continue;
                };
                let width = (rect.size.width * context.scale).round();
                let height = (rect.size.height * context.scale).round();
                if width <= 0. || height <= 0. || width > 65536. || height > 65536. {
                    continue;
                }
                top = Some(Rect {
                    x,
                    y,
                    width: width as u32,
                    height: height as u32,
                });
                break;
            }
            drop(owned);
        }
    }
    let cursor = if cursor {
        let class = AnyClass::get(c"NSEvent").ok_or(())?;
        let p: CocoaPoint = unsafe { msg_send![class, mouseLocation] };
        let primary = CGDisplay::main().bounds();
        if !finite_rect(primary) {
            return Err(());
        }
        physical(p.x, primary.size.height - p.y)
    } else {
        None
    };
    Ok(GeometrySample {
        source_generation: context.generation,
        display_id: context.display_id.clone(),
        at_ms: now,
        work_area: context.physical_scene,
        window_top: top,
        cursor,
    })
}
