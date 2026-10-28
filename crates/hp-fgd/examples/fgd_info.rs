use std::process::ExitCode;
use std::time::Instant;

use hp_fgd::{Fgd, ResolvedClass};

fn dump(class: &ResolvedClass) {
    println!("{} ({}): {}", class.name, class.kind.directive(), class.description);
    println!("  bases: {}", class.bases.join(", "));
    for helper in &class.helpers {
        println!("  helper {}({})", helper.name, helper.args.join(", "));
    }
    if let Some((mins, maxs)) = class.size() {
        println!("  size {mins:?} {maxs:?}");
    }
    for prop in &class.props {
        let default = prop.default.as_deref().unwrap_or("");
        println!("  {}({}) \"{}\" default \"{default}\"", prop.name, prop.kind, prop.label());
        for choice in &prop.choices {
            println!("      {} = {}", choice.value, choice.display);
        }
        for flag in &prop.flags {
            println!("      [{}] {} = {}", if flag.default_on { 'x' } else { ' ' }, flag.value, flag.display);
        }
    }
    for io in &class.inputs {
        println!("  input {}({})", io.name, io.arg_type);
    }
    for io in &class.outputs {
        println!("  output {}({})", io.name, io.arg_type);
    }
    println!("  new entity keyvalues: {:?}", class.default_keyvalues());
}

fn main() -> ExitCode {
    let mut files = Vec::new();
    let mut classes = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--class" {
            classes.extend(args.next());
        } else {
            files.push(arg);
        }
    }
    if files.is_empty() {
        eprintln!("usage: fgd_info <file.fgd>... [--class <name>]...");
        return ExitCode::FAILURE;
    }

    let start = Instant::now();
    let fgd = match Fgd::load_many(&files) {
        Ok(fgd) => fgd,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    };
    let loaded = Instant::now();
    let resolved = fgd.resolve();
    let done = Instant::now();

    println!(
        "{} declared classes, {} resolved ({} point, {} solid) in {:.2} ms (load {:.2} ms, resolve {:.2} ms)",
        fgd.classes.len(),
        resolved.len(),
        resolved.point_classes().len(),
        resolved.solid_classes().len(),
        (done - start).as_secs_f64() * 1e3,
        (loaded - start).as_secs_f64() * 1e3,
        (done - loaded).as_secs_f64() * 1e3,
    );
    for warning in fgd.warnings.iter().chain(&resolved.warnings) {
        println!("warning: {warning}");
    }
    for name in &classes {
        match resolved.class(name) {
            Some(class) => dump(class),
            None => println!("no class named {name}"),
        }
    }
    ExitCode::SUCCESS
}
