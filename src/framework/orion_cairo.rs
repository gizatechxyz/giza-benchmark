use std::path::PathBuf;

use cairo_lang_sierra::program::VersionedProgram;
use cairo_runner::{process_args, run, Args};
use tokio::process::Command;

use crate::{
    data::Benchmark,
    utils::{create_dir, finalize_metrics, start_metrics},
};

pub(crate) async fn benchmark(
    sierra_file: &String,
    program_args: &String,
    benchmark_path: &String,
) -> Benchmark {
    create_dir(benchmark_path);

    let trace_path = format!("{benchmark_path}/program.trace");
    let memory_path = format!("{benchmark_path}/program.memory");
    let proof_path = format!("{benchmark_path}/program.proof");

    let prove_command = format!(
        "platinum-prover prove {} {} {}",
        trace_path, memory_path, proof_path
    );

    let verify_command = format!("platinum-prover verify {} ", proof_path);

    let sierra_content = std::fs::read(sierra_file).expect("Failed to read Sierra file");
    let versioned_program = serde_json::from_slice::<VersionedProgram>(&sierra_content)
        .expect("Failed to create Version Program");
    let program = versioned_program
        .into_v1()
        .expect("Failed to create Program");
    let program = program.program;
    let program_args = process_args(program_args).expect("Failed to process provided arguments");

    let args = Args {
        trace_file: Some(PathBuf::from(trace_path)),
        memory_file: Some(PathBuf::from(memory_path)),
        layout: "all_cairo".to_string(),
        proof_mode: true,
        air_public_input: None,
        air_private_input: None,
        cairo_pie_output: None,
        args: program_args,
        print_output: true,
        append_return_values: false,
    };

    // ================ RUNNER ================
    let (start_time, mem_before) = start_metrics();
    let (output, n_steps) = run(program, args)
        .map_err(|e| format!("Encountered an error with Cairo runner: {:?}", e))
        .expect("Run VM");
    let runner_metrics: crate::data::Metrics = finalize_metrics(start_time, mem_before);

    // ================ PROVER ================
    let (start_time, mem_before) = start_metrics();
    let prove_status = Command::new("sh")
        .arg("-c")
        .arg(&prove_command)
        .status()
        .await
        .expect("failed to execute prove command");
    assert!(prove_status.success(), "Prove command failed");
    let prover_metrics = finalize_metrics(start_time, mem_before);

    // ================ VERIFIER ================
    let (start_time, mem_before) = start_metrics();
    let verify_status = Command::new("sh")
        .arg("-c")
        .arg(&verify_command)
        .status()
        .await
        .expect("failed to execute prove command");
    assert!(verify_status.success(), "Prove command failed");
    let verify_metrics = finalize_metrics(start_time, mem_before);

    println!("\nProgram result: {:?}", output.unwrap());

    Benchmark {
        runner: Some(runner_metrics),
        prover: prover_metrics,
        verifier: verify_metrics,
        n_steps: Some(n_steps),
    }
}
