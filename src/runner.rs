use std::io::Write;
use std::mem::ManuallyDrop;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::thread;
use std::{fs::OpenOptions, time::Instant};

use crate::db_output::FailureInfo;
use crate::db_output::{validate_output, FailureInfo::QueryError};
use crate::{cprintln, db_output, ecprint, ecprintln, Args, Test, TestFile};

use anyhow::{anyhow, bail, Result};
use futures::stream::FuturesOrdered;
use futures::StreamExt;
use tempfile::{tempdir, TempDir};
use xshell::{cmd, Shell};

macro_rules! path {
    ($start:ident $(/ $segment: literal)*) => {
        {
            let root: &std::path::Path = $start.as_ref();
            root $(.join($segment))*
        }
    };
    ($start:ident / $segment: expr) => {
        {
            let root: &Path = $start.as_ref();
            root.join($segment)
        }
    }
}

#[allow(dead_code)]
struct TestsEnv<'a> {
    args: &'a Args,
    sh: &'a Shell,
    temp_dir: ManuallyDrop<TempDir>,
    bindir: &'a str,
    data_dir: &'a Path,
    postmaster: Child,
    port: &'a str,
    out_path: PathBuf,
    err_path: PathBuf,
}

pub(crate) async fn run(args: &Args, tests: impl Iterator<Item = TestFile>) -> Result<()> {
    let sh = Shell::new()?;
    // TODO allow configurable pg_config
    let pg_config = "pg_config";
    let bindir = cmd!(sh, "{pg_config} --bindir").read()?;

    // TODO allow existing DB
    let temp_dir = tempdir()?;
    let data_dir = path!(temp_dir / "data");
    let db_init_location = data_dir.to_string_lossy();

    ecprintln!("Initializing DB" bold blue, " at {db_init_location}");

    let initdb = path!(bindir / "initdb");
    let init_output = cmd!(sh, "{initdb} -D {data_dir} --no-clean --no-sync")
        .quiet()
        .ignore_status()
        .output();
    match init_output {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            let out = String::from_utf8(output.stdout)?;
            let err = String::from_utf8(output.stderr)?;
            bail!("initdb failed with\nout:\n{out}\nerr:\n{err}")
        }
        Err(e) => return Err(e)?,
    }

    let conf_path = path!(data_dir / "postgresql.conf");
    let mut db_conf = OpenOptions::new().append(true).open(&conf_path)?;
    writeln!(
        &mut db_conf,
        "\n# Configuration added by test runner\n\
        log_autovacuum_min_duration = 0\n\
        log_checkpoints = on\n\
        log_line_prefix = '%m %b[%p] %q%a '\n\
        log_lock_waits = on\n\
        log_temp_files = 128kB\n\
        max_prepared_transactions = 2"
    )
    .map_err(|e| {
        anyhow!(
            "failed to write to db_conf at `{}` due to {e}",
            conf_path.display()
        )
    })?;

    // TODO allow user configs

    // TODO better port picking
    let pgport = "1763";
    sh.set_var("PGPORT", pgport);

    ecprint!("Starting postmaster" bold blue, "... ");

    let mut tester = start_postgres(args, &sh, temp_dir, &bindir, &data_dir, pgport)?;

    tester.wait_for_postmaster_start()?;

    // TODO user-specified DBs

    let postmaster_id = tester.postmaster.id();
    eprintln!("running on port {pgport} with PID {postmaster_id}\n");

    let (stateless_tests, stateful_tests): (Vec<_>, Vec<_>) =
        tests.partition(|tests| tests.stateless);

    let t1: usize = stateless_tests.iter().map(|file| file.tests.len()).sum();
    let t2: usize = stateful_tests.iter().map(|file| file.tests.len()).sum();
    let num_tests = t1 + t2;
    println!("running {num_tests} tests");

    let failures1 = tester.run_stateless_tests(stateless_tests).await?;
    let failures2 = tester.run_stateful_tests(stateful_tests).await?;

    if !failures1.is_empty() || !failures2.is_empty() {
        cprintln!("\n", "Failures" bold blue, ":");
        let mut current_file = "";
        for (file_name, test, failure) in failures1.iter().chain(failures2.iter()) {
            if file_name != current_file {
                current_file = file_name;
                cprintln!("\n", "File" bold blue, ": {current_file}\n");
            }
            failure.print(test)
        }
    }

    let num_failed = failures1.len() + failures2.len();
    let num_passed = num_tests - num_failed;
    if failures1.is_empty() && failures2.is_empty() {
        cprintln!("\ntest result: ", "ok" green, ". {num_passed} passed; {num_failed} failed\n");
        // TODO timing
    } else {
        cprintln!("\ntest result: ", "FAILED" bold red, ". {num_passed} passed; {num_failed} failed\n");
        // TODO timing
    }

    Ok(())
}

