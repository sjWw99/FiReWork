use crate::adduser::add_user_dialog;
use crate::editdlg::input_box;
use crate::exec::*;
use crate::reg::*;
use crate::theme::*;
use crate::win::*;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::{BeginPaint, CreatePen, DeleteObject, EndPaint, LineTo, MoveToEx, SelectObject, HGDIOBJ, PAINTSTRUCT, PS_SOLID};
use windows_sys::Win32::UI::Shell::SHEmptyRecycleBinW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const SIDEBAR_X: i32 = 16;
const SIDEBAR_W: i32 = 196;
const CAT_X: i32 = 24;
const CAT_W: i32 = 180;
const CAT_H: i32 = 42;
const CAT_TOP: i32 = 132;
const CAT_STEP: i32 = 50;
const AREA_X: i32 = 228;
const CARD_TOP: i32 = 186;
const CARD_H: i32 = 72;
const GAP: i32 = 14;

enum Item {
    Proc,
    Auto,
    Console(String),
    Launch(String, String),
    Run(RunFn),
}

struct Card {
    title: String,
    subtitle: String,
    accent: bool,
    item: Item,
}

struct Cat {
    name: String,
    desc: String,
    cards: Vec<Card>,
}

#[derive(Clone)]
struct CardRef {
    hwnd: isize,
    cat: i32,
    accent: bool,
}

static CARD_REFS: LazyLock<Mutex<Vec<CardRef>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static CAT_BTNS: LazyLock<Mutex<Vec<isize>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static CAT_META: LazyLock<Mutex<Vec<(String, String)>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static H_SETTINGS_BTN: AtomicIsize = AtomicIsize::new(0);

fn run<F: Fn(&dyn Fn(&str)) + Send + Sync + 'static>(f: F) -> Item {
    Item::Run(Arc::new(f))
}

fn card(title: &str, subtitle: &str, item: Item) -> Card {
    Card { title: title.into(), subtitle: subtitle.into(), accent: false, item }
}
fn accent(title: &str, subtitle: &str, item: Item) -> Card {
    Card { title: title.into(), subtitle: subtitle.into(), accent: true, item }
}

pub fn build_ui(hwnd: HWND) {
    let cats = define_categories();
    {
        let mut m = CAT_META.lock().unwrap();
        *m = cats.iter().map(|c| (c.name.clone(), c.desc.clone())).collect();
    }

    let id = next_id();
    BTN_META.lock().unwrap().insert(id, BtnInfo { kind: KIND_SIMPLE, title: "⚙  Настройки".into(), subtitle: String::new(), cat_index: 0 });
    ACTIONS.lock().unwrap().insert(id, Arc::new(|| post_main(WMAPP_SETTINGS, 0)));
    let cls = wide("BUTTON");
    let t = wide("Настройки");
    let hb = unsafe {
        CreateWindowExW(0, cls.as_ptr(), t.as_ptr(), WS_CHILD | WS_VISIBLE | WS_TABSTOP | BTN_OWNERDRAW, 0, 0, 140, 32, hwnd, id as _, hinst(), std::ptr::null())
    };
    H_SETTINGS_BTN.store(hb as isize, Ordering::Relaxed);

    for (i, c) in cats.iter().enumerate() {
        let idx = i as i32;
        let id = next_id();
        BTN_META.lock().unwrap().insert(id, BtnInfo { kind: KIND_CATEGORY, title: c.name.clone(), subtitle: String::new(), cat_index: idx });
        ACTIONS.lock().unwrap().insert(id, Arc::new(move || post_main(WMAPP_SETCAT, idx as usize)));
        let cy = CAT_TOP + i as i32 * CAT_STEP;
        let tw = wide(&c.name);
        let hb = unsafe {
            CreateWindowExW(0, cls.as_ptr(), tw.as_ptr(), WS_CHILD | WS_VISIBLE | WS_TABSTOP | BTN_OWNERDRAW, CAT_X, cy, CAT_W, CAT_H, hwnd, id as _, hinst(), std::ptr::null())
        };
        CAT_BTNS.lock().unwrap().push(hb as isize);
    }

    for (ci, c) in cats.into_iter().enumerate() {
        for card in c.cards {
            let id = next_id();
            let kind = if card.accent { KIND_ACCENT } else { KIND_CARD };
            BTN_META.lock().unwrap().insert(id, BtnInfo { kind, title: card.title.clone(), subtitle: card.subtitle.clone(), cat_index: ci as i32 });
            let title = card.title.clone();
            let action: Action = match card.item {
                Item::Proc => Arc::new(|| post_main(WMAPP_PROC, 0)),
                Item::Auto => Arc::new(|| post_main(WMAPP_AUTO, 0)),
                Item::Console(cmd) => Arc::new(move || launch("cmd.exe", &format!("/k {cmd}"))),
                Item::Launch(f, a) => Arc::new(move || launch(&f, &a)),
                Item::Run(r) => {
                    let r = r.clone();
                    Arc::new(move || crate::outwin::request_out_window(title.clone(), r.clone()))
                }
            };
            ACTIONS.lock().unwrap().insert(id, action);
            let tw = wide(&card.title);
            let hb = unsafe {
                CreateWindowExW(0, cls.as_ptr(), tw.as_ptr(), WS_CHILD | BTN_OWNERDRAW, 0, 0, 200, CARD_H, hwnd, id as _, hinst(), std::ptr::null())
            };
            CARD_REFS.lock().unwrap().push(CardRef { hwnd: hb as isize, cat: ci as i32, accent: card.accent });
        }
    }

    ACTIVE_CAT.store(0, Ordering::Relaxed);
    relayout_cards(hwnd);
}

