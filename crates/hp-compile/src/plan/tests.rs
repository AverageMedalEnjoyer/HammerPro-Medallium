use super::*;

fn root() -> PathBuf {
    std::env::temp_dir().join("hp_plan_tests")
}

fn tools() -> ToolPaths {
    let r = root();
    ToolPaths {
        vbsp: r.join("bin").join("vbsp.exe"),
        vvis: r.join("bin").join("vvis.exe"),
        vrad: r.join("bin").join("vrad.exe"),
        game_exe: Some(r.join("hl2.exe")),
        game_dir: r.join("game"),
        maps_dir: r.join("game").join("maps"),
    }
}

fn vmf() -> PathBuf {
    root().join("src").join("gm_test.vmf")
}

fn find(plan: &CompilePlan, kind: StepKind) -> Option<&Step> {
    plan.steps.iter().find(|s| s.kind == kind)
}

fn kinds(plan: &CompilePlan) -> Vec<StepKind> {
    plan.steps.iter().map(|s| s.kind).collect()
}

#[test]
fn a_full_compile_runs_vbsp_vvis_and_vrad_from_the_configuration() {
    let t = tools();
    let plan = CompilePlan::preset(Preset::Normal, &t, &vmf(), &PlanOptions::default()).unwrap();
    let k = kinds(&plan);
    assert_eq!(&k[..3], &[StepKind::Bsp, StepKind::Vis, StepKind::Rad]);
    assert_eq!(find(&plan, StepKind::Bsp).unwrap().exe, t.vbsp);
    assert_eq!(find(&plan, StepKind::Vis).unwrap().exe, t.vvis);
    assert_eq!(find(&plan, StepKind::Rad).unwrap().exe, t.vrad);
    let map = vmf().with_extension("").to_string_lossy().into_owned();
    for kind in [StepKind::Bsp, StepKind::Vis, StepKind::Rad] {
        let a = &find(&plan, kind).unwrap().args;
        assert_eq!(a.last(), Some(&map), "{kind:?}: {a:?}");
        assert!(a.windows(2).any(|w| w[0] == "-game" && Path::new(&w[1]) == t.game_dir), "{kind:?}: {a:?}");
        assert!(!a.iter().any(|x| x.starts_with("--")), "no built-in compiler switches: {a:?}");
    }
}

#[test]
fn the_presets_choose_the_vis_and_vrad_options() {
    let t = tools();
    let rad = |preset| find(&CompilePlan::preset(preset, &t, &vmf(), &PlanOptions::default()).unwrap(), StepKind::Rad).map(|s| s.args.clone());
    let fast = rad(Preset::Draft).unwrap();
    assert!(fast.iter().any(|a| a == "-fast"), "{fast:?}");
    let fin = rad(Preset::Final).unwrap();
    assert!(fin.iter().any(|a| a == "-final"), "{fin:?}");
    let draft = CompilePlan::preset(Preset::Draft, &t, &vmf(), &PlanOptions::default()).unwrap();
    assert!(find(&draft, StepKind::Vis).is_none_or(|v| v.args.iter().any(|a| a == "-fast")));
}

#[test]
fn threads_and_extra_arguments_reach_the_right_programs() {
    let opts = PlanOptions {
        threads: Some(6),
        extra_bsp_args: "-novirtualmesh".to_owned(),
        extra_rad_args: "-bounce 4".to_owned(),
        ..PlanOptions::default()
    };
    let plan = CompilePlan::preset(Preset::Normal, &tools(), &vmf(), &opts).unwrap();
    let bsp = &find(&plan, StepKind::Bsp).unwrap().args;
    assert!(bsp.iter().any(|a| a == "-novirtualmesh"), "{bsp:?}");
    assert!(!bsp.iter().any(|a| a == "-threads"), "vbsp has no -threads: {bsp:?}");
    for kind in [StepKind::Vis, StepKind::Rad] {
        let a = &find(&plan, kind).unwrap().args;
        assert!(a.windows(2).any(|w| w[0] == "-threads" && w[1] == "6"), "{kind:?}: {a:?}");
    }
    let rad = &find(&plan, StepKind::Rad).unwrap().args;
    assert!(rad.windows(2).any(|w| w[0] == "-bounce" && w[1] == "4"), "{rad:?}");
}

