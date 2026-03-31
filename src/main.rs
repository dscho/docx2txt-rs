use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.len() > 1 || args.first().map_or(false, |a| a == "-h" || a == "--help") {
        eprintln!("Usage: docx2txt [infile.docx]");
        eprintln!();
        eprintln!("  With no arguments, reads a .docx from stdin and writes text to stdout.");
        eprintln!("  With a filename, writes text to the corresponding .txt file.");
        std::process::exit(if args.first().map_or(false, |a| a == "-h" || a == "--help") { 0 } else { 1 });
    }

    match args.first() {
        None => convert_stdin_to_stdout(),
        Some(infile) => convert_file_to_txt(infile),
    }
}

fn convert_stdin_to_stdout() {
    let mut input = Vec::new();
    io::stdin()
        .read_to_end(&mut input)
        .expect("failed to read stdin");

    match docx2txt::convert(&input) {
        Ok(text) => io::stdout().write_all(text.as_bytes()).unwrap(),
        Err(e) => {
            eprintln!("docx2txt: {}", e);
            std::process::exit(1);
        }
    }
}

fn convert_file_to_txt(infile: &str) {
    let inpath = PathBuf::from(infile);

    let outpath = if inpath.extension().map_or(false, |e| e.eq_ignore_ascii_case("docx")) {
        inpath.with_extension("txt")
    } else {
        PathBuf::from(format!("{}.txt", inpath.display()))
    };

    if outpath.exists() {
        eprint!("overwrite Output text file <{}> [y/n] ? ", outpath.display());
        io::stderr().flush().unwrap();
        let mut answer = String::new();
        io::stdin().read_line(&mut answer).expect("failed to read response");
        if answer.trim() != "y" {
            eprintln!();
            eprintln!("Please copy <{}> somewhere before running the script.", outpath.display());
            std::process::exit(1);
        }
    }

    let input = fs::read(&inpath)
        .unwrap_or_else(|e| { eprintln!("docx2txt: {}: {}", inpath.display(), e); std::process::exit(1); });

    match docx2txt::convert(&input) {
        Ok(text) => {
            fs::write(&outpath, text.as_bytes())
                .unwrap_or_else(|e| { eprintln!("docx2txt: {}: {}", outpath.display(), e); std::process::exit(1); });
            eprintln!();
            eprintln!("Text extracted from <{}> is available in <{}>.", inpath.display(), outpath.display());
        }
        Err(e) => {
            eprintln!();
            eprintln!("Failed to extract text from <{}>: {}", inpath.display(), e);
            std::process::exit(1);
        }
    }
}
