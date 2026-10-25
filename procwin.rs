use crate::exec::simple;
use crate::settings::apply_topmost;
use crate::theme::{ctl_color, ctl_color_edit, rgb, ui_bg_brush};
use crate::win::*;
use std::sync::atomic::Ordering;
use std::sync::{LazyLock, Mutex, OnceLock};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::FillRect;
use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const LVM_FIRST: u32 = 0x1000;
const LVM_INSERTCOLUMNW: u32 = LVM_FIRST + 97;
const LVM_INSERTITEMW: u32 = LVM_FIRST + 77;
const LVM_SETITEMW: u32 = LVM_FIRST + 76;
const LVM_DELETEALLITEMS: u32 = LVM_FIRST + 9;
const LVM_SETEXTENDEDLISTVIEWSTYLE: u32 = LVM_FIRST + 54;
const LVM_SETBKCOLOR: u32 = LVM_FIRST + 1;
const LVM_SETTEXTCOLOR: u32 = LVM_FIRST + 36;
const LVM_SETTEXTBKCOLOR: u32 = LVM_FIRST + 38;

const LVS_REPORT: u32 = 0x0001;
const LVS_SHOWSELALWAYS: u32 = 0x0008;
const LVCF_TEXT: u32 = 0x0004;
const LVCF_WIDTH: u32 = 0x0002;
const LVCF_SUBITEM: u32 = 0x0008;
const LVIF_TEXT: u32 = 0x0001;
const LVS_EX_FULLROWSELECT: u32 = 0x0020;
const LVS_EX_DOUBLEBUFFER: u32 = 0x00010000;
const EM_SETCUEBANNER: u32 = 0x1501;

const NM_RCLICK: u32 = 0xFFFFFFFB;
const NM_CUSTOMDRAW: u32 = 0xFFFFFFF4;
const CDDS_PREPAINT: u32 = 0x00000001;
const CDDS_ITEMPREPAINT: u32 = 0x00010001;
const CDRF_NOTIFYITEMDRAW: isize = 0x00000020;
const CDRF_DODEFAULT: isize = 0x00000000;

const TPM_RETURNCMD: u32 = 0x0100;
const TPM_RIGHTBUTTON: u32 = 0x0002;
const MF_STRING: u32 = 0x0000;
const MF_SEPARATOR: u32 = 0x0800;

const PROC_QUERY_LIMITED: u32 = 0x1000;
const PROC_QUERY_INFO: u32 = 0x0400;
const PROC_VM_READ: u32 = 0x0010;
const PROC_TERMINATE: u32 = 0x0001;
const PROC_SUSPEND_RESUME: u32 = 0x0800;
const PROC_SET_INFO: u32 = 0x0200;
const BREAK_ON_TERMINATION: u32 = 29;

const ID_SEARCH: i32 = 7001;

fn col_row_even() -> u32 {
    so(rgb(0x12, 0x22, 0x33), rgb(0xF2, 0xF2, 0xF2))
}
fn col_row_odd() -> u32 {
    so(rgb(0x0C, 0x19, 0x26), rgb(0xFF, 0xFF, 0xFF))
}
fn col_row_text() -> u32 {
    so(rgb(0xDC, 0xE8, 0xF4), rgb(0x00, 0x00, 0x00))
}
fn col_critical() -> u32 {
    so(rgb(0xE6, 0xC0, 0x6A), rgb(0x96, 0x66, 0x00))
}
fn col_danger() -> u32 {
    so(rgb(0xE8, 0x95, 0x6F), rgb(0xB0, 0x30, 0x00))
}
fn so(dark: u32, light: u32) -> u32 {
    if simple() {
        light
    } else {
        dark
    }
}

#[derive(Clone)]
struct ProcEntry {
    name: String,
    mem: String,
    crit: String,
    path: String,
    pid: u32,
    critical: bool,
    danger: bool,
}

#[derive(Default)]
struct ProcState {
    all: Vec<ProcEntry>,
    pids: Vec<u32>,
    paths: Vec<String>,
    crit: Vec<bool>,
    danger: Vec<bool>,
}

