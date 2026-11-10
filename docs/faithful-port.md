# Faithful gameplay port

Goal: the remake's gameplay is exactly the original's. The acceptance test is
the four retail demo films: replayed in the remake, every tick must show the
same rand() stream, spawns, state changes and positions as the original
running in Drydock, ending on the score the film records (Demo 01: 25050).

## Why a port and not more fixes

`src/game/sim.rs` was written from the decompilation's names but not its
details. Tracing Demo 01 against the original showed differences at every
level, not just bugs:

- Coordinates: the original keeps entity x in game-area space and y in
  screen space, adding the scroll only to non-`air ` units inside
  `G_GameObject::MoveAndCheckPosition`. The remake keeps everything in map
  space with y up, so air units and delayed units drift.
- Math: `U_Math` uses 360-entry float sin/cos tables indexed by whole
  degrees, an atan table, and distances from a sqrt table indexed by the
  truncated squared distance. The remake uses f32 trig and exact sqrt, so
  collisions and headings differ at the margins.
- Order: `G_EG_Process` walks groups, then each group's children, as linked
  lists; spawns join the tail and run the same pass. The remake walks one
  flat vector.
- Heuristics with no counterpart in the original (screen-margin culls,
  smoothed orbit steps, ...).

## Approach

Port the original game logic function by function into `src/game/og/`,
keeping the original's structure, field offsets and float types:

- Unit, weapon and player definitions are parsed into byte images with the
  original's layout (`G_UnitDef_ParseUnitDef`/`ParseState` write each token
  at a fixed offset), so ported code reads the same offsets the
  disassembly reads. Verified by comparing against definitions dumped from
  the original's memory.
- `U_Math` tables are the original's own (dumped once from memory).
- The decompilation (`scratch/deimos-rising-decomp`) is the guide; rows
  marked `complete` in `config/reconstruction.csv` are byte-matched and can
  be trusted, everything else is checked against the disassembly.
- The film oracle (rand/state/spawn/position trace from Drydock) checks
  each step; the first divergent tick is the next thing to port or fix.

Once the port replays the demos, the renderer switches from `sim.rs` to it
and `sim.rs` goes away.

## Tick order (sub_020280)

1. G_Notice_Process, G_Debris_Process, G_Particle_Process, G_MotionBlur_Process
2. G_Player::Process for players 0 and 1 (state, film input, weapons, move)
3. G_ScoreBar_Process
4. game-over notice when no player is in game
5. G_Bgnd_Process: scroll by the vertical speed, level-complete check, map
   spawns at row `scrollOffset - 0x40`; level-end sequence
6. G_EG_Process; its result stops or resumes vertical scrolling
7. G_Game_Play increments the game time (the first tick runs at time 0)
