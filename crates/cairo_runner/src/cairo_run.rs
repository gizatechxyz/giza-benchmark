use cairo_lang_casm::{casm, casm_extend, hints::Hint, instructions::Instruction};
use cairo_lang_sierra::{
    extensions::{
        bitwise::BitwiseType,
        core::{CoreLibfunc, CoreType},
        ec::EcOpType,
        gas::{CostTokenType, GasBuiltinType},
        pedersen::PedersenType,
        poseidon::PoseidonType,
        range_check::RangeCheckType,
        segment_arena::SegmentArenaType,
        starknet::syscalls::SystemType,
        ConcreteType, NamedType,
    },
    ids::ConcreteTypeId,
    program::{Function, GenericArg, Program as SierraProgram},
    program_registry::ProgramRegistry,
};
use cairo_lang_sierra_ap_change::calc_ap_changes;
use cairo_lang_sierra_gas::{gas_info::GasInfo, objects::CostInfoProvider};
use cairo_lang_sierra_to_casm::{
    compiler::{CairoProgram, SierraToCasmConfig},
    metadata::{calc_metadata, Metadata, MetadataComputationConfig, MetadataError},
};
use cairo_lang_sierra_type_size::get_type_size_map;
use cairo_lang_utils::unordered_hash_map::UnorderedHashMap;
use cairo_vm::{
    hint_processor::cairo_1_hint_processor::hint_processor::Cairo1HintProcessor,
    math_utils::signed_felt,
    serde::deserialize_program::{
        ApTracking, BuiltinName, FlowTrackingData, HintParams, ReferenceManager,
    },
    types::{program::Program, relocatable::MaybeRelocatable},
    vm::{
        errors::{runner_errors::RunnerError, vm_errors::VirtualMachineError},
        runners::{
            builtin_runner::{
                BITWISE_BUILTIN_NAME, EC_OP_BUILTIN_NAME, HASH_BUILTIN_NAME, OUTPUT_BUILTIN_NAME,
                POSEIDON_BUILTIN_NAME, RANGE_CHECK_BUILTIN_NAME, SIGNATURE_BUILTIN_NAME,
            },
            cairo_runner::{CairoRunner, RunResources, RunnerMode},
        },
        vm_core::VirtualMachine,
    },
    Felt252,
};
use itertools::{chain, Itertools};
use num_traits::{cast::ToPrimitive, Zero};
use std::{collections::HashMap, iter::Peekable, slice::Iter};

use crate::{Error, FuncArg};

#[derive(Debug)]
pub struct Cairo1RunConfig<'a> {
    pub args: &'a [FuncArg],
    // Serializes program output into a user-friendly format
    pub serialize_output: bool,
    pub trace_enabled: bool,
    pub relocate_mem: bool,
    pub layout: &'a str,
    pub proof_mode: bool,
    // Should be true if either air_public_input or cairo_pie_output are needed
    // Sets builtins stop_ptr by calling `final_stack` on each builtin
    pub finalize_builtins: bool,
    // Appends return values to the output segment. This is performed by default when running in proof_mode
    pub append_return_values: bool,
}

impl Default for Cairo1RunConfig<'_> {
    fn default() -> Self {
        Self {
            args: Default::default(),
            serialize_output: false,
            trace_enabled: false,
            relocate_mem: false,
            layout: "plain",
            proof_mode: false,
            finalize_builtins: false,
            append_return_values: false,
        }
    }
}

// Runs a Cairo 1 program
// Returns the runner & VM after execution + the return values + the serialized return values (if serialize_output is enabled)
pub fn cairo_run_program(
    sierra_program: &SierraProgram,
    cairo_run_config: Cairo1RunConfig,
) -> Result<
    (
        CairoRunner,
        VirtualMachine,
        Vec<MaybeRelocatable>,
        Option<String>,
    ),
    Error,
