
use tokio_postgres::SimpleQueryMessage;

use crate::cprintln;
use crate::Test;

use self::FailureInfo::*;
use self::TestResult::*;

pub enum TestResult {
    Passed,
    Failed(FailureInfo),
}

pub enum FailureInfo {
    QueryError(tokio_postgres::Error),
    WrongNumberOfRows {
        received: Vec<Vec<String>>,
        expected: usize,
        found: usize,
    },
    MismatchedValues(Vec<Vec<String>>),
}

pub(crate) fn validate_output(output: Vec<SimpleQueryMessage>, test: &Test) -> TestResult {
    use SimpleQueryMessage::*;

    if test.ignore_output {
        return Passed;
    }

    let mut received = Vec::with_capacity(test.output.len());
    for r in output {
        match r {
            Row(r) => {
                let mut row: Vec<String> = Vec::with_capacity(r.len());
                for i in 0..r.len() {
                    row.push(r.get(i).unwrap_or("").to_string())
                }
                received.push(row);
            }
            CommandComplete(..) => break,
            _ => unreachable!(),
        }
    }

    if test.output.len() != received.len() {
        return Failed(WrongNumberOfRows {
            expected: test.output.len(),
            found: received.len(),
            received,
        });
    }

    // let all_eq = iter::zip(test.output.iter(), received.iter())
    //     .all(|(expected, received)| expected == received);

    // TODO we'll need a more complicated version later
    if test.output != received {
        return Failed(MismatchedValues(received));
    }

    Passed
}

impl FailureInfo {
    pub(crate) fn print(&self, test: &Test) {
        let test_name = &test.header;
        let received = match self {
            WrongNumberOfRows { received, .. } => {
                cprintln!("{test_name}" bold, " failed with:\n");
                received
            }
            MismatchedValues(received) => {
                cprintln!("{test_name}" bold," failed with:\n");
                received
            }
            QueryError(error) => {
                cprintln!("{test_name}" bold, " failed due to ", "error" red, ":\n{error}\n");
                return;
            }
        };

        let expected_rows = test.output.len();
        let expected_vals = stringify_table(&test.output);

        let received_rows = received.len();
        let received_vals = stringify_table(&received);

        cprintln!(
            "Expected\n" blue,
            "{expected_vals}\n",
            "({expected_rows} rows)\n" dimmed,
            "Received\n" blue,
            "{received_vals}\n",
            "({received_rows} rows)\n" dimmed,
        );

        print_diff(&test.output, &received);
    }
}

fn stringify_table(table: &[Vec<String>]) -> String {
    use std::{cmp::max, fmt::Write};
    if table.is_empty() {
        return "---".to_string();
    }
    let mut width = vec![0; table[0].len()];
    for row in table {
        // Ensure that we have width for every column
        // TODO this shouldn't be needed, but somtimes is?
        if width.len() < row.len() {
            width.extend((0..row.len() - width.len()).map(|_| 0));
        }
        for (i, value) in row.iter().enumerate() {
            width[i] = max(width[i], value.len())
        }
    }
    let mut output = String::with_capacity(width.iter().sum::<usize>() + width.len() * 3);
    for row in table {
        for (i, value) in row.iter().enumerate() {
            if i != 0 {
                output.push_str(" | ")
            }
            let _ = write!(&mut output, "{:>width$}", value, width = width[i]);
        }
        output.push('\n')
    }

    output
}

fn print_diff(left: &[Vec<String>], right: &[Vec<String>]) {
    use std::{cmp::max, io::Write};
    use termcolor::{Color, ColorSpec, WriteColor};

    cprintln!("Diff" blue);

    static EMPTY_ROW: Vec<String> = vec![];
    static EMPTY_VAL: String = String::new();

    let num_rows = max(left.len(), right.len());
    let mut width = vec![
        0;
        max(
            left.get(0).map(Vec::len).unwrap_or(0),
            right.get(0).map(Vec::len).unwrap_or(0),
        )
    ];
    for i in 0..num_rows {
        let left = left.get(i).unwrap_or(&EMPTY_ROW);
        let right = right.get(i).unwrap_or(&EMPTY_ROW);
        let cols = max(left.len(), right.len());
        for j in 0..cols {
            let left = left.get(j).unwrap_or(&EMPTY_VAL);
            let right = right.get(j).unwrap_or(&EMPTY_VAL);
            if left == right {
                width[j] = max(width[j], left.len())
            } else {
                width[j] = max(width[j], left.len() + right.len() + 2)
            }
        }
    }

    let bufwtr = termcolor::BufferWriter::stdout(*crate::colors::STDOUT_COLOR_CHOICE);
    let mut output = bufwtr.buffer();
    for i in 0..num_rows {
        let left = left.get(i).unwrap_or(&EMPTY_ROW);
        let right = right.get(i).unwrap_or(&EMPTY_ROW);
        let cols = max(left.len(), right.len());
        for j in 0..cols {
            let left = left.get(j).unwrap_or(&EMPTY_VAL);
            let right = right.get(j).unwrap_or(&EMPTY_VAL);
            if j != 0 {
                let _ = write!(&mut output, " | ");
            }
            if left == right {
                let _ = write!(
                    &mut output,
                    "{:>padding$}{left}",
                    "",
                    padding = width[j] - left.len()
                );
            } else {
                let padding = width[j] - (left.len() + right.len() + 2);
                let _ = write!(&mut output, "{:>padding$}", "", padding = padding);
                let _ = output.set_color(ColorSpec::new().set_fg(Some(Color::Magenta)));
                let _ = write!(&mut output, "-{left}");
                let _ = output.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)));
                let _ = write!(&mut output, "+{right}");
                let _ = output.reset();
            };
        }
        let _ = writeln!(&mut output);
    }
    let _ = writeln!(&mut output);
    let _ = bufwtr.print(&output);
}

#[test]
fn t() {
    assert_eq!(1, 2);
}
