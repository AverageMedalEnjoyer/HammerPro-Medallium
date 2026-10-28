use hp_kv::Block;

use crate::{Document, EditError, EditResult};

const LABEL: &str = "Edit map properties";

const RESERVED: &[&str] =
    &["versioninfo", "visgroups", "viewsettings", "world", "entity", "hidden", "cameras", "cordon", "cordons"];

fn check_name(name: &str) -> EditResult<()> {
    let name = name.trim();
    if name.is_empty() || name.chars().any(|c| c.is_whitespace() || c == '"' || c == '{' || c == '}') {
        return Err(EditError::Invalid(format!("'{name}' is not a valid block name")));
    }
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(name)) {
        return Err(EditError::Invalid(format!("'{name}' is a reserved block")));
    }
    Ok(())
}

impl Document {
    pub fn extra_block(&self, name: &str) -> Option<&hp_kv::Block> {
        self.scene.extra_blocks.iter().find(|b| b.name.eq_ignore_ascii_case(name))
    }

    pub fn set_extra_block(&mut self, name: &str, mut block: hp_kv::Block) -> EditResult<()> {
        check_name(name)?;
        let index = self.scene.extra_blocks.iter().position(|b| b.name.eq_ignore_ascii_case(name));
        block.name = match index {
            Some(i) => self.scene.extra_blocks[i].name.clone(),
            None => name.trim().to_owned(),
        };
        if index.is_some_and(|i| self.scene.extra_blocks[i] == block) {
            return Ok(());
        }
        self.tx(LABEL, |doc| {
            let blocks = doc.extra_blocks_mut();
            match index {
                Some(i) => blocks[i] = block,
                None => blocks.push(block),
            }
            Ok(())
        })
    }

    pub fn remove_extra_block(&mut self, name: &str) -> EditResult<Option<Block>> {
        let Some(index) = self.scene.extra_blocks.iter().position(|b| b.name.eq_ignore_ascii_case(name)) else {
            return Ok(None);
        };
        self.tx(LABEL, |doc| Ok(Some(doc.extra_blocks_mut().remove(index))))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use hp_kv::Block;

    use crate::Document;

    const HAMMER_PLUS_BLOCKS: &str = "palette_plus\n{\n\t\"color0\" \"255 255 255\"\n\t\"color1\" \"255 0 0\"\n}\ncolorcorrection_plus\n{\n\t\"name0\" \"\"\n\t\"weight0\" \"1\"\n}\nlight_plus\n{\n\t\"samples_sun\" \"6\"\n\t\"samples_ambient\" \"40\"\n\t\"samples_vis\" \"256\"\n\t\"texlight\" \"\"\n\t\"incremental_delay\" \"0\"\n\t\"bake_dist\" \"1024\"\n\t\"radius_scale\" \"1\"\n\t\"brightness_scale\" \"1\"\n\t\"ao_scale\" \"0\"\n\t\"bounced\" \"1\"\n\t\"incremental\" \"1\"\n\t\"supersample\" \"0\"\n\t\"bleed_hack\" \"1\"\n\t\"soften_cosine\" \"0\"\n\t\"debug\" \"0\"\n\t\"cubemap\" \"1\"\n\t\"hdr\" \"0\"\n}\npostprocess_plus\n{\n\t\"enable\" \"1\"\n\t\"copied_from_controller\" \"1\"\n\t\"bloom_scale\" \"1\"\n\t\"bloom_exponent\" \"2.5\"\n\t\"tonemap_percent_target\" \"60\"\n\t\"vignette_enable\" \"0\"\n}\nbgimages_plus\n{\n}\n";

    fn hammer_plus_vmf() -> String {
        let base = hp_vmf::write(&hp_vmf::Map::new_empty());
        format!("{base}{HAMMER_PLUS_BLOCKS}")
    }

    #[test]
    fn hammer_plus_blocks_round_trip_and_edit() {
        let text = hammer_plus_vmf();
        let mut doc = Document::from_map(hp_vmf::parse(&text).expect("parse"));
        let _ = doc.take_changes();
        assert_eq!(doc.to_vmf_string(), text);
        let names: Vec<&str> = doc.scene().extra_blocks().iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["palette_plus", "colorcorrection_plus", "light_plus", "postprocess_plus", "bgimages_plus"]);
        let light = doc.extra_block("LIGHT_PLUS").expect("light_plus");
        assert_eq!(light.get("samples_sun"), Some("6"));
        assert!(doc.extra_block("missing_plus").is_none());
        assert!(!doc.is_dirty());

        let mut edited = light.clone();
        edited.set("samples_sun", "12");
        doc.set_extra_block("light_plus", edited.clone()).expect("set");
        assert!(doc.is_dirty());
        assert_eq!(doc.undo_label(), Some("Edit map properties"));
        assert!(doc.take_changes().extra_blocks_changed);
        let out = doc.to_vmf_string();
        assert!(out.contains("\"samples_sun\" \"12\""), "{out}");
        assert_eq!(out.replace("\"samples_sun\" \"12\"", "\"samples_sun\" \"6\""), text, "only that key changed");
        let (undo, _) = doc.history_len();
        doc.set_extra_block("light_plus", edited).expect("same");
        assert_eq!(doc.history_len().0, undo);

        assert!(doc.undo());
        assert_eq!(doc.to_vmf_string(), text);
        assert!(!doc.is_dirty());
        assert!(doc.take_changes().extra_blocks_changed);
        assert!(doc.redo());
        assert_eq!(doc.extra_block("light_plus").and_then(|b| b.get("samples_sun")), Some("12"));

        let reloaded = Document::from_map(hp_vmf::parse(&doc.to_vmf_string()).expect("parse again"));
        assert_eq!(reloaded.extra_block("light_plus").and_then(|b| b.get("samples_sun")), Some("12"));
        assert_eq!(reloaded.to_vmf_string(), doc.to_vmf_string());
    }

    #[test]
    fn add_remove_and_reserved_names() {
        let mut doc = Document::new_empty();
        let mut block = Block::new("ignored");
        block.set("color0", "1 2 3");
        doc.set_extra_block("palette_plus", block).expect("add");
        let added = doc.extra_block("palette_plus").expect("added");
        assert_eq!(added.name, "palette_plus", "the block takes the name it is stored under");
        assert!(doc.to_vmf_string().contains("palette_plus\n{\n\t\"color0\" \"1 2 3\"\n}"));
        doc.begin("Edit palette");
        doc.set_extra_block("palette_plus", Block::new("palette_plus")).expect("replace");
        doc.commit();
        assert_eq!(doc.undo_label(), Some("Edit palette"));
        let removed = doc.remove_extra_block("PALETTE_PLUS").expect("remove");
        assert_eq!(removed.map(|b| b.name), Some("palette_plus".to_owned()));
        assert!(doc.extra_block("palette_plus").is_none());
        assert!(doc.remove_extra_block("palette_plus").expect("nothing").is_none());
        assert!(doc.undo());
        assert!(doc.extra_block("palette_plus").is_some());
        for bad in ["world", "Entity", "cameras", "", "two words", "a{b"] {
            assert!(doc.set_extra_block(bad, Block::new("x")).is_err(), "{bad:?}");
        }
    }
}