static PS: LazyLock<Mutex<ProcState>> = LazyLock::new(|| Mutex::new(ProcState::default()));
static H_SEARCH: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

type NtProc = unsafe extern "system" fn(HANDLE) -> i32;
type NtSetInfo = unsafe extern "system" fn(HANDLE, u32, *mut core::ffi::c_void, usize) -> i32;

struct NtProcs {
    suspend: Option<NtProc>,
    resume: Option<NtProc>,
    set_info: Option<NtSetInfo>,
}
unsafe impl Send for NtProcs {}
unsafe impl Sync for NtProcs {}

static NT: OnceLock<NtProcs> = OnceLock::new();

fn nt() -> &'static NtProcs {
    NT.get_or_init(|| unsafe {
        let name = wide("ntdll.dll");
        let h = GetModuleHandleW(name.as_ptr());
        let get = |n: &[u8]| GetProcAddress(h, n.as_ptr());
        NtProcs {
            suspend: get(b"NtSuspendProcess\0").map(|p| std::mem::transmute::<_, NtProc>(p)),
            resume: get(b"NtResumeProcess\0").map(|p| std::mem::transmute::<_, NtProc>(p)),
            set_info: get(b"NtSetInformationProcess\0").map(|p| std::mem::transmute::<_, NtSetInfo>(p)),
        }
    })
}

pub fn refresh_proc_colors() {
    let list = HPROC_LIST.load(Ordering::Relaxed) as HWND;
    if list.is_null() {
        return;
    }
    unsafe {
        SendMessageW(list, LVM_SETBKCOLOR, 0, col_row_odd() as LPARAM);
        SendMessageW(list, LVM_SETTEXTBKCOLOR, 0, col_row_odd() as LPARAM);
        SendMessageW(list, LVM_SETTEXTCOLOR, 0, col_row_text() as LPARAM);
    }
}

pub fn open_proc_window() {
    let existing = HPROC_WIN.load(Ordering::Relaxed) as HWND;
    if !existing.is_null() {
        unsafe {
            ShowWindow(existing, SW_SHOW);
            SetForegroundWindow(existing);
        }
        fill_proc_list();
        return;
    }
    register_class("ASProcWindow", Some(proc_proc));
    let h = create_tool_window("ASProcWindow", "Процессы — FiReWork", 920, 620);
    HPROC_WIN.store(h as isize, Ordering::Relaxed);
    apply_topmost(h);
}

