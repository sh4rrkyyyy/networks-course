use std::env;
use std::error::Error;
use std::net::UdpSocket;

use stop_and_wait::protocol::{recv_file, send_file, validate_loss, validate_timeout};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!(
            "Usage: cargo run --bin server -- <bind_addr> <output_file> [chunk_size] [loss] [timeout_ms]"
        );
        std::process::exit(1);
    }

    let bind_addr = &args[1];
    let output_file = &args[2];

    let chunk_size: usize = args.get(3).map(|v| v.parse()).transpose()?.unwrap_or(1024);
    let loss: i32 = args.get(4).map(|v| v.parse()).transpose()?.unwrap_or(30);
    let timeout_ms: u64 = args.get(5).map(|v| v.parse()).transpose()?.unwrap_or(1000);

    let loss = validate_loss(loss)?;
    let timeout = validate_timeout(timeout_ms)?;

    let socket = UdpSocket::bind(bind_addr)?;

    println!("[SERVER] listening on {}", socket.local_addr()?);

    let client_addr = recv_file(&socket, output_file, timeout, loss, "SERVER")?;

    println!("[SERVER] now send received file back to client");

    send_file(
        &socket,
        client_addr,
        output_file,
        chunk_size,
        timeout,
        loss,
        "SERVER",
    )?;

    Ok(())
}
