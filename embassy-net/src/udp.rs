use core::marker::PhantomData;
use core::mem;
use smoltcp::iface::{Context as SmolContext, SocketHandle};
use smoltcp::socket::UdpSocket as SyncUdpSocket;
use smoltcp::socket::{UdpPacketMetadata, UdpSocketBuffer};

use super::stack::Stack;

pub type UdpMetadata = UdpPacketMetadata;

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
}

pub fn with_socket<R>(
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
