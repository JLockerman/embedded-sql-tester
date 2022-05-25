use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};

use clap::Parser;

mod test_parser;
mod test_runner;
mod colors;
mod db_output;

#[derive(clap::Parser, Debug)]
struct Args {
    #[clap(short, long)]
    host: Option<String>,

    #[clap(short, long)]
    port: Option<u16>,

    #[clap(short = 'a', long)]
    password: Option<String>,

    #[clap(short, long, default_value = "/*--[sql-tests]")]
    start_marker: String,

    #[clap(short, long, default_value = "*/")]
    end_marker: String,

    // #[clap(short = 'x', long, default_value_t = vec!["rs".to_string(), "c".to_string(), "h".to_string()])]
    // extensions: Vec<String>,
    input_paths: Vec<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    main_with_args(&args).await
}

async fn main_with_args(args: &Args) -> Result<()> {
    if args.input_paths.is_empty() {
        bail!("no input files provided")
    }
    let (tests, errors): (Vec<_>, Vec<_>) = args
        .input_paths
        .iter()
        .flat_map(|p| extract_tests_from_path(&p, &args.start_marker, &args.end_marker))
        .partition(|t| t.is_ok());

    if !errors.is_empty() {
        let errors: String = errors
            .into_iter()
            .map(|e| format!("{}", e.unwrap_err()))
            .collect();
        bail!("{errors}");
    }
    test_runner::run(&args, tests.into_iter().map(|t| t.unwrap())).await?;
    // let tests = parsed;
    // dbg!(tests);
    Ok(())
}

fn extract_tests_from_path(
    path: &Path,
    start_marker: &str,
    end_marker: &str,
) -> Vec<Result<TestFile>> {
    ignore::WalkBuilder::new(path)
        .follow_links(true)
        .sort_by_file_path(|a, b| a.cmp(b))
        .build()
        .into_iter()
        .filter(|entry| {
            // TODO user plugable extension filter
            entry
                .as_ref()
                .map(|e| {
                    e.file_type().map(|t| t.is_file()).unwrap_or(false)
                        && (matches!(
                            e.path().extension().map(|e| e.to_str().unwrap()),
                            Some("rs") | Some("h") | Some("c") | Some("md")
                        ) || e.path() == path)
                })
                .unwrap_or(false)
        })
        .map(|entry| -> Result<TestFile> {
            let entry =
                entry.with_context(|| format!("could not read file `{}`", path.display()))?;

            let realpath;
            let path = if let Some(true) = entry.file_type().map(|f| f.is_symlink()) {
                realpath = fs::read_link(entry.path()).unwrap();
                &*realpath
            } else {
                entry.path()
            };

            let contents = fs::read_to_string(path)
                .with_context(|| format!("could not read file `{}`", path.display()))?;

            if path.extension().map(|e| e.to_str().unwrap()) == Some("md") {
                extract_all_tests_from_file(&*path.to_string_lossy(), &contents)
            } else {
                extract_marked_tests_from_file(
                    &*path.to_string_lossy(),
                    &contents,
                    start_marker,
                    end_marker,
                )
            }
        })
        .collect()
}

fn extract_all_tests_from_file(
    path: &str,
    contents: &str,
) -> Result<TestFile> {
    let tests = test_parser::extract_tests_from_string(contents);
    let stateless = tests.iter().all(|t| t.transactional);
    let file = TestFile {
        name: path.to_string(),
        stateless,
        tests,
    };
    Ok(file)
}

fn extract_marked_tests_from_file(
    path: &str,
    contents: &str,
    start_marker: &str,
    end_marker: &str,
) -> Result<TestFile> {
    let mut stateless = true;
    let mut tests = vec![];

    let test_blocks = find_marked_tests_blocks(contents, start_marker, end_marker)
        .with_context(|| format!("failed to read tests from `{}`", path))?;
    for (_, test_block) in test_blocks {
        let mut test = test_parser::extract_tests_from_string(test_block);
        for t in &mut test {
            stateless &= t.transactional;
            t.line += 0; // TODO fixup based on where blocks start
        }
        if !test.is_empty() {
            tests.extend(test);
        }
    }
    let file = TestFile {
        name: path.to_string(),
        stateless,
        tests,
    };
    Ok(file)
}

