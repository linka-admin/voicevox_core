"""Builds tools/plusfull-golden/extra/place_names.csv: Japan's municipalities as dictionary words.

Source: Japan Post's postal code data (utf_ken_all.csv, https://www.post.japanpost.jp/zipcode/), which says
"著作権を主張しません。自由に配布していただいて結構です". Its readings settle what the dictionary got wrong when it
built a name from its parts (34% of 1,892: 川越町 カワゴエチョウ not マチ, 鳥羽市 トバシ not トッパシ,
武雄市 タケオシ not タケユウイチ, 三田市 サンダシ not ミタシ).

Each 郡 and each municipality is a word ("三重郡", "川越町"), so "三重郡川越町" is read as two. A name that
municipalities share with different readings (朝日町, 海部郡) is a word only together with the rest of its name
("三重郡朝日町"), which tells it apart; so are the designated cities' wards ("札幌市南区").
The accent falls on the last mora before the suffix (カワゴエ'チョウ), as the dictionary did for these names.

A name the dictionary already has (71, 25 with another reading, e.g. 日の出町 ヒノデチョウ) costs a little less
than its entry there, so the municipality's reading wins.

Usage: python3 build.py utf_ken_all.csv [pyopenjtalk-plus dictionary CSVs, for the names it already has]
"""

import csv
import re
import sys
from collections import defaultdict
from pathlib import Path

OUT = Path(__file__).resolve().parents[1] / "plusfull-golden/extra/place_names.csv"
SUFFIXES = {"郡": "グン", "市": "シ", "区": "ク", "町": None, "村": None}  # 町・村: チョウ/マチ, ムラ/ソン by the data
# Proper noun, region, general (as naist-jdic's place names), costing less than the name's parts.
CONTEXT_ID, COST = 1353, 1000
SMALL = set("ァィゥェォャュョヮ")


def moras(kana: str) -> int:
    return sum(1 for c in kana if c not in SMALL)


def pron(kana: str) -> str:
    """The pronunciation as the dictionary writes it, a long vowel as ー: チョウ → チョー, チュウ → チュー,
    メイ → メー, オオ → オー."""
    kana = re.sub(r"(?<=[オコゴソゾトドノホボポモヨロョ])[ウオ]", "ー", kana)
    kana = re.sub(r"(?<=[ウクグスズツヌフブプムユルュ])ウ", "ー", kana)
    return re.sub(r"(?<=[エケゲセゼテデネヘベペメレ])[イエ]", "ー", kana)


def split_gun(city: str, kana: str) -> list[tuple[str, str]]:
    """"三重郡川越町" / ミエグンカワゴエチョウ → [("三重郡", "ミエグン"), ("川越町", "カワゴエチョウ")]."""
    if "郡" not in city or "グン" not in kana:
        return [(city, kana)]
    gun, rest = city.split("郡", 1)
    gun_kana, rest_kana = kana.split("グン", 1)
    return [(gun + "郡", gun_kana + "グン"), (rest, rest_kana)] if rest and rest_kana else [(city, kana)]


def entry(surface: str, kana: str, cost: int = COST) -> str | None:
    suffix = surface[-1]
    if suffix not in SUFFIXES:
        return None
    suffix_kana = SUFFIXES[suffix] or next((k for k in ("チョウ", "マチ", "ムラ", "ソン") if kana.endswith(k)), None)
    if suffix_kana is None or not kana.endswith(suffix_kana) or len(kana) == len(suffix_kana):
        return None
    base = kana[: -len(suffix_kana)]
    spoken = pron(kana)
    return (f"{surface},{CONTEXT_ID},{CONTEXT_ID},{cost},名詞,固有名詞,地域,一般,*,*,"
            f"{surface},{kana},{spoken},{moras(pron(base))}/{moras(spoken)},C1")


def main() -> None:
    municipalities = {row[7]: row[4] for row in csv.reader(open(sys.argv[1], encoding="utf-8"))}
    readings: dict[str, set[str]] = defaultdict(set)
    for city, kana in municipalities.items():
        for surface, part_kana in split_gun(city, kana):
            readings[surface].add(part_kana)
    words = {surface: next(iter(kana)) for surface, kana in readings.items() if len(kana) == 1}
    for city, kana in municipalities.items():
        parts = split_gun(city, kana)
        # A ward (札幌市南区), or a name with a part read differently elsewhere: the whole name is the word.
        if re.search(r"市.+区$", city) or any(len(readings[surface]) > 1 for surface, _ in parts):
            words[city] = kana
    # The lowest cost of an entry the dictionary already has for the name.
    existing: dict[str, int] = {}
    for source in sys.argv[2:]:
        for line in open(source, encoding="utf-8"):
            columns = line.split(",", 4)
            if columns[0] in words:
                existing[columns[0]] = min(existing.get(columns[0], COST), int(columns[3]))
    lines = sorted(filter(None, (entry(s, k, min(COST, existing.get(s, COST) - 1)) for s, k in words.items())))
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("".join(f"{line}\n" for line in lines), encoding="utf-8")
    shared = sum(1 for k in readings.values() if len(k) > 1)
    print(f"{OUT}: {len(lines)} words ({shared} names read differently in different places)")


if __name__ == "__main__":
    main()
