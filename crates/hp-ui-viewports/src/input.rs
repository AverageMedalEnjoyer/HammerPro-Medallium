use egui::{Event, Key, Modifiers, MouseWheelUnit, PointerButton, Pos2, Vec2};

use hp_app::settings::KeyboardLayout;

use crate::camera::FlyInput;

const POINTS_PER_NOTCH: f32 = 40.0;
const NOTCHES_PER_PAGE: f32 = 5.0;

pub const BUTTONS: [PointerButton; 3] = [PointerButton::Primary, PointerButton::Secondary, PointerButton::Middle];
pub const PRIMARY: usize = 0;
pub const SECONDARY: usize = 1;
pub const MIDDLE: usize = 2;

#[derive(Clone, Copy, Debug, Default)]
pub struct FrameInput {
    pub pointer: Option<Pos2>,
    pub delta: Vec2,
    pub motion: Option<Vec2>,
    pub pressed: [bool; 3],
    pub released: [bool; 3],
    pub down: [bool; 3],
    pub modifiers: Modifiers,
    pub wheel: f32,
    pub space_down: bool,
    pub focused: bool,
    pub z_pressed: bool,
    pub z_exit_pressed: bool,
    pub escape_pressed: bool,
    pub any_key_down: bool,
    pub arrow_presses: [u8; 4],
    pub arrows_down: [bool; 4],
    pub digit: Option<u8>,
    pub digit_logical: Option<Key>,
}

pub const ARROWS: [Key; 4] = [Key::ArrowLeft, Key::ArrowRight, Key::ArrowUp, Key::ArrowDown];

fn digit_of(key: Key) -> Option<u8> {
    Some(match key {
        Key::Num1 => 1,
        Key::Num2 => 2,
        Key::Num3 => 3,
        Key::Num4 => 4,
        Key::Num5 => 5,
        Key::Num6 => 6,
        Key::Num7 => 7,
        Key::Num8 => 8,
        Key::Num9 => 9,
        _ => return None,
    })
}

pub fn wheel_notches(unit: MouseWheelUnit, delta: Vec2) -> f32 {
    let d = if delta.y.is_finite() { delta.y } else { 0.0 };
    match unit {
        MouseWheelUnit::Line => d,
        MouseWheelUnit::Point => d / POINTS_PER_NOTCH,
        MouseWheelUnit::Page => d * NOTCHES_PER_PAGE,
    }
}

fn physical(key: Key, physical_key: Option<Key>) -> Key {
    physical_key.unwrap_or(key)
}

impl FrameInput {
    pub fn gather(ctx: &egui::Context) -> Self {
        Self::gather_for(ctx, KeyboardLayout::Auto)
    }

    pub fn gather_for(ctx: &egui::Context, layout: KeyboardLayout) -> Self {
        ctx.input(|i| {
            let mut out = FrameInput {
                pointer: i.pointer.latest_pos(),
                delta: i.pointer.delta(),
                motion: i.pointer.motion(),
                modifiers: i.modifiers,
                space_down: i.key_down(Key::Space),
                focused: i.focused,
                any_key_down: !i.keys_down.is_empty(),
                ..FrameInput::default()
            };
            if !i.modifiers.any() {
                for (k, key) in ARROWS.iter().enumerate() {
                    out.arrows_down[k] = i.key_down(*key);
                }
            }
            for (k, b) in BUTTONS.iter().enumerate() {
                out.pressed[k] = i.pointer.button_pressed(*b);
                out.released[k] = i.pointer.button_released(*b);
                out.down[k] = i.pointer.button_down(*b);
            }
            for e in &i.events {
                match e {
                    Event::MouseWheel { unit, delta, .. } => out.wheel += wheel_notches(*unit, *delta),
                    Event::Key { key, physical_key, pressed: true, repeat, modifiers } => {
                        let plain = !modifiers.ctrl && !modifiers.alt && !modifiers.shift && !modifiers.command;
                        if let Some(k) = ARROWS.iter().position(|a| a == key)
                            && plain
                        {
                            out.arrow_presses[k] = out.arrow_presses[k].saturating_add(1);
                        }
                        if *repeat {
                            continue;
                        }
                        let (enters, leaves) = mouselook_key(layout, *key, *physical_key);
                        if plain && enters {
                            out.z_pressed = true;
                            out.z_exit_pressed |= leaves;
                        }
                        if plain && let Some(d) = digit_key(*key, *physical_key) {
                            out.digit = Some(d);
                            out.digit_logical = Some(*key);
                        }
                        if *key == Key::Escape {
                            out.escape_pressed = true;
                        }
                    }
                    _ => {}
                }
            }
            out
        })
    }

