mod common;

use hp_bspc::{CompileOptions, VisLevel};

fn maps() -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> =
        common::maps::golden().into_iter().map(|(n, m)| (n.to_owned(), m.text())).collect();
    v.push(("generated".into(), common::maps::generated(7).text()));
    v.push(("generated, leaking".into(), common::maps::generated_leaking(11).text()));
    v.push(("generated 2".into(), common::maps::generated(23).text()));
    v
}

fn run(text: &str, opts: &CompileOptions) -> (Vec<u8>, String, Option<Vec<[u64; 3]>>) {
    let out = common::compile_with(text, &common::GameMats, opts);
    let leak = out.leak.map(|p| p.iter().map(|q| [q.x.to_bits(), q.y.to_bits(), q.z.to_bits()]).collect());
    (out.bsp, out.prt, leak)
}

fn run_on(threads: usize, text: &str, opts: &CompileOptions) -> (Vec<u8>, String, Option<Vec<[u64; 3]>>) {
    let pool =
        rayon::ThreadPoolBuilder::new().num_threads(threads).stack_size(64 << 20).build().expect("a thread pool");
    pool.install(|| run(text, opts))
}

fn check(opts: &CompileOptions) {
    let with_vis = opts.vis != VisLevel::Off;
    for (name, text) in maps() {
        if with_vis && name.starts_with("generated") && name != "generated" {
            continue;
        }
        let first = run(&text, opts);
        let again = run(&text, opts);
        assert!(first == again, "{name}: two compiles in a row differ ({:?})", opts.vis);
        for threads in [1, 2, 3, 8] {
            let other = run_on(threads, &text, opts);
            assert!(first == other, "{name}: the compile on {threads} threads differs ({:?})", opts.vis);
        }
    }
}

#[test]
fn same_bytes_without_visibility() {
    check(&CompileOptions::default());
}

#[test]
fn same_bytes_with_fast_visibility() {
    check(&CompileOptions { vis: VisLevel::Fast, ..Default::default() });
}

#[test]
fn same_bytes_with_full_visibility() {
    check(&CompileOptions { vis: VisLevel::Full, ..Default::default() });
}

#[test]
fn same_bytes_without_blocks() {
    check(&CompileOptions { block_size: 0, ..Default::default() });
}

#[test]
fn same_bytes_with_big_blocks() {
    check(&CompileOptions { block_size: 2048, ..Default::default() });
}
