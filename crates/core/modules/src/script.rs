//! A language runtime served by a module (Python): the program runs in the module, and
//! everything it does to the machine goes back to the core through [`ScriptHost`].
//!
//! The engine calls the module with the invocation ([`run`], [`run_debug`]). While the
//! program runs, the module asks the core for each host call (its files, the world's
//! network, child processes, the clock and entropy) on the core channel; the engine
//! answers from the host of the run that is waiting ([`answer`]), so the run reads and
//! writes the machine exactly as a build carrying the interpreter does, and its journal
//! records the same calls. A child process that runs the same runtime is a call of its
//! own, made while the parent waits; the host gives it another instance.
//!
//! Under a debugger the module asks the core at each stop ([`Debugger::stopped`]); the
//! engine's debugger inspects the stopped program through calls back into the module's
//! running call ([`crate::REENTER`]), answered by the program's own [`DebugTarget`].
use crate::wire::{Reader, Writer};
use cw_script_host::debug::{
    DebugConfig, DebugRunInfo, DebugTarget, Debugger, ExceptionFilters, Scope, SourceBreakpoint,
    StackFrame, Step, StopEvent, StopReason, Thread, Variable,
};
use cw_script_host::{
    FileStat, FsError, FsErrorKind, HttpRequest, HttpResponse, Invocation, NetError, NetErrorKind,
    Outcome, ScriptHost, SpawnProgram, SpawnRequest, TcpConnection,
};
use std::cell::{Cell, RefCell};

/// A runtime served by a module: its name (`modules/<name>.wasm`) and the SHA-256 this
/// engine was built with.
pub struct Runtime {
    pub name: &'static str,
    pub sha256: Option<&'static str>,
}

mod op {
    pub const RUN: u8 = 1;
    pub const DEBUG: u8 = 2;
    // Host calls, module to core.
    pub const READ_FILE: u8 = 1;
    pub const WRITE_FILE: u8 = 2;
    pub const STAT: u8 = 3;
    pub const LSTAT: u8 = 4;
    pub const LIST_DIR: u8 = 5;
    pub const MKDIR: u8 = 6;
    pub const REMOVE: u8 = 7;
    pub const RENAME: u8 = 8;
    pub const CWD: u8 = 9;
    pub const CHDIR: u8 = 10;
    pub const RESOLVE: u8 = 11;
    pub const NOW: u8 = 12;
    pub const RANDOM: u8 = 13;
    pub const USER: u8 = 14;
    pub const HOSTNAME: u8 = 15;
    pub const PID: u8 = 16;
    pub const OS_FAMILY: u8 = 17;
    pub const HTTP: u8 = 18;
    pub const RESOLVE_HOST: u8 = 19;
    pub const TCP_CONNECT: u8 = 20;
    pub const LOCAL_ADDRESS: u8 = 21;
    pub const SPAWN: u8 = 22;
    pub const SCHEDULER_SEED: u8 = 23;
    pub const STOPPED: u8 = 30;
    pub const LOG: u8 = 31;
    // The stopped program, core to module (after `REENTER`).
    pub const THREADS: u8 = 1;
    pub const STACK_TRACE: u8 = 2;
    pub const SCOPES: u8 = 3;
    pub const VARIABLES: u8 = 4;
    pub const EVALUATE: u8 = 5;
    pub const SET_VARIABLE: u8 = 6;
}

// ------------------------------------------------------------------ the engine's side

