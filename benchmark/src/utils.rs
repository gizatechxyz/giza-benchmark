use std::{fs, time::Instant};

use jemalloc_ctl::{epoch, stats};

use crate::data::Metrics;

pub(crate) fn create_dir(path: &str) {
    if let Err(e) = fs::create_dir_all(path) {
        println!("Error creating directories: {}", e);
    } else {
        println!("Directories created: {}", path);
    }
}

fn update_epoch() -> Result<(), String> {
    epoch::advance().map(|_| ()).map_err(|e| e.to_string())
}

fn get_allocated() -> Result<usize, String> {
    stats::allocated::read().map_err(|e| e.to_string())
}

pub(crate) fn start_metrics() -> (Instant, usize) {
    let start_time = Instant::now();
    update_epoch().expect("Failed to update jemalloc epoch");
    let mem_before = get_allocated().expect("Failed to read allocated memory");

    (start_time, mem_before)
}

pub(crate) fn finalize_metrics(start_time: Instant, mem_before: usize) -> Metrics {
    let end_time = Instant::now();
    let exec_time = end_time.duration_since(start_time).as_secs_f64();

    update_epoch().expect("Failed to update jemalloc epoch");
    let mem_after = get_allocated().expect("Failed to read allocated memory");
    let memory_usage_bytes = mem_after.saturating_sub(mem_before);
    let memory_usage_kb = memory_usage_bytes as f64 / 1024.0; // convert to KB

    Metrics {
        exec_time,
        memory_usage: memory_usage_kb as u64,
    }
}
