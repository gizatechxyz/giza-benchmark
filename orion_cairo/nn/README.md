# MLP benchmark

## Model Summary
```
_________________________________________________________________
Layer (type)                 Output Shape              Param #   
=================================================================
input_1 (InputLayer)         [(None, 196)]             0         
_________________________________________________________________
layer1 (Dense)               (None, 10)                1970      
_________________________________________________________________
activation_1 (ReLU)          (None, 10)                0         
_________________________________________________________________
layer2 (Dense)               (None, 10)                110       
=================================================================
Total params: 2,080
_________________________________________________________________
```

## Metrics

| Type               | Run        | Prove        | Verify      |
| ------------------ | ---------- | ------------ | ----------- |
| time (ms)          | 313.388542 | 57897.597791 | 2364.543458 |
| memory usage  (KB) | 181404     | 8392080      | 133600      |


### Proving detailed metrics
```
- Started round 0: Air Initialization
  Time spent: 2.322578583s
- Started round 1: RAP
  Time spent: 28.019009458s
- Started round 2: Compute composition polynomial
     Evaluating periodic columns on lde: 250ns
     Created boundary polynomials: 2.023479708s
     Evaluated boundary polynomials on LDE: 256.95975ms
     Evaluated transition zerofiers: 3.941829125s
     Evaluated transitions and accumulated results: 4.264447125s
  Time spent: 17.813236875s
- Started round 3: Evaluate polynomial in out of domain elements
  Time spent: 3.802154708s
- Started round 4: FRI
  Time spent: 5.758461542s
 Fraction of proving time per round: 0.0419 0.5058 0.3216 0.0686 0.1040
  Time spent in proving: 57.897597791s 
```

### Verifying detailed metrics
```
Verifying ...
- Started step 1: Recover challenges
  Time spent: 2.357160042s
- Started step 2: Verify claimed polynomial
  Time spent: 284.25µs
- Started step 3: Verify FRI
  Time spent: 3.48675ms
- Started step 4: Verify deep composition polynomial
  Time spent: 1.570291ms
 Fraction of verifying time per step: 0.9977 0.0001 0.0015 0.0007
  Time spent in verifying: 2.364543458s 
```

## Reproduce benchmark

### I. Transpile model from ONNX to Orion Cairo
You need first to convert the ONNX to Orion Cairo. Then compile it to Sierra.

1. Use [giza-cli](https://cli.gizatech.xyz/frameworks/cairo/transpile) to transpile onnx model to Orion-Cairo
```bash
giza transpile mlp.onnx --output-path cairo_mlp
```
2. Compile your cairo project to Sierra. 
```bash
$ cd cairo_mlp/inference
$ scarb build
```
The Sierra file will be in `inference/target/dev/inference.sierra`

### II. Run inference with OrionRunner
User [OrionRunner](https://github.com/gizatechxyz/orion_runner) to run the Sierra file on CairoVM and generate trace and memory files. 

1. Clone `OrionRunner`
    ```
    git clone https://github.com/gizatechxyz/orion_runner.git
    ```

2. Build a Docker image and start the container:
    ```bash
    $ docker build -t orion_runner .
    $ docker run -d -p 8080:8080 --env SIERRA_URL='path_to_the_sierra_file' orion_runner
    ```

3. Send a post request
   here is the request used:
   ```bash
   curl -v -X POST http://localhost:8080/cairo_run \
     -H "Content-Type: application/json" \
     -d '{"job_size": "M", "args": "[1 196] [0 0 0 0 0 0 0 0 514 0 771 0 771 0 514 0 514 0 771 0 257 0 257 0 0 0 0 0 0 0 0 0 0 0 0 0 514 0 257 0 0 0 0 0 0 0 0 0 771 0 1028 0 0 0 0 0 0 0 0 0 0 0 0 0 771 0 257 0 6939 0 14135 0 13621 0 13878 0 1028 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 771 0 26471 0 58339 0 63479 0 62708 0 62965 0 40863 0 8224 0 0 0 0 0 514 0 771 0 514 0 514 0 32125 0 58596 0 35980 0 57568 0 44718 0 29812 0 54998 0 40349 0 0 0 0 0 257 0 514 0 257 0 36237 0 63479 0 27242 0 0 0 4626 0 1542 0 0 0 21074 0 49344 0 1799 0 0 0 771 0 0 0 19275 0 64764 0 30583 0 0 0 771 0 514 0 514 0 0 0 19018 0 47545 0 1542 0 0 0 514 0 1028 0 45232 0 47031 0 3341 0 514 0 514 0 1285 0 771 0 3598 0 45232 0 43176 0 0 0 0 0 0 0 3598 0 53456 0 22102 0 0 0 1028 0 257 0 0 0 0 0 26214 0 65278 0 22873 0 0 0 0 0 257 0 514 0 44718 0 49858 0 7453 0 0 0 2827 0 6168 0 27242 0 61937 0 47802 0 1542 0 0 0 0 0 1028 0 257 0 19532 0 61937 0 58339 0 34181 0 53970 0 63479 0 65021 0 51400 0 11051 0 771 0 257 0 0 0 771 0 514 0 0 0 16191 0 44718 0 55512 0 53970 0 42148 0 23387 0 5397 0 0 0 1028 0 257 0 0 0 0 0 0 0 0 0 0 0 257 0 2570 0 2570 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0]"}'
   ```