fn start_postgres<'a>(
    args: &'a Args,
    sh: &'a Shell,
    temp_dir: TempDir,
    bindir: &'a str,
    data_dir: &'a Path,
    port: &'a str,
) -> Result<TestsEnv<'a>> {
    let mut redirect_options = OpenOptions::new();
    redirect_options.create(true).write(true).read(true);
    let out_path = PathBuf::from("postmaster-stdout.temp.log");
    let out = redirect_options.open(&out_path)?;

    let err_path = PathBuf::from("postmaster-stderr.temp.log");
    let err = redirect_options.open(&err_path)?;

    let postgres = path!(bindir / "postgres");
    // TODO shut down postmaster on drop?
    let postmaster = std::process::Command::new(postgres)
        .arg("-D")
        .arg(data_dir)
        .arg("-F")
        // TODO user config debug flag -d 5
        .arg("-c")
        .arg(format!("port={port}"))
        .stdout(out)
        .stderr(err)
        // .arg("-k")
        // .arg(todo!())
        .spawn()?;

    let tester = TestsEnv {
        args,
        sh,
        temp_dir: ManuallyDrop::new(temp_dir),
        bindir,
        data_dir,
        postmaster,
        port,
        out_path,
        err_path,
    };
    Ok(tester)
}

impl<'a> TestsEnv<'a> {
    fn wait_for_postmaster_start(&mut self) -> Result<()> {
        use std::time::Duration;
        let TestsEnv {
            sh,
            bindir,
            postmaster,
            port,
            ..
        } = self;

        let psql = path!(bindir / "psql");

        let wait_start = Instant::now();
        while wait_start.elapsed().as_secs() < 60 {
            let out = cmd!(sh, "{psql} -X postgres -p {port}")
                .quiet()
                .ignore_stderr()
                .ignore_status()
                .output()?;
            if out.status.success() {
                return Ok(());
            }
            if let Some(status) = postmaster.try_wait()? {
                bail!("postmaster failed with {status}")
            }

            thread::sleep(Duration::from_micros(1000000))
        }
        bail!("postmaster did no respond within 60 seconds")
    }

