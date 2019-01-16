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

use std::{thread, ptr, time, f32, mem, str};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::cell::RefCell;

#[derive(Debug, Copy, Clone)]
pub enum KeyEvent {
	Down,
	Repeat,
	Up,
}

pub trait KeyboardDevice {
	fn send_key(&mut self, key: u8, event: KeyEvent);
}

#[derive(PartialEq, Debug)]
enum KeyAction {
	Down(i32),
	Up(i32),
}

#[derive(PartialEq, Debug)]
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
	mod_key(CTRL, kv)
}

fn shift_key<T: VirtualKey>(kv: T) -> RemapTarget {
	mod_key(SHIFT, kv)
}

fn alt_key<T: VirtualKey>(kv: T) -> RemapTarget {
	mod_key(ALT, kv)
}

fn no_ctrl_key<T: VirtualKey>(kv: T) -> RemapTarget {
	RemapTarget::KeySeq(vec![
		key_up(CTRL),
		key_down(kv),
		key_up(kv),
		key_down(CTRL),
	])
}

fn remap_colemak(vk: u8) -> i32
{
	let res = match vk as char
	{
		K_E => K_F,
		K_R => K_P,
		K_T => K_G,
		K_Y => K_J,
		K_U => K_L,
		K_I => K_U,
		K_O => K_Y,
		K_S => K_R,
		K_G => K_D,
		K_J => K_N,
		K_K => K_E,
		K_L => K_I,
		K_N => K_K,
		K_P => SEMICOLON,
		K_D => K_S,
		K_F => K_T,
		SEMICOLON => K_O,
		_ => '\0',
	};
	res as u8 as i32
}

#[cfg(target_os = "windows")]
fn remap_minimize() -> RemapTarget {
	RemapTarget::KeySeq(vec![
		key_down(K_D),
		key_up(K_D),
	])
}

#[cfg(target_os = "linux")]
fn remap_minimize() -> RemapTarget {
	RemapTarget::Block
}

#[derive(Default)]
struct InputHookState
{
	colemak_on: bool,

	mod1_on: bool,
	mod2_on: bool,
	ctrlmod_on: bool,
	winkey_on: bool,
	admin_on: bool,

	leftalt_on: bool,
	leftctrl_on: bool,

	os_state: OsState,

	mod1_keys_down : HashSet<i32>,
	mod2_keys_down : HashSet<i32>,
}

impl InputHookState
{
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