> {
    let metadata = create_metadata(sierra_program, Some(Default::default()))?;
    let sierra_program_registry = ProgramRegistry::<CoreType, CoreLibfunc>::new(sierra_program)?;
    let type_sizes =
        get_type_size_map(sierra_program, &sierra_program_registry).unwrap_or_default();

    let sierra_to_casm_config = SierraToCasmConfig {
        gas_usage_check: false,
        max_bytecode_size: 4_089_446,
    };

    let casm_program = cairo_lang_sierra_to_casm::compiler::compile(
        sierra_program,
        &metadata,
        sierra_to_casm_config,
    )?;

    let main_func = find_function(sierra_program, "::main")?;

    let initial_gas = 9999999999999_usize;

    // Modified entry code to be compatible with custom cairo1 Proof Mode.
    // This adds code that's needed for dictionaries, adjusts ap for builtin pointers, adds initial gas for the gas builtin if needed, and sets up other necessary code for cairo1
    let (entry_code, builtins) = create_entry_code(
        &sierra_program_registry,
        &casm_program,
        &type_sizes,
        main_func,
        initial_gas,
        cairo_run_config.proof_mode || cairo_run_config.append_return_values,
        cairo_run_config.args,
    )?;

    // Fetch return type data
    let return_type_id = main_func
        .signature
        .ret_types
        .last()
        .ok_or(Error::NoRetTypesInSignature)?;
    let return_type_size = type_sizes
        .get(return_type_id)
        .cloned()
        .ok_or_else(|| Error::NoTypeSizeForId(return_type_id.clone()))?;

    // This footer is used by lib funcs
    let libfunc_footer = create_code_footer();

    // Header used to initiate the infinite loop after executing the program
    // Also appends return values to output segment
    let proof_mode_header = if cairo_run_config.proof_mode {
        create_proof_mode_header(builtins.len() as i16, return_type_size)
    } else if cairo_run_config.append_return_values {
        create_append_return_values_header(builtins.len() as i16, return_type_size)
    } else {
        casm! {}.instructions
    };

    // This is the program we are actually running/proving
    // With (embedded proof mode), cairo1 header and the libfunc footer
    let instructions = chain!(
        proof_mode_header.iter(),
        entry_code.iter(),
        casm_program.instructions.iter(),
        libfunc_footer.iter(),
    );

    let (processor_hints, program_hints) = build_hints_vec(instructions.clone());

    let mut hint_processor = Cairo1HintProcessor::new(&processor_hints, RunResources::default());

    let data: Vec<MaybeRelocatable> = instructions
        .flat_map(|inst| inst.assemble().encode())
        .map(|x| Felt252::from(&x))
        .map(MaybeRelocatable::from)
        .collect();

    let data_len = data.len();

    let program = if cairo_run_config.proof_mode {
        Program::new_for_proof(
            builtins,
            data,
            0,
            // Proof mode is on top
            // jmp rel 0 is on PC == 2
            2,
            program_hints,
            ReferenceManager {
                references: Vec::new(),
            },
            HashMap::new(),
            vec![],
            None,
        )?
    } else {
        Program::new(
            builtins,
            data,
            Some(0),
            program_hints,
            ReferenceManager {
                references: Vec::new(),
            },
            HashMap::new(),
            vec![],
            None,
        )?
    };

    let runner_mode = if cairo_run_config.proof_mode {
        RunnerMode::ProofModeCairo1
    } else {
        RunnerMode::ExecutionMode
    };

    let mut runner = CairoRunner::new_v2(&program, cairo_run_config.layout, runner_mode)?;
    let mut vm = VirtualMachine::new(cairo_run_config.trace_enabled);
    let end = runner.initialize(&mut vm, cairo_run_config.proof_mode)?;

    additional_initialization(&mut vm, data_len)?;

    // Run it until the end / infinite loop in proof_mode
    runner.run_until_pc(end, &mut vm, &mut hint_processor)?;
    if cairo_run_config.proof_mode {
        runner.run_for_steps(1, &mut vm, &mut hint_processor)?;
    }

    if cairo_run_config.proof_mode || cairo_run_config.append_return_values {
        // As we will be inserting the return values into the output segment after running the main program (right before the infinite loop) the computed size for the output builtin will be 0
        // We need to manually set the segment size for the output builtin's segment so memory hole counting doesn't fail due to having a higher accessed address count than the segment's size
        vm.segments
            .segment_sizes
            .insert(2, return_type_size as usize);
    }
    runner.end_run(false, false, &mut vm, &mut hint_processor)?;

    // Fetch return values
    let return_values = fetch_return_values(return_type_size, return_type_id, &vm)?;

    let serialized_output = if cairo_run_config.serialize_output {
        Some(serialize_output(
            &return_values,
            &mut vm,
            return_type_id,
            &sierra_program_registry,
            &type_sizes,
        ))
    } else {
        None
    };

    // Set stop pointers for builtins so we can obtain the air public input
    if cairo_run_config.finalize_builtins {
        finalize_builtins(
            cairo_run_config.proof_mode || cairo_run_config.append_return_values,
            &main_func.signature.ret_types,
            &type_sizes,
            &mut vm,
        )?;

        if cairo_run_config.proof_mode || cairo_run_config.append_return_values {
            // As the output builtin is not used by the program we need to compute it's stop ptr manually
            vm.set_output_stop_ptr_offset(return_type_size as usize);
        }

        // Build execution public memory
        if cairo_run_config.proof_mode {
            runner.finalize_segments(&mut vm)?;
        }
    }

    runner.relocate(&mut vm, true)?;

    Ok((runner, vm, return_values, serialized_output))
}

fn additional_initialization(vm: &mut VirtualMachine, data_len: usize) -> Result<(), Error> {
    // Create the builtin cost segment
    let builtin_cost_segment = vm.add_memory_segment();
    for token_type in CostTokenType::iter_precost() {
        vm.insert_value(
            (builtin_cost_segment + (token_type.offset_in_builtin_costs() as usize))
                .map_err(VirtualMachineError::Math)?,
            Felt252::default(),
        )?
    }
    // Put a pointer to the builtin cost segment at the end of the program (after the
    // additional `ret` statement).
    vm.insert_value(
        (vm.get_pc() + data_len).map_err(VirtualMachineError::Math)?,
        builtin_cost_segment,
    )?;

    Ok(())
}

