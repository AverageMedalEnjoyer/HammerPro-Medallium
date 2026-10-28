use glam::{DMat3, DQuat, DVec3};

pub fn angle_matrix(angles: DVec3) -> DMat3 {
    let (sp, cp) = angles.x.to_radians().sin_cos();
    let (sy, cy) = angles.y.to_radians().sin_cos();
    let (sr, cr) = angles.z.to_radians().sin_cos();
    let forward = DVec3::new(cp * cy, cp * sy, -sp);
    let left = DVec3::new(sp * sr * cy - cr * sy, sp * sr * sy + cr * cy, sr * cp);
    let up = DVec3::new(sp * cr * cy + sr * sy, sp * cr * sy - sr * cy, cr * cp);
    DMat3::from_cols(forward, left, up)
}

pub fn matrix_angles(m: &DMat3) -> DVec3 {
    let forward = m.x_axis;
    let left = m.y_axis;
    let up_z = m.z_axis.z;
    let xy = (forward.x * forward.x + forward.y * forward.y).sqrt();
    let (pitch, yaw, roll) = if xy > 0.001 {
        ((-forward.z).atan2(xy), forward.y.atan2(forward.x), left.z.atan2(up_z))
    } else {
        ((-forward.z).atan2(xy), (-left.x).atan2(left.y), 0.0)
    };
    let a = DVec3::new(pitch.to_degrees(), yaw.to_degrees(), roll.to_degrees());
    if a.is_finite() { a } else { DVec3::ZERO }
}

fn quat_normalized(q: DQuat) -> Option<DQuat> {
    let l = q.length();
    (l.is_finite() && l > 1e-12).then(|| q * (1.0 / l))
}

pub fn angles_to_quat(angles: DVec3) -> DQuat {
    DQuat::from_mat3(&angle_matrix(angles)).normalize()
}

pub fn quat_to_angles(q: DQuat) -> DVec3 {
    match quat_normalized(q) {
        Some(q) => matrix_angles(&DMat3::from_quat(q)),
        None => DVec3::ZERO,
    }
}

pub fn angles_to_vectors(angles: DVec3) -> (DVec3, DVec3, DVec3) {
    let m = angle_matrix(angles);
    (m.x_axis, -m.y_axis, m.z_axis)
}

pub fn clean_angles(angles: DVec3) -> DVec3 {
    fn c(x: f64) -> f64 {
        if !x.is_finite() {
            return 0.0;
        }
        let r = x.round();
        let v = if (x - r).abs() < 1e-6 { r } else { x };
        if v == -180.0 { 180.0 } else { v + 0.0 }
    }
    DVec3::new(c(angles.x), c(angles.y), c(angles.z))
}

pub fn transform_angles(angles: DVec3, rotation: DQuat) -> DVec3 {
    let r = match quat_normalized(rotation) {
        Some(q) => DMat3::from_quat(q),
        None => DMat3::IDENTITY,
    };
    transform_angles_mat(angles, &r)
}

pub fn transform_angles_mat(angles: DVec3, linear: &DMat3) -> DVec3 {
    let m = *linear * angle_matrix(angles);
    let Some(forward) = m.x_axis.try_normalize() else {
        return clean_angles(angles);
    };
    let up_raw = m.z_axis - forward * forward.dot(m.z_axis);
    let Some(up) = up_raw.try_normalize() else {
        return clean_angles(matrix_angles(&DMat3::from_cols(forward, m.y_axis, m.z_axis)));
    };
    let left = up.cross(forward);
    clean_angles(matrix_angles(&DMat3::from_cols(forward, left, up)))
}

pub fn align_to_normal(normal: DVec3) -> (DQuat, DVec3) {
    let n = normal.try_normalize().filter(|n| n.is_finite()).unwrap_or(DVec3::Z);
    let q = DQuat::from_rotation_arc(DVec3::Z, n);
    let q = if q.is_finite() { q } else { DQuat::IDENTITY };
    (q, clean_angles(quat_to_angles(q)))
}