#[test]
fn the_staged_game_folder_replaces_the_game_argument() {
    let opts = PlanOptions { compile_game_dir: Some(root().join("cache")), ..PlanOptions::default() };
    let plan = CompilePlan::preset(Preset::Normal, &tools(), &vmf(), &opts).unwrap();
    for kind in [StepKind::Bsp, StepKind::Vis, StepKind::Rad] {
        let a = &find(&plan, kind).unwrap().args;
        assert!(a.windows(2).any(|w| w[0] == "-game" && Path::new(&w[1]) == root().join("cache")), "{a:?}");
    }
}

#[test]
fn bad_map_names() {
    let t = tools();
    let o = PlanOptions::default();
    assert!(matches!(
        CompilePlan::preset(Preset::Draft, &t, Path::new(r"C:\maps\gm_a.v2.vmf"), &o),
        Err(CompileError::InvalidMapName { problem: MapNameProblem::Dot, .. })
    ));
    let err = CompilePlan::preset(Preset::Draft, &t, Path::new(r"C:\maps\my map.vmf"), &o);
    assert!(
        matches!(&err, Err(CompileError::InvalidMapName { problem: MapNameProblem::Whitespace, name }) if name == "my map")
    );
    assert!(err.err().is_some_and(|e| e.to_string().contains("spaces")));
    assert!(matches!(CompilePlan::preset(Preset::Draft, &t, Path::new(""), &o), Err(CompileError::InvalidMapPath(_))));
}

