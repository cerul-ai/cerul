mod client;

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde_json::{json, Value};

use client::CerulClient;

#[derive(Parser)]
#[command(name = "cerul", version, about = "Cerul unified local/cloud CLI")]
struct Cli {
    #[arg(
        long,
        env = "CERUL_BASE_URL",
        default_value = "https://api.cerul.ai/v1"
    )]
    base_url: String,
    #[arg(long, env = "CERUL_API_KEY")]
    token: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Capabilities,
    Search {
        query: String,
        #[command(flatten)]
        scope: ScopeArgs,
    },
    Ask {
        prompt: String,
        #[command(flatten)]
        scope: ScopeArgs,
        #[arg(long)]
        idempotency_key: Option<String>,
    },
    AgentSession {
        #[command(subcommand)]
        action: AgentSessionAction,
    },
    Jobs {
        #[command(subcommand)]
        action: JobAction,
    },
    Export {
        #[command(flatten)]
        scope: ScopeArgs,
        #[arg(long)]
        input_json: String,
        #[arg(long)]
        idempotency_key: Option<String>,
    },
}

#[derive(Subcommand)]
enum AgentSessionAction {
    Get { agent_session_id: String },
    Delete { agent_session_id: String },
}

#[derive(Subcommand)]
enum JobAction {
    Get {
        job_id: String,
    },
    Cancel {
        job_id: String,
        #[arg(long)]
        idempotency_key: Option<String>,
    },
    Artifacts {
        job_id: String,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ExecutionPolicy {
    LocalOnly,
    PreferLocal,
    CloudAllowed,
    CloudRequired,
}

impl ExecutionPolicy {
    fn as_contract_value(self) -> &'static str {
        match self {
            Self::LocalOnly => "local_only",
            Self::PreferLocal => "prefer_local",
            Self::CloudAllowed => "cloud_allowed",
            Self::CloudRequired => "cloud_required",
        }
    }
}

#[derive(Args)]
struct ScopeArgs {
    #[arg(long)]
    library_id: Vec<String>,
    #[arg(long)]
    asset_id: Vec<String>,
    #[arg(long, value_enum, default_value = "prefer-local")]
    execution_policy: ExecutionPolicy,
}

impl ScopeArgs {
    fn scope(&self) -> Value {
        json!({
            "library_ids": self.library_id,
            "asset_ids": self.asset_id,
        })
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = run(Cli::parse()).await {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<()> {
    let token = cli
        .token
        .or_else(|| std::env::var("CERUL_INSTALLATION_TOKEN").ok())
        .ok_or_else(|| {
            anyhow!("missing token: set CERUL_API_KEY or CERUL_INSTALLATION_TOKEN, or pass --token")
        })?;
    let client = CerulClient::new(&cli.base_url, token)?;
    let response = match cli.command {
        Command::Capabilities => client.get("/v1/capabilities").await?,
        Command::Search { query, scope } => {
            client
                .post(
                    "/v1/search",
                    json!({
                        "query": query,
                        "scope": scope.scope(),
                        "execution_policy": scope.execution_policy.as_contract_value(),
                    }),
                    None,
                )
                .await?
        }
        Command::Ask {
            prompt,
            scope,
            idempotency_key,
        } => {
            let key = idempotency_key.unwrap_or_else(generated_idempotency_key);
            client
                .post(
                    "/v1/responses",
                    json!({
                        "input": [{"type": "message", "role": "user", "content": prompt}],
                        "scope": scope.scope(),
                        "execution_policy": scope.execution_policy.as_contract_value(),
                        "stream": false,
                    }),
                    Some(&key),
                )
                .await?
        }
        Command::AgentSession { action } => match action {
            AgentSessionAction::Get { agent_session_id } => {
                client
                    .get(&format!("/v1/agent-sessions/{agent_session_id}"))
                    .await?
            }
            AgentSessionAction::Delete { agent_session_id } => {
                client
                    .delete(&format!("/v1/agent-sessions/{agent_session_id}"))
                    .await?
            }
        },
        Command::Jobs { action } => match action {
            JobAction::Get { job_id } => client.get(&format!("/v1/jobs/{job_id}")).await?,
            JobAction::Cancel {
                job_id,
                idempotency_key,
            } => {
                let key = idempotency_key.unwrap_or_else(generated_idempotency_key);
                client
                    .post(&format!("/v1/jobs/{job_id}/cancel"), json!({}), Some(&key))
                    .await?
            }
            JobAction::Artifacts { job_id } => {
                client.get(&format!("/v1/jobs/{job_id}/artifacts")).await?
            }
        },
        Command::Export {
            scope,
            input_json,
            idempotency_key,
        } => {
            let input: Value =
                serde_json::from_str(&input_json).context("--input-json must be valid JSON")?;
            let key = idempotency_key.unwrap_or_else(generated_idempotency_key);
            client
                .post(
                    "/v1/jobs",
                    json!({
                        "capability_id": "export.timeline",
                        "capability_version": "1",
                        "scope": scope.scope(),
                        "execution_policy": scope.execution_policy.as_contract_value(),
                        "input": input,
                    }),
                    Some(&key),
                )
                .await?
        }
    };
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}

fn generated_idempotency_key() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("cli-{}-{nanos}", std::process::id())
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    #[test]
    fn cli_parses_canonical_contract_fixtures() {
        let artifact: Value = serde_json::from_str(include_str!(
            "../../../examples/fixtures/artifact-response.json"
        ))
        .expect("artifact fixture must be valid JSON");
        let response: Value = serde_json::from_str(include_str!(
            "../../../examples/fixtures/response-envelope.json"
        ))
        .expect("response fixture must be valid JSON");
        let upload: Value = serde_json::from_str(include_str!(
            "../../../examples/fixtures/upload-response.json"
        ))
        .expect("upload fixture must be valid JSON");

        assert_eq!(artifact["data"]["id"], "artifact_fixture1");
        assert_eq!(artifact["execution"]["location"], "local");
        assert_eq!(response["data"]["status"], "completed");
        assert_eq!(response["data"]["citations"][0]["id"], "ev_fixture1");
        assert_eq!(upload["data"]["asset_id"], "asset_fixture1");
        assert_ne!(upload["data"]["id"], upload["data"]["asset_id"]);
    }
}
