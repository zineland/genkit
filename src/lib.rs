use std::path::Path;

use clap::{Arg, ArgAction, Command};
use parking_lot::RwLock;

pub mod build;
pub mod code_blocks;
pub mod context;
pub mod data;
pub mod engine;
pub mod entity;
pub mod helpers;
pub mod html;
pub mod markdown;
pub mod serve;

pub use context::Context;
pub use entity::Entity;
pub use minijinja::Environment;

use anyhow::Result;
use serve::run_serve;

use crate::build::watch_build;

static MODE: RwLock<Mode> = parking_lot::const_rwlock(Mode::Unknown);

#[derive(Copy, Clone)]
pub(crate) enum Mode {
    Build,
    Serve,
    Unknown,
}

/// Get current run mode.
pub(crate) fn current_mode() -> Mode {
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
        env: Environment<'a>,
        entity: &Self::Entity,
    ) -> Environment<'a> {
        env
    }

    fn on_render(&self, env: &Environment, context: Context, entity: &Self::Entity) -> Result<()> {
        Ok(())
    }
}

pub struct Genkit<G> {
    command: Command,
    generator: G,
}

fn build_command(name: &'static str) -> Command {
    Command::new(name)
        .subcommand(
            Command::new("build")
                .args([
                    Arg::new("source").help(format!("The source directory of {name} site")),
                    Arg::new("dest").help("The destination directory. Default dest dir is `build`"),
                    Arg::new("watch")
                        .short('w')
                        .action(ArgAction::SetTrue)
                        .help("Enable watching"),
                ])
                .about("Build the site"),
        )
        .subcommand(
            Command::new("serve")
                .args([
                    Arg::new("source").help(format!("The source directory of {name} site")),
                    Arg::new("port")
                        .short('p')
                        .value_parser(clap::value_parser!(u16))
                        .default_missing_value("3000")
                        .help("The port to listen"),
                    Arg::new("open")
                        .short('o')
                        .action(ArgAction::SetTrue)
                        .help("Auto open browser after server started"),
                ])
                .about("Serve the site"),
        )
}

impl<G> Genkit<G>
where
    G: Generator + Send + 'static,
{
    pub fn new(name: &'static str, generator: G) -> Self {
        let command = build_command(name);
        Self { command, generator }
    }

    pub fn set_data_filename(&self, filename: &'static str) {
        data::set_data_filename(filename);
    }

    pub async fn bootstrap(self) -> Result<()> {
        let matches = self.command.get_matches();
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

                watch_build(self.generator, &source, &dest, watch, None).await?;
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

                run_serve(self.generator, &source, port, open).await?;
            }
            _ => {}
        }
        Ok(())
    }
}
