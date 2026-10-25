use crate::editdlg::*;
use crate::exec::{exec_args, simple};
use crate::reg::*;
use crate::settings::apply_topmost;
use crate::theme::*;
use crate::win::*;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{LazyLock, Mutex};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::FillRect;
use windows_sys::Win32::System::Registry::HKEY;
use windows_sys::Win32::System::DataExchange::*;
use windows_sys::Win32::System::Memory::*;
use windows_sys::Win32::UI::Controls::*;
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

const NM_RCLICK: u32 = 0xFFFFFFFB;
const NM_CUSTOMDRAW: u32 = 0xFFFFFFF4;
const CDDS_PREPAINT: u32 = 0x00000001;
const CDDS_ITEMPREPAINT: u32 = 0x00010001;
const CDRF_NOTIFYITEMDRAW: isize = 0x00000020;

const TPM_RETURNCMD: u32 = 0x0100;
const TPM_RIGHTBUTTON: u32 = 0x0002;
const MF_STRING: u32 = 0x0000;
const MF_SEPARATOR: u32 = 0x0800;

const ID_TAB_BASE: i32 = 8100;

const AUTO_TABS: [&str; 8] = [
    "Run", "RunOnce", "Winlogon", "AppInit_DLLs", "CmdLine", "BootExecute", "Планировщик", "Сервисы",
];

#[derive(Clone)]
struct AutoEntry {
    name: String,
    value: String,
    location: String,
    kind: i32,
    root: isize,
    subkey: String,
    valname: String,
    ident: String,
}

static H_AUTO_LIST: AtomicIsize = AtomicIsize::new(0);
static TAB_BTNS: LazyLock<Mutex<Vec<isize>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static ENTRIES: LazyLock<Mutex<Vec<AutoEntry>>> = LazyLock::new(|| Mutex::new(Vec::new()));

fn so(dark: u32, light: u32) -> u32 {
    if simple() {
        light
    } else {
        dark
    }
}
fn row_even() -> u32 {
    so(rgb(0x12, 0x22, 0x33), rgb(0xF2, 0xF2, 0xF2))
}
fn row_odd() -> u32 {
    so(rgb(0x0C, 0x19, 0x26), rgb(0xFF, 0xFF, 0xFF))
}
fn row_text() -> u32 {
    so(rgb(0xDC, 0xE8, 0xF4), rgb(0, 0, 0))
}

pub fn open_auto_window() {
    let existing = HAUTO_WIN.load(Ordering::Relaxed) as HWND;
    if !existing.is_null() {
        unsafe {
            ShowWindow(existing, SW_SHOW);
            SetForegroundWindow(existing);
        }
        return;
    }
    register_class("ASAutoWindow", Some(auto_proc));
    let h = create_tool_window("ASAutoWindow", "Автозагрузка — FiReWork", 1000, 620);
    HAUTO_WIN.store(h as isize, Ordering::Relaxed);
    apply_topmost(h);
}

