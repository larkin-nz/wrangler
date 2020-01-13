use std::io;
use std::thread;
use std::time::SystemTime;

use chrome_devtools::events::DevtoolsEvent;

use console::style;

use futures_old::sync::mpsc;
use futures_old::{Future, Sink, Stream};

use tokio_tungstenite::connect_async;
use tungstenite::protocol::Message;

use tokio_old;

use url::Url;

const KEEP_ALIVE_INTERVAL: u64 = 10;

pub fn listen(session_id: String) -> Result<(), failure::Error> {
    let socket_url = format!("wss://rawhttp.cloudflareworkers.com/inspect/{}", session_id);
    let socket_url = Url::parse(&socket_url)?;

    let (keep_alive_tx, keep_alive_rx) = mpsc::channel(0);
    thread::spawn(|| keep_alive(keep_alive_tx));
    let keep_alive_rx = keep_alive_rx.map_err(|_| panic!());

    let client = connect_async(socket_url)
        .and_then(move |(ws_stream, _)| {
            let (sink, stream) = ws_stream.split();

            let enable_runtime = r#"{
          "id": 1,
          "method": "Runtime.enable"
        }"#;
            // sink.send(Message::Text(enable_runtime.into())).wait();

            let send_keep_alive = keep_alive_rx.forward(sink);
            let write_messages = stream.for_each(move |message| {
                let message = message.into_text().unwrap();
                log::info!("{}", message);
                let message: Result<DevtoolsEvent, serde_json::Error> =
                    serde_json::from_str(&message);
                match message {
                    Ok(message) => match message {
                        DevtoolsEvent::ConsoleAPICalled(event) => match event.log_type.as_str() {
                            "log" => println!("{}", style(event).blue()),
                            "error" => eprintln!("{}", style(event).red()),
                            _ => println!("unknown console event: {}", event),
                        },
                        DevtoolsEvent::ExceptionThrown(event) => {
                            eprintln!("{}", style(event).bold().red())
                        }
                    },
                    Err(e) => {
                        // this event was not parsed as a DevtoolsEvent
                        // TODO: change this to a warn after chrome-devtools-rs is parsing all messages
                        log::info!("this event was not parsed as a DevtoolsEvent:\n{}", e);
                    }
                };
                Ok(())
            });

            send_keep_alive
                .map(|_| ())
                .select(write_messages.map(|_| ()))
                .then(|_| Ok(()))
        })
        .map_err(|e| {
            println!("Error occurred during websocket: {}", e);
            io::Error::new(io::ErrorKind::Other, e)
        });

    tokio_old::runtime::run(client.map_err(|_| ()));
    Ok(())
}

fn keep_alive(mut tx: mpsc::Sender<Message>) {
    let mut keep_alive_time = SystemTime::now();
    let mut id = 2;
    loop {
        let elapsed = keep_alive_time.elapsed().unwrap().as_secs();
        if elapsed >= KEEP_ALIVE_INTERVAL {
            let keep_alive_message = format!(
                r#"{{
                "id": {},
                "method": "Runtime.getIsolateId"
            }}"#,
                id
            );
            tx = tx
                .send(Message::Text(keep_alive_message.into()))
                .wait()
                .unwrap();
            id += 1;
            keep_alive_time = SystemTime::now();
        }
    }
}
