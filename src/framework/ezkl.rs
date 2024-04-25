use crate::{
    data::Benchmark,
    utils::{create_dir, finalize_metrics, start_metrics},
};
use tokio::process::Command;

pub(crate) async fn benchmark(
    compiled_ezkl: &String,
    input: &String,
    settings: &String,
    benchmark_path: &String,
) -> Benchmark {
    create_dir(benchmark_path);

    let setup_command = format!(
        "ezkl setup -M {} --vk-path={}/vk.key --pk-path={}/pk.key",
        compiled_ezkl, benchmark_path, benchmark_path
    );
    let witness_command = format!(
        "ezkl gen-witness -D {} -M {} -O {}/witness.json",
        input, compiled_ezkl, benchmark_path
    );
    let prove_command = format!("ezkl prove -M {} --witness {}/witness.json --pk-path={}/pk.key --proof-path={}/model.proof", compiled_ezkl, benchmark_path, benchmark_path, benchmark_path);
    let verify_command = format!(
        "ezkl verify --proof-path={}/model.proof --settings-path={} --vk-path={}/vk.key",
        benchmark_path, settings, benchmark_path
    );

    let (start_time, mem_before) = start_metrics();

    // Run the setup command
    let setup_status = Command::new("sh")
        .arg("-c")
        .arg(&setup_command)
        .status()
        .await
        .expect("failed to execute setup command");
    assert!(setup_status.success(), "Setup command failed");

    // Run the generate witness command
    let witness_status = Command::new("sh")
        .arg("-c")
        .arg(&witness_command)
        .status()
        .await
        .expect("failed to execute witness command");
    assert!(witness_status.success(), "Witness command failed");

    // Run the prove command
    let prove_status = Command::new("sh")
        .arg("-c")
        .arg(&prove_command)
        .status()
        .await
        .expect("failed to execute prove command");
    assert!(prove_status.success(), "Prove command failed");

    let prover_metrics = finalize_metrics(start_time, mem_before);

    // Run the verify command and capture metrics
    let (verify_start_time, verify_mem_before) = start_metrics(); // Start metrics for verify command
    let verify_status = Command::new("sh")
        .arg("-c")
        .arg(&verify_command)
        .status()
        .await
        .expect("failed to execute verify command");
    assert!(verify_status.success(), "Verify command failed");
    let verify_metrics = finalize_metrics(verify_start_time, verify_mem_before); // Finalize metrics for verify command

    Benchmark {
        runner: None,
        prover: prover_metrics,
        verifier: verify_metrics,
        n_steps: None,
    }
}
