use std::collections::HashMap;

use egui::{Event, Key, KeyboardShortcut};
use hp_app::Action;
use hp_app::actions::{KeyBindings, clipboard_event_shortcut, format_shortcut, shortcut_from_event, shortcut_label};

pub use hp_app::actions::normalize_modifiers;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capture {
    Pending,
    Cancel,
    Set(KeyboardShortcut),
}

fn is_reserved_alone(key: Key) -> bool {
    matches!(key, Key::Escape | Key::Tab | Key::Enter)
}

pub fn is_clipboard_event(event: &Event) -> bool {
    matches!(event, Event::Copy | Event::Cut | Event::Paste(_) | Event::PasteImage(_))
}

fn clipboard_shortcut(event: &Event) -> Option<KeyboardShortcut> {
    match event {
        Event::PasteImage(_) => clipboard_event_shortcut(&Event::Paste(String::new())),
        _ => clipboard_event_shortcut(event),
    }
}

pub fn capture_from_events(events: &[Event]) -> Capture {
    for event in events {
        let sc = match event {
            Event::Key { key, pressed: true, repeat: false, modifiers, .. } => {
                let mods = normalize_modifiers(*modifiers);
                let plain = !mods.alt && !mods.shift && !mods.command;
                if *key == Key::Escape && plain {
                    return Capture::Cancel;
                }
                if plain && is_reserved_alone(*key) {
                    continue;
                }
                shortcut_from_event(event)
            }
            Event::Key { .. } => None,
            _ => clipboard_shortcut(event),
        };
        if let Some(sc) = sc {
            return Capture::Set(sc);
        }
    }
    Capture::Pending
}

pub fn shortcut_key(sc: &KeyboardShortcut) -> String {
    format_shortcut(sc)
}

pub fn conflicts(keys: &KeyBindings) -> HashMap<String, Vec<Action>> {
    let mut map: HashMap<String, Vec<Action>> = HashMap::new();
    for &action in Action::ALL {
        if let Some(sc) = keys.shortcut(action) {
            map.entry(shortcut_key(&sc)).or_default().push(action);
        }
    }
    map.retain(|_, v| v.len() > 1);
    map
}

pub fn is_customized(keys: &KeyBindings, action: Action) -> bool {
    keys.is_customized(action)
}

pub fn alias_text(keys: &KeyBindings, action: Action) -> String {
    keys.aliases(action).iter().map(shortcut_label).collect::<Vec<_>>().join(" / ")
}

pub fn actions_bound_to(keys: &KeyBindings, sc: &KeyboardShortcut) -> Vec<Action> {
    let wanted = shortcut_key(sc);
    Action::ALL.iter().copied().filter(|&a| keys.shortcut(a).is_some_and(|s| shortcut_key(&s) == wanted)).collect()
}

pub fn shortcut_text(keys: &KeyBindings, action: Action) -> String {
    keys.shortcut(action).map(|s| shortcut_label(&s)).unwrap_or_default()
}