unsafe extern "system" fn auto_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_ERASEBKGND => {
            let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            GetClientRect(hwnd, &mut r);
            FillRect(wparam as _, &r, ui_bg_brush() as _);
            1
        }
        WM_DRAWITEM => {
            draw_button_item(lparam);
            1
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORBTN => ctl_color(wparam as isize),
        WM_CREATE => {
            apply_window_chrome(hwnd, false, 255);
            create_auto_controls(hwnd);
            set_auto_tab(0);
            0
        }
        WM_SIZE => {
            layout_auto(hwnd);
            0
        }
        WM_COMMAND => {
            let id = (wparam & 0xFFFF) as i32;
            if id >= ID_TAB_BASE && id < ID_TAB_BASE + AUTO_TABS.len() as i32 {
                set_auto_tab(id - ID_TAB_BASE);
            }
            0
        }
        WM_NOTIFY => {
            let hdr = &*(lparam as *const NMHDR);
            match hdr.code as u32 {
                NM_RCLICK => {
                    let ia = &*(lparam as *const NMITEMACTIVATE);
                    show_auto_menu(hwnd, ia.iItem);
                    0
                }
                NM_CUSTOMDRAW => auto_custom_draw(lparam),
                _ => 0,
            }
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            HAUTO_WIN.store(0, Ordering::Relaxed);
            H_AUTO_LIST.store(0, Ordering::Relaxed);
            TAB_BTNS.lock().unwrap().clear();
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn create_auto_controls(parent: HWND) {
    let mut btns = TAB_BTNS.lock().unwrap();
    btns.clear();
    let cls = wide("BUTTON");
    for (i, t) in AUTO_TABS.iter().enumerate() {
        let id = ID_TAB_BASE + i as i32;
        BTN_META.lock().unwrap().insert(id, BtnInfo { kind: KIND_TAB, title: t.to_string(), subtitle: String::new(), cat_index: i as i32 });
        let tw = wide(t);
        let hb = CreateWindowExW(
            0, cls.as_ptr(), tw.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BTN_OWNERDRAW,
            0, 0, 100, 30, parent, id as _, hinst(), std::ptr::null(),
        );
        btns.push(hb as isize);
    }
    drop(btns);

    let cls_lv = wide("SysListView32");
    let list = CreateWindowExW(
        0, cls_lv.as_ptr(), std::ptr::null(),
        WS_CHILD | WS_VISIBLE | LVS_REPORT | LVS_SHOWSELALWAYS,
        0, 44, 980, 540, parent, std::ptr::null_mut(), hinst(), std::ptr::null(),
    );
    set_font(list);
    SendMessageW(list, LVM_SETEXTENDEDLISTVIEWSTYLE, 0, (LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER) as LPARAM);
    let dark = wide("DarkMode_Explorer");
    SetWindowTheme(list, dark.as_ptr(), std::ptr::null());
    SendMessageW(list, LVM_SETBKCOLOR, 0, row_odd() as LPARAM);
    SendMessageW(list, LVM_SETTEXTBKCOLOR, 0, row_odd() as LPARAM);
    SendMessageW(list, LVM_SETTEXTCOLOR, 0, row_text() as LPARAM);
    H_AUTO_LIST.store(list as isize, Ordering::Relaxed);

    let cols = [("Имя записи", 240), ("Путь / значение", 450), ("Расположение", 270)];
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

unsafe fn layout_auto(hwnd: HWND) {
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    GetClientRect(hwnd, &mut r);
    let mut x = 10i32;
    for hb in TAB_BTNS.lock().unwrap().iter() {
        MoveWindow(*hb as HWND, x, 8, 112, 30, 1);
        x += 116;
    }
    let list = H_AUTO_LIST.load(Ordering::Relaxed) as HWND;
    if !list.is_null() {
        MoveWindow(list, 0, 46, r.right, r.bottom - 46, 1);
    }
}

unsafe fn auto_custom_draw(lparam: isize) -> LRESULT {
    let cd = &*(lparam as *const NMLVCUSTOMDRAW);
    match cd.nmcd.dwDrawStage {
        CDDS_PREPAINT => CDRF_NOTIFYITEMDRAW,
        CDDS_ITEMPREPAINT => {
            let row = cd.nmcd.dwItemSpec as usize;
            let bk = if row % 2 == 0 { row_even() } else { row_odd() };
            let cd_mut = &mut *(lparam as *mut NMLVCUSTOMDRAW);
            cd_mut.clrText = row_text();
            cd_mut.clrTextBk = bk;
            0
        }
        _ => 0,
    }
}

unsafe fn auto_add_row(list: HWND, row: i32, cells: &[&str]) {
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

fn set_auto_tab(i: i32) {
    AUTO_ACTIVE_TAB.store(i, Ordering::Relaxed);
    for hb in TAB_BTNS.lock().unwrap().iter() {
        invalidate(*hb as HWND);
    }
    let entries = load_auto_tab(i);
    let list = H_AUTO_LIST.load(Ordering::Relaxed) as HWND;
    unsafe { SendMessageW(list, LVM_DELETEALLITEMS, 0, 0) };
    for (r, e) in entries.iter().enumerate() {
        unsafe { auto_add_row(list, r as i32, &[&e.name, &e.value, &e.location]) };
    }
    *ENTRIES.lock().unwrap() = entries;
}

fn load_auto_tab(i: i32) -> Vec<AutoEntry> {
    let mut out = Vec::new();
    let mut add_reg = |root: HKEY, subkey: &str, loc: &str| {
        for kv in reg_list_values(root, subkey) {
            if kv.name.is_empty() {
                continue;
            }
            out.push(AutoEntry {
                name: kv.name.clone(),
                value: kv.value,
                location: loc.to_string(),
                kind: 0,
                root: root as isize,
                subkey: subkey.to_string(),
                valname: kv.name,
                ident: String::new(),
            });
        }
    };
    let run_k = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    let wl = "Software\\Microsoft\\Windows NT\\CurrentVersion\\Winlogon";
    let win = "Software\\Microsoft\\Windows NT\\CurrentVersion\\Windows";
    match i {
        0 => {
            add_reg(hkcu(), run_k, "HKCU\\...\\CurrentVersion\\Run");
            add_reg(hklm(), run_k, "HKLM\\...\\CurrentVersion\\Run");
            add_reg(hklm(), "Software\\Wow6432Node\\Microsoft\\Windows\\CurrentVersion\\Run", "HKLM\\Wow6432Node\\...\\Run");
        }
        1 => {
            add_reg(hkcu(), &format!("{run_k}Once"), "HKCU\\...\\RunOnce");
            add_reg(hklm(), &format!("{run_k}Once"), "HKLM\\...\\RunOnce");
        }
        2 => {
            add_reg(hklm(), wl, "HKLM\\...\\Winlogon");
            add_reg(hkcu(), wl, "HKCU\\...\\Winlogon");
        }
        3 => {
            add_reg(hklm(), win, "HKLM\\...\\Windows");
            add_reg(hklm(), "Software\\Wow6432Node\\Microsoft\\Windows NT\\CurrentVersion\\Windows", "HKLM\\Wow6432Node\\...\\Windows");
        }
        4 => {
            add_reg(hklm(), "SYSTEM\\Setup", "HKLM\\SYSTEM\\Setup");
        }
        5 => {
            if let Some(v) = reg_get_string(hklm(), "SYSTEM\\CurrentControlSet\\Control\\Session Manager", "BootExecute") {
                out.push(AutoEntry {
                    name: "BootExecute".into(),
                    value: v,
                    location: "HKLM\\...\\Session Manager".into(),
                    kind: 0,
                    root: hklm() as isize,
                    subkey: "SYSTEM\\CurrentControlSet\\Control\\Session Manager".into(),
                    valname: "BootExecute".into(),
                    ident: String::new(),
                });
            }
        }
        6 => out.extend(load_tasks()),
        7 => out.extend(load_services()),
        _ => {}
    }
    out
}

fn load_tasks() -> Vec<AutoEntry> {
    let mut out = Vec::new();
    let txt = exec_args("schtasks", &["/query", "/fo", "LIST", "/v"]);
    let mut name = String::new();
    for line in txt.split('\n') {
        let line = line.trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix("TaskName:") {
            name = rest.trim().to_string();
        } else if line.starts_with("Task To Run:") || line.starts_with("Запускаемая задача:") {
            if let Some(idx) = line.find(':') {
                let run = line[idx + 1..].trim().to_string();
                if !name.is_empty() {
                    out.push(AutoEntry {
                        name: name.clone(),
                        value: run,
                        location: "Планировщик задач".into(),
                        kind: 1,
                        root: 0,
                        subkey: String::new(),
                        valname: String::new(),
                        ident: name.clone(),
                    });
                    name.clear();
                }
            }
        }
    }
    out
}

fn load_services() -> Vec<AutoEntry> {
    let mut out = Vec::new();
    let txt = exec_args(
        "powershell",
        &["-NoProfile", "-Command",
          "Get-CimInstance Win32_Service | Where-Object {$_.StartMode -eq 'Auto'} | ForEach-Object { $_.Name + '|' + $_.State + '|' + $_.PathName }"],
    );
    for line in txt.split('\n') {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let p: Vec<&str> = line.splitn(3, '|').collect();
        if p.len() < 3 {
            continue;
        }
        out.push(AutoEntry {
            name: p[0].to_string(),
            value: format!("{} — {}", p[1], p[2]),
            location: "Служба (Auto)".into(),
            kind: 2,
            root: 0,
            subkey: String::new(),
            valname: String::new(),
            ident: p[0].to_string(),
        });
    }
    out
}

unsafe fn show_auto_menu(hwnd: HWND, i_item: i32) {
    let e = {
        let entries = ENTRIES.lock().unwrap();
        if i_item < 0 || i_item as usize >= entries.len() {
            return;
        }
        entries[i_item as usize].clone()
    };
    let menu = CreatePopupMenu();
    let i1 = wide("Заменить значение");
    AppendMenuW(menu, MF_STRING, 1, i1.as_ptr());
    let i2 = wide("Удалить");
    AppendMenuW(menu, MF_STRING, 2, i2.as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
    let i3 = wide("Копировать значение");
    AppendMenuW(menu, MF_STRING, 3, i3.as_ptr());

    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, std::ptr::null());
    DestroyMenu(menu);

    match cmd {
        1 => auto_replace(&e),
        2 => auto_delete(&e),
        3 => copy_to_clipboard(&e.value),
        _ => {}
    }
    if cmd == 1 || cmd == 2 {
        set_auto_tab(AUTO_ACTIVE_TAB.load(Ordering::Relaxed));
    }
}

fn auto_replace(e: &AutoEntry) {
    if e.kind != 0 {
        msg_box("Изменение значения доступно только для записей реестра.\nДля задач и служб используйте «Удалить» / «Отключить».");
        return;
    }
    let root = e.root as HKEY;
    let (data, typ) = reg_get_raw(root, &e.subkey, &e.valname).unwrap_or((Vec::new(), REG_T_SZ));
    let cur = reg_display_for_edit(&data, typ);

    let (ok, new_name, new_type, new_val) = run_edit_dialog(EditCfg {
        title: "Изменение записи реестра".into(),
        show_name: true,
        show_type: true,
        multiline: true,
        name: e.valname.clone(),
        typ,
        value: cur,
        ..Default::default()
    });
    if !ok {
        return;
    }
    let new_name = new_name.trim().to_string();
    if new_name.is_empty() {
        msg_box("Имя параметра не может быть пустым.");
        return;
    }
    let (wrote, errmsg) = reg_set_typed(root, &e.subkey, &new_name, new_type, &new_val);
    if !errmsg.is_empty() {
        msg_box(&errmsg);
        return;
    }
    if !wrote {
        msg_box("Не удалось записать значение (возможно, ключ защищён системой).");
        return;
    }
    if new_name != e.valname {
        reg_del_value(root, &e.subkey, &e.valname);
    }
    let (rd, rtyp) = reg_get_raw(root, &e.subkey, &new_name).unwrap_or((Vec::new(), typ));
    msg_box(&format!(
        "Готово.\n\nПараметр: {}\nТип: {}\nЗначение: {}",
        new_name,
        DLG_TYPE_NAMES[type_index(rtyp)],
        reg_display_for_edit(&rd, rtyp)
    ));
}

fn auto_delete(e: &AutoEntry) {
    match e.kind {
        0 => {
            if reg_del_value(e.root as HKEY, &e.subkey, &e.valname) {
                msg_box("Запись удалена из автозагрузки.");
            } else {
                msg_box("Не удалось удалить запись.");
            }
        }
        1 => {
            exec_args("schtasks", &["/delete", "/tn", &e.ident, "/f"]);
            msg_box("Задача планировщика удалена.");
        }
        2 => {
            exec_args("sc", &["config", &e.ident, "start=", "disabled"]);
            msg_box("Автозапуск службы отключён (служба не удалена).");
        }
        _ => {}
    }
}

fn copy_to_clipboard(s: &str) {
    if s.is_empty() {
        return;
    }
    let u = wide(s);
    let size = (u.len() * 2) as usize;
    const GMEM_MOVEABLE: u32 = 0x0002;
    unsafe {
        let h = GlobalAlloc(GMEM_MOVEABLE, size);
        if h.is_null() {
            return;
        }
        let p = GlobalLock(h);
        if !p.is_null() {
            std::ptr::copy_nonoverlapping(u.as_ptr(), p as *mut u16, u.len());
            GlobalUnlock(h);
        }
        if OpenClipboard(std::ptr::null_mut()) != 0 {
            EmptyClipboard();
            const CF_UNICODETEXT: u32 = 13;
            SetClipboardData(CF_UNICODETEXT, h as HANDLE);
            CloseClipboard();
        }
    }
}
