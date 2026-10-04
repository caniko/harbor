use clap::{Parser, Subcommand};
use harbor_cad::{Result, contracts::*, storage::Store, worker};
use std::{io::Write, path::PathBuf};

#[derive(Parser)]
#[command(
    version,
    about = "Local scientific CAD worker; qualification is independent of process success"
)]
struct Cli {
    #[arg(long, global = true, env = "HARBOR_CAD_SOCKET")]
    socket: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    Doctor,
    Backend {
        #[command(subcommand)]
        command: Backend,
    },
    Schema {
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Worker {
        #[arg(long)]
        state: PathBuf,
        #[arg(long)]
        profile: PathBuf,
    },
    #[command(hide = true)]
    RunJob {
        #[arg(long)]
        state: PathBuf,
        #[arg(long)]
        profile: PathBuf,
        id: String,
    },
    Case {
        #[command(subcommand)]
        command: Case,
    },
    Job {
        #[command(subcommand)]
        command: Job,
    },
    Results {
        #[command(subcommand)]
        command: Results,
    },
    Artifact {
        #[command(subcommand)]
        command: Artifact,
    },
    Qualify,
}
#[derive(Subcommand)]
enum Backend {
    List,
}
#[derive(Subcommand)]
enum Case {
    Init,
    Validate {
        file: PathBuf,
    },
    Plan {
        file: PathBuf,
    },
    PlanOpenlbReference {
        file: PathBuf,
        #[arg(long, default_value = "research")]
        policy: String,
    },
    PlanB1 {
        file: PathBuf,
        #[arg(long)]
        devices: PathBuf,
        #[arg(long, default_value = "research")]
        policy: String,
    },
}
#[derive(Subcommand)]
enum Job {
    Submit {
        plan: PathBuf,
        #[arg(long)]
        approve: String,
        #[arg(long)]
        idempotency_key: String,
    },
    Status {
        id: String,
    },
    Logs {
        id: String,
        #[arg(long, default_value = "0")]
        after: u64,
        #[arg(long, default_value = "20")]
        limit: u32,
    },
    Cancel {
        id: String,
    },
}
#[derive(Subcommand)]
enum Results {
    Describe { id: String },
}
#[derive(Subcommand)]
enum Artifact {
    List {
        id: String,
        #[arg(long)]
        after: Option<String>,
        #[arg(long, default_value = "20")]
        limit: u32,
    },
    Export {
        #[arg(long)]
        state: PathBuf,
        id: String,
        destination: PathBuf,
    },
}
fn read<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Result<T> {
    Ok(serde_json::from_slice(&worker::read_bounded(
        path,
        MAX_MESSAGE,
    )?)?)
}
fn print(value: &impl serde::Serialize) -> Result<()> {
    serde_json::to_writer_pretty(std::io::stdout(), value)?;
    std::io::stdout().write_all(b"\n")?;
    Ok(())
}
fn main() {
    if let Err(e) = run(Cli::parse()) {
        let _ = print(&serde_json::json!({"ok":false,"error":e.diagnostic()}));
        std::process::exit(1);
    }
}
fn run(cli: Cli) -> Result<()> {
    let op = match cli.command {
        Commands::Doctor => {
            return print(&worker::doctor()?);
        }
        Commands::Backend { .. } => {
            return print(&worker::backends());
        }
        Commands::Qualify => {
            return print(
                &serde_json::json!({"qualified":false,"gates":{"A0":"partial","A1":"analytical-reference-only","B1":"unqualified","B2":"unqualified"},"doctor":worker::doctor()?}),
            );
        }
        Commands::Schema { output } => {
            let schema = serde_json::to_vec_pretty(&schemas())?;
            if let Some(path) = output {
                std::fs::write(path, schema)?;
            } else {
                print(&schemas())?;
            }
            return Ok(());
        }
        Commands::Worker { state, profile } => {
            let socket = cli.socket.unwrap_or_else(|| state.join("worker.sock"));
            return worker::serve(&state, &socket, &profile);
        }
        Commands::RunJob { state, profile, id } => {
            return worker::run_job(&state, &profile, &id);
        }
        Commands::Case { command } => match command {
            Case::Init => {
                return print(&CaseSpec::reference());
            }
            Case::Validate { file } => {
                let case: CaseSpec = read(&file)?;
                case.validate()?;
                return print(&serde_json::json!({"valid":true,"science_id":case.science_id()?}));
            }
            Case::Plan { file } => {
                let plan = ExecutionPlan::reference(read(&file)?)?;
                return print(&serde_json::json!({"approval_digest":plan.id()?,"plan":plan}));
            }
            Case::PlanOpenlbReference { file, policy } => {
                let plan = ExecutionPlan::openlb_reference(read(&file)?, policy)?;
                return print(&serde_json::json!({"approval_digest":plan.id()?,"plan":plan}));
            }
            Case::PlanB1 {
                file,
                devices,
                policy,
            } => {
                let plan = ExecutionPlan::b1(read(&file)?, read(&devices)?, policy)?;
                return print(&serde_json::json!({"approval_digest":plan.id()?,"plan":plan}));
            }
        },
        Commands::Job { command } => match command {
            Job::Submit {
                plan,
                approve,
                idempotency_key,
            } => Operation::Submit {
                plan: read(&plan)?,
                approved_digest: approve,
                idempotency_key,
            },
            Job::Status { id } => Operation::Status { job_id: id },
            Job::Logs { id, after, limit } => Operation::Logs {
                job_id: id,
                after,
                limit,
            },
            Job::Cancel { id } => Operation::Cancel { job_id: id },
        },
        Commands::Results {
            command: Results::Describe { id },
        } => Operation::Describe { job_id: id },
        Commands::Artifact { command } => match command {
            Artifact::List { id, after, limit } => Operation::Artifacts {
                job_id: id,
                after,
                limit,
            },
            Artifact::Export {
                state,
                id,
                destination,
            } => {
                let store = Store::open(&state)?;
                let count = store.export(&id, &destination)?;
                return print(
                    &serde_json::json!({"exported":destination,"artifacts":count,"uploads":false}),
                );
            }
        },
    };
    let socket = cli
        .socket
        .ok_or_else(|| invalid("--socket or HARBOR_CAD_SOCKET required"))?;
    let response = worker::request(&socket, op)?;
    print(&response)?;
    if !response.ok {
        std::process::exit(1);
    }
    Ok(())
}
