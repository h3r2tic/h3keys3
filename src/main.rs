#![windows_subsystem = "windows"]
extern crate kernel32;
extern crate user32;
extern crate winapi;
extern crate winrt;
use kernel32::GetModuleHandleA;
use winapi::shared::minwindef::*;
use winapi::shared::ntdef::LPCSTR;
use winapi::shared::windef::{HBRUSH, HCURSOR, HICON, HMENU, HWND, POINT, RECT};
use winapi::um::winuser;

use winrt::windows::data::xml::dom::*;
use winrt::windows::ui::notifications::*;
use winrt::*;

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::{f32, mem, ptr, str, thread, time};

const ESCAPE: char = winuser::VK_ESCAPE as u8 as char;
const SEMICOLON: char = winuser::VK_OEM_1 as u8 as char;
const TILDE: char = winuser::VK_OEM_3 as u8 as char;
const UK_TILDE: char = winuser::VK_OEM_5 as u8 as char;
const ALT_GR: char = winuser::VK_RMENU as u8 as char;
const RIGHTCTRL: char = winuser::VK_RCONTROL as u8 as char;
const LEFTCTRL: char = winuser::VK_LCONTROL as u8 as char;
const CTRL: char = winuser::VK_CONTROL as u8 as char;
const LEFTALT: char = winuser::VK_LMENU as u8 as char;
const ALT: char = winuser::VK_MENU as u8 as char;
const BACKSPACE: char = winuser::VK_BACK as u8 as char;
const COMMA: char = winuser::VK_OEM_COMMA as u8 as char;
const PERIOD: char = winuser::VK_OEM_PERIOD as u8 as char;
const FWD_SLASH: char = winuser::VK_OEM_2 as u8 as char;
const MINUS: char = winuser::VK_OEM_MINUS as u8 as char;
const PLUS: char = winuser::VK_OEM_PLUS as u8 as char;

// Used to distinguish input events generated by this app, and avoid recursion in input generation
const H3KEYS_MAGIC: usize = 666;

#[derive(PartialEq)]
enum KeyAction {
    Down(i32),
    Up(i32),
}

#[derive(PartialEq)]
enum RemapTarget {
    BlindKey(i32),
    KeySeq(std::vec::Vec<KeyAction>),
    Block,
}

trait VirtualKey: Copy {
    fn as_i32(&self) -> i32;
}

impl VirtualKey for i32 {
    fn as_i32(&self) -> i32 {
        *self
    }
}

impl VirtualKey for char {
    fn as_i32(&self) -> i32 {
        *self as u8 as i32
    }
}

fn key_down<T: VirtualKey>(kv: T) -> KeyAction {
    KeyAction::Down(kv.as_i32())
}

fn key_up<T: VirtualKey>(kv: T) -> KeyAction {
    KeyAction::Up(kv.as_i32())
}

fn key<T: VirtualKey>(kv: T) -> RemapTarget {
    RemapTarget::BlindKey(kv.as_i32())
}

fn mod_key<T1: VirtualKey, T2: VirtualKey>(mod_key: T1, kv: T2) -> RemapTarget {
    RemapTarget::KeySeq(vec![
        key_down(mod_key),
        key_down(kv),
        key_up(kv),
        key_up(mod_key),
    ])
}

fn ctrl_key<T: VirtualKey>(kv: T) -> RemapTarget {
    mod_key(winuser::VK_CONTROL, kv)
}

fn shift_key<T: VirtualKey>(kv: T) -> RemapTarget {
    mod_key(winuser::VK_SHIFT, kv)
}

fn alt_key<T: VirtualKey>(kv: T) -> RemapTarget {
    mod_key(winuser::VK_MENU, kv)
}

fn no_ctrl_key<T: VirtualKey>(kv: T) -> RemapTarget {
    RemapTarget::KeySeq(vec![
        key_up(winuser::VK_CONTROL),
        key_down(kv),
        key_up(kv),
        key_down(winuser::VK_CONTROL),
    ])
}

fn remap_colemak(vk: u8) -> i32 {
    let res = match vk as char {
        'E' => 'F',
        'R' => 'P',
        'T' => 'G',
        'Y' => 'J',
        'U' => 'L',
        'I' => 'U',
        'O' => 'Y',
        'S' => 'R',
        'G' => 'D',
        'J' => 'N',
        'K' => 'E',
        'L' => 'I',
        'N' => 'K',
        'P' => SEMICOLON,
        'D' => 'S',
        'F' => 'T',
        SEMICOLON => 'O',
        _ => '\0',
    };
    res as u8 as i32
}

