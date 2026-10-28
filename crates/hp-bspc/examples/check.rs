fn main() {
    let path = std::env::args().nth(1).expect("bsp path");
    let bytes = std::fs::read(&path).expect("read");
    let problems = hp_bspc::validate::validate(&bytes);
    println!("{} problems", problems.len());
    for p in problems.iter().take(20) {
        println!("  {p}");
    }
}
