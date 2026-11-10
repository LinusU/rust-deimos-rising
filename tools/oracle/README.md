# Original-game oracle

Scripts that run the retail game under Drydock and trace it, so the remake
can be compared tick by tick. See `docs/faithful-port.md`.

- `dd.py`: Drydock session helper (config: QuickTime and license from the
  CrossOver bottle).
- `trace3.py MAXGT OUT`: from `filmstart.ddck`, log rand() draws (with
  caller), player ticks, ChangeState, RequestSpawn and Destroy calls.
- `ents.py TICK...`: dump the entities G_EG_Process handles on those ticks.
- `cmp3.py ORIG REMAKE`: first tick whose events differ. The remake side
  comes from `DEIMOS_RANDLOG=1 deimos --filmtest 31306564 --filmedbg`.
- `x86dis.py EXE ADDR N`, `xref.py EXE ADDR`: disassemble, find callers.
  Run them with `uv run` inside `scratch/deimos-rising-decomp` (capstone).
