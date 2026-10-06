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
    Authority {
        #[command(subcommand)]
        command: Authority,
    },
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
        #[arg(long)]
        authority: Option<PathBuf>,
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
    Cad {
        #[command(subcommand)]
        command: Cad,
    },
    Job {
        #[command(subcommand)]
        command: Job,
    },
    Results {
        #[command(subcommand)]
        command: Results,
    },
    /// Plan presentation from a registered source job; submit its approved plan with job submit.
    Render {
        request: PathBuf,
    },
    /// Plan hardware encoding from an immutable registered rendered-frame job.
    Video {
        request: PathBuf,
    },
    /// Plan HIP point-gradient filtering of registered retained fields.
    Filter {
        request: PathBuf,
    },
    /// Plan static synthetic FEM from an approved registered CAD solid.
    FemImported {
        request: PathBuf,
    },
    Artifact {
        #[command(subcommand)]
        command: Artifact,
    },
    Qualify {
        /// Inspect immutable historical evidence from a specific job.
        #[arg(long)]
        job: Option<String>,
    },
}
#[derive(Subcommand)]
enum Authority {
    Install { file: PathBuf },
}
#[derive(Subcommand)]
enum Backend {
    List,
    HipIdentity { pci: String },
}
#[derive(Subcommand)]
enum Case {
    /// Plan a prescribed synthetic transient CPU thermal history with independent output/solver schedules.
    PlanThermalReference {
        file: PathBuf,
        #[arg(long, default_value = "research")]
        policy: String,
    },
    /// Plan a synthetic wall-centered planar wetting reference with explicit SI properties.
    PlanWettingReference {
        file: PathBuf,
        #[arg(long, default_value = "research")]
        policy: String,
    },
    /// Plan a static synthetic Gmsh/CalculiX CPU reference with explicit SI inputs.
    PlanFemReference {
        file: PathBuf,
        #[arg(long, default_value = "research")]
        policy: String,
    },
    Init,
    Validate {
        file: PathBuf,
    },
    /// Validate a conservative one-way map without running a coupled solver.
    ValidateTransfer {
        file: PathBuf,
    },
    /// Inspect cold-restart units, property ranges, histories and missing inputs.
    ValidateColdRestart {
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
    PlanCadInspection {
        file: PathBuf,
        #[arg(long, default_value = "research")]
        policy: String,
        #[arg(long, default_value = "67108864")]
        max_artifact_bytes: u64,
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
enum Cad {
    /// Plan correspondence meshing from registered authorized BREP; requires the worker.
    Mesh { request: PathBuf },
    /// Prepare an approval-bound sandboxed inspection plan; submit with job submit.
    Inspect {
        file: PathBuf,
        #[arg(long, default_value = "research")]
        policy: String,
        #[arg(long, default_value = "67108864")]
        max_artifact_bytes: u64,
    },
    /// Read registered named-solid metadata from a succeeded bound CAD job.
    Regions { id: String },
    /// Export the complete checksummed CAD job, preserving the approved original.
    Export {
        #[arg(long)]
        state: PathBuf,
        id: String,
        destination: PathBuf,
    },
}
#[derive(Subcommand)]
enum Results {
    Describe {
        id: String,
    },
    /// Sample up to 64 exact registered static native FEM entity locations.
    Sample {
        request: PathBuf,
    },
    /// Compare registered static samples on the exact same mesh and associations.
    Compare {
        request: PathBuf,
    },
    /// Sample registered thermal nodes at one exact approved retained time.
    SampleThermal {
        request: PathBuf,
    },
    /// Compare registered thermal samples on the exact same mesh; right minus left.
    CompareThermal {
        request: PathBuf,
    },
    /// Screen one complete native thermal box surface with explicit air inputs.
    Moisture {
        request: PathBuf,
    },
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
        Commands::Authority {
            command: Authority::Install { file },
        } => {
            let authority: harbor_cad::authority::HostAuthority = read(&file)?;
            let root = harbor_cad::admission::shared_root()?;
            harbor_cad::admission::Admission::install(&root, &authority)?;
            return print(
                &serde_json::json!({"authority_digest":digest(&authority)?,"admission_root":root}),
            );
        }
        Commands::Doctor => {
            return print(&worker::doctor()?);
        }
        Commands::Backend {
            command: Backend::List,
        } => {
            return print(&worker::backends());
        }
        Commands::Backend {
            command: Backend::HipIdentity { pci },
        } => {
            return print(&harbor_cad::devices::HipIdentity::resolve(&pci)?);
        }
        Commands::Qualify { job: Some(job_id) } => Operation::QualificationReport { job_id },
        Commands::Qualify { job: None } => {
            return print(
                &serde_json::json!({"qualified":false,"scope":"no specific immutable job/runtime selected","gates":{"A0":"partial","A1":"analytical-reference-only","B1":"unqualified","B2":"unqualified"},"doctor":worker::doctor()?}),
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
        Commands::Worker {
            state,
            profile,
            authority,
        } => {
            let socket = cli.socket.unwrap_or_else(|| state.join("worker.sock"));
            return worker::serve_authorized(&state, &socket, &profile, authority.as_deref());
        }
        Commands::RunJob { state, profile, id } => {
            return worker::run_job(&state, &profile, &id);
        }
        Commands::Case { command } => match command {
            Case::PlanThermalReference { file, policy } => {
                let plan = ExecutionPlan::thermal_reference(read(&file)?, policy)?;
                return print(&serde_json::json!({"approval_digest":plan.id()?,"plan":plan}));
            }
            Case::PlanWettingReference { file, policy } => {
                let plan = ExecutionPlan::wetting_reference(read(&file)?, policy)?;
                return print(&serde_json::json!({"approval_digest":plan.id()?,"plan":plan}));
            }
            Case::PlanFemReference { file, policy } => {
                let plan = ExecutionPlan::fem_reference(read(&file)?, policy)?;
                return print(&serde_json::json!({"approval_digest":plan.id()?,"plan":plan}));
            }
            Case::Init => {
                return print(&CaseSpec::reference());
            }
            Case::Validate { file } => {
                let case: CaseSpec = read(&file)?;
                case.validate()?;
                return print(&serde_json::json!({"valid":true,"science_id":case.science_id()?}));
            }
            Case::ValidateTransfer { file } => {
                let transfer: harbor_cad::transfers::ConservativeTransfer = read(&file)?;
                transfer.validate()?;
                return print(
                    &serde_json::json!({"valid":true,"transfer_id":transfer.id()?,"source_elements":transfer.source.measures.len(),
                    "destination_elements":transfer.destination.measures.len(),"geometric_correspondence":"requires native mesh verification",
                    "physical_validation":"unqualified","executed":false}),
                );
            }
            Case::Plan { file } => {
                let plan = ExecutionPlan::reference(read(&file)?)?;
                return print(&serde_json::json!({"approval_digest":plan.id()?,"plan":plan}));
            }
            Case::ValidateColdRestart { file } => {
                let case: harbor_cad::recipes::ColdRestartSpec = read(&file)?;
                return print(&case.inspect()?);
            }
            Case::PlanOpenlbReference { file, policy } => {
                let plan = ExecutionPlan::openlb_reference(read(&file)?, policy)?;
                return print(&serde_json::json!({"approval_digest":plan.id()?,"plan":plan}));
            }
            Case::PlanCadInspection {
                file,
                policy,
                max_artifact_bytes,
            } => {
                let plan = ExecutionPlan::cad_inspection(read(&file)?, policy, max_artifact_bytes)?;
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
        Commands::FemImported { request } => Operation::PlanFemImported {
            request: Box::new(read(&request)?),
        },
        Commands::Cad { command } => match command {
            Cad::Mesh { request } => Operation::PlanCadMesh {
                request: Box::new(read(&request)?),
            },
            Cad::Inspect {
                file,
                policy,
                max_artifact_bytes,
            } => {
                let plan = ExecutionPlan::cad_inspection(read(&file)?, policy, max_artifact_bytes)?;
                return print(&serde_json::json!({"approval_digest":plan.id()?,"plan":plan}));
            }
            Cad::Regions { id } => Operation::CadRegions { job_id: id },
            Cad::Export {
                state,
                id,
                destination,
            } => {
                let store = Store::open(&state)?;
                harbor_cad::cad::regions(&store, &id)?;
                let count = store.export(&id, &destination)?;
                return print(
                    &serde_json::json!({"exported":destination,"artifacts":count,"uploads":false}),
                );
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
        Commands::Results { command } => match command {
            Results::Describe { id } => Operation::Describe { job_id: id },
            Results::Sample { request } => Operation::ResultsSample {
                request: Box::new(read(&request)?),
            },
            Results::Compare { request } => Operation::ResultsCompare {
                request: Box::new(read(&request)?),
            },
            Results::SampleThermal { request } => Operation::ResultsSampleThermal {
                request: Box::new(read(&request)?),
            },
            Results::CompareThermal { request } => Operation::ResultsCompareThermal {
                request: Box::new(read(&request)?),
            },
            Results::Moisture { request } => Operation::ResultsMoisture {
                request: Box::new(read(&request)?),
            },
        },
        Commands::Render { request } => Operation::PlanPresentation {
            request: read(&request)?,
        },
        Commands::Video { request } => Operation::PlanVideo {
            request: read(&request)?,
        },
        Commands::Filter { request } => Operation::PlanFilter {
            request: Box::new(read(&request)?),
        },
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
