use crate::reg::*;
use crate::theme::{ctl_color, ctl_color_edit, ui_bg_brush};
use crate::win::*;
use crate::settings::apply_topmost;
use std::cell::RefCell;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::FillRect;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const DLG_TYPE_NAMES: [&str; 6] = ["String", "ExpandString", "Binary", "DWord", "QWord", "MultiString"];
pub const DLG_TYPE_VALS: [u32; 6] = [REG_T_SZ, REG_T_EXPAND_SZ, REG_T_BINARY, REG_T_DWORD, REG_T_QWORD, REG_T_MULTI_SZ];

pub fn type_index(t: u32) -> usize {
    DLG_TYPE_VALS.iter().position(|&v| v == t).unwrap_or(0)
}

const ID_OK: i32 = 1;
const ID_CANCEL: i32 = 2;
const ID_NAME: i32 = 20;
const ID_TYPE: i32 = 21;
const ID_VALUE: i32 = 22;

const CBS_DROPDOWNLIST: u32 = 0x0003;
const CB_ADDSTRING: u32 = 0x0143;
const CB_SETCURSEL: u32 = 0x014E;
const CB_GETCURSEL: u32 = 0x0147;
const WS_POPUP: u32 = 0x80000000;
const WS_CAPTION: u32 = 0x00C00000;
const WS_SYSMENU: u32 = 0x00080000;
const BS_DEFPUSHBUTTON: u32 = 0x0001;
const EM_SETSEL: u32 = 0x00B1;
const EM_SETREADONLY: u32 = 0x00CF;

#[derive(Default, Clone)]
pub struct EditCfg {
    pub title: String,
    pub prompt: String,
    pub show_name: bool,
    pub show_type: bool,
    pub multiline: bool,
    pub name: String,
    pub typ: u32,
    pub value: String,
}

#[derive(Default)]
struct DlgState {
    cfg: EditCfg,
    hwnd: isize,
    h_name: isize,
    h_type: isize,
    h_value: isize,
    client_w: i32,
    client_h: i32,
    done: bool,
    ok: bool,
    r_name: String,
    r_typ: u32,
    r_value: String,
}

thread_local! {
    static DLG: RefCell<DlgState> = RefCell::new(DlgState::default());
}

pub fn input_box(prompt: &str, title: &str) -> String {
    let (ok, _, _, v) = run_edit_dialog(EditCfg {
        title: title.into(),
        prompt: prompt.into(),
        ..Default::default()
    });
    if ok {
        v.trim().to_string()
    } else {
        String::new()
    }
}

pub fn run_edit_dialog(cfg: EditCfg) -> (bool, String, u32, String) {
    let parent = {
        let p = HAUTO_WIN.load(std::sync::atomic::Ordering::Relaxed) as HWND;
        if p.is_null() {
            main_hwnd()
        } else {
            p
        }
    };

    let (cw, ch) = if cfg.show_name || cfg.show_type {
        (564, 300)
    } else {
        (520, 150)
    };

    DLG.with(|d| {
        let mut d = d.borrow_mut();
        d.r_name = cfg.name.clone();
        d.r_typ = cfg.typ;
        d.r_value = cfg.value.clone();
        d.ok = false;
        d.done = false;
        d.client_w = cw;
        d.client_h = ch;
        d.cfg = cfg;
    });

    register_class("ASEditDialog", Some(edit_proc));

    let win_w = cw + 16;
    let win_h = ch + 39;
    let (x, y) = center_over(parent, win_w, win_h);

    let title = DLG.with(|d| d.borrow().cfg.title.clone());
    let cls = wide("ASEditDialog");
    let t = wide(&title);
    let hwnd = unsafe {
        CreateWindowExW(
            0, cls.as_ptr(), t.as_ptr(),
            WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
            x, y, win_w, win_h, parent, std::ptr::null_mut(), hinst(), std::ptr::null(),
        )
    };
    DLG.with(|d| d.borrow_mut().hwnd = hwnd as isize);
    apply_topmost(hwnd);

    if !parent.is_null() {
        unsafe { EnableWindow(parent, 0) };
    }
    unsafe { SetForegroundWindow(hwnd) };

    let mut m: MSG = unsafe { std::mem::zeroed() };
    loop {
        if DLG.with(|d| d.borrow().done) {
            break;
        }
        let r = unsafe { GetMessageW(&mut m, std::ptr::null_mut(), 0, 0) };
        if r <= 0 {
            unsafe { PostQuitMessage(0) };
            break;
        }
        let handled = unsafe { IsDialogMessageW(hwnd, &mut m) };
        if handled == 0 {
            unsafe {
                TranslateMessage(&m);
                DispatchMessageW(&m);
            }
        }
    }

    if !parent.is_null() {
        unsafe {
            EnableWindow(parent, 1);
            SetForegroundWindow(parent);
        }
    }

    DLG.with(|d| {
        let d = d.borrow();
        (d.ok, d.r_name.clone(), d.r_typ, d.r_value.clone())
    })
}

