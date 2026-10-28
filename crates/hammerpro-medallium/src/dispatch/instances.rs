use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hp_app::i18n::TrTable;
use hp_app::runtime::instances::{
    self as rt, CONTENT_ROOT, InstanceCache, InstanceContext, InstanceScene, key_fixup, selected_instances,
};
use hp_app::settings::InstanceDisplay;
use hp_app::{Action, Severity, t, tf};
use hp_editor::Id;
use hp_editor::instances::{
    CollapseOptions, ExpandOptions, InstanceIssue, InstanceSpec, LoadRequest, Loaded, path_key,
};

use super::{DockTab, Package};
use crate::app::HammerProApp;

pub(crate) struct Instances;

#[derive(Default)]
pub(crate) struct State {
    pending_edit: Option<(Id, Instant)>,
    collapse_since: Option<Instant>,
    panel: hp_ui_entities::instance_ui::InstancePanel,
}

const WAIT_LIMIT: Duration = Duration::from_secs(30);
const TAB: &str = "instance";

const HANDLED: &[Action] = &[
    Action::CreateInstance,
    Action::EditInstance,
    Action::CollapseInstances,
    Action::CollapseAllInstances,
    Action::InstancesHidden,
    Action::InstancesTinted,
    Action::InstancesNormal,
];

fn issue_text(issue: &InstanceIssue) -> String {
    match issue {
        InstanceIssue::NoFile => t!("instances.issue_no_file").to_owned(),
        InstanceIssue::Missing { file, .. } => tf!("instances.issue_missing", file),
        InstanceIssue::Failed { file, message } => tf!("instances.issue_failed", file, message),
        InstanceIssue::Pending => t!("instances.issue_pending").to_owned(),
        InstanceIssue::Cycle { file } => tf!("instances.issue_cycle", file),
        InstanceIssue::TooDeep { file } => tf!("instances.issue_too_deep", file),
        InstanceIssue::TooLarge => t!("instances.issue_too_large").to_owned(),
    }
}

const CONTEXT_ARG: &str = "--instance-context=";

fn parse_context_arg(value: &str) -> Option<InstanceContext> {
    let (parent, id) = value.rsplit_once('|')?;
    let entity: Id = id.trim().parse().ok()?;
    let parent = PathBuf::from(parent.trim());
    (parent.is_absolute() && !parent.as_os_str().is_empty()).then_some(InstanceContext { parent, entity })
}

fn map_argument(args: &[String]) -> Option<&String> {
    const WITH_VALUE: &[&str] =
        &["--lang", "--screenshot", "--script", "--bench-open", "--frames", "--maximize-view", "--size", "--tool"];
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if WITH_VALUE.contains(&a.as_str()) {
            it.next();
        } else if !a.starts_with("--") {
            return Some(a);
        }
    }
    None
}

fn open_instance_file(app: &mut HammerProApp, path: &Path, entity: Id) {
    let context = app.state.doc.path().map(|parent| InstanceContext { parent: parent.to_path_buf(), entity });
    if app.state.settings.workflow.open_in_new_tab {
        if let Some(c) = context {
            app.state.runtime.instances.contexts.insert(path_key(path), c);
        }
        app.state.requested_open = Some(path.to_path_buf());
        return;
    }
    if hp_compile::sandbox::is_sandboxed() {
        log::info!("edit instance {}: {}", path.display(), hp_compile::sandbox::REFUSED);
        app.state.notify(Severity::Info, tf!("instances.edit_sandboxed", path.display()));
        return;
    }
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => {
            app.state.notify(Severity::Warning, tf!("instances.edit_failed", path.display(), e));
            return;
        }
    };
    let mut command = std::process::Command::new(exe);
    command.arg(path);
    if let Some(c) = context {
        command.arg(format!("{CONTEXT_ARG}{}|{}", c.parent.display(), c.entity));
    }
    match command.spawn() {
        Ok(_) => app.state.notify(Severity::Info, tf!("instances.edit_opened", path.display())),
        Err(e) => app.state.notify(Severity::Warning, tf!("instances.edit_failed", path.display(), e)),
    }
}

