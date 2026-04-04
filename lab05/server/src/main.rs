use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    process::{Command, Stdio},
};

fn handle(mut stream: TcpStream) {
    let mut buf_reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    buf_reader.read_line(&mut line).unwrap();
    let mut parts = line.trim().split_whitespace();
    let prog = match parts.next() {
        Some(p) => p,
        None => return,
    };

    let args: Vec<&str> = parts.collect();
    let out = Command::new(prog)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    let rsp = match out {
        Ok(out) => {
            let mut res = String::new();
            res.push_str(&String::from_utf8_lossy(&out.stdout));
            res.push_str(&String::from_utf8_lossy(&out.stderr));
            if res.is_empty() {
                format!("{}\n", out.status)
            } else {
                format!("{}\n{}\n", res.trim_end(), out.status)
            }
        }
        Err(e) => {
            format!("Error: {}\n", e)
        }
    };
    stream.write_all(rsp.as_bytes()).unwrap();
    stream.write_all(b"!end!\n").unwrap();
    stream.flush().unwrap();
}

fn main() {
    let addr = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: client <addr> <command>");
        std::process::exit(1);
    });
    let listener = TcpListener::bind(addr).unwrap();
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                std::thread::spawn(|| handle(s));
            }
            Err(e) => eprintln!("Accept error: {}", e),
        }
    }
}
