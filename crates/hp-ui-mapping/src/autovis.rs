use std::collections::{HashMap, HashSet};

use hp_editor::{Document, EntityNode, Id, ObjRef, Owner, Scene, SolidNode};

use crate::util::{contains_ci, is_nodraw_material, is_tool_material, strip_prefix_ci};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AutoKind {
    WorldBrushes,
    Sky,
    Displacements,
    Water,
    NodrawBrushes,
    WorldDetails,
    FuncDetail,
    Props,
    Entities,
    PointEntities,
    BrushEntities,
    Triggers,
    Lights,
    Npcs,
    Sprites,
    Cubemaps,
    Overlays,
    Instances,
    ToolBrushes,
    Clips,
    HintSkip,
    Areaportals,
    Occluders,
    Invisible,
    Ladders,
    OtherTools,
    Hidden,
}

const S_WORLD: u32 = 1;
const S_DISP: u32 = 1 << 1;
const S_HIDDEN: u32 = 1 << 2;
const S_ALL_TOOL: u32 = 1 << 3;
const S_NODRAW: u32 = 1 << 4;
const S_SKY: u32 = 1 << 5;
const S_WATER: u32 = 1 << 6;
const S_CLIP: u32 = 1 << 7;
const S_HINT: u32 = 1 << 8;
const S_AREAPORTAL: u32 = 1 << 9;
const S_OCCLUDER: u32 = 1 << 10;
const S_INVISIBLE: u32 = 1 << 11;
const S_LADDER: u32 = 1 << 12;
const S_OTHER_TOOL: u32 = 1 << 13;
const F_TOOL: u32 = 1 << 31;

const E_ANY: u32 = 1;
const E_POINT: u32 = 1 << 1;
const E_BRUSH: u32 = 1 << 2;
const E_HIDDEN: u32 = 1 << 3;
const E_TRIGGER: u32 = 1 << 4;
const E_FUNC_DETAIL: u32 = 1 << 5;
const E_PROP: u32 = 1 << 6;
const E_LIGHT: u32 = 1 << 7;
const E_NPC: u32 = 1 << 8;
const E_SPRITE: u32 = 1 << 9;
const E_CUBEMAP: u32 = 1 << 10;
const E_OVERLAY: u32 = 1 << 11;
const E_INSTANCE: u32 = 1 << 12;

impl AutoKind {
    pub const ALL: [AutoKind; 27] = [
        AutoKind::WorldBrushes,
        AutoKind::Sky,
        AutoKind::Displacements,
        AutoKind::Water,
        AutoKind::NodrawBrushes,
        AutoKind::WorldDetails,
        AutoKind::FuncDetail,
        AutoKind::Props,
        AutoKind::Entities,
        AutoKind::PointEntities,
        AutoKind::BrushEntities,
        AutoKind::Triggers,
        AutoKind::Lights,
        AutoKind::Npcs,
        AutoKind::Sprites,
        AutoKind::Cubemaps,
        AutoKind::Overlays,
        AutoKind::Instances,
        AutoKind::ToolBrushes,
        AutoKind::Clips,
        AutoKind::HintSkip,
        AutoKind::Areaportals,
        AutoKind::Occluders,
        AutoKind::Invisible,
        AutoKind::Ladders,
        AutoKind::OtherTools,
        AutoKind::Hidden,
    ];

    pub const TREE: [(AutoKind, &'static [AutoKind]); 5] = [
        (AutoKind::WorldBrushes, &[AutoKind::Sky, AutoKind::Displacements, AutoKind::Water, AutoKind::NodrawBrushes]),
        (AutoKind::WorldDetails, &[AutoKind::FuncDetail, AutoKind::Props]),
        (
            AutoKind::Entities,
            &[
                AutoKind::PointEntities,
                AutoKind::BrushEntities,
                AutoKind::Triggers,
                AutoKind::Lights,
                AutoKind::Npcs,
                AutoKind::Sprites,
                AutoKind::Cubemaps,
                AutoKind::Overlays,
                AutoKind::Instances,
            ],
        ),
        (
            AutoKind::ToolBrushes,
            &[
                AutoKind::Clips,
                AutoKind::HintSkip,
                AutoKind::Areaportals,
                AutoKind::Occluders,
                AutoKind::Invisible,
                AutoKind::Ladders,
                AutoKind::OtherTools,
            ],
        ),
        (AutoKind::Hidden, &[]),
    ];

    pub fn label_key(self) -> &'static str {
        match self {
            AutoKind::WorldBrushes => "autovis.world_brushes",
            AutoKind::Sky => "autovis.sky",
            AutoKind::Displacements => "autovis.displacements",
            AutoKind::Water => "autovis.water",
            AutoKind::NodrawBrushes => "autovis.nodraw_brushes",
            AutoKind::WorldDetails => "autovis.world_details",
            AutoKind::FuncDetail => "autovis.func_detail",
            AutoKind::Props => "autovis.props",
            AutoKind::Entities => "autovis.entities",
            AutoKind::PointEntities => "autovis.point_entities",
            AutoKind::BrushEntities => "autovis.brush_entities",
            AutoKind::Triggers => "autovis.triggers",
            AutoKind::Lights => "autovis.lights",
            AutoKind::Npcs => "autovis.npcs",
            AutoKind::Sprites => "autovis.sprites",
            AutoKind::Cubemaps => "autovis.cubemaps",
            AutoKind::Overlays => "autovis.overlays",
            AutoKind::Instances => "autovis.instances",
            AutoKind::ToolBrushes => "autovis.tool_brushes",
            AutoKind::Clips => "autovis.clips",
            AutoKind::HintSkip => "autovis.hint_skip",
            AutoKind::Areaportals => "autovis.areaportals",
            AutoKind::Occluders => "autovis.occluders",
            AutoKind::Invisible => "autovis.invisible",
            AutoKind::Ladders => "autovis.ladders",
            AutoKind::OtherTools => "autovis.other_tools",
            AutoKind::Hidden => "autovis.hidden",
        }
    }

