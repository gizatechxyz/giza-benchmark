mod prover;
mod runner;
use cairo_lang_sierra::ProgramParser;
use prover::{generate_proof_from_trace, verify_proof, write_proof};
use runner::{process_args, run};
use stark_platinum_prover::proof::options::{ProofOptions, SecurityLevel};
use tracing::info;

use std::env;

#[tokio::main]

async fn main() {
    let proof_options = ProofOptions::new_secure(SecurityLevel::Conjecturable100Bits, 3);

    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        info!("Usage: cargo run <command> [arguments]");
        return;
    }

    let command = &args[1];

    match command.as_str() {
        "run" => {
            let sierra_file = &args[2];
            let program_args = &args[3];

            let sierra_content =
                std::fs::read_to_string(sierra_file).expect("Failed to read Sierra file");

            let sierra_program = ProgramParser::new()
                .parse(&sierra_content)
                .expect("Failed to parse Sierra program");

            let args = process_args(program_args).expect("Failed to process provided arguments");

            match run(&sierra_program, &None, &None, &args).await {
                Ok(result) => println!("✅ Program executed successfully: {:?}", result),
                Err(e) => eprintln!("Error executing program: {:?}", e),
            }
        }
        "prove" => {
            let trace_path = &args[2];
            let memory_path = &args[3];
            let output_path = &args[4];

            let Some((proof, pub_inputs)) =
                generate_proof_from_trace(trace_path, memory_path, &proof_options)
            else {
                return;
            };

            write_proof(proof, pub_inputs, output_path.to_string());
        }
        "verify" => {
            let proof_path = &args[2];

            let Ok(program_content) = std::fs::read(proof_path) else {
                eprintln!("Error opening {} file", proof_path);
                return;
            };
            let mut bytes = program_content.as_slice();
            if bytes.len() < 8 {
                eprintln!("Error reading proof from file: {}", proof_path);
                return;
            }

            let proof_len = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;

            bytes = &bytes[4..];
            if bytes.len() < proof_len {
                eprintln!("Error reading proof from file: {}", proof_path);
                return;
            }

            let Ok((proof, _)) = bincode::serde::decode_from_slice(
                &bytes[0..proof_len],
                bincode::config::standard(),
            ) else {
                println!("Error reading proof from file: {}", proof_path);
                return;
            };
            bytes = &bytes[proof_len..];

            let Ok((pub_inputs, _)) =
                bincode::serde::decode_from_slice(bytes, bincode::config::standard())
            else {
                println!("Error reading proof from file: {}", proof_path);
                return;
            };

            verify_proof(proof, pub_inputs, &proof_options);
        }
        _ => {
            eprintln!("Unsupported command: {}", command);
        }
    }
}
