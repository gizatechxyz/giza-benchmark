# Giza Benchmark

A tool to run ZKML benchmarks on Orion and EZKL frameworks.

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
giza-benchmark -f orion -p benchmarks/mnist-ff/orion/network.sierra.json -i benchmarks/mnist-ff/orion/input.txt -b benchmarks/mnist-ff/orion
```

### EZKL
```bash
giza-benchmark -f ezkl -p benchmark/mnist-ff/ezkl/network.ezkl -i benchmark/mnist-ff/ezkl/input.json -s benchmark/mnist-ff/ezkl/settings.json -b benchmark/mnist-ff/ezkl
```

## Results

You can find some results in `benchmarks` directory.