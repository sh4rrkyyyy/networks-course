use std::env;
use std::ffi::CStr;
use std::io;
use std::mem;
use std::net::{Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::process;
use std::time::Instant;

const ICMP_ECHO_REPLY: u8 = 0;
const ICMP_ECHO_REQUEST: u8 = 8;
const ICMP_TIME_EXCEEDED: u8 = 11;

const TIMEOUT: i64 = 5;

fn resolve_ipv4(host: &str) -> io::Result<Ipv4Addr> {
    let addrs = (host, 0).to_socket_addrs()?;

    for addr in addrs {
        if let SocketAddr::V4(v4) = addr {
            return Ok(*v4.ip());
        }
    }

    Err(io::Error::new(io::ErrorKind::Other, "IPv4 was not found"))
}

fn set_ttl(socket: libc::c_int, ttl: i32) {
    unsafe {
        libc::setsockopt(
            socket,
            libc::IPPROTO_IP,
            libc::IP_TTL,
            &ttl as *const _ as *const libc::c_void,
            mem::size_of_val(&ttl) as libc::socklen_t,
        );
    }
}

fn set_timeout(socket: libc::c_int, seconds: i64) {
    let timeout = libc::timeval {
        tv_sec: seconds,
        tv_usec: 0,
    };

    unsafe {
        libc::setsockopt(
            socket,
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            &timeout as *const _ as *const libc::c_void,
            mem::size_of_val(&timeout) as libc::socklen_t,
        );
    }
}

fn send_packet(
    socket: libc::c_int,
    dst_ip: Ipv4Addr,
    id: u16,
    seq: u16,
) -> Option<(Ipv4Addr, f64, u8)> {
    let packet = create_icmp_packet(id, seq);

    let dst = libc::sockaddr_in {
        sin_family: libc::AF_INET as libc::sa_family_t,
        sin_port: 0,
        sin_addr: libc::in_addr {
            s_addr: u32::from(dst_ip).to_be(),
        },
        sin_zero: [0; 8],
    };

    let start = Instant::now();

    let sent = unsafe {
        libc::sendto(
            socket,
            packet.as_ptr() as *const libc::c_void,
            packet.len(),
            0,
            &dst as *const _ as *const libc::sockaddr,
            mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
        )
    };

    if sent < 0 {
        return None;
    }

    let mut buffer = [0u8; 1024];

    let mut from: libc::sockaddr_in = unsafe { mem::zeroed() };
    let mut from_len = mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;

    let recv = unsafe {
        libc::recvfrom(
            socket,
            buffer.as_mut_ptr() as *mut libc::c_void,
            buffer.len(),
            0,
            &mut from as *mut _ as *mut libc::sockaddr,
            &mut from_len,
        )
    };

    if recv < 0 {
        return None;
    }

    let rtt = start.elapsed().as_secs_f64() * 1000.0;

    let from_ip = Ipv4Addr::from(from.sin_addr.s_addr.to_ne_bytes());

    let ip_header_len = ((buffer[0] & 0x0f) * 4) as usize;
    let icmp_type = buffer[ip_header_len];

    if icmp_type == ICMP_TIME_EXCEEDED || icmp_type == ICMP_ECHO_REPLY {
        Some((from_ip, rtt, icmp_type))
    } else {
        None
    }
}

fn create_icmp_packet(id: u16, seq: u16) -> Vec<u8> {
    let mut packet = vec![0u8; 8];

    packet[0] = ICMP_ECHO_REQUEST;
    packet[1] = 0;

    packet[4..6].copy_from_slice(&id.to_be_bytes());
    packet[6..8].copy_from_slice(&seq.to_be_bytes());

    let checksum = calculate_checksum(&packet);
    packet[2..4].copy_from_slice(&checksum.to_be_bytes());

    packet
}

fn calculate_checksum(data: &[u8]) -> u16 {
    let mut s = 0u32;
    let mut i = 0;

    while i + 1 < data.len() {
        let w = u16::from_be_bytes([data[i], data[i + 1]]) as u32;
        s += w;
        i += 2;
    }

    if i < data.len() {
        s += (data[i] as u32) << 8;
    }

    while (s >> 16) != 0 {
        s = (s & 0xffff) + (s >> 16);
    }

    !(s as u16)
}

fn get_hostname(ip: Ipv4Addr) -> String {
    let sockaddr = libc::sockaddr_in {
        sin_family: libc::AF_INET as libc::sa_family_t,
        sin_port: 0,
        sin_addr: libc::in_addr {
            s_addr: u32::from(ip).to_be(),
        },
        sin_zero: [0; 8],
    };

    let mut host = [0 as libc::c_char; 1024];

    let res = unsafe {
        libc::getnameinfo(
            &sockaddr as *const _ as *const libc::sockaddr,
            mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
            host.as_mut_ptr(),
            host.len() as libc::socklen_t,
            std::ptr::null_mut(),
            0,
            0,
        )
    };

    if res == 0 {
        unsafe { CStr::from_ptr(host.as_ptr()).to_string_lossy().into_owned() }
    } else {
        String::new()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: sudo cargo run <host> [packets count] [max ttl]");
        std::process::exit(1);
    }

    let host = &args[1];

    let packets_cnt = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(3);
    let max_ttl = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(30);

    let dst = resolve_ipv4(host)?;

    let socket = unsafe { libc::socket(libc::AF_INET, libc::SOCK_RAW, libc::IPPROTO_ICMP) };

    if socket < 0 {
        eprintln!("Can not open socket");
        std::process::exit(1);
    }

    set_timeout(socket, TIMEOUT);

    let pid = process::id() as u16;
    let mut seq: u16 = 0;

    for ttl in 1..=max_ttl {
        set_ttl(socket, ttl);

        print!("{:<3}", ttl);

        let mut reached = false;

        for _ in 0..packets_cnt {
            seq += 1;

            match send_packet(socket, dst, pid, seq) {
                Some((addr, rtt, icmp_type)) => {
                    let name = get_hostname(addr);

                    print!(" {} ({}) {:.3} ms", name, addr, rtt);

                    if icmp_type == ICMP_ECHO_REPLY {
                        reached = true;
                    }
                }
                None => {
                    print!(" *");
                }
            }
        }

        println!();

        if reached {
            break;
        }
    }

    unsafe {
        libc::close(socket);
    }

    Ok(())
}
