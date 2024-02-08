# ZKML Benchmark

## Usage

### Orion
```bash
./target/release/benchmark -f orion -p models/mlp/orion/network.sierra -i models/mlp/orion/network.input -b results/mlp/orion
```

### EZKL
```bash
./target/release/benchmark -f ezkl -p models/mlp/ezkl/network.ezkl -i models/mlp/ezkl/input.json -s models/mlp/ezkl/settings.json -b results/mlp/ezkl
```