fn get_window_under_cursor(cursor_pos: (i32, i32)) -> HWND {
    unsafe {
        let w = winuser::WindowFromPoint(POINT {
            x: cursor_pos.0,
            y: cursor_pos.1,
        });
        let w = winuser::GetAncestor(w, 2 /* GA_ROOT */);

        let win_name = [0u8; 256];
        winuser::GetWindowTextA(w, mem::transmute(&win_name), 256);

        let nul_pos = win_name.iter().position(|&x| x == 0u8);
        let nul_pos = match nul_pos {
            Some(pos) => pos,
            None => return ptr::null_mut(),
        };

        let win_name = str::from_utf8(&win_name[..nul_pos]);

        match win_name {
            // Let's not move the Desktop...
            Ok("Program Manager") => return ptr::null_mut(),
            Ok(name) => name,
            Err(_) => return ptr::null_mut(),
        };

        w
    }
}

fn get_window_rect(hwnd: HWND) -> RECT {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe {
        winuser::GetWindowRect(hwnd, &mut rect);
    }
    rect
}

struct ScrollEmuState {
    scroll_emu_on: bool,
    scroll_emu_from: (i32, i32),
    scroll_emu_acc: (f32, f32),
}

struct InputHookState {
    colemak_on: bool,

    mod1_on: bool,
    mod2_on: bool,
    ctrlmod_on: bool,
    winkey_on: bool,
    admin_on: bool,

    leftalt_on: bool,
    leftctrl_on: bool,

    window_move_hwnd: HWND,
    mouse_move_from: (i32, i32),
    window_move_from: (i32, i32),

    window_resize_hwnd: HWND,
    mouse_resize_from: (i32, i32),
    window_resize_from: (i32, i32),

    scroll_emu_state: Arc<Mutex<ScrollEmuState>>,

    mod1_keys_down: HashSet<i32>,
}

impl InputHookState {
    fn new() -> InputHookState {
        InputHookState {
            colemak_on: true,

            mod1_on: false,
            mod2_on: false,
            ctrlmod_on: false,
            winkey_on: false,
            admin_on: false, // global option control mode, colemak enable/disable, etc.

            leftalt_on: false,
            leftctrl_on: false,

            window_move_hwnd: ptr::null_mut(),
            mouse_move_from: (0, 0),
            window_move_from: (0, 0),

            window_resize_hwnd: ptr::null_mut(),
            mouse_resize_from: (0, 0),
            window_resize_from: (0, 0),

            scroll_emu_state: Arc::new(Mutex::new(ScrollEmuState::new())),

            mod1_keys_down: HashSet::new(),
        }
    }

    fn extended_key_flag(key: i32) -> DWORD {
        match key {
            winuser::VK_UP | winuser::VK_DOWN | winuser::VK_LEFT | winuser::VK_RIGHT => {
                winuser::KEYEVENTF_EXTENDEDKEY
            }
            _ => 0,
        }
    }

    fn send_key(key: u8, down: bool) {
        unsafe {
            let mut input = winuser::INPUT {
                type_: winuser::INPUT_KEYBOARD,
                u: mem::uninitialized(),
            };

            let ext_flag = Self::extended_key_flag(key as _);
            let scancode = winuser::MapVirtualKeyA(key as u32, winuser::MAPVK_VK_TO_VSC) as u16;

            *input.u.ki_mut() = winuser::KEYBDINPUT {
                wVk: key as u16,
                wScan: scancode,
                dwFlags: ext_flag | if down { 0 } else { winuser::KEYEVENTF_KEYUP },
                time: 0,
                dwExtraInfo: H3KEYS_MAGIC,
            };

            winuser::SendInput(1, &mut input, mem::size_of::<winuser::INPUT>() as i32);
        }

        //unsafe { winuser::keybd_event(key, 0, if down {0} else {winuser::KEYEVENTF_KEYUP}, H3KEYS_MAGIC); }
    }