    pub fn tooltip_key(self) -> &'static str {
        match self {
            AutoKind::WorldBrushes => "autovis.world_brushes_tip",
            AutoKind::Sky => "autovis.sky_tip",
            AutoKind::Displacements => "autovis.displacements_tip",
            AutoKind::Water => "autovis.water_tip",
            AutoKind::NodrawBrushes => "autovis.nodraw_brushes_tip",
            AutoKind::WorldDetails => "autovis.world_details_tip",
            AutoKind::FuncDetail => "autovis.func_detail_tip",
            AutoKind::Props => "autovis.props_tip",
            AutoKind::Entities => "autovis.entities_tip",
            AutoKind::PointEntities => "autovis.point_entities_tip",
            AutoKind::BrushEntities => "autovis.brush_entities_tip",
            AutoKind::Triggers => "autovis.triggers_tip",
            AutoKind::Lights => "autovis.lights_tip",
            AutoKind::Npcs => "autovis.npcs_tip",
            AutoKind::Sprites => "autovis.sprites_tip",
            AutoKind::Cubemaps => "autovis.cubemaps_tip",
            AutoKind::Overlays => "autovis.overlays_tip",
            AutoKind::Instances => "autovis.instances_tip",
            AutoKind::ToolBrushes => "autovis.tool_brushes_tip",
            AutoKind::Clips => "autovis.clips_tip",
            AutoKind::HintSkip => "autovis.hint_skip_tip",
            AutoKind::Areaportals => "autovis.areaportals_tip",
            AutoKind::Occluders => "autovis.occluders_tip",
            AutoKind::Invisible => "autovis.invisible_tip",
            AutoKind::Ladders => "autovis.ladders_tip",
            AutoKind::OtherTools => "autovis.other_tools_tip",
            AutoKind::Hidden => "autovis.hidden_tip",
        }
    }

    const fn solid_mask(self) -> u32 {
        match self {
            AutoKind::WorldBrushes => S_WORLD,
            AutoKind::Sky => S_SKY,
            AutoKind::Displacements => S_DISP,
            AutoKind::Water => S_WATER,
            AutoKind::NodrawBrushes => S_NODRAW,
            AutoKind::ToolBrushes => S_ALL_TOOL,
            AutoKind::Clips => S_ALL_TOOL | S_CLIP,
            AutoKind::HintSkip => S_ALL_TOOL | S_HINT,
            AutoKind::Areaportals => S_ALL_TOOL | S_AREAPORTAL,
            AutoKind::Occluders => S_ALL_TOOL | S_OCCLUDER,
            AutoKind::Invisible => S_ALL_TOOL | S_INVISIBLE,
            AutoKind::Ladders => S_ALL_TOOL | S_LADDER,
            AutoKind::OtherTools => S_ALL_TOOL | S_OTHER_TOOL,
            AutoKind::Hidden => S_HIDDEN,
            _ => 0,
        }
    }

    const fn entity_mask(self) -> u32 {
        match self {
            AutoKind::WorldDetails => E_FUNC_DETAIL | E_PROP,
            AutoKind::FuncDetail => E_FUNC_DETAIL,
            AutoKind::Props => E_PROP,
            AutoKind::Entities => E_ANY,
            AutoKind::PointEntities => E_POINT,
            AutoKind::BrushEntities => E_BRUSH,
            AutoKind::Triggers => E_TRIGGER,
            AutoKind::Lights => E_LIGHT,
            AutoKind::Npcs => E_NPC,
            AutoKind::Sprites => E_SPRITE,
            AutoKind::Cubemaps => E_CUBEMAP,
            AutoKind::Overlays => E_OVERLAY,
            AutoKind::Instances => E_INSTANCE,
            AutoKind::Hidden => E_HIDDEN,
            _ => 0,
        }
    }

    fn matches_solid_class(self, class: u32) -> bool {
        let mask = self.solid_mask();
        mask != 0 && class & mask == mask
    }

    fn matches_entity_class(self, class: u32) -> bool {
        class & self.entity_mask() != 0
    }

    pub fn matches(self, scene: &Scene, obj: ObjRef) -> bool {
        match obj {
            ObjRef::Solid(id) => scene.solid(id).is_some_and(|n| self.matches_solid_class(solid_class(n))),
            ObjRef::Entity(id) => {
                scene.entity(id).is_some_and(|n| self.matches_entity_class(entity_class(n, scene.is_brush_entity(id))))
            }
        }
    }
}

