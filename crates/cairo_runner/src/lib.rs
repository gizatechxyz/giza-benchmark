use bincode::enc::write::Writer;
use cairo_lang_sierra::{
    ids::ConcreteTypeId, program::Program, program_registry::ProgramRegistryError,
};
use cairo_lang_sierra_to_casm::{compiler::CompilationError, metadata::MetadataError};
use cairo_run::Cairo1RunConfig;
use cairo_vm::{
    air_public_input::PublicInputError,
    cairo_run::EncodeTraceError,
    types::errors::program_errors::ProgramError,
    vm::errors::{
        memory_errors::MemoryError, runner_errors::RunnerError, trace_errors::TraceError,
        vm_errors::VirtualMachineError,
    },
    Felt252,
};
use clap::Parser;
use std::{
    io::{self, Write},
    path::PathBuf,
};
use thiserror::Error;

pub mod cairo_run;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
    #[clap(long = "trace_file", value_parser)]
    pub trace_file: Option<PathBuf>,
    #[structopt(long = "memory_file")]
    pub memory_file: Option<PathBuf>,
    #[clap(long = "layout", default_value = "plain", value_parser=validate_layout)]
    pub layout: String,
    #[clap(long = "proof_mode", value_parser)]
    pub proof_mode: bool,
    #[clap(long = "air_public_input", requires = "proof_mode")]
    pub air_public_input: Option<PathBuf>,
    #[clap(
        long = "air_private_input",
        requires_all = ["proof_mode", "trace_file", "memory_file"] 
    )]
    pub air_private_input: Option<PathBuf>,
    #[clap(
        long = "cairo_pie_output",
        // We need to add these air_private_input & air_public_input or else
        // passing cairo_pie_output + either of these without proof_mode will not fail
        conflicts_with_all = ["proof_mode", "air_private_input", "air_public_input"]
    )]
    pub cairo_pie_output: Option<PathBuf>,
    // Arguments should be spaced, with array elements placed between brackets
    // For example " --args '1 2 [1 2 3]'" will yield 3 arguments, with the last one being an array of 3 elements
    #[clap(long = "args", default_value = "", value_parser=process_args)]
    pub args: FuncArgs,
    #[clap(long = "print_output", value_parser)]
    pub print_output: bool,
    #[clap(
        long = "append_return_values",
        // We need to add these air_private_input & air_public_input or else
        // passing cairo_pie_output + either of these without proof_mode will not fail
        conflicts_with_all = ["proof_mode", "air_private_input", "air_public_input"]
    )]
    pub append_return_values: bool,
}

#[derive(Debug, Clone)]
pub enum FuncArg {
    Array(Vec<Felt252>),
    Single(Felt252),
}

#[derive(Debug, Clone, Default)]
pub struct FuncArgs(pub Vec<FuncArg>);

pub fn process_args(value: &str) -> Result<FuncArgs, String> {
    if value.is_empty() {
        return Ok(FuncArgs::default());
    }
    let mut args = Vec::new();
    let mut input = value.split(' ');
    while let Some(value) = input.next() {
        // First argument in an array
        if value.starts_with('[') {
            let mut array_arg =
                vec![Felt252::from_dec_str(value.strip_prefix('[').unwrap()).unwrap()];
            // Process following args in array
            let mut array_end = false;
            while !array_end {
                if let Some(value) = input.next() {
                    // Last arg in array
                    if value.ends_with(']') {
                        array_arg
                            .push(Felt252::from_dec_str(value.strip_suffix(']').unwrap()).unwrap());
                        array_end = true;
                    } else {
                        array_arg.push(Felt252::from_dec_str(value).unwrap())
                    }
                }
            }
            // Finalize array
            args.push(FuncArg::Array(array_arg))
        } else {
            // Single argument
            args.push(FuncArg::Single(Felt252::from_dec_str(value).unwrap()))
        }
    }
    Ok(FuncArgs(args))
}

