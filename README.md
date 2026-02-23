# Vstorage

Encode any file into a 4K video. Decode it back, even after lossy compression (YouTube, etc).

Uses AES-256-GCM encryption, Reed-Solomon error correction, and quantized pixel blocks.

## Requirements

- Rust 1.56+
- FFmpeg (must be on PATH)

## Install

```
cargo install --path .
```

## Usage

### Encode

```
cargo run --release -- encode -i <FILE> -o <VIDEO> [OPTIONS]
```

| Flag                        | Default | Description                                  |
|-----------------------------|---------|----------------------------------------------|
| `-i, --input <INPUT>`       |         | Input file path                              |
| `-o, --output <OUTPUT>`     |         | Output video path (.mp4)                     |
| `-p, --password <PASSWORD>` |         | Encryption password (optional)               |
| `--block-size <BLOCK_SIZE>` | 8       | Pixels per logical block                     |
| `--levels <LEVELS>`         | 2       | Quantization levels per channel (power of 2) |
| `--fps <FPS>`               | 30      | Video frame rate                             |
| `--crf <CRF>`               | 18      | FFmpeg CRF quality (lower = better)          |
| `--ecc <ECC>`               | 64      | Reed-Solomon ECC parity bytes                |

### Decode

```
cargo run --release -- decode -i <VIDEO> -o <FILE>
```

| Flag                        | Description                  |
|-----------------------------|------------------------------|
| `-i, --input <INPUT>`       | Input video path             |
| `-o, --output <OUTPUT>`     | Output file path             |
| `-p, --password <PASSWORD>` | Decryption password (if set) |

Decode reads block-size, levels, and ecc from the video header automatically.

## Defaults

Defaults are tuned for YouTube survival:

| Setting          | Default          | Why                                     |
|------------------|------------------|-----------------------------------------|
| `--block-size 8` | 8x8 pixel blocks | Survives yuv420p chroma subsampling     |
| `--levels 2`     | 0 or 255 only    | Maximum noise tolerance (±127)          |
| `--ecc 64`       | 64 parity bytes  | Corrects up to 32 byte errors per block |

If decode fails after YouTube, try `--ecc 128` for more error correction.

For local use (no YouTube), you can increase capacity with:

```
cargo run --release -- encode -i myfile.zip -o output.mp4 --block-size 2 --levels 4 --ecc 32
```

## Capacity

| Preset                              | Per frame | Per minute (30fps) |
|-------------------------------------|-----------|--------------------|
| Default (block=8, levels=2, ecc=64) | ~35 KB    | ~63 MB             |
| Local (block=2, levels=4, ecc=32)   | ~1.3 MB   | ~2.3 GB            |
