use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// The background loops (event listener, push worker, scheduler), which must run as long as the
/// server does. If one ends early, by an error or a panic, the whole server is shut down (and
/// exits with an error, so the container is restarted) rather than serving on without it.
#[derive(Debug, Clone)]
pub struct Supervisor {
    tasks: TaskTracker,
    shutdown: CancellationToken,
    failed: Arc<AtomicBool>,
}

impl Supervisor {
    pub fn new(shutdown: CancellationToken) -> Self {
        Self {
            tasks: TaskTracker::new(),
            shutdown,
            failed: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Run `task`, which should only return once the shutdown token is cancelled.
    pub fn spawn<F>(&self, name: &'static str, task: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let inner = tokio::spawn(task);
        let shutdown = self.shutdown.clone();
        let failed = Arc::clone(&self.failed);
        self.tasks.spawn(async move {
            let outcome = inner.await;
            if shutdown.is_cancelled() {
                return;
            }
            match outcome {
                Err(error) if error.is_panic() => {
                    tracing::error!(task = name, "background task panicked; shutting down");
                }
                _ => tracing::error!(task = name, "background task stopped; shutting down"),
            }
            failed.store(true, Ordering::SeqCst);
            shutdown.cancel();
        });
    }

    /// Whether a task ended before shutdown.
    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::SeqCst)
    }

    /// Wait (at most `grace`) for every task to stop; the shutdown token must be cancelled.
    /// Returns whether they all did.
    pub async fn wait(&self, grace: Duration) -> bool {
        self.tasks.close();
        tokio::time::timeout(grace, self.tasks.wait()).await.is_ok()
    }
}

/// Resolves on Ctrl-C, or SIGTERM on Unix (what `docker stop` sends).
pub async fn signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "cannot listen for Ctrl-C");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(error) => {
                tracing::error!(%error, "cannot listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutdown signal received; draining connections");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_task_that_panics_shuts_the_server_down() {
        let shutdown = CancellationToken::new();
        let tasks = Supervisor::new(shutdown.clone());
        let token = shutdown.clone();
        tasks.spawn("steady", async move { token.cancelled().await });
        tasks.spawn("broken", async { panic!("boom") });
        tokio::time::timeout(Duration::from_secs(5), shutdown.cancelled())
            .await
            .expect("the panic cancels the shutdown token");
        assert!(tasks.wait(Duration::from_secs(5)).await);
        assert!(tasks.failed());
    }

    #[tokio::test]
    async fn tasks_that_stop_at_shutdown_are_not_failures() {
        let shutdown = CancellationToken::new();
        let tasks = Supervisor::new(shutdown.clone());
        let token = shutdown.clone();
        tasks.spawn("steady", async move { token.cancelled().await });
        shutdown.cancel();
        assert!(tasks.wait(Duration::from_secs(5)).await);
        assert!(!tasks.failed());
    }
}
