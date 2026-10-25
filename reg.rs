use crate::win::wide;
use windows_sys::Win32::System::Registry::*;

pub const REG_T_SZ: u32 = 1;
pub const REG_T_EXPAND_SZ: u32 = 2;
pub const REG_T_BINARY: u32 = 3;
pub const REG_T_DWORD: u32 = 4;
pub const REG_T_MULTI_SZ: u32 = 7;
pub const REG_T_QWORD: u32 = 11;

pub fn hkcu() -> HKEY {
    HKEY_CURRENT_USER
}
pub fn hklm() -> HKEY {
    HKEY_LOCAL_MACHINE
}

pub struct RegKv {
    pub name: String,
    pub value: String,
}

fn open(root: HKEY, subkey: &str, sam: u32) -> Option<HKEY> {
    let sk = wide(subkey);
    let mut hk: HKEY = std::ptr::null_mut();
    let r = unsafe { RegOpenKeyExW(root, sk.as_ptr(), 0, sam, &mut hk) };
    if r == 0 {
        Some(hk)
    } else {
        None
    }
}

fn create(root: HKEY, subkey: &str) -> Option<HKEY> {
    let sk = wide(subkey);
    let mut hk: HKEY = std::ptr::null_mut();
    let mut disp = 0u32;
    let r = unsafe {
        RegCreateKeyExW(
            root, sk.as_ptr(), 0, std::ptr::null(), 0, KEY_SET_VALUE,
            std::ptr::null(), &mut hk, &mut disp,
        )
    };
    if r == 0 {
        Some(hk)
    } else {
        None
    }
}

pub fn reg_get_dword(root: HKEY, subkey: &str, name: &str) -> Option<i64> {
    let hk = open(root, subkey, KEY_QUERY_VALUE)?;
    let nm = wide(name);
    let mut typ = 0u32;
    let mut data = 0u32;
    let mut sz = 4u32;
    let r = unsafe {
        RegQueryValueExW(hk, nm.as_ptr(), std::ptr::null(), &mut typ, &mut data as *mut _ as *mut u8, &mut sz)
    };
    unsafe { RegCloseKey(hk) };
    if r == 0 {
        Some(data as i64)
    } else {
        None
    }
}

pub fn reg_set_dword(root: HKEY, subkey: &str, name: &str, val: u32) -> bool {
    let Some(hk) = create(root, subkey) else { return false };
    let nm = wide(name);
    let v = val;
    let r = unsafe {
        RegSetValueExW(hk, nm.as_ptr(), 0, REG_T_DWORD, &v as *const _ as *const u8, 4)
    };
    unsafe { RegCloseKey(hk) };
    r == 0
}

pub fn reg_set_sz(root: HKEY, subkey: &str, name: &str, value: &str) -> bool {
    let Some(hk) = create(root, subkey) else { return false };
    let nm = wide(name);
    let u = wide(value);
    let r = unsafe {
        RegSetValueExW(hk, nm.as_ptr(), 0, REG_T_SZ, u.as_ptr() as *const u8, (u.len() * 2) as u32)
    };
    unsafe { RegCloseKey(hk) };
    r == 0
}

pub fn reg_del_value(root: HKEY, subkey: &str, name: &str) -> bool {
    let Some(hk) = open(root, subkey, KEY_SET_VALUE) else { return false };
    let nm = wide(name);
    let r = unsafe { RegDeleteValueW(hk, nm.as_ptr()) };
    unsafe { RegCloseKey(hk) };
    r == 0
}

pub fn reg_get_string(root: HKEY, subkey: &str, name: &str) -> Option<String> {
    let hk = open(root, subkey, KEY_QUERY_VALUE)?;
    let nm = wide(name);
    let mut data = vec![0u8; 8192];
    let mut sz = data.len() as u32;
    let mut typ = 0u32;
    let r = unsafe {
        RegQueryValueExW(hk, nm.as_ptr(), std::ptr::null(), &mut typ, data.as_mut_ptr(), &mut sz)
    };
    unsafe { RegCloseKey(hk) };
    if r != 0 {
        return None;
    }
    Some(reg_data_to_string(typ, &data[..sz as usize]))
}

