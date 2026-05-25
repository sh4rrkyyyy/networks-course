use rand::Rng;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;

const INF: u32 = 16;

#[derive(Deserialize)]
struct RouterConfig {
    ip: String,
    neighbours: Vec<String>,
}

#[derive(Debug, Clone)]
struct Entry {
    next_hop: String,
    metric: u32,
}

#[derive(Debug, Clone)]
struct Router {
    ip: String,
    neighbours: Vec<String>,
    table: HashMap<String, Entry>,
}

impl Router {
    fn new(ip: String, neighbours: Vec<String>) -> Self {
        let mut table = HashMap::new();
        for n in &neighbours {
            table.insert(
                n.clone(),
                Entry {
                    next_hop: n.clone(),
                    metric: 1,
                },
            );
        }

        Self {
            ip,
            neighbours,
            table,
        }
    }

    fn print_table(&self, header: &str) {
        println!("{}", header);
        println!(
            "{:<16} {:<16} {:<16} {}",
            "[Source IP]", "[Destination IP]", "[Next Hop]", "[Metric]"
        );

        let rows: Vec<_> = self.table.iter().collect();

        for (dst, e) in rows {
            println!(
                "{:<16} {:<16} {:<16} {}",
                self.ip, dst, e.next_hop, e.metric
            );
        }
        println!();
    }
}

fn generate_network(sz: usize) -> Vec<Router> {
    let mut rng = rand::thread_rng();

    let mut ips: Vec<String> = Vec::new();
    let mut set = std::collections::HashSet::new();
    while ips.len() < sz {
        let ip = format!(
            "{}.{}.{}.{}",
            rng.gen_range(1u8..=254),
            rng.gen_range(0u8..=254),
            rng.gen_range(0u8..=254),
            rng.gen_range(1u8..=254)
        );

        if set.insert(ip.clone()) {
            ips.push(ip);
        }
    }

    let mut edges: Vec<Vec<usize>> = vec![vec![]; sz];
    for i in 1..sz {
        let j = rng.gen_range(0..i);
        edges[i].push(j);
        edges[j].push(i);
    }

    ips.iter()
        .enumerate()
        .map(|(i, ip)| {
            Router::new(
                ip.clone(),
                edges[i].iter().map(|&j| ips[j].clone()).collect(),
            )
        })
        .collect()
}

fn load_network(path: &str) -> Result<Vec<Router>, Box<dyn std::error::Error>> {
    let vec: Vec<RouterConfig> = serde_json::from_str(&fs::read_to_string(path)?)?;
    Ok(vec
        .into_iter()
        .map(|rc| Router::new(rc.ip, rc.neighbours))
        .collect())
}

fn exec(routers: &mut Vec<Router>) {
    for step in 1..INF {
        for r in routers.iter() {
            r.print_table(&format!("Simulation step {} of router {}", step, r.ip));
        }
        let data: HashMap<String, HashMap<String, Entry>> = routers
            .iter()
            .map(|r| (r.ip.clone(), r.table.clone()))
            .collect();

        let mut changed = false;

        for r in routers.iter_mut() {
            for ip in &r.neighbours.clone() {
                let Some(neighbours) = data.get(ip) else {
                    continue;
                };
                for (dst, entry) in neighbours {
                    if *dst == r.ip {
                        continue;
                    }
                    let new_metric = entry.metric + 1;
                    if new_metric >= INF {
                        continue;
                    }
                    let upd = match r.table.get(dst) {
                        None => true,
                        Some(cur) => new_metric < cur.metric,
                    };
                    if upd {
                        r.table.insert(
                            dst.clone(),
                            Entry {
                                next_hop: ip.clone(),
                                metric: new_metric,
                            },
                        );
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            return;
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut routers: Vec<Router> = if args.len() > 1 {
        match load_network(&args[1]) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Error reading config: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        generate_network(4)
    };

    for r in &routers {
        println!("{} {:?}", r.ip, r.neighbours);
    }
    println!();

    exec(&mut routers);

    for router in &routers {
        router.print_table(&format!("Final state of router {} table:", router.ip));
    }
}
