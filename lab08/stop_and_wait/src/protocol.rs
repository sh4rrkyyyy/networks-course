use rand::RngExt;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

pub const HEADER_LEN: usize = 2;
pub const MAX_DATAGRAM: usize = 65_507;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacketType {
    Data,
    Ack,
    End,
}

impl PacketType {
    fn to_u8(self) -> u8 {
        match self {
            PacketType::Data => 1,
            PacketType::Ack => 2,
            PacketType::End => 3,
        }
    }

    fn from_u8(value: u8) -> Result<Self, String> {
        match value {
            1 => Ok(PacketType::Data),
            2 => Ok(PacketType::Ack),
            3 => Ok(PacketType::End),
            _ => Err(format!("unknown packet type: {value}")),
        }
    }
}

impl fmt::Display for PacketType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PacketType::Data => write!(f, "DATA"),
            PacketType::Ack => write!(f, "ACK"),
            PacketType::End => write!(f, "END"),
        }
    }
}

#[derive(Debug)]
pub struct Frame {
    pub packet_type: PacketType,
    pub seq: u8,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn to_bytes(&self) -> io::Result<Vec<u8>> {
        if self.seq > 1 {
            return Err(invalid_input("seq must be 0 or 1"));
        }

        let mut bytes = Vec::with_capacity(HEADER_LEN + self.payload.len());

        bytes.push(self.packet_type.to_u8());
        bytes.push(self.seq);
        bytes.extend_from_slice(&self.payload);

        Ok(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < HEADER_LEN {
            return Err("packet is too short".to_string());
        }

        let packet_type = PacketType::from_u8(bytes[0])?;
        let seq = bytes[1];

        if seq > 1 {
            return Err("bad sequence number".to_string());
        }

        Ok(Frame {
            packet_type,
            seq,
            payload: bytes[HEADER_LEN..].to_vec(),
        })
    }
}

pub fn send_with_loss(
    socket: &UdpSocket,
    bytes: &[u8],
    to: SocketAddr,
    loss: i32,
    description: &str,
) -> io::Result<()> {
    let mut rng = rand::rng();

    if rng.random_range(0..100) < loss {
        println!("[LOSS] {description}: packet dropped");
        return Ok(());
    }

    socket.send_to(bytes, to)?;
    Ok(())
}

pub fn send_file(
    socket: &UdpSocket,
    to: SocketAddr,
    input_file: &str,
    chunk_size: usize,
    timeout: Duration,
    loss: i32,
    label: &str,
) -> Result<(), Box<dyn Error>> {
    if chunk_size == 0 {
        return Err(invalid_input("chunk_size must be greater than 0").into());
    }

    if chunk_size + HEADER_LEN > MAX_DATAGRAM {
        return Err(invalid_input("chunk_size is too large").into());
    }

    let data = fs::read(input_file)?;
    let mut seq = 0u8;
    let mut offset = 0usize;

    while offset < data.len() {
        let end = (offset + chunk_size).min(data.len());

        let frame = Frame {
            packet_type: PacketType::Data,
            seq,
            payload: data[offset..end].to_vec(),
        };

        send_wait_ack(socket, to, frame, timeout, loss, label)?;

        offset = end;
        seq ^= 1;
    }

    let end_frame = Frame {
        packet_type: PacketType::End,
        seq,
        payload: Vec::new(),
    };

    send_wait_ack(socket, to, end_frame, timeout, loss, label)?;

    println!("[{label}] done, sent {} bytes", data.len());
    Ok(())
}

fn send_wait_ack(
    socket: &UdpSocket,
    to: SocketAddr,
    frame: Frame,
    timeout: Duration,
    loss: i32,
    label: &str,
) -> Result<(), Box<dyn Error>> {
    let bytes = frame.to_bytes()?;
    let mut buffer = vec![0u8; MAX_DATAGRAM];
    let mut attempts = 0;

    socket.set_read_timeout(Some(timeout))?;

    loop {
        attempts += 1;

        println!(
            "[{label}] send {} seq={} attempt={}",
            frame.packet_type, frame.seq, attempts
        );

        send_with_loss(socket, &bytes, to, loss, "send")?;

        match socket.recv_from(&mut buffer) {
            Ok((n, src)) => {
                if src != to {
                    println!("[{label}] ignored packet from {src}");
                    continue;
                }

                let ack = match Frame::from_bytes(&buffer[..n]) {
                    Ok(frame) => frame,
                    Err(err) => {
                        println!("[{label}] bad packet ignored: {err}");
                        continue;
                    }
                };

                if ack.packet_type == PacketType::Ack && ack.seq == frame.seq {
                    println!("[{label}] received ACK seq={}", ack.seq);
                    return Ok(());
                }

                println!(
                    "[{label}] unexpected packet: {} seq={}",
                    ack.packet_type, ack.seq
                );
            }

            Err(err)
                if err.kind() == io::ErrorKind::WouldBlock
                    || err.kind() == io::ErrorKind::TimedOut =>
            {
                println!(
                    "[{label}] timeout waiting for ACK seq={}, retransmit",
                    frame.seq
                );
            }

            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}

            Err(err) => return Err(err.into()),
        }
    }
}

