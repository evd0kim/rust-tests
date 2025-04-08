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
        .arg(
            Arg::new("t1")
                .long("t1")
                .help("Target task 1")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("t2")
                .long("t2")
                .help("Target task 2")
                .action(ArgAction::SetTrue),
        )
        .get_matches();


    if matches.get_flag("stop") {
        if matches.get_flag("t1") {
            println!("Stopping Task 1...");
            stop_task("t1").await?;
            stop_tasks().await?;
        }

        if matches.get_flag("t2") {
            println!("Stopping Task 2...");
            stop_task("t2").await?;
            stop_tasks().await?;
        }

        // if no specific task was mentioned
        if !matches.get_flag("t1") && !matches.get_flag("t2") {
            println!("No task selected. Use --t1, --t2 or --t3 with --stop.");
        }

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
        if let Err(e) = save_task_pid("task1") {
            error!("Failed to write task2.pid: {}", e);
        }
        tokio::select! {
            Ok(_) = rx1.recv() => {
                info!("Task 1 is cancelling due to shutdown signal");
            }
            _ = tokio::time::sleep(Duration::from_secs(20)) => {
                info!("Task 1 completed normally");
            }
        }
        info!("Task 1 is cleaning up");
    });

    let task2 = tokio::spawn(async move {
        debug!("Task 2 started");
        if let Err(e) = save_task_pid("task2") {
            error!("Failed to write task2.pid: {}", e);
        }
        tokio::select! {
            Ok(_) = rx2.recv() => {
                info!("Task 2 is cancelling due to shutdown signal");
            }
            _ = tokio::time::sleep(Duration::from_secs(20)) => {
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

async fn stop_task(name: &str) -> Result<(), Box<dyn std::error::Error>> {
    // You can implement real shutdown logic here (send to channel, etc.)
    println!("Simulated stopping of task: {}", name);
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

fn save_task_pid(task_name: &str) -> std::io::Result<()> {
    let pid = std::process::id();
    info!("Saving PID {} to file", task_name);
    let filename = format!("{}.pid", task_name);
    let mut file = File::create(filename)?;
    writeln!(file, "{}", pid)?;
    Ok(())
}

fn cleanup_pid() {
    debug!("Removing PID file");
    if let Err(e) = std::fs::remove_file("task_manager.pid") {
        error!("Failed to remove PID file: {}", e);
    }
    if let Err(e) = std::fs::remove_file("task1.pid") {
        error!("Failed to remove PID file: {}", e);
    }
    if let Err(e) = std::fs::remove_file("task2.pid") {
        error!("Failed to remove PID file: {}", e);
    }
}

fn setup_shutdown_handler(
    shutdown_tx: broadcast::Sender<()>,
) -> tokio::task::JoinHandle<()> {
    use tokio::time::{sleep, Duration};
    use std::path::Path;

    // global pid file, should be somewhere else like in app config or something
    let pid_file_path = "task_manager.pid";

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

        // Monitor PID file in a loop
        let pid_file_check = async {
            loop {
                if !Path::new(pid_file_path).exists() {
                    info!("PID file was removed. Initiating shutdown...");
                    break;
                }
                sleep(Duration::from_secs(1)).await;
            }
        };

        tokio::select! {
            _ = ctrl_c => {
                info!("Received Ctrl+C, initiating graceful shutdown");
            }
            _ = terminate => {
                info!("Received SIGTERM, initiating graceful shutdown");
            }
            _ = pid_file_check => {
                info!("PID file check triggered shutdown");
            }
        }

        let _ = shutdown_tx.send(());
    })
}

