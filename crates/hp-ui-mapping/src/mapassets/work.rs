use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use hp_app::runtime::AssetCollectSummary;
use hp_app::{AppState, t, tf};
use hp_fs::GameFs;
use hp_fs::collect::{CollectOptions, CollectOutcome, ExistingFiles, UnusedReport};
use hp_fs::deps::{DepNode, DepRoot, DependencyGraph, DepsCache, ResolveOptions};

use crate::release::{KeyKinds, collect_roots, fmt_size, instance_roots, key_kinds, origin_name};

#[derive(Clone)]
pub struct ResolveInput {
    pub(crate) fs: GameFs,
    pub(crate) roots: Vec<DepRoot>,
    pub(crate) instances: Vec<(String, String)>,
    pub(crate) vmf: Option<PathBuf>,
    pub(crate) kinds: KeyKinds,
    pub(crate) map: String,
    pub(crate) cache: Arc<DepsCache>,
}

impl std::fmt::Debug for ResolveInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolveInput").field("roots", &self.roots.len()).field("map", &self.map).finish_non_exhaustive()
    }
}

pub(crate) fn map_name(state: &AppState) -> String {
    state
        .doc
        .path()
        .and_then(|p| p.file_stem())
        .map_or_else(|| "untitled".to_owned(), |s| s.to_string_lossy().into_owned())
}

impl ResolveInput {
    pub fn of(state: &AppState, cache: Arc<DepsCache>) -> Result<ResolveInput, String> {
        let fs = state.assets.fs().cloned().ok_or_else(|| t!("mapassets.no_content").to_owned())?;
        let kinds = key_kinds(state.assets.fgd());
        let (roots, instances) = collect_roots(state, &kinds);
        Ok(ResolveInput {
            fs,
            roots,
            instances,
            vmf: state.doc.path().map(Path::to_path_buf),
            kinds,
            map: map_name(state),
            cache,
        })
    }

    pub fn resolve(&self, cancel: &AtomicBool) -> Result<(DependencyGraph, Vec<String>), String> {
        let mut roots = self.roots.clone();
        let mut warnings = Vec::new();
        if let Some(vmf) = &self.vmf {
            let mut warn = |w: String| warnings.push(w);
            let nested = instance_roots(vmf, &self.instances, &self.kinds, cancel, &mut warn)
                .ok_or_else(|| t!("mapassets.cancelled").to_owned())?;
            roots.extend(nested);
        }
        let opts = ResolveOptions { map_name: &self.map, cancel: Some(cancel), editor_textures: true };
        let graph = self.fs.resolve_dependencies(&roots, &opts, &self.cache);
        if graph.cancelled {
            return Err(t!("mapassets.cancelled").to_owned());
        }
        Ok((graph, warnings))
    }
}

pub(crate) struct CollectInput {
    pub fs: GameFs,
    pub graph: Arc<DependencyGraph>,
    pub nodes: Vec<usize>,
    pub missing: Vec<usize>,
    pub output: PathBuf,
    pub existing: ExistingFiles,
    pub map: String,
}

fn report_path(output: &Path, map: &str, replace: bool) -> PathBuf {
    let first = output.join(format!("{map}_assets.txt"));
    if replace || !first.exists() {
        return first;
    }
    (2..1000).map(|n| output.join(format!("{map}_assets_{n}.txt"))).find(|p| !p.exists()).unwrap_or(first)
}

