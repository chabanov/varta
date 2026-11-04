#[path = "decoration_probe/probe.rs"]
mod probe;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    probe::run(None)
}
