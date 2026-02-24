use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::{ws::WebSocketUpgrade, State},
    routing::post,
    Json,
    response::IntoResponse,
    routing::get,
    Router,
};
use clap::Parser;
use futures::{SinkExt, StreamExt};
use tokio::sync::{broadcast, Mutex};
use tracing::{error, info};

use runbook_protocol::{
    AgentState, Adjustment, AdjustmentKind, ClientKind, ClientToDaemon, DaemonToClient, DialpadButton,
    DialpadButtonPress, Hello, HelloAck, KeypadPress, KeypadRender, KeypadSlotRender, Notice,
    RenderModel, TerminalTarget, VscodeCommand,
};

mod config;
use config::RunbookConfig;

#[derive(Debug, Parser)]
#[command(name = "runbookd", about = "Runbook daemon")]
struct Args {
    /// Path to runbook.yaml
    #[arg(long, default_value = "./runbook.yaml")]
    config: String,
}

#[derive(Debug)]
struct DaemonState {
    agent_state: AgentState,
    /// Index into config.keypad.pages
    page: usize,
    /// Armed prompt id
    armed: Option<String>,
    /// Last dispatched prompt id (for display)
    last_dispatched: Option<String>,
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
        state: Arc::new(Mutex::new(DaemonState {
            agent_state: AgentState::Unknown,
            page: initial_page,
            armed: None,
            last_dispatched: None,
        })),
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
    let bytes = std::fs::read(path)
        .map_err(|e| anyhow::anyhow!("failed to read config '{path}': {e}"))?;
    let cfg: RunbookConfig = serde_yaml::from_slice(&bytes)
        .map_err(|e| anyhow::anyhow!("failed to parse yaml '{path}': {e}"))?;
    Ok(cfg)
}

async fn ws_handler(ws: WebSocketUpgrade, State(app): State<App>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(app, socket))
}

async fn hook_handler(
    State(app): State<App>,
    Json(ev): Json<runbook_protocol::HookEvent>,
) -> impl IntoResponse {
    app.on_hook_event(ev.hook, ev.matcher, ev.payload).await;
    // Also push a render update immediately.
    // (on_hook_event already does this, but keep the contract explicit.)
    "ok"
}

