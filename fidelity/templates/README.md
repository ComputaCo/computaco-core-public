# Home templates for reference captures

`cwfid capture-ref --home-template DIR` copies one of these into the application's fresh
sandbox home before launch, so first-run dialogs (settings migration, library table
setup, data-collection and update prompts) do not stand in front of the procedure.

- `kicad-home/` — KiCad 7.0 through 10.0 under `.config/kicad/<major.minor>/`: the global
  library tables (7.0 and 8.0 list the distribution's libraries directly, as those versions
  read them; 9.0 and 10.0 point at the distribution's tables with a `Table` entry), every
  one-time prompt marked seen, plugin update checks off. Copy and rename for another version.
