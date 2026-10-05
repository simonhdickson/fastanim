//! The `fastanim` command-line tool (see `docs/SPEC.md` §9).
//!
//! `fastanim diff` and `fastanim run` exist so far; the other commands arrive with later milestones.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use fastanim_core::BakedTimeline;
use fastanim_diff::{Algorithm, Cleanup, DiffOptions, Differ, Op, TieBreak};

/// fastanim: programmatic mathematical animation
#[derive(Debug, Parser)]
#[command(name = "fastanim", version)]
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
    /// Run a Rhai scene script: preview it (reloading on save), or export it
    Run(RunArgs),
    /// List every function and constant a scene script can use
    ScriptApi,
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
        Command::Run(a) => {
            return match a.run() {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            };
        }
        Command::ScriptApi => {
            for sig in fastanim_script::signatures() {
                println!("{sig}");
            }
            return ExitCode::SUCCESS;
        }
        Command::New => {
            eprintln!("error: `fastanim new` is not implemented yet");
            return ExitCode::FAILURE;
        }
        Command::Preview => "preview",
        Command::Render => "render",
        Command::Still => "still",
    };
    eprintln!(
        "error: `fastanim {cmd}` needs a scene crate, which `fastanim new` will scaffold. \
         Until then, call `fastanim_bevy::run(construct)` from your scene's `main` and use \
         `cargo run -- {cmd} --help`."
    );
    ExitCode::FAILURE
}

#[derive(Debug, clap::Args)]
struct RunArgs {
    /// The scene script
    script: PathBuf,
    /// Instead of playing it, write its typeset text to a `.bundle` beside it for the web player
    #[arg(long)]
    bundle: bool,
    #[command(subcommand)]
    command: Option<fastanim_bevy::Command>,
}

impl RunArgs {
    fn run(self) -> Result<(), String> {
        if self.bundle && self.command.is_some() {
            return Err("--bundle doesn't take a command".into());
        }
        let tl = bake(&self.script)?;
        if self.bundle {
            let out = self.script.with_extension("bundle");
            return fs::write(&out, fastanim_text::export_bundle())
                .map_err(|e| format!("{}: {e}", out.display()));
        }
        let preview = matches!(self.command, None | Some(fastanim_bevy::Command::Preview));
        let reload = preview.then(|| watch(self.script.clone()));
        fastanim_bevy::run_command(self.command, tl, reload)
    }
}

/// Runs the script at `path`; errors are `path:line:col: message`.
fn bake(path: &Path) -> Result<BakedTimeline, String> {
    let src = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    fastanim_script::bake(&src).map_err(|e| match e.line {
        0 => format!("{}: {e}", path.display()),
        _ => format!("{}:{e}", path.display()),
    })
}

/// Re-bakes `path` whenever its modification time changes. A broken script is reported and
/// the last good timeline kept.
fn watch(path: PathBuf) -> fastanim_bevy::Reload {
    let mtime = |p: &Path| fs::metadata(p).and_then(|m| m.modified()).ok();
    let mut seen = mtime(&path);
    Box::new(move || {
        let now = mtime(&path);
        if now == seen {
            return None;
        }
        seen = now;
        (bake(&path).inspect(|_| eprintln!("reloaded {}", path.display())))
            .inspect_err(|e| eprintln!("error: {e}"))
            .ok()
    })
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
        let t = fastanim_text::TextMobject::math(s)?;
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
        let cli = Cli::try_parse_from(["fastanim", "diff"].iter().chain(args))?;
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
