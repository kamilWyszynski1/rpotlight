use crate::registry;
use anyhow::Context;
use notify::Watcher;
use std::path::Path;
use tokio::sync::mpsc;
use tracing::{error, info};

pub async fn create_watcher(
    path: &Path,
    tx: mpsc::Sender<registry::RegistryMessage>,
) -> anyhow::Result<()> {
    let path_clone = path.as_os_str().to_str().unwrap().to_string();

    let mut watcher =
        notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            if let Err(err) = match res {
                Ok(event) => {
                    info!(
                        function = "create_watcher",
                        path = &path_clone,
                        event = ?event,
                        "received event"
                    );
                    match event.kind {
                        notify::EventKind::Modify(_) => tx
                            .blocking_send(registry::RegistryMessage::Parse(path_clone.clone()))
                            .context("could not send modify event"),
                        notify::EventKind::Remove(_) => tx
                            .blocking_send(registry::RegistryMessage::Remove(path_clone.clone()))
                            .context("could not send modify event"),
                        _ => {
                            println!("here");
                            Ok(())
                        }
                    }
                }
                Err(e) => Err(anyhow::format_err!("{}", e.to_string())),
            } {
                error!(err = err.to_string(), "watcher error occured")
            }
        })?;

    watcher.watch(path, notify::RecursiveMode::NonRecursive)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{registry, watcher::create_watcher};
    use std::{env::temp_dir, fs::File, io::Write, time::Duration};
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_create_watcher() -> anyhow::Result<()> {
        tracing_subscriber::fmt::init();

        let dir_path = temp_dir();
        let file_path = dir_path.join("foo.rs");
        let mut file = File::create(&file_path)?;
        file.write_all("fn foo() {}".as_bytes())?;

        let (tx, mut rx) = mpsc::channel(10);
        let c_tx = tx.clone();
        dbg!(&file_path);
        create_watcher(&file_path, tx).await?;
        println!("waiting");
        tokio::time::sleep(Duration::new(2, 0)).await;
        std::fs::remove_file(&file_path)?;
        println!("removed");

        let msg = rx.recv().await.unwrap();
        assert_eq!(
            msg,
            registry::RegistryMessage::Remove(file_path.to_str().unwrap().to_string())
        );
        drop(c_tx);
        Ok(())
    }
}