	fn key_hook(&mut self, device: &mut KeyboardDevice, key_event: KeyEvent, vk: u32) -> u32 {
		println!("key_hook {:?} {}", key_event, vk);

		let key_pressed_now = if let KeyEvent::Down = key_event { true } else { false };
		let key_pressed_or_held = if let KeyEvent::Up = key_event { false } else { true };

		let down_only = |rt: RemapTarget| {
			if let KeyEvent::Down = key_event { rt } else { key(0) }
		};

		let down_or_held_only = |rt: RemapTarget| {
			if let KeyEvent::Up = key_event { key(0) } else { rt }
		};

		// Enable caps-lock layer
		if CAPS_LOCK == vk as u8 as char {
			// If disabling, make sure all remapped keys get released
			if let KeyEvent::Up = key_event {
				for &key in self.mod1_keys_down.iter() {
					device.send_key(key as u8, KeyEvent::Up);
				}
				self.mod1_keys_down.clear();
				self.ctrlmod_on = false;
				self.os_state.on_caps_layer_enable();
				self.admin_on = false;
			}

			self.mod1_on = key_pressed_or_held;
			return 1;
		}

		// Enable pipe/backslash layer
		if OEM_102 == vk as u8 as char {
			if let KeyEvent::Up = key_event {
				for &key in self.mod2_keys_down.iter() {
					device.send_key(key as u8, KeyEvent::Up);
				}
				self.mod2_keys_down.clear();
			}

			self.mod2_on = key_pressed_or_held;
			return 1;
		}

		// Colemak
		let remap = if self.colemak_on {
			println!("Remap colemak");
			key(remap_colemak(vk as u8))
		} else {
			println!("NO Remap colemak");
			key(0)
		};

		// Windows keys
		let remap = match vk as u8 as char {
			TILDE => key(ESCAPE),
			UK_TILDE => key(TILDE),
			ALT_GR => {
				self.winkey_on = key_pressed_or_held;
				key(LWIN)
			},
			LEFTALT => {
				self.leftalt_on = key_pressed_or_held;
				remap
			}
			LEFTCTRL => {
				self.leftctrl_on = key_pressed_or_held;
				remap
			}
			BACKSPACE => {
				if key_pressed_now && self.leftalt_on && self.leftctrl_on {
					kill_top_window_process();
				}
				remap
			}
			RIGHTCTRL => key(APPS),
			K_U =>
				if self.winkey_on && key_pressed_now {
					// We will not register a key-up due to the lock screen
					self.winkey_on = false;
					lock_workstation();
					RemapTarget::Block
				} else {
					remap
				},
			K_4 =>
				if self.winkey_on && key_pressed_now {
					alt_key(F4)
				} else {
					remap
				},
			K_M =>
				if self.winkey_on && key_pressed_now {
					remap_minimize()
				} else {
					remap
				},
			_ => remap,
		};

		let remap = if self.mod1_on {
			// Caps-lock layer

			let mapped_key = match vk as u8 as char
			{
				ESCAPE => {
					self.admin_on = key_pressed_or_held;
					RemapTarget::Block
				},
				SPACE => {
					if self.admin_on {
						if let KeyEvent::Up = key_event {
							toast_notification("Program terminated");
							std::process::exit(0);
						} else {
							RemapTarget::Block
						}
					} else {
						key(SPACE)
					}
				},
				K_D => key(SHIFT),
				K_F => {
					self.ctrlmod_on = key_pressed_or_held;
					key(CTRL)
				},
				K_J => key(LEFT),
				K_L => key(RIGHT),
				K_U => key(HOME),
				K_O => key(END),
				K_H => key(BACKSPACE),
				K_1 => key(F1),
				K_2 => key(F2),
				K_3 => key(F3),
				K_4 => key(F4),
				K_5 => key(F5),
				K_6 => key(F6),
				K_7 => key(F7),
				K_8 => key(F8),
				K_9 => key(F9),
				K_0 => key(F10),
				MINUS => key(F11),
				PLUS => key(F12),
				K_N => down_or_held_only(ctrl_key(K_Z)),
				K_M => down_or_held_only(ctrl_key(K_Y)),
				K_C => {
					if self.admin_on {
						if key_pressed_now {
							self.colemak_on = !self.colemak_on;
							toast_notification(if self.colemak_on { "Colemak" } else { "Qwerty" });
						}

						RemapTarget::Block
					} else {
						down_only(ctrl_key(K_C))
					}
				},
				K_X => down_only(ctrl_key(K_X)),
				K_V => down_only(ctrl_key(K_V)),
				K_S => down_only(ctrl_key(K_S)),
				SEMICOLON => key(RETURN),
				K_P => key(DELETE),
				COMMA => down_only(shift_key(K_7)),

				PERIOD => down_or_held_only(shift_key(BACKSLASH)),
				FWD_SLASH => key(BACKSLASH),

				// caps-i is up
				// caps-ctrl-i is page up
				K_I => if self.ctrlmod_on {
					down_or_held_only(no_ctrl_key(PGUP))
				} else {
					key(UP)
				},
				// caps-key is down
				// caps-ctrl-key is page down
				K_K => if self.ctrlmod_on {
					down_or_held_only(no_ctrl_key(PGDOWN))
				} else {
					key(DOWN)
				},
				ALT_GR => {
					self.winkey_on = key_pressed_or_held;
					key(LWIN)
				},
				LEFTALT => key(0),	// pass-through
				ALT => key(0),	// pass-through
				CTRL => key(0),	// pass-through
				_ => RemapTarget::Block,
			};

			if let RemapTarget::BlindKey(k) = mapped_key {
				if key_pressed_or_held {
					self.mod1_keys_down.insert(k);
				} else {
					self.mod1_keys_down.remove(&k);
				}
			}

			mapped_key
		} else if self.mod2_on {
			// Pipe/backslash layer

			let mapped_key = match vk as u8 as char
			{
				' ' => key(SPACE),
				// h _
				K_H => down_or_held_only(shift_key(MINUS)),
				// jk ()
				K_J => down_or_held_only(shift_key(K_9)),
				K_K => down_or_held_only(shift_key(K_0)),
				// io []
				K_I => key(LSQUARE),
				K_O => key(RSQUARE),
				// l; {}
				K_L => down_or_held_only(shift_key(LSQUARE)),
				SEMICOLON => down_or_held_only(shift_key(RSQUARE)),
				// yu -+
				K_Y => key(MINUS),
				K_U => down_or_held_only(shift_key(PLUS)),
				// m =
				K_M => key(PLUS),
				// . /*
				PERIOD => down_only(
					RemapTarget::KeySeq(vec![
						key_down(FWD_SLASH),
						key_up(FWD_SLASH),
						key_down(SHIFT),
						key_down(K_8),
						key_up(K_8),
						key_up(SHIFT),
					])),
				FWD_SLASH => down_only(
					RemapTarget::KeySeq(vec![
						key_down(SHIFT),
						key_down(K_8),
						key_up(K_8),
						key_up(SHIFT),
						key_down(FWD_SLASH),
						key_up(FWD_SLASH),
					])),
				_ => RemapTarget::Block,
			};

			if let RemapTarget::BlindKey(k) = mapped_key {
				if key_pressed_or_held {
					self.mod2_keys_down.insert(k);
				} else {
					self.mod2_keys_down.remove(&k);
				}
			}

			mapped_key
		} else {
			remap
		};

		println!("reamap: {:?}", remap);

		if remap != key(0) {
			match remap {
				RemapTarget::BlindKey(key) => {
					device.send_key(key as u8, key_event);
				},
				RemapTarget::KeySeq(kseq) => {
					for key_action in kseq.iter() {
						match key_action {
							&KeyAction::Down(key) => device.send_key(key as u8, KeyEvent::Down),
							&KeyAction::Up(key) => device.send_key(key as u8, KeyEvent::Up),
						}
					}
				},
				RemapTarget::Block => ()
			}

			return 1;
		}

		return 0;
	}