#[allow(clippy::type_complexity)]
fn build_hints_vec<'b>(
    instructions: impl Iterator<Item = &'b Instruction>,
) -> (Vec<(usize, Vec<Hint>)>, HashMap<usize, Vec<HintParams>>) {
    let mut hints: Vec<(usize, Vec<Hint>)> = Vec::new();
    let mut program_hints: HashMap<usize, Vec<HintParams>> = HashMap::new();

    let mut hint_offset = 0;

    for instruction in instructions {
        if !instruction.hints.is_empty() {
            hints.push((hint_offset, instruction.hints.clone()));
            program_hints.insert(
                hint_offset,
                vec![HintParams {
                    code: hint_offset.to_string(),
                    accessible_scopes: Vec::new(),
                    flow_tracking_data: FlowTrackingData {
                        ap_tracking: ApTracking::default(),
                        reference_ids: HashMap::new(),
                    },
                }],
            );
        }
        hint_offset += instruction.body.op_size();
    }
    (hints, program_hints)
}

/// Finds first function ending with `name_suffix`.
fn find_function<'a>(
    sierra_program: &'a SierraProgram,
    name_suffix: &'a str,
) -> Result<&'a Function, RunnerError> {
    sierra_program
        .funcs
        .iter()
        .find(|f| {
            if let Some(name) = &f.id.debug_name {
                name.ends_with(name_suffix)
            } else {
                false
            }
        })
        .ok_or_else(|| RunnerError::MissingMain)
}

/// Creates a list of instructions that will be appended to the program's bytecode.
fn create_code_footer() -> Vec<Instruction> {
    casm! {
        // Add a `ret` instruction used in libfuncs that retrieve the current value of the `fp`
        // and `pc` registers.
        ret;
    }
    .instructions
}

// Create proof_mode specific instructions
// Including the "canonical" proof mode instructions (the ones added by the compiler in cairo 0)
// wich call the firt program instruction and then initiate an infinite loop.
// And also appending the return values to the output builtin's memory segment
fn create_proof_mode_header(builtin_count: i16, return_type_size: i16) -> Vec<Instruction> {
    // As the output builtin is not used by cairo 1 (we forced it for this purpose), it's segment is always empty
    // so we can start writing values directly from it's base, which is located relative to the fp before the other builtin's bases
    let output_fp_offset: i16 = -(builtin_count + 2); // The 2 here represents the return_fp & end segments

    // The pc offset where the original program should start
    // Without this header it should start at 0, but we add 2 for each call and jump instruction (as both of them use immediate values)
    // and also 1 for each instruction added to copy each return value into the output segment
    let program_start_offset: i16 = 4 + return_type_size;

    let mut ctx = casm! {};
    casm_extend! {ctx,
        call rel program_start_offset; // Begin program execution by calling the first instruction in the original program
    };
    // Append each return value to the output segment
    for (i, j) in (1..return_type_size + 1).rev().enumerate() {
        casm_extend! {ctx,
            // [ap -j] is where each return value is located in memory
            // [[fp + output_fp_offet] + 0] is the base of the output segment
            [ap - j] = [[fp + output_fp_offset] + i as i16];
        };
    }
    casm_extend! {ctx,
        jmp rel 0; // Infinite loop
    };
    ctx.instructions
}

// Create specific instructions to append the return values to the output segment when not running in proof_mode
// Call the firt program instruction, appends the return values to the output builtin's memory segment and then returns
fn create_append_return_values_header(
    builtin_count: i16,
    return_type_size: i16,
) -> Vec<Instruction> {
    // As the output builtin is not used by cairo 1 (we forced it for this purpose), it's segment is always empty
    // so we can start writing values directly from it's base, which is located relative to the fp before the other builtin's bases
    let output_fp_offset: i16 = -(builtin_count + 2); // The 2 here represents the return_fp & end segments

    // The pc offset where the original program should start
    // Without this header it should start at 0, but we add 2 for the call and 1 for the return instruction
    // and also 1 for each instruction added to copy each return value into the output segment
    let program_start_offset: i16 = 3 + return_type_size;

    let mut ctx = casm! {};
    casm_extend! {ctx,
        call rel program_start_offset; // Begin program execution by calling the first instruction in the original program
    };
    // Append each return value to the output segment
    for (i, j) in (1..return_type_size + 1).rev().enumerate() {
        casm_extend! {ctx,
            // [ap -j] is where each return value is located in memory
            // [[fp + output_fp_offet] + 0] is the base of the output segment
            [ap - j] = [[fp + output_fp_offset] + i as i16];
        };
    }
    casm_extend! {ctx,
        ret;
    };
    ctx.instructions
}