unsafe extern "system" fn proc_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_ERASEBKGND => {
            let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            GetClientRect(hwnd, &mut r);
            FillRect(wparam as _, &r, ui_bg_brush() as _);
            1
        }
        WM_CREATE => {
            apply_window_chrome(hwnd, false, 255);
            create_proc_list(hwnd);
            fill_proc_list();
            0
        }
        WM_SIZE => {
            let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            GetClientRect(hwnd, &mut r);
            let search = H_SEARCH.load(Ordering::Relaxed) as HWND;
            if !search.is_null() {
                MoveWindow(search, 10, 8, r.right - 20, 26, 1);
            }
            let list = HPROC_LIST.load(Ordering::Relaxed) as HWND;
            if !list.is_null() {
                MoveWindow(list, 0, 42, r.right, r.bottom - 42, 1);
            }
            0
        }
        WM_CTLCOLOREDIT => ctl_color_edit(wparam as isize),
        WM_CTLCOLORSTATIC => ctl_color(wparam as isize),
        WM_COMMAND => {
            if (wparam >> 16) & 0xFFFF == 0x0300 && (wparam & 0xFFFF) as i32 == ID_SEARCH {
                let q = get_text(H_SEARCH.load(Ordering::Relaxed) as HWND);
                apply_filter(&q);
            }
            0
        }
        WM_NOTIFY => {
            let hdr = &*(lparam as *const NMHDR);
            match hdr.code as u32 {
                NM_RCLICK => {
                    let ia = &*(lparam as *const NMITEMACTIVATE);
                    show_proc_menu(hwnd, ia.iItem);
                    0
                }
                NM_CUSTOMDRAW => proc_custom_draw(lparam),
                _ => 0,
            }
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            HPROC_WIN.store(0, Ordering::Relaxed);
            HPROC_LIST.store(0, Ordering::Relaxed);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn proc_custom_draw(lparam: isize) -> LRESULT {
    let cd = &*(lparam as *const NMLVCUSTOMDRAW);
    match cd.nmcd.dwDrawStage {
        CDDS_PREPAINT => CDRF_NOTIFYITEMDRAW,
        CDDS_ITEMPREPAINT => {
            let row = cd.nmcd.dwItemSpec as usize;
            let bk = if row % 2 == 0 { col_row_even() } else { col_row_odd() };
            let ps = PS.lock().unwrap();
            let txt = if row < ps.danger.len() && ps.danger[row] {
                col_danger()
            } else if row < ps.crit.len() && ps.crit[row] {
                col_critical()
            } else {
                col_row_text()
            };
            drop(ps);
            let cd_mut = &mut *(lparam as *mut NMLVCUSTOMDRAW);
            cd_mut.clrText = txt;
            cd_mut.clrTextBk = bk;
            CDRF_DODEFAULT
        }
        _ => CDRF_DODEFAULT,
    }
}

unsafe fn create_proc_list(parent: HWND) {
    let cls_edit = wide("EDIT");
    let search = CreateWindowExW(
        0, cls_edit.as_ptr(), std::ptr::null(),
        WS_CHILD | WS_VISIBLE | WS_BORDER | EDIT_AUTOHSCROLL,
        10, 8, 320, 26, parent, ID_SEARCH as _, hinst(), std::ptr::null(),
    );
    set_font(search);
    let cue = wide("Поиск процесса по имени или пути…");
    SendMessageW(search, EM_SETCUEBANNER, 1, cue.as_ptr() as LPARAM);
    H_SEARCH.store(search as isize, Ordering::Relaxed);

    let cls_lv = wide("SysListView32");
    let list = CreateWindowExW(
        0, cls_lv.as_ptr(), std::ptr::null(),
        WS_CHILD | WS_VISIBLE | LVS_REPORT | LVS_SHOWSELALWAYS,
        0, 42, 900, 540, parent, std::ptr::null_mut(), hinst(), std::ptr::null(),
    );
    set_font(list);
    SendMessageW(list, LVM_SETEXTENDEDLISTVIEWSTYLE, 0, (LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER) as LPARAM);
    let dark = wide("DarkMode_Explorer");
    SetWindowTheme(list, dark.as_ptr(), std::ptr::null());
    HPROC_LIST.store(list as isize, Ordering::Relaxed);
    refresh_proc_colors();

    let cols = [("Имя", 210), ("PID", 70), ("Память (КБ)", 120), ("Критично", 90), ("Путь", 400)];
    for (i, (text, w)) in cols.iter().enumerate() {
        let mut t = wide(text);
        let col = LVCOLUMNW {
            mask: LVCF_TEXT | LVCF_WIDTH | LVCF_SUBITEM,
            fmt: 0,
            cx: *w,
            pszText: t.as_mut_ptr(),
            cchTextMax: 0,
            iSubItem: i as i32,
            iImage: 0,
            iOrder: 0,
            cxMin: 0,
            cxDefault: 0,
            cxIdeal: 0,
        };
        SendMessageW(list, LVM_INSERTCOLUMNW, i, &col as *const _ as LPARAM);
    }
}

unsafe fn lv_add_row(list: HWND, row: i32, cells: &[&str]) {
    let mut first = wide(cells[0]);
    let it = LVITEMW {
        mask: LVIF_TEXT,
        iItem: row,
        iSubItem: 0,
        state: 0,
        stateMask: 0,
        pszText: first.as_mut_ptr(),
        cchTextMax: 0,
        iImage: 0,
        lParam: 0,
        iIndent: 0,
        iGroupId: 0,
        cColumns: 0,
        puColumns: std::ptr::null_mut(),
        piColFmt: std::ptr::null_mut(),
        iGroup: 0,
    };
    SendMessageW(list, LVM_INSERTITEMW, 0, &it as *const _ as LPARAM);
    for (sub, text) in cells.iter().enumerate().skip(1) {
        let mut t = wide(text);
        let si = LVITEMW {
            mask: LVIF_TEXT,
            iItem: row,
            iSubItem: sub as i32,
            state: 0,
            stateMask: 0,
            pszText: t.as_mut_ptr(),
            cchTextMax: 0,
            iImage: 0,
            lParam: 0,
            iIndent: 0,
            iGroupId: 0,
            cColumns: 0,
            puColumns: std::ptr::null_mut(),
            piColFmt: std::ptr::null_mut(),
            iGroup: 0,
        };
        SendMessageW(list, LVM_SETITEMW, 0, &si as *const _ as LPARAM);
    }
}

fn fmt_kb(bytes: u64) -> String {
    let kb = bytes / 1024;
    let s = kb.to_string();
    let n = s.len();
    if n <= 3 {
        return s;
    }
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (n - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

pub fn fill_proc_list() {
    let list = HPROC_LIST.load(Ordering::Relaxed) as HWND;
    if list.is_null() {
        return;
    }
    let mut entries = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE || snap.is_null() {
            return;
        }
        let mut pe: PROCESSENTRY32W = std::mem::zeroed();
        pe.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut pe);
        while ok != 0 {
            let pid = pe.th32ProcessID;
            let name = from_wide(&pe.szExeFile);
            let (mem, crit, path) = proc_details(pid);
            let low = path.to_lowercase();
            let danger = low.contains("\\temp\\") || low.contains("\\appdata\\") || low.contains("\\downloads\\");
            let critical = crit == "ДА";
            entries.push(ProcEntry { name, mem, crit, path, pid, critical, danger });
            ok = Process32NextW(snap, &mut pe);
        }
        CloseHandle(snap);
    }
    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    PS.lock().unwrap().all = entries;
    let q = get_text(H_SEARCH.load(Ordering::Relaxed) as HWND);
    apply_filter(&q);
}

pub fn apply_filter(query: &str) {
    let list = HPROC_LIST.load(Ordering::Relaxed) as HWND;
    if list.is_null() {
        return;
    }
    let q = query.trim().to_lowercase();
    unsafe { SendMessageW(list, LVM_DELETEALLITEMS, 0, 0) };
    let all = PS.lock().unwrap().all.clone();
    let mut pids = Vec::new();
    let mut paths = Vec::new();
    let mut crit = Vec::new();
    let mut danger = Vec::new();
    let mut row = 0i32;
    for p in &all {
        if !q.is_empty() && !p.name.to_lowercase().contains(&q) && !p.path.to_lowercase().contains(&q) {
            continue;
        }
        unsafe {
            lv_add_row(list, row, &[&p.name, &p.pid.to_string(), &p.mem, &p.crit, &p.path]);
        }
        pids.push(p.pid);
        paths.push(p.path.clone());
        crit.push(p.critical);
        danger.push(p.danger);
        row += 1;
    }
    let mut ps = PS.lock().unwrap();
    ps.pids = pids;
    ps.paths = paths;
    ps.crit = crit;
    ps.danger = danger;
}

fn proc_details(pid: u32) -> (String, String, String) {
    if pid == 0 {
        return (String::new(), "—".into(), "N/A".into());
    }
    unsafe {
        let mut h = OpenProcess(PROC_QUERY_LIMITED | PROC_VM_READ, 0, pid);
        if h.is_null() {
            h = OpenProcess(PROC_QUERY_INFO | PROC_VM_READ, 0, pid);
        }
        if h.is_null() {
            return (String::new(), "—".into(), "нет доступа".into());
        }
        let mut mem = String::new();
        let mut crit = "—".to_string();
        let mut path = "нет доступа".to_string();

        let mut pmc: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        pmc.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if GetProcessMemoryInfo(h, &mut pmc, pmc.cb) != 0 {
            mem = fmt_kb(pmc.WorkingSetSize as u64);
        }
        let mut buf = vec![0u16; 32768];
        let mut sz = buf.len() as u32;
        if QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut sz) != 0 {
            path = String::from_utf16_lossy(&buf[..sz as usize]);
        }
        let mut b = 0i32;
        if IsProcessCritical(h, &mut b) != 0 {
            crit = if b != 0 { "ДА".into() } else { "нет".into() };
        }
        CloseHandle(h);
        (mem, crit, path)
    }
}

