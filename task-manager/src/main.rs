use clap::{Arg, ArgAction, Command};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tokio::sync::broadcast;
use tokio::time::Duration;
use tracing::{debug, error, info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing subscriber
    tracing_subscriber::fmt::init();

    let matches = Command::new("Task manager")
        .arg(
            Arg::new("stop")
                .short('s')
                .long("stop")
                .help("Stops thread")
                .action(ArgAction::SetTrue),
        )
        .get_matches();

    if matches.get_flag("stop") {
        stop_tasks().await?;
        return Ok(());
    }

    info!("Starting task manager...");
    match save_pid() {
        Ok(_) => debug!("PID file created successfully"),
        Err(e) => {
            error!("Failed to save PID file: {}", e);
            return Err(e.into());
        }
    }

    // Set up shutdown signal handling
    let (shutdown_tx, _) = broadcast::channel::<()>(1);
    let shutdown_signal = setup_shutdown_handler(shutdown_tx.clone());

    // Set up resource management
    setup_resources().await?;

    let (tx, mut rx1) = broadcast::channel(1);
    let mut rx2 = tx.subscribe();

    // Forward shutdown signal to task cancellation
    let shutdown_tx_clone = shutdown_tx.clone();
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        if shutdown_tx_clone.subscribe().recv().await.is_ok() {
            info!("Forwarding shutdown signal to tasks");
            let _ = tx_clone.send(());
        }
    });

    let task1 = tokio::spawn(async move {
        debug!("Task 1 started");
        tokio::select! {
            Ok(_) = rx1.recv() => {
                info!("Task 1 is cancelling due to shutdown signal");
            }
            _ = tokio::time::sleep(Duration::from_secs(10)) => {
                info!("Task 1 completed normally");
            }
        }
        info!("Task 1 is cleaning up");
    });

    let task2 = tokio::spawn(async move {
        debug!("Task 2 started");
        tokio::select! {
            Ok(_) = rx2.recv() => {
                info!("Task 2 is cancelling due to shutdown signal");
            }
            _ = tokio::time::sleep(Duration::from_secs(10)) => {
                info!("Task 2 completed normally");
            }
        }
        info!("Task 2 is cleaning up");
    });

    // Wait for the tasks to finish
    match tokio::join!(task1, task2) {
        (Ok(_), Ok(_)) => info!("All tasks completed successfully"),
        (Err(e), Ok(_)) => error!("Task 1 failed: {}", e),
        (Ok(_), Err(e)) => error!("Task 2 failed: {}", e),
        (Err(e1), Err(e2)) => error!("Both tasks failed. Task 1: {}, Task 2: {}", e1, e2),
    }

    // Clean up resources
    cleanup_resources().await?;

    cleanup_pid();
    info!("All tasks completed, exiting");

    // Wait for shutdown signal to complete (if it hasn't already)
    let _ = shutdown_signal.await;

    Ok(())
}

async fn stop_tasks() -> Result<(), Box<dyn std::error::Error>> {
    info!("Attempting to stop running tasks...");

    if Path::new("task_manager.pid").exists() {
        info!("Tasks are being stopped...");
        cleanup_pid();
        info!("Tasks have been stopped.");
    } else {
        warn!("No running tasks found.");
    }

    Ok(())
}

// Setup resources (database connections, file handles, etc.)
async fn setup_resources() -> Result<(), Box<dyn std::error::Error>> {
    info!("Setting up application resources");

    Ok(())
}

// Clean up resources properly
async fn cleanup_resources() -> Result<(), Box<dyn std::error::Error>> {
    info!("Cleaning up application resources");

    Ok(())
}

fn save_pid() -> std::io::Result<()> {
    let pid = std::process::id();
    debug!("Saving PID {} to file", pid);
    let mut file = File::create("task_manager.pid")?;
    write!(file, "{}", pid)?;
    Ok(())
}

fn cleanup_pid() {
    debug!("Removing PID file");
    if let Err(e) = std::fs::remove_file("task_manager.pid") {
        error!("Failed to remove PID file: {}", e);
    }
}

// Setup shutdown signal handling
fn setup_shutdown_handler(shutdown_tx: broadcast::Sender<()>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let ctrl_c = async {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to install Ctrl+C handler")
        };

        #[cfg(unix)]
        let terminate = async {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("Failed to install signal handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => {
                info!("Received Ctrl+C, initiating graceful shutdown");
            }
            _ = terminate => {
                info!("Received SIGTERM, initiating graceful shutdown");
            }
        }

        let _ = shutdown_tx.send(());
    })
}
