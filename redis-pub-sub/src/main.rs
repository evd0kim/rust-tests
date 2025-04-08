use mini_redis::{client, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::{
    select, signal, task,
    time::{sleep, Duration},
};

#[tokio::main]
pub async fn main() -> Result<()> {
    let running = Arc::new(AtomicBool::new(true));

    // Clone shutdown signal for subscriber task
    let shutdown_signal = running.clone();

    let processor_handle = task::spawn(async move {
        let client_a = match client::connect("127.0.0.1:6379").await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to connect client A: {:?}", e);
                return;
            }
        };

        let client_b = match client::connect("127.0.0.1:6379").await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to connect client B: {:?}", e);
                return;
            }
        };

        let mut client_output = match client::connect("127.0.0.1:6379").await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to connect Output: {:?}", e);
                return;
            }
        };

        let mut sub_a = match client_a.subscribe(vec!["in_a".into()]).await {
            Ok(sub) => sub,
            Err(e) => {
                eprintln!("Failed to subscribe to in_a: {:?}", e);
                return;
            }
        };

        let mut sub_b = match client_b.subscribe(vec!["in_b".into()]).await {
            Ok(sub) => sub,
            Err(e) => {
                eprintln!("Failed to subscribe to in_b: {:?}", e);
                return;
            }
        };

        if let Err(e) = client_output.set("output", "init".into()).await {
            eprintln!("Error setting data queue Output: {:?}", e);
            return;
        }

        let result = client_output.get("output").await.ok();
        sleep(Duration::from_millis(100)).await;
        println!("Output ready? success={:?}", result.is_some());

        let mut total_sum = 0;

        loop {
            select! {
                msg_a = sub_a.next_message() => {
                    match msg_a {
                        Ok(Some(msg)) => {
                            let content = msg.content;
                            match std::str::from_utf8(&content)
                                .ok()
                                .and_then(|s| s.parse::<i64>().ok()) {
                                Some(num) => {
                                    total_sum += num;
                                    println!("A → Received: {} (sum = {})", num, total_sum);
                                }
                                None => {
                                    println!("A → Skipping non-numeric message: {:?}", content);
                                }
                            }
                        },
                        Ok(None) => break, // Subscription closed
                        Err(e) => {
                            eprintln!("Error receiving message from in_a: {:?}", e);
                        }
                    }
                },
                msg_b = sub_b.next_message() => {
                    match msg_b {
                        Ok(Some(msg)) => {
                            let content = msg.content;
                            match std::str::from_utf8(&content)
                                .ok()
                                .and_then(|s| s.parse::<i64>().ok()) {
                                Some(num) => {
                                    total_sum += num;
                                    println!("B → Received: {} (sum = {})", num, total_sum);
                                }
                                None => {
                                    println!("B → Skipping non-numeric message: {:?}", content);
                                }
                            }
                        },
                        Ok(None) => break,
                        Err(e) => {
                            eprintln!("Error receiving message from in_b: {:?}", e);
                        }
                    }
                },
                _ = signal::ctrl_c() => {
                    println!("Shutting down subscriber gracefully...");
                    shutdown_signal.store(false, Ordering::SeqCst);
                    break;
                }
            }
        }

        let _ = sub_a.unsubscribe(&["in_a".to_string()]).await;
        let _ = sub_b.unsubscribe(&["in_b".to_string()]).await;
    });

    // Give subscriber time to set up
    sleep(Duration::from_millis(100)).await;

    let feed_a_handle = task::spawn(async {
        let mut client = match client::connect("127.0.0.1:6379").await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to connect feed_a: {:?}", e);
                return;
            }
        };

        if let Err(e) = client.set("in_a", "init".into()).await {
            eprintln!("Error setting data queue in_a: {:?}", e);
            return;
        }

        let result = client.get("in_a").await.ok();
        sleep(Duration::from_millis(100)).await;
        println!("Input A ready? success={:?}", result.is_some());

        if let Err(e) = client.publish("in_a", "1".into()).await {
            eprintln!("Failed to publish to in_a: {:?}", e);
        }
    });

    let feed_b_handle = task::spawn(async {
        let mut client = match client::connect("127.0.0.1:6379").await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to connect feed_b: {:?}", e);
                return;
            }
        };

        if let Err(e) = client.set("in_b", "init".into()).await {
            eprintln!("Error setting data queue in_b: {:?}", e);
            return;
        }

        let result = client.get("in_b").await.ok();
        sleep(Duration::from_millis(100)).await;
        println!("Input B ready? success={:?}", result.is_some());

        if let Err(e) = client.publish("in_b", "2".into()).await {
            eprintln!("Failed to publish to in_b: {:?}", e);
        }
    });

    let _ = tokio::join!(processor_handle, feed_a_handle, feed_b_handle);

    println!("Application exited cleanly.");
    Ok(())
}
