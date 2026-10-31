//! A procedure (docs/expert-procedures.md) turned into the plan both sides perform:
//! the instance, its setup files, and the gestures in order with their phase labels.
use crate::util::{err, read_json, Result};
use computerworld::procedure::{Gesture, Instance, Procedure, SeedFile, Target as ProcTarget};
use computerworld::WorldDefinition;
use cw_fidelity::{ProcedureRef, Target};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct Plan {
    pub procedure_path: PathBuf,
    pub world_path: Option<PathBuf>,
    pub definition: Option<WorldDefinition>,
    pub instance: Instance,
    pub reference: ProcedureRef,
    pub machine: String,
    pub user: String,
    /// The user's home on the machine (`/home/ada`).
    pub home: String,
    pub gestures: Vec<(String, Gesture)>,
    pub setup: Vec<SeedFile>,
}

pub fn target_of(t: &ProcTarget, at: Option<[u32; 2]>) -> Target {
    Target {
        role: t.role.clone(),
        label: t.label.clone(),
        text: t.text.clone(),
        interaction: t.interaction.clone(),
        app: t.app.clone(),
        at,
        window: None,
        pick: None,
    }
}

/// The world a procedure belongs to: `<folder>/../world.json`, unless given.
fn world_for(procedure: &Path, explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return Some(p.to_path_buf());
    }
    let w = procedure.parent()?.parent()?.join("world.json");
    w.exists().then_some(w)
}

pub fn plan(
    procedure_path: &Path,
    world: Option<&Path>,
    seed: u64,
    overrides: &BTreeMap<String, String>,
) -> Result<Plan> {
    let text = std::fs::read_to_string(procedure_path)
        .map_err(|e| format!("{}: {e}", procedure_path.display()))?;
    let procedure =
        Procedure::from_json(&text).map_err(|e| format!("{}: {e}", procedure_path.display()))?;
    let instance = procedure
        .instantiate(seed, overrides)
        .map_err(|e| e.to_string())?;
    let world_path = world_for(procedure_path, world);
    let definition: Option<WorldDefinition> = match &world_path {
        Some(p) => Some(read_json(p)?),
        None => None,
    };
    let machine = instance.procedure.machine.clone();
    if machine.is_empty() {
        return err("team procedures (roles, no machine) are not supported by cwfid yet");
    }
    let (user, home) = match &definition {
        Some(d) => {
            let c = d
                .computers
                .iter()
                .find(|c| c.id == machine)
                .ok_or_else(|| format!("world has no machine {machine}"))?;
            let profile = d.profiles.iter().find(|p| p.id == c.profile);
            let home = profile
                .map(|p| {
                    p.home
                        .replace("{user}", &c.user)
                        .replace("{{user}}", &c.user)
                })
                .unwrap_or_else(|| format!("/home/{}", c.user));
            (c.user.clone(), home)
        }
        None => ("user".to_owned(), "/home/user".to_owned()),
    };
    let mut gestures = Vec::new();
    for phase in &instance.procedure.phases {
        for g in &phase.gui {
            gestures.push((phase.label.clone(), g.clone()));
        }
    }
    let reference = ProcedureRef {
        id: instance.procedure.id.clone(),
        seed,
        params: instance.values.clone(),
        world: world_path.as_ref().and_then(|p| {
            p.parent()
                .and_then(|d| d.file_name())
                .map(|n| n.to_string_lossy().into_owned())
        }),
        path: Some(procedure_path.to_string_lossy().into_owned()),
    };
    Ok(Plan {
        procedure_path: procedure_path.to_path_buf(),
        world_path,
        definition,
        setup: instance.procedure.setup.clone(),
        instance,
        reference,
        machine,
        user,
        home,
        gestures,
    })
}

/// Map a machine path (`/home/ada/Documents/x`) to a sandbox home (`<sandbox>/Documents/x`).
pub fn sandbox_path(sandbox: &Path, home: &str, path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix(home) {
        sandbox.join(rest.trim_start_matches('/'))
    } else {
        sandbox.join("_root").join(path.trim_start_matches('/'))
    }
}

/// Write a plan's setup files into a sandbox home.
pub fn stage_sandbox(plan: &Plan, sandbox: &Path) -> Result<Vec<String>> {
    let mut written = Vec::new();
    for f in &plan.setup {
        let p = sandbox_path(sandbox, &plan.home, &f.path);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let bytes = match &f.base64 {
            Some(b) => computerworld::decode_base64(b).map_err(|e| format!("{}: {e}", f.path))?,
            None => f.content.clone().into_bytes(),
        };
        std::fs::write(&p, bytes).map_err(|e| format!("{}: {e}", p.display()))?;
        written.push(p.to_string_lossy().into_owned());
    }
    Ok(written)
}
