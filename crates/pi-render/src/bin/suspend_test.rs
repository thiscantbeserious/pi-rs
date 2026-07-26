//! Test binary for the SIGTSTP/SIGCONT integration test (P14).
//!
//! Enters a terminal session (writes enter sequences to stdout), installs
//! the suspend handler (which writes restore on SIGTSTP, re-enter on
//! SIGCONT), then sleeps until killed. The integration test
//! (`tests/suspend_integration.rs`) spawns this binary, sends signals,
//! and reads the sequences from stdout.

use std::io::Write;

use pi_render::suspend::{enter_ansi, install_suspend_handler, restore_ansi};

fn main() {
    // Pre-compute the enter and restore byte sequences.
    let enter = enter_ansi(false);
    let restore = restore_ansi(false);

    // Write the enter sequence to stdout so the parent sees it.
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let _ = handle.write_all(&enter);
    let _ = handle.flush();

    // Install the SIGTSTP/SIGCONT handler with the pre-computed bytes.
    let _ = install_suspend_handler(restore, enter);

    // Sleep until killed. The signal handler writes the restore/enter
    // sequences to stdout when SIGTSTP/SIGCONT arrive.
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}
