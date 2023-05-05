use std::{collections::HashMap, path::Path};

use clap::Command;
use entity::MarkdownConfig;
use parking_lot::RwLock;

mod cmd;
pub mod code_blocks;
pub mod context;
mod data;
mod engine;
pub mod entity;
pub mod helpers;
pub mod html;
pub mod jinja;
pub mod markdown;

pub use clap::ArgMatches;
pub use cmd::Cmd;
pub use context::Context;
pub use entity::Entity;
pub use markdown::MarkdownVisitor;
pub use minijinja::Environment;

use anyhow::Result;

static MODE: RwLock<Mode> = parking_lot::const_rwlock(Mode::Unknown);

#[derive(Copy, Clone)]
pub enum Mode {
    Build,
    Serve,
    Unknown,
}

/// Get current run mode.
pub fn current_mode() -> Mode {
    *MODE.read()
}

fn set_current_mode(mode: Mode) {
    *MODE.write() = mode;
}

#[allow(unused_variables)]
pub trait Generator {
    type Entity: Entity;

    fn on_load(&self, source: &Path) -> Result<Self::Entity>;

    fn on_reload(&self, source: &Path) -> Result<Self::Entity>;

    fn on_extend_environment<'a>(
        &self,
        source: &Path,
        env: Environment<'a>,
        entity: &'a Self::Entity,
    ) -> Environment<'a> {
        env
    }

    fn on_render(
        &self,
        env: &Environment,
        context: Context,
        entity: &Self::Entity,
        source: &Path,
        dest: &Path,
    ) -> Result<()> {
        Ok(())
    }

    fn get_markdown_config(&self, entity: &Self::Entity) -> Option<MarkdownConfig> {
        None
    }
}

pub struct Genkit<G> {
    root_command: Command,
    command_map: HashMap<String, Box<dyn Cmd>>,
    generator: G,
    banner: Option<&'static str>,
}

impl<G> Genkit<G>
where
    G: Generator + Send + 'static,
{
    pub fn new(name: &'static str, generator: G) -> Self {
        Self::with_command(Command::new(name), generator)
    }

    pub fn with_command(command: Command, generator: G) -> Self {
        Self {
            root_command: cmd::build_root_command(command),
            command_map: HashMap::new(),
            generator,
            banner: None,
        }
    }

    pub fn data_filename(self, filename: &'static str) -> Self {
        data::set_data_filename(filename);
        self
    }

    pub fn markdown_visitor<V>(self, visitor: V) -> Self
    where
        V: MarkdownVisitor + Send + Sync + 'static,
    {
        data::set_markdown_visitor(Box::new(visitor));
        self
    }

    pub fn banner(mut self, banner: &'static str) -> Self {
        self.banner = Some(banner);
        self
    }

    pub fn add_command<C: Cmd + 'static>(mut self, cmd: C) -> Self {
        let command = cmd.on_init();
        let name = command.get_name().to_owned();
        self.root_command = self.root_command.subcommand(command);
        self.command_map.insert(name, Box::new(cmd));
        self
    }

    pub async fn run(mut self) -> Result<()> {
        self = self.add_command(cmd::LintCmd);

        let name = self.root_command.get_name().to_owned();
        let matches = self.root_command.arg_required_else_help(true).get_matches();
        match matches.subcommand() {
            Some(("build", arg_matches)) => {
                set_current_mode(Mode::Build);
                let source = arg_matches
                    .get_one::<String>("source")
                    .cloned()
                    .unwrap_or_else(|| ".".into());
                let dest = arg_matches
                    .get_one::<String>("dest")
                    .cloned()
                    .unwrap_or_else(|| "build".into());
                let watch = arg_matches.get_flag("watch");

                cmd::watch_build(self.generator, &source, &dest, watch, None).await?;
                println!("Build success! The build directory is `{dest}`.");
            }
            Some(("serve", arg_matches)) => {
                set_current_mode(Mode::Serve);
                let source = arg_matches
                    .get_one::<String>("source")
                    .cloned()
                    .unwrap_or_else(|| ".".into());
                let port = arg_matches.get_one::<u16>("port").copied().unwrap_or(3000);
                let open = arg_matches.get_flag("open");

                cmd::run_serve(self.generator, &source, port, open, &name, self.banner).await?;
            }
            Some((name, arg_matches)) => {
                if let Some(command) = self.command_map.get(name) {
                    command.on_execute(arg_matches).await?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
