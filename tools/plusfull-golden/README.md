# plusfull-golden

Golden data for making VOICEVOX CORE's text analysis match
[pyopenjtalk-plus](https://github.com/tsukumijima/pyopenjtalk-plus).

`generate.py` runs the pyopenjtalk-plus text frontend with its **default
options** and converts the result into VOICEVOX's AquesTalk-like kana, i.e. the
same notation `AudioQuery.kana` / `create_kana` uses. The resulting TSVs are the
expected output when CORE's own frontend is changed to behave like
pyopenjtalk-plus.

```
pyopenjtalk.run_frontend(text)        # NJD features (run_marine=False, other options default)
                                      # with the system dictionary rebuilt from the pinned CSVs
  -> pyopenjtalk.make_label(njd)      # full-context labels
  -> labels_to_kana(labels)           # mirrors full_context_label.rs + kana_parser.rs create_kana
```

## Layout

| Path | Content |
| --- | --- |
| `generate.py` | Generator (frontend -> labels -> kana). Also fetches the ITA corpus. |
| `pyproject.toml`, `uv.lock`, `.python-version` | Pinned environment (pyopenjtalk-plus built from a pinned git commit) |
| `build/dictionary/` | System dictionary exported by `--export-dict` (git-ignored, ~210 MB) |
| `corpus/conversation.txt` | 35 conversational sentences (20 general + 15 "ojousama" style) |
| `corpus/ita.txt` | ITA corpus, 424 sentences (EMOTION100 001-100, then RECITATION324 001-324) |
| `corpus/postprocessing.txt` | 243 phrases exercising the post-processing rules (`apply_postprocessing`), taken from pyopenjtalk-plus' `tests/test_postprocessing.py` |
| `golden/{conversation,ita,postprocessing}.tsv` | `text<TAB>kana`, one row per corpus line, no header |

## Pinned versions

| Package | Version | Why it matters |
| --- | --- | --- |
| `pyopenjtalk-plus[onnxruntime]` | tag `v0.4.1-post10` [`b312c6f`](https://github.com/tsukumijima/pyopenjtalk-plus/commit/b312c6f41dd82a9ac8ea94096fb1d2ff4222c067) (2026-10-21, version string `0.4.1.post10`) | The frontend itself (dictionary + NJD post-processing). Its `lib/open_jtalk` submodule is [tsukumijima/open_jtalk `1e4f83b`](https://github.com/tsukumijima/open_jtalk/commit/1e4f83b08992eaa3638416fd1f386f5f37a9d98c) (2026-10-17) |
| `sudachipy` | 0.7.0 | `use_sudachi_kanji_yomi=True` (default): homograph readings are re-decided with Sudachi |
| `sudachidict-core` | 20260723.1 | Same; the Sudachi dictionary content changes readings |
| `onnxruntime` | 1.30.0 | `predict_nani=True` (default): an ONNX model picks ナニ/ナン for standalone 「何」. Without onnxruntime pyopenjtalk-plus prints a warning and always uses ナニ |

VOICEVOX CORE builds tsukumijima/open_jtalk at `80f8c72` (2026-10-22), two commits after post10's `1e4f83b`
(a multibyte out-of-bounds read fix in text2mecab and a 「千円」 accent fix).

CORE's Rust port of the frontend follows pyopenjtalk-plus `v0.4.1-post10`. With the post10 goldens and
plus-dic-0.3.0, `matches_pyopenjtalk_plus_golden` matches 35/35 (conversation), 423/424 (ita) and 242/243
(postprocessing). The two differences are Sudachi readings (`use_sudachi_kanji_yomi`), which CORE does
not port: 「切望しつつ、主を待つ。」 (ヌシ, CORE reads the dictionary's アルジ) and 「この方」 (カタ, CORE reads ホウ).

`sudachipy` / `sudachidict-core` are hard dependencies of pyopenjtalk-plus, so
the Sudachi step is always active with the defaults. `onnxruntime` is only the
`onnxruntime` extra, which is why the extra is required here. `generate.py`
prints the installed versions (including the pyopenjtalk-plus commit and the
dictionary directory) at the end of every run so a mismatched environment is
easy to spot.

pyopenjtalk-plus is not taken from PyPI, so that the C sources are pinned to
the exact submodule commits of the tagged revision. uv builds it from source via
`[tool.uv.sources]` (`git` + `rev`); uv fetches git submodules recursively, so
`lib/open_jtalk` and `lib/hts_engine_API` are checked out at the commits
recorded in the pinned revision. Building requires CMake and a C/C++ compiler
(tested with Homebrew CMake and Xcode Command Line Tools); the first
`uv sync`/`uv run` takes about a minute. The commit is also hard-coded as
`PYOPENJTALK_PLUS_COMMIT` in `generate.py`; keep both in sync when bumping.

### System dictionary

The package bundles a **prebuilt** `sys.dic` that upstream commits only at
release time. The goldens are generated with a dictionary rebuilt from the CSVs
of the pinned commit (`--export-dict`), i.e. what upstream's
`uv run task build-dictionary` produces, plus the fork's own words in
`extra/*.csv`. For `v0.4.1-post10` (a release commit) the rebuild without the
extra words is byte-identical to the bundled `sys.dic`
(MD5 `29a816c28f591d54486224ba57dc0239`), and all 702 golden rows read the same
with either; the extra words don't occur in the corpora. (At the previous pin
`3310e63`, a master commit, the bundled `sys.dic` lagged behind its CSVs.)

The exported directory contains:

- Sources (from `pyopenjtalk/dictionary/` at the pinned commit):
  `naist-jdic.csv` (pyopenjtalk-plus' maintained naist-jdic),
  `unidic-csj.csv`, `heteronyms.csv`, `fillers.csv`, `rare_syllables.csv`,
  `symbols.csv`, `char.def`, `unk.def`, `matrix.def`, `left-id.def`,
  `right-id.def`, `pos-id.def`, `rewrite.def`, `feature.def`, `COPYING`;
  and `linka-place_names.csv` from `extra/` (see `tools/place-names`).
- Binaries built by `mecab-dict-index` of the same open_jtalk sources
  (`pyopenjtalk.build_mecab_dictionary()`): `sys.dic`, `matrix.bin`,
  `char.bin`, `unk.dic`. The build is deterministic (same `sys.dic` bytes on
  every run; since open_jtalk `1e4f83b` also across OSes, as the CSVs are read
  in a fixed order). With `v0.4.1-post10`: `sys.dic` MD5
  `aee39ba52a3ddd30deb624800b35c500`.

It is a drop-in replacement for VOICEVOX's `open_jtalk_dic_utf_8-1.11`
(OpenJTalk's naist-jdic 1.11): `left-id.def`, `right-id.def`, `pos-id.def`,
`rewrite.def` and `matrix.bin` are byte-identical (same POS/context-ID system
and connection costs), while `sys.dic` (different entries/costs/readings,
plus UniDic-CSJ, heteronym and rare-syllable entries), `char.bin` and
`unk.dic` (`char.def`/`unk.def` changes, e.g. KATAKANA unknown-word
candidates) differ. Load it in CORE by pointing `OpenJtalk` at the directory.

## Regenerating

With [uv](https://docs.astral.sh/uv/) (needs `git`, CMake and a C/C++ compiler):

```sh
cd tools/plusfull-golden
# 1. Build the system dictionary of the pinned commit (fetches
#    pyopenjtalk/dictionary with a sparse, blob-filtered git fetch).
uv run --locked generate.py --export-dict build/dictionary
# 2. Generate the goldens with it.
uv run --locked generate.py --dict-dir build/dictionary corpus/conversation.txt -o golden/conversation.tsv
uv run --locked generate.py --dict-dir build/dictionary corpus/ita.txt -o golden/ita.tsv
uv run --locked generate.py --dict-dir build/dictionary corpus/postprocessing.txt -o golden/postprocessing.tsv
```

`--export-dict` refuses to overwrite an existing directory and checks that the
installed pyopenjtalk-plus is the pinned commit. To use the dictionary
elsewhere (e.g. CORE's golden test), copy `build/dictionary/` (the `.csv` /
`matrix.def` sources can be dropped; only `sys.dic`, `matrix.bin`, `char.bin`,
`unk.dic` and the `*.def` files are needed at runtime).

Without uv, build pyopenjtalk-plus from a local clone instead:

```sh
git clone https://github.com/tsukumijima/pyopenjtalk-plus
git -C pyopenjtalk-plus checkout b312c6f41dd82a9ac8ea94096fb1d2ff4222c067
git -C pyopenjtalk-plus submodule update --init --recursive
cd tools/plusfull-golden
python3 -m venv .venv
.venv/bin/pip install '/path/to/pyopenjtalk-plus[onnxruntime]' \
    sudachipy==0.7.0 sudachidict-core==20260723.1 onnxruntime==1.30.0
.venv/bin/python -I generate.py --export-dict build/dictionary
.venv/bin/python -I generate.py --dict-dir build/dictionary corpus/conversation.txt -o golden/conversation.tsv
.venv/bin/python -I generate.py --dict-dir build/dictionary corpus/ita.txt -o golden/ita.tsv
.venv/bin/python -I generate.py --dict-dir build/dictionary corpus/postprocessing.txt -o golden/postprocessing.tsv
```

(A non-git install has no recorded commit, so the commit check of
`--export-dict` is skipped; make sure the checkout matches.)

`generate.py` reads the mora table from
`crates/voicevox_core/src/engine/mora_mappings.rs` (override with
`--mora-mappings`), so it must be run from within this repository.

### Options

- `--mode plusfull` (default): `run_frontend(text)` with default options.
- `--mode vanilla`: `run_frontend(text, use_vanilla=True)`; skips
  pyopenjtalk-plus' own NJD post-processing but still uses its dictionary.
- `--dict-dir DIR`: system dictionary to use (sets `OPEN_JTALK_DICT_DIR`).
  Without it the `sys.dic` bundled in the installed package is used.
- `--export-dict`: builds the dictionary of the pinned commit into `INPUT`
  and exits (see above).
  Useful to tell dictionary differences from post-processing differences.
- `--njd-json PATH`: also dumps, per sentence, the NJD features, the
  full-context labels and the conversion notes (below) for debugging mismatches.
- `--fetch-ita PATH`: re-downloads the ITA corpus at the pinned commit and
  writes the sentence list to `PATH` (e.g. `corpus/ita.txt`).

Input lines that are empty or start with `#` are skipped. A sentence whose
conversion fails is written with an empty kana column, reported on stderr, and
the exit status becomes 1.

## Conversion details and special cases

Kana conventions are exactly those of `create_kana`: `/` between accent phrases,
`、` for a pause, `'` after the accent nucleus mora (heiban = after the last
mora; OpenJTalk already emits `F2 == F1` for heiban), `_` before a devoiced
mora, `？` after an interrogative accent phrase (`F3 == 1`).

Two irregular label situations are handled and reported on stderr as `NOTE`:

1. **Accent position beyond the phrase** (`NOTE[clamp]`, `F2 > F1`): clamped to
   the last mora, which is what CORE does too (`accent_position.min(moras.len())`,
   the workaround for VOICEVOX/voicevox_engine#55). Occurred twice in `ita.tsv`
   with the 0.4.1.post9 release (「ジャデャクシュ。」, 「クレンペ教頭は…」, both
   unknown katakana split per syllable); does not occur with the pinned commit.
2. **Accent phrase split by a pause** (`NOTE[split]`, collected moras != `F1`):
   the nucleus is kept in the segment containing it (shifted by the first mora's
   `A2`); other segments become heiban. This only appeared with
   `run_marine=True` (marine can chain accent phrases across punctuation) and
   never occurs in the current goldens. Note that CORE would render each
   segment with `min(F2, segment length)` instead, so treat such rows with care.

## Verification

The first 20 lines of `corpus/conversation.txt` were cross-checked against an
independent earlier conversion (same pipeline, used to synthesize comparison
audio, pyopenjtalk-plus 0.4.1.post9): 20/20 identical in both `plusfull` and
`vanilla` modes. Re-running the steps above reproduces both TSVs
byte-for-byte.

Compared with the previous goldens (PyPI 0.4.1.post9 with its bundled
dictionary), `conversation.tsv` is unchanged and 99 of 424 rows of `ita.tsv`
changed: 88 through the newer C sources / post-processing (unknown katakana
words with rare syllables such as テュ, ミェ, ツァ, ヴァ are now kept as one word
with their surface kana instead of being split into one-mora phrases or
normalized to チュ/バ; `・` between katakana words no longer makes a pause;
fewer devoiced moras around ツ/チ, e.g. ドクリ_ツシヨオ -> ドクリツシヨオ;
1877 / 一二歩 numeral readings) and 11 through the rebuilt dictionary.

Bumping the pin from `3310e63` to `v0.4.1-post10` (`b312c6f`) changed 3 of 35
rows of `conversation.tsv`, 33 of 424 of `ita.tsv` and 30 of 243 of
`postprocessing.tsv`, about half through the C sources / post-processing and
half through the dictionary: a trailing 「ない」 phrase is now heiban
(シラナ'イ -> シラナイ'), 人 / 一 / 下 / 柄 get a devoiced first mora
(ヒトノ' -> _ヒトノ'), readings such as 高音 タカネ -> コオオン, 放った ホオッタ ->
ハナッタ, 出生前診断 シュッショオマエシ'ンダン, and number phrases (百兆円 one phrase).

## License notes

- `corpus/ita.txt` is derived from the
  [ITA corpus](https://github.com/mmorise/ita-corpus)
  (commit `994844bcac6925c900fd4ab45beab11b0b9ea7bd`), which is in the public
  domain (The Unlicense). Only the text column of
  `emotion_transcript_utf8.txt` / `recitation_transcript_utf8.txt` is kept.
  Reference: 小口純矢, 金井郁也, 小田恭央, 齊藤剛史, 森勢将雅: ITAコーパス:
  パブリックドメインの音素バランス文からなる日本語テキストコーパスの構築と基礎評価,
  情報処理学会研究報告, vol. 2021-MUS-131, no. 31, pp. 1-6, 2021.
- `corpus/conversation.txt`: short example sentences prepared for this
  comparison (not taken from a third-party corpus).
- `corpus/postprocessing.txt`: inputs of the post-processing tests of
  pyopenjtalk-plus (MIT), `tests/test_postprocessing.py` at `3310e63` (not
  refreshed for post10).
- The golden kana are output of pyopenjtalk-plus (MIT; see its repository for
  the licenses of the bundled dictionary and models) and are test data only.
