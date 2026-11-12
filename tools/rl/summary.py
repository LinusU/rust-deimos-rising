"""Prints a compact view of a run's log: uv run python summary.py runs/<run> [every]"""
import json
import sys

rows = {}
for l in open(sys.argv[1] + "/log.jsonl"):
    r = json.loads(l)
    rows[r["update"]] = r  # a resumed run may repeat updates; keep the last
rows = [rows[k] for k in sorted(rows)]
every = int(sys.argv[2]) if len(sys.argv) > 2 else 10
keys = ["duo/score", "duo/progress", "duo/levels", "support/charges_pm", "support/wchanges_pm", "support/misses_pm", "lead/volleys_pm", "lead/money_pm", "camper/score", "camper/progress", "camper/levels",
        "camper/partner_deaths", "idle/progress", "noisy/progress", "past/progress", "all/reward"]
print("upd    Msteps " + " ".join(f"{k.replace('/', ':')[:13]:>13}" for k in keys) + "  ent")
for i in range(0, len(rows), every):
    chunk = rows[i:i + every]
    def avg(k):
        v = [r[k] for r in chunk if k in r]
        return sum(v) / len(v) if v else float("nan")
    print(f"{chunk[-1]['update']:5d} {chunk[-1]['steps'] / 1e6:8.1f} " + " ".join(f"{avg(k):13.2f}" for k in keys)
          + "  " + str(chunk[-1]["ent"]))