pub fn relayout_cards(hwnd: HWND) {
    let refs = CARD_REFS.lock().unwrap().clone();
    if refs.is_empty() {
        return;
    }
    let mut rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetClientRect(hwnd, &mut rc) };

    let sb = H_SETTINGS_BTN.load(Ordering::Relaxed) as HWND;
    if !sb.is_null() {
        unsafe { MoveWindow(sb, rc.right - 164, 34, 140, 32, 1) };
    }

    let area_w = (rc.right - AREA_X - 24).max(300);
    let mut cols = area_w / 260;
    cols = cols.clamp(2, 4);
    let cw = (area_w - GAP * (cols - 1)) / cols;
    let active = ACTIVE_CAT.load(Ordering::Relaxed);

    let mut col = 0i32;
    let mut row = 0i32;
    let mut accents: Vec<HWND> = Vec::new();
    for cr in &refs {
        let h = cr.hwnd as HWND;
        if cr.cat != active {
            unsafe { ShowWindow(h, SW_HIDE) };
            continue;
        }
        if cr.accent {
            accents.push(h);
            continue;
        }
        let x = AREA_X + col * (cw + GAP);
        let y = CARD_TOP + row * (CARD_H + GAP);
        unsafe {
            MoveWindow(h, x, y, cw, CARD_H, 1);
            ShowWindow(h, SW_SHOW);
        }
        col += 1;
        if col >= cols {
            col = 0;
            row += 1;
        }
    }
    if col != 0 {
        row += 1;
    }
    let mut ay = CARD_TOP + row * (CARD_H + GAP);
    for h in accents {
        unsafe {
            MoveWindow(h, AREA_X, ay, area_w, 56, 1);
            ShowWindow(h, SW_SHOW);
        }
        ay += 56 + GAP;
    }
    invalidate(hwnd);
}

pub fn set_active_category(i: usize) {
    let n = CAT_META.lock().unwrap().len();
    if i >= n {
        return;
    }
    ACTIVE_CAT.store(i as i32, Ordering::Relaxed);
    relayout_cards(main_hwnd());
    for h in CAT_BTNS.lock().unwrap().iter() {
        invalidate(*h as HWND);
    }
}

pub fn paint_main(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = unsafe { std::mem::zeroed() };
    let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
    let mut rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetClientRect(hwnd, &mut rc) };

    let f_title = F_TITLE.load(Ordering::Relaxed);
    let f_sub = F_SUB.load(Ordering::Relaxed);

    fill_round(hdc, rc_r(SIDEBAR_X, 112, SIDEBAR_X + SIDEBAR_W, rc.bottom - 16), ui_panel(), ui_panel_bd(), 18);

    fill_round(hdc, rc_r(26, 34, 70, 78), ui_accent(), ui_accent(), 12);
    draw_snowflake(hdc, 48, 56, 13, ui_accent_tx());
    draw_text(hdc, "FiReWork", rc_r(86, 30, 600, 60), f_title, ui_text(), DT_LEFT | DT_SINGLELINE);
    draw_text(hdc, "Защита и восстановление Windows", rc_r(88, 62, 760, 86), f_sub, ui_muted(), DT_LEFT | DT_SINGLELINE);

    draw_pill(hdc, rc.right - 304, 34, 128, 32, "● Админ");

    unsafe {
        let pen = CreatePen(PS_SOLID, 1, ui_panel_bd());
        let op = SelectObject(hdc, pen as HGDIOBJ);
        MoveToEx(hdc, 24, 100, std::ptr::null_mut());
        LineTo(hdc, rc.right - 24, 100);
        SelectObject(hdc, op);
        DeleteObject(pen as HGDIOBJ);
    }

    let meta = CAT_META.lock().unwrap();
    let active = ACTIVE_CAT.load(Ordering::Relaxed) as usize;
    if active < meta.len() {
        let (name, desc) = &meta[active];
        fill_round(hdc, rc_r(AREA_X, 126, AREA_X + 4, 150), ui_accent(), ui_accent(), 2);
        draw_text(hdc, name, rc_r(AREA_X + 14, 120, rc.right - 24, 152), f_title, ui_text(), DT_LEFT | DT_SINGLELINE);
        draw_text(hdc, desc, rc_r(AREA_X + 14, 154, rc.right - 24, 180), f_sub, ui_muted(), DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS);
    }
    drop(meta);

    draw_text(hdc, concat!("FiReWork · v", env!("CARGO_PKG_VERSION")), rc_r(SIDEBAR_X + 18, rc.bottom - 40, SIDEBAR_X + SIDEBAR_W, rc.bottom - 18), f_sub, ui_muted(), DT_LEFT | DT_SINGLELINE);

    unsafe { EndPaint(hwnd, &ps) };
}

