use crate::win::*;
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::atomic::Ordering;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Security::*;
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn clean(mut out: Vec<u8>, mut err: Vec<u8>) -> String {
    out.append(&mut err);
    let s = String::from_utf8_lossy(&out).replace('\u{0}', "");
    s.trim_end_matches(['\r', '\n', ' ']).to_string()
}

pub fn exec_cmd(cmdline: &str) -> String {
    match Command::new("cmd")
        .args(["/c", &format!("chcp 65001>nul & {cmdline}")])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        Ok(o) => clean(o.stdout, o.stderr),
        Err(e) => format!("Ошибка запуска: {e}"),
    }
}

pub fn exec_args(file: &str, args: &[&str]) -> String {
    match Command::new(file)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        Ok(o) => clean(o.stdout, o.stderr),
        Err(e) => format!("Ошибка запуска: {e}"),
    }
}

pub fn exec_args_code(file: &str, args: &[&str]) -> (String, i32) {
    match Command::new(file)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        Ok(o) => {
            let code = o.status.code().unwrap_or(-1);
            (clean(o.stdout, o.stderr), code)
        }
        Err(e) => (format!("Ошибка запуска: {e}"), -1),
    }
}

pub fn launch(file: &str, args: &str) {
    let verb = wide("open");
    let f = wide(file);
    let a = wide(args);
    let ap = if args.is_empty() { std::ptr::null() } else { a.as_ptr() };
    unsafe {
        ShellExecuteW(
            std::ptr::null_mut(), verb.as_ptr(), f.as_ptr(), ap,
            std::ptr::null(), SW_SHOWNORMAL,
        );
    }
    logln(&format!("Запущено: {file} {args}"));
}

pub fn e_shell(e: &dyn Fn(&str), cmdline: &str) {
    e(&format!("> {cmdline}"));
    e(&exec_cmd(cmdline));
    e("— завершено —");
}

pub fn e_quiet(e: &dyn Fn(&str), cmdline: &str) {
    let out = exec_cmd(cmdline);
    if !out.trim().is_empty() {
        e(&out);
    }
}

pub fn e_exec(e: &dyn Fn(&str), file: &str, args: &[&str]) {
    e(&format!("> {} {}", file, args.join(" ")));
    let out = exec_args(file, args);
    if !out.trim().is_empty() {
        e(&out);
    }
    e("— завершено —");
}

pub fn logln(text: &str) {
    LOG_QUEUE.lock().unwrap().push(text.to_string());
    post_main(WMAPP_LOG, 0);
}

pub fn is_admin() -> bool {
    unsafe {
        let mut tok: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut tok) == 0 {
            return false;
        }
        let mut elev = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut sz = 0u32;
        let ok = GetTokenInformation(
            tok,
            TokenElevation,
            &mut elev as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut sz,
        );
        CloseHandle(tok);
        ok != 0 && elev.TokenIsElevated != 0
    }
}

pub fn enable_debug_privilege() {
    unsafe {
        let mut tok: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut tok,
        ) == 0
        {
            return;
        }
        let name = wide("SeDebugPrivilege");
        let mut luid = LUID { LowPart: 0, HighPart: 0 };
        if LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &mut luid) == 0 {
            CloseHandle(tok);
            return;
        }
        let mut tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        AdjustTokenPrivileges(tok, 0, &mut tp, 0, std::ptr::null_mut(), std::ptr::null_mut());
        CloseHandle(tok);
    }
}

pub fn remove_ifeo(e: &dyn Fn(&str)) {
    e("=== Поиск IFEO-ловушек (Debugger) ===");
    let script = concat!(
        "$OutputEncoding=[Console]::OutputEncoding=[Text.UTF8Encoding]::new()\r\n",
        "$bases=@('HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Image File Execution Options',",
        "'HKLM:\\SOFTWARE\\Wow6432Node\\Microsoft\\Windows NT\\CurrentVersion\\Image File Execution Options')\r\n",
        "$found=$false\r\n",
        "foreach($b in $bases){ if(Test-Path $b){ Get-ChildItem $b | ForEach-Object {\r\n",
        "  $d=(Get-ItemProperty $_.PSPath -Name Debugger -ErrorAction SilentlyContinue).Debugger\r\n",
        "  if($d){ $found=$true; Write-Output ('[-] '+$_.PSChildName+'  ->  '+$d)\r\n",
        "    Remove-ItemProperty -Path $_.PSPath -Name Debugger -Force -ErrorAction SilentlyContinue } } } }\r\n",
        "if(-not $found){ Write-Output 'Ловушек не найдено.' } else { Write-Output 'Блокировки сняты.' }\r\n"
    );
    let tmp = std::env::var("TEMP").unwrap_or_else(|_| std::env::var("USERPROFILE").unwrap_or_default());
    let path = format!("{tmp}\\as_ifeo.ps1");
    if std::fs::write(&path, script).is_err() {
        e("Не удалось создать временный скрипт.");
        return;
    }
    let out = exec_args("powershell", &["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", &path]);
    e(&out);
    let _ = std::fs::remove_file(&path);
    e("— завершено —");
}

pub fn simple() -> bool {
    SIMPLE_MODE.load(Ordering::Relaxed)
}
