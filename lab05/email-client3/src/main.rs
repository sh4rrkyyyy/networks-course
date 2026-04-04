use base64::{Engine, engine::general_purpose::STANDARD as B64};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    path::PathBuf,
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
    #[arg(long)]
    image: Option<PathBuf>,
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
fn create_message(
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
    content_type: &str,
    attach: Option<&PathBuf>,
) -> String {
    let Some(path) = attach else {
        return format!(
            "From: {from}\r\nTo: {to}\r\nSubject: {subject}\r\n\
             MIME-Version: 1.0\r\nContent-Type: {content_type}; charset=utf-8\r\n\
             \r\n{body}\r\n"
        );
    };

    let boundary = "----";
    let filename = path.file_name().unwrap().to_str().unwrap();
    let data = std::fs::read(path).unwrap_or_else(|e| {
        eprintln!("Cannot read {}: {}", path.display(), e);
        std::process::exit(1);
    });
    let raw = B64.encode(&data);
    let mut encoded = String::new();
    for (i, ch) in raw.chars().enumerate() {
        if i > 0 && i % 76 == 0 {
            encoded.push_str("\r\n");
        }
        encoded.push(ch);
    }
    format!(
        "From: {from}\r\nTo: {to}\r\nSubject: {subject}\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: multipart/mixed; boundary=\"{boundary}\"\r\n\
         \r\n\
         --{boundary}\r\n\
         Content-Type: {content_type}; charset=utf-8\r\n\
         \r\n\
         {body}\r\n\
         --{boundary}\r\n\
         Content-Type: image/png; name=\"{filename}\"\r\n\
         Content-Transfer-Encoding: base64\r\n\
         Content-Disposition: attachment; filename=\"{filename}\"\r\n\
         \r\n\
         {encoded}\r\n\
         --{boundary}--\r\n"
    )
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
    let message = create_message(
        &args.from,
        &args.to,
        &args.subject,
        &args.body,
        content_type,
        args.image.as_ref(),
    );
    stream.get_mut().write_all(message.as_bytes()).unwrap();
    stream.get_mut().write_all(b"\r\n.\r\n").unwrap();
    stream.get_mut().flush().unwrap();
    let resp = read_rsp(&mut stream);
    if !resp.starts_with("250") {
        eprintln!("Expected 250 code, got {}", rsp.trim());
        std::process::exit(1);
    }
    send_and_process(&mut stream, "QUIT", "221");
}