async fn handle_socket(app: App, socket: axum::extract::ws::WebSocket) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Subscribe to daemon broadcast.
    let mut rx = app.tx.subscribe();

    // Send hello proactively.
    let _ = ws_tx
        .send(axum::extract::ws::Message::Text(
            serde_json::to_string(&DaemonToClient::Hello(HelloAck {
                protocol: runbook_protocol::PROTOCOL_VERSION,
                daemon_version: env!("CARGO_PKG_VERSION").to_string(),
            }))
            .unwrap(),
        ))
        .await;

    // Task: forward broadcast -> websocket
    let mut ws_tx_clone = ws_tx.clone();
    let forward = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            let text = match serde_json::to_string(&msg) {
                Ok(t) => t,
                Err(e) => {
                    error!("failed to serialize daemon msg: {e}");
                    continue;
                }
            };
            if ws_tx_clone
                .send(axum::extract::ws::Message::Text(text))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    // Receive loop
    while let Some(Ok(msg)) = ws_rx.next().await {
        match msg {
            axum::extract::ws::Message::Text(text) => {
                match serde_json::from_str::<ClientToDaemon>(&text) {
                    Ok(parsed) => {
                        if let Err(e) = app.handle_client_message(parsed).await {
                            error!("handle_client_message: {e:#}");
                        }
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
}

impl App {
    async fn handle_client_message(&self, msg: ClientToDaemon) -> anyhow::Result<()> {
        match msg {
            ClientToDaemon::Hello(hello) => {
                self.tx.send(DaemonToClient::Notice(Notice {
                    message: format!(
                        "client connected: {:?} v{} (protocol {})",
                        hello.client, hello.version, hello.protocol
                    ),
                }))?;

                // Always respond with our current render model.
                self.broadcast_render().await;
            }

            ClientToDaemon::KeypadPress(KeypadPress { slot }) => {
                self.on_keypad_press(slot).await;
            }

            ClientToDaemon::DialpadButtonPress(DialpadButtonPress { button }) => {
                self.on_dialpad_button(button).await;
            }

            ClientToDaemon::Adjustment(Adjustment { kind, delta }) => {
                self.on_adjustment(kind, delta).await;
            }

            ClientToDaemon::HookEvent(ev) => {
                self.on_hook_event(ev.hook, ev.matcher, ev.payload).await;
            }
        }

        Ok(())
    }

    async fn on_keypad_press(&self, slot: u8) {
        let page = {
            let state = self.state.lock().await;
            state.page
        };

        let page_cfg = &self.config.keypad.pages[page];
        let slot_cfg = page_cfg.slots.get(slot as usize);

        if let Some(slot_cfg) = slot_cfg {
            let mut state = self.state.lock().await;
            state.armed = Some(slot_cfg.id.clone());
        }

        self.broadcast_render().await;
    }

    async fn on_dialpad_button(&self, button: DialpadButton) {
        match button {
            DialpadButton::Enter => {
                // If a prompt is armed, dispatch it to VS Code. Otherwise, send an Enter keystroke
                // to the Claude Code terminal (useful for confirming /export).
                let maybe = {
                    let mut state = self.state.lock().await;
                    if let Some(id) = state.armed.take() {
                        state.last_dispatched = Some(id.clone());
                        Some(id)
                    } else {
                        None
                    }
                };

                if let Some(id) = maybe {
                    let page = { self.state.lock().await.page };
                    if let Some(cmd) = self.lookup_command(page, &id) {
                        let _ = self.tx.send(DaemonToClient::VscodeCommand(cmd));
                    }
                } else {
                    let _ = self
                        .tx
                        .send(DaemonToClient::VscodeCommand(VscodeCommand::send_text(
                            TerminalTarget::ActiveClaude,
                            "",
                            true,
                        )));
                }

                self.broadcast_render().await;
            }
            DialpadButton::Esc => {
                // If armed, clear. Else send ESC.
                let cleared = {
                    let mut state = self.state.lock().await;
                    if state.armed.is_some() {
                        state.armed = None;
                        true
                    } else {
                        false
                    }
                };

                if !cleared {
                    let _ = self
                        .tx
                        .send(DaemonToClient::VscodeCommand(VscodeCommand::send_text(
                            TerminalTarget::ActiveClaude,
                            "\u{1b}",
                            false,
                        )));
                }

                self.broadcast_render().await;
            }
            DialpadButton::CtrlC => {
                // Always forward Ctrl+C (\u0003). Claude Code handles the null-first-press gate.
                let _ = self
                    .tx
                    .send(DaemonToClient::VscodeCommand(VscodeCommand::send_text(
                        TerminalTarget::ActiveClaude,
                        "\u{0003}",
                        false,
                    )));
            }
            DialpadButton::Export => {
                // Insert /export (no newline). User confirms with Enter twice.
                let _ = self
                    .tx
                    .send(DaemonToClient::VscodeCommand(VscodeCommand::send_text(
                        TerminalTarget::ActiveClaude,
                        "/export",
                        false,
                    )));
            }
        }
    }

    async fn on_adjustment(&self, kind: AdjustmentKind, delta: i32) {
        match kind {
            AdjustmentKind::Dial => {
                // Scroll the terminal output by lines.
                let _ = self
                    .tx
                    .send(DaemonToClient::VscodeCommand(VscodeCommand::scroll_terminal(
                        TerminalTarget::ActiveClaude,
                        delta,
                        runbook_protocol::TerminalScrollUnit::Lines,
                    )));
            }
            AdjustmentKind::Roller => {
                // Cycle terminals by index.
                let _ = self
                    .tx
                    .send(DaemonToClient::VscodeCommand(VscodeCommand::focus_terminal(
                        TerminalTarget::ActiveClaude,
                        delta.signum(),
                    )));
            }
        }
    }

    async fn on_hook_event(&self, hook: String, matcher: Option<String>, _payload: serde_json::Value) {
        // Minimal v1 mapping: derive a coarse agent state.
        let mut state = self.state.lock().await;

        match hook.as_str() {
            "Notification" => match matcher.as_deref() {
                Some("idle_prompt") => state.agent_state = AgentState::Idle,
                Some("permission_prompt") => state.agent_state = AgentState::WaitingPermission,
                Some("elicitation_dialog") => state.agent_state = AgentState::WaitingInput,
                _ => {}
            },
            "UserPromptSubmit" => state.agent_state = AgentState::Running,
            "TaskCompleted" => state.agent_state = AgentState::Complete,
            "Stop" => state.agent_state = AgentState::Settled,
            "SessionEnd" => state.agent_state = AgentState::Ended,
            _ => {}
        }

        drop(state);
        self.broadcast_render().await;
    }

    fn lookup_command(&self, page: usize, id: &str) -> Option<VscodeCommand> {
        let page_cfg = &self.config.keypad.pages[page];
        for slot in &page_cfg.slots {
            if slot.id == id {
                return Some(VscodeCommand::send_text(
                    TerminalTarget::ActiveClaude,
                    &slot.command,
                    true,
                ));
            }
        }
        None
    }

    async fn broadcast_render(&self) {
        let state = self.state.lock().await;
        let page_cfg = &self.config.keypad.pages[state.page];

        let slots: Vec<KeypadSlotRender> = page_cfg
            .slots
            .iter()
            .enumerate()
            .map(|(i, s)| KeypadSlotRender {
                slot: i as u8,
                label: s.label.clone(),
                sublabel: s.sublabel.clone(),
                armed: state.armed.as_deref() == Some(&s.id),
            })
            .collect();

        let armed = state
            .armed
            .as_ref()
            .and_then(|id| page_cfg.slots.iter().find(|s| &s.id == id))
            .map(|s| runbook_protocol::ArmedPrompt {
                id: s.id.clone(),
                label: s.label.clone(),
                command: s.command.clone(),
            });

        let render = RenderModel {
            agent_state: state.agent_state.clone(),
            armed,
            keypad: KeypadRender { slots },
        };

        drop(state);

        let _ = self.tx.send(DaemonToClient::Render(render));
    }
}