fn poll_edit(app: &mut HammerProApp, ctx: &egui::Context) {
    let Some((id, since)) = app.pkg.instances.pending_edit else { return };
    let Some(spec) = app.state.doc.scene().entity(id).and_then(|n| InstanceSpec::of(&n.entity)) else {
        app.pkg.instances.pending_edit = None;
        return;
    };
    if spec.file.is_empty() {
        app.pkg.instances.pending_edit = None;
        app.state.notify(Severity::Info, t!("instances.issue_no_file"));
        return;
    }
    let cache = InstanceCache::shared();
    let req = LoadRequest { file: &spec.file, containing: None, depth: 0 };
    let issue = match cache.load(&req, app.state.doc.path()) {
        Loaded::Ready { path, .. } => {
            app.pkg.instances.pending_edit = None;
            if path.starts_with(CONTENT_ROOT) {
                app.state.notify(Severity::Info, tf!("instances.edit_content", &spec.file));
            } else {
                let path: PathBuf = path.to_path_buf();
                open_instance_file(app, &path, id);
            }
            return;
        }
        Loaded::Pending if since.elapsed() < WAIT_LIMIT => {
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }
        Loaded::Pending => InstanceIssue::Pending,
        Loaded::Missing => InstanceIssue::Missing { file: spec.file.clone(), depth: 0 },
        Loaded::Failed(message) => InstanceIssue::Failed { file: spec.file.clone(), message },
    };
    app.pkg.instances.pending_edit = None;
    app.state.notify(Severity::Warning, issue_text(&issue));
}

fn poll_collapse(app: &mut HammerProApp, ctx: &egui::Context) {
    let Some(ids) = app.state.runtime.instances.pending_collapse.clone() else {
        app.pkg.instances.collapse_since = None;
        return;
    };
    let first = app.pkg.instances.collapse_since.is_none();
    let since = *app.pkg.instances.collapse_since.get_or_insert_with(Instant::now);
    let cache = InstanceCache::shared();
    let state = &mut app.state;
    let fgd = state.assets.fgd();
    let kinds = |c: &str, k: &str| fgd.and_then(|f| key_fixup(f, c, k));
    let skip_hidden = state.settings.instances.collapse_skips_hidden;
    let main = state.doc.path().map(Path::to_path_buf);
    let mut source = cache.source(main.as_deref());
    let opts = ExpandOptions { skip_hidden, kinds: Some(&kinds), main_map: main.as_deref(), first_auto_index: 1 };
    let loading =
        state.doc.expand_instances(&mut source, &opts).iter().any(|(id, e)| ids.contains(id) && e.is_pending());
    if loading {
        if since.elapsed() < WAIT_LIMIT {
            if first {
                state.toasts.push(Severity::Info, t!("instances.loading"));
            }
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }
        state.runtime.instances.pending_collapse = None;
        state.notify(Severity::Warning, t!("instances.load_timeout"));
        app.pkg.instances.collapse_since = None;
        return;
    }
    state.runtime.instances.pending_collapse = None;
    let result = state.doc.collapse_instances(&ids, &mut source, &CollapseOptions { kinds: Some(&kinds), skip_hidden });
    drop(source);
    app.pkg.instances.collapse_since = None;
    let Some(report) = app.report(Action::CollapseInstances, result) else { return };
    if !report.collapsed.is_empty() {
        app.state.notify(Severity::Info, tf!("instances.collapsed", report.collapsed.len(), report.objects.len()));
    }
    for (_, issue) in report.skipped.iter().chain(&report.warnings) {
        let text = issue_text(issue);
        app.state.notify(Severity::Warning, tf!("instances.collapse_issue", text));
    }
}

impl Package for Instances {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, ctx: &egui::Context, action: Action) -> bool {
        let display = match action {
            Action::CreateInstance => {
                if app.require_selection() {
                    app.state.runtime.instances.create_open = true;
                }
                return true;
            }
            Action::EditInstance => {
                match selected_instances(&app.state.doc).first() {
                    Some(&id) => {
                        app.pkg.instances.pending_edit = Some((id, Instant::now()));
                        poll_edit(app, ctx);
                    }
                    None => app.state.notify(Severity::Info, t!("instances.select_one")),
                }
                return true;
            }
            Action::CollapseInstances => {
                let ids = selected_instances(&app.state.doc);
                if ids.is_empty() {
                    app.state.notify(Severity::Info, t!("instances.select_one"));
                } else {
                    app.state.runtime.instances.pending_collapse = Some(ids);
                    app.pkg.instances.collapse_since = None;
                    poll_collapse(app, ctx);
                }
                return true;
            }
            Action::CollapseAllInstances => {
                app.state.runtime.instances.confirm_collapse_all = true;
                return true;
            }
            Action::InstancesHidden => InstanceDisplay::Hidden,
            Action::InstancesTinted => InstanceDisplay::Tinted,
            Action::InstancesNormal => InstanceDisplay::Normal,
            _ => return false,
        };
        if app.state.settings.instances.display != display {
            app.state.settings.instances.display = display;
            app.state.settings_dirty = true;
        }
        true
    }

    fn tick(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        let cache = InstanceCache::shared();
        cache.set_repaint(ctx);
        cache.set_content(app.state.assets.fs());
        cache.poll();
        rt::publish_scene(InstanceScene::from_settings(&app.state.settings));
        rt::publish_main_map(app.state.doc.path());
        let contexts = &app.state.runtime.instances.contexts;
        rt::publish_context(app.state.doc.path().and_then(|p| contexts.get(&path_key(p))));
        poll_edit(app, ctx);
        poll_collapse(app, ctx);
    }

    fn init(&self, app: &mut HammerProApp) {
        hp_app::i18n::register(hp_ui_entities::instance_ui::TR);
        let args: Vec<String> = std::env::args().skip(1).collect();
        let context = args.iter().find_map(|a| a.strip_prefix(CONTEXT_ARG)).and_then(parse_context_arg);
        if let (Some(c), Some(file)) = (context, map_argument(&args)) {
            let path = std::path::absolute(file).unwrap_or_else(|_| PathBuf::from(file));
            app.state.runtime.instances.contexts.insert(path_key(&path), c);
        }
    }

    fn dock_tabs(&self, _app: &HammerProApp, tabs: &mut Vec<DockTab>) {
        tabs.push(DockTab { id: TAB, label: t!("instances.tab").to_owned() });
    }

    fn dock_ui(&self, app: &mut HammerProApp, ui: &mut egui::Ui, id: &'static str) -> bool {
        if id != TAB {
            return false;
        }
        app.pkg.instances.panel.ui(ui, &mut app.state);
        true
    }

    fn translations(&self) -> TrTable {
        TR
    }
}