fn find_marked_tests_blocks<'f>(
    file: &'f str,
    start_marker: &'f str,
    end_marker: &'f str,
) -> Result<Vec<(usize, &'f str)>> {
    file.match_indices(start_marker)
        .map(move |(start, _)| -> Result<_> {
            let after_start = &file[start..];
            let end = after_start
                .find(end_marker)
                .ok_or_else(|| anyhow!("could not find test end"))?;
            let test_start = start_marker.len();
            let test = &after_start[test_start..end];
            Ok((start, test))
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
#[must_use]
pub struct TestFile {
    name: String,
    stateless: bool,
    tests: Vec<Test>,
}

#[derive(Debug, PartialEq, Eq)]
#[must_use]
pub struct Test {
    line: usize,
    header: String,
    text: String,
    output: Vec<Vec<String>>,
    transactional: bool,
    ignore_output: bool,
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    // Example tests that will be tested
    /*--[sql-tests]
    Single test
    # Test Parsing is correct
    ```SQL
    select * from foo
    ```
    ```output
    ```
    */
    /*--[sql-tests]
    Multiple tests in one block
    # Test Parsing is correct
    ```SQL
    select * from foo
    ```
    ```output
    ```

    ```SQL
    select * from multiline
    ```
    ```output
    ?column?
    ----------
        value
    ```

    ## ignored
    ```SQL,ignore
    select * from foo
    ```

    ## non-transactional
    ```SQL,non-transactional
    select * from bar
    ```
    ```output, precision(1: 3)
    a | b
    ---+---
    1 | 2
    ```

    ## no output
    ```SQL,ignore-output
    select * from baz
    ```

    ## end by header
    ```SQL
    select * from quz
    ```

    ## end by file
    ```SQL
    select * from qat
    ```
    */

    #[test]
    fn test_block_finding_finds_tests() {
        let this_file = std::fs::read_to_string(file!()).unwrap_or_else(|e| {
            panic!("could not read the source '{}' file due to: {}", file!(), e)
        });
        let blocks: Vec<_> = find_marked_tests_blocks(&this_file, "/*--[sql-tests]", "*/")
            .expect("could not parse file")
            .into_iter()
            .map(|(_, s)| s)
            .collect();
        let args = "\")]
    start_marker: String,

    #[clap(short, long, default_value = \"";
        let first_test = "
    Single test
    # Test Parsing is correct
    ```SQL
    select * from foo
    ```
    ```output
    ```
    ";
        let second_test = "
    Multiple tests in one block
    # Test Parsing is correct
    ```SQL
    select * from foo
    ```
    ```output
    ```

    ```SQL
    select * from multiline
    ```
    ```output
    ?column?
    ----------
        value
    ```

    ## ignored
    ```SQL,ignore
    select * from foo
    ```

    ## non-transactional
    ```SQL,non-transactional
    select * from bar
    ```
    ```output, precision(1: 3)
    a | b
    ---+---
    1 | 2
    ```

    ## no output
    ```SQL,ignore-output
    select * from baz
    ```

    ## end by header
    ```SQL
    select * from quz
    ```

    ## end by file
    ```SQL
    select * from qat
    ```
    ";
        // we see the markers in the tests as well, this is probably fine since it
        // won't occur in user code
        let expected_blocks = [args, first_test, second_test, r#"", ""#, r#"", ""#];
        assert_eq!(blocks, expected_blocks);
        // assert!(
        //     blocks == expected_blocks,
        //     "  left: {}\n right: {}",
        //     blocks.join("\n"),
        //     expected_blocks.join("\n")
        // )
    }

    #[test]
    fn test_parsing_this_file_works() {
        let path = Path::new(file!());
        let tests: Result<Vec<_>> = extract_tests_from_path(path, "/*--[sql-tests]", "*/")
            .into_iter()
            .collect();
        let tests = tests.expect("could not parse file");
        let expected = vec![TestFile {
            name: file!().to_string(),
            stateless: false,
            tests: vec![
                Test {
                    line: 4,
                    header: "`Test Parsing is correct`".to_string(),
                    text: "select * from foo".to_string(),
                    output: vec![],
                    transactional: true,
                    ignore_output: false,
                },
                Test {
                    line: 4,
                    header: "`Test Parsing is correct`".to_string(),
                    text: "select * from foo".to_string(),
                    output: vec![],
                    transactional: true,
                    ignore_output: false,
                },
                Test {
                    line: 10,
                    header: "`Test Parsing is correct`".to_string(),
                    text: "select * from multiline".to_string(),
                    output: vec![vec!["value".to_string()]],
                    transactional: true,
                    ignore_output: false,
                },
                Test {
                    line: 25,
                    header: "`Test Parsing is correct``non-transactional`".to_string(),
                    text: "select * from bar".to_string(),
                    output: vec![vec!["1".to_string(), "2".to_string()]],
                    transactional: false,
                    ignore_output: false,
                },
                Test {
                    line: 35,
                    header: "`Test Parsing is correct``no output`".to_string(),
                    text: "select * from baz".to_string(),
                    output: vec![],
                    transactional: true,
                    ignore_output: true,
                },
                Test {
                    line: 40,
                    header: "`Test Parsing is correct``end by header`".to_string(),
                    text: "select * from quz".to_string(),
                    output: vec![],
                    transactional: true,
                    ignore_output: true,
                },
                Test {
                    line: 45,
                    header: "`Test Parsing is correct``end by file`".to_string(),
                    text: "select * from qat".to_string(),
                    output: vec![],
                    transactional: true,
                    ignore_output: true,
                },
            ],
        }];
        assert_eq!(tests, expected)
    }
}
