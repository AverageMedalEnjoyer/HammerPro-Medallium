use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

#[derive(Debug)]
pub enum Poll<T> {
    Pending,
    Ready(T),
    Failed,
}

#[derive(Debug)]
pub struct Job<T> {
    rx: Receiver<T>,
}

impl<T: Send + 'static> Job<T> {
    pub fn spawn(ctx: &egui::Context, name: &str, f: impl FnOnce() -> T + Send + 'static) -> Self {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name(format!("hp-shell-{name}")).spawn(move || {
            let value = f();
            let _ = tx.send(value);
            ctx.request_repaint();
        });
        if let Err(e) = spawned {
            log::error!("could not start background job {name}: {e}");
        }
        Self { rx }
    }

    pub fn ready(value: T) -> Self {
        let (tx, rx) = channel();
        let _ = tx.send(value);
        Self { rx }
    }

    pub fn poll(&self) -> Poll<T> {
        match self.rx.try_recv() {
            Ok(v) => Poll::Ready(v),
            Err(TryRecvError::Empty) => Poll::Pending,
            Err(TryRecvError::Disconnected) => Poll::Failed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickTarget {
    ExtraGameDir,
    FgdOverride,
    ExtraFgds,
    CustomTextureDirs,
    MapDir,
    ConfigFolder,
    ConfigFile,
    ConfigAddFile,
}

impl PickTarget {
    pub fn is_folder(self) -> bool {
        matches!(
            self,
            PickTarget::ExtraGameDir
                | PickTarget::CustomTextureDirs
                | PickTarget::MapDir
                | PickTarget::ConfigFolder
        )
    }

    pub fn is_multi(self) -> bool {
        matches!(self, PickTarget::ExtraFgds | PickTarget::CustomTextureDirs)
    }
}

#[derive(Clone, Debug)]
pub struct PickRequest {
    pub target: PickTarget,
    pub title: String,
    pub start_dir: Option<PathBuf>,
    pub filter: Option<(String, Vec<&'static str>)>,
}

fn run_dialog(req: PickRequest) -> Vec<PathBuf> {
    let mut dialog = rfd::FileDialog::new().set_title(req.title);
    if let Some(dir) = req.start_dir.as_deref().filter(|d| d.is_dir()) {
        dialog = dialog.set_directory(dir);
    }
    if let Some((name, exts)) = &req.filter {
        dialog = dialog.add_filter(name.clone(), exts);
    }
    match (req.target.is_folder(), req.target.is_multi()) {
        (true, true) => dialog.pick_folders().unwrap_or_default(),
        (true, false) => dialog.pick_folder().into_iter().collect(),
        (false, true) => dialog.pick_files().unwrap_or_default(),
        (false, false) => dialog.pick_file().into_iter().collect(),
    }
}

#[derive(Debug)]
pub struct Picker {
    pub target: PickTarget,
    job: Job<Vec<PathBuf>>,
}

impl Picker {
    pub fn open(ctx: &egui::Context, req: PickRequest) -> Self {
        let target = req.target;
        let job = if cfg!(target_os = "macos") {
            Job::ready(run_dialog(req))
        } else {
            Job::spawn(ctx, "file-dialog", move || run_dialog(req))
        };
        Self { target, job }
    }

    pub fn poll(&self) -> Option<Vec<PathBuf>> {
        match self.job.poll() {
            Poll::Pending => None,
            Poll::Ready(v) => Some(v),
            Poll::Failed => Some(Vec::new()),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameScan {
    pub root: PathBuf,
    pub gameinfo: bool,
    pub fgd: Option<PathBuf>,
    pub vpk_count: usize,
    pub addon_count: usize,
}

impl GameScan {
    pub fn is_valid(&self) -> bool {
        self.gameinfo
    }
}

fn count_vpks(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .filter_map(Result::ok)
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            let Some(stem) = name.strip_suffix(".vpk") else { return false };
            if stem.ends_with("_dir") {
                return true;
            }
            let chunk =
                stem.rsplit_once('_').is_some_and(|(_, n)| n.len() == 3 && n.bytes().all(|b| b.is_ascii_digit()));
            !chunk
        })
        .count()
}

pub fn scan_game_folder(path: &Path) -> GameScan {
    let root = if path.join("gameinfo.txt").is_file() && path.parent().is_some_and(|p| p.join("bin").is_dir()) {
        path.parent().map(Path::to_path_buf).unwrap_or_else(|| path.to_path_buf())
    } else {
        path.to_path_buf()
    };
    let game = root.join("garrysmod");
    let bin = root.join("bin");
    let fgd = Some(bin.join("garrysmod.fgd")).filter(|p| p.is_file());
    let vpk_count = count_vpks(&game) + count_vpks(&root.join("sourceengine"));
    let addon_count = std::fs::read_dir(game.join("addons"))
        .map(|it| {
            it.filter_map(Result::ok)
                .filter(|e| {
                    let name = e.file_name().to_string_lossy().to_ascii_lowercase();
                    name.ends_with(".gma") || e.file_type().is_ok_and(|t| t.is_dir())
                })
                .count()
        })
        .unwrap_or(0);
    GameScan { gameinfo: game.join("gameinfo.txt").is_file(), root, fgd, vpk_count, addon_count }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-ui-shell/scratch").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn scan_fake_install() {
        let root = scratch("fake_gmod");
        let game = root.join("garrysmod");
        let _ = std::fs::create_dir_all(game.join("addons").join("my_addon"));
        let _ = std::fs::create_dir_all(root.join("bin").join("win64"));
        let _ = std::fs::create_dir_all(root.join("sourceengine"));
        for (p, data) in [
            (game.join("gameinfo.txt"), "x"),
            (game.join("garrysmod_dir.vpk"), "x"),
            (game.join("garrysmod_000.vpk"), "x"),
            (game.join("fallbacks_dir.vpk"), "x"),
            (game.join("addons").join("a.gma"), "x"),
            (root.join("sourceengine").join("hl2_textures_dir.vpk"), "x"),
            (root.join("bin").join("garrysmod.fgd"), "x"),
        ] {
            let _ = std::fs::write(p, data);
        }
        let scan = scan_game_folder(&root);
        assert!(scan.is_valid());
        assert_eq!(scan.vpk_count, 3);
        assert_eq!(scan.addon_count, 2);
        assert!(scan.fgd.is_some());
        let scan2 = scan_game_folder(&game);
        assert_eq!(scan2.root, root);
    }

    #[test]
    fn scan_missing_folder() {
        let scan = scan_game_folder(Path::new("Z:/definitely/not/here"));
        assert!(!scan.is_valid());
        assert_eq!(scan.vpk_count, 0);
    }

    #[test]
    fn scan_real_install_when_present() {
        let root = Path::new(r"D:\SteamLibrary\steamapps\common\GarrysMod");
        if !root.is_dir() {
            return;
        }
        let scan = scan_game_folder(root);
        assert!(scan.is_valid());
        assert!(scan.vpk_count > 0);
    }

    #[test]
    fn job_round_trip() {
        let ctx = egui::Context::default();
        let job = Job::spawn(&ctx, "test", || 41 + 1);
        let mut value = None;
        for _ in 0..500 {
            match job.poll() {
                Poll::Ready(v) => {
                    value = Some(v);
                    break;
                }
                Poll::Pending => std::thread::sleep(std::time::Duration::from_millis(2)),
                Poll::Failed => break,
            }
        }
        assert_eq!(value, Some(42));
        assert!(matches!(Job::ready(5).poll(), Poll::Ready(5)));
    }
}
