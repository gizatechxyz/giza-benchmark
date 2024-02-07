use std::fs;

use clap::{App, Arg};
use data::Metrics;
use tokio;

use crate::consts::BENCHMARK_TITLE;

mod consts;
mod data;
mod framework;
mod utils;

#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[tokio::main]
async fn main() {
    let matches = App::new("Orion Benchmark")
        .version("1.0")
        .author("Your Name <your_email@example.com>")
        .about("Benchmarks Orion performance")
        .arg(
            Arg::with_name("sierra_file")
                .short('s')
                .long("sierra-file")
                .value_name("FILE")
                .help("Sets the path to the Sierra file")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("args_file")
                .short('a')
                .long("args-file")
                .value_name("FILE")
                .help("Sets the path to the file containing program arguments")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("benchmark_path")
                .short('b')
                .long("benchmark-path")
                .value_name("PATH")
                .help("Sets the path for benchmark outputs")
                .takes_value(true)
                .required(true),
        )
        .get_matches();

    let sierra_file = matches.value_of("sierra_file").unwrap().to_string();
    let args_file = matches.value_of("args_file").unwrap(); // Changed to match the new argument
    let benchmark_path = matches.value_of("benchmark_path").unwrap().to_string();

    let program_args =
        fs::read_to_string(args_file).expect("Failed to read the program arguments file");

    let benchmark =
        framework::orion::benchmark_orion(&sierra_file, &program_args, &benchmark_path).await;

    print_metrics_table(&benchmark.runner, &benchmark.prover, &benchmark.verifier);
}

fn print_metrics_table(runner: &Option<Metrics>, prover: &Metrics, verifier: &Metrics) {
    println!("\n");
    println!("{}", BENCHMARK_TITLE);
    println!("\n");

    println!("| Type                | Run           | Prove          | Verify         |");
    println!("| ------------------- | ------------- | -------------- | -------------- |");

    // Handle the runner's execution time
    let runner_exec_time = match runner {
        Some(metrics) => format!("{:13.6}", metrics.exec_time * 1000.0), // Convert seconds to ms
        None => "Not Defined".to_string(),
    };

    println!(
        "| time (ms)          | {} | {:13.6} | {:13.6} |",
        runner_exec_time,
        prover.exec_time * 1000.0,   // Convert seconds to ms
        verifier.exec_time * 1000.0  // Convert seconds to ms
    );

    // Handle the runner's memory usage
    let runner_memory_usage = match runner {
        Some(metrics) => format!("{:13.6}", metrics.memory_usage),
        None => "Not Defined".to_string(),
    };

    println!(
        "| memory usage (KB)  | {} | {:13.6} | {:13.6} |",
        runner_memory_usage, prover.memory_usage, verifier.memory_usage
    );
    println!("\n");
}
