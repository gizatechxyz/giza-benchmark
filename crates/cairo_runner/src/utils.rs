use cairo1_run::FuncArg;
use cairo_vm::{types::layout_name::LayoutName, Felt252};

use crate::types::FuncArgs;

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


pub(crate) fn layout_str_to_enum(name: &str) -> LayoutName {
    match name {
        "plain" => LayoutName::plain,
        "small" => LayoutName::small,
        "dex" => LayoutName::dex,
        "recursive" => LayoutName::recursive,
        "starknet" => LayoutName::starknet,
        "starknet_with_keccak" => LayoutName::starknet_with_keccak,
        "recursive_large_output" => LayoutName::recursive_large_output,
        "recursive_with_poseidon" => LayoutName::recursive_with_poseidon,
        "all_solidity" => LayoutName::all_solidity,
        "all_cairo" => LayoutName::all_cairo,
        _ => LayoutName::plain,
    }
}