unsafe fn show_proc_menu(hwnd: HWND, i_item: i32) {
    let (pid, path) = {
        let ps = PS.lock().unwrap();
        if i_item < 0 || i_item as usize >= ps.pids.len() {
            return;
        }
        let idx = i_item as usize;
        (ps.pids[idx], ps.paths.get(idx).cloned().unwrap_or_default())
    };

    let menu = CreatePopupMenu();
    let items = [
        (1u32, "Завершить"),
        (2, "Заморозить"),
        (3, "Разморозить"),
        (0, ""),
        (4, "Снять критичность"),
        (0, ""),
        (5, "Свойства файла"),
        (6, "Расположение файла"),
    ];
    for (id, text) in items {
        if id == 0 {
            AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        } else {
            let t = wide(text);
            AppendMenuW(menu, MF_STRING, id as usize, t.as_ptr());
        }
    }
    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, std::ptr::null());
    DestroyMenu(menu);

    match cmd {
        1 => proc_terminate(pid),
        2 => proc_suspend(pid, true),
        3 => proc_suspend(pid, false),
        4 => proc_uncritical(pid),
        5 => file_properties(hwnd, &path),
        6 => file_location(&path),
        _ => {}
    }
    if (1..=4).contains(&cmd) {
        fill_proc_list();
    }
}

