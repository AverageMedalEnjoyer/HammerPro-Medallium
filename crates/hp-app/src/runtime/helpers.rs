pub mod geometry;
pub mod names;

use std::cell::{Cell, Ref, RefCell};

use hp_editor::Document;

use crate::settings::Settings;

pub use names::NameIndex;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SceneDisplay {
    pub overlays: bool,
    pub ropes: bool,
    pub simulate_ropes: bool,
    pub editor_objects: bool,
    pub game_look: bool,
}

impl Default for SceneDisplay {
    fn default() -> Self {
        Self { overlays: true, ropes: true, simulate_ropes: false, editor_objects: true, game_look: false }
    }
}

impl SceneDisplay {
    pub fn from_settings(s: &Settings) -> Self {
        let h = &s.helpers;
        Self {
            overlays: h.show_overlays,
            ropes: h.show_ropes,
            simulate_ropes: h.simulate_ropes,
            editor_objects: h.show_editor_objects,
            game_look: false,
        }
    }
}

thread_local! {
    static DISPLAY: Cell<SceneDisplay> = Cell::new(SceneDisplay::default());
}

pub fn publish_scene_display(display: SceneDisplay) {
    DISPLAY.with(|d| d.set(display));
}

pub fn scene_display() -> SceneDisplay {
    DISPLAY.with(Cell::get)
}

#[derive(Debug, Default)]
pub struct HelpersRuntime {
    names: RefCell<NameIndex>,
    point_entities_hidden: Cell<Option<(u64, bool)>>,
}

impl HelpersRuntime {
    pub fn names(&self, doc: &Document) -> Ref<'_, NameIndex> {
        if let Ok(mut n) = self.names.try_borrow_mut() {
            n.refresh(doc);
        }
        self.names.borrow()
    }

    pub fn point_entities_hidden(&self, doc: &Document) -> bool {
        let rev = doc.revision();
        if let Some((r, v)) = self.point_entities_hidden.get()
            && r == rev
        {
            return v;
        }
        let scene = doc.scene();
        let mut any = false;
        let mut all_hidden = true;
        for (id, n) in scene.entities() {
            if scene.is_brush_entity(*id) {
                continue;
            }
            any = true;
            if n.entity.editor.visgroup_auto_shown {
                all_hidden = false;
                break;
            }
        }
        let v = any && all_hidden;
        self.point_entities_hidden.set(Some((rev, v)));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_follows_settings_per_thread() {
        let mut s = Settings::default();
        assert_eq!(SceneDisplay::from_settings(&s), SceneDisplay::default());
        s.helpers.show_overlays = false;
        s.helpers.simulate_ropes = true;
        let d = SceneDisplay::from_settings(&s);
        publish_scene_display(d);
        assert_eq!(scene_display(), d);
        let other = std::thread::spawn(scene_display).join().expect("thread");
        assert_eq!(other, SceneDisplay::default(), "each thread has its own");
        publish_scene_display(SceneDisplay::default());
    }

    #[test]
    fn point_entities_hidden_state() {
        let mut state = crate::testing::test_state();
        let rt = HelpersRuntime::default();
        assert!(!rt.point_entities_hidden(&state.doc));
        let point = |scene: &hp_editor::Scene, obj: hp_editor::ObjRef| match obj {
            hp_editor::ObjRef::Entity(id) => !scene.is_brush_entity(id),
            hp_editor::ObjRef::Solid(_) => false,
        };
        assert_eq!(state.doc.set_auto_visibility(&point, false), 2);
        assert!(rt.point_entities_hidden(&state.doc));
        assert_eq!(rt.names(&state.doc).named("main_light"), &[5]);
        let a = rt.names(&state.doc);
        let b = rt.names(&state.doc);
        assert_eq!(a.fingerprint(), b.fingerprint());
    }
}