pub fn reg_list_values(root: HKEY, subkey: &str) -> Vec<RegKv> {
    let Some(hk) = open(root, subkey, KEY_QUERY_VALUE) else { return Vec::new() };
    let mut out = Vec::new();
    let mut idx = 0u32;
    loop {
        let mut name_buf = vec![0u16; 512];
        let mut data_buf = vec![0u8; 8192];
        let mut name_len = name_buf.len() as u32;
        let mut data_len = data_buf.len() as u32;
        let mut typ = 0u32;
        let r = unsafe {
            RegEnumValueW(
                hk, idx, name_buf.as_mut_ptr(), &mut name_len,
                std::ptr::null(), &mut typ, data_buf.as_mut_ptr(), &mut data_len,
            )
        };
        if r != 0 {
            break;
        }
        let name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
        let value = reg_data_to_string(typ, &data_buf[..data_len as usize]);
        out.push(RegKv { name, value });
        idx += 1;
    }
    unsafe { RegCloseKey(hk) };
    out
}

pub fn reg_get_raw(root: HKEY, subkey: &str, name: &str) -> Option<(Vec<u8>, u32)> {
    let hk = open(root, subkey, KEY_QUERY_VALUE)?;
    let nm = wide(name);
    let mut typ = 0u32;
    let mut sz = 0u32;
    let r = unsafe {
        RegQueryValueExW(hk, nm.as_ptr(), std::ptr::null(), &mut typ, std::ptr::null_mut(), &mut sz)
    };
    if r != 0 && r != 234 {
        unsafe { RegCloseKey(hk) };
        return None;
    }
    if sz == 0 {
        unsafe { RegCloseKey(hk) };
        return Some((Vec::new(), typ));
    }
    let mut data = vec![0u8; sz as usize];
    let r = unsafe {
        RegQueryValueExW(hk, nm.as_ptr(), std::ptr::null(), &mut typ, data.as_mut_ptr(), &mut sz)
    };
    unsafe { RegCloseKey(hk) };
    if r != 0 {
        return None;
    }
    data.truncate(sz as usize);
    Some((data, typ))
}

pub fn reg_set_raw(root: HKEY, subkey: &str, name: &str, typ: u32, data: &[u8]) -> bool {
    let Some(hk) = create(root, subkey) else { return false };
    let nm = wide(name);
    let p = if data.is_empty() { [0u8].as_ptr() } else { data.as_ptr() };
    let r = unsafe { RegSetValueExW(hk, nm.as_ptr(), 0, typ, p, data.len() as u32) };
    unsafe { RegCloseKey(hk) };
    r == 0
}

pub fn reg_del_tree(root: HKEY, subkey: &str) -> bool {
    let sk = wide(subkey);
    unsafe { RegDeleteTreeW(root, sk.as_ptr()) == 0 }
}

pub fn restriction_active(subkey: &str, name: &str) -> bool {
    if let Some(v) = reg_get_dword(hkcu(), subkey, name) {
        if v != 0 {
            return true;
        }
    }
    if let Some(v) = reg_get_dword(hklm(), subkey, name) {
        if v != 0 {
            return true;
        }
    }
    false
}

pub fn off_restriction(e: &dyn Fn(&str), subkey: &str, name: &str, label: &str) {
    let ok1 = reg_del_value(hkcu(), subkey, name);
    let ok2 = reg_del_value(hklm(), subkey, name);
    if ok1 || ok2 {
        e(&format!("[OK] {label} — разблокировано"));
    } else {
        e(&format!("[—] {label} — ограничение не найдено"));
    }
}

pub fn del_key(e: &dyn Fn(&str), root: HKEY, subkey: &str) {
    let hive = if root == hklm() { "HKLM" } else { "HKCU" };
    if reg_del_tree(root, subkey) {
        e(&format!("[OK] удалён ключ: {hive}\\{subkey}"));
    } else {
        e(&format!("[—] ключа не было: {hive}\\{subkey}"));
    }
}

fn reg_data_to_string(typ: u32, data: &[u8]) -> String {
    match typ {
        1 | 2 => utf16_bytes_to_string(data),
        7 => utf16_bytes_to_string(data),
        4 => {
            if data.len() >= 4 {
                let v = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                v.to_string()
            } else {
                String::new()
            }
        }
        _ => {
            if data.is_empty() {
                String::new()
            } else {
                "(двоичные данные)".to_string()
            }
        }
    }
}

pub fn utf16_bytes_to_string(b: &[u8]) -> String {
    let n = b.len() / 2;
    let mut u = Vec::with_capacity(n);
    for i in 0..n {
        let c = u16::from_le_bytes([b[i * 2], b[i * 2 + 1]]);
        if c == 0 {
            break;
        }
        u.push(c);
    }
    String::from_utf16_lossy(&u)
}

