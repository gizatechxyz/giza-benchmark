use cairo_lang_sierra::ProgramParser;
use cairo_stack::{
    prover::{generate_proof_from_trace, verify_proof, write_proof},
    runner::{process_args, run},
};
use stark_platinum_prover::proof::options::{ProofOptions, SecurityLevel};

use crate::{
    data::Benchmark,
    utils::{create_dir, finalize_metrics, start_metrics},
};

pub(crate) async fn benchmark_orion(
    sierra_file: &String,
    program_args: &String,
    benchmark_path: &String,
) -> Benchmark {
    create_dir(benchmark_path);
    let trace_path = format!("{benchmark_path}/program.trace");
    let memory_path = format!("{benchmark_path}/program.memory");
    let proof_path = format!("{benchmark_path}/program.proof");

    let sierra_content = std::fs::read_to_string(sierra_file).expect("Failed to read Sierra file");
    let sierra_program = ProgramParser::new()
        .parse(&sierra_content)
        .expect("Failed to parse Sierra program");
    let args = process_args(program_args).expect("Failed to process provided arguments");

    // ================ RUNNER ================
    let (start_time, mem_before) = start_metrics();
    let _ = run(
        &sierra_program,
        &Some(trace_path.clone().into()),
        &Some(memory_path.clone().into()),
        &args,
    )
    .await
    .map_err(|e| format!("Encountered an error with Cairo runner: {:?}", e));
    let runner_metrics = finalize_metrics(start_time, mem_before);

    // ================ PROVER ================
    let (start_time, mem_before) = start_metrics();
    let proof_options = ProofOptions::new_secure(SecurityLevel::Conjecturable100Bits, 3);
    let Some((proof, pub_inputs)) =
        generate_proof_from_trace(&trace_path.clone(), &memory_path.clone(), &proof_options)
    else {
        panic!("Error generating proof");
    };
    write_proof(proof, pub_inputs, proof_path.clone().clone());
    let prover_metrics = finalize_metrics(start_time, mem_before);

    // ================ VERIFIER ================
    let (start_time, mem_before) = start_metrics();
    let Ok(program_content) = std::fs::read(proof_path.clone()) else {
        eprintln!("Error opening {} file", proof_path.clone());
        panic!("Error opening {} file", proof_path.clone());
    };
    let mut bytes = program_content.as_slice();
    if bytes.len() < 8 {
        eprintln!("Error reading proof from file: {}", proof_path.clone());
        panic!("Error reading proof from file: {}", proof_path.clone());
    }

    let proof_len = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;

    bytes = &bytes[4..];
    if bytes.len() < proof_len {
        eprintln!("Error reading proof from file: {}", proof_path.clone());
        panic!("Error reading proof from file: {}", proof_path.clone());
    }

    let Ok((proof, _)) =
        bincode::serde::decode_from_slice(&bytes[0..proof_len], bincode::config::standard())
    else {
        println!("Error reading proof from file: {}", proof_path.clone());
        panic!("Error reading proof from file: {}", proof_path.clone());
    };
    bytes = &bytes[proof_len..];

    let Ok((pub_inputs, _)) = bincode::serde::decode_from_slice(bytes, bincode::config::standard())
    else {
        println!("Error reading proof from file: {}", proof_path.clone());
        panic!("Error reading proof from file: {}", proof_path.clone());
    };

    verify_proof(proof, pub_inputs, &proof_options);
    let verifier_metrics = finalize_metrics(start_time, mem_before);

    Benchmark {
        runner: Some(runner_metrics),
        prover: prover_metrics,
        verifier: verifier_metrics,
    }
}