pub fn shortcut_search_text(keys: &KeyBindings, action: Action) -> String {
    let mut out = String::new();
    for sc in keys.shortcut(action).into_iter().chain(keys.aliases(action)) {
        let label = shortcut_label(&sc);
        let key = format_shortcut(&sc);
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&label);
        if label != key {
            out.push(' ');
            out.push_str(&key);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use egui::Modifiers;

    use super::*;

    fn key_event(key: Key, modifiers: Modifiers) -> Event {
        Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    fn layout_event(key: Key, physical: Key, modifiers: Modifiers) -> Event {
        Event::Key { key, physical_key: Some(physical), pressed: true, repeat: false, modifiers }
    }

    const CTRL: Modifiers = Modifiers { alt: false, ctrl: true, shift: false, mac_cmd: false, command: true };

    #[test]
    fn capture_records_digits_by_physical_key() {
        let got = capture_from_events(&[layout_event(Key::Quote, Key::Num4, Modifiers::ALT)]);
        let expected = KeyboardShortcut::new(Modifiers::ALT, Key::Num4);
        assert_eq!(got, Capture::Set(expected));
        let mut keys = KeyBindings::default();
        keys.set(Action::RunMap, Some(expected));
        let mut events = vec![layout_event(Key::Quote, Key::Num4, Modifiers::ALT)];
        assert_eq!(keys.take_triggered(&mut events), vec![Action::RunMap]);
        assert_eq!(
            capture_from_events(&[layout_event(Key::Num1, Key::Num1, Modifiers::NONE)]),
            Capture::Set(KeyboardShortcut::new(Modifiers::NONE, Key::Num1))
        );
        assert_eq!(
            capture_from_events(&[layout_event(Key::Minus, Key::Num6, CTRL)]),
            Capture::Set(KeyboardShortcut::new(Modifiers::COMMAND, Key::Num6))
        );
        assert_eq!(
            capture_from_events(&[layout_event(Key::A, Key::Q, CTRL)]),
            Capture::Set(KeyboardShortcut::new(Modifiers::COMMAND, Key::A))
        );
        assert_eq!(capture_from_events(&[layout_event(Key::Escape, Key::Escape, Modifiers::NONE)]), Capture::Cancel);
    }

    #[test]
    fn capture_clipboard_events_as_ctrl_c_x_v() {
        let ctrl = |key| Capture::Set(KeyboardShortcut::new(Modifiers::COMMAND, key));
        assert_eq!(capture_from_events(&[Event::Copy]), ctrl(Key::C));
        assert_eq!(capture_from_events(&[Event::Cut]), ctrl(Key::X));
        assert_eq!(capture_from_events(&[Event::Paste("text".into())]), ctrl(Key::V));
        let image = std::sync::Arc::new(egui::ColorImage::filled([1, 1], egui::Color32::WHITE));
        assert_eq!(capture_from_events(&[Event::PasteImage(image)]), ctrl(Key::V));
        assert_eq!(capture_from_events(&[Event::Cut, key_event(Key::K, Modifiers::ALT)]), ctrl(Key::X));
        assert_eq!(
            capture_from_events(&[key_event(Key::K, Modifiers::ALT), Event::Cut]),
            Capture::Set(KeyboardShortcut::new(Modifiers::ALT, Key::K))
        );
        if let Capture::Set(sc) = capture_from_events(&[Event::Copy]) {
            assert_eq!(format_shortcut(&sc), "Ctrl+C");
            let mut keys = KeyBindings::default();
            keys.set(Action::Copy, None);
            keys.set(Action::RunMap, Some(sc));
            let mut events = vec![Event::Copy];
            assert_eq!(keys.take_triggered(&mut events), vec![Action::RunMap]);
        }
        assert!(is_clipboard_event(&Event::Copy) && is_clipboard_event(&Event::Paste(String::new())));
        assert!(!is_clipboard_event(&key_event(Key::C, CTRL)) && !is_clipboard_event(&Event::Text("c".into())));
    }

    #[test]
    fn capture_plain_and_modified_keys() {
        assert_eq!(capture_from_events(&[]), Capture::Pending);
        assert_eq!(
            capture_from_events(&[key_event(Key::F5, Modifiers::NONE)]),
            Capture::Set(KeyboardShortcut::new(Modifiers::NONE, Key::F5))
        );
        let ctrl_shift = Modifiers { ctrl: true, command: true, shift: true, ..Modifiers::NONE };
        let got = capture_from_events(&[key_event(Key::S, ctrl_shift)]);
        let expected = KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
        assert_eq!(got, Capture::Set(expected));
        if let Capture::Set(sc) = got {
            assert_eq!(format_shortcut(&sc), "Ctrl+Shift+S");
            assert_eq!(hp_app::actions::parse_shortcut(&format_shortcut(&sc)), Some(sc));
        }
    }

    #[test]
    fn capture_escape_cancels_but_ctrl_escape_binds() {
        assert_eq!(capture_from_events(&[key_event(Key::Escape, Modifiers::NONE)]), Capture::Cancel);
        assert_eq!(
            capture_from_events(&[key_event(Key::Escape, Modifiers::SHIFT)]),
            Capture::Set(KeyboardShortcut::new(Modifiers::SHIFT, Key::Escape))
        );
    }

    #[test]
    fn capture_waits_for_the_key_after_modifiers() {
        let ctrl = Modifiers::CTRL;
        let both = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(capture_from_events(&[key_event(Key::ControlLeft, ctrl)]), Capture::Pending);
        assert_eq!(capture_from_events(&[key_event(Key::ShiftLeft, both)]), Capture::Pending);
        let events = [key_event(Key::ControlLeft, ctrl), key_event(Key::ShiftLeft, both), key_event(Key::S, both)];
        let expected = KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
        assert_eq!(capture_from_events(&events), Capture::Set(expected));
    }

    #[test]
    fn capture_ignores_releases_repeats_and_reserved_keys() {
        let release =
            Event::Key { key: Key::A, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE };
        let repeat =
            Event::Key { key: Key::B, physical_key: None, pressed: true, repeat: true, modifiers: Modifiers::NONE };
        let tab = key_event(Key::Tab, Modifiers::NONE);
        let text = Event::Text("a".into());
        assert_eq!(capture_from_events(&[release, repeat, tab, text]), Capture::Pending);
        assert_eq!(
            capture_from_events(&[key_event(Key::Enter, Modifiers::COMMAND)]),
            Capture::Set(KeyboardShortcut::new(Modifiers::COMMAND, Key::Enter))
        );
    }

    #[test]
    fn first_usable_press_wins() {
        let events = [
            key_event(Key::Tab, Modifiers::NONE),
            key_event(Key::K, Modifiers::ALT),
            key_event(Key::J, Modifiers::NONE),
        ];
        assert_eq!(capture_from_events(&events), Capture::Set(KeyboardShortcut::new(Modifiers::ALT, Key::K)));
    }

    #[test]
    fn display_text_is_localized_and_search_text_has_both_forms() {
        let _g = crate::tests::lang_guard();
        crate::register_translations();
        let mut keys = KeyBindings::default();
        let sc = KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
        keys.set(Action::SaveMapAs, Some(sc));
        hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
        assert_eq!(shortcut_text(&keys, Action::SaveMapAs), "Ctrl+Maj+S");
        let search = shortcut_search_text(&keys, Action::SaveMapAs);
        assert!(search.contains("Ctrl+Maj+S") && search.contains("Ctrl+Shift+S"), "{search}");
        keys.set(Action::Delete, Some(KeyboardShortcut::new(Modifiers::NONE, Key::Delete)));
        assert_eq!(shortcut_text(&keys, Action::Delete), "Suppr");
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        assert_eq!(shortcut_text(&keys, Action::SaveMapAs), "Ctrl+Shift+S");
        assert_eq!(shortcut_search_text(&keys, Action::SaveMapAs), "Ctrl+Shift+S");
    }

    #[test]
    fn aliases_are_searchable_and_shown() {
        let _g = crate::tests::lang_guard();
        crate::register_translations();
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        let keys = KeyBindings::default();
        assert_eq!(alias_text(&keys, Action::SelectNone), "Shift+Q");
        assert!(shortcut_search_text(&keys, Action::SelectNone).contains("Shift+Q"));
        assert_eq!(alias_text(&keys, Action::Undo), "");
        let hammer =
            KeyBindings::from_preset(hp_app::actions::KeymapPreset::Hammer, &std::collections::BTreeMap::new());
        assert_eq!(shortcut_text(&hammer, Action::ToggleGrid), "Shift+R");
        assert!(!is_customized(&hammer, Action::ToggleGrid), "a preset default is not a customization");
        assert!(conflicts(&hammer).is_empty(), "the Hammer keys do not conflict");
        let pp =
            KeyBindings::from_preset(hp_app::actions::KeymapPreset::HammerPlusPlus, &std::collections::BTreeMap::new());
        assert!(conflicts(&pp).is_empty(), "the Hammer++ keys do not conflict");
    }

    #[test]
    fn conflict_detection() {
        let _g = crate::tests::lang_guard();
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        let mut keys = KeyBindings::default();
        assert!(conflicts(&keys).is_empty(), "default bindings must not conflict");
        let save = keys.shortcut(Action::SaveMap);
        keys.set(Action::Copy, save);
        let c = conflicts(&keys);
        let list = c.get("Ctrl+S").cloned().unwrap_or_default();
        assert!(list.contains(&Action::SaveMap) && list.contains(&Action::Copy), "{c:?}");
        assert!(is_customized(&keys, Action::Copy));
        assert!(!is_customized(&keys, Action::SaveMap));
        let sc = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
        assert_eq!(actions_bound_to(&keys, &sc).len(), 2);
        keys.set(Action::Copy, Action::Copy.default_shortcut());
        assert!(!is_customized(&keys, Action::Copy));
        assert_eq!(shortcut_text(&keys, Action::Copy), "Ctrl+C");
        keys.set(Action::Copy, None);
        assert_eq!(shortcut_text(&keys, Action::Copy), "");
    }
}
