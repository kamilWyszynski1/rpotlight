use crate::registry;
use anyhow::Context;
use notify::Watcher;
use std::path::Path;
use tokio::sync::mpsc;
use tracing::{error, info};

pub async fn create_watcher(
    path: &Path,
    tx: mpsc::Sender<registry::RegistryMessage>,
) -> anyhow::Result<notify::INotifyWatcher> {
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
                        notify::EventKind::Access(_) => tx
                            .blocking_send(registry::RegistryMessage::Parse(path_clone.clone()))
                            .context("could not send modify event"),
                        notify::EventKind::Modify(_) => tx
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

    watcher.watch(path, notify::RecursiveMode::Recursive)?;

    Ok(watcher)
}

#[cfg(test)]
mod tests {
    use crate::{registry, watcher::create_watcher};
    use std::{fs::File, io::Write, time::Duration};
    use tempdir::TempDir;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_create_watcher() -> anyhow::Result<()> {
        tracing_subscriber::fmt::init();

        let tmp_dir = TempDir::new("example")?;
        let file_path = tmp_dir.path().join("foo.rs");
        let mut file = File::create(&file_path)?;
        file.write_all("fn foo() {}".as_bytes())?;

        let (tx, mut rx) = mpsc::channel(10);
        let c_tx = tx.clone();

        let watcher = create_watcher(&file_path, tx).await?;

        tokio::time::sleep(Duration::new(0, 500)).await;
        std::fs::remove_file(&file_path)?;

        let msg = rx.recv().await.unwrap();
        assert_eq!(
            msg,
            registry::RegistryMessage::Remove(file_path.to_str().unwrap().to_string())
        );
        drop(c_tx);
        drop(watcher);

        Ok(())
    }
}
