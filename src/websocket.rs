use futures::{SinkExt, StreamExt};
use serde::{Serialize, Deserialize};
use std::error::Error;
use std::time::Duration;
use tokio::{spawn, sync::mpsc, time};
use tokio_tungstenite::{connect_async, tungstenite::{client::{IntoClientRequest}, http::{HeaderValue, Request}, Message}};

use crate::command::{BotClient};
use crate::basics::variant::VariantManager;
use crate::console::DebugCommand;

/// Spawns two tasks:
///  1) Reads from the WebSocket and handles incoming messages.
///  2) Takes commands from the queue and sends them one-per-second.
///
/// Returns a channel for queuing commands.
async fn spawn_ws_client(req: Request<()>, mut debug_receiver: mpsc::UnboundedReceiver<DebugCommand>) -> Result<(), Box<dyn Error>> {
	let (ws_stream, _response) = connect_async(req).await?;
	println!("Established websocket connection!!");

	let (mut ws_write, mut ws_read) = ws_stream.split();
	let (sender, mut receiver) = mpsc::unbounded_channel::<String>();

	// Sender task
	spawn(async move {
		while let Some(cmd) = receiver.recv().await {
			if let Err(e) = ws_write.send(Message::Text(cmd.into())).await {
				eprintln!("Error sending over WebSocket: {:?}", e);
				break;
			}
			time::sleep(Duration::from_millis(500)).await;
		}
		// If we ever exit the loop (channel closed), close the socket
		let _ = ws_write.close().await;
	});

	// Receiver task
	spawn(async move {
		let variant_manager = VariantManager::new().await;
		let mut client = BotClient::new(sender, variant_manager);

		loop {
			tokio::select! {
				msg = ws_read.next() => {
					match msg {
						Some(Ok(Message::Text(text))) => {
							client.handle_msg(text.to_string());
						}
						Some(Ok(Message::Close(frame))) => {
							println!("[Server closed connection]: {:?}", frame);
							break;
						}
						Some(Err(e)) => {
							eprintln!("WebSocket error: {:?}", e);
							break;
						}
						None => {
							println!("WebSocket stream ended.");
							break;
						}
						_ => {}
					}
				}
				debug_cmd = debug_receiver.recv() => {
					match debug_cmd {
						Some(cmd) => {
							client.handle_debug_command(cmd);
						}
						None => {
							println!("Debug command channel closed.");
							break;
						}
					}
				}
			}
		}
		println!("Receiver task ending.");
	});

	Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct WsMessage<'a> {
	msg: &'a str,
	room: &'a str,
	recipient: &'a str
}

pub fn send_pm(outgoing: &mpsc::UnboundedSender<String>, recipient: &str, msg: &str) {
	let ws_msg = WsMessage { msg, recipient, room: "lobby" };
	outgoing.send(format!("chatPM {}", serde_json::to_string(&ws_msg).unwrap())).unwrap();
}

pub fn send_chat(outgoing: &mpsc::UnboundedSender<String>, table_id: &str, msg: &str) {
	let ws_msg = WsMessage { msg, recipient: "", room: &format!("table{}", table_id) };
	outgoing.send(format!("chat {}", serde_json::to_string(&ws_msg).unwrap())).unwrap();
}

pub fn send_cmd(outgoing: &mpsc::UnboundedSender<String>, command: &str, args: &str) {
	let cmd = format!("{command} {args}");
	println!("Sending command: {}", cmd);
	outgoing.send(cmd).unwrap();
}

pub async fn connect(cookie: &str, debug_receiver: mpsc::UnboundedReceiver<DebugCommand>) -> Result<(), Box<dyn Error>> {
	let mut req = "wss://hanab.live/ws".into_client_request()?;
	let headers = req.headers_mut();
	headers.insert("Cookie", HeaderValue::from_str(cookie).unwrap());

	spawn_ws_client(req, debug_receiver).await?;
	Ok(())
}