fn file_name(name: &str) -> &str {
    let n = name.trim();
    let n = n.rsplit(['/', '\\']).next().unwrap_or(n);
    strip_suffix_ci(n, ".vmt").unwrap_or(n)
}

fn material_bits(name: &str) -> u32 {
    if !is_tool_material(name) {
        return if contains_ci(name, "water") { S_WATER } else { 0 };
    }
    let rest = file_name(name);
    let is = |n: &str| rest.eq_ignore_ascii_case(n);
    let starts = |p: &str| strip_prefix_ci(rest, p).is_some();
    let class = if is_nodraw_material(name) || starts("toolsnodraw_") {
        S_NODRAW
    } else if is("toolsskybox") || is("toolsskybox2d") {
        S_SKY
    } else if starts("toolsclip")
        || starts("toolsplayerclip")
        || starts("toolsnpcclip")
        || starts("toolsgrenadeclip")
        || starts("toolsdroneclip")
    {
        S_CLIP
    } else if is("toolshint") || is("toolsskip") {
        S_HINT
    } else if starts("toolsareaportal") {
        S_AREAPORTAL
    } else if starts("toolsoccluder") {
        S_OCCLUDER
    } else if is("toolsinvisibleladder") || is("toolsladder") {
        S_LADDER
    } else if starts("toolsinvisible") || starts("toolsblockbullets") || is("toolsblocklight") || is("toolsblock_los") {
        S_INVISIBLE
    } else if starts("toolstrigger") {
        0
    } else {
        S_OTHER_TOOL
    };
    class | F_TOOL
}

fn strip_suffix_ci<'a>(s: &'a str, suffix: &str) -> Option<&'a str> {
    let cut = s.len().checked_sub(suffix.len())?;
    let tail = s.get(cut..)?;
    tail.eq_ignore_ascii_case(suffix).then(|| &s[..cut])
}

fn solid_class(n: &SolidNode) -> u32 {
    let mut class = 0;
    if n.owner == Owner::World {
        class |= S_WORLD;
    }
    if n.has_disp() {
        class |= S_DISP;
    }
    if n.solid.hidden {
        class |= S_HIDDEN;
    }
    let mut all_tool = !n.solid.sides.is_empty();
    let mut prev: Option<(&str, u32)> = None;
    for side in &n.solid.sides {
        let m = side.material.as_str();
        let bits = match prev {
            Some((p, b)) if p == m => b,
            _ => {
                let b = material_bits(m);
                prev = Some((m, b));
                b
            }
        };
        all_tool &= bits & F_TOOL != 0;
        class |= bits & !F_TOOL;
    }
    if all_tool {
        class |= S_ALL_TOOL;
    }
    class
}

fn entity_class(n: &EntityNode, is_brush: bool) -> u32 {
    let class = n.entity.classname.trim();
    let prefix = |p: &str| strip_prefix_ci(class, p).is_some();
    let is = |c: &str| class.eq_ignore_ascii_case(c);
    let mut bits = E_ANY | if is_brush { E_BRUSH } else { E_POINT };
    if n.entity.hidden {
        bits |= E_HIDDEN;
    }
    if prefix("trigger_") {
        bits |= E_TRIGGER;
    }
    if is("func_detail") {
        bits |= E_FUNC_DETAIL;
    }
    if prefix("prop_") {
        bits |= E_PROP;
    }
    if is_light_class(class) {
        bits |= E_LIGHT;
    }
    if prefix("npc_") || prefix("monster_") {
        bits |= E_NPC;
    }
    if prefix("env_sprite") || is("env_glow") || is("env_lightglow") {
        bits |= E_SPRITE;
    }
    if is("env_cubemap") {
        bits |= E_CUBEMAP;
    }
    if prefix("info_overlay") || is("infodecal") {
        bits |= E_OVERLAY;
    }
    if is("func_instance") {
        bits |= E_INSTANCE;
    }
    bits
}

