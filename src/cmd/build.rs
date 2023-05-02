use std::{path::Path, sync::mpsc, time::Duration};

use crate::{data, engine::GenkitEngine, Generator};
use anyhow::Result;
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebouncedEventKind};
use tokio::sync::broadcast::Sender;

pub(crate) async fn watch_build<G, P: AsRef<Path>>(
    generator: G,
    source: P,
    dest: P,
    watch: bool,
    sender: Option<Sender<()>>,
) -> Result<()>
where
    G: Generator + Send + 'static,
{
    let source = std::fs::canonicalize(source)?;
    let source_path = source.clone();
    data::load(&source);
    let mut engine = GenkitEngine::new(&source, dest, generator)?;
    // Spawn the build process as a blocking task, avoid starving other tasks.
    let build_result = tokio::task::spawn_blocking(move || {
        engine.build(false)?;

        if let Some(sender) = sender.as_ref() {
            // Notify the first building finished.
            sender.send(())?;
        }

        if watch {
            tokio::spawn(async move {
                tokio::signal::ctrl_c().await.unwrap();
                // Save data only when the process gonna exist
                data::export(&source_path).unwrap();
                std::process::exit(0);
            });

            println!("Watching...");
            let (tx, rx) = mpsc::channel();
            let mut debouncer = new_debouncer(Duration::from_millis(500), None, tx)?;
            let watcher = debouncer.watcher();
            watcher.watch(&source, RecursiveMode::Recursive)?;

            // Watch templates and static directory in debug mode to support reload.
            #[cfg(debug_assertions)]
            {
                for dir in &["templates", "static"] {
                    let path = Path::new(dir);
                    if path.exists() {
                        watcher.watch(path, RecursiveMode::Recursive)?;
                    }
                }
            }

            loop {
                match rx.recv() {
                    Ok(result) => match result {
                        Ok(events) => {
                            // Prevent build too frequently, otherwise it will cause program stuck.
                            if events
                                .iter()
                                .any(|event| event.kind == DebouncedEventKind::Any)
                            {
                                match engine.build(true) {
                                    Ok(_) => {
                                        if let Some(sender) = sender.as_ref() {
                                            sender.send(())?;
                                        }
                                        // Export data to file after build
                                        data::export(&source).unwrap();
                                    }
                                    Err(err) => {
                                        println!("build error: {:?}", &err);
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            println!("watch error: {:?}", &err);
                        }
                    },
                    Err(err) => println!("watch error: {:?}", &err),
                }
            }
        } else {
            data::export(&source).unwrap();
        }
        anyhow::Ok(())
    })
    .await?;

    if cfg!(debug_assertions) {
        // Explicitly panic build result in debug mode
        build_result.unwrap();
    } else if let Err(err) = build_result {
        println!("Error: {}", &err);
        std::process::exit(1);
    }
    Ok(())
}
