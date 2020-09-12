#![windows_subsystem = "windows"]

#[cfg(target_os = "windows")]
extern crate winapi;

#[cfg(target_os = "windows")]
extern crate user32;

#[cfg(target_os = "windows")]
extern crate kernel32;

#[cfg(target_os = "windows")]
extern crate winrt;

#[cfg(target_os = "windows")]
mod win;

#[cfg(target_os = "linux")]
extern crate uinput;

#[cfg(target_os = "linux")]
extern crate evdev;

#[cfg(target_os = "linux")]
extern crate uinput_sys;

#[cfg(target_os = "linux")]
mod linux;
use linux::*;

mod remap;
use remap::*;

use std::collections::HashMap;
use std::{thread, time};

pub trait KeyboardDevice {
    fn send_key(&mut self, key: u8, event: KeyEvent);
}

#[derive(Default)]
struct InputHookState {
    remap_state: RemapState,
    os_state: OsState,
    keys_down: HashMap<u32, RemapTarget>,
}

impl InputHookState {
    /*fn new() -> InputHookState {
        InputHookState {
            colemak_on: true,

            mod1_on: false,
            mod2_on: false,
            ctrlmod_on: false,
            winkey_on: false,
            admin_on: false,	// global option control mode, colemak enable/disable, etc.

            leftalt_on: false,
            leftctrl_on: false,

            window_move_hwnd: ptr::null_mut(),
            mouse_move_from: (0, 0),
            window_move_from: (0, 0),

            window_resize_hwnd: ptr::null_mut(),
            mouse_resize_from: (0, 0),
            window_resize_from: (0, 0),

            scroll_emu_state: Arc::new(Mutex::new(ScrollEmuState::new())),

            mod1_keys_down : HashSet::new(),
        }
    }*/

    fn apply_side_effects(&mut self, side_effects: Vec<RemapSideEffect>) {
        for side_effect in side_effects {
            match side_effect {
                RemapSideEffect::KillTopWindowProcess => {
                    kill_top_window_process();
                }
                RemapSideEffect::LockWorkstation => {
                    lock_workstation();
                }
                RemapSideEffect::Notification(text) => {
                    toast_notification(&text);
                }
                RemapSideEffect::SetModifier(m, v) => {
                    self.remap_state.set_mod(m, v);
                }
                RemapSideEffect::Terminate => {
                    std::process::exit(0);
                }
            }
        }
    }

    fn execute_key_remap(
        device: &mut dyn KeyboardDevice,
        key_event: KeyEvent,
        remap: &RemapTarget,
    ) {
        match remap {
            RemapTarget::BlindKey(key) => {
                device.send_key(*key as u8, key_event);
            }
            RemapTarget::KeySeq(kseq) => {
                for key_action in kseq.iter() {
                    match key_action {
                        &KeyAction::Down(key) => device.send_key(key as u8, KeyEvent::Down),
                        &KeyAction::Up(key) => device.send_key(key as u8, KeyEvent::Up),
                    }
                }
            }
            RemapTarget::Block => (),
        }
    }

    fn key_hook(&mut self, device: &mut dyn KeyboardDevice, key_event: KeyEvent, vk: u32) -> u32 {
        //println!("key_hook {:?} {}", key_event, vk);

        let key_pressed_or_held = if let KeyEvent::Up = key_event {
            false
        } else {
            true
        };

        // Enable caps-lock layer
        if CAPS_LOCK == vk as u8 as char {
            let remap_state_before = self.remap_state.clone();

            if let KeyEvent::Up = key_event {
                self.remap_state.modifiers.remove(&Modifier::Ctrl);
                self.remap_state.modifiers.remove(&Modifier::Admin);

                self.os_state.on_caps_layer_enable();
            }

            self.remap_state
                .set_mod(Modifier::Mod1, key_pressed_or_held);

            self.on_remap_state_changed(device, remap_state_before);

            return 1;
        }

        // Enable pipe/backslash layer
        if OEM_102 == vk as u8 as char {
            let remap_state_before = self.remap_state.clone();

            self.remap_state
                .set_mod(Modifier::Mod2, key_pressed_or_held);

            self.on_remap_state_changed(device, remap_state_before);

            return 1;
        }

        let (remap, side_effects) = remap_key(&self.remap_state, key_event, vk);

        // 1 if the key should be blocked (remapped)
        let mut return_val = 0;

        if let Some(remap) = &remap {
            return_val = 1;
            Self::execute_key_remap(device, key_event, remap);
        }

        if KeyEvent::Up == key_event {
            self.keys_down.remove(&vk);
        } else {
            self.keys_down.insert(
                vk,
                remap.unwrap_or_else(|| RemapTarget::BlindKey(vk as i32)),
            );
        }

        let remap_state_before = self.remap_state.clone();
        self.apply_side_effects(side_effects);
        self.on_remap_state_changed(device, remap_state_before);

        return return_val;
    }

