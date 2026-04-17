use clap::Args;
use serde_json::json;

use crate::errors::Result;

#[derive(Debug, Args)]
pub struct AcpWorkerArgs {
    #[arg(long)]
    pub creator: String,
}

pub async fn run(args: AcpWorkerArgs) -> Result<()> {
    // Phase-1 stub — prints a single initialize reply and exits.
    // Phase-2 (WS2) replaces this with the stdin/stdout JSON-RPC main loop.
    let reply = json!({
        "jsonrpc": "2.0",
        "method":  "worker/initialize",
        "result":  { "ok": true, "creator_id": args.creator, "worker_pid": std::process::id() },
    });
    println!("{}", reply);
    Ok(())
}
