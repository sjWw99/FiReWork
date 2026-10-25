use crate::exec::simple;
use crate::win::*;
use std::sync::atomic::Ordering;
use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Controls::DRAWITEMSTRUCT;

pub const KIND_CARD: i32 = 0;
pub const KIND_ACCENT: i32 = 1;
pub const KIND_CATEGORY: i32 = 2;
pub const KIND_SIMPLE: i32 = 3;
pub const KIND_TAB: i32 = 4;

pub const DT_LEFT: u32 = 0x0000;
pub const DT_CENTER: u32 = 0x0001;
pub const DT_VCENTER: u32 = 0x0004;
pub const DT_SINGLELINE: u32 = 0x0020;
pub const DT_END_ELLIPSIS: u32 = 0x8000;

pub const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}

const G_BG_TOP: u32 = rgb(0xEF, 0xF4, 0xFC);
const G_BG_BOT: u32 = rgb(0xDC, 0xE6, 0xF5);
const G_PANEL: u32 = rgb(0xFF, 0xFF, 0xFF);
const G_PANEL_BD: u32 = rgb(0xD6, 0xE0, 0xEF);
const G_TILE: u32 = rgb(0xFF, 0xFF, 0xFF);
const G_TILE_HOT: u32 = rgb(0xEF, 0xF5, 0xFF);
const G_TILE_BD: u32 = rgb(0xDC, 0xE5, 0xF2);
const G_TILE_HI: u32 = rgb(0xFF, 0xFF, 0xFF);
const G_ICON: u32 = rgb(0xE7, 0xEF, 0xFC);
const G_ACCENT: u32 = rgb(0x0A, 0x84, 0xFF);
const G_ACCENT2: u32 = rgb(0x5E, 0x5C, 0xE6);
const G_ACCENT_HI: u32 = rgb(0x66, 0xB2, 0xFF);
const G_ACCENT_TX: u32 = rgb(0xFF, 0xFF, 0xFF);
const G_TEXT: u32 = rgb(0x1B, 0x25, 0x33);
const G_MUTED: u32 = rgb(0x6A, 0x76, 0x87);

const P_BG: u32 = rgb(0xFF, 0xFF, 0xFF);
const P_PANEL: u32 = rgb(0xF4, 0xF4, 0xF4);
const P_PANEL_BD: u32 = rgb(0xC8, 0xC8, 0xC8);
const P_TILE: u32 = rgb(0xF2, 0xF2, 0xF2);
const P_TILE_HOT: u32 = rgb(0xE6, 0xE6, 0xE6);
const P_TILE_BD: u32 = rgb(0xB6, 0xB6, 0xB6);
const P_ICON: u32 = rgb(0xDD, 0xDD, 0xDD);
const P_ACCENT: u32 = rgb(0x12, 0x5A, 0xA8);
const P_TEXT: u32 = rgb(0x0A, 0x0A, 0x0A);
const P_MUTED: u32 = rgb(0x55, 0x55, 0x55);

pub fn init_theme() {
    unsafe {
        if BG_BRUSH.load(Ordering::Relaxed) == 0 {
            BG_BRUSH.store(CreateSolidBrush(G_BG_BOT) as isize, Ordering::Relaxed);
            WHITE_BRUSH_OBJ.store(CreateSolidBrush(P_BG) as isize, Ordering::Relaxed);
        }
    }
}

fn so(glass: u32, plain: u32) -> u32 {
    if simple() {
        plain
    } else {
        glass
    }
}

