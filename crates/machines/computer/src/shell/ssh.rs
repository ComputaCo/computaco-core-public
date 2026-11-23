//! OpenSSH's client side: `ssh [-i key] [-p port] [-l user] [user@]host command` and
//! `ssh-keygen`. The connection is the world's (`ShellHost::ssh`): its DNS, routes and
//! the host's `sshd` on port 22, which the `openssh-server` package puts on a machine.
//! Keys are files as OpenSSH keeps them (`~/.ssh/id_ed25519`, `.pub`,
//! `authorized_keys`); their bytes come from the world's seeded randomness, so they are
//! a key's shape and identity, not cryptography.
use super::Fail;
use crate::{Computer, ShellHost, SshRequest, SSH_PORT};
use sha2::{Digest, Sha256};

/// The identities ssh offers when `-i` names none, in OpenSSH's order.
const DEFAULT_IDENTITIES: &[&str] = &["id_rsa", "id_ecdsa", "id_ed25519"];

fn home(c: &Computer) -> String {
    c.env
        .get("HOME")
        .cloned()
        .unwrap_or_else(|| format!("/home/{}", c.user))
        .trim_end_matches('/')
        .to_owned()
}

fn fail(text: impl Into<String>) -> Fail {
    Fail::new(text, 255).raw()
}

pub(super) fn ssh(
    c: &mut Computer,
    args: &[String],
    host: &mut dyn ShellHost,
) -> Result<String, Fail> {
    let mut port = SSH_PORT;
    let mut login: Option<String> = None;
    let mut keys: Vec<String> = vec![];
    let mut i = 0;
    while i < args.len() && args[i].starts_with('-') && args[i] != "--" {
        let a = args[i].as_str();
        let value = |i: usize| {
            args.get(i + 1)
                .cloned()
                .ok_or_else(|| fail(format!("option requires an argument -- {}\n", &a[1..])))
        };
        match a {
            "-p" => {
                port = value(i)?
                    .parse()
                    .map_err(|_| fail(format!("Bad port '{}'\n", args[i + 1])))?;
                i += 1;
            }
            "-l" => {
                login = Some(value(i)?);
                i += 1;
            }
            "-i" => {
                keys.push(value(i)?);
                i += 1;
            }
            // Options whose effect this world already has, or that change nothing in it:
            // host-key checking (the world's hosts are its own), batch mode (nothing is
            // asked), quiet, no tty, stdin from /dev/null.
            "-o" => i += 1,
            "-T" | "-n" | "-q" | "-4" | "-6" | "-x" | "-a" | "-C" => {}
            "-t" | "-tt" => {
                return Err(fail(
                    "ssh: an interactive terminal is not modelled; give the command to run: ssh host 'command'\n",
                ))
            }
            "-L" | "-R" | "-D" | "-W" | "-J" => {
                return Err(fail(format!(
                    "ssh: {a}: forwarding is not modelled inside a world; run the command on the host instead\n"
                )))
            }
            _ => return Err(fail(format!("ssh: unknown option -- {}\n", a.trim_start_matches('-')))),
        }
        i += 1;
    }
    if args.get(i).map(String::as_str) == Some("--") {
        i += 1;
    }
    let Some(destination) = args.get(i) else {
        return Err(fail(
            "usage: ssh [-i identity_file] [-l login_name] [-p port] destination [command]\n",
        ));
    };
    let (user, name) = match destination.split_once('@') {
        Some((u, h)) => (u.to_owned(), h.to_owned()),
        None => (login.unwrap_or_else(|| c.user.clone()), destination.clone()),
    };
    let command = args[i + 1..].join(" ");
    if command.trim().is_empty() {
        return Err(fail(
            "ssh: an interactive login is not modelled; give the command to run: ssh host 'command'\n",
        ));
    }
    let home = home(c);
    let files: Vec<String> = if keys.is_empty() {
        DEFAULT_IDENTITIES
            .iter()
            .map(|k| format!("{home}/.ssh/{k}"))
            .collect()
    } else {
        keys.iter().map(|k| c.resolve(k)).collect()
    };
    let mut identities = vec![];
    for file in &files {
        // A private key is offered by its public half beside it, as ssh does.
        match c.vfs.read_as(&format!("{file}.pub"), &c.user) {
            Ok(bytes) => identities.push(String::from_utf8_lossy(&bytes).trim().to_owned()),
            Err(_) if keys.is_empty() => {}
            Err(_) => {
                return Err(fail(format!(
                    "Warning: Identity file {file} not accessible: No such file or directory.\n"
                )))
            }
        }
    }
    let r = host
        .ssh(SshRequest {
            host: name,
            port,
            user,
            command,
            identities,
        })
        .map_err(|e| fail(format!("{e}\n")))?;
    if r.exit_code == 0 && r.stderr.is_empty() {
        Ok(r.stdout)
    } else {
        Err(Fail::nested(r))
    }
}

