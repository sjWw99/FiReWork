#![windows_subsystem = "windows"]
#![allow(non_snake_case, non_upper_case_globals, dead_code, clippy::missing_transmute_annotations)]

mod adduser;
mod autostart;
mod editdlg;
mod exec;
mod layout;
mod outwin;
mod procwin;
mod reg;
mod settings;
mod theme;
mod win;

use std::sync::atomic::Ordering;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::UI::Controls::{InitCommonControlsEx, INITCOMMONCONTROLSEX};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::exec::*;
use crate::layout::*;
use crate::settings::{apply_topmost, open_settings_window};
use crate::theme::*;
use crate::win::*;

fn has_elevated_flag() -> bool {
    std::env::args().any(|a| a == "--elevated")
}

fn main() {
    if !has_elevated_flag() && !is_admin() {
        if let Ok(exe) = std::env::current_exe() {
            let verb = wide("runas");
            let file = wide(&exe.to_string_lossy());
            let args = wide("--elevated");
            let r = unsafe {
                ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), args.as_ptr(), std::ptr::null(), SW_SHOW)
            };
            if r as isize > 32 {
                return;
            }
        }
    }

    unsafe {
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: 0x0000FFFF,
        };
        InitCommonControlsEx(&icc);
    }

    enable_debug_privilege();

    HINST.store(module_handle() as isize, Ordering::Relaxed);
    F_UI.store(make_font("Segoe UI", 17, 400) as isize, Ordering::Relaxed);
    F_TITLE.store(make_font("Segoe UI Semibold", 22, 600) as isize, Ordering::Relaxed);
    F_CARD.store(make_font("Segoe UI Semibold", 16, 600) as isize, Ordering::Relaxed);
    F_SUB.store(make_font("Segoe UI", 13, 400) as isize, Ordering::Relaxed);
    F_CAT.store(make_font("Segoe UI Semibold", 15, 600) as isize, Ordering::Relaxed);
    APP_ICON.store(load_app_icon() as isize, Ordering::Relaxed);
    init_theme();

    register_class("ASMainWindow", Some(main_proc));

    let cls = wide("ASMainWindow");
    let title = wide("FiReWork");
    unsafe {
        CreateWindowExW(
            0, cls.as_ptr(), title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT, CW_USEDEFAULT, 1180, 760,
            std::ptr::null_mut(), std::ptr::null_mut(), hinst(), std::ptr::null(),
        );
    }

    let mut m: MSG = unsafe { std::mem::zeroed() };
    loop {
        let r = unsafe { GetMessageW(&mut m, std::ptr::null_mut(), 0, 0) };
        if r <= 0 {
            break;
        }
        unsafe {
            TranslateMessage(&m);
            DispatchMessageW(&m);
        }
    }
}

fn load_app_icon() -> HICON {
    const IMAGE_ICON: u32 = 1;
    const LR_SHARED: u32 = 0x8000;
    const LR_DEFAULTSIZE: u32 = 0x40;
    unsafe {
        LoadImageW(hinst(), 1 as _, IMAGE_ICON, 0, 0, LR_DEFAULTSIZE | LR_SHARED) as HICON
    }
}

fn drain_log() {
    LOG_QUEUE.lock().unwrap().clear();
}

unsafe extern "system" fn main_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_CREATE => {
            MAIN_HWND.store(hwnd as isize, Ordering::Relaxed);
            build_ui(hwnd);
            apply_window_chrome(hwnd, true, 240);
            apply_topmost(hwnd);
            0
        }
        x if x == WMAPP_LOG => {
            drain_log();
            0
        }
        x if x == WMAPP_PROC => {
            crate::procwin::open_proc_window();
            0
        }
        x if x == WMAPP_OUT => {
            crate::outwin::create_out_window();
            0
        }
        x if x == WMAPP_SETTINGS => {
            open_settings_window();
            0
        }
        x if x == WMAPP_AUTO => {
            crate::autostart::open_auto_window();
            0
        }
        x if x == WMAPP_SETCAT => {
            set_active_category(wparam);
            0
        }
        WM_ERASEBKGND => {
            let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            GetClientRect(hwnd, &mut r);
            paint_background(wparam as _, r);
            1
        }
        WM_PAINT => {
            paint_main(hwnd);
            0
        }
        WM_DRAWITEM => {
            draw_button_item(lparam);
            1
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORBTN => ctl_color(wparam as isize),
        WM_COMMAND => {
            let id = (wparam & 0xFFFF) as i32;
            let act = ACTIONS.lock().unwrap().get(&id).cloned();
            if let Some(a) = act {
                std::thread::spawn(move || a());
                return 0;
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_SIZE => {
            relayout_cards(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}
