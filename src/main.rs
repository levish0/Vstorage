use std::path::Path;
use std::process;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "vstorage")]
#[command(version, about = "Encode files as 4K video frames")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Encode a file into a video
    Encode {
        /// Input file path
        #[arg(short, long)]
        input: String,
        /// Output video path (.mp4)
        #[arg(short, long)]
        output: String,
        /// Encryption password (omit for no encryption)
        #[arg(short, long)]
        password: Option<String>,
        /// Pixel block size
        #[arg(long, default_value = "8")]
        block_size: u8,
        /// Quantization levels per channel (power of 2)
        #[arg(long, default_value = "2")]
        levels: u8,
        /// Video frame rate
        #[arg(long, default_value = "30")]
        fps: u32,
        /// FFmpeg CRF quality (lower = better)
        #[arg(long, default_value = "18")]
        crf: u8,
        /// Reed-Solomon ECC parity bytes
        #[arg(long, default_value = "64")]
        ecc: u8,
    },
    /// Decode a video back into the original file
    Decode {
        /// Input video path (.mp4)
        #[arg(short, long)]
        input: String,
        /// Output file path
        #[arg(short, long)]
        output: String,
        /// Decryption password (omit if not encrypted)
        #[arg(short, long)]
        password: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Encode {
            input,
            output,
            password,
            block_size,
            levels,
            fps,
            crf,
            ecc,
        } => {
            let config = match vstorage::config::FrameConfig::new(block_size, levels, ecc, fps, crf)
            {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            };
            vstorage::encode::encode(
                Path::new(&input),
                Path::new(&output),
                password.as_deref(),
                &config,
            )
        }
        Commands::Decode {
            input,
            output,
            password,
        } => vstorage::decode::decode(Path::new(&input), Path::new(&output), password.as_deref()),
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}
