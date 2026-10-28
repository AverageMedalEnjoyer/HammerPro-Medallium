#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AtlasRect {
    pub layer: u32,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl AtlasRect {
    pub fn area(&self) -> u64 {
        u64::from(self.w) * u64::from(self.h)
    }
}

#[derive(Clone, Debug)]
struct Shelf {
    y: u32,
    h: u32,
    x: u32,
}

#[derive(Clone, Debug, Default)]
struct Layer {
    shelves: Vec<Shelf>,
    next_y: u32,
    live: u64,
    count: u32,
}

#[derive(Clone, Debug)]
pub struct Atlas {
    size: u32,
    max_layers: u32,
    layers: Vec<Layer>,
    live: u64,
}

impl Atlas {
    pub fn new(size: u32, max_layers: u32) -> Self {
        Self { size: size.clamp(64, 16384), max_layers: max_layers.clamp(1, 256), layers: Vec::new(), live: 0 }
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    pub fn layers(&self) -> u32 {
        self.layers.len() as u32
    }

    pub fn max_layers(&self) -> u32 {
        self.max_layers
    }

    pub fn live_texels(&self) -> u64 {
        self.live
    }

    pub fn waste(&self) -> f64 {
        let total = u64::from(self.size) * u64::from(self.size) * self.layers.len() as u64;
        if total == 0 { 0.0 } else { 1.0 - self.live as f64 / total as f64 }
    }

    pub fn clear(&mut self) {
        self.layers.clear();
        self.live = 0;
    }

    fn alloc_in(layer: &mut Layer, size: u32, w: u32, h: u32) -> Option<(u32, u32)> {
        let mut best: Option<usize> = None;
        for (i, s) in layer.shelves.iter().enumerate() {
            if s.h >= h
                && s.h <= h.saturating_mul(2).max(h + 4)
                && size - s.x >= w
                && best.is_none_or(|b| layer.shelves[b].h > s.h)
            {
                best = Some(i);
            }
        }
        if let Some(i) = best {
            let s = &mut layer.shelves[i];
            let at = (s.x, s.y);
            s.x += w;
            return Some(at);
        }
        if size - layer.next_y >= h {
            let y = layer.next_y;
            layer.next_y += h;
            layer.shelves.push(Shelf { y, h, x: w });
            return Some((0, y));
        }
        None
    }

    pub fn alloc(&mut self, w: u32, h: u32) -> Option<AtlasRect> {
        if w == 0 || h == 0 || w > self.size || h > self.size {
            return None;
        }
        let size = self.size;
        for (i, layer) in self.layers.iter_mut().enumerate() {
            if let Some((x, y)) = Self::alloc_in(layer, size, w, h) {
                return Some(self.commit(i, x, y, w, h));
            }
        }
        if self.layers.len() as u32 >= self.max_layers {
            return None;
        }
        let mut layer = Layer::default();
        let (x, y) = Self::alloc_in(&mut layer, size, w, h)?;
        self.layers.push(layer);
        Some(self.commit(self.layers.len() - 1, x, y, w, h))
    }

    fn commit(&mut self, layer: usize, x: u32, y: u32, w: u32, h: u32) -> AtlasRect {
        let r = AtlasRect { layer: layer as u32, x, y, w, h };
        if let Some(l) = self.layers.get_mut(layer) {
            l.live += r.area();
            l.count += 1;
        }
        self.live += r.area();
        r
    }

    pub fn free(&mut self, r: AtlasRect) {
        let Some(l) = self.layers.get_mut(r.layer as usize) else { return };
        if l.count == 0 {
            return;
        }
        l.live = l.live.saturating_sub(r.area());
        l.count -= 1;
        self.live = self.live.saturating_sub(r.area());
        if l.count == 0 {
            *l = Layer::default();
        }
    }

    pub fn alloc_many(&mut self, sizes: &[(u32, u32)]) -> Vec<Option<AtlasRect>> {
        let mut order: Vec<usize> = (0..sizes.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse((sizes[i].1, sizes[i].0)));
        let mut out = vec![None; sizes.len()];
        for i in order {
            let (w, h) = sizes[i];
            out[i] = self.alloc(w, h);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlaps(a: &AtlasRect, b: &AtlasRect) -> bool {
        a.layer == b.layer && a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    }

    #[test]
    fn packs_without_overlap() {
        let mut a = Atlas::new(256, 4);
        let sizes: Vec<(u32, u32)> = (0..300).map(|i| (1 + (i * 7) % 33, 1 + (i * 13) % 29)).collect();
        let rects: Vec<AtlasRect> = a.alloc_many(&sizes).into_iter().map(Option::unwrap).collect();
        for (i, r) in rects.iter().enumerate() {
            assert_eq!((r.w, r.h), sizes[i]);
            assert!(r.x + r.w <= 256 && r.y + r.h <= 256);
            for s in &rects[..i] {
                assert!(!overlaps(r, s), "{r:?} overlaps {s:?}");
            }
        }
        assert!(a.layers() >= 1);
        assert_eq!(a.live_texels(), rects.iter().map(AtlasRect::area).sum::<u64>());
    }

    #[test]
    fn limits_and_reuse() {
        let mut a = Atlas::new(64, 1);
        assert!(a.alloc(0, 4).is_none());
        assert!(a.alloc(65, 4).is_none());
        let big = a.alloc(64, 64).unwrap();
        assert!(a.alloc(1, 1).is_none(), "the only layer is full");
        a.free(big);
        assert_eq!(a.live_texels(), 0);
        assert!(a.alloc(64, 64).is_some());
        a.free(AtlasRect { layer: 9, x: 0, y: 0, w: 1, h: 1 });
        a.clear();
        assert_eq!(a.layers(), 0);
        assert_eq!(a.waste(), 0.0);
    }
}
