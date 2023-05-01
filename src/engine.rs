use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{context::Context, data, jinja::init_environment, Entity, Generator};

use anyhow::Result;

#[derive(Debug)]
pub(crate) struct GenkitEngine<G> {
    source: PathBuf,
    dest: PathBuf,
    generator: G,
}

impl<G> GenkitEngine<G>
where
    G: Generator + Send,
{
    pub fn new(source: impl AsRef<Path>, dest: impl AsRef<Path>, generator: G) -> Result<Self> {
        let dest = dest.as_ref().to_path_buf();
        if !dest.exists() {
            fs::create_dir_all(&dest)?;
        }
        Ok(GenkitEngine {
            source: source.as_ref().to_path_buf(),
            dest,
            generator,
        })
    }

    pub fn build(&mut self, reload: bool) -> Result<()> {
        let instant = std::time::Instant::now();
        let source = self.source.as_ref();
        let mut entity = if reload {
            self.generator.on_reload(source)?
        } else {
            self.generator.on_load(source)?
        };

        entity.parse(source)?;

        let env = self
            .generator
            .on_extend_environment(init_environment(), &entity);

        if let Some(markdown_config) = self.generator.get_markdown_config(&entity) {
            let mut guard = data::write();
            guard.set_markdown_config(markdown_config);
        }

        let context = Context::new();
        entity
            .render(&env, context.clone(), self.dest.as_ref())
            .expect("Render zine failed.");

        self.generator.on_render(&env, context, &entity)?;
        println!("Build cost: {}ms", instant.elapsed().as_millis());
        Ok(())
    }
}
