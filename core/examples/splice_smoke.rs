//! Splices a fragment into a real .gui file and prints the inserted region.
//! usage: splice_smoke <gui file> <fragment> <window>
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let file = std::fs::read_to_string(&args[1]).unwrap();
    let fragment = std::fs::read_to_string(&args[2]).unwrap();
    let rendered = eu4_harmony_core::pdx::render_fragment(&fragment, "eu4harmony_smoke_", Some(eu4_harmony_core::splice::slot(0))).unwrap();
    let out = eu4_harmony_core::pdx::splice_window(&file, &args[3], &rendered.text).unwrap();
    let at = out.find("eu4harmony_smoke_").unwrap();
    println!("{} -> {} bytes, elements {:?}\n...{}...", file.len(), out.len(), rendered.elements, &out[at.saturating_sub(120)..(at + 300).min(out.len())]);
}
