use core::marker::PhantomData;
use core::mem;
use core::task::Poll;
use futures::future::poll_fn;
use smoltcp::iface::{Context as SmolContext, SocketHandle};
use smoltcp::socket::UdpPacketMetadata as SyncUdpPacketMetadata;
use smoltcp::socket::UdpSocket as SyncUdpSocket;
use smoltcp::socket::UdpSocketBuffer;
use smoltcp::wire::IpEndpoint;

use super::stack::Stack;

pub type UdpPacketMetadata = SyncUdpPacketMetadata;

pub struct UdpSocket<'a> {
    pub handle: SocketHandle,
    ghost: PhantomData<&'a mut [u8]>,
}

impl<'a> UdpSocket<'a> {
    pub fn new(
        rx_buffer: &'a mut [u8],
        tx_buffer: &'a mut [u8],
        rx_metadata_buffer: &'a mut [UdpPacketMetadata],
        tx_metadata_buffer: &'a mut [UdpPacketMetadata],
    ) -> Self {
        let handle = Stack::with(|stack| {
            let rx_buffer: &'static mut [u8] = unsafe { mem::transmute(rx_buffer) };
            let tx_buffer: &'static mut [u8] = unsafe { mem::transmute(tx_buffer) };
            let rx_metadata_buffer: &'static mut [UdpPacketMetadata] =
                unsafe { mem::transmute(rx_metadata_buffer) };
            let tx_metadata_buffer: &'static mut [UdpPacketMetadata] =
                unsafe { mem::transmute(tx_metadata_buffer) };

            stack.iface.add_socket(SyncUdpSocket::new(
                UdpSocketBuffer::new(rx_metadata_buffer, rx_buffer),
                UdpSocketBuffer::new(tx_metadata_buffer, tx_buffer),
            ))
        });

        Self {
            handle,
            ghost: PhantomData,
        }
    }

    pub fn endpoint(&self) -> IpEndpoint {
        with_socket(self.handle, |s, _| s.endpoint())
    }

    pub fn hop_limit(&self) -> Option<u8> {
        with_socket(self.handle, |s, _| s.hop_limit())
    }

    pub fn set_hop_limit(&self, hop_limit: Option<u8>) {
        with_socket(self.handle, |s, _| s.set_hop_limit(hop_limit))
    }

    pub fn bind<T>(&mut self, endpoint: T) -> Result<(), smoltcp::Error>
    where
        T: Into<IpEndpoint>,
    {
        with_socket(self.handle, |s, _| s.bind(endpoint))
    }

    pub fn close(&mut self) {
        with_socket(self.handle, |s, _| s.close())
    }

    pub fn is_open(&self) -> bool {
        with_socket(self.handle, |s, _| s.is_open())
    }

    pub fn can_send(&self) -> bool {
        with_socket(self.handle, |s, _| s.can_send())
    }

    pub fn can_recv(&self) -> bool {
        with_socket(self.handle, |s, _| s.can_recv())
    }

    pub fn packet_recv_capacity(&self) -> usize {
        with_socket(self.handle, |s, _| s.packet_recv_capacity())
    }

    pub fn packet_send_capacity(&self) -> usize {
        with_socket(self.handle, |s, _| s.packet_send_capacity())
    }

    pub fn payload_recv_capacity(&self) -> usize {
        with_socket(self.handle, |s, _| s.payload_recv_capacity())
    }

    pub fn payload_send_capacity(&self) -> usize {
        with_socket(self.handle, |s, _| s.payload_send_capacity())
    }

    pub async fn recv_slice(&mut self, buf: &mut [u8]) -> (usize, IpEndpoint) {
        poll_fn(|cx| {
            with_socket(self.handle, |s, _| {
                if s.can_recv() {
                    match s.recv_slice(buf) {
                        Ok((n, ep)) => Poll::Ready((n, ep)),
                        Err(err) => {
                            s.register_recv_waker(cx.waker());
                            Poll::Pending
                        }
                    }
                } else {
                    s.register_recv_waker(cx.waker());
                    Poll::Pending
                }
            })
        })
        .await
    }

    pub async fn send_slice(
        &mut self,
        buf: &[u8],
        endpoint: IpEndpoint,
    ) -> Result<(), smoltcp::Error> {
        poll_fn(|cx| {
            with_socket(self.handle, |s, _| match s.send_slice(buf, endpoint) {
                Ok(()) => Poll::Ready(Ok(())),
                Err(smoltcp::Error::Exhausted) => {
                    s.register_send_waker(cx.waker());
                    Poll::Pending
                }
                Err(err) => Poll::Ready(Err(err)),
            })
        })
        .await
    }
}

fn with_socket<R>(
    handle: SocketHandle,
    f: impl FnOnce(&mut SyncUdpSocket, &mut SmolContext) -> R,
) -> R {
    Stack::with(|stack| {
        let res = {
            let (s, cx) = stack.iface.get_socket_and_context::<SyncUdpSocket>(handle);
            f(s, cx)
        };
        stack.wake();
        res
    })
}

impl<'a> Drop for UdpSocket<'a> {
    fn drop(&mut self) {
        Stack::with(|stack| {
            stack.iface.remove_socket(self.handle);
        })
    }
}
