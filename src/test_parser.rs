use std::str::Lines;

use crate::Test;

pub fn extract_tests_from_string(s: &str) -> Vec<crate::Test> {
    use self::BlockKind::*;
    use self::Event::*;
    let block_parser = BlockParser::new(s);
    let mut heading_stack = vec!["".to_string()];

    let mut tests = vec![];

    let mut current_test: Option<Test> = None;
    for event in block_parser {
        match event {
            Heading { level, text } => {
                heading_stack.truncate(level);
                heading_stack.push(format!("`{}`", text));
            }
            CodeBlock {
                starting_line,
                attributes,
                contents,
            } => {
                let header = heading_stack.join("");
                match parse_code_block_attrs(attributes) {
                    Sql {
                        ignore_output,
                        stateless,
                    } => {
                        if let Some(mut test) = current_test.take() {
                            test.ignore_output = true;
                            tests.push(test);
                        }
                        let test = Test {
                            line: starting_line,
                            header,
                            text: contents,
                            output: Vec::new(),
                            transactional: stateless,
                            ignore_output,
                        };
                        current_test = Some(test)
                    }
                    Output { ignore } => {
                        let mut test = current_test.take().unwrap_or_else(|| todo!());
                        test.output = parse_output(contents);
                        test.ignore_output = ignore;
                        tests.push(test);
                    }
                    Other => continue,
                }
            }
        }
    }
    if let Some(mut test) = current_test.take() {
        test.ignore_output = true;
        tests.push(test);
    }
    tests
}

enum BlockKind {
    Sql {
        ignore_output: bool,
        stateless: bool,
    },
    Output {
        ignore: bool,
    },
    Other,
}

fn parse_code_block_attrs(attrs: &str) -> BlockKind {
    // TODO incomplete, look at the doctester for the full version
    // TODO error handling
    let mut is_sql = false;
    let mut is_stateful = false;
    let mut is_ignoring_output = false;
    let mut is_output = false;
    let mut is_ignored = false;
    attrs.split(',').for_each(|token| {
        let token = &*token.trim().to_ascii_lowercase();
        match token {
            "output" => is_output = true,
            "sql" => is_sql = true,
            "ignore" => is_ignored = true,
            "stateful" | "non-transactional" => is_stateful = true,
            "ignore-output" => is_ignoring_output = true,
            _ => (),
        }
    });

    if is_ignored {

        return BlockKind::Other;
    }

    if is_output {
        if is_stateful {
            todo!()
        }
        return BlockKind::Output { ignore: is_ignored };
    }

    if is_sql {
        return BlockKind::Sql {
            ignore_output: is_ignored,
            stateless: !is_stateful,
        };
    }

    // TODO warn on other attributes?
    BlockKind::Other
}

