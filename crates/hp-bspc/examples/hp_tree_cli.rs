#[path = "../../hammerpro/src/bspc_cli.rs"]
#[allow(dead_code)]
mod bspc_cli;
#[path = "../../hammerpro/src/bspc_game.rs"]
#[allow(dead_code)]
mod bspc_game;

#[allow(dead_code)]
mod bspc_instances {
    use std::path::Path;

    pub(crate) struct InstanceFiles<'a> {
        pub(crate) vmf: &'a Path,
        pub(crate) game_dir: Option<&'a Path>,
        pub(crate) fs: Option<&'a hp_fs::GameFs>,
    }

    pub(crate) fn has_instances(map: &hp_vmf::Map) -> bool {
        map.entities.iter().any(|e| e.classname.eq_ignore_ascii_case("func_instance"))
    }

    pub(crate) fn merge_instances(
        _map: &mut hp_vmf::Map,
        _files: &InstanceFiles<'_>,
        _root: Option<&Path>,
        log: &mut dyn FnMut(&str),
    ) -> usize {
        log("Warning: func_instance is not merged by this tool (use hammerpro --bspc)");
        0
    }

    pub(crate) fn collapse(
        _map: &mut hp_vmf::Map,
        _files: &InstanceFiles<'_>,
        _fgd: Option<&hp_fgd::ResolvedFgd>,
        log: &mut dyn FnMut(&str),
    ) -> usize {
        log("Warning: func_instance is not merged by this tool (use hammerpro --bspc)");
        0
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(bspc_cli::run(&args));
}
