# docx2txt

A tool to extract plain text from Microsoft .docx (OOXML) documents,
preserving basic formatting cues such as list numbering, indentation,
and hyperlink annotations.

This is a Rust rewrite of docx2txt 1.4
(http://docx2txt.sourceforge.net/) by Sandeep Kumar. The original is
a Perl script; this rewrite exists so that Git for Windows can drop
its Perl dependency while retaining `git diff` support for .docx files.


## Status

Functional. Produces byte-identical output to the Perl original on all
test fixtures (both hand-crafted and Word-saved).


## Usage

    docx2txt < input.docx
    docx2txt < input.docx > output.txt
    cat input.docx | docx2txt

Input is always read from stdin, output is always written to stdout.
This is the only interface Git needs for its textconv mechanism.


## What it extracts

- Main document body text
- Hyperlink URLs (optionally appended as `[HYPERLINK: url]`)
- List markers (bullet, decimal, letter, roman) with indentation
- Paragraph borders as horizontal rules
- Caps formatting (uppercased in output)
- Superscript cross-references (wrapped in `[...]`)

For the full list of OOXML constructs handled, see `AGENTS.md` in the
parent directory.


## What it does not handle

- Headers and footers
- Footnotes and endnotes
- Images
- Tables as structured data
- Comments and tracked changes (deleted text is stripped)


## Building

    cargo build --release

The binary is at `target/release/docx2txt`.


## Testing

    cargo test

The test suite includes both Word-saved fixtures (committed as binary
files in `tests/fixtures/`) and programmatically generated fixtures
that exercise every regex in the conversion pipeline.


## License

GNU General Public License version 3 or later, matching the original
docx2txt.