fn center_over(parent: HWND, w: i32, h: i32) -> (i32, i32) {
    if !parent.is_null() {
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        if unsafe { GetWindowRect(parent, &mut r) } != 0 {
            let x = (r.left + (r.right - r.left - w) / 2).max(0);
            let y = (r.top + (r.bottom - r.top - h) / 2).max(0);
            return (x, y);
        }
    }
    (200, 160)
}

unsafe extern "system" fn edit_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_ERASEBKGND => {
            let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            GetClientRect(hwnd, &mut r);
            FillRect(wparam as _, &r, ui_bg_brush() as _);
            1
        }
        WM_CTLCOLOREDIT => ctl_color_edit(wparam as isize),
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLORLISTBOX => ctl_color(wparam as isize),
        WM_CREATE => {
            apply_window_chrome(hwnd, false, 255);
            build_edit_controls(hwnd);
            0
        }
        WM_COMMAND => {
            let id = (wparam & 0xFFFF) as i32;
            if id == ID_OK {
                close_dialog(true);
            } else if id == ID_CANCEL {
                close_dialog(false);
            }
            0
        }
        WM_CLOSE => {
            close_dialog(false);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn build_edit_controls(parent: HWND) {
    let cfg = DLG.with(|d| d.borrow().cfg.clone());
    let cw = DLG.with(|d| d.borrow().client_w);
    let ch = DLG.with(|d| d.borrow().client_h);

    let mk_label = |text: &str, x: i32, y: i32, w: i32, h: i32| {
        let cls = wide("STATIC");
        let t = wide(text);
        let hl = CreateWindowExW(0, cls.as_ptr(), t.as_ptr(), WS_CHILD | WS_VISIBLE, x, y, w, h, parent, std::ptr::null_mut(), hinst(), std::ptr::null());
        set_font(hl);
    };
    let mk_edit = |text: &str, x: i32, y: i32, w: i32, h: i32, multi: bool, id: i32| -> HWND {
        let style = if multi {
            WS_CHILD | WS_VISIBLE | WS_BORDER | WS_TABSTOP | EDIT_MULTILINE | EDIT_AUTOVSCROLL | WS_VSCROLL
        } else {
            WS_CHILD | WS_VISIBLE | WS_BORDER | WS_TABSTOP | EDIT_AUTOHSCROLL
        };
        let cls = wide("EDIT");
        let t = wide(text);
        let he = CreateWindowExW(0, cls.as_ptr(), t.as_ptr(), style, x, y, w, h, parent, id as _, hinst(), std::ptr::null());
        set_font(he);
        he
    };

    let mut h_name = 0isize;
    let mut h_type = 0isize;
    let h_value;

    if cfg.show_name || cfg.show_type {
        mk_label("Имя параметра:", 16, 12, 300, 18);
        let hn = mk_edit(&cfg.name, 16, 32, cw - 32, 24, false, ID_NAME);
        if !cfg.show_name {
            SendMessageW(hn, EM_SETREADONLY, 1, 0);
        }
        h_name = hn as isize;

        mk_label("Тип:", 16, 64, 300, 18);
        let cls = wide("COMBOBOX");
        let empty = wide("");
        let ht = CreateWindowExW(
            0, cls.as_ptr(), empty.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | CBS_DROPDOWNLIST | WS_VSCROLL,
            16, 84, 260, 220, parent, ID_TYPE as _, hinst(), std::ptr::null(),
        );
        set_font(ht);
        for n in DLG_TYPE_NAMES {
            let s = wide(n);
            SendMessageW(ht, CB_ADDSTRING, 0, s.as_ptr() as LPARAM);
        }
        SendMessageW(ht, CB_SETCURSEL, type_index(cfg.typ), 0);
        h_type = ht as isize;

        mk_label("Значение:", 16, 118, 300, 18);
        h_value = mk_edit(&cfg.value, 16, 138, cw - 32, 96, true, ID_VALUE);
    } else {
        mk_label(&cfg.prompt, 16, 12, cw - 32, 40);
        h_value = mk_edit(&cfg.value, 16, 56, cw - 32, 26, false, ID_VALUE);
    }

    let btn_y = ch - 44;
    let cls = wide("BUTTON");
    let ok_t = wide("Применить");
    let hok = CreateWindowExW(
        0, cls.as_ptr(), ok_t.as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_DEFPUSHBUTTON,
        cw - 252 - 16, btn_y, 120, 32, parent, ID_OK as _, hinst(), std::ptr::null(),
    );
    set_font(hok);
    let cn_t = wide("Отмена");
    let hcn = CreateWindowExW(
        0, cls.as_ptr(), cn_t.as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        cw - 120 - 16, btn_y, 120, 32, parent, ID_CANCEL as _, hinst(), std::ptr::null(),
    );
    set_font(hcn);

    SendMessageW(h_value, EM_SETSEL, 0, -1);

    DLG.with(|d| {
        let mut d = d.borrow_mut();
        d.h_name = h_name;
        d.h_type = h_type;
        d.h_value = h_value as isize;
    });

    let hwnd = DLG.with(|d| d.borrow().hwnd) as HWND;
    SetForegroundWindow(hwnd);
}

fn close_dialog(ok: bool) {
    let (hwnd, h_name, h_type, h_value, show_name, show_type) = DLG.with(|d| {
        let d = d.borrow();
        (d.hwnd as HWND, d.h_name as HWND, d.h_type as HWND, d.h_value as HWND, d.cfg.show_name, d.cfg.show_type)
    });
    if ok {
        let name = if !h_name.is_null() && show_name {
            get_text(h_name)
        } else {
            DLG.with(|d| d.borrow().cfg.name.clone())
        };
        let typ = if !h_type.is_null() && show_type {
            let sel = unsafe { SendMessageW(h_type, CB_GETCURSEL, 0, 0) } as usize;
            if sel < DLG_TYPE_VALS.len() {
                DLG_TYPE_VALS[sel]
            } else {
                DLG.with(|d| d.borrow().cfg.typ)
            }
        } else {
            DLG.with(|d| d.borrow().cfg.typ)
        };
        let value = if !h_value.is_null() {
            get_text(h_value)
        } else {
            String::new()
        };
        DLG.with(|d| {
            let mut d = d.borrow_mut();
            d.ok = true;
            d.r_name = name;
            d.r_typ = typ;
            d.r_value = value;
        });
    }
    unsafe { DestroyWindow(hwnd) };
    DLG.with(|d| {
        let mut d = d.borrow_mut();
        d.hwnd = 0;
        d.h_name = 0;
        d.h_type = 0;
        d.h_value = 0;
        d.done = true;
    });
}
