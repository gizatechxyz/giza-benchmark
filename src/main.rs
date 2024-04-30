use clap::{App, Arg};
use data::Metrics;
use std::fs;
use tokio;

mod consts;
mod data;
mod framework;
mod utils;

#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[tokio::main]
async fn main() {
    let matches = App::new("Framework Benchmark")
        .version("1.0")
        .author("Your Name <your_email@example.com>")
        .about("Benchmarks performance of different frameworks")
        .arg(
            Arg::with_name("compiled_program")
                .short('p')
                .long("compiled-program")
                .value_name("FILE")
                .help("Sets the path to the compiled program file (.sierra.json)")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("input")
                .short('i')
                .long("input")
                .value_name("FILE")
                .help("Sets the path to the input file")
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

    let compiled_program = matches.value_of("compiled_program").unwrap().to_string();
    let input = matches.value_of("input").unwrap().to_string();
    let benchmark_path = matches.value_of("benchmark_path").unwrap().to_string();

    let program_args =
        fs::read_to_string(&input).expect("Failed to read the program arguments file");
    let benchmark =
        framework::orion_cairo::benchmark(&compiled_program, &program_args, &benchmark_path).await;
    print_metrics_table(
        &benchmark.runner,
        &benchmark.prover,
        &benchmark.verifier,
        benchmark.n_steps,
    );
}

fn print_metrics_table(
    runner: &Option<Metrics>,
    prover: &Metrics,
    verifier: &Metrics,
    n_steps: Option<usize>,
) {
    println!("\n");
    println!("{}", consts::BENCHMARK_TITLE);
    println!("\n");

    println!("| Metric              | Run           | Prove          | Verify         |");
    println!("| ------------------- | ------------- | -------------- | -------------- |");

    // Handle the runner's execution time
    let runner_exec_time = match runner {
        Some(metrics) => format!("{:13.6}", metrics.exec_time * 1000.0), // Convert seconds to ms
        None => "Not Defined".to_string(),
    };

    println!(
        "| time (ms)           | {:<14} | {:13.6} | {:13.6} |",
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
        "| memory usage (KB)   | {:<14} | {:13.6} | {:13.6} |",
        runner_memory_usage, prover.memory_usage, verifier.memory_usage
    );

    // Handle the runner's execution time
    let steps = match n_steps {
        Some(metric) => format!("{:13.6}", metric),
        None => "Not Defined".to_string(),
    };

    println!(
        "| n_steps             | {:<14} | {:<14} | {:<14} |",
        steps, "-", "-"
    );

    println!("\n");
}
