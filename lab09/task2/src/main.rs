use std::env;
use std::net::{IpAddr, SocketAddr, TcpListener};
use std::process::exit;

fn is_port_free(ip: IpAddr, port: u16) -> bool {
    let address = SocketAddr::new(ip, port);
    TcpListener::bind(address).is_ok()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() != 4 {
        eprintln!("Usage: <app> <ip-addr> <first-port> <last-port>");
        exit(1);
    }
    let ip: IpAddr = args[1].parse()?;
    let fst_port: u16 = args[2].parse()?;
    let lst_port: u16 = args[3].parse()?;

    for p in fst_port..=lst_port {
        if is_port_free(ip, p) {
            println!("{}", p);
        }
    }

    Ok(())
}
