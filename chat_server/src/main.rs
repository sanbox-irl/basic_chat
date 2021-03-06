use async_std::{
    io::BufReader,
    net::{TcpListener, TcpStream, ToSocketAddrs},
    prelude::*,
    task,
};
use chat_shared::Message;
use futures::{channel::mpsc, select, sink::SinkExt, FutureExt};
use std::collections::hash_map::{Entry, HashMap};
use std::sync::Arc;

type Sender<T> = mpsc::UnboundedSender<T>;
type Receiver<T> = mpsc::UnboundedReceiver<T>;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn main() -> Result<()> {
    task::block_on(accept_loop("192.168.1.19:1337"))
}

async fn accept_loop(addr: impl ToSocketAddrs) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;

    let (broker_sender, broker_receiver) = mpsc::unbounded();
    let broker_handle = task::spawn(broker_loop(broker_receiver));
    let mut incoming = listener.incoming();

    while let Some(stream) = incoming.next().await {
        let stream = stream?;
        println!("Accepting from: {}", stream.peer_addr()?);
        spawn_and_log_error(connection_loop(broker_sender.clone(), stream));
    }

    drop(broker_sender);
    broker_handle.await?;

    Ok(())
}

async fn connection_loop(mut broker: Sender<Event>, stream: TcpStream) -> Result<()> {
    let stream = Arc::new(stream);
    let reader = BufReader::new(&*stream);
    let mut lines = reader.lines();

    let name = match lines.next().await {
        None => Err("peer disconnected immediately")?,
        Some(line) => line?,
    };

    println!("{} has connected.", name);

    let (_shutdown_sender, shutdown_receiver) = mpsc::unbounded();

    broker
        .send(Event::NewPeer {
            name: name.clone(),
            stream: Arc::clone(&stream),
            shutdown: shutdown_receiver,
        })
        .await
        .unwrap();

    // Business Logic
    while let Some(line) = lines.next().await {
        let line = line?;

        match serde_json::from_str(&line) {
            Ok(message) => {
                println!("Got message! {:?}", message);

                broker
                    .send(Event::Message {
                        from: name.clone(),
                        message,
                    })
                    .await
                    .unwrap();
            }
            Err(e) => {
                println!("Error reading...{}", e);
            }
        }
    }

    Ok(())
}

async fn broker_loop(events: Receiver<Event>) -> Result<()> {
    let (disconnect_sender, mut disconnect_receiver) =
        mpsc::unbounded::<(String, Receiver<String>)>();
    let mut peers: HashMap<String, Sender<String>> = HashMap::new();
    let mut events = events.fuse();

    loop {
        let event = select! {
            event = events.next().fuse() => match event {
                None => break,
                Some(event) => event,
            },
            disconnect = disconnect_receiver.next().fuse() => {
                let (name, _pending_messages) = disconnect.unwrap();
                assert!(peers.remove(&name).is_some());
                continue;
            },
        };

        match event {
            Event::Message { from, message } => {
                for addr in message.targets {
                    if let Some(peer) = peers.get_mut(&addr) {
                        let msg = format!("from {}: {}\n", from, message.message);
                        peer.send(msg).await.unwrap();
                    }
                }
            }
            Event::NewPeer {
                name,
                stream,
                shutdown,
            } => match peers.entry(name.clone()) {
                Entry::Occupied(..) => (),
                Entry::Vacant(entry) => {
                    let (client_sender, mut client_receiver) = mpsc::unbounded();
                    entry.insert(client_sender);
                    let mut disconnected_sender = disconnect_sender.clone();

                    spawn_and_log_error(async move {
                        let res =
                            connection_writer_loop(&mut client_receiver, stream, shutdown).await;
                        disconnected_sender
                            .send((name, client_receiver))
                            .await // 4
                            .unwrap();
                        res
                    });
                }
            },
        }
    }
    drop(peers);
    drop(disconnect_sender);
    while let Some((_name, _pending_messages)) = disconnect_receiver.next().await {}

    Ok(())
}

async fn connection_writer_loop(
    messages: &mut Receiver<String>,
    stream: Arc<TcpStream>,
    shutdown: Receiver<()>,
) -> Result<()> {
    let mut stream = &*stream;
    let mut messages = messages.fuse();
    let mut shutdown = shutdown.fuse();

    loop {
        select! {
            msg = messages.next().fuse() => match msg {
                Some(msg) => stream.write_all(msg.as_bytes()).await?,
                None => break,
            },
            void = shutdown.next().fuse() => match void {
                Some(void) => (),
                None => break,
            }
        }
    }

    Ok(())
}

#[derive(Debug)]
enum Event {
    NewPeer {
        name: String,
        stream: Arc<TcpStream>,
        shutdown: Receiver<()>,
    },
    Message {
        from: String,
        message: Message,
    },
}

fn spawn_and_log_error<F>(fut: F) -> task::JoinHandle<()>
where
    F: Future<Output = Result<()>> + Send + 'static,
{
    task::spawn(async move {
        if let Err(e) = fut.await {
            eprintln!("{}", e)
        }
    })
}