    fn on_remap_state_changed(
        &mut self,
        device: &mut dyn KeyboardDevice,
        remap_state_before: RemapState,
    ) {
        if self.remap_state != remap_state_before {
            let mut keys_undone = Vec::new();

            for (&vk, old_remap) in self.keys_down.iter_mut() {
                let new_remap = remap_key(&self.remap_state, KeyEvent::Repeat, vk)
                    .0
                    .unwrap_or_else(|| RemapTarget::BlindKey(vk as i32));

                if new_remap != *old_remap {
                    // A modifier was pressed/released, and now one of the held keys has changed its meaning. Undo it.

                    // Don't try do "undo" key sequences though.
                    if let RemapTarget::KeySeq(_) = old_remap {
                    } else {
                        Self::execute_key_remap(device, KeyEvent::Up, old_remap);
                        //Self::execute_key_remap(device, KeyEvent::Down, &new_remap);
                    }

                    *old_remap = new_remap;
                    keys_undone.push(vk);
                }
            }

            for vk in keys_undone {
                self.keys_down.remove(&vk);
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn win_key_hook(&mut self, code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if winuser::HC_ACTION == code {
            let input_key = unsafe { *(lparam as winuser::PKBDLLHOOKSTRUCT) };
            if input_key.dwExtraInfo == H3KEYS_MAGIC {
                return unsafe { winuser::CallNextHookEx(ptr::null_mut(), code, wparam, lparam) };
            }

            let key_pressed =
                winuser::WM_KEYDOWN == wparam as u32 || winuser::WM_SYSKEYDOWN == wparam as u32;
            let key_released =
                winuser::WM_KEYUP == wparam as u32 || winuser::WM_SYSKEYUP == wparam as u32;

            if key_pressed {
                //println!("key: 0x{:x}", vk);
            }

            if key_pressed || key_released {
                if 1 == self.key_hook(key_pressed, key_released, vk) {
                    return 1;
                }
            }
        }

        return unsafe { winuser::CallNextHookEx(ptr::null_mut(), code, wparam, lparam) };
    }

    #[cfg(target_os = "windows")]
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

#[cfg(target_os = "windows")]
static mut HOOK_STATE: Option<InputHookState> = None;

#[cfg(target_os = "windows")]
fn main() {
    let rt = RuntimeContext::init();
    run();
    rt.uninit();
}

#[cfg(target_os = "linux")]
fn main() {
    let mut idev = grab_keyboard_device();
    let mut odev = linux::UinputKeyboard::new(
        uinput::default()
            .unwrap()
            .name("test")
            .unwrap()
            .event(uinput::event::Keyboard::All)
            .unwrap()
            .create()
            .unwrap(),
    );

    let mut state = InputHookState::default();
    state.remap_state.modifiers.insert(Modifier::Colemak);

    //println!("{}", idev);
    //println!("Events:");
    loop {
        for ev in idev.events_no_sync().unwrap() {
            if ev._type == 1 {
                let key_event = match ev.value {
                    1 => KeyEvent::Down,
                    2 => KeyEvent::Repeat,
                    _ => KeyEvent::Up,
                };

                //println!("{:?}", ev);
                if 0 == state.key_hook(&mut odev, key_event, ev.code as u32) {
                    odev.send_key(ev.code as u8, key_event);
                }
            }
        }

        thread::sleep(time::Duration::from_millis(3));
    }
}
