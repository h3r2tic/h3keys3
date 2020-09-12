use crate::linux::*;
use std::collections::HashSet;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum KeyEvent {
    Down,
    Repeat,
    Up,
}

#[derive(PartialEq, Debug)]
pub enum KeyAction {
    Down(i32),
    Up(i32),
}

#[derive(PartialEq, Debug)]
pub enum RemapTarget {
    BlindKey(i32),
    KeySeq(std::vec::Vec<KeyAction>),
    Block,
}

/*impl Default for RemapTarget {
    fn default() -> Self {
        Self::Block
    }
}*/

pub trait VirtualKey: Copy {
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

pub fn key_down<T: VirtualKey>(kv: T) -> KeyAction {
    KeyAction::Down(kv.as_i32())
}

pub fn key_up<T: VirtualKey>(kv: T) -> KeyAction {
    KeyAction::Up(kv.as_i32())
}

pub fn key<T: VirtualKey>(kv: T) -> RemapTarget {
    RemapTarget::BlindKey(kv.as_i32())
}

pub fn mod_key<T1: VirtualKey, T2: VirtualKey>(mod_key: T1, kv: T2) -> RemapTarget {
    RemapTarget::KeySeq(vec![
        key_down(mod_key),
        key_down(kv),
        key_up(kv),
        key_up(mod_key),
    ])
}

pub fn ctrl_key<T: VirtualKey>(kv: T) -> RemapTarget {
    mod_key(CTRL, kv)
}

pub fn shift_key<T: VirtualKey>(kv: T) -> RemapTarget {
    mod_key(SHIFT, kv)
}

pub fn alt_key<T: VirtualKey>(kv: T) -> RemapTarget {
    mod_key(ALT, kv)
}

pub fn no_ctrl_key<T: VirtualKey>(kv: T) -> RemapTarget {
    RemapTarget::KeySeq(vec![key_up(CTRL), key_down(kv), key_up(kv), key_down(CTRL)])
}

pub fn remap_colemak(vk: u8) -> i32 {
    let res = match vk as char {
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
    RemapTarget::KeySeq(vec![key_down(K_D), key_up(K_D)])
}

#[cfg(target_os = "linux")]
fn remap_minimize() -> RemapTarget {
    RemapTarget::Block
}

#[derive(Copy, Clone, Hash, Eq, PartialEq)]
pub enum Modifier {
    Mod1,
    Mod2,
    Ctrl,
    Win,
    Admin,
    LeftAlt,
    LeftCtrl,
    Colemak,
}

pub enum RemapSideEffect {
    KillTopWindowProcess,
    LockWorkstation,
    Notification(String),
    SetModifier(Modifier, bool),
    Terminate,
}

#[derive(Default, Clone, PartialEq)]
pub struct RemapState {
    pub modifiers: HashSet<Modifier>,
}

impl RemapState {
    #[inline(always)]
    pub fn is_mod_on(&self, m: Modifier) -> bool {
        self.modifiers.contains(&m)
    }

    pub fn set_mod(&mut self, m: Modifier, v: bool) {
        if v {
            self.modifiers.insert(m);
        } else {
            self.modifiers.remove(&m);
        }
    }
}

pub fn remap_key(
    state: &RemapState,
    key_event: KeyEvent,
    vk: u32,
) -> (Option<RemapTarget>, Vec<RemapSideEffect>) {
    let mut side_effects: Vec<RemapSideEffect> = Vec::new();

    let key_pressed_now = if let KeyEvent::Down = key_event {
        true
    } else {
        false
    };
    let key_pressed_or_held = if let KeyEvent::Up = key_event {
        false
    } else {
        true
    };

    let down_only = |rt: RemapTarget| {
        if let KeyEvent::Down = key_event {
            rt
        } else {
            key(0)
        }
    };

    let down_or_held_only = |rt: RemapTarget| {
        if let KeyEvent::Up = key_event {
            key(0)
        } else {
            rt
        }
    };

    let remap = if state.is_mod_on(Modifier::Colemak) {
        //println!("Remap colemak");
        key(remap_colemak(vk as u8))
    } else {
        //println!("NO Remap colemak");
        key(0)
    };

    // Windows keys
    let remap = match vk as u8 as char {
        TILDE => key(ESCAPE),
        UK_TILDE => key(TILDE),
        ALT_GR => {
            side_effects.push(RemapSideEffect::SetModifier(
                Modifier::Win,
                key_pressed_or_held,
            ));
            key(LWIN)
        }
        LEFTALT => {
            side_effects.push(RemapSideEffect::SetModifier(
                Modifier::LeftAlt,
                key_pressed_or_held,
            ));
            remap
        }
        LEFTCTRL => {
            side_effects.push(RemapSideEffect::SetModifier(
                Modifier::LeftCtrl,
                key_pressed_or_held,
            ));
            remap
        }
        BACKSPACE => {
            if key_pressed_now
                && state.is_mod_on(Modifier::LeftAlt)
                && state.is_mod_on(Modifier::LeftCtrl)
            {
                side_effects.push(RemapSideEffect::KillTopWindowProcess);
            }
            remap
        }
        RIGHTCTRL => key(APPS),
        K_U => {
            if state.is_mod_on(Modifier::Win) && key_pressed_now {
                // We will not register a key-up due to the lock screen
                side_effects.push(RemapSideEffect::SetModifier(Modifier::Win, false));
                side_effects.push(RemapSideEffect::LockWorkstation);
                RemapTarget::Block
            } else {
                remap
            }
        }
        K_4 => {
            if state.is_mod_on(Modifier::Win) && key_pressed_now {
                alt_key(F4)
            } else {
                remap
            }
        }
        K_M => {
            if state.is_mod_on(Modifier::Win) && key_pressed_now {
                remap_minimize()
            } else {
                remap
            }
        }
        _ => remap,
    };

    let remap = if state.is_mod_on(Modifier::Mod1) {
        // Caps-lock layer

        let mapped_key = match vk as u8 as char {
            ESCAPE => {
                side_effects.push(RemapSideEffect::SetModifier(
                    Modifier::Admin,
                    key_pressed_or_held,
                ));

                RemapTarget::Block
            }
            SPACE => {
                if state.is_mod_on(Modifier::Admin) {
                    if let KeyEvent::Up = key_event {
                        side_effects.push(RemapSideEffect::Notification(
                            "Program terminated".to_owned(),
                        ));
                        side_effects.push(RemapSideEffect::Terminate);
                        //std::process::exit(0);
                    }
                    RemapTarget::Block
                } else {
                    key(SPACE)
                }
            }
            K_D => key(SHIFT),
            K_F => {
                side_effects.push(RemapSideEffect::SetModifier(
                    Modifier::Ctrl,
                    key_pressed_or_held,
                ));

                key(CTRL)
            }
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
                if state.is_mod_on(Modifier::Admin) {
                    if key_pressed_now {
                        let colemak_on = !state.is_mod_on(Modifier::Colemak);
                        side_effects
                            .push(RemapSideEffect::SetModifier(Modifier::Colemak, colemak_on));

                        side_effects.push(RemapSideEffect::Notification(if colemak_on {
                            "Colemak".to_owned()
                        } else {
                            "Qwerty".to_owned()
                        }))
                    }

                    RemapTarget::Block
                } else {
                    down_only(ctrl_key(K_C))
                }
            }
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
            K_I => {
                if state.is_mod_on(Modifier::Ctrl) {
                    down_or_held_only(no_ctrl_key(PGUP))
                } else {
                    key(UP)
                }
            }
            // caps-key is down
            // caps-ctrl-key is page down
            K_K => {
                if state.is_mod_on(Modifier::Ctrl) {
                    down_or_held_only(no_ctrl_key(PGDOWN))
                } else {
                    key(DOWN)
                }
            }
            ALT_GR => {
                side_effects.push(RemapSideEffect::SetModifier(
                    Modifier::Win,
                    key_pressed_or_held,
                ));

                key(LWIN)
            }
            LEFTALT => key(0), // pass-through
            ALT => key(0),     // pass-through
            CTRL => key(0),    // pass-through
            _ => RemapTarget::Block,
        };

        mapped_key
    } else if state.is_mod_on(Modifier::Mod2) {
        // Pipe/backslash layer

        let mapped_key = match vk as u8 as char {
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
            PERIOD => down_only(RemapTarget::KeySeq(vec![
                key_down(FWD_SLASH),
                key_up(FWD_SLASH),
                key_down(SHIFT),
                key_down(K_8),
                key_up(K_8),
                key_up(SHIFT),
            ])),
            FWD_SLASH => down_only(RemapTarget::KeySeq(vec![
                key_down(SHIFT),
                key_down(K_8),
                key_up(K_8),
                key_up(SHIFT),
                key_down(FWD_SLASH),
                key_up(FWD_SLASH),
            ])),
            _ => RemapTarget::Block,
        };

        mapped_key
    } else {
        remap
    };

    let remap = if remap != key(0) { Some(remap) } else { None };

    (remap, side_effects)
}
