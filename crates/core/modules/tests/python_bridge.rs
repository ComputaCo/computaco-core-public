//! Python run across the module boundary does what Python run in the engine does: the
//! same output, the same files, the same requests, and the same debugger session. The
//! module host here is in-process (the module's `serve`, its questions answered by the
//! core's `answer`), so this is the bridge itself with nothing else between.
use cw_modules::script::{self, Runtime, Served};
use cw_script_host::debug::{
    DebugConfig, DebugTarget, Debugger, SourceBreakpoint, Step, StopEvent,
};
use cw_script_host::memory::MemoryHost;
use cw_script_host::{Invocation, Outcome};

const PYTHON: Served = Served {
    run: cw_pyvm::run,
    run_debug: cw_pyvm::run_debug,
};
const MODULE: Runtime = Runtime {
    name: "python",
    sha256: Some("in-process"),
};
fn host(name: &str, sha256: &str, input: &[u8]) -> Result<Vec<u8>, String> {
    assert_eq!((name, sha256), ("python", "in-process"));
    Ok(script::serve(input, script::answer, &PYTHON))
}

const PROGRAM: &str = r#"
import os, random, time, json, urllib.request
os.makedirs("out/deep", exist_ok=True)
with open("out/deep/a.txt", "w") as f:
    f.write("hello\n")
with open("out/deep/a.txt", "a") as f:
    f.write("again\n")
print(open("out/deep/a.txt").read().splitlines())
print(sorted(os.listdir("out")), os.path.isdir("out/deep"), os.stat("out/deep/a.txt").st_size)
os.rename("out/deep/a.txt", "out/b.txt")
print(os.path.exists("out/deep/a.txt"), os.getcwd())
random.seed()
print(random.random(), int(time.time()))
reply = json.loads(urllib.request.urlopen("http://echo.test/path?q=1").read())
print(reply["method"], reply["path"])
try:
    urllib.request.urlopen("http://nowhere.test/")
except Exception as e:
    print(type(e).__name__)
try:
    open("missing.txt")
except FileNotFoundError as e:
    print(e)
name = input("name? ")
print("hi", name)
"#;

fn invocation() -> Invocation {
    Invocation {
        args: vec!["main.py".into()],
        env: vec![("HOME".into(), "/home/user".into())],
        stdin: "ada\n".into(),
        ..Invocation::default()
    }
}
fn machine() -> MemoryHost {
    MemoryHost::default().with_file("main.py", PROGRAM)
}

#[test]
fn a_program_run_in_the_module_reads_and_writes_the_machine_as_in_the_engine() {
    cw_modules::set_host(Some(host));
    let (mut direct, mut bridged) = (machine(), machine());
    let expected = cw_pyvm::run(&mut direct, &invocation());
    let got = script::run(&MODULE, &mut bridged, &invocation());
    assert_eq!(expected.exit_code, 0, "{}", expected.stderr);
    assert_eq!(got, expected);
    assert_eq!(bridged.files, direct.files);
    assert_eq!(bridged.dirs, direct.dirs);
    assert_eq!(bridged.requests, direct.requests);
    assert_eq!(bridged.rng, direct.rng);
    assert_eq!(
        cw_modules::take_used().get("python").map(String::as_str),
        Some("in-process")
    );
}

#[test]
fn a_module_that_cannot_be_loaded_is_a_failed_command_not_a_crash() {
    let out = script::run(
        &Runtime {
            name: "python",
            sha256: None,
        },
        &mut machine(),
        &invocation(),
    );
    assert_eq!(out.exit_code, 127);
    assert!(out.stderr.contains("could not be loaded"), "{}", out.stderr);
}

/// Stops at a breakpoint, looks at the stopped program the way a debugger front end
/// does, changes a variable, and steps.
struct Inspector {
    config: DebugConfig,
    seen: Vec<String>,
}
impl Debugger for Inspector {
    fn config(&self) -> &DebugConfig {
        &self.config
    }
    fn stopped(&mut self, event: &StopEvent, target: &mut dyn DebugTarget) -> Step {
        self.seen
            .push(format!("{:?} {:?}", event.reason, event.hit_breakpoint_ids));
        for thread in target.threads() {
            for frame in target.stack_trace(thread.id) {
                self.seen
                    .push(format!("{} {}:{}", frame.name, frame.path, frame.line));
                for scope in target.scopes(frame.id) {
                    for v in target.variables(scope.variables_reference) {
                        self.seen
                            .push(format!("  {} = {} ({})", v.name, v.value, v.type_name));
                    }
                }
            }
        }
        self.seen
            .push(format!("{:?}", target.evaluate("x * 2", None)));
        if let Some(frame) = target.stack_trace(1).first() {
            if let Some(scope) = target.scopes(frame.id).first() {
                self.seen.push(format!(
                    "{:?}",
                    target.set_variable(scope.variables_reference, "x", "41")
                ));
            }
        }
        if self.seen.len() > 200 {
            Step::Continue
        } else {
            Step::Next
        }
    }
    fn log(&mut self, text: &str) {
        self.seen.push(format!("log {text}"));
    }
}

#[test]
fn a_debugger_sees_the_same_program_in_the_module_as_in_the_engine() {
    cw_modules::set_host(Some(host));
    let source = "def f(x):\n    y = x + 1\n    return y\nx = 1\nprint(f(x))\nprint(x)\n";
    let config = DebugConfig {
        breakpoints: [(
            "/home/user/main.py".to_string(),
            vec![
                SourceBreakpoint {
                    line: 2,
                    ..SourceBreakpoint::default()
                },
                SourceBreakpoint {
                    line: 5,
                    log_message: Some("x is {x}".into()),
                    ..SourceBreakpoint::default()
                },
            ],
        )]
        .into(),
        stop_on_entry: true,
        ..DebugConfig::default()
    };
    let run = |bridged: bool| -> (Outcome, Vec<String>, MemoryHost) {
        let mut machine = MemoryHost::default().with_file("main.py", source);
        let mut debugger = Inspector {
            config: config.clone(),
            seen: vec![],
        };
        let invocation = invocation();
        let (out, _) = if bridged {
            script::run_debug(&MODULE, &mut machine, &invocation, &mut debugger)
        } else {
            cw_pyvm::run_debug(&mut machine, &invocation, &mut debugger)
        };
        (out, debugger.seen, machine)
    };
    let (expected, seen, _) = run(false);
    assert!(seen.len() > 5, "{seen:?}");
    let (got, got_seen, _) = run(true);
    assert_eq!(got, expected);
    assert_eq!(got_seen, seen);
}
