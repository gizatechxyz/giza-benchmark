use std::fs;

use clap::{App, Arg};
use data::Metrics;
use tokio;

use crate::consts::BENCHMARK_TITLE; // Ensure tokio is included for the async runtime

mod consts;
mod data;
mod orion;
mod utils;

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

    let benchmark = orion::benchmark_orion(&sierra_file, &program_args, &benchmark_path).await;

    print_metrics_table(&benchmark.runner, &benchmark.prover, &benchmark.verifier);
}

fn print_metrics_table(runner: &Metrics, prover: &Metrics, verifier: &Metrics) {
    println!("\n");
    println!("{}", BENCHMARK_TITLE);
    println!("\n");

    println!("| Type               | Run          | Prove         | Verify       |");
    println!("| ------------------ | ------------ | ------------- | ------------ |");
    println!(
        "| time (ms)          | {:13.6} | {:13.6} | {:13.6} |",
        runner.exec_time * 1000.0,   // Convert seconds to ms
        prover.exec_time * 1000.0,   // Convert seconds to ms
        verifier.exec_time * 1000.0  // Convert seconds to ms
    );
    println!(
        "| memory usage (KB)  | {:12.6} | {:13.6} | {:12.6} |",
        runner.memory_usage, prover.memory_usage, verifier.memory_usage
    );
}
