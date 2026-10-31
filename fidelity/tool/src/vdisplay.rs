//! A private virtual desktop: Xvfb on a display of its own, a private session D-Bus so
//! the accessibility bus is this display's alone, Openbox when available. Never the
//! owner's display. Every process is started in its own process group and the groups
//! are killed on drop, so nothing lingers.
use crate::util::{err, which, Result};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub struct VirtualDisplay {
    pub display: String,
    pub env: BTreeMap<String, String>,
    pub runtime_dir: PathBuf,
    pub wm: Option<String>,
    children: Vec<(String, Child)>,
}

fn base_env() -> BTreeMap<String, String> {
    let drop = [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "DBUS_SESSION_BUS_ADDRESS",
        "XAUTHORITY",
        "XDG_SESSION_TYPE",
        "AT_SPI_BUS_ADDRESS",
        "XDG_RUNTIME_DIR",
        "GTK_MODULES",
        "NO_AT_BRIDGE",
        "GTK_A11Y",
        "QT_ACCESSIBILITY",
        "QT_LINUX_ACCESSIBILITY_ALWAYS_ON",
        "XDG_CURRENT_DESKTOP",
        "DESKTOP_SESSION",
        "GNOME_DESKTOP_SESSION_ID",
        "SESSION_MANAGER",
    ];
    std::env::vars()
        .filter(|(k, _)| !drop.contains(&k.as_str()))
        .collect()
}

fn spawn_group(
    name: &str,
    cmd: &str,
    args: &[&str],
    env: &BTreeMap<String, String>,
    stdout: Stdio,
) -> Result<Child> {
    let mut c = Command::new(cmd);
    c.args(args)
        .env_clear()
        .envs(env)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(Stdio::null())
        .process_group(0);
    c.spawn().map_err(|e| format!("{name} ({cmd}): {e}"))
}

