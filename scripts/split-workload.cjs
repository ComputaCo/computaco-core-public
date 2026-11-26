// The run scripts/split-modules.mjs profiles (wasm-split --instrument): what every world does
// whether or not it opens a page. A function of a code module marked `profiled` that this
// run calls stays in the engine, so such a world never links that module.
//
//   node scripts/split-workload.cjs <dir of an instrumented engine> <repository> <workspace> <profile out>
//
// <repository> is this one (its fonts, assets and worlds); <workspace> is the cargo workspace the engine was built in
// (scripts/build-wasm.sh's CW_WASM_WORKSPACE, or this repository), whose pkg/modules and target/metrics it reads.
const path = require('node:path');
const fs = require('node:fs');
const [dir, root, workspace, out] = process.argv.slice(2);
const engine = require(path.join(dir, 'computerworld.js'));
const { createModuleHost } = require(path.join(dir, 'modules.cjs'));
const fonts = path.join(root, 'crates/graphics/render/assets/fonts');
const files = {
    modules: (name) => path.join(workspace, 'pkg/modules', name),
    fonts: (name) => (name.startsWith('metrics/') ? path.join(workspace, 'target', name) : fs.existsSync(path.join(fonts, name)) ? path.join(fonts, name) : path.join(fonts, 'pack', name)),
    data: (name) => path.join(root, 'worlds', path.basename(name, '.json'), 'world.json'),
    assets: (name) => path.join(root, 'crates/graphics/render/assets', name),
};
engine.setModuleHost(createModuleHost({ read: (file) => fs.readFileSync(files[file.split('/')[0]](file.split('/').slice(1).join('/'))) }));
const platform = process.env.CW_WASM_PLATFORM;
const definition = JSON.parse(fs.readFileSync(path.join(root, 'worlds/agent-desktop/world.json'), 'utf8'));
if (platform) definition.metadata = { ...definition.metadata, desktop_themes: Object.fromEntries(definition.computers.map((c) => [c.id, platform])) };
const [width, height] = { ios: [390, 844], android: [412, 915] }[platform] ?? [1280, 720];
const world = new engine.World(definition, 1);
const machine = definition.computers[0];
const actor = world.environment({ actor: machine.user, machines: [machine.id], actions: ['pointer.v1', 'keyboard.v1', 'application.v1'], observations: ['semantic.v1'] });
actor.render(width, height).free();
actor.observe();
actor.scene(width, height);
// The desktop's own applications, not the browser or a web application.
const own = ['terminal', 'files', 'editor', 'calculator', 'calendar', 'photos'];
for (const action of [
    { family: 'pointer.v1', op: 'click', payload: { x: Math.floor(width / 2), y: Math.floor(height / 2) } },
    ...own.map((kind) => ({ family: 'application.v1', op: 'launch', payload: { kind } })),
    { family: 'keyboard.v1', op: 'type', payload: { text: 'ls -la' } },
    { family: 'keyboard.v1', op: 'key', payload: { key: 'Enter' } },
]) {
    actor.step([{ ...action, machine: machine.id }]);
    actor.render(width, height).free();
}
actor.observe();
const snapshot = world.snapshot();
world.fork(snapshot).stateHash();
world.restore(snapshot);
world.stateHash();
const wasm = engine.__wasm;
const room = 8 + 4 * 1_000_000;
const at = wasm.__wbindgen_malloc(room, 8);
fs.writeFileSync(out, Buffer.from(new Uint8Array(wasm.memory.buffer, at, wasm.__write_profile(at, room))));
