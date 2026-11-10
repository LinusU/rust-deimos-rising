# Original-game oracle

Scripts that run the retail game under Drydock and trace it, so the remake
can be compared tick by tick. See `docs/faithful-port.md`.

- `dd.py`: Drydock session helper (config: QuickTime and license from the
  CrossOver bottle).
- `trace3.py MAXGT OUT`: from `filmstart.ddck`, log rand() draws (with
  caller), player ticks, ChangeState, RequestSpawn and Destroy calls.
- `ents.py TICK...`: dump the entities G_EG_Process handles on those ticks.
- `cmpog.py ORIG PORT`: first tick whose rand() draws or player state
  differ. The port side comes from `deimos --og-film 31306564`.
- `entraw.py FROM TO OUT` and `entdiff.py ORIG PORT`: raw entity dumps of
  the original and their comparison. The port side comes from
  `OG_ENTS=t1,t2 deimos --og-film ...` or `deimos --trace --input FILE
  --entities-at t1,t2`.
- `x86dis.py EXE ADDR N`, `xref.py EXE ADDR`: disassemble, find callers.
  Run them with `uv run` inside `scratch/deimos-rising-decomp` (capstone).
