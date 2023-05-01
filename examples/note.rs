use anyhow::Ok;
use genkit::{Entity, Generator, Genkit};

struct App {}

struct Note {}

impl Entity for Note {}

impl Generator for App {
    type Entity = Note;

    fn on_load(&self, _source: &std::path::Path) -> anyhow::Result<Self::Entity> {
        Ok(Note {})
    }

    fn on_reload(&self, _source: &std::path::Path) -> anyhow::Result<Self::Entity> {
        Ok(Note {})
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = App {};
    Genkit::new("note", app).bootstrap().await?;
    Ok(())
}