fn parse_output(s: String) -> Vec<Vec<String>> {
    s.split('\n') // parse by-line
        .skip(2) // first two lines are column names and a separator
        // .filter(|s| !s.is_empty()) TODO why was this in the original?
        .map(|s| {
            s.split('|')
                .map(|s| s.trim().to_string())
                .collect::<Vec<_>>()
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
enum Event<'s> {
    Heading {
        level: usize,
        text: &'s str,
    },
    CodeBlock {
        starting_line: usize,
        attributes: &'s str,
        contents: String,
    },
}

struct BlockParser<'s> {
    line_num: usize,
    lines: Lines<'s>,
}

impl<'s> BlockParser<'s> {
    fn new(s: &'s str) -> Self {
        Self {
            line_num: 0,
            lines: s.lines(),
        }
    }
}

impl<'s> Iterator for BlockParser<'s> {
    type Item = Event<'s>;

    fn next(&mut self) -> Option<Self::Item> {
        use self::Event::*;
        // TODO this could be done much faster by just searching for the
        //      delimiters but that's more complicated.
        loop {
            let line = match self.lines.next() {
                Some(line) => line,
                _ => return None,
            };

            self.line_num += 1;

            let trimmed = line.trim_start();
            if trimmed.starts_with('#') {
                let level = trimmed.chars().take_while(|&c| c == '#').count();
                let text = trimmed.get(level..).unwrap_or("").trim_start();
                return Some(Heading { text, level });
            } else if trimmed.starts_with("```") {
                let indent_len = line.find("```").unwrap();
                let indent = &line[..indent_len];
                let starting_line = self.line_num;
                let attributes = trimmed.get(3..).unwrap_or("").trim_start();
                let contents: Vec<_> = (&mut self.lines)
                    .take_while(|line| !line.trim_start().starts_with("```"))
                    .map(|l| l.trim_start_matches(indent))
                    .collect();
                self.line_num += contents.len() + 1;
                return Some(CodeBlock {
                    starting_line,
                    attributes,
                    contents: contents.join("\n"),
                });
            }
        }
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    static TEST_CONTENTS: &str = r##"
# Test Parsing
```SQL
select * from foo
```
```output
```

```SQL
select * from multiline;
select * from multiline;
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

## indented

    ```SQL
    select indented;
  select keeps_whitespace;
    ```
    ```output
     ???
    -----
    a | b
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
"##;

    #[test]
    fn block_parser_extracts() {
        use super::Event::*;
        let events: Vec<_> = super::BlockParser::new(TEST_CONTENTS).collect();

        let expected = vec![
            Heading {
                level: 1,
                text: "Test Parsing",
            },
            CodeBlock {
                starting_line: 3,
                attributes: "SQL",
                contents: "select * from foo".to_string(),
            },
            CodeBlock {
                starting_line: 6,
                attributes: "output",
                contents: "".to_string(),
            },
            CodeBlock {
                starting_line: 9,
                attributes: "SQL",
                contents: "select * from multiline;\nselect * from multiline;".to_string(),
            },
            CodeBlock {
                starting_line: 13,
                attributes: "output",
                contents: " ?column?\n----------\n    value".to_string(),
            },
            Heading {
                level: 2,
                text: "ignored",
            },
            CodeBlock {
                starting_line: 20,
                attributes: "SQL,ignore",
                contents: "select * from foo".to_string(),
            },
            Heading {
                level: 2,
                text: "non-transactional",
            },
            CodeBlock {
                starting_line: 25,
                attributes: "SQL,non-transactional",
                contents: "select * from bar".to_string(),
            },
            CodeBlock {
                starting_line: 28,
                attributes: "output, precision(1: 3)",
                contents: " a | b\n---+---\n 1 | 2".to_string(),
            },
            Heading {
                level: 2,
                text: "indented",
            },
            CodeBlock {
                starting_line: 36,
                attributes: "SQL",
                contents: "select indented;\n  select keeps_whitespace;".to_string(),
            },
            CodeBlock {
                starting_line: 40,
                attributes: "output",
                contents: " ???\n-----\na | b".to_string(),
            },
            Heading {
                level: 2,
                text: "no output",
            },
            CodeBlock {
                starting_line: 47,
                attributes: "SQL,ignore-output",
                contents: "select * from baz".to_string(),
            },
            Heading {
                level: 2,
                text: "end by header",
            },
            CodeBlock {
                starting_line: 52,
                attributes: "SQL",
                contents: "select * from quz".to_string(),
            },
            Heading {
                level: 2,
                text: "end by file",
            },
            CodeBlock {
                starting_line: 57,
                attributes: "SQL",
                contents: "select * from qat".to_string(),
            },
        ];
        assert_eq!(events, expected);
    }

    #[test]
    fn extract_tests_extracts() {
        use crate::Test;

        let tests = super::extract_tests_from_string(TEST_CONTENTS);
        let expected = vec![
            Test {
                line: 3,
                header: "`Test Parsing`".to_string(),
                text: "select * from foo".to_string(),
                output: vec![],
                transactional: true,
                ignore_output: false,
            },
            Test {
                line: 9,
                header: "`Test Parsing`".to_string(),
                text: "select * from multiline;\nselect * from multiline;".to_string(),
                output: vec![vec!["value".to_string()]],
                transactional: true,
                ignore_output: false,
            },
            Test {
                line: 25,
                header: "`Test Parsing``non-transactional`".to_string(),
                text: "select * from bar".to_string(),
                output: vec![vec!["1".to_string(), "2".to_string()]],
                transactional: false,
                ignore_output: false,
            },
            Test {
                line: 36,
                header: "`Test Parsing``indented`".to_string(),
                text: "select indented;\n  select keeps_whitespace;".to_string(),
                output: vec![vec!["a".to_string(), "b".to_string()]],
                transactional: true,
                ignore_output: false,
            },
            Test {
                line: 47,
                header: "`Test Parsing``no output`".to_string(),
                text: "select * from baz".to_string(),
                output: vec![],
                transactional: true,
                ignore_output: true,
            },
            Test {
                line: 52,
                header: "`Test Parsing``end by header`".to_string(),
                text: "select * from quz".to_string(),
                output: vec![],
                transactional: true,
                ignore_output: true,
            },
            Test {
                line: 57,
                header: "`Test Parsing``end by file`".to_string(),
                text: "select * from qat".to_string(),
                output: vec![],
                transactional: true,
                ignore_output: true,
            },
        ];
        assert_eq!(tests, expected);
    }
}