/// Returns the instructions to add to the beginning of the code to successfully call the main
/// function, as well as the builtins required to execute the program.
fn create_entry_code(
    sierra_program_registry: &ProgramRegistry<CoreType, CoreLibfunc>,
    casm_program: &CairoProgram,
    type_sizes: &UnorderedHashMap<ConcreteTypeId, i16>,
    func: &Function,
    initial_gas: usize,
    append_output: bool,
    args: &[FuncArg],
) -> Result<(Vec<Instruction>, Vec<BuiltinName>), Error> {
    let mut ctx = casm! {};
    // The builtins in the formatting expected by the runner.
    let (builtins, builtin_offset) = get_function_builtins(func, append_output);

    // Load all vecs to memory.
    // Load all array args content to memory.
    let mut array_args_data = vec![];
    let mut ap_offset: i16 = 0;
    for arg in args {
        let FuncArg::Array(values) = arg else {
            continue;
        };
        array_args_data.push(ap_offset);
        casm_extend! {ctx,
            %{ memory[ap + 0] = segments.add() %}
            ap += 1;
        }
        for (i, v) in values.iter().enumerate() {
            let arr_at = (i + 1) as i16;
            casm_extend! {ctx,
                [ap + 0] = (v.to_bigint());
                [ap + 0] = [[ap - arr_at] + (i as i16)], ap++;
            };
        }
        ap_offset += (1 + values.len()) as i16;
    }
    let mut array_args_data_iter = array_args_data.iter();
    let after_arrays_data_offset = ap_offset;
    let mut arg_iter = args.iter().enumerate();
    let mut param_index = 0;
    let mut expected_arguments_size = 0;
    if func.signature.param_types.iter().any(|ty| {
        get_info(sierra_program_registry, ty)
            .map(|x| x.long_id.generic_id == SegmentArenaType::ID)
            .unwrap_or_default()
    }) {
        casm_extend! {ctx,
            // SegmentArena segment.
            %{ memory[ap + 0] = segments.add() %}
            // Infos segment.
            %{ memory[ap + 1] = segments.add() %}
            ap += 2;
            [ap + 0] = 0, ap++;
            // Write Infos segment, n_constructed (0), and n_destructed (0) to the segment.
            [ap - 2] = [[ap - 3]];
            [ap - 1] = [[ap - 3] + 1];
            [ap - 1] = [[ap - 3] + 2];
        }
        ap_offset += 3;
    }
    for ty in func.signature.param_types.iter() {
        let info = get_info(sierra_program_registry, ty)
            .ok_or_else(|| Error::NoInfoForType(ty.clone()))?;
        let generic_ty = &info.long_id.generic_id;
        if let Some(offset) = builtin_offset.get(generic_ty) {
            let mut offset = *offset;
            if append_output {
                // Everything is off by 2 due to the proof mode header
                offset += 2;
            }
            casm_extend! {ctx,
                [ap + 0] = [fp - offset], ap++;
            }
            ap_offset += 1;
        } else if generic_ty == &SystemType::ID {
            casm_extend! {ctx,
                %{ memory[ap + 0] = segments.add() %}
                ap += 1;
            }
            ap_offset += 1;
        } else if generic_ty == &GasBuiltinType::ID {
            casm_extend! {ctx,
                [ap + 0] = initial_gas, ap++;
            }
            ap_offset += 1;
        } else if generic_ty == &SegmentArenaType::ID {
            let offset = -ap_offset + after_arrays_data_offset;
            casm_extend! {ctx,
                [ap + 0] = [ap + offset] + 3, ap++;
            }
            ap_offset += 1;
        } else {
            let ty_size = type_sizes[ty];
            let param_ap_offset_end = ap_offset + ty_size;
            expected_arguments_size += ty_size;
            while ap_offset < param_ap_offset_end {
                let Some((arg_index, arg)) = arg_iter.next() else {
                    break;
                };
                match arg {
                    FuncArg::Single(value) => {
                        casm_extend! {ctx,
                            [ap + 0] = (value.to_bigint()), ap++;
                        }
                        ap_offset += 1;
                    }
                    FuncArg::Array(values) => {
                        let offset = -ap_offset + array_args_data_iter.next().unwrap();
                        casm_extend! {ctx,
                            [ap + 0] = [ap + (offset)], ap++;
                            [ap + 0] = [ap - 1] + (values.len()), ap++;
                        }
                        ap_offset += 2;
                        if ap_offset > param_ap_offset_end {
                            return Err(Error::ArgumentUnaligned {
                                param_index,
                                arg_index,
                            });
                        }
                    }
                }
            }
            param_index += 1;
        };
    }
    let actual_args_size = args
        .iter()
        .map(|arg| match arg {
            FuncArg::Single(_) => 1,
            FuncArg::Array(_) => 2,
        })
        .sum::<i16>();
    if expected_arguments_size != actual_args_size {
        return Err(Error::ArgumentsSizeMismatch {
            expected: expected_arguments_size,
            actual: actual_args_size,
        });
    }

    let before_final_call = ctx.current_code_offset;
    let final_call_size = 3;
    let offset = final_call_size
        + casm_program.debug_info.sierra_statement_info[func.entry_point.0].code_offset;

    casm_extend! {ctx,
        call rel offset;
        ret;
    }
    assert_eq!(before_final_call + final_call_size, ctx.current_code_offset);

    Ok((ctx.instructions, builtins))
}