    async fn run_stateless_tests(
        &self,
        tests: Vec<TestFile>,
    ) -> Result<Vec<(String, Test, FailureInfo)>> {
        use tokio::sync::{mpsc, oneshot};
        let TestsEnv { port, .. } = self;

        cprintln!("Stateless tests" bold blue);

        let db = self.createdb(format!("stateless_test_db"))?;

        // TODO make size user-configurable
        let (unused_clients, mut clients) = mpsc::channel(4);

        let mut conns: FuturesOrdered<_> = (0..4)
            .map(|_| async {
                tokio_postgres::connect(
                    &format!("host=localhost port={port} user=postgres dbname=stateless_test_db application_name=tests"),
                    tokio_postgres::NoTls,
                )
                .await
            })
            .collect();

        for conn in (&mut conns).next().await {
            let (client, connection) = conn?;
            tokio::spawn(async move {
                if let Err(e) = connection.await {
                    cprintln!("Error" bold red, " in postgres connection: {e}");
                }
            });
            unused_clients.try_send(client)?;
        }

        let num_tests: usize = tests.iter().map(|file| file.tests.len()).sum();

        let mut results = Vec::with_capacity(num_tests);
        let mut tests = tests.into_iter().flat_map(|file| {
            file.tests
                .into_iter()
                .map(move |test| (file.name.clone(), test))
        });

        while let Some(mut client) = (&mut clients).recv().await {
            let (file, test) = match tests.next() {
                None => break,
                Some(test) => test,
            };
            let (send, recv) = oneshot::channel();
            results.push(recv);
            let unused_clients = unused_clients.clone();
            tokio::spawn(async move {
                let result = {
                    let txn = client.transaction().await;
                    match txn {
                        Err(e) => Err(e),
                        Ok(txn) => {
                            let result = txn.simple_query(&test.text).await;
                            let _ = txn.rollback().await;
                            result
                        }
                    }
                };
                send.send((file, test, result))
                    .unwrap_or_else(|_| unreachable!());
                unused_clients.send(client).await.unwrap();
            });
        }

        let mut current_file = "".to_string();
        let mut failures = vec![];
        for result in results {
            let (file_name, test, result) = result.await?;
            if file_name != current_file {
                current_file = file_name.clone();
                cprintln!("\n", "File" bold blue, ": {current_file}\n");
            }

            print_test_result(file_name, test, result, &mut failures);
        }

        drop(unused_clients);
        drop(clients);

        db.drop()?;

        Ok(failures)
    }

    async fn run_stateful_tests(
        &self,
        tests: Vec<TestFile>,
    ) -> Result<Vec<(String, Test, FailureInfo)>> {
        let TestsEnv { port, .. } = self;

        cprintln!("\nStateful tests" bold blue);

        let mut running_tests = FuturesOrdered::new();
        let mut files = tests.into_iter().rev();

        let test_runner = |test_file: TestFile, db_num: usize| async move {
            let dbname = format!("stateful-tests-{db_num}");
            let db = self.createdb(dbname)?;
            let dbname = &*db;
            let (mut client, connection) = tokio_postgres::connect(
                &format!("host=localhost port={port} user=postgres dbname={dbname} application_name=tests"),
                tokio_postgres::NoTls,
            )
            .await?;

            tokio::spawn(async move {
                if let Err(e) = connection.await {
                    cprintln!("Error" bold red, " in postgres connection: {e}");
                }
            });

            let mut results = Vec::with_capacity(test_file.tests.len());

            for test in test_file.tests {
                let result = if test.transactional {
                    let txn = client.transaction().await?;
                    let result = txn.simple_query(&test.text).await;
                    let _ = txn.rollback().await;
                    result
                } else {
                    // TODO if a stateful test fails to probably invalidates future tests
                    //      abort here and mark them as skipped somehow?
                    client.simple_query(&test.text).await
                };
                results.push((test, result));
            }

            drop(client);
            // TODO do something on error?
            let _ = db.drop();
            Ok::<_, anyhow::Error>((test_file.name, results))
        };

        // TODO make size user-configurable
        // TODO max client
        let mut i = 0;
        for file in (&mut files).take(4) {
            i += 1;
            running_tests.push(test_runner(file, i))
        }

        let mut failures = vec![];

        loop {
            let result = (&mut running_tests).next().await;
            if let Some(results) = result {
                for result in results {
                    let (current_file, result) = result;
                    cprintln!("\n", "File" bold blue, ": {current_file}\n");
                    for (test, result) in result {
                        print_test_result(current_file.clone(), test, result, &mut failures);
                    }
                }
            }
            match (&mut files).next() {
                Some(file) => {
                    i += 1;
                    running_tests.push(test_runner(file, i))
                }
                None => {
                    if running_tests.is_empty() {
                        break;
                    }
                }
            }
        }

        Ok(failures)
    }

