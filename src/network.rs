use futures::future::BoxFuture;
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, TcpStream};

pub const RESERVED_PORTS: [u16; 3] = [16728, 39561, 24329];

pub type ExpectResult<T> = Result<T, ExpectError>;
pub type ShutdownSender<T> = Arc<Mutex<Option<tokio::sync::oneshot::Sender<T>>>>;

pub trait ServeExpectHandler {
    type Output;

    fn expect_handler(
        stream: TcpStream,
        tx: ShutdownSender<Self::Output>,
    ) -> BoxFuture<'static, ExpectResult<()>>;
}

pub struct ExpectError;

pub struct Server {
    listener: Option<TcpListener>,
}

impl Server {
    pub fn new() -> Self {
        Server { listener: None }
    }

    pub async fn bind_local_listener(&mut self, port: u16) {
        self.listener = match TcpListener::bind(format!("127.0.0.1:{}", port)).await {
            Ok(l) => Some(l),
            Err(e) => panic!("{}", e),
        };
    }

    pub fn serve_with_result<T: Send + 'static>(
        self,
        handler_fn: impl Fn(TcpStream, ShutdownSender<T>) -> BoxFuture<'static, ExpectResult<T>>
            + Copy
            + Send
            + Sync
            + 'static,
    ) -> tokio::sync::oneshot::Receiver<T> where {
        let (tx_kill, rx_kill) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            let listener = self.listener.unwrap();
            let tx = Arc::new(Mutex::new(Some(tx_kill)));
            loop {
                let tx = tx.clone();
                if tx.lock().unwrap().is_none() {
                    break;
                }
                let (stream, _) = listener.accept().await.unwrap();
                let _ = handler_fn(stream, tx).await;
            }
        });

        rx_kill
    }
}