fn get_info<'a>(
    sierra_program_registry: &'a ProgramRegistry<CoreType, CoreLibfunc>,
    ty: &'a cairo_lang_sierra::ids::ConcreteTypeId,
) -> Option<&'a cairo_lang_sierra::extensions::types::TypeInfo> {
    sierra_program_registry
        .get_type(ty)
        .ok()
        .map(|ctc| ctc.info())
}

/// Creates the metadata required for a Sierra program lowering to casm.
fn create_metadata(
    sierra_program: &cairo_lang_sierra::program::Program,
    metadata_config: Option<MetadataComputationConfig>,
) -> Result<Metadata, VirtualMachineError> {
    if let Some(metadata_config) = metadata_config {
        calc_metadata(sierra_program, metadata_config).map_err(|err| match err {
            MetadataError::ApChangeError(_) => VirtualMachineError::Unexpected,
            MetadataError::CostError(_) => VirtualMachineError::Unexpected,
        })
    } else {
        Ok(Metadata {
            ap_change_info: calc_ap_changes(sierra_program, |_, _| 0)
                .map_err(|_| VirtualMachineError::Unexpected)?,
            gas_info: GasInfo {
                variable_values: Default::default(),
                function_costs: Default::default(),
            },
        })
    }
}

/// Type representing the Output builtin.
#[derive(Default)]
pub struct OutputType {}
impl cairo_lang_sierra::extensions::NoGenericArgsGenericType for OutputType {
    const ID: cairo_lang_sierra::ids::GenericTypeId =
        cairo_lang_sierra::ids::GenericTypeId::new_inline("Output");
    const STORABLE: bool = true;
    const DUPLICATABLE: bool = false;
    const DROPPABLE: bool = false;
    const ZERO_SIZED: bool = false;
}

fn get_function_builtins(
    func: &Function,
    append_output: bool,
) -> (
    Vec<BuiltinName>,
    HashMap<cairo_lang_sierra::ids::GenericTypeId, i16>,
) {
    let entry_params = &func.signature.param_types;
    let mut builtins = Vec::new();
    let mut builtin_offset: HashMap<cairo_lang_sierra::ids::GenericTypeId, i16> = HashMap::new();
    let mut current_offset = 3;
    // Fetch builtins from the entry_params in the standard order
    if entry_params
        .iter()
        .any(|ti| ti.debug_name == Some("Poseidon".into()))
    {
        builtins.push(BuiltinName::poseidon);
        builtin_offset.insert(PoseidonType::ID, current_offset);
        current_offset += 1;
    }
    if entry_params
        .iter()
        .any(|ti| ti.debug_name == Some("EcOp".into()))
    {
        builtins.push(BuiltinName::ec_op);
        builtin_offset.insert(EcOpType::ID, current_offset);
        current_offset += 1
    }
    if entry_params
        .iter()
        .any(|ti| ti.debug_name == Some("Bitwise".into()))
    {
        builtins.push(BuiltinName::bitwise);
        builtin_offset.insert(BitwiseType::ID, current_offset);
        current_offset += 1;
    }
    if entry_params
        .iter()
        .any(|ti| ti.debug_name == Some("RangeCheck".into()))
    {
        builtins.push(BuiltinName::range_check);
        builtin_offset.insert(RangeCheckType::ID, current_offset);
        current_offset += 1;
    }
    if entry_params
        .iter()
        .any(|ti| ti.debug_name == Some("Pedersen".into()))
    {
        builtins.push(BuiltinName::pedersen);
        builtin_offset.insert(PedersenType::ID, current_offset);
        current_offset += 1;
    }
    // Force an output builtin so that we can write the program output into it's segment
    if append_output {
        builtins.push(BuiltinName::output);
        builtin_offset.insert(OutputType::ID, current_offset);
    }
    builtins.reverse();
    (builtins, builtin_offset)
}

fn fetch_return_values(
    return_type_size: i16,
    return_type_id: &ConcreteTypeId,
    vm: &VirtualMachine,
) -> Result<Vec<MaybeRelocatable>, Error> {
    let mut return_values = vm.get_return_values(return_type_size as usize)?;
    // Check if this result is a Panic result
    if return_type_id
        .debug_name
        .as_ref()
        .ok_or_else(|| Error::TypeIdNoDebugName(return_type_id.clone()))?
        .starts_with("core::panics::PanicResult::")
    {
        // Check the failure flag (aka first return value)
        if return_values.first() != Some(&MaybeRelocatable::from(0)) {
            // In case of failure, extract the error from the return values (aka last two values)
            let panic_data_end = return_values
                .last()
                .ok_or(Error::FailedToExtractReturnValues)?
                .get_relocatable()
                .ok_or(Error::FailedToExtractReturnValues)?;
            let panic_data_start = return_values
                .get(return_values.len() - 2)
                .ok_or(Error::FailedToExtractReturnValues)?
                .get_relocatable()
                .ok_or(Error::FailedToExtractReturnValues)?;
            let panic_data = vm.get_integer_range(
                panic_data_start,
                (panic_data_end - panic_data_start).map_err(VirtualMachineError::Math)?,
            )?;
            return Err(Error::RunPanic(
                panic_data.iter().map(|c| *c.as_ref()).collect(),
            ));
        } else {
            if return_values.len() < 3 {
                return Err(Error::FailedToExtractReturnValues);
            }
            return_values = return_values[1..].to_vec()
        }
    }
    Ok(return_values)
}