pub(super) fn keygen(
    c: &mut Computer,
    args: &[String],
    t: u64,
    host: &mut dyn ShellHost,
) -> Result<String, Fail> {
    let mut kind = "ed25519".to_owned();
    let mut file: Option<String> = None;
    let mut comment: Option<String> = None;
    let mut quiet = false;
    let mut i = 0;
    while i < args.len() {
        let value = |i: usize| {
            args.get(i + 1).cloned().ok_or_else(|| {
                Fail::new(
                    format!(
                        "ssh-keygen: option requires an argument -- {}\n",
                        &args[i][1..]
                    ),
                    1,
                )
                .raw()
            })
        };
        match args[i].as_str() {
            "-t" => {
                kind = value(i)?;
                i += 1;
            }
            "-f" => {
                file = Some(value(i)?);
                i += 1;
            }
            "-C" => {
                comment = Some(value(i)?);
                i += 1;
            }
            // A passphrase protects a key's file; this world's keys are not secrets of anyone's.
            "-N" => i += 1,
            "-q" => quiet = true,
            other => {
                return Err(Fail::new(format!("ssh-keygen: option {other} is not modelled; use -t ed25519 -f file -N '' -C comment\n"), 1).raw())
            }
        }
        i += 1;
    }
    if kind != "ed25519" {
        return Err(Fail::new(
            format!("ssh-keygen: only ed25519 keys are modelled, not {kind}\n"),
            1,
        )
        .raw());
    }
    let home = home(c);
    let path = c.resolve(&file.unwrap_or_else(|| format!("{home}/.ssh/id_ed25519")));
    if c.vfs.exists(&path) {
        return Err(Fail::new(format!("{path} already exists.\nOverwrite (y/n)? \n"), 1).raw());
    }
    let parent = path.rsplit_once('/').map(|(p, _)| p).unwrap_or("/");
    if !c.vfs.exists(parent) {
        c.vfs.mkdir_all_as(parent, &c.user, t)?;
        if parent == format!("{home}/.ssh") {
            c.vfs.chmod_as(parent, 0o700, &c.user)?;
        }
    }
    let mut public = [0u8; 32];
    let mut secret = [0u8; 32];
    for chunk in public.chunks_mut(8).chain(secret.chunks_mut(8)) {
        chunk.copy_from_slice(&host.entropy().to_le_bytes());
    }
    let comment = comment.unwrap_or_else(|| format!("{}@{}", c.user, c.id));
    let mut blob = wire_string(b"ssh-ed25519");
    blob.extend(wire_string(&public));
    let line = format!(
        "ssh-ed25519 {} {comment}",
        crate::datautils::base64_encode(&blob)
    );
    let mut private = b"openssh-key-v1\0".to_vec();
    private.extend(&blob);
    private.extend(wire_string(&secret));
    let armored = crate::datautils::base64_encode(&private)
        .as_bytes()
        .chunks(70)
        .map(|l| String::from_utf8_lossy(l).into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    c.vfs.write_as(
        &path,
        format!(
            "-----BEGIN OPENSSH PRIVATE KEY-----\n{armored}\n-----END OPENSSH PRIVATE KEY-----\n"
        )
        .as_bytes(),
        &c.user,
        t,
    )?;
    c.vfs.chmod_as(&path, 0o600, &c.user)?;
    c.vfs.write_as(
        &format!("{path}.pub"),
        format!("{line}\n").as_bytes(),
        &c.user,
        t,
    )?;
    c.vfs.chmod_as(&format!("{path}.pub"), 0o644, &c.user)?;
    if quiet {
        return Ok(String::new());
    }
    let fingerprint = crate::datautils::base64_encode(&Sha256::digest(&blob));
    Ok(format!(
        "Generating public/private ed25519 key pair.\nYour identification has been saved in {path}\nYour public key has been saved in {path}.pub\nThe key fingerprint is:\nSHA256:{} {comment}\n",
        fingerprint.trim_end_matches('=')
    ))
}

fn wire_string(bytes: &[u8]) -> Vec<u8> {
    let mut out = (bytes.len() as u32).to_be_bytes().to_vec();
    out.extend(bytes);
    out
}

/// Whether `authorized_keys` (its text) lists one of `identities`: a key matches by its
/// type and its base64 body, whatever options precede it or comment follows it.
pub fn authorizes(authorized_keys: &str, identities: &[String]) -> bool {
    let key = |line: &str| -> Option<(String, String)> {
        let words: Vec<&str> = line.split_whitespace().collect();
        let at = words
            .iter()
            .position(|w| w.starts_with("ssh-") || w.starts_with("ecdsa-"))?;
        Some((words[at].to_owned(), words.get(at + 1)?.to_string()))
    };
    let offered: Vec<(String, String)> = identities.iter().filter_map(|l| key(l)).collect();
    authorized_keys
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(key)
        .any(|k| offered.contains(&k))
}
