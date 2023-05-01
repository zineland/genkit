use std::error::Error;

use genkit::Genkit;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    Genkit::new("note").bootstrap().await?;
    Ok(())
}
