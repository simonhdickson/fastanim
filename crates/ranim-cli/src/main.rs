//! The `ranim` command-line tool (see `docs/SPEC.md` §9).
//!
//! Only `ranim diff` exists so far; the other commands arrive with later milestones.

use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use ranim_diff::{Algorithm, Cleanup, DiffOptions, Differ, Op, TieBreak};

/// ranim: programmatic mathematical animation
#[derive(Debug, Parser)]
#[command(name = "ranim", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the edit script that turns A into B
    #[command(
        after_help = "Notation: =x equal, -[x] delete, +[x] insert, ↷x move, ~(x→y) replace."
    )]
    Diff(DiffArgs),
    /// Scaffold a scene crate (not implemented yet)
    New,
    /// Open a scene in a window with a scrubber
    Preview,
    /// Export a scene as video or frames
    Render,
    /// Export one frame of a scene
    Still,
}

fn main() -> ExitCode {
    let cmd = match Cli::parse().command {
        Command::Diff(a) => {
            return match a.run() {
                Ok(out) => {
                    print!("{out}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            };
        }
        Command::New => {
            eprintln!("error: `ranim new` is not implemented yet");
            return ExitCode::FAILURE;
        }
        Command::Preview => "preview",
        Command::Render => "render",
        Command::Still => "still",
    };
    eprintln!(
        "error: `ranim {cmd}` needs a scene crate, which `ranim new` will scaffold. \
         Until then, call `ranim_bevy::run(construct)` from your scene's `main` and use \
         `cargo run -- {cmd} --help`."
    );
    ExitCode::FAILURE
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Unit {
    Char,
    Word,
    Line,
    #[value(skip)]
    Math,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum AlgorithmArg {
    Myers,
    Linear,
    Patience,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum TieBreakArg {
    Stable,
    Myers,
}

#[derive(Debug, clap::Args)]
struct DiffArgs {
    /// The text to diff from
    a: String,
    /// The text to diff to
    b: String,
    /// Diff unit; char ignores whitespace
    #[arg(long = "by", value_name = "UNIT", default_value = "char")]
    unit: Unit,
    /// Typeset A and B as Typst math and diff their glyphs
    #[arg(long)]
    math: bool,
    /// Core diff algorithm
    #[arg(long, value_name = "NAME", default_value = "myers")]
    algorithm: AlgorithmArg,
    /// Choice between equally short scripts
    #[arg(long, value_name = "NAME", default_value = "stable")]
    tie_break: TieBreakArg,
    /// Don't pair deletions and insertions into moves
    #[arg(long)]
    no_moves: bool,
    /// Don't pair deletions and insertions into replacements
    #[arg(long)]
    no_replace: bool,
    /// Fold equal runs shorter than N into surrounding edits
    #[arg(long, value_name = "N")]
    cleanup: Option<usize>,
    /// Also print the raw ops with indices
    #[arg(long = "ops")]
    show_ops: bool,
}

impl DiffArgs {
    fn options(&self) -> DiffOptions {
        DiffOptions {
            algorithm: match self.algorithm {
                AlgorithmArg::Myers => Algorithm::Myers,
                AlgorithmArg::Linear => Algorithm::MyersLinearSpace,
                AlgorithmArg::Patience => Algorithm::Patience,
            },
            tie_break: match self.tie_break {
                TieBreakArg::Stable => TieBreak::Stable,
                TieBreakArg::Myers => TieBreak::Myers,
            },
            detect_moves: !self.no_moves,
            pair_replacements: !self.no_replace,
            cleanup: self
                .cleanup
                .map_or(Cleanup::None, |min_equal_run| Cleanup::Semantic {
                    min_equal_run,
                }),
        }
    }

    fn run(&self) -> Result<String, String> {
        let unit = if self.math { Unit::Math } else { self.unit };
        let a = tokenize(&self.a, unit)?;
        let b = tokenize(&self.b, unit)?;
        let mut differ = Differ::new(&a, &b, |t| t.text.clone())
            .options(self.options())
            .cost(|i, j| (a[i].col - b[j].col).abs());
        if unit != Unit::Line {
            differ = differ.class(|t| is_operator(&t.text).then_some(()));
        }
        let ops = differ.run();

        let mut out = render(&a, &b, &ops, unit);
        out.push('\n');
        if self.show_ops {
            for op in &ops {
                out.push_str(&format!("{op:?}\n"));
            }
        }
        Ok(out)
    }
}

/// A diff unit and its position: a column for chars and words, a line number for lines, the
/// x of the glyph's center for math.
#[derive(Debug)]
struct Token {
    text: String,
    col: f64,
}

fn tokenize(s: &str, unit: Unit) -> Result<Vec<Token>, String> {
    if unit == Unit::Math {
        let t = ranim_text::TextMobject::math(s)?;
        return Ok(t
            .tokens
            .iter()
            .map(|tok| Token {
                text: tok.key.to_string(),
                col: t.glyphs[tok.glyphs.start].path.center().x,
            })
            .collect());
    }
    Ok(match unit {
        Unit::Char => s
            .chars()
            .enumerate()
            .filter(|(_, c)| !c.is_whitespace())
            .map(|(col, c)| Token {
                text: c.to_string(),
                col: col as f64,
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
                        out.push(Token {
                            text,
                            col: st as f64,
                        });
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
                col: col as f64,
            })
            .collect(),
        Unit::Math => unreachable!(),
    })
}

fn is_operator(s: &str) -> bool {
    // Not just ASCII: Typst sets minus as `−`.
    !s.is_empty() && s != "rule" && s.chars().all(|c| !c.is_alphanumeric())
}

fn render(a: &[Token], b: &[Token], ops: &[Op], unit: Unit) -> String {
    let sep = if unit == Unit::Line { "\n" } else { " " };
    let join = |t: &[Token]| {
        t.iter().map(|t| t.text.as_str()).collect::<Vec<_>>().join(
            if matches!(unit, Unit::Char | Unit::Math) {
                ""
            } else {
                " "
            },
        )
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

    fn parse(args: &[&str]) -> Result<DiffArgs, clap::Error> {
        let cli = Cli::try_parse_from(["ranim", "diff"].iter().chain(args))?;
        let Command::Diff(a) = cli.command else {
            unreachable!()
        };
        Ok(a)
    }

    fn run(args: &[&str]) -> String {
        parse(args).unwrap().run().unwrap()
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
    fn math() {
        // SPEC Appendix B: `b²` travels, `+` morphs into `−`. Scripts are marked with `'`.
        assert_eq!(
            run(&["a^2 + b^2 = c^2", "a^2 = c^2 - b^2", "--math"]),
            "=𝑎 =2' == =𝑐 =2' ~(+→−) ↷𝑏 ↷2'\n"
        );
    }

    #[test]
    fn bad_args() {
        assert!(parse(&["only-one"]).is_err());
        assert!(parse(&["a", "b", "--by", "glyph"]).is_err());
        assert!(parse(&["a", "b", "--by", "math"]).is_err());
        assert!(parse(&["a", "b", "--cleanup", "x"]).is_err());
    }

    #[test]
    fn cli_is_well_formed() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
