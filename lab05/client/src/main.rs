use std::{
    io::{BufRead, BufReader, Write},
    net::TcpStream,
};

fn main() {
    let addr = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: client <addr> <command>");
        std::process::exit(1);
    });

    let cmd = std::env::args().nth(2).unwrap_or_else(|| {
        eprintln!("Usage: client <addr> <command>");
        std::process::exit(1);
    });
    let mut stream = TcpStream::connect(&addr).unwrap_or_else(|e| {
        eprintln!("Can not connect to {}: {}", addr, e);
        std::process::exit(1);
    });
    let mut buf_reader = BufReader::new(stream.try_clone().unwrap());
    stream.write_all(format!("{}\n", cmd).as_bytes()).unwrap();
    stream.flush().unwrap();
    loop {
        let mut line = String::new();
        buf_reader.read_line(&mut line).unwrap();
        if line.trim() == "!end!" {
            break;
        }
        print!("{}", line);
    }
}
