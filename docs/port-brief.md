# Brief for porting an original module into src/game/og/

Read first: docs/faithful-port.md (goal, conventions), src/game/og/mem.rs,
src/game/og/g.rs, src/game/og/umath.rs, src/game/og/bgnd.rs (a finished
module: follow its style exactly), src/game/og/unitdef.rs (G_UnitDef
layout; ported code reads defs from memory by offset).

Each stub in your file(s) carries the original's address, size and decomp
status. Replace every `todo!` with a faithful translation:

- Source of truth is the disassembly:
  `cd scratch/deimos-rising-decomp && uv run python ../../tools/oracle/x86dis.py original/DeimosRising.exe <hexaddr> <count>`
  (hex address without 0x; the function ends where the next starts).
  Callers: `uv run python ../../tools/oracle/xref.py original/DeimosRising.exe <hexaddr>`.
  Constants in .rdata/.data are readable at run time through `g.m` (the exe
  is mapped), so `fld dword [0x4d8654]` becomes `g.m.rf32(0x4d8654)`.
- The decompiled C++ in scratch/deimos-rising-decomp/src is a guide. Rows
  marked `complete` in config/reconstruction.csv are byte-matched and
  trustworthy; `matching`/`compiling` ones often have wrong details
  (conditions, nesting, offsets, missing calls). Check those against the
  disassembly branch by branch.
- Keep control flow, call order and every rand() draw exactly as the
  original. Globals stay at their addresses (`g.m.r32(0x4e4836)`); object
  fields stay at their offsets. `this` is a u32 address.
- x87: double precision; f32 at each `fstp/fst dword`; `umath::ftol` for
  the fist+fixup float-to-int idiom; compare exactly as the fnstsw test
  does (mind NaN and equality).
- Call other modules' functions by their stub names in src/game/og/*.rs
  with the stub signatures. If a stub's signature is wrong or unknown
  ("Signature unknown"), do NOT edit that other file: call it the way the
  disassembly does with your best signature, add `// SIG: ...` next to the
  call, and list it in your report.
- Rendering, sound output and file loading are not ported: drawing
  functions are skipped; U_Sound_Play goes through `g.host.sound_play`
  after rolling the pitch with `g.random_float` exactly where the original
  does (U_Sound_Play 0x44f5f0: RandomFloat(settings+0x10, settings+0x14),
  then RandomInt(settings+4, settings+4) which never draws); sprite sizes
  come from `g.host`. Library calls (strcmp, strcpy, memcpy...) use Mem
  helpers; add small helpers to your own file if needed.
- Use `g.m.alloca` for locals whose address is passed to a callee, and
  `g.m.pop_to` before returning.

Rules: only edit the files you were assigned (plus adding helper items
inside them). Never run `./target/release/deimos` without a CLI flag (it
opens the game window); you do not need to run it at all. Do not commit.
`cargo build --release` must succeed when you finish. Leave anything you
could not finish as `todo!` with a note, rather than guessing.

Report: which functions are done, which remain, every `// SIG:` call, any
global or offset whose meaning you inferred, and anything you suspect is
wrong.