    fn key_hook(&mut self, code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if winuser::HC_ACTION == code {
            let input_key = unsafe { *(lparam as winuser::PKBDLLHOOKSTRUCT) };
            if input_key.dwExtraInfo == H3KEYS_MAGIC {
                return unsafe { winuser::CallNextHookEx(ptr::null_mut(), code, wparam, lparam) };
            }

            let key_pressed =
                winuser::WM_KEYDOWN == wparam as u32 || winuser::WM_SYSKEYDOWN == wparam as u32;
            let key_released =
                winuser::WM_KEYUP == wparam as u32 || winuser::WM_SYSKEYUP == wparam as u32;

            let down_only = |rt: RemapTarget| {
                if key_pressed {
                    rt
                } else {
                    key(0)
                }
            };

            if key_pressed {
                //println!("key: 0x{:x}", input_key.vkCode);
            }

            if key_pressed || key_released {
                // Enable caps-lock layer
                if winuser::VK_CAPITAL == input_key.vkCode as i32 {
                    // If disabling, make sure all remapped keys get released
                    if key_released {
                        for &key in self.mod1_keys_down.iter() {
                            Self::send_key(key as u8, false);
                        }
                        self.mod1_keys_down.clear();
                        self.ctrlmod_on = false;
                        self.window_move_hwnd = ptr::null_mut();
                        self.window_resize_hwnd = ptr::null_mut();
                        self.admin_on = false;

                        let scroll = &mut self.scroll_emu_state.lock().unwrap();
                        scroll.scroll_emu_on = false;
                    }

                    self.mod1_on = key_pressed;
                    return 1;
                }

                // Enable pipe/backslash layer
                if winuser::VK_OEM_102 == input_key.vkCode as i32 {
                    self.mod2_on = key_pressed;
                    return 1;
                }

                // Colemak
                let remap = if self.colemak_on {
                    key(remap_colemak(input_key.vkCode as u8))
                } else {
                    key(0)
                };

                // Windows keys
                let remap = match input_key.vkCode as u8 as char {
                    TILDE => key(winuser::VK_ESCAPE),
                    UK_TILDE => key(TILDE),
                    ALT_GR => {
                        self.winkey_on = key_pressed;
                        key(winuser::VK_LWIN)
                    }
                    LEFTALT => {
                        self.leftalt_on = key_pressed;
                        remap
                    }
                    LEFTCTRL => {
                        self.leftctrl_on = key_pressed;
                        remap
                    }
                    BACKSPACE => {
                        if key_pressed && self.leftalt_on && self.leftctrl_on {
                            unsafe {
                                let top_window = winuser::GetForegroundWindow();
                                let mut pid: DWORD = 0;
                                winuser::GetWindowThreadProcessId(top_window, &mut pid);

                                let h = kernel32::OpenProcess(
                                    winapi::um::winnt::PROCESS_ALL_ACCESS,
                                    0,
                                    pid,
                                );
                                if h != ptr::null_mut() {
                                    kernel32::TerminateProcess(h, 0);
                                }
                            }
                        }
                        remap
                    }
                    RIGHTCTRL => key(winuser::VK_APPS),
                    'U' => {
                        if self.winkey_on && key_pressed {
                            // We will not register a key-up due to the lock screen
                            self.winkey_on = false;
                            unsafe {
                                winuser::LockWorkStation();
                            }
                            RemapTarget::Block
                        } else {
                            remap
                        }
                    }
                    '4' => {
                        if self.winkey_on && key_pressed {
                            down_only(alt_key(winuser::VK_F4))
                        } else {
                            remap
                        }
                    }
                    'M' => {
                        if self.winkey_on && key_pressed {
                            key('D')
                        } else {
                            remap
                        }
                    }
                    _ => remap,
                };

                let remap = if self.mod1_on {
                    // Caps-lock layer

                    let mapped_key = match input_key.vkCode as u8 as char {
                        ESCAPE => {
                            self.admin_on = key_pressed;
                            RemapTarget::Block
                        }
                        ' ' => {
                            if self.admin_on {
                                if key_released {
                                    toast_notification("Program terminated");
                                    std::process::exit(0);
                                } else {
                                    RemapTarget::Block
                                }
                            } else {
                                key(winuser::VK_SPACE)
                            }
                        }
                        'D' => key(winuser::VK_SHIFT),
                        'F' => {
                            self.ctrlmod_on = key_pressed;
                            key(winuser::VK_CONTROL)
                        }
                        'J' => key(winuser::VK_LEFT),
                        'L' => key(winuser::VK_RIGHT),
                        'U' => key(winuser::VK_HOME),
                        'O' => key(winuser::VK_END),
                        'H' => key(winuser::VK_BACK),
                        '1' => key(winuser::VK_F1),
                        '2' => key(winuser::VK_F2),
                        '3' => key(winuser::VK_F3),
                        '4' => key(winuser::VK_F4),
                        '5' => key(winuser::VK_F5),
                        '6' => key(winuser::VK_F6),
                        '7' => key(winuser::VK_F7),
                        '8' => key(winuser::VK_F8),
                        '9' => key(winuser::VK_F9),
                        '0' => key(winuser::VK_F10),
                        MINUS => key(winuser::VK_F11),
                        PLUS => key(winuser::VK_F12),
                        'N' => down_only(ctrl_key('Z')),
                        'M' => down_only(ctrl_key('Y')),
                        'C' => {
                            if self.admin_on {
                                if key_pressed {
                                    self.colemak_on = !self.colemak_on;
                                    toast_notification(if self.colemak_on {
                                        "Colemak"
                                    } else {
                                        "Qwerty"
                                    });
                                }

                                RemapTarget::Block
                            } else {
                                down_only(ctrl_key('C'))
                            }
                        }
                        'X' => down_only(ctrl_key('X')),
                        'V' => down_only(ctrl_key('V')),
                        'S' => down_only(ctrl_key('S')),
                        SEMICOLON => key(winuser::VK_RETURN),
                        'P' => key(winuser::VK_DELETE),
                        COMMA => down_only(shift_key('7')),
                        PERIOD => down_only(shift_key(winuser::VK_OEM_5)),
                        FWD_SLASH => down_only(key(winuser::VK_OEM_5)),
                        // caps-i is up
                        // caps-ctrl-i is page up
                        'I' => {
                            if self.ctrlmod_on {
                                down_only(no_ctrl_key(winuser::VK_PRIOR))
                            } else {
                                key(winuser::VK_UP)
                            }
                        }
                        // caps-key is down
                        // caps-ctrl-key is page down
                        'K' => {
                            if self.ctrlmod_on {
                                down_only(no_ctrl_key(winuser::VK_NEXT))
                            } else {
                                key(winuser::VK_DOWN)
                            }
                        }
                        ALT_GR => {
                            self.winkey_on = key_pressed;
                            key(winuser::VK_LWIN)
                        }
                        LEFTALT => key(0), // pass-through
                        ALT => key(0),     // pass-through
                        CTRL => key(0),    // pass-through
                        _ => RemapTarget::Block,
                    };

                    if let RemapTarget::BlindKey(k) = mapped_key {
                        if key_pressed {
                            self.mod1_keys_down.insert(k);
                        } else {
                            self.mod1_keys_down.remove(&k);
                        }
                    }

                    mapped_key
                } else if self.mod2_on {
                    // Pipe/backslash layer

                    match input_key.vkCode as u8 as char {
                        ' ' => key(winuser::VK_SPACE),
                        // h _
                        'H' => down_only(shift_key(MINUS)),
                        // jk ()
                        'J' => down_only(shift_key('9')),
                        'K' => down_only(shift_key('0')),
                        // io []
                        'I' => down_only(key(winuser::VK_OEM_4)),
                        'O' => down_only(key(winuser::VK_OEM_6)),
                        // l; {}
                        'L' => down_only(shift_key(winuser::VK_OEM_4)),
                        SEMICOLON => down_only(shift_key(winuser::VK_OEM_6)),
                        // yu -+
                        'Y' => down_only(key(MINUS)),
                        'U' => down_only(shift_key(PLUS)),
                        // m =
                        'M' => down_only(key(PLUS)),
                        // . /*
                        PERIOD => down_only(RemapTarget::KeySeq(vec![
                            key_down(winuser::VK_OEM_2),
                            key_up(winuser::VK_OEM_2),
                            key_down(winuser::VK_SHIFT),
                            key_down('8'),
                            key_up('8'),
                            key_up(winuser::VK_SHIFT),
                        ])),
                        FWD_SLASH => down_only(RemapTarget::KeySeq(vec![
                            key_down(winuser::VK_SHIFT),
                            key_down('8'),
                            key_up('8'),
                            key_up(winuser::VK_SHIFT),
                            key_down(winuser::VK_OEM_2),
                            key_up(winuser::VK_OEM_2),
                        ])),
                        _ => RemapTarget::Block,
                    }
                } else {
                    remap
                };

                if remap != key(0) {
                    match remap {
                        RemapTarget::BlindKey(key) => {
                            Self::send_key(key as u8, key_pressed);
                        }
                        RemapTarget::KeySeq(kseq) => {
                            for key_action in kseq.iter() {
                                match key_action {
                                    &KeyAction::Down(key) => Self::send_key(key as u8, true),
                                    &KeyAction::Up(key) => Self::send_key(key as u8, false),
                                }
                            }
                        }
                        RemapTarget::Block => (),
                    }

                    return 1;
                }
            }
        }

        return unsafe { winuser::CallNextHookEx(ptr::null_mut(), code, wparam, lparam) };
    }

