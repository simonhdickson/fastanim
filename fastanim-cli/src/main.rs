//! The `fastanim` command-line tool (see `docs/SPEC.md` §9).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::OnceLock;
use std::time::Instant;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use fastanim_bevy::{Command as BevyCommand, RenderArgs, StillArgs};
use fastanim_core::BakedTimeline;
use fastanim_diff::{Algorithm, Cleanup, DiffOptions, Differ, Op, TieBreak};

mod lsp;

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
    /// Open a Rhai scene in a window with a scrubber, reloading it on save
    Preview {
        /// The scene script
        script: PathBuf,
    },
    /// Export a Rhai scene as video or frames
    Render {
        /// The scene script
        script: PathBuf,
        #[command(flatten)]
        args: RenderArgs,
    },
    /// Export one frame of a Rhai scene
    Still {
        /// The scene script
        script: PathBuf,
        #[command(flatten)]
        args: StillArgs,
    },
    /// Run Rhai scenes without a window and report their errors as `path:line:col: message`
    Check {
        /// The scene scripts
        #[arg(required = true)]
        scripts: Vec<PathBuf>,
        /// How many times a second to sample each timeline, running its closures; 0 to skip
        #[arg(long, default_value_t = 30.0)]
        fps: f32,
    },
    /// Bake a Rhai scene repeatedly and print where the time goes
    Profile {
        /// The scene script
        script: PathBuf,
        /// Warm bakes to average, after the first, cold one
        #[arg(long, default_value_t = 10)]
        runs: u32,
    },
    /// List every function and constant a scene script can use
    ScriptApi,
    /// Run a language server for scene scripts on stdin and stdout: errors, completion, hover
    Lsp,
    /// Print a shell completion script, e.g. `fastanim completions fish | source`
    Completions {
        /// The shell to complete for
        shell: Shell,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<(), String> {
    match command {
        Command::Diff(a) => a.run().map(|out| print!("{out}")),
        Command::Preview { script } => {
            let tl = bake(&script)?;
            fastanim_bevy::run_command(None, &name(&script), tl, Some(watch(script)))
        }
        Command::Render { script, args } => {
            let cmd = Some(BevyCommand::Render(args));
            fastanim_bevy::run_command(cmd, &name(&script), bake(&script)?, None)
        }
        Command::Still { script, args } => {
            let cmd = Some(BevyCommand::Still(args));
            fastanim_bevy::run_command(cmd, &name(&script), bake(&script)?, None)
        }
        Command::Check { scripts, fps } => check(&scripts, fps),
        Command::Profile { script, runs } => profile(&script, runs),
        Command::ScriptApi => {
            fastanim_script::signatures()
                .iter()
                .for_each(|sig| println!("{sig}"));
            Ok(())
        }
        Command::Lsp => lsp::run(),
        Command::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "fastanim",
                &mut std::io::stdout(),
            );
            Ok(())
        }
    }
}

/// What an export of the script at `path` is called by default: its file name, without `.rhai`.
fn name(path: &Path) -> String {
    (path.file_stem()).map_or("out".into(), |s| s.to_string_lossy().into_owned())
}

/// Runs the script at `path`, with relative paths in it relative to its directory; errors are
/// `path:line:col: message`.
fn bake(path: &Path) -> Result<BakedTimeline, String> {
    load(path, fastanim_script::bake_in)
}

/// `run` on the script at `path` and its directory, with errors as in [`bake`].
fn load(
    path: &Path,
    run: impl FnOnce(&str, &Path) -> Result<BakedTimeline, fastanim_script::Error>,
) -> Result<BakedTimeline, String> {
    let src = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let dir = path.parent().unwrap_or(Path::new(""));
    run(&src, dir).map_err(|e| match e.line {
        0 => format!("{}: {e}", path.display()),
        _ => format!("{}:{e}", path.display()),
    })
}

/// Bakes and samples each script, printing its errors to stderr and a line per good one to
/// stdout, and fails if any failed.
fn check(scripts: &[PathBuf], fps: f32) -> Result<(), String> {
    let failed = (scripts.iter())
        .filter(|path| {
            let checked = load(path, |src, dir| fastanim_script::check_in(src, dir, fps));
            match checked {
                Ok(tl) => println!("{}: ok, {}s", path.display(), tl.duration()),
                Err(e) => {
                    eprintln!("{e}");
                    return true;
                }
            }
            false
        })
        .count();
    match failed {
        0 => Ok(()),
        n => Err(format!("{n} of {} scripts failed", scripts.len())),
    }
}

/// Bakes `path` once with an empty text cache, then `runs` times with a warm one, as the
/// playground does after an edit, and prints where the time went.
fn profile(path: &Path, runs: u32) -> Result<(), String> {
    fn now() -> f64 {
        static EPOCH: OnceLock<Instant> = OnceLock::new();
        EPOCH.get_or_init(Instant::now).elapsed().as_secs_f64() * 1e3
    }
    fastanim_core::time_baking(now);
    fastanim_text::time_typesetting(now);
    // total, typeset, plan, keyframes, and the rest: running the script.
    let once = || -> Result<([f64; 5], fastanim_core::BakeStats), String> {
        let (t0, typeset0) = (now(), fastanim_text::typeset_ms());
        let tl = bake(path)?;
        let total = now() - t0;
        let typeset = fastanim_text::typeset_ms() - typeset0;
        let s = tl.stats();
        let script = total - typeset - s.plan_ms - s.snapshot_ms;
        Ok(([total, typeset, s.plan_ms, s.snapshot_ms, script], s))
    };
    let (cold, stats) = once()?;
    let mut warm = [0.0; 5];
    for _ in 0..runs {
        let (t, _) = once()?;
        warm.iter_mut()
            .zip(t)
            .for_each(|(w, t)| *w += t / runs as f64);
    }
    println!(
        "{} clips, {} mobjects copied into keyframes",
        stats.clips, stats.snapshot_mobjects
    );
    let header = ["total", "typeset", "plan", "keyframes", "script"];
    println!("{:<8}{}", "ms", header.map(|h| format!("{h:>11}")).concat());
    let row = |name: &str, t: [f64; 5]| {
        println!("{name:<8}{}", t.map(|v| format!("{v:>11.2}")).concat());
    };
    row("cold", cold);
    if runs > 0 {
        row("warm", warm);
    }
    Ok(())
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
        Unit::Math => {
            let t = fastanim_text::TextMobject::math(s)?;
            t.tokens
                .iter()
                .map(|tok| Token {
                    text: tok.key.to_string(),
                    col: t.glyphs[tok.glyphs.start].path.center().x,
                })
                .collect()
        }
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
        Cli::command().debug_assert();
    }
}
