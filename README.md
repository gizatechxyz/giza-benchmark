# Giza Benchmark

## Usage

### Orion
```bash
./target/release/giza-benchmark -f orion -p benchmarks/mnist-ff/orion/network.sierra.json -i benchmarks/mnist-ff/orion/input.txt -b benchmarks/mnist-ff/orion
```

### EZKL
```bash
./target/release/giza-benchmark -f ezkl -p benchmark/mnist-ff/ezkl/network.ezkl -i benchmark/mnist-ff/ezkl/input.json -s benchmark/mnist-ff/ezkl/settings.json -b benchmark/mnist-ff/ezkl
```