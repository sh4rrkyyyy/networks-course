use std::{
    env, fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    thread,
};

use log::info;
fn send_rsp(stream: &mut TcpStream, status: u16, status_text: &str, body: &[u8]) {
    let response = format!(
        "HTTP/1.1 {} {}\r\n\
            Content-Type: text/html; charset=utf-8\r\n\
            Content-Length: {}\r\n\
            Connection: close\r\n\
            \r\n",
        status,
        status_text,
        body.len()
    );
    stream.write_all(response.as_bytes()).ok();
    stream.write_all(body).ok();
    stream.flush().ok();
}
fn handle_client(mut stream: TcpStream) {
    let buf_reader = BufReader::new(&stream);
    let request = match buf_reader.lines().next() {
        Some(Ok(line)) => line,
        _ => {
            send_rsp(
                &mut stream,
                /* status */ 400,
                /* status_text */ "Bad Request",
                /* body */ b"Bad request\n",
            );
            return;
        }
    };
    let parts: Vec<&str> = request.split_whitespace().collect();
    if parts.len() < 2 || parts[0] != "GET" {
        send_rsp(
            &mut stream,
            /* status */ 400,
            /* status_text */ "Bad Request",
            /* body */ b"Bad request\n",
        );
        return;
    }
    let path = parts[1].trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match fs::read(&path) {
        Ok(data) => {
            send_rsp(
                &mut stream,
                /* status */ 200,
                /* status_text */ "OK",
                /* body */ &data,
            );
        }
        _ => {
            send_rsp(
                &mut stream,
                /* status */ 404,
                /* status_text */ "Not found",
                /* body */ b"Not found\n",
            );
        }
    }
}

fn main() {
    env_logger::init();
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Expected 2 args: <server.exe> server_port");
        std::process::exit(1);
    }
    let port: u16 = args[1].parse().expect("Port shall have u16 format");
    let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                thread::spawn(move || {
                    info!("[{:?}] handling {}", thread::current().id(), s.peer_addr().unwrap());
                    handle_client(s);
                });
            }
            Err(e) => eprintln!("Connection failed: {}", e),
        }
    }
}
