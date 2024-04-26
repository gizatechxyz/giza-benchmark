use std::path::PathBuf;

use cairo1_run::FuncArg;

#[derive(Debug, Clone, Default)]
pub struct FuncArgs(pub Vec<FuncArg>);

#[derive(Debug)]
pub struct Args {
    pub trace_file: Option<PathBuf>,
    pub memory_file: Option<PathBuf>,
    pub layout: String,
    pub proof_mode: bool,
    pub air_public_input: Option<PathBuf>,
    pub air_private_input: Option<PathBuf>,
    pub cairo_pie_output: Option<PathBuf>,
    pub args: FuncArgs,
    pub print_output: bool,
    pub append_return_values: bool,
}
