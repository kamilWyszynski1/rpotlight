use anyhow::bail;
use tokio::sync::mpsc;

pub fn channel<T, R>(buffer: usize) -> (Sender<T, R>, Receiver<T, R>) {
    assert!(buffer > 0, "mpsc bounded channel requires buffer > 0");

    let (tx, rx) = mpsc::channel::<T>(buffer);
    let (reply_tx, reply_rx) = mpsc::channel::<R>(buffer);

    (Sender { tx, reply_rx }, Receiver { rx, reply_tx })
}

pub struct Sender<T, R> {
    pub tx: mpsc::Sender<T>,
    reply_rx: mpsc::Receiver<R>,
}

pub struct Receiver<T, R> {
    rx: mpsc::Receiver<T>,
    reply_tx: mpsc::Sender<R>,
}

impl<T, R> Sender<T, R> {
    pub async fn send(&mut self, value: T) -> anyhow::Result<Option<R>> {
        match self.tx.send(value).await {
            Ok(_) => {}
            Err(err) => bail!(err.to_string()),
        }
        Ok(self.reply_rx.recv().await)
    }
}

impl<T, R> Receiver<T, R> {
    pub async fn recv(&mut self) -> Option<T> {
        self.rx.recv().await
    }

    pub async fn response(&self, value: R) -> anyhow::Result<()> {
        self.reply_tx
            .send(value)
            .await
            .map_err(|err| anyhow::Error::msg(err.to_string()))
    }
}