    pub fn look_delta(&self) -> Vec2 {
        self.motion.unwrap_or(self.delta)
    }
}

fn digit_key(key: Key, physical_key: Option<Key>) -> Option<u8> {
    physical_key.and_then(digit_of).or_else(|| digit_of(key))
}

pub fn consume_arrows(ctx: &egui::Context) {
    ctx.input_mut(|i| {
        i.events.retain(|e| match e {
            Event::Key { key, pressed: true, modifiers, .. } if ARROWS.contains(key) => modifiers.any(),
            _ => true,
        });
    });
}

pub fn consume_digits(ctx: &egui::Context) {
    ctx.input_mut(|i| {
        i.events.retain(|e| match e {
            Event::Key { key, physical_key, pressed: true, modifiers, .. } => {
                modifiers.any() || digit_key(*key, *physical_key).is_none()
            }
            _ => true,
        });
    });
}

pub fn mouselook_key(layout: KeyboardLayout, key: Key, physical_key: Option<Key>) -> (bool, bool) {
    match layout {
        KeyboardLayout::Auto => {
            let z = key == Key::Z || physical_key == Some(Key::Z);
            (z, z && fly_bit(physical(key, physical_key)) == 0)
        }
        KeyboardLayout::Qwerty => (key == Key::Z, key == Key::Z),
        KeyboardLayout::Azerty => (key == Key::W, key == Key::W),
    }
}

pub fn consume_z(ctx: &egui::Context, layout: KeyboardLayout, exit_only: bool) {
    ctx.input_mut(|i| {
        i.events.retain(|e| match e {
            Event::Key { key, physical_key, pressed: true, .. } => {
                let (enters, leaves) = mouselook_key(layout, *key, *physical_key);
                !enters || (exit_only && !leaves)
            }
            _ => true,
        });
    });
}