    fn mouse_hook(&mut self, code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if winuser::HC_ACTION == code && self.mod1_on {
            let mouse_data = unsafe { *(lparam as winuser::PMSLLHOOKSTRUCT) };

            // Window move
            if winuser::WM_LBUTTONDOWN == wparam as u32 {
                self.mouse_move_from = (mouse_data.pt.x, mouse_data.pt.y);
                self.window_move_hwnd = get_window_under_cursor(self.mouse_move_from);
                if self.window_move_hwnd != ptr::null_mut() {
                    let rect = get_window_rect(self.window_move_hwnd);
                    self.window_move_from = (rect.left, rect.top);
                }

                return 1;
            }

            if winuser::WM_LBUTTONUP == wparam as u32 {
                self.window_move_hwnd = ptr::null_mut();
                return 1;
            }

            if winuser::WM_MOUSEMOVE == wparam as u32 && self.window_move_hwnd != ptr::null_mut() {
                let x = self.window_move_from.0 + mouse_data.pt.x - self.mouse_move_from.0;
                let y = self.window_move_from.1 + mouse_data.pt.y - self.mouse_move_from.1;

                unsafe {
                    winuser::SetWindowPos(
                        self.window_move_hwnd,
                        ptr::null_mut(),
                        x,
                        y,
                        0,
                        0,
                        winuser::SWP_NOACTIVATE
                            | winuser::SWP_NOOWNERZORDER
                            | winuser::SWP_NOSIZE
                            | winuser::SWP_NOZORDER,
                    );
                }
            }

            // Window resize
            if winuser::WM_RBUTTONDOWN == wparam as u32 {
                self.mouse_resize_from = (mouse_data.pt.x, mouse_data.pt.y);
                self.window_resize_hwnd = get_window_under_cursor(self.mouse_resize_from);
                if self.window_resize_hwnd != ptr::null_mut() {
                    let rect = get_window_rect(self.window_resize_hwnd);
                    self.window_resize_from = (rect.right - rect.left, rect.bottom - rect.top);
                }

                return 1;
            }

            if winuser::WM_RBUTTONUP == wparam as u32 {
                self.window_resize_hwnd = ptr::null_mut();
                return 1;
            }

            if winuser::WM_MOUSEMOVE == wparam as u32 && self.window_resize_hwnd != ptr::null_mut()
            {
                let x = self.window_resize_from.0 + mouse_data.pt.x - self.mouse_resize_from.0;
                let y = self.window_resize_from.1 + mouse_data.pt.y - self.mouse_resize_from.1;

                unsafe {
                    winuser::SetWindowPos(
                        self.window_resize_hwnd,
                        ptr::null_mut(),
                        0,
                        0,
                        x,
                        y,
                        winuser::SWP_NOACTIVATE
                            | winuser::SWP_NOOWNERZORDER
                            | winuser::SWP_NOMOVE
                            | winuser::SWP_NOZORDER,
                    );
                }
            }

            // Scroll emulation
            if 1 == self
                .scroll_emu_state
                .lock()
                .unwrap()
                .mouse_hook(wparam, mouse_data)
            {
                return 1;
            }
        }

        return unsafe { winuser::CallNextHookEx(ptr::null_mut(), code, wparam, lparam) };
    }
}

