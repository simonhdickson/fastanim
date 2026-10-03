//! The `ranim` command-line tool (see `docs/SPEC.md` §9).
//!
//! Only `ranim diff` exists so far; the other commands arrive with later milestones.

use std::process::ExitCode;

use ranim_diff::{Algorithm, Cleanup, DiffOptions, Differ, Op, TieBreak};

const USAGE: &str = "\
Usage: ranim diff <A> <B> [options]

Print the edit script that turns A into B.

Options:
  --by <unit>          Diff unit: char (default; whitespace ignored), word, or line
  --algorithm <name>   myers (default), linear, or patience
  --tie-break <name>   stable (default) or myers
  --no-moves           Don't pair deletions and insertions into moves
  --no-replace         Don't pair deletions and insertions into replacements
  --cleanup <n>        Fold equal runs shorter than n into surrounding edits
  --ops                Also print the raw ops with indices
  -h, --help           Show this help

Notation: =x equal, -[x] delete, +[x] insert, ↷x move, ~(x→y) replace.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("diff") => match DiffArgs::parse(&args[1..]) {
            Ok(Some(a)) => {
                print!("{}", a.run());
                ExitCode::SUCCESS
            }
            Ok(None) => {
                println!("{USAGE}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {e}\n\n{USAGE}");
                ExitCode::FAILURE
            }
        },
        Some(cmd @ ("preview" | "render" | "still")) => {
            eprintln!(
                "error: `ranim {cmd}` needs a scene crate, which `ranim new` will scaffold. \
                 Until then, call `ranim_bevy::run(construct)` from your scene's `main` and use \
                 `cargo run -- {cmd} --help`."
            );
            ExitCode::FAILURE
        }
        Some("new") => {
            eprintln!("error: `ranim new` is not implemented yet");
            ExitCode::FAILURE
        }
        None | Some("-h" | "--help" | "help") => {
            println!("ranim: programmatic mathematical animation\n\n{USAGE}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("error: unknown command `{other}`\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    Char,
    Word,
    Line,
}

#[derive(Debug)]
struct DiffArgs {
    a: String,
    b: String,
    unit: Unit,
    opts: DiffOptions,
    show_ops: bool,
}

impl DiffArgs {
    /// Parses the arguments after `diff`. `Ok(None)` means help was requested.
    fn parse(args: &[String]) -> Result<Option<Self>, String> {
        let mut inputs = Vec::new();
        let mut unit = Unit::Char;
        let mut opts = DiffOptions::default();
        let mut show_ops = false;
        let mut it = args.iter();
        while let Some(arg) = it.next() {
            let mut value = |flag: &str| {
                it.next()
                    .cloned()
                    .ok_or_else(|| format!("{flag} needs a value"))
            };
            match arg.as_str() {
                "-h" | "--help" => return Ok(None),
                "--by" => {
                    unit = match value("--by")?.as_str() {
                        "char" => Unit::Char,
                        "word" => Unit::Word,
                        "line" => Unit::Line,
                        other => return Err(format!("unknown unit `{other}`")),
                    }
                }
                "--algorithm" => {
                    opts.algorithm = match value("--algorithm")?.as_str() {
                        "myers" => Algorithm::Myers,
                        "linear" => Algorithm::MyersLinearSpace,
                        "patience" => Algorithm::Patience,
                        other => return Err(format!("unknown algorithm `{other}`")),
                    }
                }
                "--tie-break" => {
                    opts.tie_break = match value("--tie-break")?.as_str() {
                        "stable" => TieBreak::Stable,
                        "myers" => TieBreak::Myers,
                        other => return Err(format!("unknown tie-break `{other}`")),
                    }
                }
                "--no-moves" => opts.detect_moves = false,
                "--no-replace" => opts.pair_replacements = false,
                "--cleanup" => {
                    let v = value("--cleanup")?;
                    let min_equal_run = v
                        .parse()
                        .map_err(|_| format!("--cleanup expects a number, got `{v}`"))?;
                    opts.cleanup = Cleanup::Semantic { min_equal_run };
                }
                "--ops" => show_ops = true,
                "--math" => {
                    return Err("--math needs Typst support, which is not implemented yet".into());
                }
                flag if flag.starts_with("--") => return Err(format!("unknown option `{flag}`")),
                _ => inputs.push(arg.clone()),
            }
        }
        let [a, b]: [String; 2] = inputs
            .try_into()
            .map_err(|v: Vec<String>| format!("expected 2 inputs, got {}", v.len()))?;
        Ok(Some(Self {
            a,
            b,
            unit,
            opts,
            show_ops,
        }))
    }

    fn run(&self) -> String {
        let a = tokenize(&self.a, self.unit);
        let b = tokenize(&self.b, self.unit);
        let mut differ = Differ::new(&a, &b, |t| t.text.clone())
            .options(self.opts.clone())
            .cost(|i, j| a[i].col.abs_diff(b[j].col) as f64);
        if self.unit != Unit::Line {
            differ = differ.class(|t| is_operator(&t.text).then_some(()));
        }
        let ops = differ.run();

        let mut out = render(&a, &b, &ops, self.unit);
        out.push('\n');
        if self.show_ops {
            for op in &ops {
                out.push_str(&format!("{op:?}\n"));
            }
        }
        out
    }
}

/// A diff unit and its position: a column for chars and words, a line number for lines.
#[derive(Debug)]
struct Token {
    text: String,
    col: usize,
}

fn tokenize(s: &str, unit: Unit) -> Vec<Token> {
    match unit {
        Unit::Char => s
            .chars()
            .enumerate()
            .filter(|(_, c)| !c.is_whitespace())
            .map(|(col, c)| Token {
                text: c.to_string(),
                col,
            })
            .collect(),
        Unit::Word => {
            let mut out = Vec::new();
            let mut start = None;
            for (col, c) in s.chars().chain([' ']).enumerate() {
                match (c.is_whitespace(), start) {
                    (false, None) => start = Some(col),
                    (true, Some(st)) => {
                        let text = s.chars().skip(st).take(col - st).collect();
                        out.push(Token { text, col: st });
                        start = None;
                    }
                    _ => {}
                }
            }
            out
        }
        Unit::Line => s
            .lines()
            .enumerate()
            .map(|(col, l)| Token {
                text: l.trim().to_string(),
                col,
            })
            .collect(),
    }
}

fn is_operator(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_punctuation())
}

