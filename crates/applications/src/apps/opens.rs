//! Which application opens a file, by its name. Here rather than in each application
//! so a build that serves the application from its module still opens its files.
fn ext(path: &str) -> String {
    path.rsplit('.').next().unwrap_or("").to_ascii_lowercase()
}
fn lower_name(name: &str) -> String {
    name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase()
}
/// Workbooks and CSV.
pub fn sheet(name: &str) -> bool {
    matches!(ext(name).as_str(), "xlsx" | "xlsm" | "ods" | "csv")
}
/// SQLite databases.
pub fn database(name: &str) -> bool {
    matches!(ext(name).as_str(), "db" | "sqlite" | "sqlite3" | "db3")
}
/// FreeCAD documents and the CAD exchange formats.
pub fn freecad(name: &str) -> bool {
    let lower = lower_name(name);
    lower.ends_with(".fcstd.json")
        || [".step", ".stp", ".stl", ".obj", ".dxf"]
            .iter()
            .any(|e| lower.ends_with(e))
}
/// KiCad projects, schematics and boards.
pub fn kicad(name: &str) -> bool {
    let lower = lower_name(name);
    [".kicad_pro", ".kicad_sch", ".kicad_pcb"]
        .iter()
        .any(|e| lower.ends_with(e))
}