pub fn utf16_bytes(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for c in s.encode_utf16() {
        out.extend_from_slice(&c.to_le_bytes());
    }
    out.extend_from_slice(&[0, 0]);
    out
}

pub fn multi_sz_bytes(value: &str) -> Vec<u8> {
    let v = value.replace("\r\n", "\n");
    let mut out = Vec::new();
    for line in v.split('\n') {
        if line.is_empty() {
            continue;
        }
        for c in line.encode_utf16() {
            out.extend_from_slice(&c.to_le_bytes());
        }
        out.extend_from_slice(&[0, 0]);
    }
    out.extend_from_slice(&[0, 0]);
    out
}

pub fn multi_sz_to_lines(data: &[u8]) -> Vec<String> {
    let n = data.len() / 2;
    let mut u = Vec::with_capacity(n);
    for i in 0..n {
        u.push(u16::from_le_bytes([data[i * 2], data[i * 2 + 1]]));
    }
    let mut out = Vec::new();
    let mut start = 0;
    for i in 0..u.len() {
        if u[i] == 0 {
            if i > start {
                out.push(String::from_utf16_lossy(&u[start..i]));
            }
            start = i + 1;
        }
    }
    out
}

pub fn bytes_to_hex(data: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789ABCDEF";
    let mut s = String::new();
    for (i, b) in data.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xF) as usize] as char);
    }
    s
}

pub fn parse_hex_bytes(s: &str) -> Option<Vec<u8>> {
    let mut digits = Vec::new();
    for c in s.chars() {
        match c {
            '0'..='9' => digits.push(c as u8 - b'0'),
            'a'..='f' => digits.push(c as u8 - b'a' + 10),
            'A'..='F' => digits.push(c as u8 - b'A' + 10),
            ' ' | ',' | '\t' | '\r' | '\n' => {}
            _ => return None,
        }
    }
    if digits.len() % 2 != 0 {
        return None;
    }
    Some(digits.chunks(2).map(|c| (c[0] << 4) | c[1]).collect())
}

pub fn reg_set_typed(root: HKEY, subkey: &str, name: &str, typ: u32, value: &str) -> (bool, String) {
    match typ {
        REG_T_SZ | REG_T_EXPAND_SZ => (reg_set_raw(root, subkey, name, typ, &utf16_bytes(value)), String::new()),
        REG_T_DWORD => match value.trim().parse::<u32>() {
            Ok(n) => (reg_set_raw(root, subkey, name, REG_T_DWORD, &n.to_le_bytes()), String::new()),
            Err(_) => (false, "Для типа DWord впишите число (например 0 или 1).".into()),
        },
        REG_T_QWORD => match value.trim().parse::<u64>() {
            Ok(n) => (reg_set_raw(root, subkey, name, REG_T_QWORD, &n.to_le_bytes()), String::new()),
            Err(_) => (false, "Для типа QWord впишите число.".into()),
        },
        REG_T_MULTI_SZ => (reg_set_raw(root, subkey, name, REG_T_MULTI_SZ, &multi_sz_bytes(value)), String::new()),
        REG_T_BINARY => match parse_hex_bytes(value) {
            Some(b) => (reg_set_raw(root, subkey, name, REG_T_BINARY, &b), String::new()),
            None => (false, "Для типа Binary впишите HEX-байты, например: 01 A0 FF.".into()),
        },
        _ => (reg_set_raw(root, subkey, name, typ, &utf16_bytes(value)), String::new()),
    }
}

pub fn reg_display_for_edit(data: &[u8], typ: u32) -> String {
    match typ {
        REG_T_SZ | REG_T_EXPAND_SZ => utf16_bytes_to_string(data),
        REG_T_DWORD => {
            if data.len() >= 4 {
                u32::from_le_bytes([data[0], data[1], data[2], data[3]]).to_string()
            } else {
                "0".into()
            }
        }
        REG_T_QWORD => {
            if data.len() >= 8 {
                let mut a = [0u8; 8];
                a.copy_from_slice(&data[..8]);
                u64::from_le_bytes(a).to_string()
            } else {
                "0".into()
            }
        }
        REG_T_MULTI_SZ => multi_sz_to_lines(data).join("\r\n"),
        _ => bytes_to_hex(data),
    }
}