// Calculates builtins' final_stack setting each stop_ptr
// Calling this function is a must if either air_public_input or cairo_pie are needed
fn finalize_builtins(
    skip_output: bool,
    main_ret_types: &[ConcreteTypeId],
    type_sizes: &UnorderedHashMap<ConcreteTypeId, i16>,
    vm: &mut VirtualMachine,
) -> Result<(), Error> {
    // Set stop pointers for builtins so we can obtain the air public input
    // Cairo 1 programs have other return values aside from the used builtin's final pointers, so we need to hand-pick them
    let ret_types_sizes = main_ret_types
        .iter()
        .map(|id| type_sizes.get(id).cloned().unwrap_or_default());
    let ret_types_and_sizes = main_ret_types.iter().zip(ret_types_sizes.clone());

    let full_ret_types_size: i16 = ret_types_sizes.sum();
    let mut stack_pointer = (vm.get_ap() - (full_ret_types_size as usize).saturating_sub(1))
        .map_err(VirtualMachineError::Math)?;

    // Calculate the stack_ptr for each return builtin in the return values
    let mut builtin_name_to_stack_pointer = HashMap::new();
    for (id, size) in ret_types_and_sizes {
        if let Some(ref name) = id.debug_name {
            let builtin_name = match &*name.to_string() {
                "RangeCheck" => RANGE_CHECK_BUILTIN_NAME,
                "Poseidon" => POSEIDON_BUILTIN_NAME,
                "EcOp" => EC_OP_BUILTIN_NAME,
                "Bitwise" => BITWISE_BUILTIN_NAME,
                "Pedersen" => HASH_BUILTIN_NAME,
                "Output" => OUTPUT_BUILTIN_NAME,
                "Ecdsa" => SIGNATURE_BUILTIN_NAME,
                _ => {
                    stack_pointer.offset += size as usize;
                    continue;
                }
            };
            builtin_name_to_stack_pointer.insert(builtin_name, stack_pointer);
        }
        stack_pointer.offset += size as usize;
    }

    // Set stop pointer for each builtin
    vm.builtins_final_stack_from_stack_pointer_dict(&builtin_name_to_stack_pointer, skip_output)?;
    Ok(())
}

fn serialize_output(
    return_values: &[MaybeRelocatable],
    vm: &mut VirtualMachine,
    return_type_id: &ConcreteTypeId,
    sierra_program_registry: &ProgramRegistry<CoreType, CoreLibfunc>,
    type_sizes: &UnorderedHashMap<ConcreteTypeId, i16>,
) -> String {
    let mut output_string = String::new();
    let mut return_values_iter: Peekable<Iter<MaybeRelocatable>> = return_values.iter().peekable();
    serialize_output_inner(
        &mut return_values_iter,
        &mut output_string,
        vm,
        return_type_id,
        sierra_program_registry,
        type_sizes,
    );
    output_string
}

