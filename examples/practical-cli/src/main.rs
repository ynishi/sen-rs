use sen::{CliResult, State, Router, Args, init_subscriber};
use clap::Parser;

// ============================================
// Application State (Global Configuration)
// ============================================

#[derive(Clone)]
pub struct AppState {
    pub config_path: String,
    pub api_endpoint: String,
    pub verbose: bool,
}

impl AppState {
    fn load() -> CliResult<Self> {
        // グローバルオプションをパース
        let args: Vec<String> = std::env::args().collect();

        let verbose = args.contains(&"--verbose".to_string()) || args.contains(&"-v".to_string());
        let config_path = args.iter()
            .position(|a| a == "--config")
            .and_then(|i| args.get(i + 1))
            .cloned()
            .unwrap_or_else(|| "~/.myctl/config.yaml".to_string());

        Ok(Self {
            config_path,
            api_endpoint: "https://api.example.com".to_string(),
            verbose,
        })
    }
}

// ============================================
// Argument Types for Each Subcommand (with Clap)
// ============================================

// Database commands
#[derive(Parser, Debug)]
pub struct DbCreateArgs {
    /// Database name
    pub name: String,

    /// Storage size (e.g., 10GB, 100GB)
    #[arg(long, default_value = "10GB")]
    pub size: String,

    /// Database engine (postgres, mysql, mongodb)
    #[arg(long, default_value = "postgres")]
    pub engine: String,

    /// AWS region
    #[arg(long)]
    pub region: Option<String>,

    /// Enable automatic backups
    #[arg(long)]
    pub backup: bool,
}

#[derive(Parser, Debug)]
pub struct DbListArgs {
    /// Output format (json, table, yaml)
    #[arg(long, default_value = "table")]
    pub format: String,

    /// Filter by name pattern (e.g., name:prod-*)
    #[arg(long)]
    pub filter: Option<String>,
}

#[derive(Parser, Debug)]
pub struct DbDeleteArgs {
    /// Database name
    pub name: String,

    /// Force deletion without confirmation
    #[arg(long)]
    pub force: bool,

    /// Create backup before deletion
    #[arg(long)]
    pub backup: bool,
}

// Server commands
#[derive(Parser, Debug)]
pub struct ServerStartArgs {
    /// Server name
    pub name: String,

    /// EC2 instance type
    #[arg(long, default_value = "t3.medium")]
    pub instance_type: String,

    /// Number of instances to start
    #[arg(long, default_value = "1")]
    pub count: u32,

    /// Enable auto-scaling
    #[arg(long)]
    pub auto_scaling: bool,
}

#[derive(Parser, Debug)]
pub struct ServerStopArgs {
    /// Server name
    pub name: String,

    /// Perform graceful shutdown
    #[arg(long)]
    pub graceful: bool,

    /// Shutdown timeout in seconds
    #[arg(long, default_value = "30")]
    pub timeout: u32,
}

#[derive(Parser, Debug)]
pub struct ServerListArgs {
    /// Filter by status (running, stopped)
    #[arg(long)]
    pub status: Option<String>,

    /// Output format (json, table)
    #[arg(long, default_value = "table")]
    pub format: String,
}

// Deploy commands
#[derive(Parser, Debug)]
pub struct DeployAppArgs {
    /// Application name
    pub app_name: String,

    /// Docker image tag
    #[arg(long)]
    pub image: Option<String>,

    /// Number of replicas
    #[arg(long, default_value = "1")]
    pub replicas: u32,

    /// Deployment environment (production, staging, development)
    #[arg(long, default_value = "production")]
    pub environment: String,

    /// Enable health check
    #[arg(long)]
    pub health_check: bool,
}

#[derive(Parser, Debug)]
pub struct DeployRollbackArgs {
    /// Application name
    pub app_name: String,

    /// Revision number to rollback to
    #[arg(long)]
    pub revision: Option<String>,
}

// Network commands
#[derive(Parser, Debug)]
pub struct NetworkCreateArgs {
    /// Network name
    pub name: String,

    /// CIDR block (e.g., 10.0.0.0/16)
    #[arg(long, default_value = "10.0.0.0/16")]
    pub cidr: String,

    /// Number of subnets to create
    #[arg(long, default_value = "1")]
    pub subnet_count: u32,
}

// Storage commands
#[derive(Parser, Debug)]
pub struct StorageUploadArgs {
    /// S3 bucket name
    pub bucket: String,

    /// Local file path to upload
    pub file_path: String,

    /// Enable server-side encryption
    #[arg(long)]
    pub encrypt: bool,

    /// Make file publicly readable
    #[arg(long)]
    pub public: bool,
}

// ============================================
// Handlers (Organized by Resource)
// ============================================

mod handlers {
    use super::*;

    // Database handlers
    pub mod db {
        use super::*;