	#[cfg(target_os = "windows")]
	fn win_key_hook(&mut self, code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
		if winuser::HC_ACTION == code
		{
			let input_key = unsafe { *(lparam as winuser::PKBDLLHOOKSTRUCT) };
			if input_key.dwExtraInfo == H3KEYS_MAGIC {
				return unsafe { winuser::CallNextHookEx(ptr::null_mut(), code, wparam, lparam) }; 
			}

			let key_pressed = winuser::WM_KEYDOWN == wparam as u32 || winuser::WM_SYSKEYDOWN == wparam as u32;
			let key_released = winuser::WM_KEYUP == wparam as u32 || winuser::WM_SYSKEYUP == wparam as u32;

			if key_pressed {
				//println!("key: 0x{:x}", vk);
			}

			if key_pressed || key_released
			{
				if 1 == self.key_hook(key_pressed, key_released, vk) {
					return 1;
				}
			}
		}

		return unsafe { winuser::CallNextHookEx(ptr::null_mut(), code, wparam, lparam) };
	}

	#[cfg(target_os = "windows")]
	fn mouse_hook(&mut self, code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
		if winuser::HC_ACTION == code && self.mod1_on
		{
			let mouse_data = unsafe { *(lparam as winuser::PMSLLHOOKSTRUCT) };

			// Window move
			if winuser::WM_LBUTTONDOWN == wparam as u32 {
				self.mouse_move_from = (mouse_data.pt.x, mouse_data.pt.y);
				self.window_move_hwnd = get_window_under_cursor(self.mouse_move_from);
				if self.window_move_hwnd != ptr::null_mut()
				{
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
						self.window_move_hwnd, ptr::null_mut(),
						x, y, 0, 0,
						winuser::SWP_NOACTIVATE |
						winuser::SWP_NOOWNERZORDER |
						winuser::SWP_NOSIZE |
						winuser::SWP_NOZORDER);
				}
			}

			// Window resize
			if winuser::WM_RBUTTONDOWN == wparam as u32 {
				self.mouse_resize_from = (mouse_data.pt.x, mouse_data.pt.y);
				self.window_resize_hwnd = get_window_under_cursor(self.mouse_resize_from);
				if self.window_resize_hwnd != ptr::null_mut()
				{
					let rect = get_window_rect(self.window_resize_hwnd);
					self.window_resize_from = (rect.right - rect.left, rect.bottom - rect.top);
				}

				return 1;
			}

			if winuser::WM_RBUTTONUP == wparam as u32 {
				self.window_resize_hwnd = ptr::null_mut();
				return 1;
			}

			if winuser::WM_MOUSEMOVE == wparam as u32 && self.window_resize_hwnd != ptr::null_mut() {
				let x = self.window_resize_from.0 + mouse_data.pt.x - self.mouse_resize_from.0;
				let y = self.window_resize_from.1 + mouse_data.pt.y - self.mouse_resize_from.1;

				unsafe {
					winuser::SetWindowPos(
						self.window_resize_hwnd, ptr::null_mut(),
						0, 0, x, y,
						winuser::SWP_NOACTIVATE |
						winuser::SWP_NOOWNERZORDER |
						winuser::SWP_NOMOVE |
						winuser::SWP_NOZORDER);
				}
			}

			// Scroll emulation
			if 1 == self.scroll_emu_state.lock().unwrap().mouse_hook(wparam, mouse_data) {
				return 1;
			}
		}

		return unsafe { winuser::CallNextHookEx(ptr::null_mut(), code, wparam, lparam) };
	}
}


#[cfg(target_os = "windows")]
static mut HOOK_STATE : Option<InputHookState> = None;


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
    	uinput::default().unwrap()
		.name("test").unwrap()
		.event(uinput::event::Keyboard::All).unwrap()
		.create().unwrap()
	);
    let mut state = InputHookState::default();
    state.colemak_on = true;

    println!("{}", idev);
    println!("Events:");
    loop {
        for ev in idev.events_no_sync().unwrap() {
            if ev._type == 1 {
				let key_event = match ev.value {
					1 => KeyEvent::Down,
					2 => KeyEvent::Repeat,
					_ => KeyEvent::Up,
				};

	            println!("{:?}", ev);
            	if 0 == state.key_hook(&mut odev, key_event, ev.code as u32) {
            		odev.send_key(ev.code as u8, key_event);
            	}
            }
        }

		thread::sleep(time::Duration::from_millis(3));
    }
}
