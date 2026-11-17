"""Dump pyopenjtalk-plus NJD features with and without Sudachi (`use_sudachi_kanji_yomi`).

Used to mine the context rules of `engine::talk::plus::kanji_yomi`.

    uv run --locked collect_kanji_yomi.py SENTENCES.tsv OUT.jsonl DICT_DIR [SEED] [PER_KANJI]

SENTENCES.tsv: one sentence per line (the last tab-separated column is used, e.g. Tatoeba's
jpn_sentences.tsv). For each kanji of `MULTI_READ_KANJI_LIST` (without 何), up to PER_KANJI
sentences containing it are sampled.
"""
import sys, json, random, os
os.environ["OPEN_JTALK_DICT_DIR"] = sys.argv[3]
import pyopenjtalk
from pyopenjtalk import _MULTI_READ_KANJI_SET_EXCLUDING_NANI as T
random.seed(int(sys.argv[4]) if len(sys.argv) > 4 else 0)
lines = [l.rstrip("\n").split("\t")[-1] for l in open(sys.argv[1], encoding="utf-8")]
per = int(sys.argv[5]) if len(sys.argv) > 5 else 300
buckets = {k: [] for k in T}
for s in lines:
    for k in set(s) & T:
        buckets[k].append(s)
chosen = set()
for k, ss in buckets.items():
    random.shuffle(ss)
    chosen.update(ss[:per])
chosen = sorted(chosen)
keys = ["string","pos","pos_group1","pos_group2","ctype","cform","orig","read","pron","acc","mora_size","chain_rule","chain_flag"]
with open(sys.argv[2], "w", encoding="utf-8") as out:
    for i, s in enumerate(chosen):
        try:
            on = pyopenjtalk.run_frontend(s)
            off = pyopenjtalk.run_frontend(s, use_sudachi_kanji_yomi=False)
        except Exception as e:
            continue
        out.write(json.dumps({"text": s, "on": [[f[k] for k in keys] for f in on], "off": [[f[k] for k in keys] for f in off]}, ensure_ascii=False) + "\n")
        if i % 1000 == 0: print(i, len(chosen), file=sys.stderr, flush=True)
