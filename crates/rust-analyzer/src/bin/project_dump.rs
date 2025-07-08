/// Project Asset Dumper
///
/// This binary recursively scans a directory for Rust source files (`.rs`), parses each file,
/// collects their ASTs and parse errors, and encodes all files into a single binary asset file.
///
/// # Usage
///
///     cargo run --bin project_dump -- <path-to-directory> [--out <output-path>]
///
/// - `<path-to-directory>`: The root directory to scan for Rust files.
/// - `--out <output-path>`: (Optional) Path to write the output asset file. Defaults to `project.asset`.
///
/// # Example
///
///     cargo run --bin project_dump -- ./my_rust_project --out my_project.asset
///
/// This will create `my_project.asset` containing all `.rs` files in `./my_rust_project` and subdirectories.

use std::{env, fs, process::exit, path::Path, io::BufWriter};
use walkdir::WalkDir;
use rust_analyzer::asset::Project;
use rust_analyzer::asset_gen::parse_rust_to_asset_file;

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut dir = None;
    let mut out_path = String::from("project.asset");
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--out" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Missing value for --out");
                    exit(1);
                }
                out_path = args[i].clone();
            }
            _ if dir.is_none() => {
                dir = Some(args[i].clone());
            }
            _ => {
                eprintln!("Usage: {} <path-to-directory> [--out <output-path>]", args[0]);
                exit(1);
            }
        }
        i += 1;
    }
    let dir = match dir {
        Some(d) => d,
        None => {
            eprintln!("Usage: {} <path-to-directory> [--out <output-path>]", args[0]);
            exit(1);
        }
    };
    let mut files = Vec::new();
    for entry in WalkDir::new(&dir).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if path.is_file() && path.extension().map_or(false, |ext| ext == "rs") {
            let path_str = path.display().to_string();
            let text = match fs::read_to_string(path) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Failed to read file {}: {}", path_str, e);
                    continue;
                }
            };
            eprintln!("Parsing file: {}", path_str);
            let file_asset = parse_rust_to_asset_file(path_str, text);
            files.push(file_asset);
        }
    }
    let project = Project { files };
    let out_path = Path::new(&out_path);
    let out_file = match fs::File::create(out_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to create output file {}: {}", out_path.display(), e);
            exit(1);
        }
    };
    let mut writer = BufWriter::new(out_file);
    if let Err(e) = project.encode(&mut writer) {
        eprintln!("Failed to encode project asset: {}", e);
        exit(1);
    }
    println!("Project asset written to {}", out_path.display());
} 