fn rc_r(l: i32, t: i32, r: i32, b: i32) -> RECT {
    RECT { left: l, top: t, right: r, bottom: b }
}

fn draw_pill(hdc: windows_sys::Win32::Graphics::Gdi::HDC, x: i32, y: i32, w: i32, h: i32, text: &str) {
    let f_sub = F_SUB.load(Ordering::Relaxed);
    // в том же стиле, что owner-draw плитка «Настройки» (радиус 12, рамка карточки)
    fill_round(hdc, rc_r(x, y, x + w, y + h), ui_card(), ui_card_bd(), 12);
    if !simple() {
        fill_round(hdc, rc_r(x + 7, y + 2, x + w - 7, y + 5), ui_card_hi(), ui_card_hi(), 2);
    }
    draw_text(hdc, text, rc_r(x, y, x + w, y + h), f_sub, ui_accent(), DT_CENTER | DT_VCENTER | DT_SINGLELINE);
}

// --- Доп. разблокировки (в духе NHelper): клавиши, запрет программ, hosts, sethc ---

const KBD_LAYOUT_KEY: &str = "SYSTEM\\CurrentControlSet\\Control\\Keyboard Layout";
const EXPLORER_POLICY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Policies\\Explorer";
const IFEO_BASES: [&str; 2] = [
    "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Image File Execution Options",
    "SOFTWARE\\Wow6432Node\\Microsoft\\Windows NT\\CurrentVersion\\Image File Execution Options",
];
const ACCESS_EXES: [&str; 7] = [
    "sethc.exe", "utilman.exe", "osk.exe", "Narrator.exe", "Magnify.exe", "DisplaySwitch.exe", "AtBroker.exe",
];

fn off_scancode_map(e: &dyn Fn(&str)) {
    if reg_get_raw(hklm(), KBD_LAYOUT_KEY, "Scancode Map").is_some() {
        if reg_del_value(hklm(), KBD_LAYOUT_KEY, "Scancode Map") {
            e("[OK] Подмена клавиш (Scancode Map) убрана");
        } else {
            e("[!] Scancode Map найден, но удалить не удалось");
        }
    } else {
        e("[—] Подмены клавиш не найдено");
    }
}

fn off_disallow_run(e: &dyn Fn(&str)) {
    let mut hit = false;
    for hive in [hkcu(), hklm()] {
        if reg_del_value(hive, EXPLORER_POLICY, "DisallowRun") {
            hit = true;
        }
        if reg_del_tree(hive, &format!("{EXPLORER_POLICY}\\DisallowRun")) {
            hit = true;
        }
    }
    if hit {
        e("[OK] Запрет запуска программ (DisallowRun) снят");
    } else {
        e("[—] Запрета программ не найдено");
    }
}

fn off_sethc_backdoor(e: &dyn Fn(&str)) {
    e("=== Снятие backdoor спец-возможностей (sethc/utilman) ===");
    let mut hit = false;
    for base in IFEO_BASES {
        for x in ACCESS_EXES {
            if reg_del_value(hklm(), &format!("{base}\\{x}"), "Debugger") {
                e(&format!("[OK] убрана ловушка: {x}"));
                hit = true;
            }
        }
    }
    if !hit {
        e("[—] Ловушек на спец-возможности не найдено");
    }
    e("Если sethc.exe/utilman.exe были подменены — запустите «Проверка файлов» (sfc /scannow).");
}

fn reset_hosts(e: &dyn Fn(&str)) {
    e("=== Сброс файла hosts (разблокировка сайтов) ===");
    let win = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    let path = format!("{win}\\System32\\drivers\\etc\\hosts");
    let _ = exec_args("attrib", &["-r", "-h", "-s", &path]);
    let _ = std::fs::copy(&path, format!("{path}.bak"));
    let def = "# Copyright (c) 1993-2009 Microsoft Corp.\r\n#\r\n\
# This is a sample HOSTS file used by Microsoft TCP/IP for Windows.\r\n#\r\n\
#\t127.0.0.1\tlocalhost\r\n#\t::1\tlocalhost\r\n";
    match std::fs::write(&path, def) {
        Ok(_) => {
            e("[OK] hosts сброшен на стандартный (бэкап: hosts.bak)");
            let _ = exec_args("ipconfig", &["/flushdns"]);
            e("[OK] DNS-кэш очищен");
        }
        Err(err) => e(&format!("[!] Не удалось записать hosts: {err}")),
    }
}

// --- Очистка: Temp/Prefetch и корзина (нативно) ---

/// Удаляет содержимое папки (файлы и подпапки), не трогая саму папку.
/// Возвращает (удалено, пропущено_занятых, освобождено_байт).
fn clean_dir_contents(dir: &str) -> (u64, u64, u64) {
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return (0, 0, 0),
    };
    let (mut del, mut skip, mut bytes) = (0u64, 0u64, 0u64);
    for entry in rd.flatten() {
        let path = entry.path();
        let md = entry.metadata();
        let is_dir = md.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let sz = md.as_ref().map(|m| if m.is_file() { m.len() } else { 0 }).unwrap_or(0);
        let res = if is_dir {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        match res {
            Ok(_) => {
                del += 1;
                bytes += sz;
            }
            Err(_) => skip += 1,
        }
    }
    (del, skip, bytes)
}

