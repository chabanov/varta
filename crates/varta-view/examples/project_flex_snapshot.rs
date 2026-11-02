//! Project an existing headless snapshot for flex display comparison.
//! Input and output are JSON; no application-specific fixtures are referenced.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut value: serde_json::Value = serde_json::from_reader(std::io::stdin().lock())?;
    varta_view::project_flex_blockification(&mut value);
    serde_json::to_writer(std::io::stdout().lock(), &value)?;
    Ok(())
}
