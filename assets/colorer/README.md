# Colorer's syntax schemes

`hrc/` is the `base/hrc` folder of [Colorer-schemes](https://github.com/colorer/Colorer-schemes)
1.3.0 (commit `ebc9b537fd62e8a070410124940f9dc31dfbe555`), unchanged, with its licence (LGPL 2.1,
`hrc/LICENSE.md`) and the program's `hrc/catalog.xml`. The package lays it out as `hrc\` beside the
program (ADR-0024 §2); the program reads the schemes as data and links none of their code.

To update the schemes: copy a newer `base/hrc` over `hrc/` (keeping `catalog.xml`), note its version
and commit here, and run the syntax tests.
