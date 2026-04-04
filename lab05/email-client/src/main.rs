use clap::Parser;
use lettre::{
    Message, SmtpTransport, Transport, message::Mailbox,
    transport::smtp::authentication::Credentials,
};

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

fn main() {
    let args = Args::parse();
    let from: Mailbox = args.from.parse().unwrap();
    let to: Mailbox = args.to.parse().unwrap();
    let content_type = match args.fmt.as_str() {
        "html" => "text/html",
        "txt" => "text/plain",
        _ => {
            eprintln!("Unsupported format: {}", args.fmt);
            std::process::exit(1);
        }
    };

    let email = Message::builder()
        .from(from)
        .to(to.to_owned())
        .subject(args.subject)
        .header(lettre::message::header::ContentType::parse(content_type).unwrap())
        .body(args.body)
        .unwrap();

    let creds = Credentials::new(args.username, args.password);
    
    let mailer = SmtpTransport::starttls_relay(&args.host)
        .unwrap()
        .port(args.port)
        .credentials(creds)
        .build();
    
    match mailer.send(&email) {
        Ok(_) => println!("Message sent successfully to {}", to),
        Err(e) => eprintln!("Failed to send message: {}", e),
    }
}