pub(crate) fn report_text(
    input: &CollectInput,
    run: &hp_fs::collect::CollectRun,
    summary: &AssetCollectSummary,
) -> String {
    let mut out = String::new();
    let mut line = |s: String| {
        out.push_str(&s);
        out.push_str("\r\n");
    };
    line(tf!("mapassets.report_title", input.map));
    line(tf!("mapassets.report_output", input.output.display()));
    line(tf!(
        "mapassets.report_counts",
        summary.copied,
        summary.replaced,
        summary.same,
        summary.skipped,
        summary.failed.len(),
        summary.missing.len()
    ));
    line(tf!("mapassets.report_bytes", fmt_size(summary.bytes)));
    if summary.cancelled {
        line(t!("mapassets.report_cancelled").to_owned());
    }
    line(String::new());
    line(t!("mapassets.report_files").to_owned());
    let by_path: std::collections::HashMap<&str, &DepNode> =
        input.nodes.iter().filter_map(|&i| input.graph.nodes.get(i)).map(|n| (n.vpath.as_str(), n)).collect();
    for f in &run.files {
        let what = match &f.outcome {
            CollectOutcome::Copied => t!("mapassets.outcome_copied").to_owned(),
            CollectOutcome::Replaced => t!("mapassets.outcome_replaced").to_owned(),
            CollectOutcome::Skipped => t!("mapassets.outcome_skipped").to_owned(),
            CollectOutcome::Same => t!("mapassets.outcome_same").to_owned(),
            CollectOutcome::Failed(e) => tf!("mapassets.outcome_failed", e),
        };
        let from = by_path.get(f.vpath.as_str()).map_or_else(String::new, |n| {
            if n.source_label.is_empty() {
                origin_name(n.origin).to_owned()
            } else {
                format!("{}, {}", origin_name(n.origin), n.source_label)
            }
        });
        line(format!("  {}  ({from})  {what}", f.vpath));
    }
    if !summary.missing.is_empty() {
        line(String::new());
        line(t!("mapassets.report_missing").to_owned());
        for &i in &input.missing {
            let Some(m) = input.graph.missing.get(i) else { continue };
            let why: Vec<String> = m.reasons.iter().take(3).map(crate::release::reason_text).collect();
            line(format!("  {}  ({})", m.name, why.join("; ")));
        }
    }
    out
}

pub(crate) fn collect(
    input: &CollectInput,
    cancel: &AtomicBool,
    progress: &(dyn Fn(usize, usize) + Sync),
) -> AssetCollectSummary {
    let mut summary = AssetCollectSummary { output: input.output.clone(), ..AssetCollectSummary::default() };
    if let Err(e) = std::fs::create_dir_all(&input.output) {
        summary.error = Some(tf!("mapassets.err_output", input.output.display(), e));
        return summary;
    }
    let nodes: Vec<&DepNode> = input.nodes.iter().filter_map(|&i| input.graph.nodes.get(i)).collect();
    let opts = CollectOptions { existing: input.existing, cancel: Some(cancel), progress: Some(progress) };
    let run = input.fs.collect_dependencies(&nodes, &input.output, &opts);
    summary.copied = run.count(&CollectOutcome::Copied);
    summary.replaced = run.count(&CollectOutcome::Replaced);
    summary.skipped = run.count(&CollectOutcome::Skipped);
    summary.same = run.count(&CollectOutcome::Same);
    summary.failed = run
        .files
        .iter()
        .filter_map(|f| match &f.outcome {
            CollectOutcome::Failed(e) => Some((f.vpath.clone(), e.clone())),
            _ => None,
        })
        .collect();
    summary.missing =
        input.missing.iter().filter_map(|&i| input.graph.missing.get(i)).map(|m| m.name.clone()).collect();
    summary.bytes = run.bytes_written();
    summary.cancelled = run.cancelled;
    let path = report_path(&input.output, &input.map, input.existing == ExistingFiles::Replace);
    let text = report_text(input, &run, &summary);
    match std::fs::write(&path, text) {
        Ok(()) => summary.report = Some(path),
        Err(e) => summary.error = Some(tf!("mapassets.err_report", path.display(), e)),
    }
    summary
}

pub(crate) fn unused(graph: &DependencyGraph, folder: &Path, cancel: &AtomicBool) -> Result<UnusedReport, String> {
    hp_fs::collect::unused_files(folder, graph, Some(cancel)).map_err(|e| match e {
        hp_fs::FsError::NoContentFolders(p) => tf!("mapassets.err_no_content_folders", p.display()),
        hp_fs::FsError::NotADirectory(p) => tf!("mapassets.err_not_folder", p.display()),
        other => other.to_string(),
    })
}

pub(crate) fn check_output(fs: &GameFs, output: &Path) -> Option<String> {
    if output.as_os_str().is_empty() {
        return Some(t!("mapassets.err_no_output").to_owned());
    }
    if !output.is_absolute() {
        return Some(tf!("mapassets.err_relative", output.display()));
    }
    let key = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let out = key(output);
    for s in fs.sources() {
        let loose = matches!(s.kind, hp_fs::SourceKind::Dir | hp_fs::SourceKind::Custom(_));
        if loose && key(&s.path) == out {
            return Some(tf!("mapassets.err_mounted", output.display(), s.label));
        }
    }
    None
}
