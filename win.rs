use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::collections::{HashMap, VecDeque};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const WMAPP_LOG: u32 = WM_APP + 1;
pub const WMAPP_PROC: u32 = WM_APP + 2;
pub const WMAPP_OUT: u32 = WM_APP + 3;
pub const WMAPP_OUTAPPEND: u32 = WM_APP + 4;
pub const WMAPP_SETTINGS: u32 = WM_APP + 5;
pub const WMAPP_SETCAT: u32 = WM_APP + 6;
pub const WMAPP_AUTO: u32 = WM_APP + 7;

pub const BTN_OWNERDRAW: u32 = 0x0000000B;
pub const EDIT_MULTILINE: u32 = 0x0004;
pub const EDIT_AUTOVSCROLL: u32 = 0x0040;
pub const EDIT_READONLY: u32 = 0x0800;
pub const EDIT_AUTOHSCROLL: u32 = 0x0080;

pub type Action = Arc<dyn Fn() + Send + Sync + 'static>;
pub type RunFn = Arc<dyn Fn(&dyn Fn(&str)) + Send + Sync + 'static>;

pub static HINST: AtomicIsize = AtomicIsize::new(0);
pub static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
pub static APP_ICON: AtomicIsize = AtomicIsize::new(0);

pub static F_UI: AtomicIsize = AtomicIsize::new(0);
pub static F_TITLE: AtomicIsize = AtomicIsize::new(0);
pub static F_CARD: AtomicIsize = AtomicIsize::new(0);
pub static F_SUB: AtomicIsize = AtomicIsize::new(0);
pub static F_CAT: AtomicIsize = AtomicIsize::new(0);

pub static BG_BRUSH: AtomicIsize = AtomicIsize::new(0);
pub static WHITE_BRUSH_OBJ: AtomicIsize = AtomicIsize::new(0);

pub static ALWAYS_ON_TOP: AtomicBool = AtomicBool::new(true);
pub static SIMPLE_MODE: AtomicBool = AtomicBool::new(false);

pub static HPROC_WIN: AtomicIsize = AtomicIsize::new(0);
pub static HPROC_LIST: AtomicIsize = AtomicIsize::new(0);
pub static HAUTO_WIN: AtomicIsize = AtomicIsize::new(0);
pub static HSETTINGS_WIN: AtomicIsize = AtomicIsize::new(0);

pub static NEXT_ID: AtomicI32 = AtomicI32::new(1000);
pub static ACTIVE_CAT: AtomicI32 = AtomicI32::new(0);
pub static AUTO_ACTIVE_TAB: AtomicI32 = AtomicI32::new(0);

pub static CLASSES: LazyLock<Mutex<std::collections::HashSet<String>>> = LazyLock::new(|| Mutex::new(std::collections::HashSet::new()));

pub static ACTIONS: LazyLock<Mutex<HashMap<i32, Action>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
pub static BTN_META: LazyLock<Mutex<HashMap<i32, BtnInfo>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub static LOG_QUEUE: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));

pub static OUT_REQ: LazyLock<Mutex<VecDeque<OutReq>>> = LazyLock::new(|| Mutex::new(VecDeque::new()));
pub static OUT_WINS: LazyLock<Mutex<HashMap<usize, Arc<OutWin>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone)]
pub struct BtnInfo {
    pub kind: i32,
    pub title: String,
    pub subtitle: String,
    pub cat_index: i32,
}

pub struct OutReq {
    pub title: String,
    pub run: RunFn,
}

pub struct OutWin {
    pub hwnd: usize,
    pub hedit: AtomicIsize,
    pub queue: Mutex<Vec<String>>,
}

pub fn next_id() -> i32 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

pub fn hinst() -> HINSTANCE {
    HINST.load(Ordering::Relaxed) as HINSTANCE
}
pub fn main_hwnd() -> HWND {
    MAIN_HWND.load(Ordering::Relaxed) as HWND
}
pub fn app_icon() -> HICON {
    APP_ICON.load(Ordering::Relaxed) as HICON
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn from_wide(buf: &[u16]) -> String {
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..n])
}

pub fn module_handle() -> HMODULE {
    unsafe { GetModuleHandleW(std::ptr::null()) }
}

pub fn make_font(name: &str, size: i32, weight: i32) -> HFONT {
    let face = wide(name);
    unsafe {
        CreateFontW(
            size, 0, 0, 0, weight, 0, 0, 0, 1, 0, 0, 5, 0, face.as_ptr(),
        )
    }
}

