//! Native, read-only Shell adapter. Every call stays on the discovery worker;
//! no target activation, link resolution/repair, registry writes or shell verbs.

use std::{ffi::c_void, mem::size_of, path::Path, sync::Arc};

use anyhow::{Context, Result, ensure};
use windows::{
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, RPC_E_CHANGED_MODE, SIZE},
        Graphics::Gdi::{
            BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC,
            GetDIBits, GetObjectW, HBITMAP, HGDIOBJ, ReleaseDC,
        },
        Storage::EnhancedStorage::PKEY_AppUserModel_ID,
        System::{
            Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize},
            Registry::{HKEY_CURRENT_USER, RRF_RT_REG_BINARY, RRF_RT_REG_DWORD, RegGetValueW},
        },
        UI::Shell::{
            IShellItem2, IShellItemImageFactory, SHCreateItemFromIDList,
            SHCreateItemFromParsingName, SIGDN_FILESYSPATH, SIGDN_NORMALDISPLAY, SIIGBF_ICONONLY,
        },
    },
    core::{Interface, PCWSTR, PWSTR, w},
};

use super::{
    AppKind, PinnedApp,
    windows::{MAX_TASKBAND_BYTES, TaskbandPin, canonical_app_id, resolve_taskband},
};
use crate::app_icons::AppIcon;

const TASKBAND: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Taskband");

struct Apartment(bool);

impl Apartment {
    fn enter() -> Result<Self> {
        // SAFETY: Initialize this worker's COM apartment with no reserved data.
        // An existing apartment is usable too, but must not be uninitialized here.
        let status = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if status == RPC_E_CHANGED_MODE {
            return Ok(Self(false));
        }
        status.ok().context("Initialize Shell metadata apartment")?;
        Ok(Self(true))
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: Balances this thread's successful CoInitializeEx only;
            // all Shell objects have already been dropped before this guard.
            unsafe { CoUninitialize() };
        }
    }
}

pub(super) fn discover() -> Result<Vec<PinnedApp>> {
    let Some(bytes) = favorites()? else {
        return Ok(Vec::new());
    };
    let _apartment = Apartment::enter()?;
    resolve_taskband(&bytes, resolve_pin)
}

fn favorites() -> Result<Option<Vec<u8>>> {
    let mut version = 0u32;
    let mut version_size = size_of::<u32>() as u32;
    // SAFETY: Predefined HKCU is borrowed, strings are static/terminated, and
    // the DWORD output points to a live u32 with its exact writable capacity.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            TASKBAND,
            w!("FavoritesVersion"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut version as *mut u32).cast()),
            Some(&mut version_size),
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    status.ok().context("Read Taskband FavoritesVersion")?;
    ensure!(
        version == 3,
        "Unsupported Windows Taskband FavoritesVersion {version}"
    );
    let mut length = 0u32;
    // SAFETY: Size-only query has no data buffer; output size is a live u32.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            TASKBAND,
            w!("Favorites"),
            RRF_RT_REG_BINARY,
            None,
            None,
            Some(&mut length),
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    status.ok().context("Inspect current taskbar pins")?;
    ensure!(
        length as usize <= MAX_TASKBAND_BYTES,
        "Taskband metadata exceeds one MiB"
    );
    let mut bytes = vec![0u8; length as usize];
    // SAFETY: Registry receives exactly this initialized buffer's capacity.
    // A concurrent larger snapshot produces ERROR_MORE_DATA, never a partial
    // pin inventory; callers can explicitly refresh after the change settles.
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            TASKBAND,
            w!("Favorites"),
            RRF_RT_REG_BINARY,
            None,
            Some(bytes.as_mut_ptr().cast()),
            Some(&mut length),
        )
        .ok()
        .context("Read current taskbar pins")?;
    }
    bytes.truncate(length as usize);
    Ok(Some(bytes))
}