fn proc_terminate(pid: u32) {
    unsafe {
        let h = OpenProcess(PROC_TERMINATE, 0, pid);
        if h.is_null() {
            msg_box("Не удалось открыть процесс (нет доступа).");
            return;
        }
        if TerminateProcess(h, 1) == 0 {
            msg_box("Не удалось завершить процесс.");
        }
        CloseHandle(h);
    }
}

fn proc_suspend(pid: u32, suspend: bool) {
    unsafe {
        let h = OpenProcess(PROC_SUSPEND_RESUME, 0, pid);
        if h.is_null() {
            msg_box("Нет доступа к процессу.");
            return;
        }
        if suspend {
            if let Some(f) = nt().suspend {
                f(h);
            }
        } else if let Some(f) = nt().resume {
            f(h);
        }
        CloseHandle(h);
    }
}

fn proc_uncritical(pid: u32) {
    unsafe {
        let h = OpenProcess(PROC_SET_INFO, 0, pid);
        if h.is_null() {
            msg_box("Нет доступа к процессу (нужны права и SeDebugPrivilege).");
            return;
        }
        let mut v: u32 = 0;
        if let Some(f) = nt().set_info {
            let r = f(h, BREAK_ON_TERMINATION, &mut v as *mut _ as *mut core::ffi::c_void, std::mem::size_of::<u32>());
            if r != 0 {
                msg_box("Не удалось снять критичность (процесс защищён).");
            }
        }
        CloseHandle(h);
    }
}

fn file_properties(hwnd: HWND, path: &str) {
    if path.is_empty() || path == "нет доступа" || path == "N/A" {
        msg_box("Путь к файлу недоступен.");
        return;
    }
    const SEE_MASK_INVOKEIDLIST: u32 = 0x0000000C;
    unsafe {
        let verb = wide("properties");
        let file = wide(path);
        let mut sei: SHELLEXECUTEINFOW = std::mem::zeroed();
        sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        sei.fMask = SEE_MASK_INVOKEIDLIST;
        sei.hwnd = hwnd;
        sei.lpVerb = verb.as_ptr();
        sei.lpFile = file.as_ptr();
        sei.nShow = SW_SHOW;
        ShellExecuteExW(&mut sei);
    }
}

fn file_location(path: &str) {
    if path.is_empty() || path == "нет доступа" || path == "N/A" {
        msg_box("Путь к файлу недоступен.");
        return;
    }
    unsafe {
        let verb = wide("open");
        let file = wide("explorer.exe");
        let args = wide(&format!("/select,\"{path}\""));
        ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), args.as_ptr(), std::ptr::null(), SW_SHOW);
    }
}
