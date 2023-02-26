use crate::registry::RegistryMessage;
use anyhow::Context;
use notify::{self, Event, EventKind, INotifyWatcher, RecursiveMode, Watcher};
use std::{collections::HashMap, path::Path, sync::Arc};
use tokio::sync::{mpsc, Mutex};
use tracing::{error, info};

/// Manager handles creation and shutdown of watchers.
/// All messages are proxied by manager which is listening for Remove events.
/// If one occurs, watcher is closed.
#[derive(Debug, Default)]
pub struct WatcherManager {
    /// Contains currently running watchers. If watcher receives remove message
    /// it will be deleted from state and its tokio task will end.
    pub state: Arc<Mutex<HashMap<String, INotifyWatcher>>>,
}

impl WatcherManager {
    pub async fn watcher_proxy(
        &self,
        path: &Path,
        origin_tx: mpsc::Sender<RegistryMessage>,
    ) -> anyhow::Result<()> {
        let (tx, mut rx) = mpsc::channel(128);
        let watcher = create_watcher(path, tx)?;

        let state = self.state.clone();
        let key = path.to_str().context("invalid path")?.to_string();

        state.lock().await.insert(key.clone(), watcher);

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    msg = rx.recv() => {
                        match msg {
                            Some(msg) => {
                                if let Err(err) = origin_tx.send(msg.clone()).await {
                                    error!(err = err.to_string(), "could not send msg to origin_tx");
                                }
                                if msg.is_remove() {
                                    state
                                    .lock()
                                    .await
                                    .remove(&key);
                                    // stop listening and drop watcher
                                    return;
                                }
                            },
                            None => return,
                        }
                    }
                }
            }
        });

        Ok(())
    }
}

/// Creates single watcher and configures event handler for that.
fn create_watcher(
    path: &Path,
    tx: mpsc::Sender<RegistryMessage>,
) -> anyhow::Result<INotifyWatcher> {
    let path_clone = path.as_os_str().to_str().unwrap().to_string();
    let file_path = path.to_path_buf();

    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        if let Err(err) = match res {
            Ok(event) => {
                if !event.paths.contains(&file_path) {
                    return;
                }
                info!(
                    function = "create_watcher",
                    path = &path_clone,
                    event = ?event,
                    "received event"
                );

                let send_remove = || {
                    tx.blocking_send(RegistryMessage::Remove(path_clone.clone()))
                        .context("could not send remove event")
                };

                let send_modify = || {
                    tx.blocking_send(RegistryMessage::Parse {
                        file_path: path_clone.clone(),
                        new: false,
                    })
                    .context("could not send modify event")
                };

                // NOTICE: there will be multiple events for single file operation,
                // e.g. file deletion generated Modify(Metadata(_)), Access(_) and Remove(_) events
                match event.kind {
                    EventKind::Modify(modify_kind) => match modify_kind {
                        notify::event::ModifyKind::Metadata(_) => Ok(()),
                        // modify name is some kind of alias on file deletion
                        notify::event::ModifyKind::Name(_) => send_remove(),
                        // default modify is simple modification
                        _ => send_modify(),
                    },
                    EventKind::Remove(_) => send_remove(),
                    _ => Ok(()),
                }
            }
            Err(e) => Err(anyhow::format_err!("{}", e.to_string())),
        } {
            error!(err = err.to_string(), "watcher error occurred")
        }
    })?;

    watcher
        .watch(path.parent().unwrap(), RecursiveMode::Recursive)
        .with_context(|| format!("could not watch {:?} file", path))?;

    Ok(watcher)
}

#[cfg(test)]
mod tests {
    use crate::{
        registry,
        watcher::{create_watcher, WatcherManager},
    };
    use std::{fs::File, io::Write, time::Duration};
    use tempdir::TempDir;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_create_watcher() -> anyhow::Result<()> {
        let tmp_dir = TempDir::new("example")?;
        let file_path = tmp_dir.path().join("foo.rs");
        let mut file = File::create(&file_path)?;
        file.write_all("fn foo() {}".as_bytes())?;

        let (tx, mut rx) = mpsc::channel(10);
        let c_tx = tx.clone();

        let watcher = create_watcher(&file_path, tx)?;

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

    #[tokio::test]
    async fn test_watcher_manager() -> anyhow::Result<()> {
        let tmp_dir = TempDir::new("example")?;
        let file_path = tmp_dir.path().join("foo.rs");
        let mut file = File::create(&file_path)?;
        file.write_all("fn foo() {}".as_bytes())?;

        let (tx, mut rx) = mpsc::channel(10);
        let c_tx = tx.clone();

        let manager = WatcherManager::default();
        manager.watcher_proxy(&file_path, tx).await?;
        assert_eq!(manager.state.lock().await.len(), 1);

        tokio::time::sleep(Duration::new(0, 500)).await;
        std::fs::remove_file(&file_path)?;

        let msg = rx.recv().await.unwrap();
        assert_eq!(
            msg,
            registry::RegistryMessage::Remove(file_path.to_str().unwrap().to_string())
        );

        assert_eq!(manager.state.lock().await.len(), 0);

        drop(c_tx);
        Ok(())
    }

    #[tokio::test]
    async fn test_watcher_manager_modify() -> anyhow::Result<()> {
        let tmp_dir = TempDir::new("example")?;
        let file_path = tmp_dir.path().join("foo.rs");
        let mut file = File::create(&file_path)?;
        file.write_all("fn foo() {}".as_bytes())?;

        let (tx, mut rx) = mpsc::channel(10);
        let c_tx = tx.clone();

        let manager = WatcherManager::default();
        manager.watcher_proxy(&file_path, tx).await?;
        assert_eq!(manager.state.lock().await.len(), 1);

        tokio::time::sleep(Duration::new(0, 500)).await;
        file.write_all("fn boo() {}".as_bytes())?;

        let msg = rx.recv().await.unwrap();
        assert_eq!(
            msg,
            registry::RegistryMessage::Parse {
                file_path: file_path.to_str().unwrap().to_string(),
                new: false
            }
        );

        assert_eq!(manager.state.lock().await.len(), 1);

        drop(c_tx);
        Ok(())
    }
}