pub(crate) fn is_light_class(class: &str) -> bool {
    strip_prefix_ci(class, "light").is_some()
        || class.eq_ignore_ascii_case("point_spotlight")
        || class.eq_ignore_ascii_case("env_projectedtexture")
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GroupCount {
    pub total: usize,
    pub shown: usize,
}

impl GroupCount {
    fn add(&mut self, shown: bool) {
        self.total += 1;
        if shown {
            self.shown += 1;
        }
    }

    pub fn state(&self) -> Option<bool> {
        if self.shown == self.total {
            Some(true)
        } else if self.shown == 0 {
            Some(false)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct FgdSubGroup {
    pub name: String,
    pub classes: Vec<String>,
    pub count: GroupCount,
}

#[derive(Clone, Debug, Default)]
pub struct FgdGroup {
    pub name: String,
    pub subs: Vec<FgdSubGroup>,
}

impl FgdGroup {
    pub fn count(&self) -> GroupCount {
        let mut c = GroupCount::default();
        for s in &self.subs {
            c.total += s.count.total;
            c.shown += s.count.shown;
        }
        c
    }

    pub fn all_classes(&self) -> Vec<String> {
        let mut v: Vec<String> = self.subs.iter().flat_map(|s| s.classes.iter().cloned()).collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}

const PARALLEL_SOLIDS: usize = 8192;
const MAX_THREADS: usize = 8;

struct SolidPass {
    counts: Vec<GroupCount>,
    owners_shown: Vec<Id>,
}

fn solid_pass<'a>(scene: &Scene, solids: impl Iterator<Item = &'a SolidNode>) -> SolidPass {
    let masks = AutoKind::ALL.map(AutoKind::solid_mask);
    let mut counts = vec![GroupCount::default(); AutoKind::ALL.len()];
    let mut owners_shown = Vec::new();
    for n in solids {
        let owner_shown = match n.owner {
            Owner::World => true,
            Owner::Entity(e) => {
                if n.solid.editor.visgroup_auto_shown {
                    owners_shown.push(e);
                }
                scene.entity(e).is_none_or(|en| en.entity.editor.visgroup_auto_shown)
            }
        };
        let shown = n.solid.editor.visgroup_auto_shown && owner_shown;
        let class = solid_class(n);
        for ((kind, mask), c) in AutoKind::ALL.iter().zip(masks).zip(counts.iter_mut()) {
            if mask != 0 && class & mask == mask {
                c.add(shown && *kind != AutoKind::Hidden);
            }
        }
    }
    SolidPass { counts, owners_shown }
}

#[derive(Clone, Debug, Default)]
pub struct AutoVisData {
    pub builtin: Vec<GroupCount>,
    pub fgd: Vec<FgdGroup>,
    class_index: HashMap<String, Vec<(usize, usize)>>,
}

impl AutoVisData {
    pub fn new(fgd_groups: &[hp_fgd::AutoVisGroup]) -> Self {
        let mut fgd = Vec::with_capacity(fgd_groups.len());
        let mut class_index: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
        for (gi, g) in fgd_groups.iter().enumerate() {
            let mut subs = Vec::with_capacity(g.groups.len());
            for (si, s) in g.groups.iter().enumerate() {
                let mut classes: Vec<String> = s.classes.iter().map(|c| c.trim().to_ascii_lowercase()).collect();
                classes.retain(|c| !c.is_empty());
                classes.sort_unstable();
                classes.dedup();
                for c in &classes {
                    class_index.entry(c.clone()).or_default().push((gi, si));
                }
                subs.push(FgdSubGroup { name: s.name.clone(), classes, count: GroupCount::default() });
            }
            fgd.push(FgdGroup { name: g.name.clone(), subs });
        }
        Self { builtin: vec![GroupCount::default(); AutoKind::ALL.len()], fgd, class_index }
    }

    pub fn recount(&mut self, doc: &Document) {
        let scene = doc.scene();
        let mut builtin = vec![GroupCount::default(); AutoKind::ALL.len()];
        for g in &mut self.fgd {
            for s in &mut g.subs {
                s.count = GroupCount::default();
            }
        }
        let solids = scene.solids().as_slice();
        let threads = if solids.len() >= PARALLEL_SOLIDS {
            std::thread::available_parallelism().map_or(1, std::num::NonZero::get).clamp(1, MAX_THREADS)
        } else {
            1
        };
        let mut entities_with_shown_solid: HashSet<Id> = HashSet::new();
        let mut merge = |pass: SolidPass| {
            for (c, p) in builtin.iter_mut().zip(&pass.counts) {
                c.total += p.total;
                c.shown += p.shown;
            }
            entities_with_shown_solid.extend(pass.owners_shown);
        };
        if threads <= 1 {
            merge(solid_pass(scene, solids.values()));
        } else {
            let chunk = solids.len().div_ceil(threads);
            std::thread::scope(|s| {
                let handles: Vec<_> = (0..threads)
                    .filter_map(|t| solids.get_range(t * chunk..((t + 1) * chunk).min(solids.len())))
                    .map(|part| s.spawn(move || solid_pass(scene, part.values())))
                    .collect();
                for h in handles {
                    if let Ok(pass) = h.join() {
                        merge(pass);
                    }
                }
            });
        }
        let entity_masks = AutoKind::ALL.map(AutoKind::entity_mask);
        let mut lower = String::new();
        for n in scene.entities().values() {
            let is_brush = scene.is_brush_entity(n.entity.id);
            let shown =
                n.entity.editor.visgroup_auto_shown && (!is_brush || entities_with_shown_solid.contains(&n.entity.id));
            let class = entity_class(n, is_brush);
            for ((kind, mask), c) in AutoKind::ALL.iter().zip(entity_masks).zip(builtin.iter_mut()) {
                if class & mask != 0 {
                    c.add(shown && *kind != AutoKind::Hidden);
                }
            }
            if !self.class_index.is_empty() {
                lower.clear();
                lower.extend(n.entity.classname.trim().chars().map(|c| c.to_ascii_lowercase()));
                if let Some(slots) = self.class_index.get(lower.as_str()) {
                    for &(gi, si) in slots {
                        if let Some(sub) = self.fgd.get_mut(gi).and_then(|g| g.subs.get_mut(si)) {
                            sub.count.add(shown);
                        }
                    }
                }
            }
        }
        self.builtin = builtin;
    }

    pub fn builtin_count(&self, kind: AutoKind) -> GroupCount {
        AutoKind::ALL.iter().position(|k| *k == kind).and_then(|i| self.builtin.get(i).copied()).unwrap_or_default()
    }
}

fn linked_member(scene: &Scene, obj: ObjRef, matches: &dyn Fn(&Scene, ObjRef) -> bool) -> bool {
    match obj {
        ObjRef::Solid(id) => match scene.solid(id).map(|n| n.owner) {
            Some(Owner::Entity(e)) => matches(scene, ObjRef::Entity(e)),
            _ => false,
        },
        ObjRef::Entity(id) => scene.entity_solid_ids(id).into_iter().any(|s| matches(scene, ObjRef::Solid(s))),
    }
}

fn set_matching_visible(doc: &mut Document, matches: &dyn Fn(&Scene, ObjRef) -> bool, visible: bool) -> usize {
    if visible {
        doc.set_auto_visibility(&|scene, obj| matches(scene, obj) || linked_member(scene, obj, matches), true)
    } else {
        doc.set_auto_visibility(matches, false)
    }
}

pub fn set_builtin_visible(doc: &mut Document, kind: AutoKind, visible: bool) -> usize {
    if kind == AutoKind::Hidden {
        return if visible { doc.unhide_all() } else { 0 };
    }
    set_matching_visible(doc, &|scene, obj| kind.matches(scene, obj), visible)
}

pub fn set_classes_visible(doc: &mut Document, classes: &[String], visible: bool) -> usize {
    if classes.is_empty() {
        return 0;
    }
    let pred = |scene: &Scene, obj: ObjRef| match obj {
        ObjRef::Entity(id) => scene.entity(id).is_some_and(|n| {
            let class = n.entity.classname.trim();
            classes.iter().any(|c| c.eq_ignore_ascii_case(class))
        }),
        ObjRef::Solid(_) => false,
    };
    set_matching_visible(doc, &pred, visible)
}

pub fn show_all_auto(doc: &mut Document) -> usize {
    doc.set_auto_visibility(&|_, _| true, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fgd_groups() -> Vec<hp_fgd::AutoVisGroup> {
        vec![hp_fgd::AutoVisGroup {
            name: "Lighting".into(),
            groups: vec![
                hp_fgd::AutoVisSubGroup { name: "Lights".into(), classes: vec!["light".into(), "LIGHT_SPOT".into()] },
                hp_fgd::AutoVisSubGroup { name: "Spawns".into(), classes: vec!["info_player_start".into()] },
            ],
        }]
    }

    #[test]
    fn builtin_counts_on_test_map() {
        let state = hp_app::testing::test_state();
        let mut data = AutoVisData::new(&[]);
        data.recount(&state.doc);
        let c = |k| data.builtin_count(k).total;
        assert_eq!(c(AutoKind::WorldBrushes), 2);
        assert_eq!(c(AutoKind::Entities), 3);
        assert_eq!(c(AutoKind::PointEntities), 2);
        assert_eq!(c(AutoKind::BrushEntities), 1);
        assert_eq!(c(AutoKind::Triggers), 1);
        assert_eq!(c(AutoKind::ToolBrushes), 1);
        assert_eq!(c(AutoKind::Lights), 1);
        assert_eq!(c(AutoKind::Props), 0);
        assert_eq!(c(AutoKind::FuncDetail), 0);
        assert_eq!(c(AutoKind::Displacements), 0);
        assert_eq!(c(AutoKind::NodrawBrushes), 0);
        assert_eq!(c(AutoKind::Hidden), 0);
        for k in [AutoKind::Clips, AutoKind::HintSkip, AutoKind::OtherTools, AutoKind::Sky, AutoKind::Water] {
            assert_eq!(c(k), 0, "{k:?}");
        }
        assert_eq!(data.builtin_count(AutoKind::Triggers).state(), Some(true));
    }

    #[test]
    fn tree_lists_every_group_once() {
        let mut seen: Vec<AutoKind> = Vec::new();
        for (parent, children) in AutoKind::TREE {
            seen.push(parent);
            seen.extend(children.iter().copied());
        }
        assert_eq!(seen, AutoKind::ALL.to_vec(), "panel order follows the tree");
        for k in AutoKind::ALL {
            assert!(k.solid_mask() != 0 || k.entity_mask() != 0, "{k:?} holds nothing");
        }
    }

    #[test]
    fn materials_are_classified_by_name() {
        let cases: [(&str, u32); 22] = [
            ("TOOLS/TOOLSNODRAW", S_NODRAW),
            ("materials/tools/toolsnodraw.vmt", S_NODRAW),
            ("tools\\toolsskybox2d", S_SKY),
            ("tools/toolsclip", S_CLIP),
            ("tools/toolsplayerclip", S_CLIP),
            ("tools/toolsnpcclip", S_CLIP),
            ("tools/toolsclip_wood", S_CLIP),
            ("tools/toolsgrenadeclip", S_CLIP),
            ("tools/toolshint", S_HINT),
            ("tools/toolsskip", S_HINT),
            ("tools/toolsareaportal", S_AREAPORTAL),
            ("tools/toolsoccluder", S_OCCLUDER),
            ("tools/toolsinvisibleladder", S_LADDER),
            ("tools/toolsladder", S_LADDER),
            ("tools/toolsinvisible", S_INVISIBLE),
            ("tools/toolsblockbullets", S_INVISIBLE),
            ("tools/toolsblocklight", S_INVISIBLE),
            ("tools/toolsblock_los", S_INVISIBLE),
            ("tools/toolstrigger", 0),
            ("tools/toolsblack", S_OTHER_TOOL),
            ("tools/toolsorigin", S_OTHER_TOOL),
            ("tools/mycustomtool", S_OTHER_TOOL),
        ];
        for (name, bits) in cases {
            assert_eq!(material_bits(name), bits | F_TOOL, "{name}");
        }
        assert_eq!(material_bits("nature/water_canals_water2"), S_WATER);
        assert_eq!(material_bits("DEV/DEV_WATER2"), S_WATER);
        assert_eq!(material_bits("concrete/concretefloor001a"), 0);
        assert_eq!(material_bits(""), 0);
        assert_eq!(material_bits("toolsnodraw"), 0, "no tools/ folder");
    }

    fn rich_doc() -> Document {
        use std::fmt::Write as _;
        let mut s = String::from("world { \"id\" \"1\" \"classname\" \"worldspawn\"\n");
        let mats = [
            "tools/toolsclip",
            "tools/toolshint",
            "tools/toolsareaportal",
            "tools/toolsoccluder",
            "tools/toolsinvisible",
            "tools/toolsinvisibleladder",
            "tools/toolsblack",
            "tools/toolsskybox",
            "nature/water_river",
            "tools/toolsnodraw",
        ];
        for (i, m) in mats.iter().enumerate() {
            let x0 = i as f64 * 64.0;
            let x1 = x0 + 32.0;
            let _ = writeln!(s, "solid {{ \"id\" \"{}\"", 10 + i);
            let planes = [
                format!("({x0} 32 32) ({x1} 32 32) ({x1} 0 32)"),
                format!("({x0} 0 0) ({x1} 0 0) ({x1} 32 0)"),
                format!("({x0} 32 32) ({x0} 0 32) ({x0} 0 0)"),
                format!("({x1} 32 0) ({x1} 0 0) ({x1} 0 32)"),
                format!("({x1} 32 32) ({x0} 32 32) ({x0} 32 0)"),
                format!("({x1} 0 0) ({x0} 0 0) ({x0} 0 32)"),
            ];
            for (k, p) in planes.iter().enumerate() {
                let mat = if i == 8 && k > 0 { "tools/toolsnodraw" } else { m };
                let _ = writeln!(
                    s,
                    "side {{ \"id\" \"{}\" \"plane\" \"{p}\" \"material\" \"{mat}\" \"uaxis\" \"[1 0 0 0] 0.25\" \"vaxis\" \"[0 -1 0 0] 0.25\" }}",
                    100 + i * 6 + k
                );
            }
            s.push_str("editor { \"visgroupshown\" \"1\" \"visgroupautoshown\" \"1\" } }\n");
        }
        s.push_str("}\n");
        let classes = [
            "npc_citizen",
            "monster_generic",
            "env_sprite",
            "env_glow",
            "env_cubemap",
            "info_overlay",
            "infodecal",
            "func_instance",
            "prop_static",
        ];
        for (i, c) in classes.iter().enumerate() {
            let _ =
                writeln!(s, "entity {{ \"id\" \"{}\" \"classname\" \"{c}\" \"origin\" \"0 0 {}\" }}", 500 + i, i * 8);
        }
        Document::from_map(hp_vmf::parse(&s).expect("parse"))
    }

    #[test]
    fn hierarchy_groups_count_their_members() {
        let mut doc = rich_doc();
        let mut data = AutoVisData::new(&[]);
        data.recount(&doc);
        let c = |data: &AutoVisData, k| data.builtin_count(k).total;
        for k in [
            AutoKind::Clips,
            AutoKind::HintSkip,
            AutoKind::Areaportals,
            AutoKind::Occluders,
            AutoKind::Invisible,
            AutoKind::Ladders,
            AutoKind::OtherTools,
            AutoKind::Sky,
            AutoKind::Water,
            AutoKind::Sprites,
            AutoKind::Cubemaps,
            AutoKind::Instances,
            AutoKind::Props,
        ] {
            assert_eq!(c(&data, k), if matches!(k, AutoKind::Sprites) { 2 } else { 1 }, "{k:?}");
        }
        assert_eq!(c(&data, AutoKind::Npcs), 2);
        assert_eq!(c(&data, AutoKind::Overlays), 2);
        assert_eq!(c(&data, AutoKind::WorldDetails), 1);
        assert_eq!(c(&data, AutoKind::ToolBrushes), 9);
        assert_eq!(c(&data, AutoKind::NodrawBrushes), 2);
        assert_eq!(set_builtin_visible(&mut doc, AutoKind::Clips, false), 1);
        assert!(!doc.is_solid_visible(10) && doc.is_solid_visible(11));
        data.recount(&doc);
        assert_eq!(data.builtin_count(AutoKind::ToolBrushes).state(), None);
        assert_eq!(set_builtin_visible(&mut doc, AutoKind::ToolBrushes, true), 1);
        assert!(doc.is_solid_visible(10));
        assert_eq!(set_builtin_visible(&mut doc, AutoKind::Npcs, false), 2);
        assert!(!doc.is_entity_visible(500) && doc.is_entity_visible(502));
    }

    #[test]
    fn toggling_builtin_groups() {
        let mut state = hp_app::testing::test_state();
        let mut data = AutoVisData::new(&[]);
        assert_eq!(set_builtin_visible(&mut state.doc, AutoKind::Triggers, false), 1);
        assert!(!state.doc.is_visible(ObjRef::Entity(6)));
        assert!(!state.doc.is_solid_visible(7));
        data.recount(&state.doc);
        assert_eq!(data.builtin_count(AutoKind::Triggers).state(), Some(false));
        assert_eq!(data.builtin_count(AutoKind::Entities).state(), None);
        assert_eq!(set_builtin_visible(&mut state.doc, AutoKind::Entities, true), 1);
        assert!(state.doc.is_visible(ObjRef::Entity(6)));
        assert!(state.doc.undo());
        assert!(!state.doc.is_visible(ObjRef::Entity(6)));
        assert_eq!(show_all_auto(&mut state.doc), 1);
        assert!(state.doc.is_visible(ObjRef::Entity(6)));
    }

    #[test]
    fn linked_groups_follow_real_visibility() {
        let mut state = hp_app::testing::test_state();
        let mut data = AutoVisData::new(&[]);
        assert_eq!(set_builtin_visible(&mut state.doc, AutoKind::Triggers, false), 1);
        data.recount(&state.doc);
        assert!(!state.doc.is_solid_visible(7));
        assert_eq!(data.builtin_count(AutoKind::ToolBrushes).state(), Some(false));
        assert_eq!(data.builtin_count(AutoKind::BrushEntities).state(), Some(false));
        assert!(set_builtin_visible(&mut state.doc, AutoKind::ToolBrushes, true) > 0);
        assert!(state.doc.is_solid_visible(7));
        data.recount(&state.doc);
        assert_eq!(data.builtin_count(AutoKind::ToolBrushes).state(), Some(true));
        assert_eq!(data.builtin_count(AutoKind::Triggers).state(), Some(true));
        set_builtin_visible(&mut state.doc, AutoKind::ToolBrushes, false);
        data.recount(&state.doc);
        assert_eq!(data.builtin_count(AutoKind::Triggers).state(), Some(false));
        assert!(set_builtin_visible(&mut state.doc, AutoKind::Triggers, true) > 0);
        assert!(state.doc.is_solid_visible(7));
        data.recount(&state.doc);
        assert_eq!(data.builtin_count(AutoKind::ToolBrushes).state(), Some(true));
        set_builtin_visible(&mut state.doc, AutoKind::Triggers, false);
        assert!(state.doc.is_solid_visible(2) && state.doc.is_solid_visible(3));
    }

    #[test]
    fn hidden_group_unhides() {
        let mut state = hp_app::testing::test_state();
        state.doc.set_hidden(&[ObjRef::Solid(2), ObjRef::Entity(5)], true);
        let mut data = AutoVisData::new(&[]);
        data.recount(&state.doc);
        let hidden = data.builtin_count(AutoKind::Hidden);
        assert_eq!(hidden.total, 2);
        assert_eq!(hidden.state(), Some(false));
        assert_eq!(set_builtin_visible(&mut state.doc, AutoKind::Hidden, false), 0);
        assert_eq!(set_builtin_visible(&mut state.doc, AutoKind::Hidden, true), 2);
        assert!(state.doc.is_visible(ObjRef::Solid(2)));
    }

    #[test]
    fn fgd_groups_count_and_toggle() {
        let mut state = hp_app::testing::test_state();
        let mut data = AutoVisData::new(&fgd_groups());
        data.recount(&state.doc);
        assert_eq!(data.fgd.len(), 1);
        assert_eq!(data.fgd[0].subs[0].classes, vec!["light", "light_spot"]);
        assert_eq!(data.fgd[0].subs[0].count.total, 1);
        assert_eq!(data.fgd[0].subs[1].count.total, 1);
        assert_eq!(data.fgd[0].count().total, 2);
        let classes = data.fgd[0].all_classes();
        assert_eq!(set_classes_visible(&mut state.doc, &classes, false), 2);
        assert!(!state.doc.is_visible(ObjRef::Entity(4)));
        assert!(!state.doc.is_visible(ObjRef::Entity(5)));
        assert!(state.doc.is_visible(ObjRef::Entity(6)));
        data.recount(&state.doc);
        assert_eq!(data.fgd[0].count().state(), Some(false));
        assert_eq!(set_classes_visible(&mut state.doc, &[], true), 0);
    }

    #[test]
    fn parallel_recount_matches_one_pass() {
        use std::fmt::Write as _;
        let mut s = String::from(
            "world { \"id\" \"1\" \"classname\" \"worldspawn\"
",
        );
        let cube = |s: &mut String, id: usize, x0: f64, mat: &str| {
            let x1 = x0 + 8.0;
            let _ = writeln!(s, "solid {{ \"id\" \"{id}\"");
            for (k, p) in [
                format!("({x0} 8 8) ({x1} 8 8) ({x1} 0 8)"),
                format!("({x0} 0 0) ({x1} 0 0) ({x1} 8 0)"),
                format!("({x0} 8 8) ({x0} 0 8) ({x0} 0 0)"),
                format!("({x1} 8 0) ({x1} 0 0) ({x1} 0 8)"),
                format!("({x1} 8 8) ({x0} 8 8) ({x0} 8 0)"),
                format!("({x1} 0 0) ({x0} 0 0) ({x0} 0 8)"),
            ]
            .iter()
            .enumerate()
            {
                let _ = writeln!(
                    s,
                    "side {{ \"id\" \"{}\" \"plane\" \"{p}\" \"material\" \"{mat}\" \"uaxis\" \"[1 0 0 0] 0.25\" \"vaxis\" \"[0 -1 0 0] 0.25\" }}",
                    id * 6 + k
                );
            }
            let shown = if id.is_multiple_of(7) { 0 } else { 1 };
            let _ = writeln!(s, "editor {{ \"visgroupshown\" \"1\" \"visgroupautoshown\" \"{shown}\" }} }}");
        };
        let mats =
            ["dev/dev_measuregeneric01", "tools/toolsclip", "tools/toolsnodraw", "nature/water_x", "tools/toolshint"];
        for i in 0..9000 {
            cube(&mut s, 10 + i, i as f64 * 16.0, mats[i % mats.len()]);
        }
        s.push_str(
            "}
",
        );
        for e in 0..40 {
            let _ = writeln!(s, "entity {{ \"id\" \"{}\" \"classname\" \"func_detail\"", 100_000 + e);
            cube(&mut s, 200_000 + e, -1000.0 - e as f64 * 16.0, "tools/toolstrigger");
            s.push_str(
                "}
",
            );
        }
        let doc = Document::from_map(hp_vmf::parse(&s).expect("parse"));
        assert!(doc.scene().solids().len() >= PARALLEL_SOLIDS);
        let mut data = AutoVisData::new(&[]);
        data.recount(&doc);
        let one = solid_pass(doc.scene(), doc.scene().solids().values());
        for (k, (c, o)) in AutoKind::ALL.iter().zip(data.builtin.iter().zip(&one.counts)) {
            if k.solid_mask() != 0 {
                assert_eq!(c, o, "{k:?}");
            }
        }
        assert_eq!(data.builtin_count(AutoKind::Clips).total, 1800);
        assert_eq!(data.builtin_count(AutoKind::Water).total, 1800);
        assert_eq!(data.builtin_count(AutoKind::FuncDetail).total, 40);
        assert!(data.builtin_count(AutoKind::WorldBrushes).shown < 9000);
    }

    #[test]
    fn light_classes() {
        assert!(is_light_class("light_spot"));
        assert!(is_light_class("Light"));
        assert!(is_light_class("env_projectedtexture"));
        assert!(!is_light_class("prop_physics"));
    }
}
