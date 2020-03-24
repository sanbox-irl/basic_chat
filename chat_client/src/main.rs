#![recursion_limit = "256"]

use async_std::{
    io::{stdin, BufReader},
    net::{TcpStream, ToSocketAddrs},
    prelude::*,
    task,
};
use chat_shared::Message;
use futures::{select, FutureExt};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn main() -> Result<()> {
    task::block_on(try_run("142.129.113.141:1337"))
}

async fn try_run(addr: impl ToSocketAddrs) -> Result<()> {
    let stream = TcpStream::connect(addr).await?;
    let (reader, mut writer) = (&stream, &stream);
    let mut lines_from_server = BufReader::new(reader).lines().fuse();
    {
        println!("Please type your name and then hit enter:");
        let mut name = String::new();
        stdin().read_line(&mut name).await?;
        writer.write_all(name.as_bytes()).await?;
    }

    let mut lines_from_stdin = BufReader::new(stdin()).lines().fuse();

    loop {
        select! {
            line = lines_from_server.next().fuse() => match line {
                Some(line) => {
                    let line = line?;
                    println!("{}", line);
                },
                None => break,
            },
            line = lines_from_stdin.next().fuse() => match line {
                Some(line) => {
                    let line = line?;
                    if let Some(message) = Message::easy_parse(&line) {
                        let message = serde_json::to_string(&message).unwrap();
                        writer.write_all(message.as_bytes()).await?;
                        writer.write_all(b"\n").await?;

                    } else {
                        println!("Error! We couldn't parse that input!");
                    }
                }
                None => break,
            }
        }
    }
    println!("Thanks for trying it out!");
    Ok(())
}
