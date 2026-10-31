// This engine was split into code modules (the engine's design notes § Code modules,
// scripts/split-modules.mjs). Each import namespace `cw-code.<name>` holds placeholders
// for the functions of code module `name`: the first call to one asks the module host
// to link `modules/<name>.wasm` into this instance, which puts the module's functions
// into the table in place of the placeholders, and then makes the call.
var __cw_link_host = null;
function __cw_link_imports(imports) {
    return new Proxy(imports, {
        get(target, namespace) {
            if (typeof namespace !== 'string' || !namespace.startsWith('cw-code')) return target[namespace];
            const name = namespace.slice('cw-code'.length).replace(/^\./, '');
            return new Proxy({}, {
                get(_, slot) {
                    return function () {
                        if (!__cw_link_host) throw new Error(`${name} is a code module of this engine, and the engine has no module host to link it (setModuleHost)`);
                        const table = __cw_link_host.link(name, __cw_link_host.code?.[name], wasm);
                        return table.get(Number(slot)).apply(null, arguments);
                    };
                },
            });
        },
    });
}