fn serialize_output_inner(
    return_values_iter: &mut Peekable<Iter<MaybeRelocatable>>,
    output_string: &mut String,
    vm: &mut VirtualMachine,
    return_type_id: &ConcreteTypeId,
    sierra_program_registry: &ProgramRegistry<CoreType, CoreLibfunc>,
    type_sizes: &UnorderedHashMap<ConcreteTypeId, i16>,
) {
    match sierra_program_registry.get_type(return_type_id).unwrap() {
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::Array(info) => {
            // Fetch array from memory
            let array_start = return_values_iter
                .next()
                .expect("Missing return value")
                .get_relocatable()
                .expect("Array start_ptr not Relocatable");
            // Arrays can come in two formats: either [start_ptr, end_ptr] or [end_ptr], with the start_ptr being implicit (base of the end_ptr's segment)
            let (array_start, array_size ) = match return_values_iter.peek().and_then(|mr| mr.get_relocatable()) {
                Some(array_end) if array_end.segment_index == array_start.segment_index && array_end.offset >= array_start.offset  => {
                    // Pop the value we just peeked
                    return_values_iter.next();
                    (array_start, (array_end - array_start).unwrap())
                }
                _ => ((array_start.segment_index, 0).into(), array_start.offset),
            };
            let array_data = vm.get_continuous_range(array_start, array_size).unwrap();
            let mut array_data_iter = array_data.iter().peekable();
            let array_elem_id = &info.ty;
            // Serialize array data
            maybe_add_whitespace(output_string);
            output_string.push('[');
            while array_data_iter.peek().is_some() {
                serialize_output_inner(
                    &mut array_data_iter,
                    output_string,
                    vm,
                    array_elem_id,
                    sierra_program_registry,
                    type_sizes,
                )
            }
            output_string.push(']');
        }
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::Box(info) => {
            // As this represents a pointer, we need to extract it's values
            let ptr = return_values_iter
                .next()
                .expect("Missing return value")
                .get_relocatable()
                .expect("Box Pointer is not Relocatable");
            let type_size = type_sizes.type_size(&info.ty);
            let data = vm
                .get_continuous_range(ptr, type_size)
                .expect("Failed to extract value from nullable ptr");
            let mut data_iter = data.iter().peekable();
            serialize_output_inner(
                &mut data_iter,
                output_string,
                vm,
                &info.ty,
                sierra_program_registry,
                type_sizes,
            )
        }
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::Const(_) => {
            unimplemented!("Not supported in the current version")
        },
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::Felt252(_)
        // Only unsigned integer values implement Into<Bytes31>
        | cairo_lang_sierra::extensions::core::CoreTypeConcrete::Bytes31(_)
        | cairo_lang_sierra::extensions::core::CoreTypeConcrete::Uint8(_)
        | cairo_lang_sierra::extensions::core::CoreTypeConcrete::Uint16(_)
        | cairo_lang_sierra::extensions::core::CoreTypeConcrete::Uint32(_)
        | cairo_lang_sierra::extensions::core::CoreTypeConcrete::Uint64(_)
        | cairo_lang_sierra::extensions::core::CoreTypeConcrete::Uint128(_) => {
            maybe_add_whitespace(output_string);
            let val = return_values_iter
                .next()
                .expect("Missing return value")
                .get_int()
                .expect("Value is not an integer");
            output_string.push_str(&val.to_string());
        }
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::Sint8(_)
        | cairo_lang_sierra::extensions::core::CoreTypeConcrete::Sint16(_)
        | cairo_lang_sierra::extensions::core::CoreTypeConcrete::Sint32(_)
        | cairo_lang_sierra::extensions::core::CoreTypeConcrete::Sint64(_)
        | cairo_lang_sierra::extensions::core::CoreTypeConcrete::Sint128(_) => {
            maybe_add_whitespace(output_string);
            let val = return_values_iter
                .next()
                .expect("Missing return value")
                .get_int()
                .expect("Value is not an integer");
            output_string.push_str(&signed_felt(val).to_string());
        }
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::NonZero(info) => {
            serialize_output_inner(
                return_values_iter,
                output_string,
                vm,
                &info.ty,
                sierra_program_registry,
                type_sizes,
            )
        }
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::Nullable(info) => {
            // As this represents a pointer, we need to extract it's values
            let ptr = match return_values_iter.next().expect("Missing return value") {
                MaybeRelocatable::RelocatableValue(ptr) => *ptr,
                MaybeRelocatable::Int(felt) if felt.is_zero() => {
                    // Nullable is Null
                    maybe_add_whitespace(output_string);
                    output_string.push_str("null");
                    return;
                }
                _ => panic!("Invalid Nullable"),
            };
            let type_size = type_sizes.type_size(&info.ty);
            let data = vm
                .get_continuous_range(ptr, type_size)
                .expect("Failed to extract value from nullable ptr");
            let mut data_iter = data.iter().peekable();
            serialize_output_inner(
                &mut data_iter,
                output_string,
                vm,
                &info.ty,
                sierra_program_registry,
                type_sizes,
            )
        }
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::Enum(info) => {
            // First we check if it is a Panic enum, as we already handled panics when fetching return values,
            // we can ignore them and move on to the non-panic variant
            if let GenericArg::UserType(user_type) = &info.info.long_id.generic_args[0] {
                if user_type
                    .debug_name
                    .as_ref()
                    .is_some_and(|n| n.starts_with("core::panics::PanicResult"))
                {
                    return serialize_output_inner(
                        return_values_iter,
                        output_string,
                        vm,
                        &info.variants[0],
                        sierra_program_registry,
                        type_sizes,
                    );
                }
            }
            let num_variants = &info.variants.len();
            let casm_variant_idx: usize = return_values_iter
                .next()
                .expect("Missing return value")
                .get_int()
                .expect("Enum tag is not integer")
                .to_usize()
                .expect("Invalid enum tag");
            // Convert casm variant idx to sierra variant idx
            let variant_idx = if *num_variants > 2 {
                num_variants - 1 - (casm_variant_idx >> 1)
            } else {
                casm_variant_idx
            };
            let variant_type_id = &info.variants[variant_idx];

            // Handle core::bool separately
            if let GenericArg::UserType(user_type) = &info.info.long_id.generic_args[0] {
                if user_type
                    .debug_name
                    .as_ref()
                    .is_some_and(|n| n == "core::bool")
                {
                    // Sanity checks
                    assert!(
                        *num_variants == 2
                            && variant_idx < 2
                            && type_sizes
                                .get(&info.variants[0])
                                .is_some_and(|size| size.is_zero())
                            && type_sizes
                                .get(&info.variants[1])
                                .is_some_and(|size| size.is_zero()),
                        "Malformed bool enum"
                    );

                    let boolean_string = match variant_idx {
                        0 => "false",
                        _ => "true",
                    };
                    maybe_add_whitespace(output_string);
                    output_string.push_str(boolean_string);
                    return;
                }
            }
            // TODO: Something similar to the bool handling could be done for unit enum variants if we could get the type info with the variant names

            // Space is always allocated for the largest enum member, padding with zeros in front for the smaller variants
            let mut max_variant_size = 0;
            for variant in &info.variants {
                let variant_size = type_sizes.get(variant).unwrap();
                max_variant_size = std::cmp::max(max_variant_size, *variant_size)
            }
            for _ in 0..max_variant_size - type_sizes.get(variant_type_id).unwrap() {
                // Remove padding
                assert_eq!(
                    return_values_iter.next(),
                    Some(&MaybeRelocatable::from(0)),
                    "Malformed enum"
                );
            }
            serialize_output_inner(
                return_values_iter,
                output_string,
                vm,
                variant_type_id,
                sierra_program_registry,
                type_sizes,
            )
        }
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::Struct(info) => {
            for member_type_id in &info.members {
                serialize_output_inner(
                    return_values_iter,
                    output_string,
                    vm,
                    member_type_id,
                    sierra_program_registry,
                    type_sizes,
                )
            }
        }
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::Felt252Dict(info) => {
            // Process Dictionary
            let dict_ptr = return_values_iter
                .next()
                .expect("Missing return val")
                .get_relocatable()
                .expect("Dict Ptr not Relocatable");
            if !(dict_ptr.offset
                == vm
                    .get_segment_size(dict_ptr.segment_index as usize)
                    .unwrap_or_default()
                && dict_ptr.offset % 3 == 0)
            {
                panic!("Return value is not a valid Felt252Dict")
            }
            // Fetch dictionary values type id
            let value_type_id = &info.ty;
            // Fetch the dictionary's memory
            let dict_mem = vm
                .get_continuous_range((dict_ptr.segment_index, 0).into(), dict_ptr.offset)
                .expect("Malformed dictionary memory");
            // Serialize the dictionary
            output_string.push('{');
            // The dictionary's memory is made up of (key, prev_value, next_value) tuples
            // The prev value is not relevant to the user so we can skip over it for calrity
            for (key, _, value) in dict_mem.iter().tuples() {
                maybe_add_whitespace(output_string);
                // Serialize the key wich should always be a Felt value
                output_string.push_str(&key.to_string());
                output_string.push(':');
                // Serialize the value
                // We create a peekable array here in order to use the serialize_output_inner as the value could be a span
                let value_vec = vec![value.clone()];
                let mut value_iter: Peekable<Iter<MaybeRelocatable>> = value_vec.iter().peekable();
                serialize_output_inner(
                    &mut value_iter,
                    output_string,
                    vm,
                    value_type_id,
                    sierra_program_registry,
                    type_sizes,
                );
            }
            output_string.push('}');
        }
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::SquashedFelt252Dict(info) => {
            // Process Dictionary
            let dict_start = return_values_iter
                .next()
                .expect("Missing return val")
                .get_relocatable()
                .expect("Squashed dict_start ptr not Relocatable");
            let dict_end = return_values_iter
                .next()
                .expect("Missing return val")
                .get_relocatable()
                .expect("Squashed dict_end ptr not Relocatable");
            let dict_size = (dict_end - dict_start).unwrap();
            if dict_size % 3 != 0 {
                panic!("Return value is not a valid SquashedFelt252Dict")
            }
            // Fetch dictionary values type id
            let value_type_id = &info.ty;
            // Fetch the dictionary's memory
            let dict_mem = vm
                .get_continuous_range(dict_start, dict_size)
                .expect("Malformed squashed dictionary memory");
            // Serialize the dictionary
            output_string.push('{');
            // The dictionary's memory is made up of (key, prev_value, next_value) tuples
            // The prev value is not relevant to the user so we can skip over it for calrity
            for (key, _, value) in dict_mem.iter().tuples() {
                maybe_add_whitespace(output_string);
                // Serialize the key wich should always be a Felt value
                output_string.push_str(&key.to_string());
                output_string.push(':');
                // Serialize the value
                // We create a peekable array here in order to use the serialize_output_inner as the value could be a span
                let value_vec = vec![value.clone()];
                let mut value_iter: Peekable<Iter<MaybeRelocatable>> = value_vec.iter().peekable();
                serialize_output_inner(
                    &mut value_iter,
                    output_string,
                    vm,
                    value_type_id,
                    sierra_program_registry,
                    type_sizes,
                );
            }
            output_string.push('}');
        }
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::Span(_) => unimplemented!("Span types get resolved to Array in the current version"),
        cairo_lang_sierra::extensions::core::CoreTypeConcrete::Snapshot(info) => {
            serialize_output_inner(
                return_values_iter,
                output_string,
                vm,
                &info.ty,
                sierra_program_registry,
                type_sizes,
            )
        }
        _ => panic!("Unexpected return type")
    }
}

fn maybe_add_whitespace(string: &mut String) {
    if !string.is_empty() && !string.ends_with('[') && !string.ends_with('{') {
        string.push(' ');
    }
}