pub fn ui_bg_brush() -> isize {
    if simple() {
        WHITE_BRUSH_OBJ.load(Ordering::Relaxed)
    } else {
        BG_BRUSH.load(Ordering::Relaxed)
    }
}
pub fn ui_bg() -> u32 {
    so(G_PANEL, P_BG)
}
pub fn ui_panel() -> u32 {
    so(G_PANEL, P_PANEL)
}
pub fn ui_panel_bd() -> u32 {
    so(G_PANEL_BD, P_PANEL_BD)
}
pub fn ui_text() -> u32 {
    so(G_TEXT, P_TEXT)
}
pub fn ui_muted() -> u32 {
    so(G_MUTED, P_MUTED)
}
pub fn ui_card() -> u32 {
    so(G_TILE, P_TILE)
}
pub fn ui_card_hot() -> u32 {
    so(G_TILE_HOT, P_TILE_HOT)
}
pub fn ui_card_bd() -> u32 {
    so(G_TILE_BD, P_TILE_BD)
}
pub fn ui_card_hi() -> u32 {
    so(G_TILE_HI, rgb(0xFF, 0xFF, 0xFF))
}
pub fn ui_icon() -> u32 {
    so(G_ICON, P_ICON)
}
pub fn ui_accent() -> u32 {
    so(G_ACCENT, P_ACCENT)
}
pub fn ui_accent2() -> u32 {
    so(G_ACCENT2, rgb(0x0E, 0x45, 0x80))
}
pub fn ui_accent_tx() -> u32 {
    so(G_ACCENT_TX, rgb(0xFF, 0xFF, 0xFF))
}
pub fn ui_snow() -> u32 {
    so(G_ACCENT, P_ACCENT)
}

fn lerp(a: u32, b: u32, t: f32) -> u32 {
    let ar = a & 0xFF;
    let ag = (a >> 8) & 0xFF;
    let ab = (a >> 16) & 0xFF;
    let br = b & 0xFF;
    let bg = (b >> 8) & 0xFF;
    let bb = (b >> 16) & 0xFF;
    let r = (ar as f32 + (br as f32 - ar as f32) * t) as u32;
    let g = (ag as f32 + (bg as f32 - ag as f32) * t) as u32;
    let bl = (ab as f32 + (bb as f32 - ab as f32) * t) as u32;
    r | (g << 8) | (bl << 16)
}

pub fn fill_vgrad(hdc: HDC, r: RECT, top: u32, bottom: u32) {
    let h = r.bottom - r.top;
    if h <= 0 {
        return;
    }
    let step = 2;
    let mut y = 0;
    while y < h {
        let t = y as f32 / h as f32;
        let col = lerp(top, bottom, t);
        let band = RECT { left: r.left, top: r.top + y, right: r.right, bottom: r.top + (y + step).min(h) };
        fill_rect_col(hdc, band, col);
        y += step;
    }
}

pub fn paint_background(hdc: HDC, r: RECT) {
    if simple() {
        fill_rect_col(hdc, r, P_BG);
    } else {
        fill_vgrad(hdc, r, G_BG_TOP, G_BG_BOT);
    }
}

// Для подписей, чекбоксов и кнопок: фон текста = цвет фоновой кисти,
// иначе под текстом остаётся белый прямоугольник.
pub fn ctl_color(hdc: isize) -> isize {
    unsafe {
        SetTextColor(hdc as HDC, ui_text());
        SetBkColor(hdc as HDC, so(G_BG_BOT, P_BG));
    }
    ui_bg_brush()
}

// Для полей ввода (EDIT): чистый белый фон — привычный вид «поля для ввода».
pub fn ctl_color_edit(hdc: isize) -> isize {
    unsafe {
        SetTextColor(hdc as HDC, ui_text());
        SetBkColor(hdc as HDC, rgb(0xFF, 0xFF, 0xFF));
    }
    WHITE_BRUSH_OBJ.load(Ordering::Relaxed)
}

pub fn draw_text(hdc: HDC, s: &str, r: RECT, font: isize, color: u32, flags: u32) {
    unsafe {
        if font != 0 {
            SelectObject(hdc, font as HGDIOBJ);
        }
        SetTextColor(hdc, color);
        SetBkMode(hdc, TRANSPARENT as i32);
        let mut u = wide(s);
        let mut rr = r;
        DrawTextW(hdc, u.as_mut_ptr(), -1, &mut rr, flags);
    }
}

pub fn rc(l: i32, t: i32, r: i32, b: i32) -> RECT {
    RECT { left: l, top: t, right: r, bottom: b }
}

