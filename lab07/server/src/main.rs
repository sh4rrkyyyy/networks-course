use rand::RngExt;
use std::net::UdpSocket;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket = UdpSocket::bind("127.0.0.1:8085").unwrap();

    let mut buf = [0u8; 1024];
    loop {
        let (sz, src_addr) = socket.recv_from(&mut buf)?;
        let recv = str::from_utf8(&buf[..sz]).unwrap();
        let mut rng = rand::rng();
        let drop = rng.random_range(0..100) < 20;
        if drop {
            println!("Drop packet from {}", src_addr);
            continue;
        }
        let rsp = recv.to_uppercase();
        socket.send_to(rsp.as_bytes(), src_addr)?;
        println!("Sent to {}: {}", src_addr, rsp);
    }
}
