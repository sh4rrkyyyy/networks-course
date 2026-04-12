use clap::{Parser, Subcommand};
use std::fs::File;
use std::io::Write;
use suppaftp::FtpStream;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    host: String,
    #[arg(long, default_value_t = 21)]
    port: u16,
    #[arg(long)]
    username: String,
    #[arg(long)]
    password: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    List {
        #[arg(long)]
        path: Option<String>,
    },
    Upload {
        #[arg(long)]
        local_file: String,
        #[arg(long)]
        remote_file: String,
    },
    Download {
        #[arg(long)]
        local_file: String,
        #[arg(long)]
        remote_file: String,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let mut ftp = FtpStream::connect(format!("{}:{}", args.host, args.port))?;
    ftp.login(&args.username, &args.password)?;
    ftp.transfer_type(suppaftp::types::FileType::Binary)?;

    match args.command {
        Command::List { path } => {
            let listing = ftp.list(path.as_deref())?;
            for entry in listing {
                match entry.parse::<suppaftp::list::File>() {
                    Ok(f) => {
                        let kind = if f.is_directory() { "DIR " } else { "FILE" };
                        println!("{} {}", kind, f.name());
                    }
                    Err(e) => println!("{}, {}", entry, e),
                }
            }
        }
        Command::Upload {
            local_file,
            remote_file,
        } => {
            let mut file = std::fs::File::open(&local_file)?;
            ftp.put_file(&remote_file, &mut file)?;
            println!(
                "File '{}' was successfully upload as '{}'",
                local_file, remote_file
            );
        }
        Command::Download {
            remote_file,
            local_file,
        } => {
            let cursor = ftp.retr_as_buffer(&remote_file)?;
            let mut file = File::create(&local_file)?;
            file.write_all(cursor.get_ref())?;
            println!(
                "File '{}' was download and save as '{}'",
                remote_file, local_file
            );
        }
    }

    ftp.quit()?;
    Ok(())
}