/// The run a module's host calls are answered from: the innermost one waiting.
struct Frame {
    host: *mut (dyn ScriptHost + 'static),
    debugger: Option<*mut (dyn Debugger + 'static)>,
    runtime: (&'static str, Option<&'static str>),
}
thread_local! {
    static FRAMES: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };
}

/// Runs `invocation` in the runtime's module, answering its host calls from `host`.
pub fn run(runtime: &Runtime, host: &mut dyn ScriptHost, invocation: &Invocation) -> Outcome {
    let mut request = Writer::new();
    request.u8(op::RUN);
    put_invocation(&mut request, invocation);
    match with_frame(runtime, host, None, &request.finish()) {
        Ok(reply) => get_outcome(&mut Reader::new(&reply)),
        Err(e) => unavailable(runtime, &e),
    }
}
/// [`run`] under a debugger, which the module consults at every stop.
pub fn run_debug(
    runtime: &Runtime,
    host: &mut dyn ScriptHost,
    invocation: &Invocation,
    debugger: &mut dyn Debugger,
) -> (Outcome, DebugRunInfo) {
    let mut request = Writer::new();
    request.u8(op::DEBUG);
    put_invocation(&mut request, invocation);
    put_config(&mut request, debugger.config());
    match with_frame(runtime, host, Some(debugger), &request.finish()) {
        Ok(reply) => {
            let mut r = Reader::new(&reply);
            let out = get_outcome(&mut r);
            let info = DebugRunInfo {
                suspended: r.bool(),
                terminated: r.bool(),
            };
            (out, info)
        }
        Err(e) => (unavailable(runtime, &e), DebugRunInfo::default()),
    }
}
fn unavailable(runtime: &Runtime, error: &str) -> Outcome {
    Outcome::new(
        "",
        format!("{}: could not be loaded: {error}\n", runtime.name),
        127,
    )
}
fn with_frame(
    runtime: &Runtime,
    host: &mut dyn ScriptHost,
    debugger: Option<&mut dyn Debugger>,
    request: &[u8],
) -> Result<Vec<u8>, String> {
    // SAFETY: the pointers are used only by `answer`, while this call is on the stack
    // and its own references to `host` and `debugger` are not (it waits on the module),
    // and they are popped before it returns.
    let frame = unsafe {
        Frame {
            host: std::mem::transmute::<*mut (dyn ScriptHost + '_), *mut (dyn ScriptHost + 'static)>(
                host,
            ),
            debugger: debugger.map(|d| {
                std::mem::transmute::<*mut (dyn Debugger + '_), *mut (dyn Debugger + 'static)>(d)
            }),
            runtime: (runtime.name, runtime.sha256),
        }
    };
    FRAMES.with(|f| f.borrow_mut().push(frame));
    let reply = crate::call(runtime.name, runtime.sha256, request);
    FRAMES.with(|f| f.borrow_mut().pop());
    reply
}

/// The core's answer to a host call from the module whose run is waiting innermost.
pub fn answer(request: &[u8]) -> Vec<u8> {
    let Some((host, debugger, runtime)) = FRAMES.with(|f| {
        f.borrow()
            .last()
            .map(|frame| (frame.host, frame.debugger, frame.runtime))
    }) else {
        return Vec::new();
    };
    // SAFETY: see `with_frame`; the frame's run is waiting on this module call.
    let host = unsafe { &mut *host };
    let mut r = Reader::new(request);
    let mut w = Writer::new();
    match r.u8() {
        op::READ_FILE => put_fs(&mut w, host.read_file(&r.str()), |w, d| {
            w.bytes(&d);
        }),
        op::WRITE_FILE => {
            let (path, data, append) = (r.str(), r.bytes(), r.bool());
            put_fs(&mut w, host.write_file(&path, &data, append), |_, ()| {})
        }
        op::STAT => put_fs(&mut w, host.stat(&r.str()), put_stat),
        op::LSTAT => put_fs(&mut w, host.lstat(&r.str()), put_stat),
        op::LIST_DIR => put_fs(&mut w, host.list_dir(&r.str()), |w, names| {
            w.strs(&names);
        }),
        op::MKDIR => {
            let (path, parents) = (r.str(), r.bool());
            put_fs(&mut w, host.mkdir(&path, parents), |_, ()| {})
        }
        op::REMOVE => {
            let (path, recursive) = (r.str(), r.bool());
            put_fs(&mut w, host.remove(&path, recursive), |_, ()| {})
        }
        op::RENAME => {
            let (from, to) = (r.str(), r.str());
            put_fs(&mut w, host.rename(&from, &to), |_, ()| {})
        }
        op::CWD => {
            w.str(&host.cwd());
        }
        op::CHDIR => put_fs(&mut w, host.chdir(&r.str()), |_, ()| {}),
        op::RESOLVE => {
            w.str(&host.resolve(&r.str()));
        }
        op::NOW => {
            w.i64(host.now_micros());
        }
        op::RANDOM => {
            w.u64(host.random_u64());
        }
        op::USER => {
            w.str(&host.user());
        }
        op::HOSTNAME => {
            w.str(&host.hostname());
        }
        op::PID => {
            w.u64(host.pid());
        }
        op::OS_FAMILY => {
            w.str(&host.os_family());
        }
        op::HTTP => {
            let request = HttpRequest {
                method: r.str(),
                url: r.str(),
                headers: r.pairs(),
                body: r.bytes(),
                timeout_micros: r.bool().then(|| r.u64()),
            };
            put_net(&mut w, host.http(&request), |w, response| {
                w.u16(response.status)
                    .pairs(&response.headers)
                    .bytes(&response.body)
                    .u64(response.elapsed_micros);
            })
        }
        op::RESOLVE_HOST => put_net(&mut w, host.resolve_host(&r.str()), |w, names| {
            w.strs(&names);
        }),
        op::TCP_CONNECT => {
            let (name, port) = (r.str(), r.u16());
            put_net(&mut w, host.tcp_connect(&name, port), |w, c| {
                w.str(&c.remote_address)
                    .u16(c.remote_port)
                    .str(&c.local_address)
                    .u16(c.local_port)
                    .u64(c.elapsed_micros);
            })
        }
        op::LOCAL_ADDRESS => {
            w.str(&host.local_address());
        }
        op::SPAWN => {
            let program = if r.u8() == 0 {
                SpawnProgram::Shell(r.str())
            } else {
                SpawnProgram::Argv(r.strs())
            };
            let request = SpawnRequest {
                program,
                stdin: r.str(),
                cwd: r.opt_str(),
                env: r.bool().then(|| r.pairs()),
            };
            put_fs(&mut w, host.spawn(&request), put_outcome)
        }
        op::SCHEDULER_SEED => {
            w.u64(host.scheduler_seed());
        }
        op::STOPPED => {
            let event = get_event(&mut r);
            let step = match debugger {
                // SAFETY: as `host` above.
                Some(debugger) => unsafe { &mut *debugger }.stopped(
                    &event,
                    &mut RemoteTarget {
                        name: runtime.0,
                        sha256: runtime.1,
                    },
                ),
                None => Step::Continue,
            };
            w.u8(step.tag());
        }
        op::LOG => {
            if let Some(debugger) = debugger {
                // SAFETY: as `host` above.
                unsafe { &mut *debugger }.log(&r.str());
            }
        }
        _ => {}
    }
    w.finish()
}

/// The program a module stopped, inspected through calls back into its running call.
struct RemoteTarget {
    name: &'static str,
    sha256: Option<&'static str>,
}
impl RemoteTarget {
    fn ask(&mut self, write: impl FnOnce(&mut Writer)) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(crate::REENTER);
        write(&mut w);
        crate::call(self.name, self.sha256, &w.finish()).unwrap_or_default()
    }
}
impl DebugTarget for RemoteTarget {
    fn threads(&mut self) -> Vec<Thread> {
        let reply = self.ask(|w| {
            w.u8(op::THREADS);
        });
        let mut r = Reader::new(&reply);
        (0..r.u32())
            .map(|_| Thread {
                id: r.u64(),
                name: r.str(),
            })
            .collect()
    }
    fn stack_trace(&mut self, thread_id: u64) -> Vec<StackFrame> {
        let reply = self.ask(|w| {
            w.u8(op::STACK_TRACE).u64(thread_id);
        });
        let mut r = Reader::new(&reply);
        (0..r.u32())
            .map(|_| StackFrame {
                id: r.u64(),
                name: r.str(),
                path: r.str(),
                line: r.u32(),
                column: r.u32(),
            })
            .collect()
    }
    fn scopes(&mut self, frame_id: u64) -> Vec<Scope> {
        let reply = self.ask(|w| {
            w.u8(op::SCOPES).u64(frame_id);
        });
        let mut r = Reader::new(&reply);
        (0..r.u32())
            .map(|_| Scope {
                name: r.str(),
                variables_reference: r.u64(),
                expensive: r.bool(),
            })
            .collect()
    }
    fn variables(&mut self, variables_reference: u64) -> Vec<Variable> {
        let reply = self.ask(|w| {
            w.u8(op::VARIABLES).u64(variables_reference);
        });
        let mut r = Reader::new(&reply);
        (0..r.u32()).map(|_| get_variable(&mut r)).collect()
    }
    fn evaluate(&mut self, expression: &str, frame_id: Option<u64>) -> Result<Variable, String> {
        let reply = self.ask(|w| {
            w.u8(op::EVALUATE).str(expression).bool(frame_id.is_some());
            if let Some(id) = frame_id {
                w.u64(id);
            }
        });
        get_variable_result(&mut Reader::new(&reply))
    }
    fn set_variable(
        &mut self,
        variables_reference: u64,
        name: &str,
        value: &str,
    ) -> Result<Variable, String> {
        let reply = self.ask(|w| {
            w.u8(op::SET_VARIABLE)
                .u64(variables_reference)
                .str(name)
                .str(value);
        });
        get_variable_result(&mut Reader::new(&reply))
    }
}

