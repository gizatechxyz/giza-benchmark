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
            Arg::with_name("framework")
                .short('f')
                .long("framework")
                .value_name("FRAMEWORK")
                .help("Specifies the framework to benchmark ('orion' or 'ezkl')")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("compiled_program")
                .short('p')
                .long("compiled-program")
                .value_name("FILE")
                .help("Sets the path to the compiled program file (.sierra for Orion, .ezkl for EZKL)")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("input")
                .short('i')
                .long("input")
                .value_name("FILE")
                .help("Sets the path to the input file (args_file for Orion, input data for EZKL)")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("settings")
                .short('s')
                .long("settings")
                .value_name("FILE")
                .help("Sets the path to the settings file (only required for EZKL)")
                .takes_value(true)
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

    let framework = matches.value_of("framework").unwrap();
    let compiled_program = matches.value_of("compiled_program").unwrap().to_string();
    let input = matches.value_of("input").unwrap().to_string();
    let benchmark_path = matches.value_of("benchmark_path").unwrap().to_string();

    match framework {
        "orion" => {
            let program_args =
                fs::read_to_string(&input).expect("Failed to read the program arguments file");
            let benchmark = framework::orion::benchmark_orion(
                &compiled_program,
                &program_args,
                &benchmark_path,
            )
            .await;
            print_metrics_table(&benchmark.runner, &benchmark.prover, &benchmark.verifier);
        }
        "ezkl" => {
            let settings = matches
                .value_of("settings")
                .expect("Settings file is required for EZKL benchmarking")
                .to_string();
            let benchmark = framework::ezkl::benchmark_ezkl(
                &compiled_program,
                &input,
                &settings,
                &benchmark_path,
            )
            .await;
            print_metrics_table(&None, &benchmark.prover, &benchmark.verifier); // Assuming there's no runner metrics for EZKL
        }
        _ => println!("Invalid framework specified. Please choose 'orion' or 'ezkl'."),
    }
}

fn print_metrics_table(runner: &Option<Metrics>, prover: &Metrics, verifier: &Metrics) {
    println!("\n");
    println!("{}", consts::BENCHMARK_TITLE);
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
