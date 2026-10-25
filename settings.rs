use crate::procwin::refresh_proc_colors;
use crate::theme::{ctl_color, ui_bg_brush};
use crate::win::*;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::FillRect;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOSIZE: u32 = 0x0001;
const BS_AUTOCHECKBOX: u32 = 0x0003;
const BM_GETCHECK: u32 = 0x00F0;
const BM_SETCHECK: u32 = 0x00F1;
const ID_CHK_TOP: i32 = 9001;
const ID_CHK_SIMPLE: i32 = 9002;

static H_CHK_TOP: AtomicIsize = AtomicIsize::new(0);
static H_CHK_SIMPLE: AtomicIsize = AtomicIsize::new(0);

pub fn apply_topmost(hwnd: HWND) {
    if hwnd.is_null() {
        return;
    }
    let after = if ALWAYS_ON_TOP.load(Ordering::Relaxed) {
        -1isize as HWND
    } else {
        -2isize as HWND
    };
    unsafe { SetWindowPos(hwnd, after, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE) };
}

pub fn apply_topmost_all() {
    apply_topmost(main_hwnd());
    apply_topmost(HPROC_WIN.load(Ordering::Relaxed) as HWND);
    apply_topmost(HAUTO_WIN.load(Ordering::Relaxed) as HWND);
    apply_topmost(HSETTINGS_WIN.load(Ordering::Relaxed) as HWND);
    let wins: Vec<usize> = OUT_WINS.lock().unwrap().keys().copied().collect();
    for h in wins {
        apply_topmost(h as HWND);
    }
}

fn redraw(h: HWND) {
    if !h.is_null() {
        const RDW_ALL: u32 = 0x0001 | 0x0004 | 0x0080;
        unsafe { windows_sys::Win32::Graphics::Gdi::RedrawWindow(h, std::ptr::null(), std::ptr::null_mut(), RDW_ALL) };
    }
}

pub fn apply_simple_mode() {
    redraw(main_hwnd());
    redraw(HSETTINGS_WIN.load(Ordering::Relaxed) as HWND);
    redraw(HAUTO_WIN.load(Ordering::Relaxed) as HWND);
    let hp = HPROC_WIN.load(Ordering::Relaxed) as HWND;
    if !hp.is_null() {
        refresh_proc_colors();
        redraw(hp);
    }
    let wins: Vec<usize> = OUT_WINS.lock().unwrap().keys().copied().collect();
    for h in wins {
        redraw(h as HWND);
    }
}

pub fn open_settings_window() {
    let existing = HSETTINGS_WIN.load(Ordering::Relaxed) as HWND;
    if !existing.is_null() {
        unsafe {
            ShowWindow(existing, SW_SHOW);
            SetForegroundWindow(existing);
        }
        return;
    }
    register_class("ASSettings", Some(settings_proc));
    let h = create_tool_window("ASSettings", "Настройки", 460, 250);
    HSETTINGS_WIN.store(h as isize, Ordering::Relaxed);
    apply_topmost(h);
}

unsafe extern "system" fn settings_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_ERASEBKGND => {
            let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            GetClientRect(hwnd, &mut r);
            FillRect(wparam as _, &r, ui_bg_brush() as _);
            1
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORBTN => ctl_color(wparam as isize),
        WM_CREATE => {
            apply_window_chrome(hwnd, true, 244);
            label(hwnd, "Поведение окна", 18, 16, 400, 20);
            let t = wide("Поверх всех окон");
            let cls = wide("BUTTON");
            let chk_top = CreateWindowExW(
                0, cls.as_ptr(), t.as_ptr(),
                WS_CHILD | WS_VISIBLE | BS_AUTOCHECKBOX,
                22, 46, 400, 26, hwnd, ID_CHK_TOP as _, hinst(), std::ptr::null(),
            );
            set_font(chk_top);
            if ALWAYS_ON_TOP.load(Ordering::Relaxed) {
                SendMessageW(chk_top, BM_SETCHECK, 1, 0);
            }
            H_CHK_TOP.store(chk_top as isize, Ordering::Relaxed);

            let t2 = wide("Упрощённый режим (белый фон) — для WinRE");
            let chk_simple = CreateWindowExW(
                0, cls.as_ptr(), t2.as_ptr(),
                WS_CHILD | WS_VISIBLE | BS_AUTOCHECKBOX,
                22, 84, 400, 26, hwnd, ID_CHK_SIMPLE as _, hinst(), std::ptr::null(),
            );
            set_font(chk_simple);
            if SIMPLE_MODE.load(Ordering::Relaxed) {
                SendMessageW(chk_simple, BM_SETCHECK, 1, 0);
            }
            H_CHK_SIMPLE.store(chk_simple as isize, Ordering::Relaxed);

            label(hwnd, "Упрощённый режим убирает тёмную тему: белый фон и", 22, 124, 420, 20);
            label(hwnd, "чёрный текст — надёжнее в среде восстановления.", 22, 146, 420, 20);
            0
        }
        WM_COMMAND => {
            let id = (wparam & 0xFFFF) as i32;
            if id == ID_CHK_TOP {
                let st = SendMessageW(H_CHK_TOP.load(Ordering::Relaxed) as HWND, BM_GETCHECK, 0, 0);
                ALWAYS_ON_TOP.store(st == 1, Ordering::Relaxed);
                apply_topmost_all();
            } else if id == ID_CHK_SIMPLE {
                let st = SendMessageW(H_CHK_SIMPLE.load(Ordering::Relaxed) as HWND, BM_GETCHECK, 0, 0);
                SIMPLE_MODE.store(st == 1, Ordering::Relaxed);
                apply_simple_mode();
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            HSETTINGS_WIN.store(0, Ordering::Relaxed);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}
