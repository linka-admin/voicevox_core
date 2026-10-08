"""Builds crates/voicevox_core/src/engine/talk/kanalizer_dataset.tsv.gz from VOICEVOX's kanalizer dataset.

The dataset (MIT, https://huggingface.co/datasets/VOICEVOX/kanalizer-dataset, v3) is what the kanalizer model
was trained on: 117,659 English words with their katakana reading. Words in it are read from the table; the
model only guesses the rest (it misread some of them, e.g. "minutes" as ミニューツ).

Usage: python3 build.py [dataset.jsonl]  (downloads the dataset when no path is given)
"""

import gzip
import json
import sys
import urllib.request
from pathlib import Path

URL = "https://huggingface.co/datasets/VOICEVOX/kanalizer-dataset/resolve/main/dataset/dataset.jsonl"
OUT = Path(__file__).resolve().parents[2] / "crates/voicevox_core/src/engine/talk/kanalizer_dataset.tsv.gz"


def main() -> None:
    if len(sys.argv) > 1:
        lines = Path(sys.argv[1]).read_text(encoding="utf-8").splitlines()
    else:
        with urllib.request.urlopen(URL) as response:
            lines = response.read().decode("utf-8").splitlines()
    rows = sorted({(row["word"], row["kata"][0]) for row in map(json.loads, lines) if row["kata"]})
    text = "".join(f"{word}\t{kata}\n" for word, kata in rows)
    # mtime 0: the same dataset always gives the same bytes.
    OUT.write_bytes(gzip.compress(text.encode("utf-8"), compresslevel=9, mtime=0))
    print(f"{OUT}: {len(rows)} words")


if __name__ == "__main__":
    main()
