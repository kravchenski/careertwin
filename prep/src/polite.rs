//! Heavy work (the voice, the ear, the picture of the sphere) runs at the ordinary priority, not
//! above it. A program started from a compositor inherits the compositor's raised priority, and
//! its busiest threads would then keep the rest of the machine waiting.

/// Where heavy threads run: the ordinary priority.
const WORKER_NICE: i32 = 0;

/// Lowers the calling thread to the ordinary priority. On Linux the priority belongs to the thread,
/// so nothing else in the program is affected.
pub fn lower_this_thread() {
    #[cfg(unix)]
    {
        let _ = rustix::process::setpriority_process(None, WORKER_NICE);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    #[test]
    fn a_thread_that_asks_to_be_polite_ends_up_at_the_ordinary_priority() {
        let nice = std::thread::spawn(|| {
            super::lower_this_thread();
            rustix::process::getpriority_process(None).unwrap()
        })
        .join()
        .unwrap();
        assert_eq!(nice, super::WORKER_NICE);
    }
}