pub fn set_font(h: HWND) {
    let f = F_UI.load(Ordering::Relaxed);
    if f != 0 {
        unsafe { SendMessageW(h, WM_SETFONT, f as WPARAM, 1) };
    }
}

pub fn set_font_of(h: HWND, f: AtomicIsize) {
    let fv = f.load(Ordering::Relaxed);
    if fv != 0 {
        unsafe { SendMessageW(h, WM_SETFONT, fv as WPARAM, 1) };
    }
}

pub fn post_main(msg: u32, wparam: WPARAM) {
    let h = MAIN_HWND.load(Ordering::Relaxed);
    if h != 0 {
        unsafe { PostMessageW(h as HWND, msg, wparam, 0) };
    }
}

pub fn label(parent: HWND, text: &str, x: i32, y: i32, w: i32, h: i32) {
    let cls = wide("STATIC");
    let t = wide(text);
    let hl = unsafe {
        CreateWindowExW(
            0, cls.as_ptr(), t.as_ptr(),
            WS_CHILD | WS_VISIBLE,
            x, y, w, h, parent, std::ptr::null_mut(), hinst(), std::ptr::null(),
        )
    };
    set_font(hl);
}

pub fn get_text(h: HWND) -> String {
    unsafe {
        let n = GetWindowTextLengthW(h);
        if n <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; (n + 1) as usize];
        GetWindowTextW(h, buf.as_mut_ptr(), n + 1);
        from_wide(&buf)
    }
}

pub fn msg_box(text: &str) {
    let t = wide(text);
    let cap = wide("FiReWork");
    unsafe {
        MessageBoxW(std::ptr::null_mut(), t.as_ptr(), cap.as_ptr(), MB_OK);
    }
}

pub fn invalidate(h: HWND) {
    if !h.is_null() {
        unsafe { InvalidateRect(h, std::ptr::null(), 1) };
    }
}

pub fn apply_window_chrome(hwnd: HWND, layered: bool, alpha: u8) {
    use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWCP_ROUND: u32 = 2;
    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_LAYERED: isize = 0x0008_0000;
    const LWA_ALPHA: u32 = 0x0000_0002;
    unsafe {
        let corner = DWMWCP_ROUND;
        DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &corner as *const _ as *const core::ffi::c_void, 4);
        let dark: u32 = 0;
        DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as *const core::ffi::c_void, 4);
        if layered {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED);
            SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA);
        }
    }
}

pub fn register_class(name: &str, wndproc: WNDPROC) {
    let mut s = CLASSES.lock().unwrap();
    if s.contains(name) {
        return;
    }
    let cls = wide(name);
    unsafe {
        let cursor = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: wndproc,
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst(),
            hIcon: app_icon(),
            hCursor: cursor,
            hbrBackground: BG_BRUSH.load(Ordering::Relaxed) as HBRUSH,
            lpszMenuName: std::ptr::null(),
            lpszClassName: cls.as_ptr(),
            hIconSm: app_icon(),
        };
        RegisterClassExW(&wc);
    }
    s.insert(name.to_string());
}

pub fn create_tool_window(class: &str, title: &str, w: i32, h: i32) -> HWND {
    let c = wide(class);
    let t = wide(title);
    unsafe {
        CreateWindowExW(
            0, c.as_ptr(), t.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT, CW_USEDEFAULT, w, h,
            std::ptr::null_mut(), std::ptr::null_mut(), hinst(), std::ptr::null(),
        )
    }
}

pub fn append_edit(hedit: HWND, text: &str) {
    if hedit.is_null() {
        return;
    }
    const EM_SETSEL: u32 = 0x00B1;
    const EM_REPLACESEL: u32 = 0x00C2;
    const EM_SETREADONLY: u32 = 0x00CF;
    const EM_SCROLLCARET: u32 = 0x00B7;
    let norm = text.replace("\r\n", "\n").replace('\n', "\r\n") + "\r\n";
    let u = wide(&norm);
    unsafe {
        SendMessageW(hedit, EM_SETREADONLY, 0, 0);
        let n = SendMessageW(hedit, WM_GETTEXTLENGTH, 0, 0);
        SendMessageW(hedit, EM_SETSEL, n as WPARAM, n);
        SendMessageW(hedit, EM_REPLACESEL, 0, u.as_ptr() as LPARAM);
        SendMessageW(hedit, EM_SCROLLCARET, 0, 0);
        SendMessageW(hedit, EM_SETREADONLY, 1, 0);
    }
}