pub fn fill_rect_col(hdc: HDC, r: RECT, col: u32) {
    unsafe {
        let b = CreateSolidBrush(col);
        FillRect(hdc, &r, b);
        DeleteObject(b as HGDIOBJ);
    }
}

pub fn fill_round(hdc: HDC, r: RECT, fill: u32, border: u32, radius: i32) {
    unsafe {
        let hbr = CreateSolidBrush(fill);
        let hpen = CreatePen(PS_SOLID, 1, border);
        let ob = SelectObject(hdc, hbr as HGDIOBJ);
        let op = SelectObject(hdc, hpen as HGDIOBJ);
        RoundRect(hdc, r.left, r.top, r.right, r.bottom, radius, radius);
        SelectObject(hdc, ob);
        SelectObject(hdc, op);
        DeleteObject(hbr as HGDIOBJ);
        DeleteObject(hpen as HGDIOBJ);
    }
}

pub fn fill_circle(hdc: HDC, cx: i32, cy: i32, r: i32, fill: u32) {
    unsafe {
        let hbr = CreateSolidBrush(fill);
        let hpen = CreatePen(PS_SOLID, 1, fill);
        let ob = SelectObject(hdc, hbr as HGDIOBJ);
        let op = SelectObject(hdc, hpen as HGDIOBJ);
        Ellipse(hdc, cx - r, cy - r, cx + r, cy + r);
        SelectObject(hdc, ob);
        SelectObject(hdc, op);
        DeleteObject(hbr as HGDIOBJ);
        DeleteObject(hpen as HGDIOBJ);
    }
}

pub fn draw_snowflake(hdc: HDC, cx: i32, cy: i32, r: i32, color: u32) {
    unsafe {
        let hpen = CreatePen(PS_SOLID, 2, color);
        let op = SelectObject(hdc, hpen as HGDIOBJ);
        let d = r * 7 / 10;
        let lines = [
            (cx, cy - r, cx, cy + r),
            (cx - r, cy, cx + r, cy),
            (cx - d, cy - d, cx + d, cy + d),
            (cx + d, cy - d, cx - d, cy + d),
        ];
        for (x1, y1, x2, y2) in lines {
            MoveToEx(hdc, x1, y1, std::ptr::null_mut());
            LineTo(hdc, x2, y2);
        }
        SelectObject(hdc, op);
        DeleteObject(hpen as HGDIOBJ);
    }
}

fn glass_tile(hdc: HDC, r: RECT, fill: u32, border: u32, radius: i32) {
    fill_round(hdc, r, fill, border, radius);
    fill_round(hdc, rc(r.left + 7, r.top + 2, r.right - 7, r.top + 5), ui_card_hi(), ui_card_hi(), 2);
}

fn draw_icon_chip(hdc: HDC, cx: i32, cy: i32, size: i32, bg: u32, flake: u32) {
    let half = size / 2;
    fill_round(hdc, rc(cx - half, cy - half, cx + half, cy + half), bg, bg, 10);
    draw_snowflake(hdc, cx, cy, half - 5, flake);
}

