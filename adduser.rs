// Диалог «Добавить пользователя»: имя, пароль, выбор обычный / администратор.
use crate::settings::apply_topmost;
use crate::theme::{ctl_color, ctl_color_edit, ui_bg_brush};
use crate::win::*;
use std::cell::RefCell;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::FillRect;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const ID_OK: i32 = 1;
const ID_CANCEL: i32 = 2;
const ID_NAME: i32 = 20;
const ID_PASS: i32 = 21;
const ID_R_USER: i32 = 30;
const ID_R_ADMIN: i32 = 31;

const WS_POPUP: u32 = 0x8000_0000;
const WS_CAPTION: u32 = 0x00C0_0000;
const WS_SYSMENU: u32 = 0x0008_0000;
const WS_GROUP: u32 = 0x0002_0000;
const BS_DEFPUSHBUTTON: u32 = 0x0001;
const BS_AUTORADIOBUTTON: u32 = 0x0009;
const BM_GETCHECK: u32 = 0x00F0;
const BM_SETCHECK: u32 = 0x00F1;
const EM_SETSEL: u32 = 0x00B1;

const CW: i32 = 460;
const CH: i32 = 252;

#[derive(Default)]
struct AddUserState {
    hwnd: isize,
    h_name: isize,
    h_pass: isize,
    h_admin: isize,
    done: bool,
    ok: bool,
    r_name: String,
    r_pass: String,
    r_admin: bool,
}

thread_local! {
    static AU: RefCell<AddUserState> = RefCell::new(AddUserState::default());
}

/// Показывает диалог. Возвращает Some((имя, пароль, админ?)) при подтверждении.
pub fn add_user_dialog() -> Option<(String, String, bool)> {
    let parent = {
        let p = HAUTO_WIN.load(std::sync::atomic::Ordering::Relaxed) as HWND;
        if p.is_null() {
            main_hwnd()
        } else {
            p
        }
    };

    AU.with(|d| *d.borrow_mut() = AddUserState::default());
    register_class("ASAddUserDialog", Some(adduser_proc));

    let win_w = CW + 16;
    let win_h = CH + 39;
    let (x, y) = center_over(parent, win_w, win_h);

    let cls = wide("ASAddUserDialog");
    let t = wide("Добавить пользователя");
    let hwnd = unsafe {
        CreateWindowExW(
            0, cls.as_ptr(), t.as_ptr(),
            WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
            x, y, win_w, win_h, parent, std::ptr::null_mut(), hinst(), std::ptr::null(),
        )
    };
    AU.with(|d| d.borrow_mut().hwnd = hwnd as isize);
    apply_topmost(hwnd);

    if !parent.is_null() {
        unsafe { EnableWindow(parent, 0) };
    }
    unsafe { SetForegroundWindow(hwnd) };

    let mut m: MSG = unsafe { std::mem::zeroed() };
    loop {
        if AU.with(|d| d.borrow().done) {
            break;
        }
        let r = unsafe { GetMessageW(&mut m, std::ptr::null_mut(), 0, 0) };
        if r <= 0 {
            unsafe { PostQuitMessage(0) };
            break;
        }
        if unsafe { IsDialogMessageW(hwnd, &mut m) } == 0 {
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

    AU.with(|d| {
        let d = d.borrow();
        if d.ok {
            Some((d.r_name.clone(), d.r_pass.clone(), d.r_admin))
        } else {
            None
        }
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
    (220, 160)
}

unsafe extern "system" fn adduser_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
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
            build_controls(hwnd);
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

unsafe fn build_controls(parent: HWND) {
    let mk_label = |text: &str, x: i32, y: i32, w: i32, h: i32| {
        let cls = wide("STATIC");
        let t = wide(text);
        let hl = CreateWindowExW(0, cls.as_ptr(), t.as_ptr(), WS_CHILD | WS_VISIBLE, x, y, w, h, parent, std::ptr::null_mut(), hinst(), std::ptr::null());
        set_font(hl);
    };
    let mk_edit = |text: &str, x: i32, y: i32, w: i32, h: i32, id: i32| -> HWND {
        let cls = wide("EDIT");
        let t = wide(text);
        let he = CreateWindowExW(
            0, cls.as_ptr(), t.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_BORDER | WS_TABSTOP | EDIT_AUTOHSCROLL,
            x, y, w, h, parent, id as _, hinst(), std::ptr::null(),
        );
        set_font(he);
        he
    };
    let mk_radio = |text: &str, x: i32, y: i32, w: i32, h: i32, id: i32, group: bool| -> HWND {
        let mut style = WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTORADIOBUTTON;
        if group {
            style |= WS_GROUP;
        }
        let cls = wide("BUTTON");
        let t = wide(text);
        let hr = CreateWindowExW(0, cls.as_ptr(), t.as_ptr(), style, x, y, w, h, parent, id as _, hinst(), std::ptr::null());
        set_font(hr);
        hr
    };

    mk_label("Имя пользователя:", 16, 12, CW - 32, 18);
    let h_name = mk_edit("", 16, 32, CW - 32, 26, ID_NAME);

    mk_label("Пароль:", 16, 66, CW - 32, 18);
    let h_pass = mk_edit("", 16, 86, CW - 32, 26, ID_PASS);

    mk_label("Тип учётной записи:", 16, 122, CW - 32, 18);
    let h_user = mk_radio("Обычный пользователь", 16, 144, CW - 32, 24, ID_R_USER, true);
    let h_admin = mk_radio("С правами администратора", 16, 172, CW - 32, 24, ID_R_ADMIN, false);
    SendMessageW(h_user, BM_SETCHECK, 1, 0);

    let btn_y = CH - 44;
    let cls = wide("BUTTON");
    let ok_t = wide("Создать");
    let hok = CreateWindowExW(
        0, cls.as_ptr(), ok_t.as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_GROUP | BS_DEFPUSHBUTTON,
        CW - 252 - 16, btn_y, 120, 32, parent, ID_OK as _, hinst(), std::ptr::null(),
    );
    set_font(hok);
    let cn_t = wide("Отмена");
    let hcn = CreateWindowExW(
        0, cls.as_ptr(), cn_t.as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        CW - 120 - 16, btn_y, 120, 32, parent, ID_CANCEL as _, hinst(), std::ptr::null(),
    );
    set_font(hcn);

    AU.with(|d| {
        let mut d = d.borrow_mut();
        d.h_name = h_name as isize;
        d.h_pass = h_pass as isize;
        d.h_admin = h_admin as isize;
    });

    SetFocus(h_name);
    SendMessageW(h_name, EM_SETSEL, 0, -1);
}

fn close_dialog(ok: bool) {
    let (hwnd, h_name, h_pass, h_admin) = AU.with(|d| {
        let d = d.borrow();
        (d.hwnd as HWND, d.h_name as HWND, d.h_pass as HWND, d.h_admin as HWND)
    });
    if ok {
        let name = get_text(h_name);
        let pass = get_text(h_pass);
        let admin = !h_admin.is_null() && unsafe { SendMessageW(h_admin, BM_GETCHECK, 0, 0) } == 1;
        AU.with(|d| {
            let mut d = d.borrow_mut();
            d.ok = true;
            d.r_name = name;
            d.r_pass = pass;
            d.r_admin = admin;
        });
    }
    unsafe { DestroyWindow(hwnd) };
    AU.with(|d| {
        let mut d = d.borrow_mut();
        d.hwnd = 0;
        d.h_name = 0;
        d.h_pass = 0;
        d.h_admin = 0;
        d.done = true;
    });
}
