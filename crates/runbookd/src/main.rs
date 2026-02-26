use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::{ws::WebSocketUpgrade, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use futures::{SinkExt, StreamExt};
use tokio::sync::{broadcast, Mutex};
use tracing::{error, info, warn};

use runbook_protocol::{
    ClientKind, ClientToDaemon, DaemonToClient, HelloAck, HookEvent, Notice,
    PROTOCOL_VERSION,
};

mod config;
mod reducer;
mod render;
mod state;

use config::RunbookConfig;
use reducer::{ClientKindTag, Event, SideEffect};
use state::DaemonState;

#[derive(Debug, Parser)]
#[command(name = "runbookd", about = "Runbook daemon")]
struct Args {
    /// Path to runbook.yaml
    #[arg(long, default_value = "./runbook.yaml")]
    config: String,
}

#[derive(Clone)]
struct App {
    config: Arc<RunbookConfig>,
    state: Arc<Mutex<DaemonState>>,
    tx: broadcast::Sender<DaemonToClient>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    let config = load_config(&args.config)?;
    config.validate()?;

    let initial_page = config.keypad.initial_page;

    let (tx, _rx) = broadcast::channel::<DaemonToClient>(256);

    let app = App {
        config: Arc::new(config),
        state: Arc::new(Mutex::new(DaemonState::new(initial_page))),
        tx,
    };

    // Emit initial render.
    app.broadcast_render().await;

    let router = Router::new()
        .route("/ws", get(ws_handler))
        .route("/hook", post(hook_handler))
        .with_state(app.clone());

    let addr: SocketAddr = app
        .config
        .daemon
        .listen
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid daemon.listen: {e}"))?;

    info!(%addr, "runbookd listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}

fn load_config(path: &str) -> anyhow::Result<RunbookConfig> {
    let bytes =
        std::fs::read(path).map_err(|e| anyhow::anyhow!("failed to read config '{path}': {e}"))?;
    let cfg: RunbookConfig = serde_yaml::from_slice(&bytes)
        .map_err(|e| anyhow::anyhow!("failed to parse yaml '{path}': {e}"))?;
    Ok(cfg)
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

async fn ws_handler(ws: WebSocketUpgrade, State(app): State<App>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(app, socket))
}

async fn hook_handler(
    State(app): State<App>,
    Json(ev): Json<HookEvent>,
) -> impl IntoResponse {
    app.apply_event(Event::HookEvent {
        hook: ev.hook,
        matcher: ev.matcher,
        session_id: ev.session_id,
        session_tag: ev.session_tag,
    })
    .await;
    "ok"
}

// ---------------------------------------------------------------------------
// WebSocket connection handler
// ---------------------------------------------------------------------------

async fn handle_socket(app: App, socket: axum::extract::ws::WebSocket) {
    let (ws_tx, mut ws_rx) = socket.split();

    // Wrap the sender in an Arc<Mutex> so both tasks can use it.
    let ws_tx = Arc::new(Mutex::new(ws_tx));

    // Subscribe to daemon broadcast.
    let mut rx = app.tx.subscribe();

    // Send hello proactively.
    {
        let mut tx = ws_tx.lock().await;
        let _ = tx
            .send(axum::extract::ws::Message::Text(
                serde_json::to_string(&DaemonToClient::Hello(HelloAck {
                    protocol: PROTOCOL_VERSION,
                    daemon_version: env!("CARGO_PKG_VERSION").to_string(),
                }))
                .unwrap(),
            ))
            .await;
    }

    // Track which client kind this is for disconnect handling.
    let client_kind: Arc<Mutex<Option<ClientKindTag>>> = Arc::new(Mutex::new(None));

    // Task: forward broadcast → websocket
    let forward = {
        let ws_tx_fwd = Arc::clone(&ws_tx);
        tokio::spawn(async move {
            while let Ok(msg) = rx.recv().await {
                let text = match serde_json::to_string(&msg) {
                    Ok(t) => t,
                    Err(e) => {
                        error!("failed to serialize daemon msg: {e}");
                        continue;
                    }
                };
                let mut tx = ws_tx_fwd.lock().await;
                if tx
                    .send(axum::extract::ws::Message::Text(text))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        })
    };

    // Receive loop
    while let Some(Ok(msg)) = ws_rx.next().await {
        match msg {
            axum::extract::ws::Message::Text(ref text) => {
                match serde_json::from_str::<ClientToDaemon>(text) {
                    Ok(parsed) => {
                        app.handle_client_message(parsed, &client_kind).await;
                    }
                    Err(e) => {
                        error!("invalid json from client: {e}; text={text}");
                    }
                }
            }
            axum::extract::ws::Message::Close(_) => break,
            _ => {}
        }
    }

    forward.abort();

    // Handle disconnect.
    let kind = client_kind.lock().await.take();
    if let Some(k) = kind {
        app.apply_event(Event::ClientDisconnected { kind: k }).await;
    }
}

// ---------------------------------------------------------------------------
// App methods
// ---------------------------------------------------------------------------

impl App {
    async fn handle_client_message(
        &self,
        msg: ClientToDaemon,
        client_kind: &Arc<Mutex<Option<ClientKindTag>>>,
    ) {
        match msg {
            ClientToDaemon::Hello(hello) => {
                // Track client kind.
                let kind_tag = match hello.client {
                    ClientKind::Logi => Some(ClientKindTag::Logi),
                    ClientKind::Vscode => Some(ClientKindTag::Vscode),
                    ClientKind::Hooks => None,
                };
                if let Some(k) = kind_tag {
                    *client_kind.lock().await = Some(k);
                    self.apply_event(Event::ClientConnected { kind: k }).await;
                }

                let _ = self.tx.send(DaemonToClient::Notice(Notice {
                    message: format!(
                        "client connected: {:?} v{} (protocol {})",
                        hello.client, hello.version, hello.protocol
                    ),
                }));

                // Send current render state.
                self.broadcast_render().await;
            }

            ClientToDaemon::KeypadPress(kp) => {
                // Check if this is a gate (not a prompt).
                let is_gate = self.check_gate(&kp.prompt_id).await;
                if !is_gate {
                    self.apply_event(Event::KeypadPress {
                        prompt_id: kp.prompt_id,
                    })
                    .await;
                }
            }

            ClientToDaemon::DialpadButtonPress(bp) => {
                self.apply_event(Event::DialpadButton { button: bp.button })
                    .await;
            }

            ClientToDaemon::Adjustment(adj) => {
                self.apply_event(Event::Adjustment {
                    kind: adj.kind,
                    delta: adj.delta,
                })
                .await;
            }

            ClientToDaemon::PageNav(pn) => {
                self.apply_event(Event::PageNav {
                    direction: pn.direction,
                })
                .await;
            }

            ClientToDaemon::HookEvent(ev) => {
                self.apply_event(Event::HookEvent {
                    hook: ev.hook,
                    matcher: ev.matcher,
                    session_id: ev.session_id,
                    session_tag: ev.session_tag,
                })
                .await;
            }

            ClientToDaemon::TerminalsSnapshot(snapshot) => {
                self.apply_event(Event::TerminalsSnapshot(snapshot)).await;
            }
        }
    }

    /// Check if a prompt_id is actually a gate; if so, dispatch it immediately.
    async fn check_gate(&self, id: &str) -> bool {
        if let Some(gate) = self.config.gates.get(id) {
            // Gates dispatch immediately (they're navigation, not prompts).
            info!(gate_id = id, action = %gate.action, "gate triggered");
            let cmd = runbook_protocol::VscodeCommand::open_uri(&gate.action);
            let _ = self.tx.send(DaemonToClient::VscodeCommand(cmd));
            true
        } else {
            false
        }
    }

    /// Apply a reducer event: mutate state, then execute side effects.
    async fn apply_event(&self, event: Event) {
        let effects = {
            let mut state = self.state.lock().await;
            reducer::reduce(&mut state, &self.config, event)
        };

        for effect in effects {
            match effect {
                SideEffect::BroadcastRender => {
                    self.broadcast_render().await;
                }
                SideEffect::SendVscodeCommand(cmd) => {
                    if let Err(e) = self.tx.send(DaemonToClient::VscodeCommand(cmd)) {
                        warn!("no clients to receive VS Code command: {e}");
                    }
                }
            }
        }
    }

    async fn broadcast_render(&self) {
        let state = self.state.lock().await;
        let model = render::build_render_model(&state, &self.config);
        drop(state);
        let _ = self.tx.send(DaemonToClient::Render(model));
    }
}