fn resolve_pin(pin: &TaskbandPin<'_>) -> Option<PinnedApp> {
    // Taskband records are not naturally aligned. ITEMIDLIST requires word
    // alignment, and this copy also keeps its validated terminator alive.
    let aligned = pin
        .pidl
        .chunks(2)
        .map(|word| u16::from_le_bytes([word[0], *word.get(1).unwrap_or(&0)]))
        .collect::<Vec<_>>();
    // SAFETY: The portable parser validated every item and the terminator;
    // aligned storage remains alive throughout creation, which copies the PIDL.
    let item: IShellItem2 = unsafe { SHCreateItemFromIDList(aligned.as_ptr().cast()) }.ok()?;
    let id = pin.app_id.clone().or_else(|| item_id(&item))?;
    canonical_app_id(&id)?;
    // Resolve the current AppsFolder entry first: a captured packaged PIDL may
    // name an old installed package version after a Store update. AppsFolder is
    // a resolver for THIS pin, never a substitute inventory of suggested apps.
    if let Some(current) = apps_folder_item(&id) {
        return metadata(&current, id);
    }
    // A remaining classic pin must still reference a real local shortcut.
    // Never resurrect uninstalled packaged entries using their cached metadata.
    // SAFETY: Live Shell interface; returned COM string is freed by take_string.
    let path = take_string(unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }.ok()?)?;
    let path = Path::new(&path);
    if !path.is_file()
        || !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"))
    {
        return None;
    }
    metadata(&item, id)
}

pub(super) fn application(id: &str) -> Result<Option<PinnedApp>> {
    ensure!(
        canonical_app_id(id).is_some(),
        "Invalid Windows application routing ID"
    );
    let _apartment = Apartment::enter()?;
    if let Some(item) = apps_folder_item(id) {
        return Ok(metadata(&item, id.to_owned()));
    }
    // Shell-generated Win32 IDs can be absolute executable paths even when
    // the application has no registered AppsFolder entry. Read its metadata
    // directly, never interpret arguments, expand environment variables or run it.
    if let Some(item) = local_executable_item(id) {
        return Ok(metadata(&item, id.to_owned()));
    }
    // Some Win32 shortcuts have Explorer-generated IDs but no AppsFolder item.
    // This also supports exact observed IDs for those currently pinned entries.
    let Some(bytes) = favorites()? else {
        return Ok(None);
    };
    let apps = resolve_taskband(&bytes, |pin| {
        if pin.app_id.as_deref().is_some_and(|pin_id| pin_id != id) {
            return None;
        }
        resolve_pin(pin).filter(|app| app.id == id)
    })?;
    Ok(apps.into_iter().find(|app| app.id == id))
}

fn apps_folder_item(id: &str) -> Option<IShellItem2> {
    canonical_app_id(id)?;
    let name = format!("shell:AppsFolder\\{id}")
        .encode_utf16()
        .chain([0])
        .collect::<Vec<_>>();
    // SAFETY: The parsing name is NUL-terminated and remains live throughout
    // the call; no bind context, launch verb, or executable invocation is used.
    let item: IShellItem2 =
        unsafe { SHCreateItemFromParsingName(PCWSTR(name.as_ptr()), None) }.ok()?;
    // Do not accept a namespace/path interpretation as a different application.
    (item_id(&item).as_deref() == Some(id)).then_some(item)
}

fn local_executable_item(id: &str) -> Option<IShellItem2> {
    let path = Path::new(id);
    let bytes = id.as_bytes();
    // Only a local drive-qualified executable, not UNC/device/URI namespaces.
    if bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1..3] != *b":\\"
        || !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        || !path.is_file()
    {
        return None;
    }
    let name = id.encode_utf16().chain([0]).collect::<Vec<_>>();
    // SAFETY: Existing local file and live terminated buffer. Creating a Shell
    // metadata item does not invoke the executable or a context-menu verb.
    unsafe { SHCreateItemFromParsingName(PCWSTR(name.as_ptr()), None) }.ok()
}

