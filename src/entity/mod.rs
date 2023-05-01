use anyhow::Result;
use minijinja::Environment;
use rayon::{
    iter::{IntoParallelRefMutIterator, ParallelIterator},
    prelude::IntoParallelRefIterator,
};
use std::path::Path;

mod markdown;

use crate::context::Context;

pub use markdown::MarkdownConfig;

/// A trait represents the entity of config file.
///
/// An entity contains two stage:
/// - **parse**, the stage the entity to parse its attribute, such as parse markdown to html.
/// - **render**, the stage to render the entity to html file.
///
/// [`Entity`] has default empty implementations for both methods.
#[allow(unused_variables)]
pub trait Entity {
    fn parse(&mut self, source: &Path) -> Result<()> {
        Ok(())
    }

    fn render(&self, env: &Environment, context: Context, dest: &Path) -> Result<()> {
        Ok(())
    }
}

impl<T: Entity> Entity for Option<T> {
    fn parse(&mut self, source: &Path) -> Result<()> {
        if let Some(entity) = self {
            entity.parse(source)?;
        }
        Ok(())
    }

    fn render(&self, env: &Environment, context: Context, dest: &Path) -> Result<()> {
        if let Some(entity) = self {
            entity.render(env, context, dest)?;
        }
        Ok(())
    }
}

impl<T: Entity + Sync + Send + Clone + 'static> Entity for Vec<T> {
    fn parse(&mut self, source: &Path) -> Result<()> {
        self.par_iter_mut()
            .try_for_each(|entity| entity.parse(source))
    }

    fn render(&self, env: &Environment, context: Context, dest: &Path) -> Result<()> {
        self.par_iter().try_for_each(|entity| {
            let context = context.clone();
            entity.render(env, context, dest)
        })
    }
}
