use uinput_sys::*;
use uinput::event::keyboard;
use uinput;
use evdev;
use std;
use std::io::prelude::*;
use KeyboardDevice;

pub const ESCAPE : char = KEY_ESC as u8 as char;
pub const SEMICOLON : char = KEY_SEMICOLON as u8 as char;
pub const TILDE : char = KEY_APOSTROPHE as u8 as char;
pub const UK_TILDE : char = KEY_GRAVE as u8 as char;
pub const ALT_GR : char = KEY_RIGHTALT as u8 as char;
pub const RIGHTCTRL : char = KEY_RIGHTCTRL as u8 as char;
pub const LEFTCTRL : char = KEY_LEFTCTRL as u8 as char;
pub const CTRL : char = KEY_LEFTCTRL as u8 as char;
pub const LEFTALT : char = KEY_LEFTALT as u8 as char;
pub const ALT : char = KEY_RIGHTALT as u8 as char;
pub const BACKSPACE : char = KEY_BACKSPACE as u8 as char;
pub const COMMA : char = KEY_COMMA as u8 as char;
pub const PERIOD : char = KEY_DOT as u8 as char;
pub const FWD_SLASH : char = KEY_SLASH as u8 as char;
pub const MINUS : char = KEY_MINUS as u8 as char;
pub const PLUS : char = KEY_EQUAL as u8 as char;

pub const RETURN : char = KEY_ENTER as u8 as char;
pub const DELETE : char = KEY_DELETE as u8 as char;
pub const PGUP : char = KEY_PAGEUP as u8 as char;
pub const PGDOWN : char = KEY_PAGEDOWN as u8 as char;
				
pub const SHIFT : char = KEY_LEFTSHIFT as u8 as char;
pub const CAPS_LOCK : char = KEY_CAPSLOCK as u8 as char;
pub const LWIN : char = KEY_LEFTMETA as u8 as char;
pub const OEM_102 : char = KEY_102ND as u8 as char;
//pub const OEM_5 : char = KEY_OEM_5 as u8 as char;
pub const LSQUARE : char = KEY_LEFTBRACE as u8 as char;
pub const RSQUARE : char = KEY_RIGHTBRACE as u8 as char;
pub const APPS : char = KEY_COMPOSE as u8 as char;
pub const SPACE : char = KEY_SPACE as u8 as char;

pub const F1 : char = KEY_F1 as u8 as char;
pub const F2 : char = KEY_F2 as u8 as char;
pub const F3 : char = KEY_F3 as u8 as char;
pub const F4 : char = KEY_F4 as u8 as char;
pub const F5 : char = KEY_F5 as u8 as char;
pub const F6 : char = KEY_F6 as u8 as char;
pub const F7 : char = KEY_F7 as u8 as char;
pub const F8 : char = KEY_F8 as u8 as char;
pub const F9 : char = KEY_F9 as u8 as char;
pub const F10 : char = KEY_F10 as u8 as char;
pub const F11 : char = KEY_F11 as u8 as char;
pub const F12 : char = KEY_F12 as u8 as char;

pub const LEFT : char = KEY_LEFT as u8 as char;
pub const RIGHT : char = KEY_RIGHT as u8 as char;
pub const UP : char = KEY_UP as u8 as char;
pub const DOWN : char = KEY_DOWN as u8 as char;
pub const HOME : char = KEY_HOME as u8 as char;
pub const END : char = KEY_END as u8 as char;
//pub const BACK : char = KEY_BACK as u8 as char;

pub const K_1 : char = KEY_1 as u8 as char;
pub const K_2 : char = KEY_2 as u8 as char;
pub const K_3 : char = KEY_3 as u8 as char;
pub const K_4 : char = KEY_4 as u8 as char;
pub const K_5 : char = KEY_5 as u8 as char;
pub const K_6 : char = KEY_6 as u8 as char;
pub const K_7 : char = KEY_7 as u8 as char;
pub const K_8 : char = KEY_8 as u8 as char;
pub const K_9 : char = KEY_9 as u8 as char;
pub const K_0 : char = 11 as u8 as char;
pub const K_Q : char = KEY_Q as u8 as char;
pub const K_W : char = KEY_W as u8 as char;
pub const K_E : char = KEY_E as u8 as char;
pub const K_R : char = KEY_R as u8 as char;
pub const K_T : char = KEY_T as u8 as char;
pub const K_Y : char = KEY_Y as u8 as char;
pub const K_U : char = KEY_U as u8 as char;
pub const K_I : char = KEY_I as u8 as char;
pub const K_O : char = KEY_O as u8 as char;
pub const K_P : char = KEY_P as u8 as char;
pub const K_A : char = KEY_A as u8 as char;
pub const K_S : char = KEY_S as u8 as char;
pub const K_D : char = 32 as u8 as char;
pub const K_F : char = KEY_F as u8 as char;
pub const K_G : char = KEY_G as u8 as char;
pub const K_H : char = KEY_H as u8 as char;
pub const K_J : char = KEY_J as u8 as char;
pub const K_K : char = KEY_K as u8 as char;
pub const K_L : char = KEY_L as u8 as char;
pub const K_Z : char = KEY_Z as u8 as char;
pub const K_X : char = KEY_X as u8 as char;
pub const K_C : char = KEY_C as u8 as char;
pub const K_V : char = KEY_V as u8 as char;
pub const K_B : char = KEY_B as u8 as char;
pub const K_N : char = KEY_N as u8 as char;
pub const K_M : char = KEY_M as u8 as char;

#[derive(Default)]
pub struct OsState {}

impl OsState {
	pub fn on_caps_layer_enable(&mut self) {
				//self.window_move_hwnd = ptr::null_mut();
				//self.window_resize_hwnd = ptr::null_mut();


				//let scroll = &mut self.scroll_emu_state.lock().unwrap();
				//scroll.scroll_emu_on = false;
	}
}

pub struct UinputKeyboard {
	device: uinput::device::Device
}

impl UinputKeyboard {
	pub fn new(d: uinput::device::Device) -> UinputKeyboard {
		UinputKeyboard { device: d }
	}
}

impl KeyboardDevice for UinputKeyboard {
	fn send_key(&mut self, key: u8, down: bool) {
		println!("send_key({}, {})", key, down);
		self.device.write(EV_KEY, key as i32, if down {1} else {0}).unwrap();
		self.device.synchronize().unwrap();
	}
}

pub fn kill_top_window_process() {}
pub fn lock_workstation() {}
pub fn toast_notification(_content: &str) {}

pub fn grab_keyboard_device() -> evdev::Device {
    let mut args = std::env::args_os();
    let d;
    if args.len() > 1 {
        d = evdev::Device::open(&args.nth(1).unwrap()).unwrap();
    } else {
        let mut devices = evdev::enumerate();
        for (i, d) in devices.iter().enumerate() {
            println!("{}: {:?}", i, d.name());
        }
        print!("Select the device [0-{}]: ", devices.len());
        let _ = std::io::stdout().flush();
        let mut chosen = String::new();
        std::io::stdin().read_line(&mut chosen).unwrap();
        d = devices.swap_remove(chosen.trim().parse::<usize>().unwrap());
    }

    unsafe { evdev::raw::eviocgrab(d.fd(), &1) }.unwrap();

    d
}
