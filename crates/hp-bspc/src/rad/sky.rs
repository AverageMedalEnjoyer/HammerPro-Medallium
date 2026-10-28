use glam::Vec3A;

use super::bake::{Ctx, Part, Sample};
use super::lights::Kind;

const REL: f32 = 0.015;
const ABS: f32 = 0.5;

pub(crate) fn lights_of(ctx: &Ctx<'_>, cand: &[(usize, usize)]) -> Vec<(usize, usize)> {
    cand.iter()
        .copied()
        .filter(|&(li, _)| ctx.lights.list.get(li).is_some_and(|l| l.kind == Kind::SkyAmbient))
        .collect()
}

pub(crate) fn agree(buf: &[Vec3A], stride: usize, near: &[usize]) -> bool {
    let Some(&first) = near.first() else { return false };
    (0..stride).all(|k| {
        let (mut lo, mut hi) = (buf[first * stride + k], buf[first * stride + k]);
        for &j in &near[1..] {
            lo = lo.min(buf[j * stride + k]);
            hi = hi.max(buf[j * stride + k]);
        }
        let limit = hi.abs().max(lo.abs()) * REL + Vec3A::splat(ABS);
        (hi - lo).cmple(limit).all()
    })
}

pub(crate) fn face(
    ctx: &Ctx<'_>,
    lights: &[(usize, usize)],
    slots: usize,
    nn: usize,
    samples: &[Option<Sample>],
    acc: &mut [Vec3A],
) -> Vec<Vec3A> {
    let stride = ctx.passes * slots * nn;
    let mut sky = vec![Vec3A::ZERO; samples.len() * stride];
    for (i, sp) in samples.iter().enumerate() {
        if let Some(sp) = sp {
            ctx.gather(lights, slots, nn, sp, Part::Ambient, &mut sky[i * stride..(i + 1) * stride]);
        }
    }
    for (a, v) in acc.iter_mut().zip(&sky) {
        *a += *v;
    }
    sky
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agreement() {
        let buf = [
            Vec3A::new(100.0, 50.0, 0.0),
            Vec3A::new(101.0, 50.4, 0.3),
            Vec3A::new(104.0, 50.0, 0.0),
            Vec3A::ZERO,
            Vec3A::splat(0.4),
            Vec3A::splat(0.6),
        ];
        assert!(agree(&buf, 1, &[0, 1]));
        assert!(!agree(&buf, 1, &[0, 2]));
        assert!(!agree(&buf, 1, &[0, 1, 2]));
        assert!(agree(&buf, 1, &[0]));
        assert!(!agree(&buf, 1, &[]));
        assert!(agree(&buf, 1, &[3, 4]));
        assert!(!agree(&buf, 1, &[3, 5]));
        assert!(!agree(&buf, 2, &[0, 1]));
    }
}
