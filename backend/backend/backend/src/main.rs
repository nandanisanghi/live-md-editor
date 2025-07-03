use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::Extension,
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{SinkExt, StreamExt};
use std::{
    collections::HashSet,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tower_http::cors::{Any, CorsLayer};
use yrs::{Doc, Transact};

type SharedDoc = Arc<Mutex<Doc>>;
type Clients = Arc<Mutex<HashSet<UnboundedSender<Message>>>>;

#[tokio::main]
async fn main() {
    // Shared CRDT document
    let doc = Arc::new(Mutex::new(Doc::new()));

    // Active client list
    let clients: Clients = Arc::new(Mutex::new(HashSet::new()));

    // Build Axum app
    let app = Router::new()
        .route("/ws", get({
            let doc = doc.clone();
            let clients = clients.clone();
            move |ws| ws_handler(ws, doc, clients)
        }))
        .layer(CorsLayer::very_permissive());

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("🚀 Server running at ws://{}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

// WebSocket upgrade route handler
async fn ws_handler(
    ws: WebSocketUpgrade,
    doc: SharedDoc,
    clients: Clients,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, doc, clients))
}

// Handle individual WebSocket client session
async fn handle_socket(
    socket: WebSocket,
    doc: SharedDoc,
    clients: Clients,
) {
    let (mut sender, mut receiver) = socket.split();

    // Create message channel for broadcasting to this client
    let (tx, mut rx) = unbounded_channel();
    clients.lock().unwrap().insert(tx.clone());

    // Spawn task to send messages to client
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Receive messages from client
    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(txt) = msg {
            // Apply incoming text to shared doc (overwrite approach)
            let mut doc = doc.lock().unwrap();
            let mut txn = doc.transact_mut();
            let text_ref = doc.get_text("root");

            // Replace entire content for now (can be replaced with CRDT binary ops)
            text_ref.delete(&mut txn, 0, text_ref.len(&txn));
            text_ref.insert(&mut txn, 0, &txt);

            // Broadcast updated text to all connected clients
            for client in clients.lock().unwrap().iter() {
                let _ = client.send(Message::Text(txt.clone()));
            }
        }
    }

    // Cleanup on disconnect
    clients.lock().unwrap().remove(&tx);
    let _ = send_task.await;
}
