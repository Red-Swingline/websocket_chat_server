mod message_store;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use warp::Filter;
use warp::ws::{Message, WebSocket};
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use futures_util::stream::StreamExt;
use serde_json::Value;
use uuid::Uuid;
use warp::http::StatusCode;
use warp::Reply;

type Clients = Arc<Mutex<Vec<Arc<Mutex<(Uuid, mpsc::UnboundedSender<Message>, String)>>>>>;
type RoomMap = Arc<Mutex<HashMap<String, Vec<Uuid>>>>;

#[derive(serde::Deserialize)]
struct NewRoom {
    name: String,
}

#[tokio::main]
async fn main() {
    let clients: Clients = Arc::new(Mutex::new(Vec::new()));
    let rooms: RoomMap = Arc::new(Mutex::new(HashMap::new()));
    let message_store = Arc::new(message_store::MessageStore::new().unwrap());

    let ws = warp::path("ws")
        .and(warp::ws())
        .and({
            let clients = clients.clone();
            let rooms = rooms.clone();
            let message_store = message_store.clone();
            warp::any().map(move || (clients.clone(), rooms.clone(), message_store.clone()))
        })
        .map(|ws: warp::ws::Ws, (clients, rooms, message_store)| {
            ws.on_upgrade(move |socket| handle_connection(socket, clients, message_store, rooms))
        });

    let get_rooms = {
        let message_store = message_store.clone();
        warp::path("rooms").map(move || {
            let rooms = message_store.get_rooms().unwrap_or_else(|_| Vec::new());
            warp::reply::json(&rooms)
        })
    };

    let add_room = {
        let message_store = message_store.clone();
        warp::path("add_room")
            .and(warp::post())
            .and(warp::body::json())
            .map(move |new_room: NewRoom| -> Box<dyn Reply> {
                let room_id = Uuid::new_v4().to_string();
                if message_store.add_room(room_id.clone(), new_room.name.clone()).is_ok() {
                    Box::new(warp::reply::with_status(
                        room_id,
                        StatusCode::CREATED,
                    ))
                } else {
                    Box::new(warp::reply::with_status(
                        "Failed to create room",
                        StatusCode::INTERNAL_SERVER_ERROR,
                    ))
                }
            })
    };

    let routes = get_rooms.or(ws).or(add_room);
    warp::serve(routes).run(([0, 0, 0, 0], 3030)).await;
}

async fn handle_connection(ws: WebSocket, clients: Clients, message_store: Arc<message_store::MessageStore>, rooms: RoomMap) {
    let (client_ws_tx, mut client_ws_rx) = ws.split();
    let (client_tx, client_rx) = mpsc::unbounded_channel();
    let client_rx = UnboundedReceiverStream::new(client_rx);
    let client_id = Uuid::new_v4();
    let mut room_id = String::new();

    tokio::task::spawn(
        client_rx
            .map(Ok)
            .forward(client_ws_tx)
    );

    let client_info = Arc::new(Mutex::new((client_id, client_tx, room_id.clone())));
    {
        let mut clients = clients.lock().unwrap();
        clients.push(client_info.clone());
        println!("Added new client. Number of connected clients: {}", clients.len());
    }

    while let Some(result) = client_ws_rx.next().await {
        let mut locked_rooms = rooms.lock().unwrap();
        match result {
            Ok(msg) => {
                if msg.is_close() {
                    break;
                }
                if let Ok(text) = msg.to_str() {
                    let parsed_msg: Result<Value, _> = serde_json::from_str(text);
                    println!("{:?}", parsed_msg);
                    if let Ok(parsed_msg) = parsed_msg {
                        room_id = parsed_msg["room_id"].as_str().unwrap_or_default().to_string();
                        {
                            let mut client_data = client_info.lock().unwrap();
                            client_data.2 = room_id.clone();
                        }

                        let room_name = parsed_msg["room_name"].as_str().unwrap_or_default().to_string();
                        let message_text = parsed_msg["text"].as_str().unwrap_or_default().to_string();

                        locked_rooms.entry(room_id.clone()).or_insert_with(Vec::new).push(client_id);
                        let _ = message_store.add_message(room_id.clone(), room_name.clone(), message_text.clone());

                        let room_clients = locked_rooms.get(&room_id).unwrap_or(&Vec::new()).clone();
                        eprintln!("Clients in room {}: {:?}", room_id, room_clients);

                        let clients = clients.lock().unwrap();
                        eprintln!("All connected clients: {:?}", clients);

                        for client_arc_mutex in clients.iter() {
                            let (other_client_id, client, other_room_id) = &*client_arc_mutex.lock().unwrap();
                            eprintln!("Checking client: {}, room: {}", other_client_id, other_room_id);
                            if room_clients.contains(other_client_id) && room_id == *other_room_id && client_id != *other_client_id {
                                eprintln!("Sending message to client: {}", other_client_id);
                                let _ = client.send(Message::text(message_text.clone()));
                            }
                        }
                    }
                }
            },
            Err(_) => break,
        }
    }

    {
        let mut clients = clients.lock().unwrap();
        clients.retain(|client_arc_mutex| {
            let (id, _, _) = &*client_arc_mutex.lock().unwrap();
            *id != client_id
        });
    }
}