fn item_id(item: &IShellItem2) -> Option<String> {
    // SAFETY: Property key is the SDK's System.AppUserModel.ID and item is live.
    let id = take_string(unsafe { item.GetString(&PKEY_AppUserModel_ID) }.ok()?)?;
    canonical_app_id(&id)?;
    Some(id)
}

fn metadata(item: &IShellItem2, id: String) -> Option<PinnedApp> {
    // SAFETY: Live interface; ownership of its allocated string is transferred.
    let name = take_string(unsafe { item.GetDisplayName(SIGDN_NORMALDISPLAY) }.ok()?)?;
    let name = name.strip_suffix(".lnk").unwrap_or(&name);
    if name.is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
        return None;
    }
    let kind = super::windows::classify_app(&id, name);
    Some(PinnedApp {
        id,
        name: name.to_owned(),
        kind,
        suggested: matches!(kind, AppKind::Communication | AppKind::Productivity),
        icon: shell_icon(item).map(Arc::new),
    })
}

fn take_string(value: PWSTR) -> Option<String> {
    if value.is_null() {
        return None;
    }
    // SAFETY: Only documented CoTaskMem-allocated Shell output strings enter
    // here. Bound the scan before copying; free exactly once even on bad text.
    unsafe {
        let mut length = 0usize;
        while length < 32_768 && *value.0.add(length) != 0 {
            length += 1;
        }
        let result = (length < 32_768)
            .then(|| String::from_utf16(std::slice::from_raw_parts(value.0, length)).ok())
            .flatten();
        CoTaskMemFree(Some(value.0.cast::<c_void>()));
        result
    }
}

struct Bitmap(HBITMAP);
impl Drop for Bitmap {
    fn drop(&mut self) {
        // SAFETY: We own this GetImage bitmap and never select it into a DC.
        let _ = unsafe { DeleteObject(HGDIOBJ(self.0.0)) };
    }
}

fn shell_icon(item: &IShellItem2) -> Option<AppIcon> {
    let factory: IShellItemImageFactory = item.cast().ok()?;
    // SAFETY: Valid factory; GetImage transfers ownership of the returned HBITMAP.
    let bitmap =
        Bitmap(unsafe { factory.GetImage(SIZE { cx: 64, cy: 64 }, SIIGBF_ICONONLY) }.ok()?);
    let mut object = BITMAP::default();
    // SAFETY: Query writes only the size of the initialized BITMAP output.
    if unsafe {
        GetObjectW(
            HGDIOBJ(bitmap.0.0),
            size_of::<BITMAP>() as i32,
            Some((&mut object as *mut BITMAP).cast()),
        )
    } == 0
    {
        return None;
    }
    let (width, height) = (object.bmWidth, object.bmHeight);
    if !(1..=256).contains(&width) || !(1..=256).contains(&height) {
        return None;
    }
    let mut rgba = vec![0u8; width as usize * height as usize * 4];
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY: Borrow screen DC for this synchronous copy, release on every path.
    // The top-down 32bpp buffer has exactly width*height*4 bytes, with no padding.
    let copied = unsafe {
        let dc = GetDC(None);
        if dc.is_invalid() {
            return None;
        }
        let copied = GetDIBits(
            dc,
            bitmap.0,
            0,
            height as u32,
            Some(rgba.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        ReleaseDC(None, dc);
        copied
    };
    if copied != height {
        return None;
    }
    let mut totals = [0u64; 3];
    let mut weight = 0u64;
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        let alpha = pixel[3] as u64;
        for channel in 0..3 {
            pixel[channel] = (pixel[channel] as u64 * 255)
                .checked_div(alpha)
                .unwrap_or(0)
                .min(255) as u8;
            totals[channel] += pixel[channel] as u64 * alpha;
        }
        weight += alpha;
    }
    if weight == 0 {
        return None;
    }
    Some(AppIcon {
        width: width as usize,
        height: height as usize,
        rgba,
        dominant: totals.map(|total| (total / weight) as u8),
    })
}
