use std::env;

use nexo_verifier::{verify_audit_export, verify_export};

fn main() {
    let mut args = env::args_os();
    let program = args.next().unwrap_or_else(|| "nexo-verify".into());
    let Some(first) = args.next() else {
        eprintln!("usage: {} [audit] <export.json>", program.to_string_lossy());
        std::process::exit(2);
    };
    let audit = first == "audit";
    let path = if audit { args.next() } else { Some(first) };
    let Some(path) = path else {
        eprintln!(
            "usage: {} audit <audit-export.json>",
            program.to_string_lossy()
        );
        std::process::exit(2);
    };
    if args.next().is_some() {
        eprintln!("usage: {} [audit] <export.json>", program.to_string_lossy());
        std::process::exit(2);
    }
    if audit {
        match verify_audit_export(path) {
            Ok(report)
                if report.complete_history
                    && report.integrity_ok
                    && report.linkage_ok
                    && report.hmac_checked
                    && report.hmac_ok =>
            {
                println!(
                    "AUDIT VERIFIED chain={} entries={} tip={}",
                    report.chain_id, report.entries, report.tip_digest
                );
            }
            Ok(report) => {
                eprintln!("AUDIT REJECTED: {report:?}");
                std::process::exit(1);
            }
            Err(error) => {
                eprintln!("AUDIT REJECTED: {error:?}");
                std::process::exit(1);
            }
        }
        return;
    }
    match verify_export(path) {
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