fn clean_temp(e: &dyn Fn(&str)) {
    e("=== Очистка временных файлов ===");
    let win = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    let mut targets: Vec<String> = Vec::new();
    for var in ["TEMP", "TMP"] {
        if let Ok(t) = std::env::var(var) {
            if !targets.iter().any(|x| x.eq_ignore_ascii_case(&t)) {
                targets.push(t);
            }
        }
    }
    targets.push(format!("{win}\\Temp"));
    targets.push(format!("{win}\\Prefetch"));

    let (mut d, mut s, mut b) = (0u64, 0u64, 0u64);
    for t in &targets {
        let (dd, ss, bb) = clean_dir_contents(t);
        e(&format!("  {t} — удалено {dd}, пропущено {ss}"));
        d += dd;
        s += ss;
        b += bb;
    }
    e("");
    e(&format!("Итого удалено объектов: {d}, освобождено ~{} МБ.", b / 1024 / 1024));
    if s > 0 {
        e(&format!("Пропущено занятых (используются системой): {s} — это нормально."));
    }
    e("— завершено —");
}

fn empty_recycle_bin(e: &dyn Fn(&str)) {
    e("=== Очистка корзины ===");
    const SHERB_NOCONFIRMATION: u32 = 0x1;
    const SHERB_NOPROGRESSUI: u32 = 0x2;
    const SHERB_NOSOUND: u32 = 0x4;
    let hr = unsafe {
        SHEmptyRecycleBinW(
            std::ptr::null_mut(),
            std::ptr::null(),
            SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
        )
    };
    if hr == 0 {
        e("[OK] Корзина очищена.");
    } else {
        e(&format!("[!] Код 0x{:08X} (вероятно, корзина уже пуста).", hr as u32));
    }
    e("— завершено —");
}

// --- Восстановление системы (точки) ---

fn run_ps(e: &dyn Fn(&str), script: &str) {
    let tmp = std::env::var("TEMP").unwrap_or_else(|_| std::env::var("USERPROFILE").unwrap_or_default());
    let path = format!("{tmp}\\as_rp.ps1");
    // BOM обязателен: без него PowerShell 5.1 читает файл как ANSI и ломает кириллицу.
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(script.as_bytes());
    if std::fs::write(&path, &bytes).is_err() {
        e("Не удалось создать временный скрипт.");
        return;
    }
    let out = exec_args("powershell", &["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", &path]);
    if !out.trim().is_empty() {
        e(&out);
    }
    let _ = std::fs::remove_file(&path);
}

fn ensure_restore_enabled(e: &dyn Fn(&str)) {
    // снять лимит «одна точка в 24 часа»
    reg_set_dword(
        hklm(),
        "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\SystemRestore",
        "SystemRestorePointCreationFrequency",
        0,
    );
    // включить Защиту системы на диске C:
    let out = exec_args("powershell", &["-NoProfile", "-Command", "Enable-ComputerRestore -Drive 'C:\\'"]);
    if !out.trim().is_empty() {
        e(&out);
    }
}

fn create_restore_point(e: &dyn Fn(&str)) {
    e("=== Создание точки восстановления ===");
    e("Включаю Защиту системы и снимаю 24-часовой лимит…");
    ensure_restore_enabled(e);
    e("Создаю точку (может занять до минуты)…");
    let script = concat!(
        "$OutputEncoding=[Console]::OutputEncoding=[Text.UTF8Encoding]::new()\r\n",
        "$before=@(Get-ComputerRestorePoint -ErrorAction SilentlyContinue).Count\r\n",
        "try{ Checkpoint-Computer -Description 'FiReWork' -RestorePointType 'MODIFY_SETTINGS' -ErrorAction Stop }\r\n",
        "catch{ Write-Output ('Ошибка: '+$_.Exception.Message) }\r\n",
        "$after=@(Get-ComputerRestorePoint -ErrorAction SilentlyContinue).Count\r\n",
        "if($after -gt $before){ Write-Output 'Точка восстановления создана.' }\r\n",
        "else{ Write-Output 'Новая точка не появилась. Проверьте: Защита системы включена в «Свойства системы», на диске выделено место под восстановление.' }\r\n",
    );
    run_ps(e, script);
    e("— завершено —");
}

fn show_restore_points(e: &dyn Fn(&str)) {
    e("=== Точки восстановления ===");
    let script = concat!(
        "$OutputEncoding=[Console]::OutputEncoding=[Text.UTF8Encoding]::new()\r\n",
        "$pts=@(Get-ComputerRestorePoint -ErrorAction SilentlyContinue)\r\n",
        "if($pts.Count -eq 0){ Write-Output 'Точек восстановления нет.' }\r\n",
        "else{ foreach($p in $pts){ $t=$p.ConvertToDateTime($p.CreationTime); ",
        "Write-Output ('#'+$p.SequenceNumber+'   '+$t.ToString('yyyy-MM-dd HH:mm')+'   '+$p.Description) } }\r\n",
    );
    run_ps(e, script);
    e("— завершено —");
}

fn open_restore_wizard(e: &dyn Fn(&str)) {
    e("Включаю Защиту системы и открываю мастер восстановления…");
    ensure_restore_enabled(e);
    let win = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    launch(&format!("{win}\\System32\\rstrui.exe"), "");
}

