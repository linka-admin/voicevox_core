"""Generate golden VOICEVOX kana from the pyopenjtalk-plus text frontend.

Pipeline (per sentence):

    pyopenjtalk.run_frontend(text)   # NJD features, default options (run_marine=False)
      -> pyopenjtalk.make_label(njd) # HTS full-context labels
      -> labels_to_kana(labels)      # VOICEVOX AquesTalk-like kana

The kana conversion mirrors VOICEVOX CORE:

- crates/voicevox_core/src/engine/talk/full_context_label.rs
  (labels -> utterance / breath groups / accent phrases / moras)
- crates/voicevox_core/src/engine/talk/kana_parser.rs `create_kana`
  (accent phrases -> kana string)

Kana conventions (same as `create_kana`):

- `/`  accent phrase boundary without pause
- `、` accent phrase boundary with pause (one `pau` label)
- `'`  placed right after the accent nucleus mora; heiban is written as an accent on
       the last mora (OpenJTalk already emits F2 == F1 for heiban), as CORE does
- `_`  prefix of a devoiced mora (capital vowel A/I/U/E/O in the label)
- `？` suffix of an interrogative accent phrase (label field F3 == 1)

Usage:

    uv run generate.py corpus/conversation.txt -o golden/conversation.tsv
    uv run generate.py --fetch-ita corpus/ita.txt
    uv run generate.py --export-dict build/dictionary
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path

HERE = Path(__file__).resolve().parent
DEFAULT_MORA_MAPPINGS = (
    HERE.parent.parent / "crates/voicevox_core/src/engine/mora_mappings.rs"
)

# Must match `[tool.uv.sources]` in pyproject.toml. Used by --export-dict to
# fetch the dictionary sources (CSV/def files) of the same commit, because the
# built package only ships the compiled dictionary.
PYOPENJTALK_PLUS_REPO = "https://github.com/tsukumijima/pyopenjtalk-plus"
PYOPENJTALK_PLUS_COMMIT = "3310e63654473ccee5d38c18a6c93129b641bf85"
PYOPENJTALK_PLUS_DICT_PATH = "pyopenjtalk/dictionary"

ITA_REPO_COMMIT = "994844bcac6925c900fd4ab45beab11b0b9ea7bd"
ITA_FILES = ("emotion_transcript_utf8.txt", "recitation_transcript_utf8.txt")
ITA_RAW_URL = "https://raw.githubusercontent.com/mmorise/ita-corpus/{commit}/{name}"

PAUSE_DELIMITER = "、"
NOPAUSE_DELIMITER = "/"
ACCENT_SYMBOL = "'"
UNVOICE_SYMBOL = "_"
WIDE_INTERROGATION_MARK = "？"

VOICED_VOWELS = frozenset("aiueo")
UNVOICED_VOWELS = frozenset("AIUEO")
# Phonemes that end a mora (the mora's "vowel" in CORE terms).
MORA_FINAL = VOICED_VOWELS | UNVOICED_VOWELS | {"N", "cl"}


# --------------------------------------------------------------------------
# Mora table
# --------------------------------------------------------------------------


def load_mora_table(path: Path) -> dict[tuple[str, str], str]:
    """Read `(consonant, vowel) -> kana` from CORE's `mora_mappings.rs`.

    The table is read from the Rust source (single source of truth) rather
    than duplicated here.
    """
    src = path.read_text(encoding="utf-8")
    table = {
        (c, v): k
        for c, v, k in re.findall(
            r'#\[mora_mappings\("([^"]*)", "([^"]*)"\)\]\s*(\S+?),', src
        )
    }
    if not table:
        raise RuntimeError(f"no mora mappings found in {path}")
    return table


# --------------------------------------------------------------------------
# Label parsing
# --------------------------------------------------------------------------

_P3 = re.compile(r"-(.+?)\+")
_A = re.compile(r"/A:([\-\dxX]+)\+(\d+|xx)\+(\d+|xx)/")
_F = re.compile(r"/F:(\d+)_(\d+)#(\d+)_")
_F_ALL = re.compile(r"/F:[^/]*")
_I_ALL = re.compile(r"/I:[^/]*")


@dataclass
class Phoneme:
    phoneme: str
    # The following are only set for non-silence phonemes.
    a2: int = 0  # mora position in the accent phrase (1-origin, forward)
    f1: int = 0  # mora count of the accent phrase
    f2: int = 0  # accent nucleus position (== f1 for heiban)
    f3: int = 0  # 1 = interrogative
    ap_key: tuple[str, str] = ("", "")  # identifies the accent phrase


def parse_label(label: str) -> Phoneme:
    p3 = _P3.search(label).group(1)
    if p3 in ("sil", "pau"):
        return Phoneme(p3)
    a = _A.search(label)
    f = _F.search(label)
    return Phoneme(
        phoneme=p3,
        a2=int(a.group(2)),
        f1=int(f.group(1)),
        f2=int(f.group(2)),
        f3=int(f.group(3)),
        # /I: (breath group) + /F: (accent phrase incl. its position inside the
        # breath group) together are unique per accent phrase in an utterance.
        ap_key=(_I_ALL.search(label).group(0), _F_ALL.search(label).group(0)),
    )


# --------------------------------------------------------------------------
# Labels -> kana
# --------------------------------------------------------------------------


@dataclass
class AccentPhrase:
    f1: int
    f2: int
    f3: int
    first_a2: int  # A2 of the first mora collected into this segment
    moras: list[str] = field(default_factory=list)


@dataclass
class Notes:
    """Non-standard situations encountered while converting one sentence."""

    split_phrases: list[str] = field(default_factory=list)
    clamped_accents: list[str] = field(default_factory=list)


def labels_to_kana(
    labels: list[str], table: dict[tuple[str, str], str], notes: Notes
) -> str:
    breath_groups: list[list[AccentPhrase]] = []
    cur_bg: list[AccentPhrase] | None = None
    cur_ap: AccentPhrase | None = None
    prev_key: tuple[str, str] | None = None
    consonant = ""

    for ph in map(parse_label, labels):
        if ph.phoneme == "sil":
            continue
        if ph.phoneme == "pau":
            cur_bg = cur_ap = prev_key = None
            consonant = ""
            continue
        if cur_bg is None:
            cur_bg = []
            breath_groups.append(cur_bg)
        if ph.phoneme not in MORA_FINAL:
            consonant = ph.phoneme
            continue
        if cur_ap is None or ph.ap_key != prev_key:
            cur_ap = AccentPhrase(f1=ph.f1, f2=ph.f2, f3=ph.f3, first_a2=ph.a2)
            cur_bg.append(cur_ap)
        vowel = ph.phoneme
        unvoiced = vowel in UNVOICED_VOWELS
        key_vowel = vowel.lower() if unvoiced else vowel
        kana = table[(consonant, key_vowel)]
        cur_ap.moras.append((UNVOICE_SYMBOL if unvoiced else "") + kana)
        consonant = ""
        prev_key = ph.ap_key

    return PAUSE_DELIMITER.join(
        NOPAUSE_DELIMITER.join(_render_phrase(ap, notes) for ap in bg)
        for bg in breath_groups
    )


def _render_phrase(ap: AccentPhrase, notes: Notes) -> str:
    n = len(ap.moras)
    # OpenJTalk emits heiban as F2 == F1 (accent on the last mora), which is
    # what CORE expects; F2 == 0 is mapped to the same defensively.
    acc = ap.f2 if ap.f2 != 0 else ap.f1

    # Special case 1: F2 > F1 (accent position beyond the phrase's mora count).
    # OpenJTalk can emit this for some NJD chains; clamp to the last mora,
    # same as CORE's `accent_position.min(moras.len())` workaround for
    # VOICEVOX/voicevox_engine#55.
    if acc > ap.f1:
        notes.clamped_accents.append(f"{''.join(ap.moras)} f1={ap.f1} f2={ap.f2}")
        acc = ap.f1

    # Special case 2: one label accent phrase is split by a `pau` (the moras
    # collected here are only part of the phrase described by F1/F2). The
    # nucleus is kept in the segment that contains it (shifted by the first
    # mora's A2); segments that don't contain it are rendered heiban.
    # This only happens with run_marine=True (marine can chain across
    # punctuation); it has not been observed with the default frontend.
    # NOTE: CORE itself would instead render each segment with
    # min(F2, segment length), so a golden row hitting this case is suspect.
    if n != ap.f1:
        notes.split_phrases.append(
            f"{''.join(ap.moras)} f1={ap.f1} f2={ap.f2} a2={ap.first_a2}"
        )
        acc -= ap.first_a2 - 1
        if not 1 <= acc <= n:
            acc = n

    acc = min(acc, n)
    s = "".join(ap.moras[:acc]) + ACCENT_SYMBOL + "".join(ap.moras[acc:])
    if ap.f3 == 1:
        s += WIDE_INTERROGATION_MARK
    return s


# --------------------------------------------------------------------------
# Frontend
# --------------------------------------------------------------------------


def run_frontend(text: str, mode: str):
    import pyopenjtalk

    # Default options of pyopenjtalk-plus (run_marine=False,
    # use_sudachi_kanji_yomi=True, predict_nani=True). `vanilla` skips the
    # pyopenjtalk-plus specific NJD post-processing.
    return pyopenjtalk.run_frontend(text, use_vanilla=(mode == "vanilla"))


def environment_info() -> dict[str, str]:
    from importlib.metadata import PackageNotFoundError, version

    info = {"python": sys.version.split()[0]}
    for pkg in ("pyopenjtalk-plus", "sudachipy", "sudachidict-core", "onnxruntime"):
        try:
            info[pkg] = version(pkg)
        except PackageNotFoundError:
            info[pkg] = "(not installed)"
    if commit := installed_commit():
        info["pyopenjtalk-plus"] += f"@{commit[:7]}"
    info["dict"] = os.environ.get("OPEN_JTALK_DICT_DIR", "(bundled)")
    return info


def installed_commit() -> str | None:
    from importlib.metadata import PackageNotFoundError, distribution

    try:
        direct_url = distribution("pyopenjtalk-plus").read_text("direct_url.json")
    except PackageNotFoundError:
        return None
    return json.loads(direct_url or "{}").get("vcs_info", {}).get("commit_id")


# --------------------------------------------------------------------------
# Dictionary
# --------------------------------------------------------------------------


def export_dictionary(dest: Path) -> None:
    """Build pyopenjtalk-plus' system dictionary from source into `dest`.

    The dictionary directory of the pinned commit (CSV/def sources plus the
    prebuilt binaries committed upstream) is fetched with git, then
    `sys.dic` / `matrix.bin` / `char.bin` / `unk.dic` are rebuilt from the CSVs
    with `pyopenjtalk.build_mecab_dictionary()` (mecab-dict-index from the
    same open_jtalk sources). The committed `sys.dic` is only refreshed by
    upstream at release time, so on master it lags behind the CSVs.
    """
    commit = installed_commit()
    if commit is not None and commit != PYOPENJTALK_PLUS_COMMIT:
        raise RuntimeError(
            f"installed pyopenjtalk-plus is {commit}, expected {PYOPENJTALK_PLUS_COMMIT}"
        )
    if dest.exists():
        raise FileExistsError(f"{dest} already exists")

    with tempfile.TemporaryDirectory() as tmp:
        repo = Path(tmp) / "repo"

        def git(*args: str) -> None:
            subprocess.run(["git", "-C", str(repo), *args], check=True)

        repo.mkdir()
        git("init", "-q")
        git("remote", "add", "origin", PYOPENJTALK_PLUS_REPO)
        git("config", "remote.origin.promisor", "true")
        git("config", "remote.origin.partialclonefilter", "blob:none")
        git("fetch", "-q", "--depth=1", "--filter=blob:none", "origin", PYOPENJTALK_PLUS_COMMIT)
        git("sparse-checkout", "set", "--no-cone", PYOPENJTALK_PLUS_DICT_PATH)
        git("checkout", "-q", PYOPENJTALK_PLUS_COMMIT)
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(repo / PYOPENJTALK_PLUS_DICT_PATH, dest)

    # The fork's own words (tools/plusfull-golden/extra/*.csv, e.g. place_names.csv from tools/place-names).
    for extra in sorted((Path(__file__).parent / "extra").glob("*.csv")):
        shutil.copy(extra, dest / f"linka-{extra.name}")
        print(f"added {extra.name}", file=sys.stderr)

    import pyopenjtalk

    # Removes *.dic / *.bin in `dest`, then runs mecab-dict-index on it.
    pyopenjtalk.build_mecab_dictionary(str(dest))
    print(f"built dictionary in {dest}", file=sys.stderr)


# --------------------------------------------------------------------------
# ITA corpus
# --------------------------------------------------------------------------


def fetch_ita(dest: Path) -> None:
    """Download the ITA corpus (public domain) at a pinned commit.

    Lines are `ID:text,yomi`; only `text` is kept, in file order
    (EMOTION100_001..100, then RECITATION324_001..324).
    """
    sentences: list[str] = []
    for name in ITA_FILES:
        url = ITA_RAW_URL.format(commit=ITA_REPO_COMMIT, name=name)
        with urllib.request.urlopen(url) as r:
            body = r.read().decode("utf-8")
        for line in body.splitlines():
            if not line.strip():
                continue
            _id, rest = line.split(":", 1)
            text, _yomi = rest.rsplit(",", 1)
            sentences.append(text)
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text("\n".join(sentences) + "\n", encoding="utf-8")
    print(f"wrote {len(sentences)} sentences to {dest}", file=sys.stderr)


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------


def read_sentences(path: Path) -> list[str]:
    return [
        line.strip()
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.startswith("#")
    ]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("input", type=Path, help="sentence list (one per line, '#' = comment)")
    ap.add_argument("-o", "--output", type=Path, help="output TSV (default: stdout)")
    ap.add_argument("--mode", choices=("plusfull", "vanilla"), default="plusfull",
                    help="plusfull: run_frontend() defaults; vanilla: use_vanilla=True")
    ap.add_argument("--njd-json", type=Path,
                    help="also dump NJD features and labels per sentence to this JSON file")
    ap.add_argument("--mora-mappings", type=Path, default=DEFAULT_MORA_MAPPINGS,
                    help="CORE's mora_mappings.rs (default: %(default)s)")
    ap.add_argument("--fetch-ita", action="store_true",
                    help="download the ITA corpus into INPUT and exit")
    ap.add_argument("--export-dict", action="store_true",
                    help="build the system dictionary of the pinned commit into INPUT and exit")
    ap.add_argument("--dict-dir", type=Path,
                    help="system dictionary to use (sets OPEN_JTALK_DICT_DIR; "
                         "default: the one bundled in the installed package)")
    args = ap.parse_args()

    if args.fetch_ita:
        fetch_ita(args.input)
        return 0
    if args.export_dict:
        export_dictionary(args.input)
        return 0
    if args.dict_dir is not None:
        if not (args.dict_dir / "sys.dic").is_file():
            ap.error(f"{args.dict_dir}/sys.dic not found (run --export-dict first)")
        # Read by pyopenjtalk at import time.
        os.environ["OPEN_JTALK_DICT_DIR"] = str(args.dict_dir.resolve())

    table = load_mora_table(args.mora_mappings)
    sentences = read_sentences(args.input)

    import pyopenjtalk

    rows: list[str] = []
    dump: list[dict] = []
    failures = 0
    t0 = time.perf_counter()
    for text in sentences:
        notes = Notes()
        njd = labels = None
        try:
            njd = run_frontend(text, args.mode)
            labels = pyopenjtalk.make_label(njd)
            kana = labels_to_kana(labels, table, notes)
        except Exception as e:  # keep going; report at the end
            failures += 1
            kana = ""
            print(f"FAILED: {text}: {type(e).__name__}: {e}", file=sys.stderr)
        for kind, items in (("split", notes.split_phrases), ("clamp", notes.clamped_accents)):
            for item in items:
                print(f"NOTE[{kind}]: {text}: {item}", file=sys.stderr)
        rows.append(f"{text}\t{kana}")
        if args.njd_json is not None:
            dump.append({
                "text": text,
                "kana": kana,
                "notes": {"split": notes.split_phrases, "clamp": notes.clamped_accents},
                "njd": njd,
                "labels": labels,
            })
    elapsed = time.perf_counter() - t0

    out = "\n".join(rows) + "\n"
    if args.output is None:
        sys.stdout.write(out)
    else:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(out, encoding="utf-8")
    if args.njd_json is not None:
        args.njd_json.parent.mkdir(parents=True, exist_ok=True)
        args.njd_json.write_text(
            json.dumps({"mode": args.mode, "env": environment_info(), "sentences": dump},
                       ensure_ascii=False, indent=1) + "\n",
            encoding="utf-8",
        )

    n = max(len(sentences), 1)
    print(
        f"{len(sentences)} sentences, {failures} failed, "
        f"{elapsed:.2f}s total, {elapsed / n * 1000:.1f} ms/sentence "
        f"(mode={args.mode}, {environment_info()})",
        file=sys.stderr,
    )
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
