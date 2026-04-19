use std::{
    net::UdpSocket,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server_addr = "127.0.0.1:8085";
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    socket.set_read_timeout(Some(Duration::from_secs(1)))?;
    let mut buf = [0u8; 1024];
    for i in 1..=10 {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
        let msg = format!("Ping {i} {now}");
        let st = Instant::now();
        socket.send_to(msg.as_bytes(), server_addr)?;
        match socket.recv_from(&mut buf) {
            Ok((sz, _)) => {
                let rtt = st.elapsed().as_secs_f64();
                let rsp = String::from_utf8_lossy(&buf[..sz]);
                println!("Server response: {rsp}");
                println!("RTT: {rtt:.6} seconds");
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut
                {
                    println!("Request timed out");
                } else {
                    println!("{e}");
                }
            }
        }
    }
    Ok(())
}
