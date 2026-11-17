//! Enable dark classic Windows menus for this process, without changing Windows settings.
//! The entry point is version-dependent: resolve it at runtime and keep native defaults
//! when unavailable. The supported application minimum is Windows 10 build 19041.
#[cfg(windows)]
pub fn dark_menus() -> bool {
    use std::sync::LazyLock;
    use windows_sys::Win32::{
        System::{
            LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32},
            SystemInformation::OSVERSIONINFOW,
        },
        UI::{
            Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
            WindowsAndMessaging::{SystemParametersInfoW, SPI_GETHIGHCONTRAST},
        },
    };
    type SetMode = unsafe extern "system" fn(i32) -> i32;
    type Flush = unsafe extern "system" fn();
    struct Api {
        set_mode: SetMode,
        flush: Option<Flush>,
    }
    static API: LazyLock<Option<Api>> = LazyLock::new(|| unsafe {
        // These libraries are loaded from System32 and retained for the process
        // lifetime so the cached function pointers never outlive their module.
        let ntdll: Vec<u16> = "ntdll.dll\0".encode_utf16().collect();
        let module = LoadLibraryExW(
            ntdll.as_ptr(),
            std::ptr::null_mut(),
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        );
        if module.is_null() {
            return None;
        }
        let version_fn = GetProcAddress(module, c"RtlGetVersion".as_ptr().cast())?;
        let version_fn: unsafe extern "system" fn(*mut OSVERSIONINFOW) -> i32 =
            std::mem::transmute(version_fn);
        let mut version: OSVERSIONINFOW = std::mem::zeroed();
        version.dwOSVersionInfoSize = std::mem::size_of_val(&version) as u32;
        if version_fn(&mut version) < 0
            || version.dwMajorVersion != 10
            || version.dwBuildNumber < 18362
        {
            return None;
        }
        let name: Vec<u16> = "uxtheme.dll\0".encode_utf16().collect();
        let module = LoadLibraryExW(
            name.as_ptr(),
            std::ptr::null_mut(),
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        );
        if module.is_null() {
            return None;
        }
        // Ordinal 135 is SetPreferredAppMode on Windows 10 1903 and later.
        // Earlier builds used another signature and are deliberately excluded.
        let set_mode = std::mem::transmute::<unsafe extern "system" fn() -> isize, SetMode>(
            GetProcAddress(module, 135_usize as *const u8)?,
        );
        let flush = GetProcAddress(module, 136_usize as *const u8)
            .map(|proc| std::mem::transmute::<unsafe extern "system" fn() -> isize, Flush>(proc));
        Some(Api { set_mode, flush })
    });
    let Some(api) = API.as_ref() else {
        return false;
    };
    // Respect the accessibility theme, including changes made while Tonal runs.
    let mut contrast: HIGHCONTRASTW = unsafe { std::mem::zeroed() };
    contrast.cbSize = std::mem::size_of_val(&contrast) as u32;
    let high_contrast = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            contrast.cbSize,
            (&mut contrast as *mut HIGHCONTRASTW).cast(),
            0,
        ) != 0
            && contrast.dwFlags & HCF_HIGHCONTRASTON != 0
    };
    // SAFETY: the runtime version check above validates the ABI; supported mode
    // values are Default=0 and ForceDark=2. No system preference is written.
    unsafe {
        (api.set_mode)(if high_contrast { 0 } else { 2 });
        if let Some(flush) = api.flush {
            flush();
        }
    }
    !high_contrast
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Bureau Windows interactif ; ouvre et annule son propre menu natif, lit ses pixels, aucune action utilisateur ni changement de réglage Windows."]
    fn native_menu_background_is_dark_live() {
        use std::{
            ptr::{null, null_mut},
            time::{Duration, Instant},
        };
        use tray_icon::menu::{ContextMenu, Menu, MenuItem};
        use windows_sys::Win32::{
            Graphics::Gdi::{GetDC, GetPixel, ReleaseDC},
            System::Threading::GetCurrentProcessId,
            UI::WindowsAndMessaging::*,
        };
        assert!(
            dark_menus(),
            "Native dark mode unavailable or accessibility contrast enabled"
        );
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let popup_class: Vec<u16> = "#32768\0".encode_utf16().collect();
        // This hidden owner mirrors the notification icon's hidden owner window.
        let owner = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class.as_ptr(),
                null(),
                WS_POPUP,
                400,
                300,
                200,
                100,
                null_mut(),
                null_mut(),
                null_mut(),
                null(),
            )
        };
        assert!(!owner.is_null());
        struct Owner(windows_sys::Win32::Foundation::HWND);
        impl Drop for Owner {
            fn drop(&mut self) {
                unsafe {
                    DestroyWindow(self.0);
                }
            }
        }
        let owner = Owner(owner);
        let menu = Menu::new();
        menu.append_items(&[
            &MenuItem::new("Ouvrir", true, None),
            &MenuItem::new("Quitter", true, None),
        ])
        .unwrap();
        let pid = unsafe { GetCurrentProcessId() };
        let probe = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline {
                let mut window = null_mut();
                loop {
                    window =
                        unsafe { FindWindowExW(null_mut(), window, popup_class.as_ptr(), null()) };
                    if window.is_null() {
                        break;
                    }
                    let mut found_pid = 0;
                    unsafe {
                        GetWindowThreadProcessId(window, &mut found_pid);
                    }
                    if found_pid != pid {
                        continue;
                    }
                    let mut rect = windows_sys::Win32::Foundation::RECT::default();
                    unsafe {
                        GetClientRect(window, &mut rect);
                    }
                    let dc = unsafe { GetDC(window) };
                    if dc.is_null() {
                        continue;
                    }
                    let color = unsafe { GetPixel(dc, (rect.right - 12).max(4), 10) };
                    unsafe {
                        ReleaseDC(window, dc);
                    }
                    if color != u32::MAX {
                        return Some([color & 255, (color >> 8) & 255, (color >> 16) & 255]);
                    }
                }
                std::thread::sleep(Duration::from_millis(30));
            }
            None
        });
        unsafe extern "system" fn dismiss(
            _: windows_sys::Win32::Foundation::HWND,
            _: u32,
            _: usize,
            _: u32,
        ) {
            unsafe {
                EndMenu();
            }
        }
        unsafe {
            SetTimer(owner.0, 1, 700, Some(dismiss));
            menu.show_context_menu_for_hwnd(
                owner.0 as isize,
                Some(winit::dpi::PhysicalPosition::new(0, 0).into()),
            );
            KillTimer(owner.0, 1);
        }
        let color = probe
            .join()
            .unwrap()
            .expect("Own native menu window must appear");
        eprintln!("Native context menu background RGB={color:?}; menu dismissed without selecting an action.");
        assert!(
            color.iter().all(|value| *value < 128),
            "Expected a dark native menu background"
        );
    }
}
