//! SSH between a world's machines: the `openssh-server` package puts `sshd` on a machine
//! (listening on 22 from boot, stopped and started with systemctl), `ssh-keygen` makes a
//! key, `authorized_keys` decides who gets in, and `ssh host command` runs the command on
//! the host as its user, over the world's own network. A replay rebuilds all of it.
use computerworld::World;
use cw_protocol::{ActionEnvelope, EnvironmentConfig, WorldDefinition};
use serde_json::{json, Value};

const DEFINITION: &str = include_str!("../../../worlds/agent-desktop/world.json");

fn world(server_packages: &[&str]) -> (World, String) {
    let mut d: Value = serde_json::from_str(DEFINITION).unwrap();
    d["computers"].as_array_mut().unwrap().push(json!({
        "id": "app-server", "profile": "ubuntu", "address": "10.0.1.20", "user": "ops",
        "installed_apps": ["terminal"], "packages": server_packages,
    }));
    d["network"]["nodes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "app-server", "address": "10.0.1.20", "zone": "local"}));
    d["network"]["links"].as_array_mut().unwrap().push(json!({"from": "workstation", "to": "app-server", "bidirectional": true, "latency_us": 10, "loss_per_million": 0}));
    d["network"]["dns"].as_array_mut().unwrap().push(json!({"name": "app-server.internal", "address": "10.0.1.20", "ttl_us": 60000000, "resolver": "intranet-server"}));
    let definition = WorldDefinition::from_json(&d.to_string()).unwrap();
    let mut world = World::new(definition, 7).unwrap();
    world.interfaces_mut().enable_replay_verification(true);
    let session = world
        .environment(EnvironmentConfig {
            actor: "ada".into(),
            machines: vec!["workstation".into(), "app-server".into()],
            actions: vec!["terminal.v1".into()],
            observations: vec![],
            action_budget: 64,
        })
        .unwrap();
    (world, session)
}

/// (stdout, stderr, exit code) of one command on `machine`.
fn sh(world: &mut World, session: &str, machine: &str, command: &str) -> (String, String, i64) {
    let r = world
        .step(
            session,
            vec![ActionEnvelope::new(
                "terminal.v1",
                "execute",
                machine,
                json!({ "command": command }),
            )],
        )
        .unwrap();
    let v = &r.outcomes[0].value;
    assert!(
        r.outcomes[0].success,
        "{command}: {:?}",
        r.outcomes[0].error
    );
    (
        v["stdout"].as_str().unwrap_or_default().to_owned(),
        v["stderr"].as_str().unwrap_or_default().to_owned(),
        v["exit_code"].as_i64().unwrap_or(-1),
    )
}

#[test]
fn a_key_in_authorized_keys_lets_ssh_run_a_command_on_the_server() {
    let (mut world, session) = world(&["coreutils", "openssh-server"]);
    let initial = world.snapshot();
    let ws = "workstation";
    let (out, _, _) = sh(&mut world, &session, "app-server", "ps -e");
    assert!(out.contains("sshd"), "sshd runs from boot: {out}");

    // No key yet: the server refuses, as OpenSSH does.
    let (_, err, code) = sh(
        &mut world,
        &session,
        ws,
        "ssh ops@app-server.internal hostname",
    );
    assert_eq!(code, 255);
    assert!(err.contains("Permission denied (publickey)"), "{err}");

    let (out, _, code) = sh(
        &mut world,
        &session,
        ws,
        "ssh-keygen -t ed25519 -N '' -C ada@workstation",
    );
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("Your public key has been saved in /home/ada/.ssh/id_ed25519.pub"),
        "{out}"
    );
    let (public, _, _) = sh(&mut world, &session, ws, "cat ~/.ssh/id_ed25519.pub");
    assert!(
        public.starts_with("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI"),
        "{public}"
    );
    let (mode, _, _) = sh(&mut world, &session, ws, "ls -l ~/.ssh/id_ed25519");
    assert!(mode.starts_with("-rw-------"), "{mode}");
    let (_, _, code) = sh(&mut world, &session, ws, "ssh-keygen -t ed25519 -N ''");
    assert_eq!(code, 1, "an existing key is not overwritten");

    // The server's user lists the key.
    let (_, err, code) = sh(
        &mut world,
        &session,
        "app-server",
        &format!(
            "mkdir -p ~/.ssh && echo '{}' >> ~/.ssh/authorized_keys",
            public.trim()
        ),
    );
    assert_eq!(code, 0, "{err}");

    let (out, err, code) = sh(&mut world, &session, ws, "ssh ops@app-server.internal 'hostname; echo cw-731 > /tmp/over-ssh; cat /tmp/over-ssh; pwd'");
    assert_eq!((code, err.as_str()), (0, ""), "{out}");
    assert_eq!(out, "app-server\ncw-731\n/home/ops\n");
    let (out, _, _) = sh(&mut world, &session, "app-server", "cat /tmp/over-ssh");
    assert_eq!(out, "cw-731\n", "the command ran on the server");

    // The remote command's own failure is ssh's exit status and stderr.
    let (_, err, code) = sh(
        &mut world,
        &session,
        ws,
        "ssh ops@app-server.internal cat /tmp/none",
    );
    assert_eq!(code, 1);
    assert!(err.contains("No such file or directory"), "{err}");

    // Another login has no account there; a name that does not resolve; a host with no sshd.
    let (_, err, code) = sh(
        &mut world,
        &session,
        ws,
        "ssh root@app-server.internal hostname",
    );
    assert!(code == 255 && err.contains("Permission denied"), "{err}");
    let (_, err, code) = sh(
        &mut world,
        &session,
        ws,
        "ssh ops@nowhere.internal hostname",
    );
    assert!(
        code == 255 && err.contains("Could not resolve hostname nowhere.internal"),
        "{err}"
    );
    let (_, err, code) = sh(
        &mut world,
        &session,
        "app-server",
        "ssh ada@10.0.0.10 hostname",
    );
    assert!(code == 255 && err.contains("Connection refused"), "{err}");
    let (_, err, code) = sh(&mut world, &session, ws, "ssh ops@app-server.internal");
    assert!(
        code == 255 && err.contains("interactive login is not modelled"),
        "{err}"
    );

    // sshd stopped in the world: refused; started again: in.
    let (out, _, _) = sh(&mut world, &session, "app-server", "systemctl stop sshd");
    assert_eq!(out, "Stopped sshd\n");
    let (_, err, code) = sh(
        &mut world,
        &session,
        ws,
        "ssh ops@app-server.internal hostname",
    );
    assert!(code == 255 && err.contains("Connection refused"), "{err}");
    let (out, _, _) = sh(&mut world, &session, "app-server", "systemctl start sshd");
    assert_eq!(out, "Started sshd\n");
    let (out, _, code) = sh(
        &mut world,
        &session,
        ws,
        "ssh -o BatchMode=yes -i ~/.ssh/id_ed25519 ops@app-server.internal hostname",
    );
    assert_eq!((out.as_str(), code), ("app-server\n", 0));

    let kinds: Vec<String> = world.trajectory().iter().map(|e| e.kind.clone()).collect();
    assert!(kinds.iter().any(|k| k == "ssh.session") && kinds.iter().any(|k| k == "ssh.denied"));

    // A replay of the journal makes the same keys and the same sessions.
    let hash = world.state_hash().unwrap();
    let journal = world.interfaces().action_journal().clone();
    let (mut again, _) = self::world(&["coreutils", "openssh-server"]);
    again.interfaces_mut().replay(&initial, &journal).unwrap();
    assert_eq!(again.state_hash().unwrap(), hash);
}

#[test]
fn a_machine_without_the_package_has_no_sshd() {
    let (mut world, session) = world(&["coreutils"]);
    let (out, _, _) = sh(&mut world, &session, "app-server", "ps -e");
    assert!(!out.contains("sshd"), "{out}");
    let (_, err, code) = sh(&mut world, &session, "app-server", "systemctl start sshd");
    assert_ne!(code, 0);
    assert!(err.contains("openssh-server"), "{err}");
    let (_, err, code) = sh(
        &mut world,
        &session,
        "workstation",
        "ssh ops@app-server.internal hostname",
    );
    assert!(code == 255 && err.contains("Connection refused"), "{err}");
}
