use std::env;

use nexo_verifier::verify_export;

fn main() {
    let mut args = env::args_os();
    let program = args.next().unwrap_or_else(|| "nexo-verify".into());
    let Some(manifest) = args.next() else {
        eprintln!("usage: {} <manifest.json>", program.to_string_lossy());
        std::process::exit(2);
    };
    if args.next().is_some() {
        eprintln!("usage: {} <manifest.json>", program.to_string_lossy());
        std::process::exit(2);
    }
    match verify_export(manifest) {
        Ok(report) => {
            println!(
                "VERIFIED case={} artifacts={} manifest_sha256={}",
                report.case_reference, report.artifact_count, report.manifest_digest
            );
        }
        Err(error) => {
            eprintln!("REJECTED: {error:?}");
            std::process::exit(1);
        }
    }
}
