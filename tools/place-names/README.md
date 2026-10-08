# place-names

Japan's municipalities (and their 郡) as dictionary words, from Japan Post's postal code data:
`build.py` writes `../plusfull-golden/extra/place_names.csv`, which `generate.py --export-dict` adds to the
Open JTalk dictionary it builds.

- Data: Japan Post `utf_ken_all.csv` (https://www.post.japanpost.jp/zipcode/), "著作権を主張しません。自由に配布していただいて結構です"
- Effect (2026-10-08, 1,892 municipalities): misread 645 → 2; the app's ordinary sentences read the same.
  The dictionary (sys.dic) grows by about 0.34 MB.

```sh
python3 build.py utf_ken_all.csv path/to/pyopenjtalk-plus/dictionary/naist-jdic.csv path/to/unidic-csj.csv
```

The dictionary CSVs are optional: with them a name the dictionary already has costs a little less than its
entry there, so the municipality's reading wins (日の出町 ヒノデマチ, not ヒノデチョウ).