impl ScrollEmuState {
    fn new() -> ScrollEmuState {
        ScrollEmuState {
            scroll_emu_on: false,
            scroll_emu_from: (0, 0),
            scroll_emu_acc: (0f32, 0f32),
        }
    }

    fn mouse_hook(&mut self, wparam: WPARAM, mouse_data: winuser::MSLLHOOKSTRUCT) -> LRESULT {
        if winuser::WM_MBUTTONDOWN == wparam as u32 {
            self.scroll_emu_from = (mouse_data.pt.x, mouse_data.pt.y);
            self.scroll_emu_acc = (0f32, 0f32);
            self.scroll_emu_on = true;
            return 1;
        }

        if winuser::WM_MBUTTONUP == wparam as u32 {
            self.scroll_emu_on = false;
            return 1;
        }

        if winuser::WM_MOUSEMOVE == wparam as u32 && self.scroll_emu_on {
            let hscroll = (mouse_data.pt.x - self.scroll_emu_from.0) as f32;
            let vscroll = (self.scroll_emu_from.1 - mouse_data.pt.y) as f32;

            // Dead zone
            //let hscroll = if hscroll * hscroll > 1f32 { hscroll } else { 0f32 };
            //let vscroll = if vscroll * vscroll > 1f32 { vscroll } else { 0f32 };

            // Curve
            let hscroll = hscroll.signum() * hscroll.abs().powf(1.5f32);
            let vscroll = vscroll.signum() * vscroll.abs().powf(1.5f32);

            // Blend
            let t = 0.3f32;
            self.scroll_emu_acc.0 = self.scroll_emu_acc.0 * (1.0f32 - t) + hscroll * t;
            self.scroll_emu_acc.1 = self.scroll_emu_acc.1 * (1.0f32 - t) + vscroll * t;

            return 1;
        }

        0
    }

