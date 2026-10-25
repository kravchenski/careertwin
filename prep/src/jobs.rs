//! Work that takes a while (the review after a call, a set of questions, a translation) runs in the
//! background while the window stays alive; its result is picked up on the next frame.

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Instant;

pub struct Running {
    pub id: u64,
    pub label: String,
    pub started: Instant,
    /// Whether the person may stop it, as they may a download.
    pub cancellable: bool,
}

pub struct Jobs<T> {
    runtime: tokio::runtime::Handle,
    sender: Sender<(u64, T)>,
    receiver: Receiver<(u64, T)>,
    running: Vec<Running>,
    aborts: HashMap<u64, tokio::task::AbortHandle>,
    next: u64,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl<T: Send + 'static> Jobs<T> {
    pub fn new(runtime: tokio::runtime::Handle, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (sender, receiver) = channel();
        Self { runtime, sender, receiver, running: Vec::new(), aborts: HashMap::new(), next: 1, wake }
    }

    /// Starts `work`; the id comes back with its result.
    pub fn spawn(&mut self, label: impl Into<String>, work: impl Future<Output = T> + Send + 'static) -> u64 {
        self.start(label.into(), false, work)
    }

    /// Like [`Jobs::spawn`], for work the person may stop: no result comes back for it once it is cancelled.
    pub fn spawn_cancellable(
        &mut self,
        label: impl Into<String>,
        work: impl Future<Output = T> + Send + 'static,
    ) -> u64 {
        self.start(label.into(), true, work)
    }

    fn start(&mut self, label: String, cancellable: bool, work: impl Future<Output = T> + Send + 'static) -> u64 {
        let id = self.next;
        self.next += 1;
        self.running.push(Running { id, label, started: Instant::now(), cancellable });
        let sender = self.sender.clone();
        let wake = Arc::clone(&self.wake);
        let task = self.runtime.spawn(async move {
            let result = work.await;
            let _ = sender.send((id, result));
            wake();
        });
        self.aborts.insert(id, task.abort_handle());
        id
    }

    /// Stops a job that is still running. It never reports a result. False if it was not running.
    pub fn cancel(&mut self, id: u64) -> bool {
        let was_running = self.running.iter().any(|job| job.id == id);
        if let Some(handle) = self.aborts.remove(&id) {
            handle.abort();
        }
        self.running.retain(|job| job.id != id);
        was_running
    }

    /// Results that have arrived since the last call.
    pub fn poll(&mut self) -> Vec<(u64, T)> {
        let done: Vec<(u64, T)> = self.receiver.try_iter().collect();
        self.running.retain(|job| !done.iter().any(|(id, _)| *id == job.id));
        for (id, _) in &done {
            self.aborts.remove(id);
        }
        done
    }

    pub fn running(&self) -> &[Running] {
        &self.running
    }

    pub fn is_busy(&self) -> bool {
        !self.running.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn jobs() -> (tokio::runtime::Runtime, Jobs<String>, Arc<AtomicUsize>) {
        let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&wakes);
        let jobs = Jobs::new(
            runtime.handle().clone(),
            Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
        );
        (runtime, jobs, wakes)
    }

    fn wait_for(jobs: &mut Jobs<String>, count: usize) -> Vec<(u64, String)> {
        let mut got = Vec::new();
        for _ in 0..200 {
            got.extend(jobs.poll());
            if got.len() >= count {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        got
    }

    #[test]
    fn a_job_runs_and_its_result_comes_back_with_its_id() {
        let (_runtime, mut jobs, wakes) = jobs();
        let id = jobs.spawn("Review", async { "done".to_owned() });
        assert!(jobs.is_busy() && jobs.running()[0].label == "Review");
        let got = wait_for(&mut jobs, 1);
        assert_eq!(got, vec![(id, "done".to_owned())]);
        assert!(!jobs.is_busy());
        assert!(wakes.load(Ordering::SeqCst) >= 1, "the window is told to redraw");
    }

    #[test]
    fn jobs_finish_independently_and_keep_their_ids() {
        let (_runtime, mut jobs, _) = jobs();
        let slow = jobs.spawn("slow", async {
            tokio::time::sleep(Duration::from_millis(80)).await;
            "slow".to_owned()
        });
        let fast = jobs.spawn("fast", async { "fast".to_owned() });
        let got = wait_for(&mut jobs, 2);
        assert_eq!(got.len(), 2);
        assert!(got.contains(&(slow, "slow".into())) && got.contains(&(fast, "fast".into())));
        assert_ne!(slow, fast);
    }

    #[test]
    fn a_cancelled_job_stops_and_never_reports() {
        let (_runtime, mut jobs, _) = jobs();
        let slow = jobs.spawn_cancellable("download", async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            "never".to_owned()
        });
        assert!(jobs.running()[0].cancellable);
        assert!(jobs.cancel(slow));
        assert!(!jobs.is_busy());
        std::thread::sleep(Duration::from_millis(50));
        assert!(jobs.poll().is_empty());
        assert!(!jobs.cancel(slow), "a second cancel finds nothing running");
    }

    #[test]
    fn polling_with_nothing_ready_is_empty() {
        let (_runtime, mut jobs, _) = jobs();
        assert!(jobs.poll().is_empty());
        assert!(!jobs.is_busy());
    }
}
