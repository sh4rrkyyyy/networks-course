use std::env;
use std::error::Error;
use std::net::{SocketAddr, UdpSocket};

use stop_and_wait::protocol::{recv_file, send_file, validate_loss, validate_timeout};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 4 {
        eprintln!(
            "Usage: cargo run --bin client -- <server_addr> <input_file> <output_file> [chunk_size] [loss] [timeout_ms]"
        );
        std::process::exit(1);
    }

    let server_addr: SocketAddr = args[1].parse()?;
    let input_file = &args[2];
    let output_file = &args[3];

    let chunk_size: usize = args.get(4).map(|v| v.parse()).transpose()?.unwrap_or(1024);
    let loss: i32 = args.get(5).map(|v| v.parse()).transpose()?.unwrap_or(30);
    let timeout_ms: u64 = args.get(6).map(|v| v.parse()).transpose()?.unwrap_or(1000);

    let loss = validate_loss(loss)?;
    let timeout = validate_timeout(timeout_ms)?;

    let socket = UdpSocket::bind("0.0.0.0:0")?;

    send_file(
        &socket,
        server_addr,
        input_file,
        chunk_size,
        timeout,
        loss,
        "CLIENT",
    )?;

    println!("[CLIENT] now wait file from server");

    recv_file(&socket, output_file, timeout, loss, "CLIENT")?;

    Ok(())
}