fn render(a: &[Token], b: &[Token], ops: &[Op], unit: Unit) -> String {
    let sep = match unit {
        Unit::Char => " ",
        Unit::Word => " ",
        Unit::Line => "\n",
    };
    let join = |t: &[Token]| {
        t.iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join(if unit == Unit::Char { "" } else { " " })
    };
    ops.iter()
        .map(|op| match op {
            Op::Equal { b: j, .. } => format!("={}", b[*j].text),
            Op::Delete { a: i } => format!("-[{}]", a[*i].text),
            Op::Insert { b: j } => format!("+[{}]", b[*j].text),
            Op::Move { b: j, .. } => format!("↷{}", b[*j].text),
            Op::Replace { a: r, b: s } => {
                format!("~({}→{})", join(&a[r.clone()]), join(&b[s.clone()]))
            }
        })
        .collect::<Vec<_>>()
        .join(sep)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> String {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        DiffArgs::parse(&args).unwrap().unwrap().run()
    }

    #[test]
    fn chars() {
        assert_eq!(run(&["a + b = c", "b + a = c"]), "↷b =+ ↷a == =c\n");
        assert_eq!(run(&["a+b=c", "a+b+d=c"]), "=a =+ =b +[+] +[d] == =c\n");
    }

    #[test]
    fn operators_pair_across_hunks() {
        assert_eq!(run(&["x + y = z", "x = z - y"]), "=x == =z ~(+→-) ↷y\n");
        assert_eq!(
            run(&["a + b = b + a", "b + a = a + b"]),
            "↷b =+ ↷a == ↷a =+ ↷b\n"
        );
    }

    #[test]
    fn words_and_lines() {
        assert_eq!(
            run(&["the quick fox", "the slow fox", "--by", "word"]),
            "=the ~(quick→slow) =fox\n"
        );
        assert_eq!(run(&["x\ny\nz", "y\nz\nx", "--by", "line"]), "=y\n=z\n↷x\n");
    }

    #[test]
    fn bad_args() {
        let parse = |args: &[&str]| {
            let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            DiffArgs::parse(&args)
        };
        assert!(parse(&["only-one"]).is_err());
        assert!(parse(&["a", "b", "--by", "glyph"]).is_err());
        assert!(parse(&["a", "b", "--math"]).is_err());
        assert!(parse(&["--help"]).unwrap().is_none());
    }
}
