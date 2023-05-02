use anyhow::Result;
use async_trait::async_trait;
use clap::{Arg, ArgAction, Command};
mod build;
mod lint;
mod serve;

pub(crate) use build::*;
pub(crate) use lint::LintCmd;
pub(crate) use serve::*;

#[async_trait]
pub trait Cmd {
    fn on_init(&self) -> Command;

    async fn on_execute(&self, arg_matches: &crate::ArgMatches) -> Result<()>;
}

pub(crate) fn build_root_command(name: &'static str) -> Command {
    Command::new(name)
        .subcommand(
            Command::new("build")
                .args([
                    Arg::new("source").help(format!("The source directory of {name} site")),
                    Arg::new("dest").help("The destination directory. Default dest dir is `build`"),
                    Arg::new("watch")
                        .short('w')
                        .long("watch")
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
                        .long("port")
                        .value_parser(clap::value_parser!(u16))
                        .default_missing_value("3000")
                        .help("The port to listen"),
                    Arg::new("open")
                        .long("open")
                        .short('o')
                        .action(ArgAction::SetTrue)
                        .help("Auto open browser after server started"),
                ])
                .about("Serve the site"),
        )
}