const TR: TrTable = &[
    ("instances.tab", "Instance", "Instance"),
    ("instances.select_one", "Select a func_instance first.", "Sélectionnez d'abord une func_instance."),
    ("instances.issue_no_file", "The instance has no file.", "L'instance n'a pas de fichier."),
    ("instances.issue_missing", "Instance file not found: {0}", "Fichier d'instance introuvable : {0}"),
    (
        "instances.issue_failed",
        "Cannot read the instance file {0}: {1}",
        "Impossible de lire le fichier d'instance {0} : {1}",
    ),
    (
        "instances.issue_pending",
        "The instance file is still loading.",
        "Le fichier de l'instance est encore en chargement.",
    ),
    ("instances.issue_cycle", "The instance {0} contains itself.", "L'instance {0} se contient elle-même."),
    ("instances.issue_too_deep", "Instances nested too deeply: {0}", "Instances imbriquées trop profondément : {0}"),
    ("instances.issue_too_large", "Too many nested instances.", "Trop d'instances imbriquées."),
    ("instances.edit_opened", "Opening {0} in a new window", "Ouverture de {0} dans une nouvelle fenêtre"),
    ("instances.edit_failed", "Cannot open {0}: {1}", "Impossible d'ouvrir {0} : {1}"),
    (
        "instances.edit_sandboxed",
        "Automated run: {0} is not opened in a new window.",
        "Exécution automatisée : {0} n'est pas ouvert dans une nouvelle fenêtre.",
    ),
    (
        "instances.edit_content",
        "{0} comes from the game content and cannot be edited.",
        "{0} vient du contenu du jeu et ne peut pas être modifié.",
    ),
    ("instances.loading", "Loading the instance files...", "Chargement des fichiers des instances..."),
    (
        "instances.load_timeout",
        "The instance files took too long to load: nothing was collapsed.",
        "Les fichiers des instances ont mis trop de temps à charger : rien n'a été fusionné.",
    ),
    ("instances.collapsed", "Instances collapsed: {0} ({1} objects)", "Instances fusionnées : {0} ({1} objets)"),
    ("instances.collapse_issue", "Collapse: {0}", "Fusion : {0}"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issues_have_texts() {
        for issue in [
            InstanceIssue::NoFile,
            InstanceIssue::Missing { file: "a.vmf".into(), depth: 0 },
            InstanceIssue::Failed { file: "a.vmf".into(), message: "x".into() },
            InstanceIssue::Pending,
            InstanceIssue::Cycle { file: "a.vmf".into() },
            InstanceIssue::TooDeep { file: "a.vmf".into() },
            InstanceIssue::TooLarge,
        ] {
            assert!(!issue_text(&issue).is_empty());
        }
        assert!(hp_app::i18n::lint_table(TR).is_empty());
    }

    #[test]
    fn context_arguments() {
        let abs = if cfg!(windows) { "C:\\maps\\main.vmf" } else { "/maps/main.vmf" };
        let c = parse_context_arg(&format!("{abs}|42")).expect("parsed");
        assert_eq!((c.parent, c.entity), (PathBuf::from(abs), 42));
        assert!(parse_context_arg("main.vmf|42").is_none(), "relative parents are refused");
        assert!(parse_context_arg(&format!("{abs}|x")).is_none());
        assert!(parse_context_arg(abs).is_none());
        let args: Vec<String> = ["--lang", "fr", "room.vmf", "--instance-context=x|1"].map(String::from).to_vec();
        assert_eq!(map_argument(&args).map(String::as_str), Some("room.vmf"));
        assert_eq!(map_argument(&args[..2]), None);
    }
}
