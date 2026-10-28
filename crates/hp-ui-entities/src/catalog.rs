use std::sync::{Arc, Mutex};

use hp_app::state::AssetsApi;

const MAX_FILES: usize = 200_000;

struct Slot {
    generation: u64,
    files: Mutex<Option<Arc<Vec<String>>>>,
}

fn files(
    ctx: &egui::Context,
    assets: &dyn AssetsApi,
    id: &'static str,
    build: fn(&hp_fs::GameFs) -> Vec<String>,
) -> Option<Arc<Vec<String>>> {
    let fs = assets.fs()?;
    let generation = assets.generation();
    let key = egui::Id::unique(("hp_ui_entities.catalog", id));
    let current = ctx.data(|d| d.get_temp::<Arc<Slot>>(key)).filter(|s| s.generation == generation);
    let slot = match current {
        Some(s) => s,
        None => {
            let slot = Arc::new(Slot { generation, files: Mutex::new(None) });
            ctx.data_mut(|d| d.insert_temp(key, Arc::clone(&slot)));
            let (fs, worker_slot, repaint) = (fs.clone(), Arc::clone(&slot), ctx.clone());
            let spawned = std::thread::Builder::new().name(format!("hp-list-{id}")).spawn(move || {
                let mut list = build(&fs);
                list.truncate(MAX_FILES);
                if let Ok(mut f) = worker_slot.files.lock() {
                    *f = Some(Arc::new(list));
                }
                repaint.request_repaint();
            });
            if let Err(e) = spawned {
                log::warn!("could not start the {id} list thread: {e}");
                if let Ok(mut f) = slot.files.lock() {
                    *f = Some(Arc::new(Vec::new()));
                }
            }
            slot
        }
    };
    let files = slot.files.lock().ok().and_then(|f| f.clone());
    if files.is_none() {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    files
}

pub(crate) fn scene_files(ctx: &egui::Context, assets: &dyn AssetsApi) -> Option<Arc<Vec<String>>> {
    files(ctx, assets, "scenes", |fs| fs.list("scenes", Some("vcd")).into_iter().map(|e| e.name).collect())
}

const VSCRIPTS: &str = "scripts/vscripts/";

pub(crate) fn script_files(ctx: &egui::Context, assets: &dyn AssetsApi) -> Option<Arc<Vec<String>>> {
    files(ctx, assets, "vscripts", |fs| {
        let mut out: Vec<String> = ["nut", "lua"]
            .iter()
            .flat_map(|ext| fs.list("scripts/vscripts", Some(ext)))
            .filter_map(|e| e.name.strip_prefix(VSCRIPTS).map(str::to_owned))
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    })
}

pub(crate) fn soundscape_names(ctx: &egui::Context, assets: &dyn AssetsApi) -> Option<Arc<Vec<String>>> {
    files(ctx, assets, "soundscapes", |fs| {
        let Some(manifest) = fs.read("scripts/soundscapes_manifest.txt") else { return Vec::new() };
        let Ok(root) = hp_kv::parse_bytes(&manifest) else { return Vec::new() };
        let mut out: Vec<String> = Vec::new();
        for block in root.children.iter().take(1) {
            for file in block.get_all("file").take(1024) {
                let path = file.trim().replace('\\', "/").to_ascii_lowercase();
                if let Some(bytes) = fs.read(&path) {
                    out.extend(crate::soundscapes::block_names(&bytes));
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    })
}

pub(crate) fn append_to_list(list: &str, item: &str) -> String {
    let item = item.trim();
    let mut words: Vec<&str> = list.split_whitespace().collect();
    if !item.is_empty() && !words.iter().any(|w| w.eq_ignore_ascii_case(item)) {
        words.push(item);
    }
    words.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_lists_append_once() {
        assert_eq!(append_to_list("", "a.nut"), "a.nut");
        assert_eq!(append_to_list("a.nut  b.nut", "c.nut"), "a.nut b.nut c.nut");
        assert_eq!(append_to_list("a.nut b.nut", "A.NUT"), "a.nut b.nut");
        assert_eq!(append_to_list("a.nut", "  "), "a.nut");
    }

    #[test]
    fn lists_need_mounted_content() {
        let state = hp_app::testing::test_state();
        let ctx = egui::Context::default();
        assert!(scene_files(&ctx, state.assets.as_ref()).is_none());
        assert!(script_files(&ctx, state.assets.as_ref()).is_none());
    }

    #[test]
    fn lists_load_in_the_background() {
        let dir = std::env::temp_dir().join(format!("hp-ui-entities-catalog-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(dir.join("scenes").join("npc"));
        let _ = std::fs::create_dir_all(dir.join("scripts").join("vscripts").join("lib"));
        let _ = std::fs::write(dir.join("scenes").join("npc").join("hello.vcd"), b"x");
        let _ = std::fs::write(dir.join("scripts").join("vscripts").join("lib").join("Util.nut"), b"x");
        let fs = hp_fs::GameFs::empty();
        assert!(fs.add_game_dir(&dir).is_ok());
        let built: Vec<String> = fs.list("scenes", Some("vcd")).into_iter().map(|e| e.name).collect();
        assert_eq!(built, ["scenes/npc/hello.vcd"]);
        let scripts: Vec<String> = fs
            .list("scripts/vscripts", Some("nut"))
            .into_iter()
            .filter_map(|e| e.name.strip_prefix(VSCRIPTS).map(str::to_owned))
            .collect();
        assert_eq!(scripts, ["lib/util.nut"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
