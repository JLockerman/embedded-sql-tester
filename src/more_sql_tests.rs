/*--[sql-tests]
# Success
# No rows
```SQL
select from generate_series(1,0);
```
```output
```

# one row
```SQL
select 1;
```
```output
 a
---
 1
```

# many rows
```SQL
select i from generate_series(1, 5) i;
```
```output
  b
----
  1
  2
  3
  4
  5
```

# multi col
```SQL
select i, j from generate_series(1, 3) i, generate_series(1, 3) j;
```
```output
 a | b
-------
 1 | 1
 1 | 2
 1 | 3
 2 | 1
 2 | 2
 2 | 3
 3 | 1
 3 | 2
 3 | 3
```
*/

/*--[sql-tests]
# Failure
# No rows
```SQL
select from generate_series(1,0);
```
```output
 a
---
 1
```

# one row
```SQL
select 2;
```
```output
 a
---
 1
```

# many rows
```SQL
select i from generate_series(1, 5) i;
```
```output
  b
----
  1
  4
  3
  6
  5
```

# multi col
```SQL
select i, j from generate_series(1, 3) i, generate_series(1, 3) j;
```
```output
 a | b
-------
 1 | 1
 2 | 1
 3 | 1
 1 | 2
 2 | 2
 3 | 2
 1 | 3
 2 | 3
 3 | 3
```
*/