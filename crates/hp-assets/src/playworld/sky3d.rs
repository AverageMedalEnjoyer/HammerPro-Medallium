use glam::DVec3;

use super::{disp_object, face_object};
use crate::AssetManager;

const ROOM_SLACK: f64 = 1.0;

pub(crate) fn in_room(min: [f32; 3], max: [f32; 3], room: (DVec3, DVec3)) -> bool {
    let (lo, hi) = (room.0 - DVec3::splat(ROOM_SLACK), room.1 + DVec3::splat(ROOM_SLACK));
    let (gmin, gmax) = (DVec3::from(min.map(f64::from)), DVec3::from(max.map(f64::from)));
    if !(gmin.is_finite() && gmax.is_finite() && gmin.cmple(gmax).all()) {
        return false;
    }
    let center = (gmin + gmax) * 0.5;
    center.cmpge(lo).all() && center.cmple(hi).all() && gmin.cmpge(lo).all() && gmax.cmple(hi).all()
}

impl AssetManager {
    pub(crate) fn compiled_sky_objects(&self, room: Option<(DVec3, DVec3)>) -> Vec<u32> {
        let (Some(room), Some((world, objects))) = (room, self.play_world.uploaded.as_ref()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (i, (group, o)) in world.groups.iter().zip(objects).enumerate() {
            if !in_room(group.min, group.max, room) {
                continue;
            }
            if o & 1 != 0 {
                out.push(face_object(i));
            }
            if o & 2 != 0 {
                out.push(disp_object(i));
            }
        }
        out.sort_unstable();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_inside_the_skybox_room() {
        let room = (DVec3::new(-100.0, -100.0, -500.0), DVec3::new(100.0, 100.0, -300.0));
        assert!(in_room([-100.0, -100.0, -500.0], [100.0, 100.0, -484.0], room), "the floor");
        assert!(in_room([-20.0, -20.0, -484.0], [20.0, 20.0, -400.0], room), "a building");
        assert!(!in_room([-2048.0, -2048.0, -16.0], [2048.0, 2048.0, 0.0], room), "the playable floor");
        assert!(!in_room([-20.0, -20.0, -484.0], [20.0, 20.0, 400.0], room), "crossing the room");
        assert!(!in_room([1.0, 0.0, 0.0], [0.0, 0.0, 0.0], room), "empty box");
    }
}
