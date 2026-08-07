//! Standalone SASS Lifter CLI — zero GPU dependency.
//!
//! Usage:
//!   sass_lift --sass <file.sass> --sm 89 --check

use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::Command;

use clap::Parser;
use sass_toolkit::sass::pipeline::{LiftPipeline, LiftPipelineCtx};
use sass_toolkit::sass::{
    emit::EmitStage, lower::LowerStage,
    parse::stage_parse::ParseStage,
    type_infer::TypeInferStage, SassLiftOptions,
};

#[derive(Debug, Parser)]
#[command(name = "sass_lift", about = "Standalone SASS→PTX lifter")]
struct Args {
    #[arg(long)] sass: Option<String>,
    #[arg(long)] cubin: Option<String>,
    #[arg(short = 'o', long)] output: Option<PathBuf>,
    #[arg(long, default_value_t = 89)] sm: u32,
    #[arg(short = 'k', long)] kernel: Option<String>,
    #[arg(long)] check: bool,
    #[arg(short = 'v', long)] verbose: bool,
}

fn main() {
    let args = Args::parse();
    if args.sass.is_none() && args.cubin.is_none() {
        eprintln!("Error: --sass or --cubin required");
        std::process::exit(1);
    }
    let kernel = args.kernel.unwrap_or_else(|| "lifted_kernel".to_string());

    let pipeline = LiftPipeline::new(vec![
        Box::new(ParseStage),
        Box::new(LowerStage),
        Box::new(TypeInferStage),
        Box::new(EmitStage { kernel_name: kernel.clone() }),
    ]);

    let mut ctx = LiftPipelineCtx::new(SassLiftOptions {
        sm_version: args.sm, kernel_name: kernel,
        include_sass_comments: true, emit_unsupported_comments: !args.check,
        trace_lift: args.verbose, nvinfo: None,
    });

    // Feed input
    if let Some(ref path) = args.cubin {
        let data = std::fs::read(path).unwrap_or_else(|e| {
            eprintln!("Error: {}: {}", path, e); std::process::exit(1);
        });
        ctx.cubin_bytes = Some(data);
    } else if let Some(ref path) = args.sass {
        let text = if path == "-" {
            let mut b = String::new();
            io::stdin().read_to_string(&mut b).unwrap();
            b
        } else {
            std::fs::read_to_string(path).unwrap_or_else(|e| {
                eprintln!("Error: {}: {}", path, e); std::process::exit(1);
            })
        };
        ctx.sass_text = Some(text);
    }

    if let Err(err) = pipeline.run(&mut ctx) {
        eprintln!("Pipeline error: {}", err);
        std::process::exit(1);
    }

    match &args.output {
        Some(p) => std::fs::write(p, &ctx.output).unwrap_or_else(|e| {
            eprintln!("Write {}: {}", p.display(), e); std::process::exit(1);
        }),
        None => { io::stdout().write_all(ctx.output.as_bytes()).unwrap(); }
    }

    if args.check {
        eprintln!("\n─── ptxas validation ───");
        match Command::new("ptxas").arg(format!("-arch=sm_{}", args.sm))
            .arg("-o").arg("/dev/null").arg("-")
            .stdin(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn()
        {
            Ok(mut child) => {
                if let Some(ref mut si) = child.stdin { let _ = si.write_all(ctx.output.as_bytes()); }
                match child.wait_with_output() {
                    Ok(o) if o.status.success() => eprintln!("  PASS"),
                    Ok(o) => {
                        eprintln!("  FAIL");
                        for l in String::from_utf8_lossy(&o.stderr).lines().take(20) { eprintln!("    {}", l); }
                    }
                    Err(e) => eprintln!("  ptxas error: {}", e),
                }
            }
            Err(e) => eprintln!("  ptxas not found: {}", e),
        }
    }
}