pub fn draw_button_item(lp: isize) {
    let dis = unsafe { &*(lp as *const DRAWITEMSTRUCT) };
    let id = dis.CtlID as i32;
    let info = match BTN_META.lock().unwrap().get(&id).cloned() {
        Some(i) => i,
        None => return,
    };
    let hdc = dis.hDC;
    let r = dis.rcItem;
    let pressed = dis.itemState & 0x0001 != 0;
    let hot = dis.itemState & 0x0040 != 0;
    let radius = if simple() { 6 } else { 16 };

    let f_sub = F_SUB.load(Ordering::Relaxed);
    let f_card = F_CARD.load(Ordering::Relaxed);
    let f_cat = F_CAT.load(Ordering::Relaxed);

    match info.kind {
        x if x == KIND_SIMPLE => {
            let fill = if pressed || hot { ui_card_hot() } else { ui_card() };
            let bd = if pressed || hot { ui_accent() } else { ui_card_bd() };
            glass_tile(hdc, r, fill, bd, 12);
            draw_text(hdc, &info.title, r, f_sub, ui_text(), DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        }
        x if x == KIND_TAB => {
            let active = info.cat_index == AUTO_ACTIVE_TAB.load(Ordering::Relaxed);
            if active {
                fill_round(hdc, r, ui_accent(), ui_accent(), 10);
                fill_round(hdc, rc(r.left + 8, r.top + 2, r.right - 8, r.top + 5), ui_accent_hi(), ui_accent_hi(), 2);
                draw_text(hdc, &info.title, r, f_sub, ui_accent_tx(), DT_CENTER | DT_VCENTER | DT_SINGLELINE);
            } else {
                if hot {
                    fill_round(hdc, r, ui_card(), ui_card_bd(), 10);
                }
                let col = if pressed || hot { ui_text() } else { ui_muted() };
                draw_text(hdc, &info.title, r, f_sub, col, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
            }
        }
        x if x == KIND_CATEGORY => {
            let active = info.cat_index == ACTIVE_CAT.load(Ordering::Relaxed);
            let cy = (r.top + r.bottom) / 2;
            if active {
                fill_round(hdc, r, ui_accent(), ui_accent(), 13);
                fill_round(hdc, rc(r.left + 10, r.top + 3, r.right - 10, r.top + 6), ui_accent_hi(), ui_accent_hi(), 2);
                draw_text(hdc, &info.title, rc(r.left + 22, r.top, r.right - 8, r.bottom), f_cat, ui_accent_tx(), DT_LEFT | DT_VCENTER | DT_SINGLELINE);
            } else {
                if hot {
                    fill_round(hdc, r, ui_card(), ui_card_bd(), 13);
                }
                fill_circle(hdc, r.left + 14, cy, 3, ui_snow());
                let col = if pressed || hot { ui_text() } else { ui_muted() };
                draw_text(hdc, &info.title, rc(r.left + 26, r.top, r.right - 8, r.bottom), f_cat, col, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
            }
        }
        x if x == KIND_ACCENT => {
            let fill = if pressed { ui_accent2() } else { ui_accent() };
            fill_round(hdc, r, fill, fill, radius);
            fill_round(hdc, rc(r.left + 10, r.top + 3, r.right - 10, r.top + 7), ui_accent_hi(), ui_accent_hi(), 3);
            let cy = (r.top + r.bottom) / 2;
            draw_icon_chip(hdc, r.left + 34, cy, 40, ui_accent_soft(), ui_accent_tx());
            draw_text(hdc, &info.title, rc(r.left + 70, r.top + 10, r.right - 16, r.top + 34), f_card, ui_accent_tx(), DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS);
            draw_text(hdc, &info.subtitle, rc(r.left + 70, r.top + 32, r.right - 16, r.bottom - 8), f_sub, ui_accent_tx(), DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS);
        }
        _ => {
            let fill = if pressed || hot { ui_card_hot() } else { ui_card() };
            let bd = if pressed || hot { ui_accent() } else { ui_card_bd() };
            glass_tile(hdc, r, fill, bd, radius);
            if hot || pressed {
                fill_round(hdc, rc(r.left + 2, r.top + 12, r.left + 6, r.bottom - 12), ui_accent(), ui_accent(), 2);
            }
            let cy = r.top + (r.bottom - r.top) / 2;
            draw_icon_chip(hdc, r.left + 32, cy, 38, ui_icon(), ui_snow());
            draw_text(hdc, &info.title, rc(r.left + 64, r.top + 13, r.right - 12, r.top + 37), f_card, ui_text(), DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS);
            draw_text(hdc, &info.subtitle, rc(r.left + 64, r.top + 38, r.right - 12, r.bottom - 10), f_sub, ui_muted(), DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS);
        }
    }
}

fn ui_accent_hi() -> u32 {
    so(G_ACCENT_HI, rgb(0x4C, 0x8C, 0xCE))
}
fn ui_accent_soft() -> u32 {
    so(rgb(0x3C, 0x9C, 0xFF), rgb(0x3A, 0x72, 0xB8))
}
