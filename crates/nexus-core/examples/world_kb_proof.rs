//! P1 proof scaffold: concurrent HTTP/CLI scenario (full CLI path lands in T3).

use std::path::PathBuf;
use std::process::Command;

fn main() {
    let mut scenario = String::new();
    let mut cli_bin: Option<PathBuf> = None;
    let mut out_dir: Option<PathBuf> = None;
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--scenario" if i + 1 < args.len() => {
                scenario = args[i + 1].clone();
                i += 2;
            }
            "--cli-bin" if i + 1 < args.len() => {
                cli_bin = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--out" if i + 1 < args.len() => {
                out_dir = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            other => {
                eprintln!("unknown argument: {other}");
                std::process::exit(2);
            }
        }
    }

    if scenario != "concurrent-http-cli" {
        eprintln!("unsupported scenario '{scenario}'; expected --scenario concurrent-http-cli");
        std::process::exit(2);
    }

    let out = out_dir.unwrap_or_else(|| PathBuf::from("evidence"));
    std::fs::create_dir_all(&out).expect("create evidence dir");

    match cli_bin {
        Some(bin) => {
            let status = Command::new(&bin)
                .arg("--help")
                .status()
                .expect("spawn cli");
            if !status.success() {
                eprintln!(
                    "CLI binary at {} lacks the direct basic-cli path (T3); degrading with clear error",
                    bin.display()
                );
                std::process::exit(78);
            }
            std::fs::write(
                out.join("README.txt"),
                "CLI binary responded to --help; full concurrent-http-cli proof completes in P1-T3.\n",
            )
            .expect("write evidence");
        }
        None => {
            std::fs::write(
                out.join("README.txt"),
                "Pass --cli-bin to exercise the basic CLI leg; HTTP host leg is exercised by integration tests until T3.\n",
            )
            .expect("write evidence");
        }
    }

    println!("world_kb_proof scaffold complete for scenario={scenario} out={}", out.display());
}
