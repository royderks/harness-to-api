use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::TcpStream,
    time::Sleep,
};
use tokio_util::sync::CancellationToken;

pub(super) struct Socket {
    pub stream: TcpStream,
    pub canceled: CancellationToken,
    pub write_wait: Option<Pin<Box<Sleep>>>,
}
impl AsyncRead for Socket {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        let result = Pin::new(&mut self.stream).poll_read(cx, buf);
        if matches!(result, Poll::Ready(Err(_)))
            || matches!(result, Poll::Ready(Ok(()))) && buf.filled().len() == before
        {
            self.canceled.cancel();
        }
        result
    }
}
impl Socket {
    fn deadline(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<usize>> {
        use std::future::Future;
        let wait = self
            .write_wait
            .get_or_insert_with(|| Box::pin(tokio::time::sleep(Duration::from_secs(10))));
        if wait.as_mut().poll(cx).is_ready() {
            self.canceled.cancel();
            Poll::Ready(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "response write timeout",
            )))
        } else {
            Poll::Pending
        }
    }
}
impl AsyncWrite for Socket {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if let Poll::Ready(error) = self.deadline(cx) {
            return Poll::Ready(error);
        }
        match Pin::new(&mut self.stream).poll_write(cx, buf) {
            Poll::Pending => Poll::Pending,
            result => {
                if matches!(result, Poll::Ready(Err(_))) {
                    self.canceled.cancel();
                }
                result
            }
        }
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let result = Pin::new(&mut self.stream).poll_flush(cx);
        if result.is_ready() {
            self.write_wait = None;
        }
        result
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}
impl Drop for Socket {
    fn drop(&mut self) {
        self.canceled.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn blocked_response_write_expires_and_cancels_connection() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let writer = tokio::net::TcpSocket::new_v4().unwrap();
        writer.set_send_buffer_size(4096).unwrap();
        let stream = writer
            .connect(listener.local_addr().unwrap())
            .await
            .unwrap();
        let (_reader, _) = listener.accept().await.unwrap();
        let canceled = CancellationToken::new();
        let mut socket = Socket {
            stream,
            canceled: canceled.clone(),
            write_wait: None,
        };
        socket.write_all(b"headers").await.unwrap();
        // A partially delivered response shares one deadline through its final flush.
        let started = tokio::time::Instant::now();
        tokio::time::sleep(Duration::from_secs(2)).await;
        let bytes = vec![0; 1024 * 1024];
        let write = async {
            loop {
                if let Err(error) = socket.write_all(&bytes).await {
                    break Err::<(), _>(error);
                }
            }
        };
        let error = tokio::time::timeout(Duration::from_secs(10), write)
            .await
            .expect("write deadline must finish before the outer timeout")
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(canceled.is_cancelled());
        assert!(started.elapsed() < Duration::from_secs(12));
    }
}