impl VirtualDisplay {
    pub fn start(width: u32, height: u32, want_wm: bool) -> Result<Self> {
        if which("Xvfb").is_none() {
            return err("Xvfb is not installed");
        }
        let owner = std::env::var("DISPLAY").unwrap_or_default();
        let mut children = Vec::new();
        let env0 = base_env();
        // Pick a display number nobody holds; Xvfb fails fast when one is taken.
        let mut chosen = None;
        for n in 90..200u32 {
            let d = format!(":{n}");
            if d == owner
                || PathBuf::from(format!("/tmp/.X{n}-lock")).exists()
                || PathBuf::from(format!("/tmp/.X11-unix/X{n}")).exists()
            {
                continue;
            }
            let child = spawn_group(
                "Xvfb",
                "Xvfb",
                &[
                    &d,
                    "-screen",
                    "0",
                    &format!("{width}x{height}x24"),
                    "-nolisten",
                    "tcp",
                    "-ac",
                    "+extension",
                    "MIT-SCREEN-SAVER",
                ],
                &env0,
                Stdio::null(),
            )?;
            let socket = PathBuf::from(format!("/tmp/.X11-unix/X{n}"));
            let start = Instant::now();
            let mut child = child;
            let mut ok = false;
            while start.elapsed() < Duration::from_secs(10) {
                if let Ok(Some(_)) = child.try_wait() {
                    break;
                }
                if socket.exists() {
                    ok = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(30));
            }
            if ok {
                children.push(("Xvfb".to_owned(), child));
                chosen = Some(d);
                break;
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        let Some(display) = chosen else {
            return err("no free X display between :90 and :199");
        };
        if display == owner {
            return err("refusing to use the owner's display");
        }
        let runtime_dir = std::env::temp_dir().join(format!(
            "cwfid-vd-{}-{}",
            std::process::id(),
            display.trim_start_matches(':')
        ));
        std::fs::create_dir_all(&runtime_dir).map_err(|e| e.to_string())?;
        let mut env = env0.clone();
        env.insert("DISPLAY".into(), display.clone());
        env.insert("XDG_SESSION_TYPE".into(), "x11".into());
        env.insert(
            "XDG_RUNTIME_DIR".into(),
            runtime_dir.to_string_lossy().into_owned(),
        );
        env.insert("GTK_MODULES".into(), "gail:atk-bridge".into());
        env.insert("NO_AT_BRIDGE".into(), "0".into());
        env.insert("GTK_A11Y".into(), "atspi".into());
        env.insert("QT_ACCESSIBILITY".into(), "1".into());
        env.insert("QT_LINUX_ACCESSIBILITY_ALWAYS_ON".into(), "1".into());
        env.insert("ACCESSIBILITY_ENABLED".into(), "1".into());
        env.insert("GSETTINGS_BACKEND".into(), "memory".into());
        env.insert("XDG_CURRENT_DESKTOP".into(), "GNOME".into());
        // A private session bus; AT-SPI's bus launcher is activated on it.
        let mut dbus = spawn_group(
            "dbus-daemon",
            "dbus-daemon",
            &["--session", "--nofork", "--print-address=1"],
            &env,
            Stdio::piped(),
        )?;
        let address = {
            let stdout = dbus.stdout.take().ok_or("dbus-daemon: no stdout")?;
            let mut line = String::new();
            BufReader::new(stdout)
                .read_line(&mut line)
                .map_err(|e| e.to_string())?;
            line.trim().to_owned()
        };
        if address.is_empty() {
            return err("dbus-daemon printed no address");
        }
        children.push(("dbus-daemon".to_owned(), dbus));
        env.insert("DBUS_SESSION_BUS_ADDRESS".into(), address);
        let mut wm = None;
        if want_wm && which("openbox").is_some() {
            let rc = runtime_dir.join("openbox-rc.xml");
            std::fs::write(&rc, OPENBOX_RC).map_err(|e| e.to_string())?;
            let child = spawn_group(
                "openbox",
                "openbox",
                &["--sm-disable", "--config-file", &rc.to_string_lossy()],
                &env,
                Stdio::null(),
            )?;
            children.push(("openbox".to_owned(), child));
            wm = Some("openbox".to_owned());
            std::thread::sleep(Duration::from_millis(500));
        }
        Ok(Self {
            display,
            env,
            runtime_dir,
            wm,
            children,
        })
    }
    /// Start a child in this display, in its own process group.
    pub fn spawn(
        &mut self,
        name: &str,
        cmd: &str,
        args: &[&str],
        extra_env: &BTreeMap<String, String>,
        cwd: Option<&std::path::Path>,
    ) -> Result<u32> {
        let mut env = self.env.clone();
        env.extend(extra_env.iter().map(|(k, v)| (k.clone(), v.clone())));
        let mut c = Command::new(cmd);
        c.args(args)
            .env_clear()
            .envs(&env)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0);
        if let Some(d) = cwd {
            c.current_dir(d);
        }
        let child = c.spawn().map_err(|e| format!("{name} ({cmd}): {e}"))?;
        let pid = child.id();
        self.children.push((name.to_owned(), child));
        Ok(pid)
    }
    /// One frame of the whole screen. `xwd` fails with BadWindow when a window closes
    /// under it, so a failed dump is retried a few times before it is an error.
    pub fn grab(&self) -> Result<cw_fidelity::Image> {
        let mut last = String::new();
        for attempt in 0..4 {
            if attempt > 0 {
                std::thread::sleep(Duration::from_millis(60 * attempt));
            }
            let mut c = Command::new("xwd");
            c.args(["-root", "-silent"])
                .env_clear()
                .envs(&self.env)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let out = c.output().map_err(|e| format!("xwd: {e}"))?;
            if out.status.success() {
                match crate::pngio::parse_xwd(&out.stdout) {
                    Ok(img) => return Ok(img),
                    Err(e) => last = e,
                }
            } else {
                last = format!("xwd: {}", String::from_utf8_lossy(&out.stderr).trim());
            }
        }
        err(last)
    }
    pub fn alive(&mut self, name: &str) -> bool {
        self.children
            .iter_mut()
            .any(|(n, c)| n == name && matches!(c.try_wait(), Ok(None)))
    }
    fn kill_all(&mut self) {
        for (_, child) in self.children.iter().rev() {
            let _ = Command::new("kill")
                .args(["-TERM", "--", &format!("-{}", child.id())])
                .status();
        }
        std::thread::sleep(Duration::from_millis(300));
        for (_, child) in self.children.iter_mut().rev() {
            let _ = Command::new("kill")
                .args(["-KILL", "--", &format!("-{}", child.id())])
                .status();
            let _ = child.kill();
            let _ = child.wait();
        }
        self.children.clear();
        let _ = std::fs::remove_dir_all(&self.runtime_dir);
    }
}
impl Drop for VirtualDisplay {
    fn drop(&mut self) {
        self.kill_all();
    }
}

const OPENBOX_RC: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<openbox_config xmlns="http://openbox.org/3.4/rc">
  <focus><focusNew>yes</focusNew><followMouse>no</followMouse></focus>
  <placement><policy>Smart</policy></placement>
  <theme><name>Clearlooks</name><titleLayout>NLIMC</titleLayout></theme>
  <desktops><number>1</number></desktops>
  <keyboard></keyboard>
  <mouse><dragThreshold>3</dragThreshold><doubleClickTime>300</doubleClickTime>
    <context name="Client"><mousebind button="Left" action="Press"><action name="Focus"/><action name="Raise"/></mousebind></context>
    <context name="Titlebar"><mousebind button="Left" action="Press"><action name="Focus"/><action name="Raise"/></mousebind><mousebind button="Left" action="Drag"><action name="Move"/></mousebind></context>
  </mouse>
  <applications/>
</openbox_config>
"#;
