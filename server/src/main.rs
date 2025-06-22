use anyhow::Result;
use socket2::{Domain, Protocol, Socket, Type};
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;

async fn create_broadcast_socket() -> Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;

    socket.set_reuse_address(true)?;
    socket.set_broadcast(true)?;

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 42069);
    socket.bind(&addr.into())?;

    let std_socket: std::net::UdpSocket = socket.into();
    std_socket.set_nonblocking(true)?;

    Ok(UdpSocket::from_std(std_socket)?)
}

#[tokio::main]
async fn main() -> Result<()> {
    let socket = create_broadcast_socket().await?;
    let socket = Arc::new(socket);
    let clients = Arc::new(Mutex::new(HashSet::<SocketAddr>::new()));
    let mut buf = vec![0u8; 1024];

    loop {
        let (len, sender) = socket.recv_from(&mut buf).await?;
        let msg = &buf[..len];

        {
            let mut clients_guard = clients.lock().await;
            clients_guard.insert(sender);
        }

        let socket = socket.clone();
        let clients = clients.clone();
        let msg = msg.to_vec();
        tokio::spawn(async move {
            let clients_guard = clients.lock().await;
            for client in clients_guard.iter() {
                if *client != sender {
                    let _ = socket.send_to(&msg, client).await;
                }
            }
        });
    }
}