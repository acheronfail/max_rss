use std::ffi::OsString;
use std::path::PathBuf;
use std::process;

use anyhow::{bail, Result};
use lexopt::Parser;

fn print_version() {
    println!(
        "{crate_name} {crate_version}",
        crate_name = env!("CARGO_PKG_NAME"),
        crate_version = env!("CARGO_PKG_VERSION")
    );
}

fn print_help() {
    println!(
        "{}",
        format!(
            r#"
{crate_name} {crate_version}
{crate_authors}

{crate_name} is a minimal application that can be used to estimate the max
resident set size (max_rss) of a process. Because this application is used to
measure other programs, it writes its results to a JSON file to avoid output
from the measured program and this program interfering with one other.

Project home page: {crate_homepage}

USAGE:
    {bin} [flags] <COMMAND>...
    {bin} [flags] -- <COMMAND>...

OPTIONS:
    -o OUTPUT, --output OUTPUT
        Specify output path for the results JSON file. If not provided it
        defaults to {bin}.json in the current working directory.

    -r, --return-result
        If set, and COMMAND exits with a non-zero exit code, then {bin} itself
        will exit with that same exit code and print an error to stderr.

        Can be disabled with --no-return-result.

EXAMPLES:
    Using {bin} should be more or less the same as using something like `time`:

        {bin} sleep 1
            Run `sleep 1` and measure its max_rss.

        {bin} --return-result --output ./results.json -- find .
            If `find` fails and exits with a non-zero code, then {bin} will
            also exit with that code (--return-result). The results will be
            written to ./results.json, too (--output).
"#,
            bin = env!("CARGO_BIN_NAME"),
            crate_name = env!("CARGO_PKG_NAME"),
            crate_version = env!("CARGO_PKG_VERSION"),
            crate_homepage = env!("CARGO_PKG_HOMEPAGE"),
            crate_authors = env!("CARGO_PKG_AUTHORS")
                .split(':')
                .collect::<Vec<_>>()
                .join("\n")
                .trim(),
        )
        .trim()
    );
}

#[derive(Debug)]
pub struct Args {
    pub return_result: bool,
    pub output: PathBuf,

    pub command: Vec<OsString>,
}

impl Default for Args {
    fn default() -> Self {
        Args {
            return_result: false,
            output: PathBuf::from(format!("./{}.json", env!("CARGO_BIN_NAME"))),
            command: vec![],
        }
    }
}

impl Args {
    pub fn parse() -> Result<Args> {
        Args::parse_impl(lexopt::Parser::from_env())
    }

    fn parse_impl(mut parser: Parser) -> Result<Args> {
        use lexopt::prelude::*;

        let mut args = Args::default();

        while let Some(arg) = parser.next()? {
            match arg {
                // -r, --return-result, --no-return-result
                Short('r') | Long("return-result") => args.return_result = true,
                Long("no-return-result") => args.return_result = false,

                // -o=X, --output=X
                Short('o') | Long("output") => {
                    args.output = parser.value()?.into();
                }

                // -h, --help
                Short('h') | Long("help") => {
                    print_help();
                    process::exit(0);
                }

                // -v, --version
                Short('v') | Long("version") => {
                    print_version();
                    process::exit(0);
                }

                // collect the rest of the arguments as the command to run
                Value(other) => {
                    args.command.push(other);
                    args.command.extend(parser.raw_args()?);
                }

                _ => bail!(arg.unexpected()),
            }
        }

        if args.command.is_empty() {
            print_help();
            bail!("No command was given.");
        }

        Ok(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! args {
        () => {
            Args::parse_impl(Parser::from_args(Vec::<OsString>::new()))
        };
        ($($x:expr $(,)?)+) => {
            Args::parse_impl(Parser::from_args([$($x,)+]))
        };
    }

    #[test]
    fn command() -> Result<()> {
        assert_eq!(args!("--return-result", "--", "foo")?.command, vec!["foo"]);
        assert_eq!(args!("foo")?.command, vec!["foo"]);
        assert_eq!(
            args!("foo", "--return-result")?.command,
            vec!["foo", "--return-result"]
        );
        assert_eq!(
            args!("--return-result", "foo", "--output=bar")?.command,
            vec!["foo", "--output=bar"]
        );
        Ok(())
    }

    #[test]
    fn command_required() -> Result<()> {
        assert!(args!().is_err());
        assert!(args!("--return-result").is_err());
        assert!(args!("--return-result", "--").is_err());
        Ok(())
    }

    #[test]
    fn output() -> Result<()> {
        assert_eq!(args!("ls", ".")?.output, PathBuf::from("./max_rss.json"));
        assert_eq!(
            args!("--output=foo", "ls", ".")?.output,
            PathBuf::from("foo")
        );
        Ok(())
    }

    #[test]
    fn return_result() -> Result<()> {
        assert_eq!(args!("foo")?.return_result, false);
        assert_eq!(args!("-r", "foo")?.return_result, true);
        assert_eq!(args!("--return-result", "foo")?.return_result, true);
        assert_eq!(
            args!("-r", "--no-return-result", "foo")?.return_result,
            false
        );
        assert_eq!(
            args!("--return-result", "--no-return-result", "foo")?.return_result,
            false
        );
        Ok(())
    }
}