    fn emulate_scroll(&mut self) -> Box<dyn Fn()> {
        if self.scroll_emu_on {
            let decay = 0.92f32;
            self.scroll_emu_acc = (self.scroll_emu_acc.0 * decay, self.scroll_emu_acc.1 * decay);
        }

        let scroll_from = self.scroll_emu_from;
        let scroll_acc = if self.scroll_emu_on {
            self.scroll_emu_acc
        } else {
            (0f32, 0f32)
        };

        // Defer winapi usage so that we can bring it outside of the mutex in the calling code
        Box::new(move || {
            if scroll_acc.0 as i32 != 0 {
                unsafe {
                    winuser::mouse_event(
                        winuser::MOUSEEVENTF_HWHEEL,
                        scroll_from.0 as u32,
                        scroll_from.1 as u32,
                        scroll_acc.0 as i32 as u32,
                        H3KEYS_MAGIC,
                    );
                }
            }

            if scroll_acc.1 as i32 != 0 {
                unsafe {
                    winuser::mouse_event(
                        winuser::MOUSEEVENTF_WHEEL,
                        scroll_from.0 as u32,
                        scroll_from.1 as u32,
                        scroll_acc.1 as i32 as u32,
                        H3KEYS_MAGIC,
                    );
                }
            }
        })
    }
}

static mut HOOK_STATE: Option<InputHookState> = None;

unsafe extern "system" fn global_key_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if let Some(hook_state) = HOOK_STATE.as_mut() {
        hook_state.key_hook(code, wparam, lparam)
    } else {
        0
    }
}

unsafe extern "system" fn global_mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if let Some(hook_state) = HOOK_STATE.as_mut() {
        hook_state.mouse_hook(code, wparam, lparam)
    } else {
        0
    }
}

pub unsafe extern "system" fn win_proc(
    h_wnd: HWND,
    msg: UINT,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if msg == winuser::WM_DESTROY {
        winuser::PostQuitMessage(0);
    }
    return winuser::DefWindowProcW(h_wnd, msg, w_param, l_param);
}

fn main() {
    let rt = RuntimeContext::init();
    run();
    rt.uninit();
}

thread_local! {
    static TOAST_NOTIFIER : RefCell<winrt::ComPtr<ToastNotifier>> =
        RefCell::new(ToastNotificationManager::create_toast_notifier_with_id(
            // Use PowerShell's App ID to circumvent the need to register one.
            &FastHString::new("{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe")
        ).unwrap());

    static PREVIOUS_TOAST: RefCell<Option<ComPtr<ToastNotification>>> = RefCell::new(None);
}

