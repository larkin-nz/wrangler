use std::time::SystemTime;

use chrome_devtools::events::DevtoolsEvent;

use console::style;

use futures::{future, pin_mut, StreamExt};
use futures_util::sink::SinkExt;

use tokio_tungstenite::connect_async;
use tungstenite::protocol::Message;

use url::Url;

const KEEP_ALIVE_INTERVAL: u64 = 10;

pub async fn listen(session_id: String) -> Result<(), failure::Error> {
    let socket_url = format!("wss://rawhttp.cloudflareworkers.com/inspect/{}", session_id);
    let socket_url = Url::parse(&socket_url)?;

    let (ws_stream, _) = connect_async(socket_url)
        .await
        .expect("Failed to connect to devtools instance");

    let (mut write, read) = ws_stream.split();

    let enable_runtime = r#"{
      "id": 1,
      "method": "Runtime.enable"
    }"#;
    write.send(Message::Text(enable_runtime.into())).await?;

    let (keepalive_tx, keepalive_rx) = futures::channel::mpsc::unbounded();
    tokio::spawn(keep_alive(keepalive_tx));
    let keepalive_to_ws = keepalive_rx.map(Ok).forward(write);

    let ws_to_stdout = {
        read.for_each(|msg| {
            async {
                let msg = msg.unwrap().into_text().unwrap();
                log::info!("{}", msg);
                let msg: Result<DevtoolsEvent, serde_json::Error> = serde_json::from_str(&msg);
                match msg {
                    Ok(msg) => match msg {
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
                }
            }
        })
    };
    pin_mut!(keepalive_to_ws, ws_to_stdout);
    future::select(keepalive_to_ws, ws_to_stdout).await;
    Ok(())
}

async fn keep_alive(tx: futures::channel::mpsc::UnboundedSender<Message>) {
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
            tx.unbounded_send(Message::Text(keep_alive_message.into()))
                .unwrap();
            id += 1;
            keep_alive_time = SystemTime::now();
        }
    }
}
