use if_addrs::IfAddr;

fn main() {
    for iface in if_addrs::get_if_addrs().unwrap() {
        if iface.is_loopback() {
            continue;
        }
        match iface.addr {
            IfAddr::V4(addr) => {
                println!("interface: {}", iface.name);
                println!("ip address: {}", addr.ip);
                println!("mask: {}", addr.netmask);
                println!();
            }
            IfAddr::V6(addr) => {
                println!("interface: {}", iface.name);
                println!("ip address: {}", addr.ip);
                println!("mask: {}", addr.netmask);
                println!();
            }
        }
    }
}
