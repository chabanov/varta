//! Print the registry module, or write it to the explicit output path.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = varta_common::browser::javascript()?;
    if let Some(path) = std::env::args_os().nth(1) {
        std::fs::write(path, output)?;
    } else {
        print!("{output}");
    }
    Ok(())
}