        #[sen::handler(desc = "Create a new database")]
        pub async fn create(
            state: State<AppState>,
            Args(args): Args<DbCreateArgs>,
        ) -> CliResult<String> {
            let app = state.read().await;

            if app.verbose {
                println!("[DEBUG] Creating database with args: {:?}", args);
            }

            Ok(format!(
                "Creating database '{}'\n\
                 - Engine: {}\n\
                 - Size: {}\n\
                 - Region: {}\n\
                 - Backup: {}\n\
                 - API: {}",
                args.name,
                args.engine,
                args.size,
                args.region.as_deref().unwrap_or("default"),
                args.backup,
                app.api_endpoint
            ))
        }

        #[sen::handler(desc = "List all databases")]
        pub async fn list(
            state: State<AppState>,
            Args(args): Args<DbListArgs>,
        ) -> CliResult<String> {
            let app = state.read().await;

            if app.verbose {
                println!("[DEBUG] Listing databases with format: {}", args.format);
            }

            let filter_str = args.filter.as_deref().unwrap_or("none");
            Ok(format!(
                "Databases (format: {}, filter: {})\n\
                 - prod-db-01 (postgres, running)\n\
                 - dev-db-02 (mysql, stopped)\n\
                 - staging-db-03 (postgres, running)",
                args.format, filter_str
            ))
        }

        #[sen::handler(desc = "Delete a database")]
        pub async fn delete(
            state: State<AppState>,
            Args(args): Args<DbDeleteArgs>,
        ) -> CliResult<String> {
            let app = state.read().await;

            if !args.force {
                return Err(sen::CliError::user(
                    "Delete requires --force flag for confirmation"
                ));
            }

            if app.verbose {
                println!("[DEBUG] Deleting database: {}", args.name);
            }

            Ok(format!(
                "Deleting database '{}' (backup: {})",
                args.name, args.backup
            ))
        }
    }

    // Server handlers
    pub mod server {
        use super::*;

        #[sen::handler(desc = "Start server instances")]
        pub async fn start(
            state: State<AppState>,
            Args(args): Args<ServerStartArgs>,
        ) -> CliResult<String> {
            let app = state.read().await;

            if app.verbose {
                println!("[DEBUG] Starting servers: {:?}", args);
            }

            Ok(format!(
                "Starting {} instance(s) of '{}'\n\
                 - Type: {}\n\
                 - Auto-scaling: {}",
                args.count, args.name, args.instance_type, args.auto_scaling
            ))
        }

        pub async fn stop(
            state: State<AppState>,
            Args(args): Args<ServerStopArgs>,
        ) -> CliResult<String> {
            let app = state.read().await;

            if app.verbose {
                println!("[DEBUG] Stopping server: {}", args.name);
            }

            Ok(format!(
                "Stopping server '{}' (graceful: {}, timeout: {}s)",
                args.name, args.graceful, args.timeout
            ))
        }

        pub async fn list(
            state: State<AppState>,
            Args(args): Args<ServerListArgs>,
        ) -> CliResult<String> {
            let app = state.read().await;

            if app.verbose {
                println!("[DEBUG] Listing servers");
            }

            let status_filter = args.status.as_deref().unwrap_or("all");
            Ok(format!(
                "Servers (status: {}, format: {})\n\
                 - web-server-01 (running, t3.medium)\n\
                 - api-server-02 (running, t3.large)\n\
                 - worker-server-03 (stopped, t3.small)",
                status_filter, args.format
            ))
        }
    }

    // Deploy handlers
    pub mod deploy {
        use super::*;

        pub async fn app(
            state: State<AppState>,
            Args(args): Args<DeployAppArgs>,
        ) -> CliResult<String> {
            let app = state.read().await;

            if app.verbose {
                println!("[DEBUG] Deploying app: {:?}", args);
            }

            let image = args.image.unwrap_or_else(|| format!("{}:latest", args.app_name));

            Ok(format!(
                "Deploying '{}'\n\
                 - Image: {}\n\
                 - Replicas: {}\n\
                 - Environment: {}\n\
                 - Health check: {}",
                args.app_name, image, args.replicas, args.environment, args.health_check
            ))
        }

        pub async fn rollback(
            state: State<AppState>,
            Args(args): Args<DeployRollbackArgs>,
        ) -> CliResult<String> {
            let app = state.read().await;

            if app.verbose {
                println!("[DEBUG] Rolling back app: {}", args.app_name);
            }

            let revision = args.revision.as_deref().unwrap_or("previous");
            Ok(format!(
                "Rolling back '{}' to revision: {}",
                args.app_name, revision
            ))
        }
    }

    // Network handlers
    pub mod network {
        use super::*;

