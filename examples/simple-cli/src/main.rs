use sen::{CliResult, State, SenRouter, init_subscriber, version_info, info, error};

// ============================================
// Application State
// ============================================

pub struct AppState {
    pub project_root: String,
    pub config: ProjectConfig,
}

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
// Commands (Router)
// ============================================

#[derive(SenRouter)]
#[sen(state = AppState)]
enum Commands {
    #[sen(handler = handlers::status)]
    Status,

    #[sen(handler = handlers::build)]
    Build(BuildArgs),

    #[sen(handler = handlers::test)]
    Test(TestArgs),
}

pub struct BuildArgs {
    pub release: bool,
}

pub struct TestArgs {
    pub filter: Option<String>,
}

impl Commands {
    fn parse() -> CliResult<Self> {
        let args: Vec<String> = std::env::args().collect();

        if args.len() < 2 {
            return Err(sen::CliError::user(
                "No command specified. Usage: admin <status|build|test|version> [options]",
            ));
        }

        match args[1].as_str() {
            "status" => Ok(Commands::Status),
            "build" => {
                let release = args.get(2).map(|s| s == "--release").unwrap_or(false);
                Ok(Commands::Build(BuildArgs { release }))
            }
            "test" => {
                let filter = args.get(2).cloned();
                Ok(Commands::Test(TestArgs { filter }))
            }
            "--version" | "-V" | "version" => {
                println!("{}", version_info());
                std::process::exit(0);
            }
            cmd => Err(sen::CliError::user(format!(
                "Unknown command '{}'. Use: status, build, test, or --version",
                cmd
            ))),
        }
    }
}

// ============================================
// Handlers
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

    pub async fn build(state: State<AppState>, args: BuildArgs) -> CliResult<String> {
        info!(release = args.release, "Starting build");
        let app = state.read().await;

        let mut cmd = app.config.build_command.clone();
        if args.release {
            cmd.push_str(" --release");
        }

        info!(command = %cmd, "Build command prepared");
        Ok(format!("Would execute: {}", cmd))
    }

    pub async fn test(state: State<AppState>, args: TestArgs) -> CliResult<String> {
        info!(filter = ?args.filter, "Starting tests");
        let app = state.read().await;

        // Demo: validation error
        if args.filter.as_deref() == Some("invalid") {
            error!("Invalid test filter specified");
            return Err(sen::CliError::user("Test filter 'invalid' matches no tests"));
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
// Main Entry Point
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

    // 2. Parse CLI arguments
    let cmd = match Commands::parse() {
        Ok(cmd) => cmd,
        Err(e) => {
            eprintln!("{}", format_error(&e));
            std::process::exit(e.exit_code());
        }
    };

    // 3. Execute command (macro-generated async execute() method)
    let response = cmd.execute(State::new(app_state)).await;

    // 4. Handle response
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
