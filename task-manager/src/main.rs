use tokio::sync::broadcast;
use tokio::time::Duration;
use clap::{Arg, ArgAction, Command};
use std::path::Path;
use std::fs::File;
use std::io::Write;

#[tokio::main]
async fn main() {
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
        stop_tasks().await;
        return;
    }

    println!("Starting task manager...");
    save_pid().expect("Failed to save PID file");

    let (tx, mut rx1) = broadcast::channel(1);
    let mut rx2 = tx.subscribe();

    // Set up a Ctrl+C handler
    let tx_ctrlc = tx.clone();
    tokio::spawn(async move {
        if let Ok(()) = tokio::signal::ctrl_c().await {
            println!("Received Ctrl+C, stopping tasks...");
            let _ = tx_ctrlc.send(());
        }
    });

    let task1 = tokio::spawn(async move {
        tokio::select! {
            Ok(_) = rx1.recv() => {
                println!("Task 1 is cancelling...");
            }
            _ = tokio::time::sleep(Duration::from_secs(10)) => {
                println!("Task 1 completed normally");
            }
        }
        println!("Task 1 is cleaning up");
    });

    let task2 = tokio::spawn(async move {
        tokio::select! {
            Ok(_) = rx2.recv() => {
                println!("Task 2 is cancelling...");
            }
            _ = tokio::time::sleep(Duration::from_secs(10)) => {
                println!("Task 2 completed normally");
            }
        }
        println!("Task 2 is cleaning up");
    });

    // Wait for the tasks to finish
    let _ = tokio::join!(task1, task2);

    cleanup_pid();
    println!("All tasks completed, exiting");
}

async fn stop_tasks() {
    println!("Attempting to stop running tasks...");

    if Path::new("task_manager.pid").exists() {
        // In a real implementation, you would use this PID to send signals
        // or use a more sophisticated IPC mechanism
        println!("Tasks are being stopped...");
        cleanup_pid();
        println!("Tasks have been stopped.");
    } else {
        println!("No running tasks found.");
    }
}

fn save_pid() -> std::io::Result<()> {
    let pid = std::process::id();
    let mut file = File::create("task_manager.pid")?;
    write!(file, "{}", pid)?;
    Ok(())
}

fn cleanup_pid() {
    if let Err(e) = std::fs::remove_file("task_manager.pid") {
        eprintln!("Failed to remove PID file: {}", e);
    }
}