pub fn consume_escape(ctx: &egui::Context) {
    ctx.input_mut(|i| {
        i.consume_key(Modifiers::NONE, Key::Escape);
        i.events.retain(|e| !matches!(e, Event::Key { key: Key::Escape, pressed: true, .. }));
    });
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlyKeys {
    bits: u8,
    pub layout: hp_app::settings::KeyboardLayout,
}

fn fly_key(layout: hp_app::settings::KeyboardLayout, key: Key, physical_key: Option<Key>) -> u8 {
    use hp_app::settings::KeyboardLayout as L;
    match layout {
        L::Auto => fly_bit(physical(key, physical_key)),
        L::Qwerty => fly_bit(key),
        L::Azerty => match key {
            Key::Z => W,
            Key::Q => A,
            Key::A => Q,
            Key::W => 0,
            k => fly_bit(k),
        },
    }
}

const W: u8 = 1;
const A: u8 = 1 << 1;
const S: u8 = 1 << 2;
const D: u8 = 1 << 3;
const Q: u8 = 1 << 4;
const E: u8 = 1 << 5;
const SPACE: u8 = 1 << 6;

fn fly_bit(key: Key) -> u8 {
    match key {
        Key::W => W,
        Key::A => A,
        Key::S => S,
        Key::D => D,
        Key::Q => Q,
        Key::E => E,
        Key::Space => SPACE,
        _ => 0,
    }
}

impl FlyKeys {
    pub fn update(&mut self, events: &[Event], focused: bool, any_key_down: bool) {
        for e in events {
            if let Event::Key { key, physical_key, pressed, repeat: false, .. } = e {
                let bit = fly_key(self.layout, *key, *physical_key);
                if *pressed {
                    self.bits |= bit;
                } else {
                    self.bits &= !bit;
                }
            }
        }
        if !focused || !any_key_down {
            self.bits = 0;
        }
    }

    pub fn clear(&mut self) {
        self.bits = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.bits == 0
    }

    pub fn fly_input(&self, modifiers: Modifiers) -> FlyInput {
        self.fly_input_with(modifiers, true)
    }

    pub fn fly_input_with(&self, modifiers: Modifiers, space_up: bool) -> FlyInput {
        let axis =
            |pos: u8, neg: u8| f64::from(u8::from(self.bits & pos != 0)) - f64::from(u8::from(self.bits & neg != 0));
        let up_bits = if space_up { E | SPACE } else { E };
        FlyInput { forward: axis(W, S), right: axis(D, A), up: axis(up_bits, Q), fast: modifiers.shift }
    }

    pub fn update_from_ctx(&mut self, ctx: &egui::Context, input: &FrameInput) {
        ctx.input(|i| self.update(&i.events, input.focused, input.any_key_down));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(k: Key, physical_key: Option<Key>, pressed: bool) -> Event {
        Event::Key { key: k, physical_key, pressed, repeat: false, modifiers: Modifiers::NONE }
    }

    #[test]
    fn forced_layouts_map_key_labels() {
        use hp_app::settings::KeyboardLayout as L;
        let mut k = FlyKeys { layout: L::Azerty, ..FlyKeys::default() };
        k.update(
            &[key(Key::Z, None, true), key(Key::Q, None, true), key(Key::A, None, true), key(Key::W, None, true)],
            true,
            true,
        );
        assert_eq!(k.bits, W | A | Q);
        let mut k = FlyKeys { layout: L::Qwerty, ..FlyKeys::default() };
        k.update(&[key(Key::W, Some(Key::Z), true), key(Key::Q, None, true)], true, true);
        assert_eq!(k.bits, W | Q);
        let mut k = FlyKeys::default();
        k.update(&[key(Key::Z, Some(Key::W), true)], true, true);
        assert_eq!(k.bits, W);
    }

    #[test]
    fn azerty_z_enters_but_does_not_leave_mouselook() {
        let ctx = egui::Context::default();
        let input = egui::RawInput { events: vec![key(Key::Z, Some(Key::W), true)], ..Default::default() };
        let mut seen = FrameInput::default();
        let mut out = ctx.run_ui(input, |ui| {
            seen = FrameInput::gather(ui.ctx());
            consume_z(ui.ctx(), KeyboardLayout::Auto, true);
            let after = FrameInput::gather(ui.ctx());
            assert!(after.z_pressed, "the forward key event is kept");
        });
        out.textures_delta.clear();
        assert!(seen.z_pressed);
        assert!(!seen.z_exit_pressed);
    }

    #[test]
    fn arrows_and_digits() {
        let ctx = egui::Context::default();
        let repeat = Event::Key {
            key: Key::ArrowLeft,
            physical_key: None,
            pressed: true,
            repeat: true,
            modifiers: Modifiers::NONE,
        };
        let shifted = Event::Key {
            key: Key::ArrowUp,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::SHIFT,
        };
        let input = egui::RawInput {
            events: vec![key(Key::ArrowLeft, None, true), repeat, shifted, key(Key::Quote, Some(Key::Num3), true)],
            ..Default::default()
        };
        let mut seen = FrameInput::default();
        let mut out = ctx.run_ui(input, |ui| {
            seen = FrameInput::gather(ui.ctx());
            consume_arrows(ui.ctx());
            consume_digits(ui.ctx());
            let after = FrameInput::gather(ui.ctx());
            assert_eq!(after.arrow_presses, [0; 4]);
            assert_eq!(after.digit, None);
            let left = ui.ctx().input(|i| i.events.len());
            assert_eq!(left, 1);
        });
        out.textures_delta.clear();
        assert_eq!(seen.arrow_presses, [2, 0, 0, 0], "repeats count, Shift + arrow does not");
        assert_eq!(seen.digit, Some(3));
        assert_eq!(digit_key(Key::Num9, None), Some(9));
        assert_eq!(digit_key(Key::Num0, Some(Key::Num0)), None);
    }

    #[test]
    fn space_is_not_up_during_space_gestures() {
        let mut k = FlyKeys::default();
        k.update(&[key(Key::Space, None, true), key(Key::W, None, true)], true, true);
        assert_eq!(k.fly_input(Modifiers::NONE).up, 1.0);
        let i = k.fly_input_with(Modifiers::NONE, false);
        assert_eq!((i.up, i.forward), (0.0, 1.0));
        assert_eq!(k.fly_input_with(Modifiers::CTRL, false).up, 0.0);
    }

    #[test]
    fn ctrl_does_not_fly_down() {
        use hp_app::settings::KeyboardLayout as L;
        let mut k = FlyKeys::default();
        assert!(k.fly_input(Modifiers::CTRL).is_idle());
        assert!(k.fly_input(Modifiers::CTRL | Modifiers::SHIFT).is_idle());
        k.update(&[key(Key::W, None, true)], true, true);
        let i = k.fly_input(Modifiers::CTRL);
        assert_eq!((i.forward, i.up), (1.0, 0.0));
        for (layout, down, up) in [(L::Qwerty, Key::Q, Key::E), (L::Azerty, Key::A, Key::E), (L::Auto, Key::Q, Key::E)]
        {
            let mut k = FlyKeys { layout, ..FlyKeys::default() };
            k.update(&[key(down, None, true)], true, true);
            assert_eq!(k.fly_input(Modifiers::NONE).up, -1.0, "{layout:?} down");
            k.clear();
            k.update(&[key(up, None, true)], true, true);
            assert_eq!(k.fly_input(Modifiers::CTRL).up, 1.0, "{layout:?} up");
        }
    }

    #[test]
    fn notches() {
        assert_eq!(wheel_notches(MouseWheelUnit::Line, Vec2::new(0.0, 1.0)), 1.0);
        assert_eq!(wheel_notches(MouseWheelUnit::Point, Vec2::new(0.0, -80.0)), -2.0);
        assert_eq!(wheel_notches(MouseWheelUnit::Page, Vec2::new(0.0, 1.0)), 5.0);
        assert_eq!(wheel_notches(MouseWheelUnit::Line, Vec2::new(0.0, f32::NAN)), 0.0);
    }

    #[test]
    fn fly_keys_by_physical_position() {
        let mut k = FlyKeys::default();
        k.update(&[key(Key::Z, Some(Key::W), true), key(Key::D, Some(Key::D), true)], true, true);
        let i = k.fly_input(Modifiers::NONE);
        assert_eq!((i.forward, i.right, i.up), (1.0, 1.0, 0.0));
        assert!(!i.fast);
        k.update(&[key(Key::Z, Some(Key::W), false)], true, true);
        assert_eq!(k.fly_input(Modifiers::NONE).forward, 0.0);
        k.update(&[key(Key::S, None, true), key(Key::Q, None, true)], true, true);
        let i = k.fly_input(Modifiers::SHIFT);
        assert_eq!((i.forward, i.up), (-1.0, -1.0));
        assert!(i.fast);
        k.clear();
        assert_eq!(k.fly_input(Modifiers::CTRL).up, 0.0);
        k.update(&[key(Key::Space, None, true)], true, true);
        assert_eq!(k.fly_input(Modifiers::CTRL).up, 1.0);
        k.update(&[], false, true);
        assert!(k.is_empty());
        k.update(&[key(Key::W, None, true)], true, true);
        k.update(&[], true, false);
        assert!(k.is_empty());
    }

    #[test]
    fn gather_reads_wheel_and_toggle() {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            events: vec![
                Event::MouseWheel {
                    unit: MouseWheelUnit::Line,
                    delta: Vec2::new(0.0, 2.0),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: Modifiers::NONE,
                },
                key(Key::W, Some(Key::Z), true),
                key(Key::Escape, None, true),
            ],
            ..Default::default()
        };
        let mut seen = FrameInput::default();
        let mut out = ctx.run_ui(input, |ui| {
            seen = FrameInput::gather(ui.ctx());
            consume_z(ui.ctx(), KeyboardLayout::Auto, false);
            consume_escape(ui.ctx());
            let after = FrameInput::gather(ui.ctx());
            assert!(!after.z_pressed && !after.escape_pressed);
        });
        out.textures_delta.clear();
        assert_eq!(seen.wheel, 2.0);
        assert!(seen.z_pressed, "physical Z (AZERTY W) toggles mouselook");
        assert!(seen.z_exit_pressed, "and leaves it");
        assert!(seen.escape_pressed);
    }

    #[test]
    fn mouselook_key_follows_the_layout() {
        use KeyboardLayout as L;
        let label_z = (Key::Z, Some(Key::W));
        let label_w = (Key::W, Some(Key::Z));
        assert_eq!(mouselook_key(L::Auto, label_z.0, label_z.1), (true, false));
        assert_eq!(mouselook_key(L::Auto, label_w.0, label_w.1), (true, true));
        assert_eq!(mouselook_key(L::Qwerty, label_z.0, label_z.1), (true, true));
        assert_eq!(mouselook_key(L::Qwerty, label_w.0, label_w.1), (false, false));
        assert_eq!(mouselook_key(L::Azerty, label_z.0, label_z.1), (false, false));
        assert_eq!(mouselook_key(L::Azerty, label_w.0, label_w.1), (true, true));
        for layout in [L::Auto, L::Qwerty, L::Azerty] {
            for (k, p) in [label_z, label_w, (Key::Z, None), (Key::W, None)] {
                let (_, leaves) = mouselook_key(layout, k, p);
                assert!(!leaves || fly_key(layout, k, p) == 0, "{layout:?} {k:?}: the leave key never flies");
            }
        }
        let ctx = egui::Context::default();
        let input = egui::RawInput { events: vec![key(label_w.0, label_w.1, true)], ..Default::default() };
        let mut seen = FrameInput::default();
        let mut out = ctx.run_ui(input, |ui| {
            seen = FrameInput::gather_for(ui.ctx(), L::Qwerty);
            consume_z(ui.ctx(), L::Qwerty, false);
            assert_eq!(ui.ctx().input(|i| i.events.len()), 1, "W is kept for flying");
        });
        out.textures_delta.clear();
        assert!(!seen.z_pressed, "QWERTY by label: W does not toggle mouselook");
    }
}