fn toast_notification(content: &str) {
    TOAST_NOTIFIER.with(|toast_notifier| {
        let toast_notifier = &*toast_notifier.borrow();

        PREVIOUS_TOAST.with(|prev_toast| {
            let prev_toast = &mut *prev_toast.borrow_mut();

            // If there's any previous toast, hide it right away.
            let should_hide_previous = if let &mut Some(ref toast) = prev_toast {
                unsafe {
                    toast_notifier.hide(toast).ok();
                }
                true
            } else {
                false
            };

            if should_hide_previous {
                *prev_toast = None;
            }

            unsafe {
                // Get a toast XML template
                let toast_xml =
                    ToastNotificationManager::get_template_content(ToastTemplateType::ToastText02)
                        .unwrap();

                // Fill in the text elements
                let toast_text_elements = toast_xml
                    .get_elements_by_tag_name(&FastHString::new("text"))
                    .unwrap();

                toast_text_elements
                    .item(0)
                    .unwrap()
                    .append_child(
                        &*toast_xml
                            .create_text_node(&FastHString::new("h3keys"))
                            .unwrap()
                            .query_interface::<IXmlNode>()
                            .unwrap(),
                    )
                    .unwrap();
                toast_text_elements
                    .item(1)
                    .unwrap()
                    .append_child(
                        &*toast_xml
                            .create_text_node(&FastHString::new(content))
                            .unwrap()
                            .query_interface::<IXmlNode>()
                            .unwrap(),
                    )
                    .unwrap();

                // Create the toast and attach event listeners
                let toast = ToastNotification::create_toast_notification(&*toast_xml).unwrap();

                // Show the toast
                (*toast_notifier).show(&*toast).unwrap();

                // Save it for next time, so we can hide it quickly
                *prev_toast = Some(toast);
            }
        });
    });
}

fn run() {
    unsafe {
        HOOK_STATE = Some(InputHookState::new());
        kernel32::SetThreadPriority(
            kernel32::GetCurrentThread(),
            1, /* THREAD_PRIORITY_ABOVE_NORMAL */
        );
    }

    {
        let scroll_state = unsafe { HOOK_STATE.as_mut().unwrap().scroll_emu_state.clone() };

        thread::spawn(move || loop {
            let run_scroll_actions = scroll_state.lock().unwrap().emulate_scroll();
            run_scroll_actions();
            thread::sleep(time::Duration::from_millis(10));
        });
    }

    unsafe {
        winuser::SetWindowsHookExA(
            winuser::WH_KEYBOARD_LL,
            Some(global_key_hook),
            GetModuleHandleA(ptr::null()) as HINSTANCE,
            0,
        );

        winuser::SetWindowsHookExA(
            winuser::WH_MOUSE_LL,
            Some(global_mouse_hook),
            GetModuleHandleA(ptr::null()) as HINSTANCE,
            0,
        );
    }

    let class_name = "h3keys3";
    let wnd_class = winuser::WNDCLASSA {
        style: 0,
        lpfnWndProc: Some(win_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: 0 as HINSTANCE,
        hIcon: 0 as HICON,
        hCursor: 0 as HCURSOR,
        hbrBackground: 16 as HBRUSH,
        lpszMenuName: 0 as LPCSTR,
        lpszClassName: class_name.as_ptr() as *const i8,
    };

    if 0 == unsafe { winuser::RegisterClassA(&wnd_class) } {
        panic!("RegisterClassA failed.");
    }

    let hwnd = unsafe {
        winuser::CreateWindowExA(
            0,
            class_name.as_ptr() as *const i8,
            class_name.as_ptr() as *const i8,
            0,
            winuser::CW_USEDEFAULT,
            winuser::CW_USEDEFAULT,
            320,
            240,
            winuser::GetDesktopWindow(),
            0 as HMENU,
            0 as HINSTANCE,
            std::ptr::null_mut(),
        )
    };

    let mut msg = winuser::MSG {
        hwnd: 0 as HWND,
        message: 0 as UINT,
        wParam: 0 as WPARAM,
        lParam: 0 as LPARAM,
        time: 0 as DWORD,
        pt: POINT { x: 0, y: 0 },
    };

    loop {
        unsafe {
            let pm = winuser::GetMessageW(&mut msg, hwnd, 0, 0);
            if pm > 0 {
                winuser::TranslateMessage(&mut msg);
                winuser::DispatchMessageW(&mut msg);
            }
        }
    }
}