fn validate_layout(value: &str) -> Result<String, String> {
    match value {
        "plain"
        | "small"
        | "dex"
        | "starknet"
        | "starknet_with_keccak"
        | "recursive_large_output"
        | "all_cairo"
        | "all_solidity"
        | "dynamic" => Ok(value.to_string()),
        _ => Err(format!("{value} is not a valid layout")),
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Invalid arguments")]
    Cli(#[from] clap::Error),
    #[error("Failed to interact with the file system")]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    EncodeTrace(#[from] EncodeTraceError),
    #[error(transparent)]
    VirtualMachine(#[from] VirtualMachineError),
    #[error(transparent)]
    Trace(#[from] TraceError),
    #[error(transparent)]
    PublicInput(#[from] PublicInputError),
    #[error(transparent)]
    Runner(#[from] RunnerError),
    #[error(transparent)]
    ProgramRegistry(#[from] Box<ProgramRegistryError>),
    #[error(transparent)]
    Compilation(#[from] Box<CompilationError>),
    #[error("Failed to compile to sierra:\n {0}")]
    SierraCompilation(String),
    #[error(transparent)]
    Metadata(#[from] MetadataError),
    #[error(transparent)]
    Program(#[from] ProgramError),
    #[error(transparent)]
    Memory(#[from] MemoryError),
    #[error("Program panicked with {0:?}")]
    RunPanic(Vec<Felt252>),
    #[error("Function signature has no return types")]
    NoRetTypesInSignature,
    #[error("No size for concrete type id: {0}")]
    NoTypeSizeForId(ConcreteTypeId),
    #[error("Concrete type id has no debug name: {0}")]
    TypeIdNoDebugName(ConcreteTypeId),
    #[error("No info in sierra program registry for concrete type id: {0}")]
    NoInfoForType(ConcreteTypeId),
    #[error("Failed to extract return values from VM")]
    FailedToExtractReturnValues,
    #[error("Function expects arguments of size {expected} and received {actual} instead.")]
    ArgumentsSizeMismatch { expected: i16, actual: i16 },
    #[error("Function param {param_index} only partially contains argument {arg_index}.")]
    ArgumentUnaligned {
        param_index: usize,
        arg_index: usize,
    },
}

pub struct FileWriter {
    buf_writer: io::BufWriter<std::fs::File>,
    bytes_written: usize,
}

impl Writer for FileWriter {
    fn write(&mut self, bytes: &[u8]) -> Result<(), bincode::error::EncodeError> {
        self.buf_writer
            .write_all(bytes)
            .map_err(|e| bincode::error::EncodeError::Io {
                inner: e,
                index: self.bytes_written,
            })?;

        self.bytes_written += bytes.len();

        Ok(())
    }
}

impl FileWriter {
    fn new(buf_writer: io::BufWriter<std::fs::File>) -> Self {
        Self {
            buf_writer,
            bytes_written: 0,
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.buf_writer.flush()
    }
}

pub fn run(sierra_program: Program, args: Args) -> Result<Option<String>, Error> {
    let cairo_run_config = Cairo1RunConfig {
        proof_mode: args.proof_mode,
        serialize_output: args.print_output,
        relocate_mem: args.memory_file.is_some() || args.air_public_input.is_some(),
        layout: &args.layout,
        trace_enabled: args.trace_file.is_some() || args.air_public_input.is_some(),
        args: &args.args.0,
        finalize_builtins: args.air_private_input.is_some() || args.cairo_pie_output.is_some(),
        append_return_values: args.append_return_values,
    };

    let (runner, vm, _, serialized_output) =
        cairo_run::cairo_run_program(&sierra_program, cairo_run_config)?;

    if let Some(file_path) = args.air_public_input {
        let json = runner.get_air_public_input(&vm)?.serialize_json()?;
        std::fs::write(file_path, json)?;
    }

    if let (Some(file_path), Some(trace_file), Some(memory_file)) = (
        args.air_private_input,
        args.trace_file.clone(),
        args.memory_file.clone(),
    ) {
        // Get absolute paths of trace_file & memory_file
        let trace_path = trace_file
            .as_path()
            .canonicalize()
            .unwrap_or(trace_file.clone())
            .to_string_lossy()
            .to_string();
        let memory_path = memory_file
            .as_path()
            .canonicalize()
            .unwrap_or(memory_file.clone())
            .to_string_lossy()
            .to_string();

        let json = runner
            .get_air_private_input(&vm)
            .to_serializable(trace_path, memory_path)
            .serialize_json()
            .map_err(PublicInputError::Serde)?;
        std::fs::write(file_path, json)?;
    }

    if let Some(ref file_path) = args.cairo_pie_output {
        runner.get_cairo_pie(&vm)?.write_zip_file(file_path)?
    }

    if let Some(trace_path) = args.trace_file {
        let relocated_trace = runner
            .relocated_trace
            .ok_or(Error::Trace(TraceError::TraceNotRelocated))?;
        let trace_file = std::fs::File::create(trace_path)?;
        let mut trace_writer =
            FileWriter::new(io::BufWriter::with_capacity(3 * 1024 * 1024, trace_file));

        cairo_vm::cairo_run::write_encoded_trace(&relocated_trace, &mut trace_writer)?;
        trace_writer.flush()?;
    }
    if let Some(memory_path) = args.memory_file {
        let memory_file = std::fs::File::create(memory_path)?;
        let mut memory_writer =
            FileWriter::new(io::BufWriter::with_capacity(5 * 1024 * 1024, memory_file));

        cairo_vm::cairo_run::write_encoded_memory(&runner.relocated_memory, &mut memory_writer)?;
        memory_writer.flush()?;
    }

    Ok(serialized_output)
}