    fn createdb(&self, dbname: String) -> Result<DbDropper> {
        use once_cell::sync::OnceCell;

        let Self { bindir, port, .. } = self;

        let sh = Shell::new()?;

        let createdb = path!(bindir / "createdb");
        cmd!(sh, "{createdb} -p {port} {dbname}").quiet().run()?;

        let psql = path!(bindir / "psql");

        static CREATE_ROLE_ONCE: OnceCell<()> = OnceCell::new();

        CREATE_ROLE_ONCE.get_or_try_init(|| {
            let create_role = "CREATE ROLE postgres WITH LOGIN;";
            // TODO print output only on error
            cmd!(sh, "{psql} -X -p {port} -c {create_role} {dbname}")
                .quiet()
                .ignore_stdout()
                .ignore_stderr()
                .run()
        })?;

        Ok(DbDropper {
            dbname,
            sh,
            bindir: bindir.to_string(),
            port: port.to_string(),
        })
    }
}

fn print_test_result(
    file_name: String,
    test: Test,
    result: Result<Vec<tokio_postgres::SimpleQueryMessage>, tokio_postgres::Error>,
    failures: &mut Vec<(String, Test, FailureInfo)>,
) {
    let header = &test.header;
    match result {
        Err(e) => {
            cprintln!("test {header}... ", "FAILED" bold red);
            failures.push((file_name, test, QueryError(e)))
        }
        Ok(query_result) => {
            print!("test {header}... ");
            match validate_output(query_result, &test) {
                db_output::TestResult::Passed => cprintln!("ok" green),
                db_output::TestResult::Failed(failure) => {
                    failures.push((file_name, test, failure));
                    cprintln!("FAILED" bold red)
                }
            }
        }
    }
}

impl<'a> Drop for TestsEnv<'a> {
    fn drop(&mut self) {
        #[cfg(unix)]
        use nix::{
            sys::signal::{kill, SIGTERM},
            unistd::Pid,
        };

        let pid = self.postmaster.id();
        let copy_output_locally = || {
            use std::fs::rename;

            let out_file = format!("postmaster-out.log");
            match rename(&self.out_path, &out_file) {
                Ok(_) => ecprintln!("Postmaster stdout" bold blue, " can be found in {out_file}"),
                Err(err) => cprintln!(
                    "Error" bold red,
                    " could not copy postmaster stdout from `postmaster-stdout.temp.log` due to {err}"
                ),
            };

            let err_file = format!("postmaster-err.log");
            let _ = std::fs::rename(&self.err_path, &err_file).map_err(|err| {
                ecprintln!(
                    "Error" bold red,
                    " could not copy postmaster stderr from `postmaster-stderr.temp.log` due to {err}"
                )
            });
            ecprintln!("Postmaster stderr" bold blue, " can be found in {err_file}");
        };
        match self.postmaster.try_wait() {
            // TODO log output location?
            Ok(Some(_)) => copy_output_locally(),
            Ok(None) => {
                ecprint!("Stopping postmaster" bold blue, "... ");
                #[cfg(unix)]
                let result = kill(Pid::from_raw(self.postmaster.id() as i32), SIGTERM);

                // TODO do this on unix also if the term fails?
                #[cfg(not(unix))]
                let result = self.postmaster.kill();

                // FIXME anything to do with this error?
                match result {
                    Err(error) => {
                        ecprintln!(
                            "ERROR:" bold red,
                            " could not kill postmaster at pid {pid} due to {error}",
                        );
                        // TODO terminate if kill doesn't work?
                    }
                    Ok(_) => {
                        // TODO timeout
                        let _ = self.postmaster.wait();
                        eprintln!("stopped");
                        copy_output_locally();
                        unsafe {
                            ManuallyDrop::drop(&mut self.temp_dir);
                        }
                    } // TODO only on test error?
                }
            }
            Err(err) => ecprintln!(
                "Error" bold red, " could not shut down postgres due to {err}"
            ),
        }
    }
}

#[must_use]
struct DbDropper {
    dbname: String,
    sh: Shell,
    bindir: String,
    port: String,
}

impl DbDropper {
    fn drop(self) -> Result<()> {
        let DbDropper {
            dbname,
            sh,
            bindir,
            port,
        } = self;
        let dropdb = path!(bindir / "dropdb");
        cmd!(sh, "{dropdb} -f -p {port} {dbname}").quiet().run()?;
        Ok(())
    }
}

impl std::ops::Deref for DbDropper {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.dbname
    }
}
