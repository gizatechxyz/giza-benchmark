# Giza Benchmark

A tool to run Orion/Giza  benchmarks.

## Prerequisites

- Rust
- `lambdaworks/provers/cairo` from [here](https://github.com/lambdaclass/lambdaworks/tree/fed12d674418e4f09bc843b71bc90008a85b1aed) for proving only. As of February 2024, the tested revision is `fed12d6`.

## Installation

Clone this repository and run:
```bash
cargo install --path .
```

## Usage Example

### Orion
```bash
giza-benchmark cargo run -- -p examples/xgb/xgb_inf.sierra.json -i examples/xgb/input.txt -b examples/xgb
```

## TODO
We plan to support more ZKML frameworks in a near future.