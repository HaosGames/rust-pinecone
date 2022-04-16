use crate::frames::{Frame, SnekPacket};
use crate::router::PublicKey;
use std::io::{Error, ErrorKind};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{Receiver, Sender};
#[allow(unused)]
use crate::client::Client;

/// A session with a node in the network. This is being given out
/// by the `dial` method on [`Client`].
///
/// It is not guaranteed that sent data actually arrives at it's destination;
/// that is the node with the public key that this session was created for.
/// If the node doesn't exist in the network or routing of data fails due to another reason
/// the data that is being sent is dropped by the network.
/// This implements [`AsyncRead`]/[`AsyncWrite`].
pub struct Session {
    pub(crate) router_key: PublicKey,
    pub(crate) dialed_key: PublicKey,
    pub(crate) upload: Sender<Frame>,
    pub(crate) download: Receiver<Frame>,
}
impl AsyncRead for Session {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut().download.poll_recv(cx) {
            Poll::Ready(result) => match result {
                None => Poll::Ready(Err(Error::new(
                    ErrorKind::BrokenPipe,
                    "Session download channel closed",
                ))),
                Some(frame) => {
                    if let Frame::SnekRouted(packet) = frame {
                        if buf.remaining() < packet.payload.len() {
                            return Poll::Ready(Err(Error::new(
                                ErrorKind::OutOfMemory,
                                "Buffer not large enough",
                            )));
                        }
                        buf.put_slice(packet.payload.as_slice());
                    }
                    Poll::Ready(Ok(()))
                }
            },
            Poll::Pending => Poll::Pending,
        }
    }
}
impl AsyncWrite for Session {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, Error>> {
        let payload = Vec::from(buf);
        let frame = Frame::SnekRouted(SnekPacket {
            destination_key: self.dialed_key,
            source_key: self.router_key,
            payload,
        });
        match self.upload.try_send(frame) {
            Ok(_) => Poll::Ready(Ok(buf.len())),
            Err(e) => match e {
                TrySendError::Full(_) => Poll::Pending,
                TrySendError::Closed(_) => Poll::Ready(Err(Error::new(
                    ErrorKind::BrokenPipe,
                    "Session upload channel closed",
                ))),
            },
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Poll::Ready(Ok(()))
    }
}
