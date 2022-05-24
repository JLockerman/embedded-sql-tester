# Embedded SQL Test Runner

**NOTE:** This repo is currently a PoC.

Test SQL embedded in other files.

This tool runs SQL tests embedded in C, Rust, and Markdown files. In source code
it looks input/output pairs in block comments that start with `--[sql-tests]`
like the following (indentation due to markdown):
```rust
/*--[sql-tests]
    ```sql
    ```
    SELECT sum(i) FROM generate_series(1, 10) i;
    ```output
     sum
    -----
      55
    ```
 */

fn main() {
    todo!()
}
```

In markdown it will run any SQL codeblock comparing it with an output if one
exists, for instance:

```SQL
SELECT 5, 6 FROM generate_series(1, 3);
```
```output
 a | b
---+---
 5 | 6
 5 | 6
 5 | 6
```

or

```SQL
SELECT 'this string is ignored';
```

The tester works on this file! An example of the output when running
`cargo run -- .` can be found in [`./example.out`](./example.out). Though it's
better in color ;)