pub fn align_angles_to_normal(angles: DVec3, normal: DVec3) -> DVec3 {
    let Some(n) = normal.try_normalize().filter(|n| n.is_finite()) else {
        return clean_angles(angles);
    };
    let up = angle_matrix(angles).z_axis;
    let q = DQuat::from_rotation_arc(up, n);
    if !q.is_finite() {
        return clean_angles(angles);
    }
    transform_angles(angles, q)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::Rng;

    #[test]
    fn align_to_surfaces() {
        let (q, a) = align_to_normal(DVec3::Z);
        assert!(q.abs_diff_eq(DQuat::IDENTITY, 1e-12));
        assert_eq!(a, DVec3::ZERO);
        let (q, a) = align_to_normal(DVec3::X);
        assert!(close(q * DVec3::Z, DVec3::X));
        assert!(close(angle_matrix(a).z_axis, DVec3::X), "{a:?}");
        assert_eq!(align_to_normal(DVec3::ZERO).1, DVec3::ZERO);
        let n = DVec3::new(0.5, 0.0, 1.0).normalize();
        let a = align_angles_to_normal(DVec3::new(0.0, 90.0, 0.0), n);
        let m = angle_matrix(a);
        assert!(close(m.z_axis, n), "{a:?}");
        assert!(m.x_axis.y > 0.99, "still facing +Y: {:?}", m.x_axis);
        let a = align_angles_to_normal(DVec3::ZERO, DVec3::NEG_Z);
        assert!(close(angle_matrix(a).z_axis, DVec3::NEG_Z), "{a:?}");
        assert_eq!(align_angles_to_normal(DVec3::new(0.0, 45.0, 0.0), DVec3::NAN), DVec3::new(0.0, 45.0, 0.0));
    }

    fn close(a: DVec3, b: DVec3) -> bool {
        (a - b).length() < 1e-9
    }

    #[test]
    fn known_source_values() {
        let (f, r, u) = angles_to_vectors(DVec3::new(0.0, 90.0, 0.0));
        assert!(close(f, DVec3::Y) && close(r, DVec3::X) && close(u, DVec3::Z));
        let (f, _, u) = angles_to_vectors(DVec3::new(90.0, 0.0, 0.0));
        assert!(close(f, DVec3::NEG_Z) && close(u, DVec3::X));
        let m = angle_matrix(DVec3::new(0.0, 0.0, 90.0));
        assert!(close(m.y_axis, DVec3::Z) && close(m.z_axis, DVec3::NEG_Y));
        assert!((angle_matrix(DVec3::ZERO) - DMat3::IDENTITY).abs_diff_eq(DMat3::ZERO, 1e-15));
        let (f, _, _) = angles_to_vectors(DVec3::new(30.0, 45.0, 0.0));
        let c30 = 30f64.to_radians().cos();
        let e = DVec3::new(c30 * 45f64.to_radians().cos(), c30 * 45f64.to_radians().sin(), -0.5);
        assert!(close(f, e));
    }

    #[test]
    fn round_trips() {
        let cases = [
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(30.0, 45.0, 60.0),
            DVec3::new(-45.0, 135.0, -30.0),
            DVec3::new(10.0, -170.0, 179.0),
            DVec3::new(89.0, 12.0, 5.0),
            DVec3::new(-89.0, 180.0, 0.0),
        ];
        for a in cases {
            let back = matrix_angles(&angle_matrix(a));
            assert!(close(clean_angles(back), clean_angles(a)), "{a:?} -> {back:?}");
            let q = angles_to_quat(a);
            assert!(close(clean_angles(quat_to_angles(q)), clean_angles(a)));
        }
        let mut rng = Rng(77);
        for _ in 0..500 {
            let a = DVec3::new(rng.range(-89.0, 89.0), rng.range(-180.0, 180.0), rng.range(-180.0, 180.0));
            let m1 = angle_matrix(a);
            let m2 = angle_matrix(quat_to_angles(angles_to_quat(a)));
            assert!(m1.abs_diff_eq(m2, 1e-6), "{a:?}");
        }
    }

    #[test]
    fn gimbal_lock() {
        let a = matrix_angles(&angle_matrix(DVec3::new(90.0, 30.0, 0.0)));
        assert!((a.x - 90.0).abs() < 1e-9);
        assert_eq!(a.z, 0.0);
        let m = angle_matrix(a);
        assert!(m.abs_diff_eq(angle_matrix(DVec3::new(90.0, 30.0, 0.0)), 1e-9));
    }

    #[test]
    fn rotate_entity() {
        let q = DQuat::from_rotation_z(90f64.to_radians());
        assert_eq!(transform_angles(DVec3::ZERO, q), DVec3::new(0.0, 90.0, 0.0));
        assert_eq!(transform_angles(DVec3::new(0.0, 90.0, 0.0), q), DVec3::new(0.0, 180.0, 0.0));
        assert_eq!(transform_angles(DVec3::new(0.0, 180.0, 0.0), q), DVec3::new(0.0, -90.0, 0.0));
        let q = DQuat::from_rotation_y(90f64.to_radians());
        let a = transform_angles(DVec3::ZERO, q);
        assert!((a.x - 90.0).abs() < 1e-9, "{a:?}");
        let a = transform_angles_mat(DVec3::new(0.0, 30.0, 0.0), &DMat3::from_diagonal(DVec3::new(-1.0, 1.0, 1.0)));
        assert!((a.y - 150.0).abs() < 1e-9, "{a:?}");
    }
}
