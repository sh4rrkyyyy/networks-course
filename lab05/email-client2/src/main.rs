use base64::{Engine, engine::general_purpose::STANDARD as B64};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
};

use clap::Parser;
use native_tls::TlsConnector;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    from: String,
    #[arg(long)]
    to: String,
    #[arg(long)]
    subject: String,
    #[arg(long)]
    body: String,
    #[arg(long)]
    fmt: String,
    #[arg(long)]
    host: String,
    #[arg(long)]
    port: u16,
    #[arg(long)]
    username: String,
    #[arg(long)]
    password: String,
}

fn read_rsp(reader: &mut impl BufRead) -> String {
    let mut result = String::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        result.push_str(&line);
        if !(line.len() > 3 && line.as_bytes()[3] == b'-') {
            break;
        }
    }
    result
}
fn send_and_process<S: Read + Write>(stream: &mut BufReader<S>, cmd: &str, exp: &str) {
    stream
        .get_mut()
        .write_all(format!("{}\r\n", cmd).as_bytes())
        .unwrap();
    stream.get_mut().flush().unwrap();
    let rsp = read_rsp(stream);
    if !rsp.starts_with(exp) {
        eprintln!("Expected {}, got: {}", exp, rsp.trim());
        std::process::exit(1);
    }
}

fn main() {
    let args = Args::parse();
    let content_type = match args.fmt.as_str() {
        "html" => "text/html",
        "txt" => "text/plain",
        _ => {
            eprintln!("Unsupported format: {}", args.fmt);
            std::process::exit(1);
        }
    };
    let addr = format!("{}:{}", args.host, args.port);
    let stream = TcpStream::connect(&addr).unwrap_or_else(|e| {
        eprintln!("Can not connect to {}: {}", addr, e);
        std::process::exit(1);
    });
    let mut stream = BufReader::new(stream);
    let rsp = read_rsp(&mut stream);
    if !rsp.starts_with("220") {
        eprintln!("Expected 220 code, got {}", rsp.trim());
        std::process::exit(1);
    }
    send_and_process(&mut stream, &format!("EHLO {}", args.host), "250");
    send_and_process(&mut stream, "STARTTLS", "220");
    let tls = TlsConnector::new()
        .unwrap()
        .connect(&args.host, stream.into_inner())
        .unwrap_or_else(|e| {
            eprintln!("TLS error: {}", e);
            std::process::exit(1);
        });

    let mut stream = BufReader::new(tls);
    send_and_process(&mut stream, &format!("EHLO {}", args.host), "250");

    send_and_process(&mut stream, "AUTH LOGIN", "334");
    send_and_process(&mut stream, &B64.encode(&args.username), "334");
    send_and_process(&mut stream, &B64.encode(&args.password), "235");
    send_and_process(&mut stream, &format!("MAIL FROM:<{}>", args.from), "250");
    send_and_process(&mut stream, &format!("RCPT TO:<{}>", args.to), "250");
    send_and_process(&mut stream, "DATA", "354");
    let message = format!(
        "From: {}\r\nTo: {}\r\nSubject: {}\r\nMIME-Version: 1.0\r\nContent-Type: {}\r\n\r\n{}\r\n.\r\n",
        args.from, args.to, args.subject, content_type, args.body
    );
    stream.get_mut().write_all(message.as_bytes()).unwrap();
    stream.get_mut().flush().unwrap();
    let resp = read_rsp(&mut stream);
    if !resp.starts_with("250") {
        eprintln!("Expected 250 code, got {}", rsp.trim());
        std::process::exit(1);
    }
    send_and_process(&mut stream, "QUIT", "221");
}