        pub async fn create(
            state: State<AppState>,
            Args(args): Args<NetworkCreateArgs>,
        ) -> CliResult<String> {
            let app = state.read().await;

            if app.verbose {
                println!("[DEBUG] Creating network: {:?}", args);
            }

            Ok(format!(
                "Creating network '{}'\n\
                 - CIDR: {}\n\
                 - Subnets: {}",
                args.name, args.cidr, args.subnet_count
            ))
        }

        pub async fn list(state: State<AppState>) -> CliResult<String> {
            let app = state.read().await;

            if app.verbose {
                println!("[DEBUG] Listing networks");
            }

            Ok("Networks:\n\
                - vpc-prod (10.0.0.0/16)\n\
                - vpc-dev (10.1.0.0/16)".to_string())
        }
    }

    // Storage handlers
    pub mod storage {
        use super::*;

        pub async fn upload(
            state: State<AppState>,
            Args(args): Args<StorageUploadArgs>,
        ) -> CliResult<String> {
            let app = state.read().await;

            if app.verbose {
                println!("[DEBUG] Uploading file: {:?}", args);
            }

            Ok(format!(
                "Uploading '{}' to bucket '{}'\n\
                 - Encryption: {}\n\
                 - Public: {}",
                args.file_path, args.bucket, args.encrypt, args.public
            ))
        }

        pub async fn list(state: State<AppState>) -> CliResult<String> {
            let app = state.read().await;

            if app.verbose {
                println!("[DEBUG] Listing storage buckets");
            }

            Ok("Buckets:\n\
                - prod-assets (10GB)\n\
                - backup-storage (50GB)".to_string())
        }
    }

    // Config handlers
    pub mod config {
        use super::*;

        pub async fn show(state: State<AppState>) -> CliResult<String> {
            let app = state.read().await;
            Ok(format!(
                "Configuration:\n\
                 - Config path: {}\n\
                 - API endpoint: {}\n\
                 - Verbose: {}",
                app.config_path, app.api_endpoint, app.verbose
            ))
        }

        #[derive(Parser, Debug)]
        pub struct SetArgs {
            /// Configuration key
            pub key: String,

            /// Configuration value
            pub value: String,
        }

        pub async fn set(state: State<AppState>, Args(args): Args<SetArgs>) -> CliResult<String> {
            let app = state.read().await;

            if app.verbose {
                println!("[DEBUG] Setting config: {} = {}", args.key, args.value);
            }

            Ok(format!("Set {} = {}", args.key, args.value))
        }
    }

    // Version handler
    pub async fn version(_state: State<AppState>) -> CliResult<String> {
        Ok("myctl v1.0.0\nCopyright (c) 2025".to_string())
    }
}

// ============================================
// Router Setup with nest() (New!)
// ============================================

#[sen::sen(
    name = "myctl",
    version = "1.0.0",
    about = "Cloud Resource Management CLI"
)]
fn build_router(state: AppState) -> Router<()> {
    // Create sub-routers for each resource
    let db_router = Router::new()
        .route("create", handlers::db::create())
        .route("list", handlers::db::list())
        .route("delete", handlers::db::delete());

    let server_router = Router::new()
        .route("start", handlers::server::start())
        .route("stop", handlers::server::stop)
        .route("list", handlers::server::list);

    let deploy_router = Router::new()
        .route("app", handlers::deploy::app)
        .route("rollback", handlers::deploy::rollback);

    let network_router = Router::new()
        .route("create", handlers::network::create)
        .route("list", handlers::network::list);

    let storage_router = Router::new()
        .route("upload", handlers::storage::upload)
        .route("list", handlers::storage::list);

    let config_router = Router::new()
        .route("show", handlers::config::show)
        .route("set", handlers::config::set);

    // Compose them with nest() - cleaner and more organized!
    Router::new()
        .nest("db", db_router)
        .nest("server", server_router)
        .nest("deploy", deploy_router)
        .nest("network", network_router)
        .nest("storage", storage_router)
        .nest("config", config_router)
        .route("version", handlers::version)
        .with_state(state)
}

// ============================================
// Main Entry Point
// ============================================

#[tokio::main]
async fn main() {
    init_subscriber();

    // Load global state
    let app_state = match AppState::load() {
        Ok(state) => state,
        Err(e) => {
            eprintln!("{}", format_error(&e));
            std::process::exit(e.exit_code());
        }
    };

    // Parse command line arguments (filter out global flags)
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with("--verbose") && !a.starts_with("-v") && !a.starts_with("--config"))
        .collect();

    // Build router and execute
    let router = build_router(app_state);
    let response = router.execute(&args).await;

    if !response.output.is_empty() {
        println!("{}", response.output);
    }

    std::process::exit(response.exit_code);
}


fn format_error(e: &sen::CliError) -> String {
    match e {
        sen::CliError::User(user_err) => format!("{}", user_err),
        sen::CliError::System(sys_err) => format!("{}", sys_err),
    }
}
