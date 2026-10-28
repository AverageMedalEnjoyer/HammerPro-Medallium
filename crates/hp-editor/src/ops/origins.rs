use super::clean_vec;
use crate::document::Document;
use crate::{EditResult, Id};

impl Document {
    fn origin_center(&self, id: Id) -> Option<(Id, String)> {
        let node = self.scene.entities.get(&id)?;
        let current = node.entity.get("origin")?;
        let b = self.scene.entity_solid_bounds(id)?;
        if b.is_empty() || !b.min.is_finite() || !b.max.is_finite() {
            return None;
        }
        let center = clean_vec(b.center());
        let text = hp_vmf::fmt_vec3(center);
        let same = hp_vmf::parse_vec3(current).is_some_and(|o| clean_vec(o) == center);
        (!same).then_some((id, text))
    }

    pub fn center_origins(&mut self, entities: &[Id]) -> EditResult<usize> {
        let mut seen = crate::ids::id_set(entities.len());
        let moves: Vec<(Id, String)> =
            entities.iter().filter(|id| seen.insert(**id)).filter_map(|id| self.origin_center(*id)).collect();
        if moves.is_empty() {
            return Ok(0);
        }
        self.tx("Center origins", |doc| {
            for (id, origin) in &moves {
                doc.update_entity(*id, |e| e.set("origin", origin.as_str()));
            }
            Ok(moves.len())
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::Document;

    #[test]
    fn origins_move_to_the_solid_center() {
        let text = r#"world { "id" "1" "classname" "worldspawn" }
entity
{
	"id" "2"
	"classname" "func_rotating"
	"origin" "0 0 0"
	solid
	{
		"id" "3"
		side { "id" "1" "plane" "(64 64 64) (128 64 64) (128 0 64)" "material" "A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" }
		side { "id" "2" "plane" "(64 0 0) (128 0 0) (128 64 0)" "material" "A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" }
		side { "id" "3" "plane" "(64 64 64) (64 0 64) (64 0 0)" "material" "A" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
		side { "id" "4" "plane" "(128 64 0) (128 0 0) (128 0 64)" "material" "A" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
		side { "id" "5" "plane" "(128 64 64) (64 64 64) (64 64 0)" "material" "A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
		side { "id" "6" "plane" "(128 0 0) (64 0 0) (64 0 64)" "material" "A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
	}
}
entity { "id" "4" "classname" "info_target" "origin" "5 5 5" }
"#;
        let mut doc = Document::from_map(hp_vmf::parse(text).expect("map"));
        let steps = doc.history_len().0;
        assert_eq!(doc.center_origins(&[2, 2, 4, 99]).expect("center"), 1);
        assert_eq!(doc.scene().entity(2).and_then(|n| n.entity.get("origin")), Some("96 32 32"));
        assert_eq!(doc.scene().entity(4).and_then(|n| n.entity.get("origin")), Some("5 5 5"), "point entity");
        assert_eq!(doc.history_len().0, steps + 1);
        assert_eq!(doc.undo_label(), Some("Center origins"));
        assert_eq!(doc.center_origins(&[2]).expect("center"), 0);
        assert_eq!(doc.history_len().0, steps + 1);
    }
}