// ------------------------------------------------------------------ the module's side

/// How a module asks the core one host call: the call's bytes in, the answer out.
pub type Ask = fn(&[u8]) -> Vec<u8>;
/// The runtime a module serves: a plain run, and a run under a debugger.
pub struct Served {
    pub run: fn(&mut dyn ScriptHost, &Invocation) -> Outcome,
    pub run_debug: for<'a> fn(
        &'a mut dyn ScriptHost,
        &Invocation,
        &'a mut dyn Debugger,
    ) -> (Outcome, DebugRunInfo),
}

thread_local! {
    /// The program stopped in the debugger, while the core inspects it.
    static STOPPED: Cell<Option<*mut (dyn DebugTarget + 'static)>> = const { Cell::new(None) };
}

/// A module's answer to one call from the engine.
pub fn serve(input: &[u8], ask: Ask, served: &Served) -> Vec<u8> {
    let mut r = Reader::new(input);
    let mut w = Writer::new();
    match r.u8() {
        op::RUN => {
            let invocation = get_invocation(&mut r);
            put_outcome(&mut w, (served.run)(&mut RemoteHost(ask), &invocation));
        }
        op::DEBUG => {
            let invocation = get_invocation(&mut r);
            let mut debugger = RemoteDebugger {
                ask,
                config: get_config(&mut r),
            };
            let (out, info) = (served.run_debug)(&mut RemoteHost(ask), &invocation, &mut debugger);
            put_outcome(&mut w, out);
            w.bool(info.suspended).bool(info.terminated);
        }
        crate::REENTER => {
            if let Some(target) = STOPPED.with(Cell::get) {
                // SAFETY: set by `RemoteDebugger::stopped` for the program stopped below
                // this call on the module's stack, and cleared before it resumes.
                inspect(unsafe { &mut *target }, &mut r, &mut w);
            }
        }
        _ => {}
    }
    w.finish()
}
fn inspect(target: &mut dyn DebugTarget, r: &mut Reader, w: &mut Writer) {
    match r.u8() {
        op::THREADS => {
            let threads = target.threads();
            w.u32(threads.len() as u32);
            threads.iter().for_each(|t| {
                w.u64(t.id).str(&t.name);
            });
        }
        op::STACK_TRACE => {
            let frames = target.stack_trace(r.u64());
            w.u32(frames.len() as u32);
            frames.iter().for_each(|f| {
                w.u64(f.id)
                    .str(&f.name)
                    .str(&f.path)
                    .u32(f.line)
                    .u32(f.column);
            });
        }
        op::SCOPES => {
            let scopes = target.scopes(r.u64());
            w.u32(scopes.len() as u32);
            scopes.iter().for_each(|s| {
                w.str(&s.name).u64(s.variables_reference).bool(s.expensive);
            });
        }
        op::VARIABLES => {
            let variables = target.variables(r.u64());
            w.u32(variables.len() as u32);
            variables.iter().for_each(|v| put_variable(w, v));
        }
        op::EVALUATE => {
            let expression = r.str();
            let frame = r.bool().then(|| r.u64());
            put_variable_result(w, target.evaluate(&expression, frame));
        }
        op::SET_VARIABLE => {
            let (reference, name, value) = (r.u64(), r.str(), r.str());
            put_variable_result(w, target.set_variable(reference, &name, &value));
        }
        _ => {}
    }
}

/// The machine, as a program in a module sees it: every call asked of the core.
struct RemoteHost(Ask);
impl RemoteHost {
    fn ask(&self, write: impl FnOnce(&mut Writer)) -> Vec<u8> {
        let mut w = Writer::new();
        write(&mut w);
        (self.0)(&w.finish())
    }
}
impl ScriptHost for RemoteHost {
    fn read_file(&mut self, path: &str) -> Result<Vec<u8>, FsError> {
        get_fs(
            &self.ask(|w| {
                w.u8(op::READ_FILE).str(path);
            }),
            |r| r.bytes(),
        )
    }
    fn write_file(&mut self, path: &str, data: &[u8], append: bool) -> Result<(), FsError> {
        get_fs(
            &self.ask(|w| {
                w.u8(op::WRITE_FILE).str(path).bytes(data).bool(append);
            }),
            |_| (),
        )
    }
    fn stat(&mut self, path: &str) -> Result<FileStat, FsError> {
        get_fs(
            &self.ask(|w| {
                w.u8(op::STAT).str(path);
            }),
            get_stat,
        )
    }
    fn lstat(&mut self, path: &str) -> Result<FileStat, FsError> {
        get_fs(
            &self.ask(|w| {
                w.u8(op::LSTAT).str(path);
            }),
            get_stat,
        )
    }
    fn list_dir(&mut self, path: &str) -> Result<Vec<String>, FsError> {
        get_fs(
            &self.ask(|w| {
                w.u8(op::LIST_DIR).str(path);
            }),
            |r| r.strs(),
        )
    }
    fn mkdir(&mut self, path: &str, parents: bool) -> Result<(), FsError> {
        get_fs(
            &self.ask(|w| {
                w.u8(op::MKDIR).str(path).bool(parents);
            }),
            |_| (),
        )
    }
    fn remove(&mut self, path: &str, recursive: bool) -> Result<(), FsError> {
        get_fs(
            &self.ask(|w| {
                w.u8(op::REMOVE).str(path).bool(recursive);
            }),
            |_| (),
        )
    }
    fn rename(&mut self, from: &str, to: &str) -> Result<(), FsError> {
        get_fs(
            &self.ask(|w| {
                w.u8(op::RENAME).str(from).str(to);
            }),
            |_| (),
        )
    }
    fn cwd(&self) -> String {
        Reader::new(&self.ask(|w| {
            w.u8(op::CWD);
        }))
        .str()
    }
    fn chdir(&mut self, path: &str) -> Result<(), FsError> {
        get_fs(
            &self.ask(|w| {
                w.u8(op::CHDIR).str(path);
            }),
            |_| (),
        )
    }
    fn resolve(&self, path: &str) -> String {
        Reader::new(&self.ask(|w| {
            w.u8(op::RESOLVE).str(path);
        }))
        .str()
    }
    fn now_micros(&self) -> i64 {
        Reader::new(&self.ask(|w| {
            w.u8(op::NOW);
        }))
        .i64()
    }
    fn random_u64(&mut self) -> u64 {
        Reader::new(&self.ask(|w| {
            w.u8(op::RANDOM);
        }))
        .u64()
    }
    fn user(&self) -> String {
        Reader::new(&self.ask(|w| {
            w.u8(op::USER);
        }))
        .str()
    }
    fn hostname(&self) -> String {
        Reader::new(&self.ask(|w| {
            w.u8(op::HOSTNAME);
        }))
        .str()
    }
    fn pid(&self) -> u64 {
        Reader::new(&self.ask(|w| {
            w.u8(op::PID);
        }))
        .u64()
    }
    fn os_family(&self) -> String {
        Reader::new(&self.ask(|w| {
            w.u8(op::OS_FAMILY);
        }))
        .str()
    }
    fn http(&mut self, request: &HttpRequest) -> Result<HttpResponse, NetError> {
        let reply = self.ask(|w| {
            w.u8(op::HTTP)
                .str(&request.method)
                .str(&request.url)
                .pairs(&request.headers)
                .bytes(&request.body)
                .bool(request.timeout_micros.is_some());
            if let Some(t) = request.timeout_micros {
                w.u64(t);
            }
        });
        get_net(&reply, |r| HttpResponse {
            status: r.u16(),
            headers: r.pairs(),
            body: r.bytes(),
            elapsed_micros: r.u64(),
        })
    }
    fn resolve_host(&mut self, name: &str) -> Result<Vec<String>, NetError> {
        get_net(
            &self.ask(|w| {
                w.u8(op::RESOLVE_HOST).str(name);
            }),
            |r| r.strs(),
        )
    }
    fn tcp_connect(&mut self, host: &str, port: u16) -> Result<TcpConnection, NetError> {
        get_net(
            &self.ask(|w| {
                w.u8(op::TCP_CONNECT).str(host).u16(port);
            }),
            |r| TcpConnection {
                remote_address: r.str(),
                remote_port: r.u16(),
                local_address: r.str(),
                local_port: r.u16(),
                elapsed_micros: r.u64(),
            },
        )
    }
    fn local_address(&self) -> String {
        Reader::new(&self.ask(|w| {
            w.u8(op::LOCAL_ADDRESS);
        }))
        .str()
    }
    fn spawn(&mut self, request: &SpawnRequest) -> Result<Outcome, FsError> {
        let reply = self.ask(|w| {
            w.u8(op::SPAWN);
            match &request.program {
                SpawnProgram::Shell(line) => w.u8(0).str(line),
                SpawnProgram::Argv(argv) => w.u8(1).strs(argv),
            };
            w.str(&request.stdin).opt_str(request.cwd.as_deref());
            w.bool(request.env.is_some());
            if let Some(env) = &request.env {
                w.pairs(env);
            }
        });
        get_fs(&reply, get_outcome)
    }
    fn scheduler_seed(&mut self) -> u64 {
        Reader::new(&self.ask(|w| {
            w.u8(op::SCHEDULER_SEED);
        }))
        .u64()
    }
}

/// The engine's debugger, as a program in a module reports to it.
struct RemoteDebugger {
    ask: Ask,
    config: DebugConfig,
}
impl Debugger for RemoteDebugger {
    fn config(&self) -> &DebugConfig {
        &self.config
    }
    fn stopped(&mut self, event: &StopEvent, target: &mut dyn DebugTarget) -> Step {
        let mut w = Writer::new();
        w.u8(op::STOPPED);
        put_event(&mut w, event);
        // SAFETY: cleared below, before `target` is used again by the program.
        let target = unsafe {
            std::mem::transmute::<*mut (dyn DebugTarget + '_), *mut (dyn DebugTarget + 'static)>(
                target,
            )
        };
        let held = STOPPED.with(|s| s.replace(Some(target)));
        let reply = (self.ask)(&w.finish());
        STOPPED.with(|s| s.set(held));
        Step::from_tag(Reader::new(&reply).u8())
    }
    fn log(&mut self, text: &str) {
        let mut w = Writer::new();
        w.u8(op::LOG).str(text);
        (self.ask)(&w.finish());
    }
}

// ------------------------------------------------------------------ the shapes

fn put_invocation(w: &mut Writer, i: &Invocation) {
    w.strs(&i.args)
        .pairs(&i.env)
        .str(&i.stdin)
        .bool(i.interactive)
        .bool(i.eof);
}
fn get_invocation(r: &mut Reader) -> Invocation {
    Invocation {
        args: r.strs(),
        env: r.pairs(),
        stdin: r.str(),
        interactive: r.bool(),
        eof: r.bool(),
    }
}
fn put_outcome(w: &mut Writer, o: Outcome) {
    w.str(&o.stdout)
        .str(&o.stderr)
        .i32(o.exit_code)
        .bool(o.awaiting_input)
        .u64(o.elapsed_micros);
}
fn get_outcome(r: &mut Reader) -> Outcome {
    Outcome {
        stdout: r.str(),
        stderr: r.str(),
        exit_code: r.i32(),
        awaiting_input: r.bool(),
        elapsed_micros: r.u64(),
    }
}
const FS_KINDS: [FsErrorKind; 7] = [
    FsErrorKind::NotFound,
    FsErrorKind::Exists,
    FsErrorKind::NotADirectory,
    FsErrorKind::IsADirectory,
    FsErrorKind::NotEmpty,
    FsErrorKind::PermissionDenied,
    FsErrorKind::Invalid,
];
fn put_fs<T>(w: &mut Writer, result: Result<T, FsError>, ok: impl FnOnce(&mut Writer, T)) {
    match result {
        Ok(value) => {
            w.u8(0);
            ok(w, value);
        }
        Err(e) => {
            let kind = FS_KINDS.iter().position(|k| *k == e.kind).unwrap_or(6) as u8;
            w.u8(1).u8(kind);
        }
    }
}
fn get_fs<T>(reply: &[u8], ok: impl FnOnce(&mut Reader) -> T) -> Result<T, FsError> {
    let mut r = Reader::new(reply);
    match r.u8() {
        0 => Ok(ok(&mut r)),
        _ => Err(FsError::new(FS_KINDS[usize::from(r.u8()).min(6)])),
    }
}
fn put_net<T>(w: &mut Writer, result: Result<T, NetError>, ok: impl FnOnce(&mut Writer, T)) {
    match result {
        Ok(value) => {
            w.u8(0);
            ok(w, value);
        }
        Err(e) => {
            w.u8(1).u8(e.kind.index()).str(&e.message);
        }
    }
}
fn get_net<T>(reply: &[u8], ok: impl FnOnce(&mut Reader) -> T) -> Result<T, NetError> {
    let mut r = Reader::new(reply);
    match r.u8() {
        0 => Ok(ok(&mut r)),
        _ => {
            let kind = NetErrorKind::from_index(r.u8());
            Err(NetError::new(kind, r.str()))
        }
    }
}
fn put_stat(w: &mut Writer, s: FileStat) {
    w.u64(s.size)
        .bool(s.is_dir)
        .bool(s.is_symlink)
        .u32(s.mode)
        .i64(s.mtime_micros)
        .u64(s.inode)
        .u32(s.links)
        .str(&s.owner);
}
fn get_stat(r: &mut Reader) -> FileStat {
    FileStat {
        size: r.u64(),
        is_dir: r.bool(),
        is_symlink: r.bool(),
        mode: r.u32(),
        mtime_micros: r.i64(),
        inode: r.u64(),
        links: r.u32(),
        owner: r.str(),
    }
}
fn put_config(w: &mut Writer, c: &DebugConfig) {
    w.u32(c.breakpoints.len() as u32);
    for (path, breakpoints) in &c.breakpoints {
        w.str(path).u32(breakpoints.len() as u32);
        for b in breakpoints {
            w.u32(b.line)
                .opt_str(b.condition.as_deref())
                .opt_str(b.hit_condition.as_deref())
                .opt_str(b.log_message.as_deref());
        }
    }
    w.bool(c.exceptions.raised)
        .bool(c.exceptions.uncaught)
        .bool(c.stop_on_entry)
        .bool(c.pause_after.is_some());
    if let Some(n) = c.pause_after {
        w.u64(n);
    }
}
fn get_config(r: &mut Reader) -> DebugConfig {
    let breakpoints = (0..r.u32())
        .map(|_| {
            let path = r.str();
            let list = (0..r.u32())
                .map(|_| SourceBreakpoint {
                    line: r.u32(),
                    condition: r.opt_str(),
                    hit_condition: r.opt_str(),
                    log_message: r.opt_str(),
                })
                .collect();
            (path, list)
        })
        .collect();
    DebugConfig {
        breakpoints,
        exceptions: ExceptionFilters {
            raised: r.bool(),
            uncaught: r.bool(),
        },
        stop_on_entry: r.bool(),
        pause_after: r.bool().then(|| r.u64()),
    }
}
const REASONS: [StopReason; 5] = [
    StopReason::Entry,
    StopReason::Breakpoint,
    StopReason::Step,
    StopReason::Pause,
    StopReason::Exception,
];
fn put_event(w: &mut Writer, e: &StopEvent) {
    let reason = REASONS.iter().position(|r| *r == e.reason).unwrap_or(0) as u8;
    w.u8(reason)
        .u64(e.thread_id)
        .str(&e.description)
        .str(&e.text)
        .u32(e.hit_breakpoint_ids.len() as u32);
    e.hit_breakpoint_ids.iter().for_each(|id| {
        w.u64(*id);
    });
}
fn get_event(r: &mut Reader) -> StopEvent {
    StopEvent {
        reason: REASONS[usize::from(r.u8()).min(4)],
        thread_id: r.u64(),
        description: r.str(),
        text: r.str(),
        hit_breakpoint_ids: (0..r.u32()).map(|_| r.u64()).collect(),
    }
}
fn put_variable(w: &mut Writer, v: &Variable) {
    w.str(&v.name)
        .str(&v.value)
        .str(&v.type_name)
        .u64(v.variables_reference)
        .u64(v.named_variables)
        .u64(v.indexed_variables);
}
fn get_variable(r: &mut Reader) -> Variable {
    Variable {
        name: r.str(),
        value: r.str(),
        type_name: r.str(),
        variables_reference: r.u64(),
        named_variables: r.u64(),
        indexed_variables: r.u64(),
    }
}
fn put_variable_result(w: &mut Writer, v: Result<Variable, String>) {
    match v {
        Ok(v) => {
            w.u8(0);
            put_variable(w, &v);
        }
        Err(e) => {
            w.u8(1).str(&e);
        }
    }
}
fn get_variable_result(r: &mut Reader) -> Result<Variable, String> {
    match r.u8() {
        0 => Ok(get_variable(r)),
        _ => Err(r.str()),
    }
}
