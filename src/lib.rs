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

pub fn set_current_mode(mode: Mode) {
    *MODE.write() = mode;
}

pub trait Genkit {
    fn build(&self, reload: bool) -> Result<()>;
}