#[test]
fn split_and_quote() {
    assert_eq!(split_args(""), Vec::<String>::new());
    assert_eq!(split_args("  -a  -b 2 "), vec!["-a", "-b", "2"]);
    assert_eq!(split_args(r#"-lights "C:\my dir\x.rad" -x"#), vec!["-lights", r"C:\my dir\x.rad", "-x"]);
    assert_eq!(split_args(r#"a "" b"#), vec!["a", "", "b"]);
    assert_eq!(split_args(r#"say \"hi\""#), vec!["say", "\"hi\""]);
    assert_eq!(split_args(r"C:\path\to"), vec![r"C:\path\to"]);

    assert_eq!(quote_arg("-fast"), "-fast");
    assert_eq!(quote_arg(""), "\"\"");
    assert_eq!(quote_arg(r"C:\My Maps\a"), r#""C:\My Maps\a""#);
    assert_eq!(quote_arg(r"C:\My Maps\"), r#""C:\My Maps\\""#);
    assert_eq!(quote_arg(r#"say "hi""#), r#""say \"hi\"""#);
}

#[test]
fn options_roundtrip_defaults() {
    let o = PlanOptions::default();
    assert_eq!(o.lighting, Lighting::Ldr);
    assert!(o.copy_to_maps && !o.launch_game && !o.stop_on_leak);
    assert_eq!(o.game_args, DEFAULT_GAME_ARGS);
    assert_eq!(crate::launch::url_encode("gm_a b+c"), "gm_a%20b%2Bc");
    let old: PlanOptions = serde_json::from_str(r#"{"extra_vis_args":"-fast","caps":{"bsp":{}},"threads":4}"#).unwrap();
    assert_eq!(old.threads, Some(4));
}

#[test]
fn presets_and_modes() {
    for p in [Preset::Draft, Preset::Normal, Preset::Final] {
        assert_eq!(Preset::of_modes(p.modes()), p);
    }
    let custom = StepModes::new(BspMode::OnlyEnts, VisMode::Off, RadMode::Off);
    assert_eq!(Preset::of_modes(custom), Preset::Custom);
    assert_eq!(Preset::Custom.modes(), Preset::Normal.modes());
    let de = serde::de::value::StrDeserializer::<serde::de::value::Error>::new("Fast");
    assert_eq!(Preset::deserialize(de).expect("alias"), Preset::Draft);
}

fn partial_dir(name: &str, with_bsp: bool) -> PathBuf {
    let dir = crate::test_util::scratch(name);
    if with_bsp {
        std::fs::write(dir.join("gm_part.bsp"), b"VBSP").unwrap();
    }
    dir.join("gm_part.vmf")
}

#[test]
fn partial_modes() {
    let vmf = partial_dir("plan_partial", true);
    let opts = PlanOptions {
        modes: Some(StepModes::new(BspMode::OnlyEnts, VisMode::Full, RadMode::Off)),
        launch_game: true,
        ..PlanOptions::default()
    };
    let plan = CompilePlan::preset(Preset::Normal, &tools(), &vmf, &opts).unwrap();
    assert_eq!(kinds(&plan), vec![StepKind::Bsp, StepKind::CopyToMaps, StepKind::LaunchGame]);
    assert!(plan.steps[0].args.contains(&"-onlyents".to_owned()));
    assert!(!plan.steps[0].args.iter().any(|a| a == "-vis"));
    assert_eq!(plan.modes.vis, VisMode::Off);
    assert!(plan.is_partial() && plan.notices.is_empty());
    let launch = &plan.steps[2].args;
    let stale = launch.iter().position(|a| a == "-allowstalezip").expect("-allowstalezip");
    assert!(stale < launch.iter().position(|a| a == "+map").unwrap_or(0));

    let opts = PlanOptions {
        modes: Some(StepModes::new(BspMode::OnlyEnts, VisMode::Off, RadMode::Normal)),
        copy_to_maps: false,
        ..PlanOptions::default()
    };
    let plan = CompilePlan::preset(Preset::Normal, &tools(), &vmf, &opts).unwrap();
    assert_eq!(kinds(&plan), vec![StepKind::Bsp, StepKind::Rad]);

    let opts = PlanOptions {
        modes: Some(StepModes::new(BspMode::EntsAndProps, VisMode::Off, RadMode::Draft)),
        copy_to_maps: false,
        ..PlanOptions::default()
    };
    let plan = CompilePlan::preset(Preset::Normal, &tools(), &vmf, &opts).unwrap();
    assert_eq!(kinds(&plan), vec![StepKind::Bsp, StepKind::Rad]);
    let bsp = &plan.steps[0].args;
    assert!(bsp.contains(&"-onlyents".to_owned()) && bsp.contains(&"-onlyprops".to_owned()));

    let vmf = partial_dir("plan_partial_nobsp", false);
    let opts = PlanOptions {
        modes: Some(StepModes::new(BspMode::OnlyEnts, VisMode::Off, RadMode::Off)),
        copy_to_maps: false,
        launch_game: true,
        ..PlanOptions::default()
    };
    let plan = CompilePlan::preset(Preset::Draft, &tools(), &vmf, &opts).unwrap();
    assert_eq!(kinds(&plan), vec![StepKind::Bsp, StepKind::Vis, StepKind::Rad, StepKind::LaunchGame]);
    assert_eq!(plan.modes, Preset::Draft.modes());
    assert!(matches!(&plan.notices[..], [PlanNotice { step: 0, notice: Notice::NoPreviousBsp { .. } }]));
    assert!(!plan.steps[3].args.iter().any(|a| a == "-allowstalezip"));

    let opts = PlanOptions {
        modes: Some(StepModes::new(BspMode::Off, VisMode::Full, RadMode::Normal)),
        copy_to_maps: false,
        ..PlanOptions::default()
    };
    let plan = CompilePlan::preset(Preset::Custom, &tools(), &vmf, &opts).unwrap();
    assert_eq!(kinds(&plan), vec![StepKind::Rad]);
    assert_eq!(plan.modes.vis, VisMode::Off);
}

