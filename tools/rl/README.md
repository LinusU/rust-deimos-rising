# Co-pilot bot

A small neural network that flies player 2 next to a person in a
two-player game. It is trained by reinforcement learning (PPO) on the
faithful port, headless and many games at a time, and runs in the game on
the CPU (`src/rl/policy.rs`).

## Playing with it

Put a policy file at `deimos-copilot.bin` in the directory you start the
game from (or pass `--copilot <file>`), start the game, and press **B** on
the title screen: a two-player game starts with you on player 1's keys and
the bot flying player 2. With `--copilot`, *Two Players* on the menu means
you and the bot too. `--copilot-temp T` changes how it samples its actions:
1 plays as trained, below 1 is steadier, above 1 sloppier (an easy way to
make a snapshot a little weaker).

## What it sees (no cheating)

Only what is on the screen and the score bars (`src/rl/obs.rs`):

- every object drawn this frame (up to the 64 nearest the ship): sprite id
  (embedded, so it learns what a sprite means by seeing what it does),
  position relative to its ship and on screen, motion since the last look,
  facing (animation frame), size, layer, translucency and hit flash;
- its own and its partner's ship position, state, shields, power (the
  charge bar), lives and air weapon slots as the score bars show them
  (every air weapon shares one score bar sprite and differs by frame, so
  a slot is embedded by both), the level and how far it has scrolled;
- its own previous action, whether a charge is running, and its role.

Never hidden state: no hit points, timers, spawn tables or future rows of
the map.

To play like a person rather than a machine it decides ten times a second
(the action is held for 3 ticks) and acts on what the screen showed 100 ms
before (`--obs-delay 1`), so its reactions take 100 to 200 ms. Changing
direction and cycling weapons cost a little reward, against jitter.

Its actions are a direction, the ground weapon, weapon select, and the
air weapon as one of *off*, *fire* or *charge*. Holding air fire charges
the weapon (the power bar fills in about 2 s; held much longer, the ship
overloads and blows up), and letting go releases the charge: the blue
Rear Gun's charge lays bullets that circle in place for a while. A
decision every 100 ms would almost never hold the button that long by
chance, so *charge* does it as one order, as a person watching the bar
would: it holds air fire until the bar is full, then lets go
(`Latch` in `src/rl/obs.rs`).

## How it is trained (`train.py`)

One network plays two roles, told apart by an input: **support** (the
co-pilot; the game always runs it in this role) and **lead**, a stand-in
for a person who goes for ground targets and coins and takes more risks.

PPO with a cooperative reward: a ship's own score, lost lives and shield
damage, plus half of its partner's, plus a bonus for each level both get
through. On top of that:

- support: pays for its partner's damage and deaths (`--protect`, so they
  count in full) and for bomb volleys that score nothing within 1.5 s
  (`--bomb-miss`);
- lead: a bonus for bomb volleys that score (`--bomb-hit`) and for money
  (`--coin`), and a smaller death penalty (`--lead-death`);
- both: a bonus per full charge that fades out over the first updates
  (`--charge-bonus`, `--charge-anneal`), to get it trying charges at all.

Each training game draws a partner setup per episode, so the support
learns to cooperate with partners of every kind rather than only with a
copy of itself (the idea of Fictitious Co-Play, Strouse et al. 2021):

| setup  | partner                                                    |
|--------|------------------------------------------------------------|
| duo    | the current policy as lead (both ships learn)              |
| past   | a frozen earlier snapshot as lead (weaker partners)        |
| camper | flies to a bottom spot and stays there, sometimes firing   |
| idle   | never touches the controls                                 |
| noisy  | wanders like a distracted beginner                         |

Games start on random levels (with the full three lives, unlike the
original's one-life level select) and on a random air weapon, so every
level and weapon is seen.

A run can start from an older run's weights with `--init` (new inputs and
actions start near silent); `copilot2` started from `copilot1`.

The network (`model.py`) is a 2-layer entity transformer, width 96, about
300k parameters.

## Commands

```bash
cargo build --release
cd tools/rl
uv run python train.py --run copilot1            # trains; checkpoints in runs/copilot1
uv run python summary.py runs/copilot1 20        # learning curves
uv run python eval.py runs/copilot1 --every 4    # whole games from level 1, per snapshot
uv run python parity.py runs/copilot1/latest.pt  # Rust forward pass vs PyTorch
uv run python behaviour.py runs/copilot2/ckpt_00500.pt --level 5  # twitchiness, charges, weapon changes, bombs
./evalwatch.sh runs/copilot1 16                  # evaluates new snapshots as they appear
```

Training resumes with `--resume runs/<run>/latest.pt` (weights, optimiser
and partner pool). `runs/<run>/notes.txt` records changes made mid-run.

To watch the bot without the window: `deimos --og-live-test 4000 --players 2
--copilot <policy.bin>` prints both ships' state every tick.

`deimos --rl-bench` measures the environment's speed; `deimos --rl-server`
is what `envclient.py` talks to.

## Picking a snapshot

`train.py` keeps `ckpt_NNNNN.pt` and the matching `policy_NNNNN.bin` every
50 updates. `eval.py` plays each through whole games from level 1 in two
setups and appends to `runs/<run>/eval.jsonl`:

- **camper**: player 1 hides in a bottom corner and never fires. A good
  co-pilot for this game should not get through the game like this.
- **duo**: the bot flies both ships (lead and support), a stand-in for a
  partner who plays.

There are 12 levels. Pick the snapshot that does well in *duo* while
staying well short of winning in *camper*, copy its `policy_NNNNN.bin` to `deimos-copilot.bin`,
and fine-tune difficulty with `--copilot-temp`.
