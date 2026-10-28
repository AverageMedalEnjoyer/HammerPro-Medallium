pub mod overlay;

use rayon::prelude::*;

use crate::ids::{id_map, id_set};
use crate::{Document, FaceRef, Id, IdMap, IdSet};

pub const MAX_SIDE_LIST: usize = 4096;

pub fn parse_side_list(s: &str) -> Vec<Id> {
    let mut seen = id_set(0);
    let mut out = Vec::new();
    for token in s.split(|c: char| c.is_ascii_whitespace() || c == ',') {
        if out.len() >= MAX_SIDE_LIST {
            break;
        }
        let Ok(id) = token.parse::<Id>() else { continue };
        if id > 0 && seen.insert(id) {
            out.push(id);
        }
    }
    out
}

pub fn format_side_list(ids: &[Id]) -> String {
    let mut s = String::with_capacity(ids.len() * 6);
    for (i, id) in ids.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&id.to_string());
    }
    s
}

pub fn toggle_side(ids: &[Id], side: Id) -> Vec<Id> {
    if ids.contains(&side) { ids.iter().copied().filter(|s| *s != side).collect() } else { [ids, &[side]].concat() }
}

impl Document {
    pub fn find_sides_where(&self, wanted: &(dyn Fn(Id) -> bool + Sync)) -> IdMap<FaceRef> {
        let found: Vec<(Id, FaceRef)> = self
            .scene
            .solids
            .par_values()
            .flat_map_iter(|n| {
                let solid = n.solid.id;
                n.solid
                    .sides
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| wanted(s.id))
                    .map(move |(i, s)| (s.id, FaceRef::new(solid, i)))
                    .collect::<Vec<_>>()
            })
            .collect();
        let mut out = id_map(found.len());
        for (id, face) in found {
            out.entry(id).or_insert(face);
        }
        out
    }

    pub fn find_sides(&self, ids: &IdSet) -> IdMap<FaceRef> {
        if ids.is_empty() {
            return id_map(0);
        }
        self.find_sides_where(&|id| ids.contains(&id))
    }

    pub fn face_of_side(&self, side: Id) -> Option<FaceRef> {
        self.find_sides_where(&|id| id == side).get(&side).copied()
    }

    pub fn side_id(&self, face: FaceRef) -> Option<Id> {
        self.scene.solids.get(&face.solid)?.solid.sides.get(face.side).map(|s| s.id)
    }

    pub fn face_has_side(&self, face: FaceRef, side: Id) -> bool {
        self.side_id(face) == Some(side)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Document {
        let map = hp_vmf::parse(include_str!("side_index/test_map.vmf")).expect("test map");
        Document::from_map(map)
    }

    #[test]
    fn side_lists() {
        assert_eq!(parse_side_list("12 15 18"), vec![12, 15, 18]);
        assert_eq!(parse_side_list("  3,4 x 4 -2 0 5 "), vec![3, 4, 5]);
        assert!(parse_side_list("").is_empty());
        let huge = (1..10_000).map(|i| i.to_string()).collect::<Vec<_>>().join(" ");
        assert_eq!(parse_side_list(&huge).len(), MAX_SIDE_LIST);
        assert_eq!(format_side_list(&[1, 22, 3]), "1 22 3");
        assert_eq!(format_side_list(&[]), "");
        assert_eq!(toggle_side(&[1, 2], 3), vec![1, 2, 3]);
        assert_eq!(toggle_side(&[1, 2, 3], 2), vec![1, 3]);
    }

    #[test]
    fn finds_faces_of_sides() {
        let doc = doc();
        assert_eq!(doc.face_of_side(1), Some(FaceRef::new(2, 0)));
        assert_eq!(doc.face_of_side(12), Some(FaceRef::new(3, 5)));
        assert_eq!(doc.face_of_side(9999), None);
        let mut ids = id_set(0);
        ids.extend([1, 7, 9999]);
        let found = doc.find_sides(&ids);
        assert_eq!(found.len(), 2);
        assert_eq!(found.get(&7), Some(&FaceRef::new(3, 0)));
        assert_eq!(doc.side_id(FaceRef::new(2, 1)), Some(2));
        assert!(doc.face_has_side(FaceRef::new(2, 1), 2));
        assert!(!doc.face_has_side(FaceRef::new(2, 9), 2));
    }
}
