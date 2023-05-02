use anyhow::Ok;
use clap::Command;
use genkit::{Cmd, Entity, Generator, Genkit};

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

struct VersionCmd;

#[async_trait::async_trait]
impl Cmd for VersionCmd {
    fn on_init(&self) -> clap::Command {
        Command::new("version")
    }

    async fn on_execute(&self, _matches: &clap::ArgMatches) -> anyhow::Result<()> {
        println!("Version command");
        Ok(())
    }
}

struct PublishCmd;

#[async_trait::async_trait]
impl Cmd for PublishCmd {
    fn on_init(&self) -> clap::Command {
        Command::new("publish")
    }

    async fn on_execute(&self, _matches: &clap::ArgMatches) -> anyhow::Result<()> {
        println!("Publish command");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = App {};
    Genkit::new("note", app)
        .set_banner("NOTE")
        .add_command(VersionCmd)
        .add_command(PublishCmd)
        .bootstrap()
        .await?;
    Ok(())
}
