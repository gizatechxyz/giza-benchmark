use std::io::{self, Write};

use bincode::enc::write::Writer;
use cairo1_run::{cairo_run_program, error::Error, Cairo1RunConfig};
use cairo_lang_sierra::program::Program;
use cairo_vm::{air_public_input::PublicInputError, vm::errors::trace_errors::TraceError};

use crate::{types::Args, utils::layout_str_to_enum};

struct FileWriter {
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

pub fn run(sierra_program: Program, args: Args) -> Result<(Option<String>, usize), Error> {
    let cairo_run_config = Cairo1RunConfig {
        args: &args.args.0,
        serialize_output: args.print_output,
        trace_enabled: args.trace_file.is_some() || args.air_public_input.is_some(),
        relocate_mem: args.memory_file.is_some() || args.air_public_input.is_some(),
        layout: layout_str_to_enum(&args.layout),
        proof_mode: args.proof_mode,
        finalize_builtins: args.air_private_input.is_some() || args.cairo_pie_output.is_some(),
        append_return_values: args.append_return_values,
    };

    let (runner, vm, _, serialized_output) = cairo_run_program(&sierra_program, cairo_run_config)?;
    let resources = runner.get_execution_resources(&vm)?;
    let n_steps = resources.n_steps;    
    
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

    Ok((serialized_output, n_steps))
}
