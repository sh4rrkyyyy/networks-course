use std::{
    env,
    io::{Read, Write},
    net::TcpStream,
};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        eprintln!("Expected 4 args: <client.exe> server_host server_port filename");
        std::process::exit(1);
    }
    let host = &args[1];
    let file = &args[3];
    let port: u16 = args[2].parse().expect("Port shall have u16 format");
    let addr = format!("{host}:{port}");

    let mut stream = TcpStream::connect(&addr).unwrap_or_else(|e| {
        eprintln!("Can not connect to {}: {}", addr, e);
        std::process::exit(1);
    });
    let req = format!(
        "GET /{} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        file, host
    );
    stream.write_all(req.as_bytes()).unwrap();

    let mut rsp = String::new();
    stream.read_to_string(&mut rsp).unwrap();
    if let Some(split) = rsp.find("\r\n\r\n") {
        let headers = &rsp[..split];
        let body = &rsp[split + 4..];
        println!("headers: {headers}");
        println!("body: {body}");
    } else {
        println!("response: {rsp}");
    }
}