fn define_categories() -> Vec<Cat> {
    vec![
        Cat {
            name: "Инструменты".into(),
            desc: "Основные инструменты восстановления и ремонта системы.".into(),
            cards: vec![
                card("Проверка файлов", "sfc /scannow", Item::Console("sfc /scannow".into())),
                card("Восстановить LogonUI", "Вернуть экран входа", run(|e| {
                    e("=== Восстановление оболочки входа ===");
                    let wl = "HKLM\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Winlogon";
                    e_exec(e, "reg", &["add", wl, "/v", "Shell", "/t", "REG_SZ", "/d", "explorer.exe", "/f"]);
                    e_exec(e, "reg", &["add", wl, "/v", "Userinit", "/t", "REG_SZ", "/d", "C:\\Windows\\system32\\userinit.exe,", "/f"]);
                    e("Готово. Оболочка входа возвращена к стандартной.");
                })),
                card("Сброс Windows Update", "Очистить кэш обновлений", run(|e| {
                    e("=== Сброс компонентов Windows Update ===");
                    e_quiet(e, "net stop wuauserv");
                    e_quiet(e, "net stop bits");
                    e_quiet(e, "net stop cryptsvc");
                    e_quiet(e, "ren \"%systemroot%\\SoftwareDistribution\" SoftwareDistribution.old 2>nul");
                    e_quiet(e, "ren \"%systemroot%\\System32\\catroot2\" catroot2.old 2>nul");
                    e_quiet(e, "net start wuauserv");
                    e_quiet(e, "net start bits");
                    e_shell(e, "net start cryptsvc");
                })),
                card("Сброс сети", "DNS / Winsock / прокси", run(|e| {
                    e("=== Сброс сетевых настроек ===");
                    e_quiet(e, "ipconfig /flushdns");
                    e_quiet(e, "netsh winhttp reset proxy");
                    e_quiet(e, "netsh winsock reset");
                    e_quiet(e, "netsh int ip reset");
                    e("");
                    e("Отдельные «Access is denied» при сбросе TCP/IP — это нормально.");
                    e("Сброс выполнен. Для завершения нужна перезагрузка.");
                })),
                card("Восстановить загрузчик", "bcdboot C:\\Windows", Item::Console("bcdboot C:\\Windows & echo. & echo Если система не на C: — укажите нужную букву.".into())),
                card("Проверить диск", "chkdsk C: /f /r", Item::Console("chkdsk C: /f /r".into())),
                card("Сброс кеша иконок", "Перезапуск проводника", run(|e| {
                    e("=== Сброс кеша иконок ===");
                    e_quiet(e, "taskkill /f /im explorer.exe");
                    e_quiet(e, "del /a /f /q \"%LocalAppData%\\IconCache.db\" 2>nul");
                    e_quiet(e, "del /a /f /q \"%LocalAppData%\\Microsoft\\Windows\\Explorer\\iconcache*\" 2>nul");
                    e_shell(e, "start explorer.exe");
                })),
                card("Сброс политик безоп.", "secedit defltbase", run(|e| {
                    e_shell(e, "secedit /configure /cfg %windir%\\inf\\defltbase.inf /db defltbase.sdb /verbose");
                })),
                card("Включить UAC", "Стандартный уровень", run(|e| {
                    e("=== Включение UAC (уровень по умолчанию) ===");
                    let s = "HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Policies\\System";
                    e_exec(e, "reg", &["add", s, "/v", "EnableLUA", "/t", "REG_DWORD", "/d", "1", "/f"]);
                    e_exec(e, "reg", &["add", s, "/v", "ConsentPromptBehaviorAdmin", "/t", "REG_DWORD", "/d", "5", "/f"]);
                    e_exec(e, "reg", &["add", s, "/v", "PromptOnSecureDesktop", "/t", "REG_DWORD", "/d", "1", "/f"]);
                    e("Готово. Нужна перезагрузка.");
                })),
                card("Diskpart", "Управление дисками", Item::Launch("diskpart.exe".into(), String::new())),
                card("Ассоциации файлов", "Приложения по умолчанию", Item::Launch("ms-settings:defaultapps".into(), String::new())),
                card("Восстановить MBR", "bootrec (в WinRE)", Item::Console("bootrec /fixmbr & bootrec /fixboot".into())),
                card("Полный доступ к файлу", "takeown + icacls", run(|e| {
                    e("Открываю окно ввода пути…");
                    let p = input_box("Путь к файлу или папке (например C:\\Windows\\file.exe):", "Полный доступ к файлу");
                    if p.is_empty() {
                        e("Отменено — путь не указан.");
                        return;
                    }
                    e(&format!("=== Получение полного доступа: {p} ==="));
                    e_exec(e, "takeown", &["/f", &p, "/r", "/d", "y"]);
                    e_exec(e, "icacls", &[&p, "/grant", "*S-1-5-32-544:F", "/t"]);
                    e("Готово.");
                })),
                accent("Разблокировать диск C", "Снять все ограничения", run(|e| {
                    e("=== Снятие ВСЕХ ограничений Проводника и дисков ===");
                    e("Сбрасываю локальную групповую политику…");
                    e_quiet(e, "rd /s /q \"%WinDir%\\System32\\GroupPolicy\"");
                    e_quiet(e, "rd /s /q \"%WinDir%\\System32\\GroupPolicyUsers\"");
                    e_exec(e, "gpupdate", &["/force"]);
                    let exp = "Software\\Microsoft\\Windows\\CurrentVersion\\Policies\\Explorer";
                    del_key(e, hkcu(), exp);
                    del_key(e, hklm(), exp);
                    del_key(e, hkcu(), "Software\\Policies\\Microsoft\\Windows\\Explorer");
                    del_key(e, hklm(), "Software\\Policies\\Microsoft\\Windows\\Explorer");
                    e("Перезапускаю проводник…");
                    e_quiet(e, "taskkill /f /im explorer.exe");
                    let _ = exec_args("cmd", &["/c", "start explorer.exe"]);
                    e("");
                    let active = reg_get_dword(hkcu(), exp, "NoDrives").is_some()
                        || reg_get_dword(hklm(), exp, "NoDrives").is_some()
                        || reg_get_dword(hkcu(), exp, "NoViewOnDrive").is_some()
                        || reg_get_dword(hklm(), exp, "NoViewOnDrive").is_some();
                    if active {
                        e("Проверка: значения ещё есть. Нужен выход из системы и вход снова.");
                    } else {
                        e("Проверка: ограничения на диски сняты ✓");
                    }
                })),
            ],
        },
        Cat {
            name: "Разблокировка".into(),
            desc: "Снимаем ограничения, которые ставят вирусы.".into(),
            cards: vec![
                card("Проверка ограничений", "Найти активные запреты", run(|e| {
                    e("=== Проверка системы на ограничения ===");
                    e("");
                    let mut count = 0;
                    let sys = "Software\\Microsoft\\Windows\\CurrentVersion\\Policies\\System";
                    let exp = "Software\\Microsoft\\Windows\\CurrentVersion\\Policies\\Explorer";
                    let mut chk = |subkey: &str, name: &str, label: &str| {
                        if restriction_active(subkey, name) {
                            e(&format!("  [!] {label}"));
                            count += 1;
                        }
                    };
                    chk(sys, "DisableTaskMgr", "Диспетчер задач заблокирован");
                    chk(sys, "DisableRegistryTools", "Редактор реестра заблокирован");
                    chk("Software\\Policies\\Microsoft\\Windows\\System", "DisableCMD", "Командная строка заблокирована");
                    chk(exp, "NoFolderOptions", "«Свойства папки» скрыты");
                    chk(exp, "NoRun", "«Выполнить» скрыт");
                    chk(exp, "NoControlPanel", "Панель управления скрыта");
                    chk(exp, "NoDrives", "Диски скрыты");
                    chk(exp, "NoViewOnDrive", "Доступ к дискам заблокирован");
                    if let Some(v) = reg_get_dword(hklm(), sys, "EnableLUA") {
                        if v == 0 {
                            e("  [!] UAC отключён");
                            count += 1;
                        }
                    }
                    let ifeo = exec_args("powershell", &["-NoProfile", "-Command",
                        "$b='HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Image File Execution Options'; (Get-ChildItem $b | Where-Object { (Get-ItemProperty $_.PSPath -Name Debugger -ErrorAction SilentlyContinue).Debugger }).Count"]);
                    let ifeo = ifeo.trim();
                    if !ifeo.is_empty() && ifeo != "0" {
                        e(&format!("  [!] IFEO-ловушки (Debugger): {ifeo} шт."));
                        count += 1;
                    }
                    if reg_get_raw(hklm(), KBD_LAYOUT_KEY, "Scancode Map").is_some() {
                        e("  [!] Подмена клавиш (Scancode Map)");
                        count += 1;
                    }
                    if reg_get_dword(hkcu(), EXPLORER_POLICY, "DisallowRun").unwrap_or(0) != 0
                        || reg_get_dword(hklm(), EXPLORER_POLICY, "DisallowRun").unwrap_or(0) != 0
                    {
                        e("  [!] Запрет запуска программ (DisallowRun)");
                        count += 1;
                    }
                    let win = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
                    if let Ok(h) = std::fs::read_to_string(format!("{win}\\System32\\drivers\\etc\\hosts")) {
                        let bad = h
                            .lines()
                            .filter(|l| {
                                let t = l.trim();
                                !t.is_empty() && !t.starts_with('#') && !t.contains("localhost")
                            })
                            .count();
                        if bad > 0 {
                            e(&format!("  [!] В hosts активных записей: {bad}"));
                            count += 1;
                        }
                    }
                    e("");
                    if count == 0 {
                        e("Ограничений не найдено — система чистая.");
                    } else {
                        e(&format!("Найдено проблем: {count}."));
                        e("Снять: кнопки этого раздела и «Снять все ограничения».");
                    }
                })),
                card("Диспетчер задач", "Вернуть доступ", run(|e| {
                    off_restriction(e, "Software\\Microsoft\\Windows\\CurrentVersion\\Policies\\System", "DisableTaskMgr", "Диспетчер задач");
                })),
                card("Редактор реестра", "regedit", run(|e| {
                    off_restriction(e, "Software\\Microsoft\\Windows\\CurrentVersion\\Policies\\System", "DisableRegistryTools", "Редактор реестра");
                })),
                card("Командная строка", "cmd", run(|e| {
                    off_restriction(e, "Software\\Policies\\Microsoft\\Windows\\System", "DisableCMD", "Командная строка");
                })),
                card("Свойства папки", "Вернуть пункт", run(|e| {
                    off_restriction(e, "Software\\Microsoft\\Windows\\CurrentVersion\\Policies\\Explorer", "NoFolderOptions", "Свойства папки");
                })),
                card("Пункт «Выполнить»", "Вернуть", run(|e| {
                    off_restriction(e, "Software\\Microsoft\\Windows\\CurrentVersion\\Policies\\Explorer", "NoRun", "Пункт «Выполнить»");
                })),
                card("Скрытые файлы", "Показать (вкл. системные)", run(|e| {
                    let adv = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced";
                    reg_set_dword(hkcu(), adv, "Hidden", 1);
                    reg_set_dword(hkcu(), adv, "HideFileExt", 0);
                    reg_set_dword(hkcu(), adv, "ShowSuperHidden", 1);
                    e("[OK] Включён показ скрытых, системных файлов и расширений.");
                    e_quiet(e, "taskkill /f /im explorer.exe");
                    let _ = exec_args("cmd", &["/c", "start explorer.exe"]);
                    e("Готово.");
                })),
                card("Шрифты по умолчанию", "Вернуть системный шрифт", run(|e| {
                    e("=== Восстановление системных шрифтов ===");
                    let fs = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\FontSubstitutes";
                    let bad = ["Segoe UI", "Segoe UI Semibold", "Segoe UI Light", "Segoe UI Black", "Tahoma", "Arial", "Verdana", "Calibri", "Microsoft Sans Serif"];
                    for n in bad {
                        if reg_del_value(hklm(), fs, n) {
                            e(&format!("[OK] убрана подмена: {n}"));
                        }
                    }
                    reg_set_sz(hklm(), fs, "MS Shell Dlg", "Microsoft Sans Serif");
                    reg_set_sz(hklm(), fs, "MS Shell Dlg 2", "Tahoma");
                    let fonts = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Fonts";
                    let reg: [(&str, &str); 8] = [
                        ("Segoe UI (TrueType)", "segoeui.ttf"),
                        ("Segoe UI Bold (TrueType)", "segoeuib.ttf"),
                        ("Segoe UI Italic (TrueType)", "segoeuii.ttf"),
                        ("Segoe UI Bold Italic (TrueType)", "segoeuiz.ttf"),
                        ("Segoe UI Semibold (TrueType)", "seguisb.ttf"),
                        ("Segoe UI Light (TrueType)", "segoeuil.ttf"),
                        ("Tahoma (TrueType)", "tahoma.ttf"),
                        ("Microsoft Sans Serif (TrueType)", "micross.ttf"),
                    ];
                    for (name, file) in reg {
                        reg_set_sz(hklm(), fonts, name, file);
                    }
                    e("");
                    e("Стандартные шрифты восстановлены. Перезагрузите компьютер.");
                })),
                card("IFEO-ловушки", "Найти и снять", run(|e| remove_ifeo(e))),
                card("Подмена клавиш", "Убрать Scancode Map", run(|e| off_scancode_map(e))),
                card("Запрет программ", "Снять DisallowRun", run(|e| off_disallow_run(e))),
                card("Клавиши залипания", "Снять backdoor sethc/utilman", run(|e| off_sethc_backdoor(e))),
                card("Файл hosts", "Разблокировать сайты", run(|e| reset_hosts(e))),
                accent("Снять все ограничения", "Диспетчер, реестр, cmd, «Выполнить» и прочее", run(|e| {
                    e("=== Снимаю все ограничения ===");
                    let sys = "Software\\Microsoft\\Windows\\CurrentVersion\\Policies\\System";
                    let exp = "Software\\Microsoft\\Windows\\CurrentVersion\\Policies\\Explorer";
                    off_restriction(e, sys, "DisableTaskMgr", "Диспетчер задач");
                    off_restriction(e, sys, "DisableRegistryTools", "Редактор реестра");
                    off_restriction(e, "Software\\Policies\\Microsoft\\Windows\\System", "DisableCMD", "Командная строка");
                    off_restriction(e, exp, "NoFolderOptions", "Свойства папки");
                    off_restriction(e, exp, "NoRun", "Пункт «Выполнить»");
                    off_restriction(e, exp, "NoControlPanel", "Панель управления");
                    off_restriction(e, exp, "NoDrives", "Скрытие дисков");
                    off_restriction(e, exp, "NoViewOnDrive", "Доступ к дискам");
                    off_scancode_map(e);
                    off_disallow_run(e);
                    off_sethc_backdoor(e);
                    e("");
                    e("Сбрасываю локальную групповую политику…");
                    e_quiet(e, "rd /s /q \"%WinDir%\\System32\\GroupPolicy\"");
                    e_quiet(e, "rd /s /q \"%WinDir%\\System32\\GroupPolicyUsers\"");
                    e_quiet(e, "gpupdate /force");
                    e("=== Готово. Если что-то осталось — выход/вход в систему. ===");
                })),
            ],
        },
        Cat {
            name: "Процессы".into(),
            desc: "Просмотр программ, завершение, заморозка и снятие критичности.".into(),
            cards: vec![card("Список процессов", "Таблица + меню по ПКМ", Item::Proc)],
        },
        Cat {
            name: "Восстановление".into(),
            desc: "Точки восстановления системы и откат к сохранённому состоянию.".into(),
            cards: vec![
                card("Создать точку", "Снимок системы", run(|e| create_restore_point(e))),
                card("Показать точки", "Список снимков", run(|e| show_restore_points(e))),
                card("Мастер отката", "rstrui", run(|e| open_restore_wizard(e))),
            ],
        },
        Cat {
            name: "Очистка".into(),
            desc: "Удаление временных файлов и очистка корзины.".into(),
            cards: vec![
                card("Temp / Prefetch", "Временные файлы", run(|e| clean_temp(e))),
                card("Очистить корзину", "Освободить корзину", run(|e| empty_recycle_bin(e))),
            ],
        },
        Cat {
            name: "Диагностика".into(),
            desc: "Проверка и восстановление целостности системы, антивирус.".into(),
            cards: vec![
                card("Восстановить образ", "DISM RestoreHealth", Item::Console("dism /online /cleanup-image /restorehealth".into())),
                card("Проверить диск", "chkdsk C: (только чтение)", Item::Console("chkdsk C:".into())),
                card("Проверка Defender", "Быстрая проверка", run(|e| {
                    e_shell(e, "powershell -NoProfile -Command \"Start-MpScan -ScanType QuickScan\"");
                })),
            ],
        },
        Cat {
            name: "Автозагрузка".into(),
            desc: "Run, Winlogon, AppInit, планировщик, службы. Можно удалить или заменить.".into(),
            cards: vec![card("Открыть автозагрузку", "Run / Winlogon / службы / задачи", Item::Auto)],
        },
        Cat {
            name: "Система".into(),
            desc: "Автозагрузка, службы, планировщик задач и безопасный режим.".into(),
            cards: vec![
                card("Показать автозагрузку", "Реестр + папка", run(|e| {
                    e("=== Реестр: автозагрузка ===");
                    e_quiet(e, "reg query \"HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\"");
                    e_quiet(e, "reg query \"HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\"");
                    e_quiet(e, "reg query \"HKLM\\Software\\Wow6432Node\\Microsoft\\Windows\\CurrentVersion\\Run\"");
                    e("=== Папка автозагрузки ===");
                    e_quiet(e, "dir \"%APPDATA%\\Microsoft\\Windows\\Start Menu\\Programs\\Startup\"");
                })),
                card("Папка автозагрузки", "Открыть", Item::Launch("explorer.exe".into(), "shell:startup".into())),
                card("Конфигурация системы", "msconfig", Item::Launch("msconfig".into(), String::new())),
                card("Службы Windows", "services.msc", Item::Launch("services.msc".into(), String::new())),
                card("Планировщик: список", "schtasks", run(|e| e_shell(e, "schtasks /query /fo TABLE"))),
                card("Открыть планировщик", "taskschd.msc", Item::Launch("taskschd.msc".into(), String::new())),
                card("Добавить пользователя", "Имя, пароль, права", run(|e| {
                    let Some((name, pass, admin)) = add_user_dialog() else {
                        e("Отменено.");
                        return;
                    };
                    let name = name.trim().to_string();
                    if name.is_empty() {
                        e("Отменено — имя не указано.");
                        return;
                    }
                    e(&format!("=== Создание пользователя: {name} ==="));
                    let (out, code) = exec_args_code("net", &["user", &name, &pass, "/add"]);
                    if !out.trim().is_empty() {
                        e(&out);
                    }
                    if code != 0 {
                        e("[!] Не удалось создать пользователя (см. сообщение выше).");
                        return;
                    }
                    e("[OK] Пользователь создан.");
                    if admin {
                        let (o2, c2) = exec_args_code("net", &["localgroup", "Administrators", &name, "/add"]);
                        if !o2.trim().is_empty() {
                            e(&o2);
                        }
                        if c2 == 0 {
                            e("[OK] Выданы права администратора.");
                        } else {
                            // на русской Windows группа называется «Администраторы»
                            let (o3, c3) = exec_args_code("net", &["localgroup", "Администраторы", &name, "/add"]);
                            if !o3.trim().is_empty() {
                                e(&o3);
                            }
                            if c3 == 0 {
                                e("[OK] Выданы права администратора.");
                            } else {
                                e("[!] Пользователь создан, но добавить в администраторы не удалось.");
                            }
                        }
                    } else {
                        e("Тип: обычный пользователь.");
                    }
                    e("— завершено —");
                })),
                card("Безопасный режим", "Включить (reboot)", run(|e| {
                    e("=== Включаю загрузку в безопасный режим ===");
                    e_shell(e, "bcdedit /set {current} safeboot minimal");
                    e("После перезагрузки система стартует в безопасном режиме.");
                })),
                card("Обычная загрузка", "Выключить безопасный режим", run(|e| {
                    e_shell(e, "bcdedit /deletevalue {current} safeboot");
                })),
            ],
        },
    ]
}
