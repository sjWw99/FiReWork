use crate::settings::apply_topmost;
use crate::theme::{ctl_color, ctl_color_edit, ui_bg_brush};
use crate::win::*;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Arc;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::FillRect;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub fn request_out_window(title: String, run: RunFn) {
    OUT_REQ.lock().unwrap().push_back(OutReq { title, run });
    post_main(WMAPP_OUT, 0);
}

pub fn create_out_window() {
    let req = match OUT_REQ.lock().unwrap().pop_front() {
        Some(r) => r,
        None => return,
    };
    register_class("ASOutWindow", Some(out_proc));
    let h = create_tool_window("ASOutWindow", &req.title, 760, 500);
    apply_topmost(h);

    let ow = {
        let wins = OUT_WINS.lock().unwrap();
        wins.get(&(h as usize)).cloned()
    };
    let ow = match ow {
        Some(o) => o,
        None => return,
    };
    let run = req.run;
    std::thread::spawn(move || {
        let ow2 = ow.clone();
        let emit = move |s: &str| {
            ow2.queue.lock().unwrap().push(s.to_string());
            unsafe { PostMessageW(ow2.hwnd as HWND, WMAPP_OUTAPPEND, 0, 0) };
        };
        run(&emit);
    });
}

unsafe extern "system" fn out_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_ERASEBKGND => {
            let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            GetClientRect(hwnd, &mut r);
            FillRect(wparam as _, &r, ui_bg_brush() as _);
            1
        }
        WM_CTLCOLOREDIT => ctl_color_edit(wparam as isize),
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => ctl_color(wparam as isize),
        WM_CREATE => {
            apply_window_chrome(hwnd, false, 255);
            let cls = wide("EDIT");
            let hedit = CreateWindowExW(
                0, cls.as_ptr(), std::ptr::null(),
                WS_CHILD | WS_VISIBLE | WS_VSCROLL | EDIT_MULTILINE | EDIT_AUTOVSCROLL | EDIT_READONLY,
                0, 0, 700, 440, hwnd, std::ptr::null_mut(), hinst(), std::ptr::null(),
            );
            set_font(hedit);
            let ow = Arc::new(OutWin {
                hwnd: hwnd as usize,
                hedit: AtomicIsize::new(hedit as isize),
                queue: std::sync::Mutex::new(Vec::new()),
            });
            OUT_WINS.lock().unwrap().insert(hwnd as usize, ow);
            0
        }
        WM_SIZE => {
            if let Some(ow) = OUT_WINS.lock().unwrap().get(&(hwnd as usize)) {
                let hedit = ow.hedit.load(Ordering::Relaxed) as HWND;
                if !hedit.is_null() {
                    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                    GetClientRect(hwnd, &mut r);
                    MoveWindow(hedit, 0, 0, r.right, r.bottom, 1);
                }
            }
            0
        }
        x if x == WMAPP_OUTAPPEND => {
            let (hedit, items) = {
                let wins = OUT_WINS.lock().unwrap();
                match wins.get(&(hwnd as usize)) {
                    Some(ow) => {
                        let mut q = ow.queue.lock().unwrap();
                        let items: Vec<String> = q.drain(..).collect();
                        (ow.hedit.load(Ordering::Relaxed) as HWND, items)
                    }
                    None => return 0,
                }
            };
            for s in items {
                append_edit(hedit, &s);
            }
            0
        }
        WM_CLOSE => {
            OUT_WINS.lock().unwrap().remove(&(hwnd as usize));
            DestroyWindow(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}