pub fn recv_file(
    socket: &UdpSocket,
    output_file: &str,
    timeout: Duration,
    loss: i32,
    label: &str,
) -> Result<SocketAddr, Box<dyn Error>> {
    socket.set_read_timeout(None)?;

    let mut result = Vec::new();
    let mut buffer = vec![0u8; MAX_DATAGRAM];

    let mut expected_seq = 0u8;
    let mut peer: Option<SocketAddr> = None;

    loop {
        let (n, src) = socket.recv_from(&mut buffer)?;

        if peer.is_none() {
            peer = Some(src);
        }

        if Some(src) != peer {
            println!("[{label}] ignored packet from {src}");
            continue;
        }

        let frame = match Frame::from_bytes(&buffer[..n]) {
            Ok(frame) => frame,
            Err(err) => {
                println!("[{label}] bad packet ignored: {err}");
                continue;
            }
        };

        match frame.packet_type {
            PacketType::Data => {
                if frame.seq == expected_seq {
                    result.extend_from_slice(&frame.payload);

                    println!(
                        "[{label}] received DATA seq={} bytes={}",
                        frame.seq,
                        frame.payload.len()
                    );

                    expected_seq ^= 1;
                } else {
                    println!("[{label}] duplicate DATA seq={} ignored", frame.seq);
                }

                send_ack(socket, src, frame.seq, loss)?;
            }

            PacketType::End => {
                println!("[{label}] received END seq={}", frame.seq);

                send_ack(socket, src, frame.seq, loss)?;
                fs::write(output_file, &result)?;

                wait_duplicate_end(socket, src, frame.seq, timeout, loss, label)?;

                println!("[{label}] done, received {} bytes", result.len());
                println!("[{label}] saved file: {output_file}");

                return Ok(src);
            }

            PacketType::Ack => {
                println!("[{label}] unexpected ACK ignored");
            }
        }
    }
}

fn send_ack(socket: &UdpSocket, to: SocketAddr, seq: u8, loss: i32) -> Result<(), Box<dyn Error>> {
    let ack = Frame {
        packet_type: PacketType::Ack,
        seq,
        payload: Vec::new(),
    };

    send_with_loss(socket, &ack.to_bytes()?, to, loss, "ACK")?;
    Ok(())
}

fn wait_duplicate_end(
    socket: &UdpSocket,
    peer: SocketAddr,
    end_seq: u8,
    timeout: Duration,
    loss: i32,
    label: &str,
) -> Result<(), Box<dyn Error>> {
    socket.set_read_timeout(Some(timeout))?;

    let mut buffer = vec![0u8; MAX_DATAGRAM];

    for _ in 0..3 {
        match socket.recv_from(&mut buffer) {
            Ok((n, src)) if src == peer => {
                let frame = match Frame::from_bytes(&buffer[..n]) {
                    Ok(frame) => frame,
                    Err(_) => continue,
                };

                if frame.packet_type == PacketType::End && frame.seq == end_seq {
                    println!(
                        "[{label}] duplicate END seq={} received, resend ACK",
                        end_seq
                    );
                    send_ack(socket, peer, end_seq, loss)?;
                }
            }

            Ok(_) => {}

            Err(err)
                if err.kind() == io::ErrorKind::WouldBlock
                    || err.kind() == io::ErrorKind::TimedOut => {}

            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}

            Err(err) => return Err(err.into()),
        }
    }

    Ok(())
}

pub fn invalid_input(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

pub fn validate_loss(loss: i32) -> io::Result<i32> {
    if !(0..=100).contains(&loss) {
        return Err(invalid_input("loss must be in range [0, 100]"));
    }

    Ok(loss)
}

pub fn validate_timeout(timeout_ms: u64) -> io::Result<Duration> {
    if timeout_ms == 0 {
        return Err(invalid_input("timeout_ms must be greater than 0"));
    }

    Ok(Duration::from_millis(timeout_ms))
}
