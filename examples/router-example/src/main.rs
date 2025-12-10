use sen::{CliResult, State, Router, Args, FromArgs, CliError, init_subscriber, info, error};

// ============================================
// Application State
// ============================================

#[derive(Clone)]
pub struct AppState {
    pub project_root: String,
    pub config: ProjectConfig,
}

#[derive(Clone)]
pub struct ProjectConfig {
    pub build_command: String,
    pub test_command: String,
}

impl AppState {
    fn load() -> CliResult<Self> {
        let project_root = std::env::current_dir()
            .map_err(|e| sen::CliError::system(format!("Failed to get current directory: {}", e)))?
            .to_string_lossy()
            .to_string();

        Ok(Self {
            project_root,
            config: ProjectConfig {
                build_command: "cargo build".to_string(),
                test_command: "cargo test".to_string(),
            },
        })
    }
}

// ============================================
// Argument Types (using FromArgs trait)
// ============================================

#[derive(Debug)]
pub struct BuildArgs {
    pub release: bool,
    pub target: Option<String>,
}

impl FromArgs for BuildArgs {
    fn from_args(args: &[String]) -> Result<Self, CliError> {
        let mut release = false;
        let mut target = None;

        for arg in args {
            if arg == "--release" {
                release = true;
            } else if arg.starts_with("--target=") {
                target = Some(arg.strip_prefix("--target=").unwrap().to_string());
            }
        }

        Ok(BuildArgs { release, target })
    }
}

#[derive(Debug)]
pub struct TestArgs {
    pub filter: Option<String>,
}

impl FromArgs for TestArgs {
    fn from_args(args: &[String]) -> Result<Self, CliError> {
        let filter = args.first().cloned();
        Ok(TestArgs { filter })
    }
}

// ============================================
// Handlers (Axum-style with Args<T> extractor)
// ============================================

mod handlers {
    use super::*;

    pub async fn status(state: State<AppState>) -> CliResult<String> {
        info!("Fetching project status");
        let app = state.read().await;

        Ok(format!(
            "Project Status\n\
             ==============\n\
             Root: {}\n\
             Build: {}\n\
             Test: {}\n",
            app.project_root, app.config.build_command, app.config.test_command,
        ))
    }

    // Handler with State + Args extractor (Axum-style!)
    pub async fn build(state: State<AppState>, Args(args): Args<BuildArgs>) -> CliResult<String> {
        info!(release = args.release, target = ?args.target, "Starting build");
        let app = state.read().await;

        let mut cmd = app.config.build_command.clone();
        if args.release {
            cmd.push_str(" --release");
        }
        if let Some(target) = &args.target {
            cmd.push_str(&format!(" --target={}", target));
        }

        info!(command = %cmd, "Build command prepared");
        Ok(format!("Would execute: {}", cmd))
    }

    // Handler with State + Args extractor
    pub async fn test(state: State<AppState>, Args(args): Args<TestArgs>) -> CliResult<String> {
        info!(filter = ?args.filter, "Starting tests");
        let app = state.read().await;

        // Demo: validation error
        if args.filter.as_deref() == Some("invalid") {
            error!("Invalid test filter specified");
            return Err(CliError::user("Test filter 'invalid' matches no tests"));
        }

        let mut cmd = app.config.test_command.clone();
        if let Some(filter) = &args.filter {
            cmd.push_str(&format!(" {}", filter));
        }

        info!(command = %cmd, "Test command prepared");
        Ok(format!("Would execute: {}", cmd))
    }
}

// ============================================
// Main Entry Point (Router-based)
// ============================================

#[tokio::main]
async fn main() {
    // 0. Initialize tracing (controlled by RUST_LOG environment variable)
    init_subscriber();

    // 1. Load application state
    let app_state = match AppState::load() {
        Ok(state) => state,
        Err(e) => {
            eprintln!("{}", format_error(&e));
            std::process::exit(e.exit_code());
        }
    };

    // 2. Build Router (Axum-style)
    let router = Router::new()
        .route("status", handlers::status)
        .route("build", handlers::build)
        .route("test", handlers::test)
        .with_state(app_state);

    // 3. Execute command via Router (reads from env::args automatically)
    let response = router.execute().await;

    // 5. Handle response
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
