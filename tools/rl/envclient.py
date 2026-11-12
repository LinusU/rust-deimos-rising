"""Talks to `deimos --rl-server`: many two-player games stepped in parallel."""

import os
import subprocess
import struct

import numpy as np

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
BIN = os.path.join(ROOT, "target", "release", "deimos")

# info columns (src/rl/env.rs fill_info): per player p at 8*p
I_DSCORE, I_LIVES, I_SHIELDS, I_STATE, I_INGAME, I_DEATHS, I_DAMAGE, I_SCORE = range(8)
I_LEVEL, I_LEVELS_DONE, I_LEVEL_DONE, I_END, I_TIME, I_TICKS, I_START_LEVEL, I_PROGRESS, I_COMPLETE = range(16, 25)
# self columns the trainer reads or sets
S_INGAME, S_LATCH, S_ROLE = 11, 48, 49


class EnvServer:
    def __init__(self, envs, threads=16, seed=1, repeat=3, max_ticks=15000, max_levels=2, levels=None):
        args = [BIN, "--rl-server", "--envs", str(envs), "--threads", str(threads), "--seed", str(seed),
                "--repeat", str(repeat), "--max-ticks", str(max_ticks), "--max-levels", str(max_levels)]
        if levels:
            args += ["--levels", ",".join(str(l) for l in levels)]
        self.p = subprocess.Popen(args, cwd=ROOT, stdin=subprocess.PIPE, stdout=subprocess.PIPE, bufsize=0)
        hdr = self._read(4 + 6 * 4)
        assert hdr[:4] == b"DRL2", hdr[:4]
        self.n, self.K, self.F, self.S, self.SELF_IDS, self.INFO = struct.unpack("<6I", hdr[4:])
        assert self.n == envs
        n = self.n
        self.sizes = [
            ("ent", np.float32, (n, 2, self.K, self.F)),
            ("ent_id", np.int32, (n, 2, self.K, 2)),
            ("self", np.float32, (n, 2, self.S)),
            ("self_id", np.int32, (n, 2, self.SELF_IDS * 2)),
            ("info", np.float32, (n, self.INFO)),
        ]
        self.block = sum(int(np.prod(s)) * 4 for _, _, s in self.sizes)
        self.pending = False

    def _read(self, size):
        buf = bytearray(size)
        view = memoryview(buf)
        got = 0
        while got < size:
            r = self.p.stdout.readinto(view[got:])
            if not r:
                raise EOFError("rl-server closed its output")
            got += r
        return bytes(buf)

    def read(self):
        raw = self._read(self.block)
        out = {}
        off = 0
        for name, dt, shape in self.sizes:
            cnt = int(np.prod(shape))
            out[name] = np.frombuffer(raw, dtype=dt, count=cnt, offset=off).reshape(shape)
            off += cnt * 4
        return out

    def send(self, actions, learners):
        """actions: uint8 [n, 2] action bytes; learners: bool [n, 2]."""
        a = np.ascontiguousarray(actions, dtype=np.uint8)
        l = np.ascontiguousarray(learners, dtype=np.uint8)
        assert a.shape == (self.n, 2) and l.shape == (self.n, 2)
        self.p.stdin.write(b"S" + a.tobytes() + l.tobytes())

    def step(self, actions, learners):
        self.send(actions, learners)
        return self.read()

    def close(self):
        try:
            self.p.stdin.write(b"Q")
            self.p.stdin.close()
        except Exception:
            pass
        self.p.wait(timeout=10)
