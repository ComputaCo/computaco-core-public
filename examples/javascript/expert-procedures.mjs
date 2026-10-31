// Expert procedures projected onto the GUI from JavaScript: instantiate each
// procedure for a few seeds, stage it, have the engine perform it through pointer and
// keyboard, and record what the procedure's own checks said. The Python dataset
// generator's `--compare` reads this output and must agree on every state hash.
//
//   node examples/javascript/expert-procedures.mjs --output target/javascript-procedures
//
// See docs/expert-procedures.md.
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const args = process.argv.slice(2);
const option = (name, fallback) => {
  const at = args.indexOf(name);
  return at >= 0 ? args[at + 1] : fallback;
};
if (args.includes('--help')) {
  console.log('usage: expert-procedures.mjs [--world <world.json>] [--seeds <n>] [--pkg <dir>] --output <dir>');
  process.exit(0);
}
const worldPath = path.resolve(option('--world', path.join(root, 'worlds/agent-desktop/world.json')));
const seeds = Number(option('--seeds', '3'));
const output = option('--output');
if (!output) throw new Error('--output <dir> is required');
const require = createRequire(import.meta.url);
const cw = require(path.resolve(option('--pkg', path.join(root, 'pkg/node')), 'computerworld.js'));

const definition = JSON.parse(fs.readFileSync(worldPath, 'utf8'));
const folder = path.join(path.dirname(worldPath), 'procedures');
const episodes = [];
for (const file of fs.readdirSync(folder).filter((f) => f.endsWith('.json')).sort()) {
  const procedure = JSON.parse(fs.readFileSync(path.join(folder, file), 'utf8'));
  const users = Object.fromEntries(definition.computers.map((c) => [c.id, c.user]));
  for (let seed = 1; seed <= seeds; seed++) {
    const instance = cw.instantiate(procedure, seed);
    // The screens the procedure is performed on, as Procedure::prepare applies them.
    const prepared = structuredClone(definition);
    prepared.metadata = { ...prepared.metadata, desktop_themes: { ...prepared.metadata?.desktop_themes, ...instance.procedure.desktops } };
    const world = new cw.World(prepared, 7);
    world.stage(instance);
    // One session per role (or one for a one-person procedure), from the instance.
    const roles = instance.procedure.roles ?? { '': { machine: procedure.machine, user: users[procedure.machine] } };
    const sessions = {};
    for (const [name, role] of Object.entries(roles)) {
      sessions[name] = world.environment({
        actor: role.user,
        machines: [role.machine],
        actions: ['application.v1', 'keyboard.v1', 'pointer.v1', 'browser.v1', 'filesystem.v1', 'terminal.v1'],
        observations: ['semantic.v1'],
      }).id;
    }
    sessions[''] ??= Object.values(sessions)[0];
    const shown = world.project(sessions, instance);
    if (!shown.verification.passed) throw new Error(`${procedure.id} seed ${seed}: ${shown.failure}`);
    episodes.push({
      procedure: procedure.id,
      seed,
      state_hash: shown.state_hash,
      actions: shown.counts.actions,
      passed: shown.verification.passed,
    });
    console.log(`${procedure.id.padEnd(32)} seed ${seed}: ${shown.counts.actions} actions, ${shown.state_hash}`);
  }
}
fs.mkdirSync(output, { recursive: true });
fs.writeFileSync(path.join(output, 'procedures.json'), JSON.stringify({ engine: cw.engineVersion(), episodes }, null, 2) + '\n');
