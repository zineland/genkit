use anyhow::Ok;
use genkit::{Entity, Generator, Genkit};

struct App {}

struct Note {
    template: String,
}

impl Entity for Note {}

impl Generator for App {
    type Entity = Note;

    fn on_load(&self, _source: &std::path::Path) -> anyhow::Result<Self::Entity> {
        Ok(Note {
            template: String::from("head_template.jinja"),
        })
    }

    fn on_reload(&self, _source: &std::path::Path) -> anyhow::Result<Self::Entity> {
        Ok(Note {
            template: String::from("head_template.jinja"),
        })
    }

    fn on_extend_environment<'a>(
        &self,
        _source: &std::path::Path,
        mut env: minijinja::Environment<'a>,
        entity: &'a Self::Entity,
    ) -> minijinja::Environment<'a> {
        env.add_template("head_template.jinja", &entity.template)
            .expect("Cannot add head_template");
        env
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = App {};
    Genkit::new("note", app)
        .set_banner("NOTE")
        .bootstrap()
        .await?;
    Ok(())
}
