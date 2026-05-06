use schemars::schema_for;
use std::fs;
use std::path::PathBuf;

use runbook_protocol::{ClientToDaemon, DaemonToClient, RenderModel};

fn main() -> anyhow::Result<()> {
    let schema_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schema");
    fs::create_dir_all(&schema_dir)?;

    let client_to_daemon_schema = schema_for!(ClientToDaemon);
    let daemon_to_client_schema = schema_for!(DaemonToClient);
    let render_model_schema = schema_for!(RenderModel);

    let write_schema =
        |filename: &str, schema: &schemars::schema::RootSchema| -> anyhow::Result<()> {
            let path = schema_dir.join(filename);
            let json = serde_json::to_string_pretty(schema)?;
            fs::write(&path, json)?;
            println!("Generated {}", path.display());
            Ok(())
        };

    write_schema("client_to_daemon.schema.json", &client_to_daemon_schema)?;
    write_schema("daemon_to_client.schema.json", &daemon_to_client_schema)?;
    write_schema("render_model.schema.json", &render_model_schema)?;
    Ok(())
}
