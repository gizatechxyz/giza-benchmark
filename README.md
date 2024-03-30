# Giza Benchmark

## Usage

### Orion
```bash
./target/release/giza-benchmark -f orion -p benchmarks/mnist/orion/network.sierra.json -i benchmarks/mnist/orion/input.txt -b benchmarks/mnist/orion
```

### EZKL
```bash
./target/release/giza-benchmark -f ezkl -p benchmark/mnist/ezkl/network.ezkl -i benchmark/mnist/ezkl/input.json -s benchmark/mnist/ezkl/settings.json -b benchmark/mnist/ezkl
```